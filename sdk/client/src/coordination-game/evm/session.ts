/**
 * Ephemeral EVM session key — the Base-game analog of the Solana session
 * keypair (`session.ts`). One main-wallet popup funds the session address with
 * gas (while stake is escrowed to the player's on-chain ledger); the session key
 * then signs createGame/joinGame/commitGuess/revealGuess locally.
 *
 * Persistence invariant (`P1-5`): active EVM session keys are persisted in
 * `localStorage` first (mirroring to `sessionStorage`, and falling back to
 * `sessionStorage` when `localStorage` is unavailable) and retained until the
 * delegate's on-chain ETH balance is swept to `0`, so closing and reopening a
 * browser tab never orphans funded ETH in an ephemeral EVM session key. Legacy
 * keys stored only in `sessionStorage` are transparently migrated to `localStorage`.
 */

import {
  bytesToHex,
  createPublicClient,
  createWalletClient,
  fallback,
  hexToBytes,
  http,
  type Account,
  type Chain,
  type Hex,
  type PublicClient,
  type WalletClient,
} from "viem";
import { generatePrivateKey, privateKeyToAccount } from "viem/accounts";

export const EVM_SESSION_STORAGE_KEY = "coordination-evm-session";
const SESSION_KEY = EVM_SESSION_STORAGE_KEY;

export interface SessionStorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

export interface EvmSessionStoragePair {
  localStorage?: SessionStorageLike | null;
  sessionStorage?: SessionStorageLike | null;
}

export type EvmSessionStorageInput =
  | SessionStorageLike
  | EvmSessionStoragePair
  | null
  | undefined;

interface StoredEvmSession {
  privateKey: Hex;
  createdAt: number;
  wallet: string; // 0x address of the main wallet that created this session
}

const PRIVATE_KEY_RE = /^0x[0-9a-fA-F]{64}$/;

function isStorageLike(candidate: unknown): candidate is SessionStorageLike {
  return (
    Boolean(candidate) &&
    typeof candidate === "object" &&
    typeof (candidate as SessionStorageLike).getItem === "function" &&
    typeof (candidate as SessionStorageLike).setItem === "function" &&
    typeof (candidate as SessionStorageLike).removeItem === "function"
  );
}

function getSafeGlobalStorage(kind: "localStorage" | "sessionStorage"): SessionStorageLike | null {
  try {
    if (
      typeof (globalThis as Record<string, unknown>).window === "undefined" &&
      typeof process !== "undefined" &&
      Boolean(process.versions?.node) &&
      Object.getOwnPropertyDescriptor(globalThis, kind)?.get &&
      !process.execArgv.some((arg) => arg.startsWith("--localstorage-file="))
    ) {
      return null;
    }
    const candidate = (globalThis as Record<string, unknown>)[kind];
    if (!isStorageLike(candidate)) return null;
    // Probe read access (Safari/Firefox private mode may throw on storage access).
    candidate.getItem("__swarm_evm_session_probe__");
    return candidate;
  } catch {
    return null;
  }
}

function resolveStoragePair(input?: EvmSessionStorageInput): {
  local: SessionStorageLike | null;
  session: SessionStorageLike | null;
} {
  if (input === undefined || input === null) {
    return {
      local: getSafeGlobalStorage("localStorage"),
      session: getSafeGlobalStorage("sessionStorage"),
    };
  }
  if (!isStorageLike(input)) {
    return {
      local: input.localStorage !== undefined ? input.localStorage : getSafeGlobalStorage("localStorage"),
      session:
        input.sessionStorage !== undefined
          ? input.sessionStorage
          : getSafeGlobalStorage("sessionStorage"),
    };
  }
  const globalSession = getSafeGlobalStorage("sessionStorage");
  const globalLocal = getSafeGlobalStorage("localStorage");
  if (globalSession && input === globalSession) {
    return { local: globalLocal, session: input };
  }
  if (globalLocal && input === globalLocal) {
    return { local: input, session: globalSession };
  }
  return { local: input, session: null };
}

