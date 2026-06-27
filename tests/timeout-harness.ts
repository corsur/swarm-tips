// Same-chain TIMEOUT coverage through the modular harness, under bankrun. The
// three terminal cells (full commit+reveal) are covered by game-harness.ts; this
// file covers the three TIMEOUT OutcomeKinds — TimeoutP1Wins, TimeoutP2Wins,
// TimeoutBothForfeit — which require advancing the SLOT clock past the on-chain
// deadline (COMMIT_TIMEOUT_SLOTS / REVEAL_TIMEOUT_SLOTS) and cranking the
// permissionless resolve_timeout. Each scenario PLAYS a partial game, warps past
// the deadline, resolves, and verifies against the SHARED oracle (deriveClaimOutcome
// on the realized transcript) + the protocol-take ledger — never a hardcoded outcome.
//
//   stalled state      played                         → oracle outcome    protocol take
//   Active             (no commits)                   → BothForfeit       2·stake
//   Committing         P1 commits only                → TimeoutP1Wins     0
//   Committing         P2 commits only                → TimeoutP2Wins     0
//   Revealing          both commit, P1 reveals only   → TimeoutP1Wins     0
//   Revealing          both commit, neither reveals   → BothForfeit       2·stake

import { BN } from "@coral-xyz/anchor";
import { Keypair, LAMPORTS_PER_SOL } from "@solana/web3.js";
import { assert } from "chai";
import { startBankrun, BankrunHandle } from "./harness/bankrun";
import { Ledger, Transcript } from "./harness/ledger";
import {
  GameCtx,
  GameMatch,
  setupGame,
  createTournament,
  startMatch,
  commit,
  reveal,
  resolveTimeout,
  generateCommit,
} from "./harness/game-steps";
import { OutcomeKind, deriveClaimOutcome } from "./helpers/outcome-oracle";

// Warp amounts: just past the on-chain deadlines (COMMIT=7200, REVEAL=14400 slots).
const COMMIT_WARP = 7_300;
const REVEAL_WARP = 14_500;

