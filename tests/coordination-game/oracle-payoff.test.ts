import { assert } from "chai";
import {
  OutcomeKind,
  deriveTerminalOutcome,
  deriveClaimOutcome,
  resolvePayoff,
} from "../helpers/outcome-oracle.ts";

describe("payoff matrix — exhaustive oracle", () => {
  const STAKE_L = BigInt(50_000_000); // 0.05 SOL

  // Independent expectation: derived straight from the spec, NOT from the
  // function under test, so a wrong oracle cannot agree with itself.
  function expectedTerminal(
    mt: 0 | 1,
    p1: 0 | 1,
    p2: 0 | 1,
    fc: 1 | 2
  ): OutcomeKind {
    const p1ok = p1 === mt;
    const p2ok = p2 === mt;
    if (mt === 0) {
      if (p1ok && p2ok) return OutcomeKind.HomogBothCorrect;
      if (p1ok) return OutcomeKind.HomogP1Correct;
      if (p2ok) return OutcomeKind.HomogP2Correct;
      return OutcomeKind.BothWrong;
    }
    if (!p1ok && !p2ok) return OutcomeKind.BothWrong;
    if (p1ok && p2ok)
      return fc === 1 ? OutcomeKind.HeteroP1Wins : OutcomeKind.HeteroP2Wins;
    return p1ok ? OutcomeKind.HeteroP1Wins : OutcomeKind.HeteroP2Wins;
  }

  it("derives the right outcome and conserves stake for all 16 terminal transcripts", () => {
    const two = STAKE_L * 2n;
    for (const mt of [0, 1] as const) {
      for (const p1 of [0, 1] as const) {
        for (const p2 of [0, 1] as const) {
          for (const fc of [1, 2] as const) {
            const t = {
              stepCount: 4,
              matchupType: mt,
              p1Guess: p1,
              p2Guess: p2,
              firstCommitter: fc,
            };
            assert.equal(
              deriveTerminalOutcome(t),
              expectedTerminal(mt, p1, p2, fc),
              `terminal outcome mt=${mt} p1=${p1} p2=${p2} fc=${fc}`
            );
            const payoff = resolvePayoff({ ...t, stake: STAKE_L });
            assert.equal(
              (
                payoff.p1Return +
                payoff.p2Return +
                payoff.tournamentGain
              ).toString(),
              two.toString(),
              `stake conservation mt=${mt} p1=${p1} p2=${p2} fc=${fc}`
            );
          }
        }
      }
    }
  });

  it("derives timeout outcomes for partial transcripts (steps 0-3)", () => {
    const base = { matchupType: 1 as const, p1Guess: 255, p2Guess: 255 };
    // step 1: only the first committer landed -> committer wins.
    assert.equal(
      deriveClaimOutcome({ ...base, stepCount: 1, firstCommitter: 1 }),
      OutcomeKind.TimeoutP1Wins
    );
    assert.equal(
      deriveClaimOutcome({ ...base, stepCount: 1, firstCommitter: 2 }),
      OutcomeKind.TimeoutP2Wins
    );
    // step 3: both committed, exactly one revealed -> revealer wins.
    assert.equal(
      deriveClaimOutcome({
        matchupType: 1,
        p1Guess: 1,
        p2Guess: 255,
        stepCount: 3,
        firstCommitter: 1,
      }),
      OutcomeKind.TimeoutP1Wins
    );
    assert.equal(
      deriveClaimOutcome({
        matchupType: 1,
        p1Guess: 255,
        p2Guess: 1,
        stepCount: 3,
        firstCommitter: 1,
      }),
      OutcomeKind.TimeoutP2Wins
    );
    // both revealed at step 3, and steps 0 & 2 -> both forfeit.
    assert.equal(
      deriveClaimOutcome({
        matchupType: 1,
        p1Guess: 1,
        p2Guess: 1,
        stepCount: 3,
        firstCommitter: 1,
      }),
      OutcomeKind.TimeoutBothForfeit
    );
    for (const stepCount of [0, 2]) {
      assert.equal(
        deriveClaimOutcome({ ...base, stepCount, firstCommitter: 1 }),
        OutcomeKind.TimeoutBothForfeit
      );
    }
  });
});