function safeGetItem(storage: SessionStorageLike | null, key: string): string | null {
  if (!storage) return null;
  try {
    return storage.getItem(key);
  } catch {
    return null;
  }
}

function safeSetItem(storage: SessionStorageLike | null, key: string, value: string): boolean {
  if (!storage) return false;
  try {
    storage.setItem(key, value);
    return true;
  } catch {
    return false;
  }
}

function safeRemoveItem(storage: SessionStorageLike | null, key: string): void {
  if (!storage) return;
  try {
    storage.removeItem(key);
  } catch {
    // Ignore storage removal errors.
  }
}

function parseStoredEvmSession(raw: string): StoredEvmSession | null {
  let parsed: StoredEvmSession;
  try {
    parsed = JSON.parse(raw) as StoredEvmSession;
  } catch {
    return null;
  }
  if (
    !parsed ||
    typeof parsed !== "object" ||
    typeof parsed.privateKey !== "string" ||
    !PRIVATE_KEY_RE.test(parsed.privateKey) ||
    typeof parsed.createdAt !== "number" ||
    typeof parsed.wallet !== "string"
  ) {
    return null;
  }
  return parsed;
}

/** Generate a fresh ephemeral secp256k1 private key for session signing. */
export function generateEvmSessionKey(): Hex {
  return generatePrivateKey();
}

/**
 * Read the persisted EVM session key from `localStorage` first, falling back to
 * `sessionStorage` and migrating legacy `sessionStorage`-only keys into `localStorage`.
 * Retains valid keys across tab closes and past 24h until explicitly cleared after
 * on-chain ETH balance is swept to `0`.
 */
export function getStoredEvmSession(
  storage?: EvmSessionStorageInput,
  walletAddress?: string,
  _now: () => number = Date.now,
): Hex | null {
  const { local, session } = resolveStoragePair(storage);

  const rawLocal = safeGetItem(local, SESSION_KEY);
  if (rawLocal !== null) {
    const parsedLocal = parseStoredEvmSession(rawLocal);
    if (!parsedLocal) {
      safeRemoveItem(local, SESSION_KEY);
    } else if (
      !walletAddress ||
      parsedLocal.wallet.toLowerCase() === walletAddress.toLowerCase()
    ) {
      if (session && safeGetItem(session, SESSION_KEY) !== rawLocal) {
        safeSetItem(session, SESSION_KEY, rawLocal);
      }
      return parsedLocal.privateKey;
    }
  }

  const rawSession = safeGetItem(session, SESSION_KEY);
  if (rawSession !== null) {
    const parsedSession = parseStoredEvmSession(rawSession);
    if (!parsedSession) {
      safeRemoveItem(session, SESSION_KEY);
      return null;
    }
    if (
      walletAddress &&
      parsedSession.wallet.toLowerCase() !== walletAddress.toLowerCase()
    ) {
      return null;
    }
    // Migrate legacy sessionStorage key into localStorage so tab close cannot orphan ETH.
    if (local && rawLocal === null) {
      safeSetItem(local, SESSION_KEY, rawSession);
    }
    return parsedSession.privateKey;
  }

  return null;
}

/**
 * Ergonomic loader for persisted EVM session keys (`localStorage` primary,
 * `sessionStorage` fallback + legacy migration).
 */
export function loadSessionKey(
  walletAddress?: string,
  storage?: EvmSessionStorageInput,
): Hex | null;
export function loadSessionKey(
  storage: EvmSessionStorageInput,
  walletAddress?: string,
): Hex | null;
export function loadSessionKey(
  walletOrStorage?: string | EvmSessionStorageInput,
  optionsOrWallet?: EvmSessionStorageInput | string,
): Hex | null {
  if (typeof walletOrStorage === "string" || walletOrStorage === undefined) {
    const storage =
      typeof optionsOrWallet === "object" ? optionsOrWallet : undefined;
    return getStoredEvmSession(storage, walletOrStorage);
  }
  const walletAddress =
    typeof optionsOrWallet === "string" ? optionsOrWallet : undefined;
  return getStoredEvmSession(walletOrStorage, walletAddress);
}

