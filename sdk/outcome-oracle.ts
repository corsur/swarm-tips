// Outcome oracle — the canonical coordination-game payoff matrix, in TypeScript.
//
// This is the off-chain test/tooling mirror of the on-chain derivation logic
// that three implementations already agree on (pinned by the golden vectors in
// tests/fixtures/cert-vectors.json):
//
//   - crates/chain-core/src/cert_schema.rs   `Checkpoint::derive_outcome_kind`
//   - evm/src/CertLib.sol                     `deriveClaimOutcome`
//   - programs/coordination-game/src/payoff.rs `resolve_game`
//
// The harness uses it as the single source of truth for "what SHOULD this game
// resolve to" so every scenario — same-chain Solana, same-chain EVM, cross-chain
// — asserts on-chain reality against ONE expectation, instead of re-deriving the
// matrix per test. If this drifts from the on-chain encoders, the combinatorial
// integration tests (which compare on-chain resolution to this oracle) fail.

export const GUESS_SAME_TEAM = 0;
export const GUESS_DIFF_TEAM = 1;
export const UNREVEALED = 255;
export const TERMINAL_STEP_COUNT = 4;

/** Wire-format outcome kinds (0..=8), identical to cert_schema.rs `OutcomeKind`. */
export enum OutcomeKind {
  HomogBothCorrect = 0,
  HomogP1Correct = 1,
  HomogP2Correct = 2,
  BothWrong = 3,
  HeteroP1Wins = 4,
  HeteroP2Wins = 5,
  TimeoutP1Wins = 6,
  TimeoutP2Wins = 7,
  TimeoutBothForfeit = 8,
}

export interface Transcript {
  /** 0..=4 (TERMINAL_STEP_COUNT), or anything else for a malformed transcript. */
  stepCount: number;
  matchupType: number; // 0 same-team, 1 diff-team
  p1Guess: number; // 0 | 1, or UNREVEALED (255)
  p2Guess: number;
  firstCommitter: number; // 1 | 2 (255 before any commit)
}

/** Mirrors `cert_schema.rs::derive_terminal_outcome` / `CertLib.deriveTerminalOutcome`. */
export function deriveTerminalOutcome(t: Transcript): OutcomeKind {
  const p1Correct = t.p1Guess === t.matchupType;
  const p2Correct = t.p2Guess === t.matchupType;
  if (t.matchupType === 0) {
    if (p1Correct && p2Correct) return OutcomeKind.HomogBothCorrect;
    if (p1Correct) return OutcomeKind.HomogP1Correct;
    if (p2Correct) return OutcomeKind.HomogP2Correct;
    return OutcomeKind.BothWrong;
  }
  // Heterogeneous: winner takes both stakes; ties break to the first committer.
  if (!p1Correct && !p2Correct) return OutcomeKind.BothWrong;
  if (p1Correct === p2Correct) {
    return t.firstCommitter === 1
      ? OutcomeKind.HeteroP1Wins
      : OutcomeKind.HeteroP2Wins;
  }
  return p1Correct ? OutcomeKind.HeteroP1Wins : OutcomeKind.HeteroP2Wins;
}

/** Mirrors `cert_schema.rs::derive_outcome_kind` / `CertLib.deriveClaimOutcome` (terminal + timeouts). */
export function deriveClaimOutcome(t: Transcript): OutcomeKind {
  if (t.stepCount === TERMINAL_STEP_COUNT) return deriveTerminalOutcome(t);
  if (t.stepCount === 1) {
    // One commit landed; the committer wins. Inconsistent committer → both forfeit.
    if (t.firstCommitter === 1) return OutcomeKind.TimeoutP1Wins;
    if (t.firstCommitter === 2) return OutcomeKind.TimeoutP2Wins;
    return OutcomeKind.TimeoutBothForfeit;
  }
  if (t.stepCount === 3) {
    // Both committed, exactly one revealed: the revealer wins. Both-set or
    // both-unset is inconsistent → both forfeit.
    const p1Revealed = t.p1Guess !== UNREVEALED;
    const p2Revealed = t.p2Guess !== UNREVEALED;
    if (p1Revealed === p2Revealed) return OutcomeKind.TimeoutBothForfeit;
    return p1Revealed ? OutcomeKind.TimeoutP1Wins : OutcomeKind.TimeoutP2Wins;
  }
  // step 0 (nobody committed) / step 2 (both committed, none revealed).
  return OutcomeKind.TimeoutBothForfeit;
}

