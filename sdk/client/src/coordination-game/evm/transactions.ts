import {
  createPublicClient,
  encodeFunctionData,
  http,
  type Abi,
  type Chain,
  type Hex,
  type PublicClient,
  type TransactionReceipt,
} from "viem";
import { evmReadClient, evmSessionClient } from "./session.js";
import { COORDINATION_GAME_ABI } from "./abi.js";

/** What sendConfirmedTx needs from a signer — the session wallet client satisfies
 *  it; tests pass a mock. */
export interface TxSender {
  account: { address: Hex };
  sendTransaction: (args: Record<string, unknown>) => Promise<Hex>;
}
/** What sendConfirmedTx needs from a read client PINNED to one RPC (so nonce
 *  reads are monotonic — the fallback set's pending-nonce views disagree). */
export interface PinnedReader {
  getTransactionCount: (args: { address: Hex; blockTag: "pending" }) => Promise<number>;
  getTransactionReceipt: (args: { hash: Hex }) => Promise<TransactionReceipt>;
  estimateFeesPerGas: () => Promise<{
    maxFeePerGas?: bigint;
    maxPriorityFeePerGas?: bigint;
  }>;
}

/** A read client over ALL rpcUrls (viem `fallback`) used ONLY for an authoritative
 *  receipt re-check when the pinned poll lags — the receipt is chain state, so any
 *  RPC that has it is correct (unlike the nonce, which must stay pinned). */
export interface FallbackReader {
  getTransactionReceipt: (args: { hash: Hex }) => Promise<TransactionReceipt>;
}

/** A read client pinned to a SINGLE rpc — never the fallback set, whose rotation
 *  between nodes with disagreeing pending-nonce views is what produces stale-low
 *  "nonce lower than current" reverts. */
export function pinnedGameReader(chain: Chain, rpcUrls: string[]): PublicClient {
  return createPublicClient({ chain, transport: http(rpcUrls[0]) });
}

async function bumpedFees(pinned: PinnedReader, attempt: number) {
  const { maxFeePerGas, maxPriorityFeePerGas } = await pinned.estimateFeesPerGas();
  // +25% per resubmit — viem rejects a same-nonce replacement at equal fees.
  const mul = BigInt(100 + 25 * attempt);
  return {
    maxFeePerGas: maxFeePerGas ? (maxFeePerGas * mul) / 100n : undefined,
    maxPriorityFeePerGas: maxPriorityFeePerGas
      ? (maxPriorityFeePerGas * mul) / 100n
      : undefined,
  };
}

async function pollReceipt(
  pinned: PinnedReader,
  hash: Hex,
  confirmMs: number,
  pollMs: number,
): Promise<TransactionReceipt | null> {
  for (let i = 0; i < Math.ceil(confirmMs / pollMs); i++) {
    try {
      return await pinned.getTransactionReceipt({ hash });
    } catch {
      // not mined yet
    }
    await new Promise((r) => setTimeout(r, pollMs));
  }
  return null;
}

/**
 * Send a game tx (session-key signed) and CONFIRM it, resilient to Base Sepolia's
 * two testnet flakes with NO test-level retry — the real system handling the load:
 *
 *  - INCONSISTENT NONCE: the session wallet's `fallback` transport reads the nonce
 *    from whichever RPC it lands on, and their pending-nonce views disagree → a
 *    stale-LOW read reverts "nonce lower than the current nonce". We read the
 *    nonce ONCE from a single pinned reader and reuse it for every resubmit, so it
 *    is consistent and every resubmit REPLACES the same intent (never a second
 *    game). (viem's nonceManager can't be used: its counter advances optimistically
 *    and never rolls back, so a dropped tx leaves a permanent gap.)
 *  - DROPPED TX: Base Sepolia silently drops txs; a single send then never mines
 *    and the game stalls ("creator's createGame timed out"). If the tx is not
 *    included within `confirmMs`, resubmit at the SAME nonce with bumped fees — a
 *    replacement that fills the freed slot (dropped) or supersedes it (stuck).
 *  - LAGGY PINNED RECEIPT RPC (mainnet): the pinned nonce reader is a single RPC
 *    (`rpcUrls[0]`), and on mainnet that one endpoint rate-limits / lags on
 *    `getTransactionReceipt` — so a tx that MINED (createGame/commit/reveal all
 *    succeeded on-chain) still times out the pinned poll and the game stalls. The
 *    receipt is chain state, not nonce state, so before declaring failure we do
 *    ONE authoritative re-check over `fallbackReader` (all rpcUrls); a success
 *    there means the tx landed and we return it instead of a false failure.
 *
 * Returns the successful receipt; throws on a real revert or after `attempts`.
 */