/**
 * Persist the EVM session key in `localStorage` first, mirroring to `sessionStorage`
 * (and falling back to `sessionStorage` when `localStorage` is unavailable).
 */
export function storeEvmSession(
  storage: EvmSessionStorageInput,
  privateKey: Hex,
  walletAddress: string,
  now: () => number = Date.now,
): void {
  const { local, session } = resolveStoragePair(storage);
  const serialized = JSON.stringify({
    privateKey,
    createdAt: now(),
    wallet: walletAddress,
  } satisfies StoredEvmSession);
  const wroteLocal = safeSetItem(local, SESSION_KEY, serialized);
  const wroteSession = safeSetItem(session, SESSION_KEY, serialized);
  if (!wroteLocal && !wroteSession) {
    console.warn("[evm-session] failed to persist EVM session key in localStorage/sessionStorage");
  }
}

/** Ergonomic helper to persist an EVM session key in `localStorage` + `sessionStorage`. */
export function saveSessionKey(
  privateKey: Hex,
  walletAddress: string,
  options?: EvmSessionStoragePair & { now?: () => number },
): void {
  storeEvmSession(options, privateKey, walletAddress, options?.now);
}

/** Remove the EVM session key from both `localStorage` and `sessionStorage`. */
export function clearEvmSession(storage?: EvmSessionStorageInput): void {
  const { local, session } = resolveStoragePair(storage);
  safeRemoveItem(local, SESSION_KEY);
  safeRemoveItem(session, SESSION_KEY);
}

/** A plain native-ETH transfer uses 21,000 gas. */
export const SWEEP_GAS_LIMIT = 21_000n;

/**
 * Compute the native ETH amount to sweep from a session key back to the main wallet:
 * `balance - 1.25 * (21_000 * maxFeePerGas)`, clamped at `0n`.
 */
export function sweepValueWei(balance: bigint, maxFeePerGas: bigint): bigint {
  if (balance <= 0n || maxFeePerGas <= 0n) return 0n;
  const cost = SWEEP_GAS_LIMIT * maxFeePerGas;
  const reserve = cost + cost / 4n;
  const value = balance - reserve;
  return value > 0n ? value : 0n;
}

export interface SweepEvmSessionOptions {
  storage?: EvmSessionStorageInput;
  /** When `true`, deduct exact `21_000 * maxFeePerGas` with `maxPriorityFeePerGas = maxFeePerGas` so EIP-1559 drains to `0n`. */
  exact?: boolean;
  readClient?: {
    getBalance: (args: { address: Hex }) => Promise<bigint>;
    estimateFeesPerGas?: () => Promise<{
      maxFeePerGas?: bigint;
      maxPriorityFeePerGas?: bigint;
    }>;
    getGasPrice?: () => Promise<bigint>;
    waitForTransactionReceipt: (args: { hash: Hex }) => Promise<{ status?: string }>;
  };
  walletClient?: {
    account?: Account | Hex;
    sendTransaction: (args: Record<string, unknown>) => Promise<Hex>;
  };
}

/**
 * Sweep leftover ETH from an EVM session key back to `mainWallet`, clearing
 * persisted storage **only** once the session address's on-chain balance reaches `0n`.
 * If the sweep fails or non-zero balance remains on-chain, the key is retained in
 * `localStorage` so funded ETH is never orphaned.
 */
