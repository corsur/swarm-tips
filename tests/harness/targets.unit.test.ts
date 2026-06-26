// Unit tests for the EVM + backend adapters and the cross-layer / metamorphic
// battery checks they enable — no viem, no network, CI-safe. The live wiring
// (real viem reads + game-api fetch) lives in tests/live/* and runs manually.
//
// Run: npx ts-mocha -p ./tsconfig.json tests/harness/targets.unit.test.ts

import { assert } from "chai";
import {
  evmGameStatusLabel,
  evmGamePhase,
  evmGameView,
  evmPhaseView,
} from "./evm-target";
import { backendPhase, backendPhaseView } from "./backend-target";
import { assertCrossLayer, assertMetamorphic } from "./assertions";
import { OutcomeKind } from "../helpers/outcome-oracle";

async function expectRejects(p: Promise<unknown>, ctx: string): Promise<void> {
  let threw = false;
  try {
    await p;
  } catch {
    threw = true;
  }
  assert.isTrue(threw, `expected a rejection: ${ctx}`);
}

const fixedEvm = (status: number, fields = {}) => ({
  status: async () => status,
  fields: async () => ({
    matchupType: 1,
    p1Guess: 1,
    p2Guess: 0,
    firstCommitter: 1,
    ...fields,
  }),
});
const fixedBackend = (matchStatus: string, bothCommitted: boolean) => ({
  matchStatus: async () => matchStatus,
  bothCommitted: async () => bothCommitted,
});

describe("harness/evm-target", () => {
  it("maps EVM status codes to the shared labels", () => {
    assert.equal(evmGameStatusLabel(1), "Pending");
    assert.equal(evmGameStatusLabel(4), "Revealing");
    assert.equal(evmGameStatusLabel(5), "Resolved");
    assert.throws(() => evmGameStatusLabel(9), /unknown EVM game status/);
  });

  it("maps status to the coarse cross-layer phase", () => {
    assert.equal(evmGamePhase(0), "Unmatched");
    assert.equal(evmGamePhase(2), "Live");
    assert.equal(evmGamePhase(4), "BothCommitted");
    assert.equal(evmGamePhase(5), "Resolved");
  });

  it("evmGameView derives the outcome from terminal fields via the oracle", async () => {
    const view = evmGameView(fixedEvm(5)); // hetero P1 correct / P2 wrong
    assert.equal(await view.readStatus(), "Resolved");
    assert.equal(await view.readOutcomeKind(), OutcomeKind.HeteroP1Wins);
  });

  it("evmGameView reports no outcome before resolution", async () => {
    const view = evmGameView(fixedEvm(2));
    assert.equal(await view.readOutcomeKind(), -1);
  });
});

describe("harness/backend-target", () => {
  it("maps matchmaking state to the shared phase", () => {
    assert.equal(backendPhase("waiting", false), "Unmatched");
    assert.equal(backendPhase("matched", false), "Live");
    assert.equal(backendPhase("matched", true), "BothCommitted");
    assert.throws(() => backendPhase("bogus", false), /unknown backend/);
  });
});

describe("harness/battery cross-layer over real adapters", () => {
  it("passes when chain and backend agree on the phase", async () => {
    await assertCrossLayer([
      evmPhaseView(fixedEvm(2)), // on-chain: Active -> Live
      backendPhaseView(fixedBackend("matched", false)), // backend: matched -> Live
    ]);
  });

  it("catches backend lagging the chain (the Step-5 bug class)", async () => {
    // Chain has both commits (Revealing -> BothCommitted) but the backend still
    // reports the match as not-yet-committed (Live). That divergence is exactly
    // the integration drift a monolithic single-layer test misses.
    await expectRejects(
      assertCrossLayer([
        evmPhaseView(fixedEvm(4)), // BothCommitted
        backendPhaseView(fixedBackend("matched", false)), // Live
      ]),
      "chain ahead of backend"
    );
  });
});

describe("harness/battery metamorphic across runtimes", () => {
  it("passes when the EVM and Solana legs reach the same outcome + net", () => {
    // Same logical scenario, different runtimes; the EVM adapter and a Solana
    // (bankrun) run must agree on the oracle outcome and protocol movement.
    const evmResult = { outcome: OutcomeKind.HeteroP1Wins, protocolNet: 0n };
    const solResult = { outcome: OutcomeKind.HeteroP1Wins, protocolNet: 0n };
    assertMetamorphic(evmResult, solResult);
  });

  it("catches the two runtimes diverging", () => {
    assert.throws(
      () =>
        assertMetamorphic(
          { outcome: OutcomeKind.HeteroP1Wins, protocolNet: 0n },
          { outcome: OutcomeKind.HeteroP2Wins, protocolNet: 0n }
        ),
      /metamorphic/
    );
  });
});
