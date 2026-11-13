import { Connection, PublicKey } from "@solana/web3.js";
import { SwarmClientError } from "../errors.js";
import type { FetchLike } from "../http.js";
import { shillbotProtocol } from "./decoders/shillbot.js";
import { anchorDiscriminator, bytesEqual } from "./discriminator.js";
import { asV1, checkSchema } from "./schema.js";
import type {
  AasV1Attestation,
  FailureReason,
  ProtocolHandler,
  Verdict,
} from "./types.js";

export const DEFAULT_SCORE_MAX = 1_000_000n;

/** Default maximum VOW attestation HTTP response size (`2 MiB`). */
export const DEFAULT_MAX_VOW_BYTES = 2 * 1024 * 1024;

const DEFAULT_VOW_SOLANA_RPC: Readonly<Record<"mainnet" | "devnet", string>> = {
  mainnet: "https://api.mainnet-beta.solana.com",
  devnet: "https://api.devnet.solana.com",
};

const MAX_VOW_URL_LENGTH = 2048;
const FORBIDDEN_EXACT_HOSTS = new Set([
  "localhost",
  "metadata.google.internal",
  "metadata",
  "instance-data",
]);
const FORBIDDEN_HOST_SUFFIXES = [
  ".internal",
  ".local",
  ".localhost",
  ".run.app",
  ".nip.io",
  ".sslip.io",
  ".xip.io",
] as const;

export interface FetchVowOptions {
  fetch?: FetchLike;
  /** Maximum response body size in bytes; default `2 * 1024 * 1024` (2 MiB). */
  maxBytes?: number;
  /** Explicitly opt in to `http://127.0.0.1`, `http://localhost`, or `http://[::1]` for local testing. */
  allowLocalhostHttp?: boolean;
  operation?: string;
}

export interface VerifyVowOptions extends FetchVowOptions {
  rpcUrl?: string;
  connection?: Connection;
  network?: "mainnet" | "devnet";
  protocol?: ProtocolHandler;
  expectedScoreMax?: bigint | number | string;
}

function isNonGlobalIpv4(o0: number, o1: number, o2: number, _o3: number): boolean {
  if (o0 === 0) return true; // 0.0.0.0/8
  if (o0 === 10) return true; // 10.0.0.0/8 RFC 1918
  if (o0 === 100 && (o1 & 0xc0) === 64) return true; // 100.64.0.0/10 CGNAT
  if (o0 === 127) return true; // 127.0.0.0/8 loopback
  if (o0 === 169 && o1 === 254) return true; // 169.254.0.0/16 link-local / cloud metadata
  if (o0 === 172 && o1 >= 16 && o1 <= 31) return true; // 172.16.0.0/12 RFC 1918
  if (o0 === 192 && o1 === 0 && (o2 === 0 || o2 === 2)) return true; // 192.0.0.0/24 & 192.0.2.0/24
  if (o0 === 192 && o1 === 88 && o2 === 99) return true; // 192.88.99.0/24 6to4 relay
  if (o0 === 192 && o1 === 168) return true; // 192.168.0.0/16 RFC 1918
  if (o0 === 198 && (o1 === 18 || o1 === 19)) return true; // 198.18.0.0/15 benchmarking
  if (o0 === 198 && o1 === 51 && o2 === 100) return true; // 198.51.100.0/24 TEST-NET-2
  if (o0 === 203 && o1 === 0 && o2 === 113) return true; // 203.0.113.0/24 TEST-NET-3
  if (o0 >= 224) return true; // 224.0.0.0/4 multicast + 240.0.0.0/4 reserved/broadcast
  return false;
}

function parseIpv4Octets(host: string): [number, number, number, number] | null {
  const parts = host.split(".");
  if (parts.length !== 4) return null;
  const octets: number[] = [];
  for (const part of parts) {
    if (!/^\d{1,3}$/.test(part)) return null;
    if (part.length > 1 && part.startsWith("0")) return null;
    const n = Number(part);
    if (!Number.isInteger(n) || n < 0 || n > 255) return null;
    octets.push(n);
  }
  return [octets[0], octets[1], octets[2], octets[3]];
}

