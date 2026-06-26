// Same-chain coordination-game coverage through the modular harness, run under
// bankrun (no validator, CI-safe). Proves game-steps.ts + the property battery on
// the single-chain path: a representative set of payoff cells is PLAYED through
// the composable steps, then verified against the SHARED oracle — not hardcoded:
//   - payoff:   realized protocol take (treasury + prize) == oracle tournamentGain
//               derived from the REALIZED transcript (fee-free; protocol PDAs
//               don't sign, so no gas to net out).
//   - oracle:   the on-chain resolved (guesses, matchup, first-committer) decodes
//               to the same OutcomeKind the oracle derives from what we played.
//   - legality: Pending→Active→Committing→Revealing→Resolved (asserted in steps).
//
// The same game-steps run on a validatorRuntime too (follow-up) — same scenario,
// two runtimes = the battery's metamorphic check.

import { BN } from "@coral-xyz/anchor";
import { Keypair, LAMPORTS_PER_SOL } from "@solana/web3.js";
import { startBankrun, BankrunHandle } from "./harness/bankrun";
import { Ledger, Transcript } from "./harness/ledger";
import {
  assertPayoffMatchesLedger,
  assertOracleOutcome,
} from "./harness/assertions";
import {
  GameCtx,
  setupGame,
  createTournament,
  startMatch,
  commit,
  reveal,
  gameView,
  generateCommit,
} from "./harness/game-steps";
import { OutcomeKind } from "./helpers/outcome-oracle";

interface Cell {
  name: string;
  matchupType: 0 | 1;
  p1Guess: 0 | 1;
  p2Guess: 0 | 1;
  commitFirst: 1 | 2;
  expected: OutcomeKind;
}

// Representative payoff cells: tournamentGain 0 / partial / full forfeit, plus a
// hetero tie-break that turns on commit order.
const CELLS: Cell[] = [
  {
    name: "homog both correct (gain 0)",
    matchupType: 0,
    p1Guess: 0,
    p2Guess: 0,
    commitFirst: 1,
    expected: OutcomeKind.HomogBothCorrect,
  },
  {
    name: "homog P1 only (gain 1.5·stake)",
    matchupType: 0,
    p1Guess: 0,
    p2Guess: 1,
    commitFirst: 1,
    expected: OutcomeKind.HomogP1Correct,
  },
  {
    name: "homog both wrong (gain 2·stake)",
    matchupType: 0,
    p1Guess: 1,
    p2Guess: 1,
    commitFirst: 1,
    expected: OutcomeKind.BothWrong,
  },
  {
    name: "hetero P1 wins (gain 0)",
    matchupType: 1,
    p1Guess: 1,
    p2Guess: 0,
    commitFirst: 1,
    expected: OutcomeKind.HeteroP1Wins,
  },
  {
    name: "hetero both correct → first committer P2 wins",
    matchupType: 1,
    p1Guess: 1,
    p2Guess: 1,
    commitFirst: 2,
    expected: OutcomeKind.HeteroP2Wins,
  },
];

describe("coordination-game same-chain (bankrun, harness)", () => {
  let handle: BankrunHandle;
  let ctx: GameCtx;

  const STAKE = new BN(50_000_000); // 0.05 SOL
  const STAKE_L = BigInt(STAKE.toString());
  const p1 = Keypair.generate();
  const p2 = Keypair.generate();
  const treasury = Keypair.generate().publicKey; // distinct from the fee-payer

  before(async () => {
    handle = await startBankrun();
    await handle.runtime.fund(p1.publicKey, 5 * LAMPORTS_PER_SOL);
    await handle.runtime.fund(p2.publicKey, 5 * LAMPORTS_PER_SOL);
    ctx = await setupGame(handle.runtime, { treasury, splitBps: 5000 });
  });

  CELLS.forEach((cell, i) => {
    it(`${cell.name} resolves to the oracle outcome`, async () => {
      const tournamentId = new BN(100 + i);
      const tournamentPda = await createTournament(ctx, tournamentId);

      // Track the protocol take (treasury + prize pool) across the match.
      const led = new Ledger();
      led.open("treasury", "protocol", await ctx.rt.getBalance(treasury));
      led.open("prize", "protocol", await ctx.rt.getBalance(tournamentPda));

      const m = await startMatch(ctx, {
        tournamentId,
        tournamentPda,
        p1,
        p2,
        stake: STAKE,
        matchupType: cell.matchupType,
      });

      const transcript = new Transcript();
      const c1 = generateCommit(cell.p1Guess);
      const c2 = generateCommit(cell.p2Guess);

      // first_committer is set by COMMIT order.
      const commitOrder: (1 | 2)[] = cell.commitFirst === 1 ? [1, 2] : [2, 1];
      for (const n of commitOrder) {
        await commit(
          ctx,
          m,
          { playerNum: n, commit: n === 1 ? c1 : c2 },
          transcript
        );
      }

      // P1 reveals first (supplies r_matchup), then P2.
      await reveal(
        ctx,
        m,
        { playerNum: 1, commit: c1, guess: cell.p1Guess, first: true },
        transcript
      );
      await reveal(
        ctx,
        m,
        { playerNum: 2, commit: c2, guess: cell.p2Guess, first: false },
        transcript
      );

      led.close("treasury", await ctx.rt.getBalance(treasury));
      led.close("prize", await ctx.rt.getBalance(tournamentPda));

      const input = transcript.toOracleInput();
      // (2) the chain's recorded outcome matches the oracle on what we played.
      const got = await assertOracleOutcome(gameView(ctx, m.gamePda), input);
      if (got !== cell.expected) {
        throw new Error(
          `cell expectation drift: ${OutcomeKind[got]} != ${
            OutcomeKind[cell.expected]
          }`
        );
      }
      // (1) protocol take == oracle tournamentGain for the realized transcript.
      assertPayoffMatchesLedger(led, { ...input, stake: STAKE_L });
    });
  });
});
