// Unit tests for the portable SDK battery (sdk/battery.ts) — pure, no chain.
// Proves each path-independent check passes on a valid realized game and CATCHES
// its violation. Run: npx ts-mocha -p ./tsconfig.json tests/harness/battery-sdk.unit.test.ts

import { assert } from "chai";
import { OutcomeKind } from "../../sdk/outcome-oracle";
import {
  RealizedGame,
  assertConservation,
  assertPayoffMatchesChain,
  assertOutcomeAgrees,
  verifyRealizedGame,
} from "../../sdk/battery";

const STAKE = 100n;

// Hetero, P1 correct (guess==matchup), P2 wrong → P1 takes the pot (2*stake).
const heteroP1Wins: RealizedGame = {
  matchupType: 1,
  p1Guess: 1,
  p2Guess: 0,
  firstCommitter: 1,
  p1Return: 200n,
  p2Return: 0n,
  tournamentGain: 0n,
  stake: STAKE,
};

// Homog, only P1 correct → P1 gets half, protocol takes 1.5*stake.
const homogP1Only: RealizedGame = {
  matchupType: 0,
  p1Guess: 0,
  p2Guess: 1,
  firstCommitter: 1,
  p1Return: 50n,
  p2Return: 0n,
  tournamentGain: 150n,
  stake: STAKE,
};

describe("sdk/battery (1) payoff-from-realized-transcript", () => {
  it("passes when on-chain returns match the oracle, returning the outcome", () => {
    assert.equal(
      assertPayoffMatchesChain(heteroP1Wins),
      OutcomeKind.HeteroP1Wins
    );
    assert.equal(
      assertPayoffMatchesChain(homogP1Only),
      OutcomeKind.HomogP1Correct
    );
  });

  it("catches an on-chain return that disagrees with the oracle", () => {
    const tampered = { ...heteroP1Wins, p1Return: 199n }; // off by one
    assert.throws(() => assertPayoffMatchesChain(tampered), /p1_return/);
  });
});

describe("sdk/battery (2) conservation", () => {
  it("passes when returns + gain == 2*stake", () => {
    assertConservation(heteroP1Wins);
    assertConservation(homogP1Only);
  });

  it("catches stake created or destroyed", () => {
    assert.throws(
      () => assertConservation({ ...homogP1Only, tournamentGain: 149n }),
      /conservation/
    );
  });
});

describe("sdk/battery (3) cross-layer", () => {
  it("passes when the surface-observed outcome matches chain/oracle", () => {
    assertOutcomeAgrees(heteroP1Wins, OutcomeKind.HeteroP1Wins);
  });

  it("catches a surface reporting the wrong outcome", () => {
    assert.throws(
      () => assertOutcomeAgrees(heteroP1Wins, OutcomeKind.HeteroP2Wins),
      /cross-layer/
    );
  });
});

describe("sdk/battery verifyRealizedGame", () => {
  it("runs the full verdict and returns the derived outcome", () => {
    assert.equal(
      verifyRealizedGame(homogP1Only, OutcomeKind.HomogP1Correct),
      OutcomeKind.HomogP1Correct
    );
  });
});