export async function sweepEvmSession(
  sessionKey: Hex,
  mainWallet: Hex,
  chain: Chain,
  rpcUrls: string[],
  options: SweepEvmSessionOptions = {},
): Promise<bigint> {
  const pub = options.readClient ?? evmReadClient(chain, rpcUrls);
  const sessionAddr = evmSessionAddress(sessionKey);
  const balance = await pub.getBalance({ address: sessionAddr });
  if (balance === 0n) {
    clearEvmSession(options.storage);
    return 0n;
  }

  const fees = pub.estimateFeesPerGas
    ? await pub.estimateFeesPerGas()
    : { maxFeePerGas: undefined, maxPriorityFeePerGas: undefined };
  const maxFeePerGas =
    fees.maxFeePerGas ?? (pub.getGasPrice ? await pub.getGasPrice() : 0n);
  const exactCost = SWEEP_GAS_LIMIT * maxFeePerGas;
  const value = options.exact
    ? balance > exactCost && maxFeePerGas > 0n
      ? balance - exactCost
      : 0n
    : sweepValueWei(balance, maxFeePerGas);

  if (value > 0n) {
    const maxPriorityFeePerGas = options.exact
      ? maxFeePerGas
      : (fees.maxPriorityFeePerGas ?? maxFeePerGas);
    const hash = options.walletClient
      ? await options.walletClient.sendTransaction({
          account: options.walletClient.account ?? privateKeyToAccount(sessionKey),
          chain: null,
          to: mainWallet,
          value,
          gas: SWEEP_GAS_LIMIT,
          maxFeePerGas,
          maxPriorityFeePerGas,
        })
      : await evmSessionClient(sessionKey, chain, rpcUrls).sendTransaction({
          account: privateKeyToAccount(sessionKey),
          chain: null,
          to: mainWallet,
          value,
          gas: SWEEP_GAS_LIMIT,
          maxFeePerGas,
          maxPriorityFeePerGas,
        });
    const receipt = await pub.waitForTransactionReceipt({ hash });
    if (receipt?.status && receipt.status !== "success") {
      throw new Error(`EVM session sweep reverted (tx ${hash}); retaining session key`);
    }
  }

  const remaining = await pub.getBalance({ address: sessionAddr });
  if (remaining === 0n) {
    clearEvmSession(options.storage);
  }
  return value;
}

// --- Committed-guess preimage persistence (the EVM analog of the Solana
//     pendingR store). Mirrored to localStorage + sessionStorage so neither a
//     reload nor a tab close mid-game strands a committed reveal. ---

const PENDING_GUESS_KEY = "coordination-evm-pending-guess";

/** Persist the committed guess preimage for `gameId` so a reload/reopen can still reveal. */
export function persistEvmGuess(
  storage: EvmSessionStorageInput,
  gameId: string,
  r: Uint8Array,
): void {
  const { local, session } = resolveStoragePair(storage);
  const serialized = JSON.stringify({ gameId, r: bytesToHex(r) });
  const wroteLocal = safeSetItem(local, PENDING_GUESS_KEY, serialized);
  const wroteSession = safeSetItem(session, PENDING_GUESS_KEY, serialized);
  if (!wroteLocal && !wroteSession) {
    console.warn("[evm-session] guess persist failed (in-memory only)");
  }
}

/** The persisted preimage for `gameId`, or null (missing / different game / unreadable). */
export function loadEvmGuess(
  storage: EvmSessionStorageInput,
  gameId: string,
): Uint8Array | null {
  const { local, session } = resolveStoragePair(storage);
  for (const candidate of [local, session]) {
    const raw = safeGetItem(candidate, PENDING_GUESS_KEY);
    if (raw === null) continue;
    try {
      const stored = JSON.parse(raw) as { gameId?: string; r?: string };
      if (stored.gameId === gameId && typeof stored.r === "string") {
        if (candidate === session && local && safeGetItem(local, PENDING_GUESS_KEY) === null) {
          safeSetItem(local, PENDING_GUESS_KEY, raw);
        }
        return hexToBytes(stored.r as Hex);
      }
    } catch (e) {
      console.warn("[evm-session] guess load failed", e);
    }
  }
  return null;
}

/** Drop the persisted guess once the game is done. */
export function clearEvmGuess(storage?: EvmSessionStorageInput): void {
  const { local, session } = resolveStoragePair(storage);
  safeRemoveItem(local, PENDING_GUESS_KEY);
  safeRemoveItem(session, PENDING_GUESS_KEY);
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
