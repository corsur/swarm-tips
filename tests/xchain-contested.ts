// Cross-chain CONTESTED-path coverage for the coordination game's Solana leg,
// run in-process under bankrun (no validator, no network, CI-safe — free).
//
// Migrated onto the modular harness (tests/harness/*). The on-chain coverage is
// unchanged — open_xclaim, supersede_xclaim, settle_xclaim, the equal-step reject
// — but the assertions now go through the shared PROPERTY BATTERY rather than
// hardcoded expectations:
//   - conservation:   whole-system lamports balance across the cross-chain settle
//                     (the pool FRONTS the tranche; value still nets to zero).
//   - oracle:         the chain's derived outcome == the TS oracle on the REALIZED
//                     checkpoint (cross-impl agreement: Rust derive vs outcome-oracle).
//   - legality:       Funded→Locked→Claiming→ClaimSettled transitions are legal;
//                     pre-window settle + equal-step supersede revert.
//   - monotonic:      supersede strictly increases best_step_count (inside the step).
//
//   lock_xtranche ─► open_xclaim(step-1 cp) ─► [warp past window] ─► settle_xclaim
//                                        └─► supersede_xclaim(step-4 cp) ─► settle
//
import { BN } from "@coral-xyz/anchor";
import { Keypair, LAMPORTS_PER_SOL } from "@solana/web3.js";
import { assert } from "chai";
import { createHash } from "crypto";
import { Checkpoint, newSessionSigner, keccak256 } from "./helpers/xchain-cert";
import { startBankrun, BankrunHandle } from "./harness/bankrun";
import { Ledger } from "./harness/ledger";
import { assertConservation, assertOracleOutcome } from "./harness/assertions";
import {
  SessionBundle,
  XchainCtx,
  LockedMatch,
  setupXchain,
  lockMatch,
  openXclaim,
  supersedeXclaim,
  settleXclaim,
  xmatchView,
  readXmatch,
  checkpointOracleInput,
} from "./harness/xchain-steps";

const enc = (s: string): Uint8Array => new TextEncoder().encode(s);
function sha256(bytes: Uint8Array): Uint8Array {
  return Uint8Array.from(
    createHash("sha256").update(Buffer.from(bytes)).digest()
  );
}

