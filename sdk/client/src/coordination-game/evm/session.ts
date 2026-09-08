/**
 * Ephemeral EVM session key — the Base-game analog of the Solana session
 * keypair (`session.ts`). One main-wallet popup funds the session address with
 * stake + gas; the session key then signs createGame/joinGame/commitGuess/
 * revealGuess locally.
 *
 * Under the shipped v3 WALLET-AS-PLAYER design the session key is a gas-only
 * DELEGATE, not the player: the wallet is `game.player1/2` on-chain, and the
 * payout is credited to the wallet and realized with `withdrawFor`. There is
 * therefore NO sweep, and no `evm-sweep.ts` — that file never existed under v3.
 *
 * The private key lives in sessionStorage (cleared on tab close) bound to the
 * main wallet, with a 24h TTL — same guards as the Solana session. Only gas
 * dust ever sits on the session address.
 */

import {
  bytesToHex,
  createPublicClient,
  createWalletClient,
  fallback,
  hexToBytes,
  http,
  type Chain,
  type Hex,
  type PublicClient,
  type WalletClient,
} from "viem";
import { generatePrivateKey, privateKeyToAccount } from "viem/accounts";

const SESSION_KEY = "coordination-evm-session";
const SESSION_TTL_MS = 24 * 60 * 60 * 1000; // 24 hours

export interface SessionStorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

interface StoredEvmSession {
  privateKey: Hex;
  createdAt: number;
  wallet: string; // 0x address of the main wallet that created this session
}

const PRIVATE_KEY_RE = /^0x[0-9a-fA-F]{64}$/;

/** Generate a fresh ephemeral secp256k1 private key for session signing. */
export function generateEvmSessionKey(): Hex {
  return generatePrivateKey();
}

/**
 * Read the session key from sessionStorage. Returns null if no session
 * exists, the session has expired (24h TTL), or it belongs to a different
 * main wallet. Any invalid stored value is removed.
 */
export function getStoredEvmSession(
  storage: SessionStorageLike,
  walletAddress?: string,
  now: () => number = Date.now,
): Hex | null {
  const raw = storage.getItem(SESSION_KEY);
  if (raw === null) return null;

  let stored: StoredEvmSession;
  try {
    stored = JSON.parse(raw) as StoredEvmSession;
  } catch {
    storage.removeItem(SESSION_KEY);
    return null;
  }

  if (
    typeof stored.privateKey !== "string" ||
    !PRIVATE_KEY_RE.test(stored.privateKey) ||
    typeof stored.createdAt !== "number" ||
    typeof stored.wallet !== "string"
  ) {
    storage.removeItem(SESSION_KEY);
    return null;
  }

  if (
    walletAddress &&
    stored.wallet.toLowerCase() !== walletAddress.toLowerCase()
  ) {
    storage.removeItem(SESSION_KEY);
    return null;
  }

  if (now() - stored.createdAt > SESSION_TTL_MS) {
    storage.removeItem(SESSION_KEY);
    return null;
  }

  return stored.privateKey;
}

/** Persist the session key in sessionStorage, bound to the main wallet. */
export function storeEvmSession(
  storage: SessionStorageLike,
  privateKey: Hex,
  walletAddress: string,
  now: () => number = Date.now,
): void {
  const stored: StoredEvmSession = {
    privateKey,
    createdAt: now(),
    wallet: walletAddress,
  };
  storage.setItem(SESSION_KEY, JSON.stringify(stored));
}

/** Remove the session key from sessionStorage. */
export function clearEvmSession(storage: SessionStorageLike): void {
  storage.removeItem(SESSION_KEY);
}

// --- Committed-guess preimage persistence (the EVM analog of the Solana
//     pendingR store). The preimage R is generated at commit and needed again at
//     reveal; keeping it only in a React ref means a page reload mid-game strands
//     the reveal. Persist it (keyed by gameId so a stale one from a prior game is
//     never reused) in sessionStorage — same tab lifetime as the session key,
//     and it never leaves the browser, so the commit-reveal anonymity barrier
//     holds. ---