describe("coordination-game timeouts (bankrun, slot-warp)", () => {
  let handle: BankrunHandle;
  let ctx: GameCtx;

  const STAKE = new BN(50_000_000); // 0.05 SOL
  const STAKE_L = BigInt(STAKE.toString());
  const p1 = Keypair.generate();
  const p2 = Keypair.generate();
  const treasury = Keypair.generate().publicKey;

  before(async () => {
    handle = await startBankrun();
    await handle.runtime.fund(p1.publicKey, 50 * LAMPORTS_PER_SOL);
    await handle.runtime.fund(p2.publicKey, 50 * LAMPORTS_PER_SOL);
    ctx = await setupGame(handle.runtime, { treasury, splitBps: 5000 });
  });

  /** Open the protocol ledger (treasury + prize pool) just before resolution. */
  async function openProtocol(m: GameMatch): Promise<Ledger> {
    const led = new Ledger();
    led.open("treasury", "protocol", await ctx.rt.getBalance(treasury));
    led.open("prize", "protocol", await ctx.rt.getBalance(m.tournamentPda));
    return led;
  }

  async function closeProtocol(led: Ledger, m: GameMatch): Promise<bigint> {
    led.close("treasury", await ctx.rt.getBalance(treasury));
    led.close("prize", await ctx.rt.getBalance(m.tournamentPda));
    return led.netByKind("protocol");
  }

  /** Verify a resolved timeout: oracle outcome from the transcript + protocol take
   *  (2·stake on forfeit, 0 when a single player takes the pot) + winner payout. */
  async function verifyTimeout(
    m: GameMatch,
    transcript: Transcript,
    expected: OutcomeKind
  ): Promise<void> {
    const derived = deriveClaimOutcome(transcript.toOracleInput());
    assert.equal(
      derived,
      expected,
      `oracle: derived ${OutcomeKind[derived]} != expected ${OutcomeKind[expected]}`
    );

    const led = await openProtocol(m);
    const p1Before = await ctx.rt.getBalance(m.p1.publicKey);
    const p2Before = await ctx.rt.getBalance(m.p2.publicKey);

    await resolveTimeout(ctx, m);

    const protocolTake = await closeProtocol(led, m);
    const p1Delta = (await ctx.rt.getBalance(m.p1.publicKey)) - p1Before;
    const p2Delta = (await ctx.rt.getBalance(m.p2.publicKey)) - p2Before;

    if (expected === OutcomeKind.TimeoutBothForfeit) {
      // The whole pot (2·stake) is forfeited to the protocol (treasury + prize).
      assert.equal(
        protocolTake.toString(),
        (STAKE_L * 2n).toString(),
        "forfeit: protocol take != 2*stake"
      );
      assert.isAtMost(Number(p1Delta), 0, "forfeit: p1 should not be paid");
      assert.isAtMost(Number(p2Delta), 0, "forfeit: p2 should not be paid");
    } else {
      // A single player takes the whole pot; the protocol takes nothing.
      assert.equal(protocolTake.toString(), "0", "winner: protocol take != 0");
      const winnerDelta =
        expected === OutcomeKind.TimeoutP1Wins ? p1Delta : p2Delta;
      const loserDelta =
        expected === OutcomeKind.TimeoutP1Wins ? p2Delta : p1Delta;
      // Winner receives both stakes; loser receives nothing.
      assert.equal(
        winnerDelta.toString(),
        (STAKE_L * 2n).toString(),
        "winner payout != 2*stake"
      );
      assert.isAtMost(Number(loserDelta), 0, "loser should not be paid");
    }
  }

  let tid = 500;
  async function freshMatch(matchupType: 0 | 1 = 0): Promise<GameMatch> {
    const tournamentId = new BN(tid++);
    const tournamentPda = await createTournament(ctx, tournamentId);
    return startMatch(ctx, {
      tournamentId,
      tournamentPda,
      p1,
      p2,
      stake: STAKE,
      matchupType,
    });
  }

  it("Active (no commits) → both forfeit after commit deadline", async () => {
    const m = await freshMatch();
    const t = new Transcript(); // stepCount 0
    await ctx.rt.warpBySlots(COMMIT_WARP);
    await verifyTimeout(m, t, OutcomeKind.TimeoutBothForfeit);
  });

  it("Committing (P1 only) → P1 wins after commit deadline", async () => {
    const m = await freshMatch();
    const t = new Transcript();
    await commit(ctx, m, { playerNum: 1, commit: generateCommit(0) }, t);
    await ctx.rt.warpBySlots(COMMIT_WARP);
    await verifyTimeout(m, t, OutcomeKind.TimeoutP1Wins);
  });

  it("Committing (P2 only) → P2 wins after commit deadline", async () => {
    const m = await freshMatch();
    const t = new Transcript();
    await commit(ctx, m, { playerNum: 2, commit: generateCommit(0) }, t);
    await ctx.rt.warpBySlots(COMMIT_WARP);
    await verifyTimeout(m, t, OutcomeKind.TimeoutP2Wins);
  });

  it("Revealing (both commit, P1 reveals) → P1 wins after reveal deadline", async () => {
    const m = await freshMatch();
    const t = new Transcript();
    const c1 = generateCommit(0);
    const c2 = generateCommit(0);
    await commit(ctx, m, { playerNum: 1, commit: c1 }, t);
    await commit(ctx, m, { playerNum: 2, commit: c2 }, t);
    await reveal(
      ctx,
      m,
      { playerNum: 1, commit: c1, guess: 0, first: true },
      t
    );
    await ctx.rt.warpBySlots(REVEAL_WARP);
    await verifyTimeout(m, t, OutcomeKind.TimeoutP1Wins);
  });

  it("Revealing (both commit, neither reveals) → both forfeit after reveal deadline", async () => {
    const m = await freshMatch();
    const t = new Transcript();
    const c1 = generateCommit(0);
    const c2 = generateCommit(0);
    await commit(ctx, m, { playerNum: 1, commit: c1 }, t);
    await commit(ctx, m, { playerNum: 2, commit: c2 }, t);
    await ctx.rt.warpBySlots(REVEAL_WARP);
    await verifyTimeout(m, t, OutcomeKind.TimeoutBothForfeit);
  });
});