function parseIpv6Segments(bracketedHost: string): Uint16Array | null {
  if (!bracketedHost.startsWith("[") || !bracketedHost.endsWith("]")) return null;
  const inner = bracketedHost.slice(1, -1);
  if (!inner || inner.includes("%")) return null;

  const doubleColonParts = inner.split("::");
  if (doubleColonParts.length > 2) return null;

  const expandTokens = (chunk: string): number[] | null => {
    if (!chunk) return [];
    const rawTokens = chunk.split(":");
    const out: number[] = [];
    for (let i = 0; i < rawTokens.length; i++) {
      const tok = rawTokens[i];
      if (tok.includes(".")) {
        if (i !== rawTokens.length - 1) return null;
        const v4 = parseIpv4Octets(tok);
        if (!v4) return null;
        out.push((v4[0] << 8) | v4[1], (v4[2] << 8) | v4[3]);
      } else {
        if (!/^[0-9a-fA-F]{1,4}$/.test(tok)) return null;
        out.push(parseInt(tok, 16));
      }
    }
    return out;
  };

  if (doubleColonParts.length === 1) {
    const segs = expandTokens(doubleColonParts[0]);
    if (!segs || segs.length !== 8) return null;
    return Uint16Array.from(segs);
  }

  const left = expandTokens(doubleColonParts[0]);
  const right = expandTokens(doubleColonParts[1]);
  if (!left || !right || left.length + right.length >= 8) return null;
  const zeroFill = new Array<number>(8 - left.length - right.length).fill(0);
  return Uint16Array.from([...left, ...zeroFill, ...right]);
}

function isNonGlobalIpv6(seg: Uint16Array): boolean {
  const allZeroPrefix6 =
    seg[0] === 0 && seg[1] === 0 && seg[2] === 0 && seg[3] === 0 && seg[4] === 0 && seg[5] === 0;
  // :: (unspecified), ::1 (loopback), and ::a.b.c.d (IPv4-compatible)
  if (allZeroPrefix6) return true;
  // ::ffff:0:0/96 (IPv4-mapped IPv6 literals — rejected outright)
  if (seg[0] === 0 && seg[1] === 0 && seg[2] === 0 && seg[3] === 0 && seg[4] === 0 && seg[5] === 0xffff) {
    return true;
  }
  // ::ffff:0:0:0/96 (IPv4-translated)
  if (seg[0] === 0 && seg[1] === 0 && seg[2] === 0 && seg[3] === 0 && seg[4] === 0xffff && seg[5] === 0) {
    return true;
  }
  // 64:ff9b::/96 & 64:ff9b:1::/48 (NAT64)
  if (seg[0] === 0x0064 && seg[1] === 0xff9b) return true;
  // 2002::/16 (6to4)
  if (seg[0] === 0x2002) return true;
  // 2001:0000::/23 (Teredo / IETF protocol) & 2001:db8::/32 (documentation)
  if (seg[0] === 0x2001 && (seg[1] < 0x0200 || seg[1] === 0x0db8)) return true;
  // fc00::/7 (Unique Local)
  if ((seg[0] & 0xfe00) === 0xfc00) return true;
  // fe80::/10 (Link-Local) & fec0::/10 (Site-Local)
  if ((seg[0] & 0xffc0) === 0xfe80 || (seg[0] & 0xffc0) === 0xfec0) return true;
  // ff00::/8 (Multicast)
  if ((seg[0] & 0xff00) === 0xff00) return true;
  return false;
}

/**
 * Validate a VOW attestation URL against SSRF, scheme downgrade, trailing-dot FQDN
 * bypasses, internal/metadata suffixes, and private/loopback/link-local IP literals.
 */