describe("coordination-game cross-chain contested (bankrun, harness)", () => {
  let handle: BankrunHandle;
  let ctx: XchainCtx;

  const TOURNAMENT_ID = new BN(7777);
  const STAKE = new BN(0.02 * LAMPORTS_PER_SOL);
  const TRANCHE = new BN(0.02 * LAMPORTS_PER_SOL);
  const CLAIM_WINDOW = 3600;
  const WIN_PAYOUT = BigInt(STAKE.add(TRANCHE).toString()); // local P1 win

  const player = Keypair.generate();
  const cranker = Keypair.generate(); // distinct, untracked gas-payer for settle
  const treasury = Keypair.generate().publicKey; // distinct from the fee-payer
  const bundle: SessionBundle = {
    operator: newSessionSigner(0x1111),
    local: newSessionSigner(0x2222),
    counter: newSessionSigner(0x3333),
  };

  before(async () => {
    handle = await startBankrun();
    await handle.runtime.fund(player.publicKey, 2 * LAMPORTS_PER_SOL);
    await handle.runtime.fund(cranker.publicKey, 1 * LAMPORTS_PER_SOL);
    ctx = await setupXchain(handle.runtime, {
      bundle,
      tournamentId: TOURNAMENT_ID,
      stake: STAKE,
      tranche: TRANCHE,
      claimWindow: CLAIM_WINDOW,
      treasury,
    });
  });

  // Snapshot every value-bearing account around settle so the battery can assert
  // whole-system conservation. Gas is paid by the untracked `cranker`.
  async function ledgerAround(m: LockedMatch): Promise<{
    open: () => Promise<Ledger>;
    close: (l: Ledger) => Promise<void>;
  }> {
    const rt = ctx.rt;
    const config = await rt.program.account.globalConfig.fetch(
      ctx.globalConfigPda
    );
    const accts: [string, "player" | "protocol", () => Promise<bigint>][] = [
      ["player", "player", () => rt.getBalance(player.publicKey)],
      ["xmatch", "protocol", () => rt.getBalance(m.xmatchPda)],
      ["pool", "protocol", () => rt.getBalance(ctx.poolPda)],
      ["treasury", "protocol", () => rt.getBalance(config.treasury)],
      ["tournament", "protocol", () => rt.getBalance(ctx.tournamentPda)],
    ];
    return {
      open: async () => {
        const l = new Ledger();
        for (const [name, kind, read] of accts)
          l.open(name, kind, await read());
        return l;
      },
      close: async (l: Ledger) => {
        for (const [name, , read] of accts) l.close(name, await read());
      },
    };
  }

  function step1Checkpoint(m: LockedMatch): Checkpoint {
    return {
      matchLiveDigest: m.liveDigest,
      stepCount: 1,
      p1Commit: keccak256(enc("p1c")),
      p2Commit: new Uint8Array(32),
      p1Guess: 255,
      p2Guess: 255,
      firstCommitter: 1, // only P1 committed → TimeoutP1Wins
      matchupType: 255,
      transcriptHash: keccak256(enc("t1")),
      rMatchup: new Uint8Array(32),
    };
  }

  it("open_xclaim (step-1) → settle: committer wins after the claim window", async () => {
    const m = await lockMatch(ctx, {
      seed: "contested-open-settle",
      matchupCommitment: keccak256(enc("commit-1")),
      player,
    });
    const cp = step1Checkpoint(m);
    await openXclaim(ctx, m, cp);

    const view = xmatchView(ctx, m.xmatchPda);
    assert.equal((await readXmatch(ctx, m.xmatchPda)).statusLabel, "Claiming");
    // battery (2): on-chain derived outcome == TS oracle on the realized checkpoint.
    await assertOracleOutcome(view, checkpointOracleInput(cp));

    // settle before the window must revert (state-machine guard).
    let rejected = false;
    try {
      await settleXclaim(ctx, m);
    } catch {
      rejected = true;
    }
    assert.isTrue(rejected, "settle before window-end must revert");

    await ctx.rt.warpTo(m.windowEnd + 1);
    const led = await ledgerAround(m);
    const l = await led.open();
    await settleXclaim(ctx, m, { cranker });
    await led.close(l);

    assertConservation(l); // battery (1)
    assert.equal(
      l.delta("player").toString(),
      WIN_PAYOUT.toString(),
      "win payout"
    );
    assert.equal(
      (await readXmatch(ctx, m.xmatchPda)).statusLabel,
      "ClaimSettled"
    );
    const profile = await ctx.rt.program.account.playerProfile.fetch(
      m.playerProfilePda
    );
    assert.equal(profile.wins.toString(), "1");
  });

  it("supersede_xclaim: a higher-step terminal checkpoint overrides the claim", async () => {
    // Terminal hetero transcript: P1 correct / P2 wrong → HeteroP1Wins. r_matchup
    // must hash to the cert's matchup_commitment and its low bit == matchup_type.
    const rMatchup = new Uint8Array(32);
    rMatchup[31] = 1; // matchup_type = 1 (different teams)
    const m = await lockMatch(ctx, {
      seed: "contested-supersede",
      matchupCommitment: sha256(rMatchup),
      player,
    });

    const openCp = step1Checkpoint(m);
    await openXclaim(ctx, m, openCp);

    // Equal-step supersede must revert — only strictly-higher steps override.
    let equalRejected = false;
    try {
      await supersedeXclaim(ctx, m, openCp);
    } catch {
      equalRejected = true;
    }
    assert.isTrue(equalRejected, "equal-step supersede must revert");

    const termCp: Checkpoint = {
      matchLiveDigest: m.liveDigest,
      stepCount: 4,
      p1Commit: keccak256(enc("p1c")),
      p2Commit: keccak256(enc("p2c")),
      p1Guess: 1, // correct (== matchup_type)
      p2Guess: 0, // wrong
      firstCommitter: 1,
      matchupType: 1,
      transcriptHash: keccak256(enc("t4")),
      rMatchup,
    };
    await supersedeXclaim(ctx, m, termCp); // asserts strict step increase 1→4

    const view = xmatchView(ctx, m.xmatchPda);
    assert.equal((await readXmatch(ctx, m.xmatchPda)).bestStepCount, 4);
    await assertOracleOutcome(view, checkpointOracleInput(termCp)); // HeteroP1Wins

    await ctx.rt.warpTo(m.windowEnd + 1);
    const led = await ledgerAround(m);
    const l = await led.open();
    await settleXclaim(ctx, m, { cranker });
    await led.close(l);

    assertConservation(l); // battery (1)
    assert.equal(
      l.delta("player").toString(),
      WIN_PAYOUT.toString(),
      "win payout"
    );
    assert.equal(
      (await readXmatch(ctx, m.xmatchPda)).statusLabel,
      "ClaimSettled"
    );
  });
});