const PENDING_GUESS_KEY = "coordination-evm-pending-guess";

/** Persist the committed guess preimage for `gameId` so a reload can still reveal. */
export function persistEvmGuess(
  storage: SessionStorageLike,
  gameId: string,
  r: Uint8Array,
): void {
  try {
    storage.setItem(
      PENDING_GUESS_KEY,
      JSON.stringify({ gameId, r: bytesToHex(r) }),
    );
  } catch (e) {
    console.warn("[evm-session] guess persist failed (in-memory only)", e);
  }
}

/** The persisted preimage for `gameId`, or null (missing / different game /
 *  unreadable). A mismatched gameId guards against revealing a stale guess. */
export function loadEvmGuess(
  storage: SessionStorageLike,
  gameId: string,
): Uint8Array | null {
  try {
    const raw = storage.getItem(PENDING_GUESS_KEY);
    if (raw === null) return null;
    const stored = JSON.parse(raw) as { gameId?: string; r?: string };
    if (stored.gameId !== gameId || typeof stored.r !== "string") return null;
    return hexToBytes(stored.r as Hex);
  } catch (e) {
    console.warn("[evm-session] guess load failed", e);
    return null;
  }
}

/** Drop the persisted guess once the game is done. */
export function clearEvmGuess(storage: SessionStorageLike): void {
  try {
    storage.removeItem(PENDING_GUESS_KEY);
  } catch (e) {
    console.debug("[evm-session] guess clear failed", e);
  }
}

/** 0x address of a session private key. */
export function evmSessionAddress(privateKey: Hex): Hex {
  return privateKeyToAccount(privateKey).address;
}

/**
 * A viem WalletClient signing with the session key against the SELECTED chain's
 * failover RPCs. Structurally a `CallSender`, so it drops into the existing
 * `sendUnsignedCall` / `sendContractCall` exactly where the wagmi wallet
 * client used to go — the EVM analog of Solana's `getSessionProgram`. The chain
 * is passed in (from the picker's selectedChain) so game txs land on the chain
 * the player actually chose, not a hardcoded default.
 */
export function evmSessionClient(privateKey: Hex, chain: Chain, rpcUrls: string[]): WalletClient {
  // No viem nonceManager: its counter advances optimistically and never rolls
  // back, so a DROPPED Base Sepolia tx leaves a nonce GAP that stalls every
  // later send ("creator's createGame timed out"). Nonce consistency + drop
  // recovery are handled at the send site instead — see sendConfirmedTx: it
  // reads the nonce from a single PINNED RPC (consistent, no stale-low race) and
  // resubmits with the freed nonce if a tx drops.
  const account = privateKeyToAccount(privateKey);
  // Name the SIGNER. Game failures surface as "the total cost ... exceeds the
  // balance of the account" labelled with the PLAYER's address, which is not the
  // account that pays — the session key is. Without this line the two are
  // indistinguishable in a log, and a session funded at one address while
  // signing from another looks identical to a session that was never funded.
  console.info("[evm-session] signing client", {
    sessionAddr: account.address,
    chainId: chain.id,
    rpcCount: rpcUrls.length,
  });
  return createWalletClient({
    account,
    chain,
    transport: fallback(rpcUrls.map((u) => http(u))),
  });
}

/** Read-only client for the selected chain (balances, receipts). */
export function evmReadClient(chain: Chain, rpcUrls: string[]): PublicClient {
  return createPublicClient({
    chain,
    transport: fallback(rpcUrls.map((u) => http(u))),
  });
}

/**
 * Sign a message with the session key locally (EIP-191 personal_sign) — used
 * for the silent auth handshake and the funding-tx binding proof. No popup.
 */
export function signWithEvmSession(
  privateKey: Hex,
  message: string,
): Promise<Hex> {
  return privateKeyToAccount(privateKey).signMessage({ message });
}