export function validateVowUrl(
  rawUrl: string,
  opts?: { allowLocalhostHttp?: boolean; operation?: string },
): URL {
  const operation = opts?.operation ?? "tasks.verifyVow";
  const trimmed = rawUrl.trim();
  if (!trimmed || trimmed.length > MAX_VOW_URL_LENGTH) {
    throw new SwarmClientError({
      code: "INVALID_ARGUMENT",
      operation,
      message: "VOW URL is empty or exceeds maximum length (2048 chars)",
    });
  }

  let parsed: URL;
  try {
    parsed = new URL(trimmed);
  } catch (cause) {
    throw new SwarmClientError({
      code: "INVALID_ARGUMENT",
      operation,
      message: `Invalid VOW URL: ${trimmed}`,
      cause,
    });
  }

  if (parsed.username || parsed.password) {
    throw new SwarmClientError({
      code: "INVALID_ARGUMENT",
      operation,
      message: "VOW URL must not include embedded credentials",
    });
  }

  const host = parsed.hostname.toLowerCase();
  if (!host || host.endsWith(".")) {
    throw new SwarmClientError({
      code: "INVALID_ARGUMENT",
      operation,
      message: `VOW URL hostname "${parsed.hostname}" is forbidden (trailing-dot FQDNs are rejected)`,
    });
  }

  const isLoopbackHost = host === "127.0.0.1" || host === "localhost" || host === "[::1]";
  if (parsed.protocol === "http:") {
    if (opts?.allowLocalhostHttp === true && isLoopbackHost) {
      return parsed;
    }
    throw new SwarmClientError({
      code: "INVALID_ARGUMENT",
      operation,
      message: "VOW URL must use https: (http: is only allowed on loopback with allowLocalhostHttp: true)",
    });
  }

  if (parsed.protocol !== "https:") {
    throw new SwarmClientError({
      code: "INVALID_ARGUMENT",
      operation,
      message: `Unsupported VOW URL scheme "${parsed.protocol}" (expected https:)`,
    });
  }

  if (parsed.port !== "" && parsed.port !== "443") {
    throw new SwarmClientError({
      code: "INVALID_ARGUMENT",
      operation,
      message: `Non-standard HTTPS port "${parsed.port}" is forbidden for VOW URLs`,
    });
  }

  if (
    FORBIDDEN_EXACT_HOSTS.has(host) ||
    FORBIDDEN_HOST_SUFFIXES.some((suffix) => host.endsWith(suffix))
  ) {
    throw new SwarmClientError({
      code: "INVALID_ARGUMENT",
      operation,
      message: `Forbidden internal or metadata hostname "${host}" in VOW URL`,
    });
  }

  if (host.startsWith("[")) {
    const ipv6 = parseIpv6Segments(host);
    if (!ipv6 || isNonGlobalIpv6(ipv6)) {
      throw new SwarmClientError({
        code: "INVALID_ARGUMENT",
        operation,
        message: `Forbidden non-global or malformed IPv6 literal "${host}" in VOW URL`,
      });
    }
    return parsed;
  }

  const ipv4 = parseIpv4Octets(host);
  if (ipv4) {
    if (isNonGlobalIpv4(ipv4[0], ipv4[1], ipv4[2], ipv4[3])) {
      throw new SwarmClientError({
        code: "INVALID_ARGUMENT",
        operation,
        message: `Forbidden private, loopback, or link-local IPv4 literal "${host}" in VOW URL`,
      });
    }
    return parsed;
  }

  if (/^[\d.]+$/.test(host) || !host.includes(".")) {
    throw new SwarmClientError({
      code: "INVALID_ARGUMENT",
      operation,
      message: `Invalid or single-label hostname "${host}" in VOW URL`,
    });
  }

  return parsed;
}

/**
 * Safely fetch and JSON-decode a VOW v1 attestation document with `redirect: "error"`
 * and a strict streaming byte cap (`DEFAULT_MAX_VOW_BYTES` = 2 MiB).
 */