export async function sendConfirmedTx(
  wallet: TxSender,
  pinned: PinnedReader,
  chain: Chain,
  call: { to: Hex; data: Hex; value: bigint },
  label: string,
  {
    attempts = 4,
    confirmMs = 30_000,
    pollMs = 3_000,
  }: { attempts?: number; confirmMs?: number; pollMs?: number } = {},
  fallbackReader?: FallbackReader,
): Promise<TransactionReceipt> {
  const address = wallet.account.address;
  // ONE nonce for all attempts — reusing it makes every resubmit a replacement of
  // the same intent, never a second on-chain action.
  const nonce = await pinned.getTransactionCount({ address, blockTag: "pending" });
  let lastHash: Hex | undefined;
  let lastErr: unknown;
  for (let i = 0; i < attempts; i++) {
    try {
      lastHash = await wallet.sendTransaction({
        ...call,
        chain,
        nonce,
        ...(i > 0 ? await bumpedFees(pinned, i) : {}),
      });
    } catch (e) {
      // A resubmit can bounce with "nonce too low" (the prior send already mined)
      // or "already known" / "replacement underpriced" (in flight). None is
      // terminal — poll the hash we already have instead of failing.
      lastErr = e;
      if (!lastHash) throw e;
    }
    const receipt = await pollReceipt(pinned, lastHash, confirmMs, pollMs);
    if (receipt) {
      if (receipt.status !== "success") {
        throw new Error(`${label} tx ${lastHash} reverted on-chain`);
      }
      return receipt;
    }
    console.warn(
      `[evm-tx] ${label} not mined in ${confirmMs}ms (attempt ${i + 1}/${attempts}) — resubmitting at nonce ${nonce} with bumped fees`,
      { hash: lastHash },
    );
  }
  // The pinned poll (single rpcUrls[0]) exhausted every attempt — but on mainnet
  // that one RPC lags on receipts, so the tx may have MINED. Do one authoritative
  // re-check over the fallback set before declaring failure.
  if (lastHash && fallbackReader) {
    const receipt = await fallbackReader
      .getTransactionReceipt({ hash: lastHash })
      .catch(() => null);
    if (receipt) {
      if (receipt.status !== "success") {
        throw new Error(`${label} tx ${lastHash} reverted on-chain`);
      }
      console.info("[evm-tx] " + label + " confirmed via fallback receipt read after pinned poll lag", {
        hash: lastHash,
      });
      return receipt;
    }
  }
  throw new Error(
    `${label} did not confirm after ${attempts} attempts (last tx ${lastHash ?? "none"})` +
      (lastErr ? `: ${lastErr instanceof Error ? lastErr.message : String(lastErr)}` : ""),
  );
}

/** Convenience: build the session wallet + pinned reader for a game and confirm a
 *  raw {to,data,value} call. */
export function sendConfirmedGameTx(
  sessionKey: Hex,
  chain: Chain,
  rpcUrls: string[],
  call: { to: Hex; data: Hex; value: bigint },
  label: string,
): Promise<TransactionReceipt> {
  return sendConfirmedTx(
    evmSessionClient(sessionKey, chain, rpcUrls) as unknown as TxSender,
    pinnedGameReader(chain, rpcUrls) as unknown as PinnedReader,
    chain,
    call,
    label,
    undefined,
    // Fallback over ALL rpcUrls — authoritative receipt re-check when the single
    // pinned rpc lags. The nonce stays pinned; only the final receipt is fallback.
    evmReadClient(chain, rpcUrls) as unknown as FallbackReader,
  );
}

/** CoordinationGame.sol Status enum: None=0, Pending=1, Active=2, Committing=3,
 *  Revealing=4, Resolved=5. Any value >= 1 means the game EXISTS on-chain. */
export function gameStatusExists(status: number): boolean {
  return status >= 1;
}

