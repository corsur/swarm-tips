import { Buffer } from "buffer";
import bs58 from "bs58";
import { SwarmClientError } from "../errors.js";
import { HttpClient, type FetchLike } from "../http.js";
import type { SwarmSigner } from "../signer/types.js";

export type TokenProvider = string | (() => Promise<string | undefined> | string | undefined);

export interface WalletTokenManagerOptions {
  signer?: SwarmSigner;
  token?: TokenProvider;
  gameUrl?: string;
  fetch?: FetchLike;
  now?: () => number;
}

interface JwtClaims {
  exp?: number;
  wallet?: string;
  sub?: string;
}

function parseJwtClaims(token: string): JwtClaims {
  const parts = token.split(".");
  if (parts.length < 2) return {};
  try {
    const b64 = parts[1].replace(/-/g, "+").replace(/_/g, "/");
    const padded = b64 + "=".repeat((4 - (b64.length % 4)) % 4);
    const parsed = JSON.parse(Buffer.from(padded, "base64").toString("utf8"));
    return parsed && typeof parsed === "object" ? (parsed as JwtClaims) : {};
  } catch {
    return {};
  }
}

const DEFAULT_OPAQUE_TTL_MS = 23 * 60 * 60 * 1_000;

/** Canonical server-issued auth challenge nonce format (`hex::encode([u8; 32])`, P2-25). */
export const AUTH_CHALLENGE_NONCE_RE = /^[0-9a-f]{64}$/;

/**
 * Validate a server-supplied auth challenge nonce before signing it.
 * Rejects arbitrary strings (such as `swarm-attr-v1:...` referral messages or
 * crafted off-chain authorizations) so a compromised API cannot turn wallet
 * login into a blind-signing oracle.
 */
export function validateAuthChallengeNonce(nonce: unknown, operation: string): string {
  if (typeof nonce !== "string" || nonce.length === 0) {
    throw new SwarmClientError({
      code: "INVALID_RESPONSE",
      operation,
      message: "Auth challenge returned no nonce",
    });
  }
  if (!AUTH_CHALLENGE_NONCE_RE.test(nonce)) {
    throw new SwarmClientError({
      code: "INVALID_RESPONSE",
      operation,
      message:
        "Auth challenge nonce must be 64 lowercase hex characters; refusing to sign arbitrary payload",
    });
  }
  return nonce;
}

/**
 * Non-custodial JWT manager backed by `game-api` wallet challenges:
 * - Solana: `POST /auth/challenge` + `POST /auth/verify` (Ed25519 detached bs58)
 * - EVM: `POST /auth/evm/challenge` + `POST /auth/evm/verify` (EIP-191 personal_sign)
 *
 * Caches minted tokens until `(exp - 60) * 1000`, deduplicates concurrent refresh
 * calls, and exposes `invalidate()` for single-retry 401 recovery.
 */
export class WalletTokenManager {
  readonly signer?: SwarmSigner;
  private readonly staticToken?: string;
  private readonly customGetToken?: () => Promise<string | undefined> | string | undefined;
  private readonly http: HttpClient;
  private readonly now: () => number;
  private cachedToken: string | null = null;
  private expiresAtMs = 0;
  private inFlight: Promise<string> | null = null;

  constructor(options: WalletTokenManagerOptions = {}) {
    this.signer = options.signer;
    if (typeof options.token === "string") {
      this.staticToken = options.token;
    } else if (typeof options.token === "function") {
      this.customGetToken = options.token;
    }
    this.http = new HttpClient({
      baseUrl: options.gameUrl ?? "https://api.coordination.game",
      fetch: options.fetch,
    });
    this.now = options.now ?? (() => Date.now());
  }

  get walletAddress(): string | undefined {
    if (this.signer) return this.signer.address;
    const candidate = this.staticToken ?? this.cachedToken;
    if (!candidate) return undefined;
    const claims = parseJwtClaims(candidate);
    return claims.wallet ?? claims.sub;
  }

  invalidate(): void {
    this.cachedToken = null;
    this.expiresAtMs = 0;
  }

  async getToken(): Promise<string | undefined> {
    if (this.staticToken !== undefined) {
      return this.staticToken;
    }
    if (this.customGetToken) {
      return this.customGetToken();
    }
    if (!this.signer) {
      return undefined;
    }
    if (this.cachedToken && this.now() < this.expiresAtMs) {
      return this.cachedToken;
    }
    return this.authenticate();
  }

  async authenticate(): Promise<string> {
    if (this.staticToken !== undefined) {
      return this.staticToken;
    }
    if (this.customGetToken) {
      const resolved = await this.customGetToken();
      if (!resolved) {
        throw new SwarmClientError({
          code: "AUTH_REQUIRED",
          operation: "auth.authenticate",
          message: "Token callback returned no token",
        });
      }
      return resolved;
    }
    if (!this.signer) {
      throw new SwarmClientError({
        code: "AUTH_REQUIRED",
        operation: "auth.authenticate",
        message: "A SwarmSigner or token is required to authenticate",
      });
    }
    if (this.inFlight) {
      return this.inFlight;
    }
    const signer = this.signer;
    this.inFlight = (async () => {
      try {
        let token: string;
        if (signer.chain === "solana") {
          const challenge = await this.http.post<{ nonce: string }>("/auth/challenge", {
            wallet: signer.address,
          });
          const nonce = validateAuthChallengeNonce(challenge?.nonce, "auth.challenge");
          const sigBytes = await signer.signMessage(new TextEncoder().encode(nonce));
          const verified = await this.http.post<{ token: string }>("/auth/verify", {
            wallet: signer.address,
            nonce,
            signature: bs58.encode(sigBytes),
          });
          if (!verified?.token) {
            throw new SwarmClientError({
              code: "INVALID_RESPONSE",
              operation: "auth.verify",
              message: "Auth verify returned no token",
            });
          }
          token = verified.token;
        } else {
          const challenge = await this.http.post<{ nonce: string }>("/auth/evm/challenge", {
            wallet: signer.address,
          });
          const nonce = validateAuthChallengeNonce(challenge?.nonce, "auth.evmChallenge");
          const signature = await signer.signMessage(nonce);
          const verified = await this.http.post<{ token: string }>("/auth/evm/verify", {
            wallet: signer.address,
            nonce,
            signature,
          });
          if (!verified?.token) {
            throw new SwarmClientError({
              code: "INVALID_RESPONSE",
              operation: "auth.evmVerify",
              message: "EVM auth verify returned no token",
            });
          }
          token = verified.token;
        }

        const claims = parseJwtClaims(token);
        const nowMs = this.now();
        this.cachedToken = token;
        this.expiresAtMs =
          typeof claims.exp === "number"
            ? (claims.exp - 60) * 1_000
            : nowMs + DEFAULT_OPAQUE_TTL_MS;
        return token;
      } finally {
        this.inFlight = null;
      }
    })();
    return this.inFlight;
  }
}
