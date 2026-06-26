// Pure unit tests for the harness verification substrate (no chain, no validator,
// no network). Proves each property-battery check PASSES on a valid realized run
// and CATCHES the corresponding violation (negative control) — the latter is what
// makes the battery worth anything.
//
// Run: npx ts-mocha -p ./tsconfig.json tests/harness/battery.unit.test.ts

import { assert } from "chai";
import { Ledger, Transcript } from "./ledger";
import {
  StateView,
  assertConservation,
  assertOracleOutcome,
  assertPayoffMatchesLedger,
  assertLegalTransition,
  assertTerminal,
  assertCrossLayer,
  assertMetamorphic,
  assertNonDecreasing,
  assertStrictlyIncreasing,
  TransitionGraph,
} from "./assertions";
import { OutcomeKind } from "../helpers/outcome-oracle";

/** In-memory StateView for testing the battery without a runtime. */
class FakeView implements StateView {
  constructor(private status: string, private outcome: number) {}
  async readStatus(): Promise<string> {
    return this.status;
  }
  async readOutcomeKind(): Promise<number> {
    return this.outcome;
  }
}

function expectThrows(fn: () => void, context: string): void {
  assert.throws(fn, undefined, undefined, `expected a violation: ${context}`);
}
async function expectRejects(
  p: Promise<unknown>,
  context: string
): Promise<void> {
  let threw = false;
  try {
    await p;
  } catch {
    threw = true;
  }
  assert.isTrue(threw, `expected a rejection: ${context}`);
}

// The coordination-game status graph (same-chain) used for legality tests.
const GAME_GRAPH: TransitionGraph = {
  Pending: ["Active"],
  Active: ["Committing", "Resolved"],
  Committing: ["Revealing", "Resolved"],
  Revealing: ["Resolved"],
  Resolved: [],
};

describe("harness/Transcript — realized step derivation", () => {
  it("derives stepCount + firstCommitter from the actual sequence", () => {
    const t = new Transcript();
    assert.equal(t.stepCount, 0, "no commits → step 0");
    t.commit(2);
    assert.equal(t.stepCount, 1, "one commit → step 1");
    assert.equal(t.firstCommitter, 2, "P2 committed first");
    t.commit(1);
    assert.equal(t.stepCount, 2, "both committed → step 2");
    t.reveal(1, 1, 1); // P1 reveals guess=1, matchup=1 (different teams)
    assert.equal(t.stepCount, 3, "one reveal → step 3");
    t.reveal(2, 0);
    assert.equal(t.stepCount, 4, "both revealed → terminal");
    const o = t.toOracleInput();
    assert.deepEqual(o, {
      stepCount: 4,
      matchupType: 1,
      p1Guess: 1,
      p2Guess: 0,
      firstCommitter: 2,
    });
  });

  it("first reveal fixes the matchup type", () => {
    const t = new Transcript();
    t.commit(1);
    t.commit(2);
    t.reveal(1, 0, 0);
    t.reveal(2, 0, 1); // later matchupType is ignored — first reveal binds it
    assert.equal(t.toOracleInput().matchupType, 0);
  });
});

describe("harness/battery (1) conservation", () => {
  it("passes for a payoff where the protocol is a passthrough (net 0)", () => {
    // Hetero P1 wins: P2's stake moves to P1 via the game PDA; system nets 0.
    const led = new Ledger();
    led.open("p1", "player", 100n);
    led.open("p2", "player", 100n);
    led.open("gamePda", "protocol", 0n);
    // both stake 50 into the PDA, P1 takes the whole 100 pot back.
    led.close("p1", 100n - 50n + 100n); // 150
    led.close("p2", 100n - 50n); // 50
    led.close("gamePda", 0n);
    assertConservation(led);
  });

  it("passes a FORFEIT where the protocol subtotal is non-zero (system still nets 0)", () => {
    // Both forfeit: players lose 2*stake, treasury+prize ABSORB it. Protocol net
    // != 0, but whole-system conservation still holds — the universal property.
    const led = new Ledger();
    led.open("p1", "player", 100n);
    led.open("p2", "player", 100n);
    led.open("treasury", "protocol", 0n);
    led.close("p1", 50n); // staked 50, forfeited
    led.close("p2", 50n);
    led.close("treasury", 100n); // protocol absorbed 2*stake
    assertConservation(led);
  });

  it("catches value created out of nothing", () => {
    const led = new Ledger();
    led.open("gamePda", "protocol", 0n);
    led.close("gamePda", 7n); // 7 lamports appeared from nowhere
    expectThrows(() => assertConservation(led), "value created");
  });

  it("catches value leaving beyond the declared fee", () => {
    const led = new Ledger();
    led.open("p1", "player", 100n);
    led.open("treasury", "protocol", 0n);
    led.close("p1", 90n); // lost 10
    led.close("treasury", 5n); // only 5 accounted
    expectThrows(
      () => assertConservation(led, { feesLamports: 0n }),
      "unaccounted value loss"
    );
  });
});