export async function fetchVowAttestationJson(
  rawUrl: string,
  opts?: FetchVowOptions,
): Promise<unknown> {
  const operation = opts?.operation ?? "tasks.verifyVow";
  const maxBytes = opts?.maxBytes ?? DEFAULT_MAX_VOW_BYTES;
  if (!Number.isInteger(maxBytes) || maxBytes <= 0) {
    throw new SwarmClientError({
      code: "INVALID_ARGUMENT",
      operation,
      message: `maxBytes must be a positive integer, got ${String(maxBytes)}`,
    });
  }

  const validatedUrl = validateVowUrl(rawUrl, {
    allowLocalhostHttp: opts?.allowLocalhostHttp,
    operation,
  });

  const rawFetch = opts?.fetch ?? globalThis.fetch;
  if (!rawFetch) {
    throw new SwarmClientError({
      code: "INVALID_ARGUMENT",
      operation,
      message: "A fetch implementation is required to retrieve VOW URLs",
    });
  }
  const fetchImpl: FetchLike = opts?.fetch
    ? (input, init) => opts.fetch!(input, init)
    : rawFetch.bind(globalThis);

  let resp: Response;
  try {
    resp = await fetchImpl(validatedUrl.toString(), {
      method: "GET",
      redirect: "error",
      headers: { Accept: "application/json" },
    });
  } catch (cause) {
    if (cause instanceof SwarmClientError) throw cause;
    throw new SwarmClientError({
      code: "NETWORK_ERROR",
      operation,
      message: `Failed to fetch VOW attestation (redirects are forbidden): ${
        cause instanceof Error ? cause.message : String(cause)
      }`,
      cause,
    });
  }

  if (
    (resp.status >= 300 && resp.status < 400) ||
    resp.type === "opaqueredirect" ||
    resp.redirected === true
  ) {
    await resp.body?.cancel?.().catch(() => undefined);
    throw new SwarmClientError({
      code: "INVALID_RESPONSE",
      operation,
      status: resp.status,
      message: "HTTP redirects are forbidden when fetching VOW attestations",
    });
  }

  if (!resp.ok) {
    await resp.body?.cancel?.().catch(() => undefined);
    throw new SwarmClientError({
      code: "HTTP_ERROR",
      operation,
      status: resp.status,
      message: `Failed to fetch VOW attestation: HTTP ${resp.status}`,
    });
  }

  const contentLengthHeader = resp.headers?.get?.("content-length") ?? null;
  if (contentLengthHeader !== null && contentLengthHeader.trim() !== "") {
    const declaredLength = Number(contentLengthHeader.trim());
    if (!Number.isFinite(declaredLength) || declaredLength < 0) {
      await resp.body?.cancel?.().catch(() => undefined);
      throw new SwarmClientError({
        code: "INVALID_RESPONSE",
        operation,
        message: `Malformed Content-Length header on VOW response: ${contentLengthHeader}`,
      });
    }
    if (declaredLength > maxBytes) {
      await resp.body?.cancel?.().catch(() => undefined);
      throw new SwarmClientError({
        code: "INVALID_RESPONSE",
        operation,
        message: `VOW attestation Content-Length (${declaredLength} bytes) exceeds maxBytes (${maxBytes})`,
      });
    }
  }

  let utf8Text: string;
  if (resp.body && typeof resp.body.getReader === "function") {
    const reader = resp.body.getReader();
    const chunks: Uint8Array[] = [];
    let totalBytes = 0;
    try {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        if (value) {
          totalBytes += value.byteLength;
          if (totalBytes > maxBytes) {
            await reader.cancel().catch(() => undefined);
            throw new SwarmClientError({
              code: "INVALID_RESPONSE",
              operation,
              message: `VOW attestation response body exceeded maxBytes (${maxBytes} bytes)`,
            });
          }
          chunks.push(value);
        }
      }
    } finally {
      reader.releaseLock?.();
    }
    const combined = new Uint8Array(totalBytes);
    let offset = 0;
    for (const chunk of chunks) {
      combined.set(chunk, offset);
      offset += chunk.byteLength;
    }
    try {
      utf8Text = new TextDecoder("utf-8", { fatal: true }).decode(combined);
    } catch (cause) {
      throw new SwarmClientError({
        code: "INVALID_RESPONSE",
        operation,
        message: "VOW attestation response is not valid UTF-8",
        cause,
      });
    }
  } else {
    utf8Text = await resp.text();
    const encodedLen = new TextEncoder().encode(utf8Text).byteLength;
    if (encodedLen > maxBytes) {
      throw new SwarmClientError({
        code: "INVALID_RESPONSE",
        operation,
        message: `VOW attestation response body exceeded maxBytes (${maxBytes} bytes)`,
      });
    }
  }

  try {
    return JSON.parse(utf8Text);
  } catch (cause) {
    throw new SwarmClientError({
      code: "INVALID_RESPONSE",
      operation,
      message: "VOW attestation response is not valid JSON",
      cause,
    });
  }
}