export interface Payoff {
  p1Return: bigint;
  p2Return: bigint;
  tournamentGain: bigint;
}

/**
 * Mirrors `programs/coordination-game/src/payoff.rs::resolve_game` — the
 * same-chain settlement amounts (lamports). `firstCommitter` must be 1 or 2 in
 * the heterogeneous both-correct case. Conserves stake: the three returns sum to
 * `2 * stake`.
 */
export function resolvePayoff(t: Transcript & { stake: bigint }): Payoff {
  if (t.stake <= 0n) throw new Error("stake must be positive");
  const two = t.stake * 2n;
  const half = t.stake / 2n;

  const payoff =
    t.matchupType === 0
      ? resolveHomogeneous(t.p1Guess, t.p2Guess, t.stake, half, two)
      : resolveHeterogeneous(t.p1Guess, t.p2Guess, t.firstCommitter, two);

  const total = payoff.p1Return + payoff.p2Return + payoff.tournamentGain;
  if (total !== two) {
    throw new Error(`lamport conservation broken: ${total} != ${two}`);
  }
  return payoff;
}

function resolveHomogeneous(
  p1Guess: number,
  p2Guess: number,
  stake: bigint,
  half: bigint,
  two: bigint
): Payoff {
  const p1Correct = p1Guess === GUESS_SAME_TEAM;
  const p2Correct = p2Guess === GUESS_SAME_TEAM;
  if (p1Correct && p2Correct)
    return { p1Return: stake, p2Return: stake, tournamentGain: 0n };
  if (p1Correct)
    return { p1Return: half, p2Return: 0n, tournamentGain: two - half };
  if (p2Correct)
    return { p1Return: 0n, p2Return: half, tournamentGain: two - half };
  return { p1Return: 0n, p2Return: 0n, tournamentGain: two };
}

function resolveHeterogeneous(
  p1Guess: number,
  p2Guess: number,
  firstCommitter: number,
  two: bigint
): Payoff {
  const p1Correct = p1Guess === GUESS_DIFF_TEAM;
  const p2Correct = p2Guess === GUESS_DIFF_TEAM;
  if (!p1Correct && !p2Correct)
    return { p1Return: 0n, p2Return: 0n, tournamentGain: two };
  if (firstCommitter !== 1 && firstCommitter !== 2) {
    throw new Error("first_committer must be 1 or 2");
  }
  const p1Wins = p1Correct === p2Correct ? firstCommitter === 1 : p1Correct;
  return p1Wins
    ? { p1Return: two, p2Return: 0n, tournamentGain: 0n }
    : { p1Return: 0n, p2Return: two, tournamentGain: 0n };
}

/**
 * Split a forfeited tournament gain into (treasury, prize-pool) shares the way
 * the program does at resolution: treasury gets `gain * bps / 10_000` (floored),
 * the prize pool gets the remainder. Bps is bounded to [2000, 8000] on-chain.
 */
export function splitTournamentGain(
  gain: bigint,
  treasurySplitBps: number
): { treasuryGain: bigint; prizeGain: bigint } {
  const treasuryGain = (gain * BigInt(treasurySplitBps)) / 10_000n;
  return { treasuryGain, prizeGain: gain - treasuryGain };
}

/** Matchup type encoded in the last bit of the 32-byte preimage (R_matchup[31] & 1). */
export function matchupTypeFromR(r: Uint8Array | number[]): number {
  return r[31] & 1;
}
