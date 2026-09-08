/**
 * Pure decision rules for the same-chain EVM game, extracted from `useEvmGame`
 * so the two paths that previously shipped bugs — reveal ordering and timeout
 * recovery — are locked by deterministic unit tests instead of relying on a
 * live e2e that only ever exercises the happy path.
 *
 * Mirrors CoordinationGame.sol exactly; keep in lockstep with the contract.
 */

import type { Hex } from "viem";

/** games().matchupType before any reveal binds it. */
export const MATCHUP_UNSET = 255;

/** games().pNGuess before that player reveals. */
export const UNREVEALED = 255;

/** The 32-byte zero sentinel the SECOND revealer must pass for rMatchup. */
export const ZERO_BYTES32: Hex = `0x${"00".repeat(32)}`;

/**
 * The `rMatchup` argument for revealGuess, chosen from ON-CHAIN state (never the
 * creator/joiner role). The FIRST revealer binds the matchup with the real
 * r_matchup; the SECOND — matchup already bound on-chain — MUST pass the zero
 * sentinel, or the contract reverts CertMismatch (the reveal-order bug). Returns
 * null only when we're the first revealer but don't yet hold r_matchup (both
 * players haven't committed), which the caller surfaces as "not ready".
 */
export function revealMatchupArg(
  onChainMatchupType: number,
  rMatchup: Hex | null,
): Hex | null {
  const bound = onChainMatchupType !== MATCHUP_UNSET;
  return bound ? ZERO_BYTES32 : rMatchup;
}

// CoordinationGame.Status values resolveTimeout can crank (opponent stalled).
export const STATUS_ACTIVE = 2;
export const STATUS_COMMITTING = 3;
export const STATUS_REVEALING = 4;

export interface EvmGameTimes {
  status: number;
  activatedAt: bigint;
  firstCommitAt: bigint;
  bothCommitAt: bigint;
  commitWindowSecs: bigint;
  revealWindowSecs: bigint;
}

/**
 * The unix second at which an opponent's stall becomes resolvable via
 * resolveTimeout, or null when the status has no timeout (not Active/Committing/
 * Revealing). Anchors mirror CoordinationGame.resolveTimeout: Active/Committing
 * time out on the commit window (from join / first-commit); Revealing on the
 * reveal window (from both-commit).
 */
export function timeoutDeadline(g: EvmGameTimes): bigint | null {
  if (g.status === STATUS_ACTIVE) return g.activatedAt + g.commitWindowSecs;
  if (g.status === STATUS_COMMITTING) return g.firstCommitAt + g.commitWindowSecs;
  if (g.status === STATUS_REVEALING) return g.bothCommitAt + g.revealWindowSecs;
  return null;
}

/** Whether the opponent's stall is resolvable at `nowSecs`. */
export function timeoutClaimableAt(g: EvmGameTimes, nowSecs: bigint): boolean {
  const deadline = timeoutDeadline(g);
  return deadline !== null && nowSecs >= deadline;
}

/** The on-chain Game fields needed to resume a game after a page reload. */
export interface EvmResumeGame {
  status: number;
  player1: string;
  player2: string;
  p1Commit: Hex;
  p2Commit: Hex;
  p1Guess: number;
  p2Guess: number;
}

export interface EvmResumeState {
  phase: "chat" | "committed";
  bothCommitted: boolean;
}

function sameAddr(a: string, b: string): boolean {
  return a.toLowerCase() === b.toLowerCase();
}

/**
 * Is this match on the chain the wallet is currently connected to?
 *
 * The resume path takes the contract address from the MATCH record but reads it
 * through the CONNECTED chain's client. Those are independent, and when they
 * disagree it queries an address that means nothing on that chain. Observed on
 * both mainnet e2e runs (2026-08-13): connected to eip155:1, it read the Base
 * Sepolia proxy 0x4fbbceb9… and logged `"games" returned no data ("0x")`.
 *
 * Exact string equality on the CAIP-2 id, deliberately: no prefix matching (so
 * `eip155:1` can never satisfy chain 11), and a missing/malformed/non-EVM value
 * returns false rather than defaulting to allow — the boundary rejects rather
 * than guesses.
 */
export function matchIsOnConnectedChain(
  matchChain: string | undefined,
  connectedChainId: number,
): boolean {
  if (!matchChain) return false;
  return matchChain === `eip155:${connectedChainId}`;
}

/**
 * The local phase to resume into after a page reload, derived purely from the
 * on-chain game + our session address — EVM's `/evmplay` has no gameId in the
 * URL, so a refresh otherwise strands a committed player who can no longer
 * reveal. Returns null (stay idle) when there is nothing to resume: no
 * in-progress game (Pending/Resolved/None), not a participant, or we already
 * revealed (the game will resolve and auto-sweep on its own).
 */
export function deriveResumeState(
  g: EvmResumeGame,
  myAddr: string,
): EvmResumeState | null {
  if (
    g.status !== STATUS_ACTIVE &&
    g.status !== STATUS_COMMITTING &&
    g.status !== STATUS_REVEALING
  ) {
    return null;
  }
  const isP1 = sameAddr(myAddr, g.player1);
  const isP2 = sameAddr(myAddr, g.player2);
  if (!isP1 && !isP2) return null;

  const myGuess = isP1 ? g.p1Guess : g.p2Guess;
  if (myGuess !== UNREVEALED) return null; // already revealed

  const iCommitted = (isP1 ? g.p1Commit : g.p2Commit) !== ZERO_BYTES32;
  const bothCommitted = g.p1Commit !== ZERO_BYTES32 && g.p2Commit !== ZERO_BYTES32;
  return { phase: iCommitted ? "committed" : "chat", bothCommitted };
}

/** The resolved-game fields needed to describe an outcome to the player. */
export interface EvmResolvedGame {
  player1: string;
  player2: string;
  p1Guess: number;
  p2Guess: number;
  matchupType: number;
  firstCommitter: number;
}

/**
 * Turn a resolved on-chain EVM game into the same player-relative shape the
 * Solana side uses, so `outcomeLabel`/`shareText` can describe it identically.
 *
 * The EVM end-screen said only "Revealed — the game settles on-chain and your
 * winnings return to your wallet automatically": no win/loss, no explorer link,
 * no share. Solana has told players the outcome all along. The payoff matrix is
 * the same on both chains (homogeneous by design), so the difference was purely
 * that nobody built the panel — not a missing concept.
 *
 * Returns null when the game has no bound matchup or an unrevealed guess, i.e.
 * there is not yet a truthful outcome to state.
 */
export function evmGameResult(
  g: EvmResolvedGame,
  myAddr: string,
): { myGuess: 0 | 1; opponentGuess: 0 | 1; matchupType: 0 | 1; iAmP1: boolean; firstCommitter: number } | null {
  const iAmP1 = sameAddr(myAddr, g.player1);
  if (!iAmP1 && !sameAddr(myAddr, g.player2)) return null;

  const myGuess = iAmP1 ? g.p1Guess : g.p2Guess;
  const oppGuess = iAmP1 ? g.p2Guess : g.p1Guess;
  // 255 is the unrevealed sentinel and MATCHUP_UNSET means nobody bound it.
  if (myGuess > 1 || oppGuess > 1 || g.matchupType > 1) return null;

  return {
    myGuess: myGuess as 0 | 1,
    opponentGuess: oppGuess as 0 | 1,
    matchupType: g.matchupType as 0 | 1,
    iAmP1,
    firstCommitter: g.firstCommitter,
  };
}
