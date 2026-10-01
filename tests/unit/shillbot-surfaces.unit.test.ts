// Pure-TS, CI-safe surfaces test for the Shillbot payout oracle — the
// deterministic counterpart to the game's metamorphic-surfaces.unit.test.ts.
// No chain, no validator: it (1) guards the committed golden fixture against TS
// drift and (2) proves deriveTaskOutcome is internally consistent across the
// full combinatorial axis cross-product (conservation, the kind-1 binary rule,
// monotonicity of payment in score). Runs in harness-unit.yml.

import { readFileSync } from "fs";
import { join } from "path";
import { assert } from "chai";
import {
  deriveTaskOutcome,
  computePayment,
  computeChallengeBond,
  TaskOutcomeKind,
  TaskScenario,
  MAX_SCORE,
} from "../helpers/task-outcome-oracle";

const FIXTURE = join(__dirname, "..", "fixtures", "task-payout-vectors.json");

describe("shillbot payout oracle — surfaces (pure TS)", () => {
  describe("golden-vector fixture parity", () => {
    const doc = JSON.parse(readFileSync(FIXTURE, "utf8"));
    const vectors: any[] = doc.vectors;

    it("committed fixture is non-empty and count matches", () => {
      assert.isArray(vectors);
      assert.isAbove(vectors.length, 0);
      assert.equal(doc.count, vectors.length);
      assert.equal(doc.maxScore, MAX_SCORE);
    });

    it("every committed vector recomputes from the current oracle (drift guard)", () => {
      // If this fails, the TS oracle changed: rerun
      //   npx tsx scripts/gen-task-payout-vectors.ts
      // and confirm the Rust + Solidity mirrors still pass.
      for (const [i, v] of vectors.entries()) {
        const s = v.scenario;
        const scenario: TaskScenario = {
          escrowLamports: BigInt(s.escrowLamports),
          qualityThreshold: s.qualityThreshold,
          protocolFeeBps: s.protocolFeeBps,
          compositeScore: s.compositeScore,
          verificationKind: s.verificationKind,
          challengeBondMultiplier: s.challengeBondMultiplier,
          bondSlashTreasuryBps: s.bondSlashTreasuryBps,
          outcome: s.outcome,
        };
        const { payment, fee, remainder } = computePayment(
          scenario.compositeScore,
          scenario.qualityThreshold,
          scenario.escrowLamports,
          scenario.protocolFeeBps
        );
        assert.equal(payment.toString(), v.payment, `vector ${i} payment`);
        assert.equal(fee.toString(), v.fee, `vector ${i} fee`);
        assert.equal(
          remainder.toString(),
          v.remainder,
          `vector ${i} remainder`
        );
        assert.equal(
          computeChallengeBond(
            scenario.escrowLamports,
            scenario.challengeBondMultiplier
          ).toString(),
          v.bond,
          `vector ${i} bond`
        );
        const p = deriveTaskOutcome(scenario);
        assert.equal(
          p.agentLamports.toString(),
          v.payout.agentLamports,
          `v${i} agent`
        );
        assert.equal(
          p.treasuryLamports.toString(),
          v.payout.treasuryLamports,
          `v${i} treasury`
        );
        assert.equal(
          p.clientLamports.toString(),
          v.payout.clientLamports,
          `v${i} client`
        );
        assert.equal(
          p.challengerLamports.toString(),
          v.payout.challengerLamports,
          `v${i} challenger`
        );
      }
    });
  });

  describe("internal consistency over the axis cross-product", () => {
    // The confirmed combinatorial axes (score uses boundary representatives).
    const ESCROW = 1_000_000_000n;
    const THRESHOLD = 200_000;
    const FEE_BPS = [0, 1000, 2500];
    const KIND0_SCORES = [1, THRESHOLD - 1, THRESHOLD, 600_000, MAX_SCORE];
    const KIND1_SCORES = [0, MAX_SCORE];
    const MULTIPLIERS = [2, 10];
    const SLASH_BPS = [3000, 5000];
    const OUTCOMES = [
      TaskOutcomeKind.Finalized,
      TaskOutcomeKind.ResolvedAgentWins,
      TaskOutcomeKind.ResolvedChallengerWins,
      TaskOutcomeKind.DefaultResolved,
      TaskOutcomeKind.Expired,
    ];

    function* cells(): Generator<TaskScenario> {
      for (const kind of [0, 1] as const) {
        const scores = kind === 0 ? KIND0_SCORES : KIND1_SCORES;
        for (const feeBps of FEE_BPS) {
          for (const score of scores) {
            for (const mult of MULTIPLIERS) {
              for (const bps of SLASH_BPS) {
                for (const outcome of OUTCOMES) {
                  yield {
                    escrowLamports: ESCROW,
                    qualityThreshold: THRESHOLD,
                    protocolFeeBps: feeBps,
                    compositeScore: score,
                    verificationKind: kind,
                    challengeBondMultiplier: mult,
                    bondSlashTreasuryBps: bps,
                    outcome,
                  };
                }
              }
            }
          }
        }
      }
    }

    it("every cell conserves escrow (+ bond when a challenge occurred)", () => {
      let n = 0;
      for (const s of cells()) {
        const p = deriveTaskOutcome(s);
        const total =
          p.agentLamports +
          p.treasuryLamports +
          p.clientLamports +
          p.challengerLamports;
        const bond = computeChallengeBond(
          s.escrowLamports,
          s.challengeBondMultiplier
        );
        const bondInPlay =
          s.outcome === TaskOutcomeKind.ResolvedAgentWins ||
          s.outcome === TaskOutcomeKind.ResolvedChallengerWins ||
          s.outcome === TaskOutcomeKind.DefaultResolved;
        const expected = bondInPlay
          ? s.escrowLamports + bond
          : s.escrowLamports;
        assert.equal(total.toString(), expected.toString(), "conservation");
        n++;
      }
      assert.isAbove(n, 100, "expected a substantial cell count");
    });

    it("payment is non-decreasing in score (fixed other params), finalize path", () => {
      for (const feeBps of FEE_BPS) {
        let prev = -1n;
        for (const score of KIND0_SCORES) {
          const { payment } = computePayment(score, THRESHOLD, ESCROW, feeBps);
          assert.isTrue(payment >= prev, `payment dropped at score ${score}`);
          prev = payment;
        }
      }
    });

    it("kind-1 rejects a non-binary score", () => {
      assert.throws(() =>
        deriveTaskOutcome({
          escrowLamports: ESCROW,
          qualityThreshold: THRESHOLD,
          protocolFeeBps: 1000,
          compositeScore: 500_000,
          verificationKind: 1,
          challengeBondMultiplier: 2,
          bondSlashTreasuryBps: 5000,
          outcome: TaskOutcomeKind.Finalized,
        })
      );
    });

    it("below-threshold and threshold-exact scores pay zero", () => {
      for (const score of [1, THRESHOLD - 1, THRESHOLD]) {
        const { payment, fee } = computePayment(score, THRESHOLD, ESCROW, 1000);
        assert.equal(payment, 0n, `score ${score} payment`);
        assert.equal(fee, 0n, `score ${score} fee`);
      }
    });
  });
});
