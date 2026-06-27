// Portable property battery for the coordination game — the path-independent
// checks over the REALIZED result, shipped in @corsur/coordination-game-sdk so
// any e2e (on-chain, Playwright, MCP) verifies the same way. Pure: NO chai, no
// chain deps — throws plain Errors so it runs anywhere.
//
// The point: at the browser/MCP layer the end state is genuinely variable — the
// matchmaker hides matchup_type and the opponent (grok / another player) picks
// its own guess. So you don't assert a fixed outcome; you read the realized game
// from chain after resolution and verify properties that hold for ANY transcript.

import {
  OutcomeKind,
  Transcript as OracleInput,
  deriveClaimOutcome,
  resolvePayoff,
  TERMINAL_STEP_COUNT,
} from "./outcome-oracle";

/** A realized same-chain game, decoded from the on-chain Game account. */
export interface RealizedGame {
  matchupType: number;
  p1Guess: number;
  p2Guess: number;
  firstCommitter: number;
  p1Return: bigint;
  p2Return: bigint;
  tournamentGain: bigint;
  stake: bigint;
}

function fail(msg: string): never {
  throw new Error(`battery: ${msg}`);
}

/** The oracle input a fully-revealed (terminal) realized game represents. */
export function realizedTranscript(g: RealizedGame): OracleInput {
  return {
    stepCount: TERMINAL_STEP_COUNT,
    matchupType: g.matchupType,
    p1Guess: g.p1Guess,
    p2Guess: g.p2Guess,
    firstCommitter: g.firstCommitter,
  };
}

/**
 * (1) Oracle-from-realized-transcript: the on-chain settlement
 * (p1_return / p2_return / tournament_gain) must equal what the shared oracle
 * derives from the realized transcript. The outcome is computed from what
 * actually happened, never assumed — so a variable end state is fine. Returns
 * the derived OutcomeKind.
 */
export function assertPayoffMatchesChain(g: RealizedGame): OutcomeKind {
  const t = realizedTranscript(g);
  const p = resolvePayoff({ ...t, stake: g.stake });
  if (g.p1Return !== p.p1Return)
    fail(`p1_return ${g.p1Return} != oracle ${p.p1Return}`);
  if (g.p2Return !== p.p2Return)
    fail(`p2_return ${g.p2Return} != oracle ${p.p2Return}`);
  if (g.tournamentGain !== p.tournamentGain)
    fail(`tournament_gain ${g.tournamentGain} != oracle ${p.tournamentGain}`);
  return deriveClaimOutcome(t);
}

/** (2) Conservation: stake is never created or destroyed. */
export function assertConservation(g: RealizedGame): void {
  const total = g.p1Return + g.p2Return + g.tournamentGain;
  const expected = g.stake * 2n;
  if (total !== expected) fail(`conservation: ${total} != 2*stake ${expected}`);
}

/**
 * (3) Cross-layer: the outcome OBSERVED on a surface (UI text / MCP get_result)
 * must equal the outcome the chain + oracle agree on — catches a surface that
 * reports a result the chain didn't actually produce.
 */
export function assertOutcomeAgrees(
  g: RealizedGame,
  observed: OutcomeKind
): void {
  const expected = deriveClaimOutcome(realizedTranscript(g));
  if (observed !== expected)
    fail(
      `cross-layer: surface ${OutcomeKind[observed]} != chain/oracle ${OutcomeKind[expected]}`
    );
}

/**
 * Full same-chain verdict in one call: conservation + payoff-from-transcript,
 * plus optional cross-layer if a surface outcome was observed. Returns the
 * derived OutcomeKind so callers can compare runs (metamorphic).
 */
export function verifyRealizedGame(
  g: RealizedGame,
  observed?: OutcomeKind
): OutcomeKind {
  assertConservation(g);
  const outcome = assertPayoffMatchesChain(g);
  if (observed !== undefined) assertOutcomeAgrees(g, observed);
  return outcome;
}