/**
 * High-level VOW v1 verifier accepting an HTTPS URL, JSON string, or parsed object.
 * Enforces SSRF host/IP validation, `redirect: "error"`, and a 2 MiB streaming body cap.
 */
export async function verifyVow(
  urlOrJson: string | Record<string, unknown>,
  opts?: VerifyVowOptions,
): Promise<Verdict> {
  const operation = opts?.operation ?? "tasks.verifyVow";
  let attestation: unknown = urlOrJson;
  if (typeof urlOrJson === "string") {
    const trimmed = urlOrJson.trim();
    if (trimmed.startsWith("{") || trimmed.startsWith("[")) {
      try {
        attestation = JSON.parse(trimmed);
      } catch (cause) {
        throw new SwarmClientError({
          code: "INVALID_ARGUMENT",
          operation,
          message: "Expected valid VOW URL, JSON string, or object",
          cause,
        });
      }
    } else if (/^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(trimmed)) {
      attestation = await fetchVowAttestationJson(trimmed, opts);
    } else {
      try {
        attestation = JSON.parse(trimmed);
      } catch (cause) {
        throw new SwarmClientError({
          code: "INVALID_ARGUMENT",
          operation,
          message: "Expected VOW URL, JSON string, or object",
          cause,
        });
      }
    }
  }

  const defaultNet = opts?.network ?? "mainnet";
  const net: "mainnet" | "devnet" =
    attestation &&
    typeof attestation === "object" &&
    (attestation as { network?: unknown }).network === "devnet"
      ? "devnet"
      : defaultNet;
  const conn =
    opts?.connection ??
    new Connection(opts?.rpcUrl ?? DEFAULT_VOW_SOLANA_RPC[net], "confirmed");
  return verifyV1(
    attestation,
    opts?.protocol ?? shillbotProtocol,
    conn,
    opts?.expectedScoreMax,
  );
}

/**
 * Step-by-step verifier per `docs/specs/vow-v1.md` §4.
 *
 * Steps split:
 *   - `verifyV1Schema` — pure (steps 1, 6).
 *   - `verifyV1OnChain` — touches RPC (steps 2-5, 7).
 *   - `verifyV1` (this) — full 7-step pipeline.
 *
 * Splitting lets unit tests cover the pure parts without an RPC.
 */
export async function verifyV1(
  attestation: unknown,
  protocol: ProtocolHandler,
  rpc: Connection,
  expectedScoreMax?: bigint | number | string
): Promise<Verdict> {
  const checkedAt = nowRfc3339();
  const rpcEndpoint = rpc.rpcEndpoint;

  const schemaFail = verifyV1Schema(attestation);
  if (schemaFail) {
    return {
      valid: false,
      attestation: attestation as AasV1Attestation,
      checked_at: checkedAt,
      rpc_endpoint: rpcEndpoint,
      failure_reason: schemaFail,
    };
  }

  const a = asV1(attestation);
  const onChainFail = await verifyV1OnChain(a, protocol, rpc, expectedScoreMax);
  if (onChainFail) {
    return {
      valid: false,
      attestation: a,
      checked_at: checkedAt,
      rpc_endpoint: rpcEndpoint,
      failure_reason: onChainFail,
    };
  }

  return {
    valid: true,
    attestation: a,
    checked_at: checkedAt,
    rpc_endpoint: rpcEndpoint,
  };
}

/**
 * Pure step 1 + step 6 of the protocol. Returns null on success or
 * the first failure reason.
 */
export function verifyV1Schema(attestation: unknown): FailureReason | null {
  const schemaFail = checkSchema(attestation);
  if (schemaFail) return schemaFail;

  const a = asV1(attestation);
  // Step 6 — domain bound. Both are decimal-string u64 (passed step 1).
  if (BigInt(a.composite_score) > BigInt(a.score_max)) {
    return "score_above_max";
  }
  return null;
}