/**
 * Whether the "matched" phase should auto-fire `fund()` (create/join) with no
 * manual button. The stake is ALREADY escrowed at the queue's one popup, so
 * create/join is popup-less — there is nothing for the player to confirm and a
 * "Create game"/"Join game" button is pure friction (mirrors Solana, where P1
 * now creates silently via the session key). Fires exactly once per game:
 *  - only at "matched" (before it, there is no game to fund; after, it is done),
 *  - only once every precondition the tx needs is present (wallet/session ready),
 *  - only if this game_id hasn't already been auto-funded (guards React
 *    re-renders + strict-mode double-invoke; a NEW match re-arms it).
 * Pure so the gating is unit-testable without a wallet or chain.
 */
export function shouldAutoFund(args: {
  phase: string;
  gameId: string | undefined;
  ready: boolean;
  lastFundedGameId: string | null;
}): boolean {
  return (
    args.phase === "matched" &&
    !!args.gameId &&
    args.ready &&
    args.lastFundedGameId !== args.gameId
  );
}

/**
 * Whether the game already EXISTS on-chain — reads `games(gameId).status` over
 * the chain's RPC fallbacks (the same read path the hook uses elsewhere) and
 * decodes status >= 1. This verifies the on-chain EFFECT of createGame, which is
 * authoritative when the receipt poll is not: on mainnet the createGame tx can
 * MINE (game created + joinable) while the RPC lags on returning its receipt, so
 * a receipt-poll timeout does NOT mean the game wasn't created.
 */
export async function gameExistsOnChain(
  contract: Hex,
  gameId: Hex,
  chain: Chain,
  rpcUrls: string[],
): Promise<boolean> {
  const game = (await evmReadClient(chain, rpcUrls).readContract({
    address: contract,
    abi: COORDINATION_GAME_ABI,
    functionName: "games",
    args: [gameId],
  })) as readonly unknown[];
  return gameStatusExists(Number(game[0]));
}

/**
 * Send createGame and CONFIRM its on-chain EFFECT, not just its receipt.
 *
 * WHY: mainnet RPC receipt-fetch lag makes sendConfirmedTx's receipt poll time
 * out ("createGame did not confirm after N attempts") even though the tx MINED
 * and the game is created + joinable on-chain. Trusting only the receipt strands
 * the browser before chat (chat-input never enables). Verifying the on-chain
 * effect is authoritative. This supersedes the reverted selector-matching
 * approach, which couldn't see the selector once sendConfirmedTx flattened the
 * error to a message string.
 *
 * Both deps are injected so the send/detect decision is unit-testable with no
 * chain: `send` performs the tx (throws on a receipt-poll timeout OR a real
 * failure), `gameExists` reads games(gameId).status. Returns "sent" on a normal
 * confirm, "recovered" when the send threw but the game exists on-chain (proceed
 * anyway); re-throws only when the send threw AND the game is not on-chain.
 */
export async function sendCreateGameConfirmingEffect(
  send: () => Promise<unknown>,
  gameExists: () => Promise<boolean>,
): Promise<"sent" | "recovered"> {
  try {
    await send();
    return "sent";
  } catch (e) {
    // gameExists() reads games() over the RPC fallback, which can transiently
    // revert on a lagging node. A read failure must NOT mask the REAL createGame
    // error (that turned a plain revert into a confusing "games() reverted" that
    // hid the actual cause): default to "not confirmed" and re-throw the original.
    let exists = false;
    try {
      exists = await gameExists();
    } catch {
      exists = false;
    }
    if (exists) return "recovered";
    throw e;
  }
}

/**
 * Send a game CONTRACT call (session-key signed) and CONFIRM it. Encodes the call
 * and routes through sendConfirmedTx, so every game tx — createGame, joinGame,
 * commitGuess, revealGuess, resolveTimeout — gets the same confirm-or-replace
 * resilience instead of a fire-and-wait that stranded games on Base Sepolia.
 */
export async function sendAndConfirm(
  sessionKey: Hex,
  chain: Chain,
  rpcUrls: string[],
  call: { abi: Abi; functionName: string; args: readonly unknown[]; to: Hex },
  label: string,
): Promise<void> {
  const data = encodeFunctionData({
    abi: call.abi,
    functionName: call.functionName,
    args: call.args,
  });
  await sendConfirmedGameTx(sessionKey, chain, rpcUrls, { to: call.to, data, value: 0n }, label);
}