describe("harness/battery (2) oracle-from-realized-transcript", () => {
  it("passes when the view matches the oracle-derived outcome", async () => {
    const t = new Transcript();
    t.commit(1);
    t.commit(2);
    t.reveal(1, 1, 1); // hetero, P1 correct
    t.reveal(2, 0); // P2 wrong → HeteroP1Wins
    const view = new FakeView("Resolved", OutcomeKind.HeteroP1Wins);
    const got = await assertOracleOutcome(view, t.toOracleInput());
    assert.equal(got, OutcomeKind.HeteroP1Wins);
  });

  it("catches a chain outcome that disagrees with the realized transcript", async () => {
    const t = new Transcript();
    t.commit(1);
    t.commit(2);
    t.reveal(1, 1, 1);
    t.reveal(2, 0);
    const wrong = new FakeView("Resolved", OutcomeKind.HeteroP2Wins);
    await expectRejects(
      assertOracleOutcome(wrong, t.toOracleInput()),
      "outcome mismatch"
    );
  });

  it("derives timeout outcomes for partial transcripts (step 1)", async () => {
    const t = new Transcript();
    t.commit(1); // only P1 committed
    const view = new FakeView("Resolved", OutcomeKind.TimeoutP1Wins);
    await assertOracleOutcome(view, t.toOracleInput());
  });

  it("reconciles protocol net against the oracle payoff split", () => {
    // Homog, only P1 correct → tournamentGain = 2*stake - stake/2 = 1.5*stake.
    const stake = 100n;
    const led = new Ledger();
    led.open("prizeAndTreasury", "protocol", 0n);
    led.close("prizeAndTreasury", 150n);
    assertPayoffMatchesLedger(led, {
      stepCount: 4,
      matchupType: 0,
      p1Guess: 0,
      p2Guess: 1,
      firstCommitter: 1,
      stake,
    });
  });
});

describe("harness/battery (3) state-machine legality", () => {
  it("accepts legal transitions and the terminal state", () => {
    assertLegalTransition(GAME_GRAPH, "Pending", "Active");
    assertLegalTransition(GAME_GRAPH, "Active", "Committing");
    assertLegalTransition(GAME_GRAPH, "Revealing", "Resolved");
    assertTerminal(GAME_GRAPH, "Resolved");
  });

  it("rejects an illegal transition", () => {
    expectThrows(
      () => assertLegalTransition(GAME_GRAPH, "Pending", "Resolved"),
      "Pending cannot jump to Resolved"
    );
  });

  it("rejects a 'terminal' that still has outgoing edges", () => {
    expectThrows(
      () => assertTerminal(GAME_GRAPH, "Active"),
      "Active is not terminal"
    );
  });
});

describe("harness/battery (4) cross-layer agreement", () => {
  it("passes when every view reports the same status", async () => {
    await assertCrossLayer([
      new FakeView("Resolved", 1),
      new FakeView("Resolved", 1),
      new FakeView("Resolved", 1),
    ]);
  });

  it("catches a backend view that lags the chain (the Step-5 bug class)", async () => {
    await expectRejects(
      assertCrossLayer([
        new FakeView("Resolved", 1), // chain
        new FakeView("Active", -1), // backend still thinks it's mid-game
      ]),
      "cross-layer divergence"
    );
  });
});

describe("harness/battery (5) metamorphic equivalence", () => {
  it("passes when two targets agree on outcome + protocol net", () => {
    assertMetamorphic(
      { outcome: OutcomeKind.HeteroP1Wins, protocolNet: 0n },
      { outcome: OutcomeKind.HeteroP1Wins, protocolNet: 0n }
    );
  });

  it("catches two targets that resolve differently", () => {
    expectThrows(
      () =>
        assertMetamorphic(
          { outcome: OutcomeKind.HeteroP1Wins, protocolNet: 0n },
          { outcome: OutcomeKind.HeteroP2Wins, protocolNet: 0n }
        ),
      "targets disagree on outcome"
    );
  });
});

describe("harness/battery (6) monotonic bounds", () => {
  it("accepts non-decreasing and strictly-increasing sequences", () => {
    assertNonDecreasing(3, 3, "wins"); // equal is allowed
    assertNonDecreasing(3, 5, "wins");
    assertStrictlyIncreasing(1, 4, "best_step_count");
  });

  it("catches a decrease and a non-strict increase", () => {
    expectThrows(() => assertNonDecreasing(5, 4, "wins"), "wins decreased");
    expectThrows(
      () => assertStrictlyIncreasing(4, 4, "best_step_count"),
      "equal step is not an increase"
    );
  });
});