/**
 * Steps 2-5 + 7 — touches RPC. Returns null on success or the first
 * failure reason.
 */
export async function verifyV1OnChain(
  a: AasV1Attestation,
  protocol: ProtocolHandler,
  rpc: Connection,
  expectedScoreMax?: bigint | number | string
): Promise<FailureReason | null> {
  const expectedMax = BigInt(
    expectedScoreMax ?? protocol.scoreMax ?? DEFAULT_SCORE_MAX
  );
  if (BigInt(a.score_max) !== expectedMax) {
    return "field_mismatch:score_max";
  }

  // Step 2 — fetch the on-chain account.
  const accountInfo = await rpc.getAccountInfo(new PublicKey(a.account));
  if (accountInfo === null) {
    return "account_closed";
  }
  const data = new Uint8Array(accountInfo.data);

  // Step 3 — owner check.
  if (accountInfo.owner.toBase58() !== a.program_id) {
    return "owner_mismatch";
  }
  if (protocol.programId !== undefined && accountInfo.owner.toBase58() !== protocol.programId) {
    return "owner_mismatch";
  }

  // Step 4 — Anchor discriminator check.
  if (data.length < 8) {
    return "discriminator_mismatch";
  }
  const expectedDisc = anchorDiscriminator(a.account_kind);
  if (!bytesEqual(expectedDisc, data.slice(0, 8))) {
    return "discriminator_mismatch";
  }

  // Step 5 — field equality. Decoder strips the 8-byte discriminator.
  // The decoders THROW on a short/malformed body (shillbot rejects < 286 bytes,
  // extension likewise). Letting that escape turned "this account does not
  // verify" into an exception the caller had to catch, so a truncated or
  // wrong-kind account crashed the verifier instead of returning a verdict.
  // A body we cannot decode IS a schema failure — report it as one.
  let decoded: ReturnType<ProtocolHandler["decode"]>;
  try {
    decoded = protocol.decode(data.slice(8), a.account_kind);
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    return `schema_invalid:${msg}`;
  }

  if (decoded.task_id.toString() !== a.task_id) {
    return "field_mismatch:task_id";
  }
  if (decoded.client !== a.client) {
    return "field_mismatch:client";
  }
  if (decoded.agent !== a.agent) {
    return "field_mismatch:agent";
  }
  if (decoded.platform !== undefined && decoded.platform !== a.platform) {
    return "field_mismatch:platform";
  }
  if (decoded.composite_score.toString() !== a.composite_score) {
    return "field_mismatch:composite_score";
  }
  if (decoded.verification_hash !== a.verification_hash) {
    return "field_mismatch:verification_hash";
  }
  if (decoded.content_hash !== a.content_hash) {
    return "field_mismatch:content_hash";
  }
  if (decoded.content_id_hash !== a.content_id_hash) {
    return "field_mismatch:content_id_hash";
  }

  // verified_at: chain stores i64 unix seconds; wire is RFC 3339 with
  // no fractional seconds. Convert and compare with ±1 second
  // tolerance (per spec §4 step 5).
  const wireSeconds = Math.floor(Date.parse(a.verified_at) / 1000);
  const chainSeconds = Number(decoded.verified_at);
  if (Math.abs(wireSeconds - chainSeconds) > 1) {
    return "field_mismatch:verified_at";
  }

  // Step 5 (state) — map decoded u8 to the protocol's published name.
  const resolved = protocol.resolveState(decoded.state, a.account_kind);
  if (resolved === null || resolved !== a.state) {
    return "field_mismatch:state";
  }

  // Step 7 — state validity. Spec §4 step 7 says protocols publish
  // which on-chain states are "valid for attestation"; the verifier
  // delegates to the protocol handler's `validStates` to avoid
  // hardcoding any one protocol's policy. Shillbot returns
  // `["verified"]`; other protocols can return larger sets without
  // forking this verifier.
  const valid = protocol.validStates(a.account_kind);
  if (!valid.includes(a.state)) {
    return "state_invalid";
  }

  return null;
}

function nowRfc3339(): string {
  return new Date().toISOString().replace(/\.\d+Z$/, "Z");
}
