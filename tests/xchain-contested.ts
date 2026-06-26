// Cross-chain CONTESTED-path coverage for the coordination game's Solana leg,
// run in-process under bankrun (no validator, no network, CI-safe — free).
//
// The happy path (create → lock → settle) and the refund backstop are covered
// by tests/xchain.ts (validator). The optimistic-claim path — open_xclaim,
// supersede_xclaim, settle_xclaim — was exercisable ONLY against a live
// validator and only on the EVM side (CrossChainGame.t.sol). This closes that
// gap on Solana: the same 12-word Checkpoint flow the cross-chain protocol
// uses, asserted against the SHARED outcome oracle (tests/helpers/
// outcome-oracle.ts) so on-chain reality is checked against ONE expectation.
//
// Bankrun lets us warp the clock past `claim_window_end` deterministically,
// which a real clock can't do in a unit test. Pattern mirrors
// tests/shillbot-lifecycle.ts (startAnchor + BankrunProvider + Clock warp).
//
//   lock_xtranche ─► open_xclaim(step-1 cp) ─► [warp past window] ─► settle_xclaim
//                                        └─► supersede_xclaim(step-4 cp) ─► settle
//
import { startAnchor, BankrunProvider } from "anchor-bankrun";
import { BN, Program } from "@coral-xyz/anchor";
import {
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  SystemProgram,
  Transaction,
} from "@solana/web3.js";
import { Clock } from "solana-bankrun";
import { assert } from "chai";
import { createHash } from "crypto";
import { CoordinationGame } from "../target/types/coordination_game";
import {
  CertLeg,
  MatchLiveCert,
  Checkpoint,
  matchLiveDigest,
  checkpointDigest,
  keccak256,
  newSessionSigner,
  signDigest,
  toArray,
} from "./helpers/xchain-cert";
import {
  deriveClaimOutcome,
  deriveTerminalOutcome,
} from "./helpers/outcome-oracle";

const IDL = require("../target/idl/coordination_game.json");

const SOLANA_CHAIN_TAG = keccak256(
  new TextEncoder().encode("solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1")
);
const EVM_CHAIN_TAG = keccak256(new TextEncoder().encode("eip155:84532"));

function u64le(n: number | bigint): Buffer {
  const b = Buffer.alloc(8);
  b.writeBigUInt64LE(BigInt(n));
  return b;
}

function sha256(bytes: Uint8Array): Uint8Array {
  return Uint8Array.from(
    createHash("sha256").update(Buffer.from(bytes)).digest()
  );
}

describe("coordination-game cross-chain contested (bankrun)", () => {
  let context: Awaited<ReturnType<typeof startAnchor>>;
  let provider: BankrunProvider;
  let program: Program<CoordinationGame>;

  const TOURNAMENT_ID = new BN(7777);
  const STAKE = new BN(0.02 * LAMPORTS_PER_SOL);
  const TRANCHE = new BN(0.02 * LAMPORTS_PER_SOL);
  const CLAIM_WINDOW = 3600;

  const player = Keypair.generate();
  const cranker2 = Keypair.generate(); // distinct fee-payer for a second settle
  const operatorSigner = newSessionSigner(0x1111);
  const localSigner = newSessionSigner(0x2222); // Solana-leg session key
  const counterSigner = newSessionSigner(0x3333); // EVM-leg session key

  let globalConfigPda: PublicKey;
  let tournamentPda: PublicKey;
  let poolPda: PublicKey;
  let operatorWallet: PublicKey;

  const pda = (seeds: (Buffer | Uint8Array)[]) =>
    PublicKey.findProgramAddressSync(seeds, program.programId)[0];

  async function chainNow(): Promise<number> {
    const clock = await context.banksClient.getClock();
    return Number(clock.unixTimestamp);
  }
  async function warpTo(targetTimestamp: number): Promise<void> {
    const c = await context.banksClient.getClock();
    context.setClock(
      new Clock(
        c.slot,
        c.epochStartTimestamp,
        c.epoch,
        c.leaderScheduleEpoch,
        BigInt(targetTimestamp)
      )
    );
  }
  async function getBalance(pubkey: PublicKey): Promise<number> {
    const acct = await context.banksClient.getAccount(pubkey);
    return acct === null ? 0 : Number(acct.lamports);
  }
  async function fund(recipient: PublicKey, lamports: number): Promise<void> {
    const tx = new Transaction().add(
      SystemProgram.transfer({
        fromPubkey: provider.wallet.publicKey,
        toPubkey: recipient,
        lamports,
      })
    );
    await provider.sendAndConfirm(tx);
  }

  before(async () => {
    context = await startAnchor(".", [], []);
    provider = new BankrunProvider(context);
    program = new Program<CoordinationGame>(IDL, provider);
    operatorWallet = provider.wallet.publicKey;

    await fund(player.publicKey, 2 * LAMPORTS_PER_SOL);
    await fund(cranker2.publicKey, 1 * LAMPORTS_PER_SOL);

    globalConfigPda = pda([Buffer.from("global_config")]);
    tournamentPda = pda([Buffer.from("tournament"), u64le(7777)]);
    poolPda = pda([Buffer.from("xpool")]);

    await program.methods
      .initialize()
      .accountsPartial({
        gameCounter: pda([Buffer.from("game_counter")]),
        authority: operatorWallet,
        systemProgram: SystemProgram.programId,
      })
      .rpc();
    await program.methods
      .initializeConfig(5000)
      .accountsPartial({
        globalConfig: globalConfigPda,
        authority: operatorWallet,
        matchmaker: operatorWallet,
        treasury: operatorWallet,
        systemProgram: SystemProgram.programId,
      })
      .rpc();
    const now = await chainNow();
    await program.methods
      .createTournament(TOURNAMENT_ID, new BN(now - 60), new BN(now + 86400))
      .accountsPartial({
        tournament: tournamentPda,
        authority: operatorWallet,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    await program.methods
      .initializeXpool(
        operatorWallet,
        toArray(operatorSigner.address),
        TRANCHE.muln(8), // max_tranche
        CLAIM_WINDOW, // max_claim_window
        900 // skew_margin
      )
      .accountsPartial({
        pool: poolPda,
        globalConfig: globalConfigPda,
        authority: operatorWallet,
        systemProgram: SystemProgram.programId,
      })
      .rpc();
    await program.methods
      .xpoolDeposit(TRANCHE.muln(8))
      .accountsPartial({
        pool: poolPda,
        funder: operatorWallet,
        systemProgram: SystemProgram.programId,
      })
      .rpc();
  });

  // --- arg marshalling: Uint8Array → number[] for the Anchor client ---
  function toLegArg(l: CertLeg) {
    return {
      chainTag: toArray(l.chainTag),
      contract: toArray(l.contract),
      player: toArray(l.player),
      sessionKey: toArray(l.sessionKey),
      stake: new BN(l.stake.toString()),
      tranche: new BN(l.tranche.toString()),
    };
  }
  function toCertArg(c: MatchLiveCert) {
    return {
      matchId: toArray(c.matchId),
      tournamentId: new BN(c.tournamentId.toString()),
      matchupCommitment: toArray(c.matchupCommitment),
      legA: toLegArg(c.legA),
      legB: toLegArg(c.legB),
      quoteTimestamp: new BN(c.quoteTimestamp.toString()),
      quoteMaxAgeSecs: c.quoteMaxAgeSecs,
      matchDeadline: new BN(c.matchDeadline.toString()),
      claimWindowSecs: c.claimWindowSecs,
      aIsP1: c.aIsP1,
    };
  }
  function toCheckpointArg(cp: Checkpoint) {
    return {
      matchLiveDigest: toArray(cp.matchLiveDigest),
      stepCount: cp.stepCount,
      p1Commit: toArray(cp.p1Commit),
      p2Commit: toArray(cp.p2Commit),
      p1Guess: cp.p1Guess,
      p2Guess: cp.p2Guess,
      firstCommitter: cp.firstCommitter,
      matchupType: cp.matchupType,
      transcriptHash: toArray(cp.transcriptHash),
      rMatchup: toArray(cp.rMatchup),
    };
  }

  // Drive create_xmatch → lock_xtranche; returns the locked match + its cert.
  async function lockedMatch(seed: string, matchupCommitment: Uint8Array) {
    const matchId = Array.from(keccak256(new TextEncoder().encode(seed)));
    const matchIdBuf = Buffer.from(matchId);
    const xmatchPda = pda([Buffer.from("xmatch"), matchIdBuf]);
    const playerProfilePda = pda([
      Buffer.from("player"),
      u64le(7777),
      player.publicKey.toBuffer(),
    ]);

    const now = await chainNow();
    const fundDeadline = new BN(now + 3600);
    const matchDeadline = new BN(now + 7200);

    await program.methods
      .createXmatch(matchId, {
        tournamentId: TOURNAMENT_ID,
        playerIsP1: true,
        sessionKey: toArray(localSigner.address),
        counterSessionKey: toArray(counterSigner.address),
        stakeLamports: STAKE,
        fundDeadline,
        matchDeadline,
      })
      .accountsPartial({
        xmatch: xmatchPda,
        globalConfig: globalConfigPda,
        matchmaker: operatorWallet,
        player: player.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([player])
      .rpc();

    const legA: CertLeg = {
      chainTag: SOLANA_CHAIN_TAG,
      contract: program.programId.toBytes(),
      player: player.publicKey.toBytes(),
      sessionKey: localSigner.address,
      stake: BigInt(STAKE.toString()),
      tranche: BigInt(TRANCHE.toString()),
    };
    const legB: CertLeg = {
      chainTag: EVM_CHAIN_TAG,
      contract: new Uint8Array(32),
      player: new Uint8Array(32),
      sessionKey: counterSigner.address,
      stake: BigInt(STAKE.toString()),
      tranche: BigInt(TRANCHE.toString()),
    };
    const cert: MatchLiveCert = {
      matchId: Uint8Array.from(matchId),
      tournamentId: BigInt(TOURNAMENT_ID.toString()),
      matchupCommitment,
      legA,
      legB,
      quoteTimestamp: BigInt(now),
      quoteMaxAgeSecs: 600,
      matchDeadline: BigInt(matchDeadline.toString()),
      claimWindowSecs: CLAIM_WINDOW,
      aIsP1: 1,
    };
    const liveDigest = matchLiveDigest(cert);
    const liveSigs = [
      signDigest(localSigner, liveDigest),
      signDigest(counterSigner, liveDigest),
      signDigest(operatorSigner, liveDigest),
    ];

    await program.methods
      .lockXtranche(toCertArg(cert), liveSigs[2])
      .accountsPartial({
        xmatch: xmatchPda,
        pool: poolPda,
        cranker: operatorWallet,
      })
      .rpc();

    const windowEnd = Number(matchDeadline) + CLAIM_WINDOW;
    return {
      cert,
      liveDigest,
      liveSigs,
      xmatchPda,
      playerProfilePda,
      windowEnd,
    };
  }

  function cpSigsFor(cp: Checkpoint) {
    const d = checkpointDigest(cp);
    return [signDigest(localSigner, d), signDigest(counterSigner, d)];
  }

  // settle is permissionless. `cranker` lets a caller vary the fee-payer so two
  // settle attempts on the same match are distinct transactions — bankrun
  // dedupes by signature, so a rejected pre-window attempt and the real one
  // must not be byte-identical.
  async function settle(
    m: { xmatchPda: PublicKey; playerProfilePda: PublicKey },
    cranker?: Keypair
  ) {
    const config = await program.account.globalConfig.fetch(globalConfigPda);
    const b = program.methods.settleXclaim().accountsPartial({
      xmatch: m.xmatchPda,
      pool: poolPda,
      tournament: tournamentPda,
      playerProfile: m.playerProfilePda,
      globalConfig: globalConfigPda,
      treasury: config.treasury,
      player: player.publicKey,
      cranker: cranker ? cranker.publicKey : operatorWallet,
      systemProgram: SystemProgram.programId,
    });
    await (cranker ? b.signers([cranker]).rpc() : b.rpc());
  }

  it("open_xclaim (step-1) → settle: committer wins after the claim window", async () => {
    // Only one commit landed (P1, the local player). The committer wins the
    // pot under timeout semantics; the oracle calls this TimeoutP1Wins.
    const m = await lockedMatch(
      "contested-open-settle",
      keccak256(new TextEncoder().encode("commit-1"))
    );
    const cp: Checkpoint = {
      matchLiveDigest: m.liveDigest,
      stepCount: 1,
      p1Commit: keccak256(new TextEncoder().encode("p1c")),
      p2Commit: new Uint8Array(32),
      p1Guess: 255,
      p2Guess: 255,
      firstCommitter: 1,
      matchupType: 255,
      transcriptHash: keccak256(new TextEncoder().encode("t1")),
      rMatchup: new Uint8Array(32),
    };
    const expectedKind = deriveClaimOutcome({
      stepCount: 1,
      matchupType: 255,
      p1Guess: 255,
      p2Guess: 255,
      firstCommitter: 1,
    });

    await program.methods
      .openXclaim(
        toCertArg(m.cert),
        toCheckpointArg(cp),
        m.liveSigs,
        cpSigsFor(cp)
      )
      .accountsPartial({ xmatch: m.xmatchPda, pool: poolPda })
      .rpc();

    let xm = await program.account.xChainMatch.fetch(m.xmatchPda);
    assert.deepEqual(xm.status, { claiming: {} });
    assert.equal(xm.bestStepCount, 1);
    assert.equal(xm.bestOutcomeKind, expectedKind); // TimeoutP1Wins (6)

    // settle is rejected until the claim window has elapsed.
    let rejected = false;
    try {
      await settle(m);
    } catch {
      rejected = true;
    }
    assert.isTrue(rejected, "settle before window-end must revert");

    await warpTo(m.windowEnd + 1);
    const before = await getBalance(player.publicKey);
    await settle(m, cranker2);
    const after = await getBalance(player.publicKey);

    // Local player is P1 and wins → receives stake + tranche.
    assert.equal(after - before, STAKE.add(TRANCHE).toNumber());
    xm = await program.account.xChainMatch.fetch(m.xmatchPda);
    assert.deepEqual(xm.status, { claimSettled: {} });
    const profile = await program.account.playerProfile.fetch(
      m.playerProfilePda
    );
    assert.equal(profile.wins.toString(), "1");
  });

  it("supersede_xclaim: a higher-step terminal checkpoint overrides the claim", async () => {
    // Terminal transcript: hetero (matchup_type 1), P1 correct / P2 wrong →
    // HeteroP1Wins. r_matchup must hash to the cert's matchup_commitment and
    // its low bit must equal matchup_type (the on-chain binding).
    const rMatchup = new Uint8Array(32);
    rMatchup[31] = 1; // matchup_type = 1 (different teams)
    const matchupCommitment = sha256(rMatchup);
    const m = await lockedMatch("contested-supersede", matchupCommitment);

    // Open with a step-1 claim first (committer-wins placeholder verdict).
    const openCp: Checkpoint = {
      matchLiveDigest: m.liveDigest,
      stepCount: 1,
      p1Commit: keccak256(new TextEncoder().encode("p1c")),
      p2Commit: new Uint8Array(32),
      p1Guess: 255,
      p2Guess: 255,
      firstCommitter: 1,
      matchupType: 255,
      transcriptHash: keccak256(new TextEncoder().encode("t1")),
      rMatchup: new Uint8Array(32),
    };
    await program.methods
      .openXclaim(
        toCertArg(m.cert),
        toCheckpointArg(openCp),
        m.liveSigs,
        cpSigsFor(openCp)
      )
      .accountsPartial({ xmatch: m.xmatchPda, pool: poolPda })
      .rpc();

    // A step-1 (equal-step) supersede must be rejected — only strictly higher
    // step counts override.
    let equalRejected = false;
    try {
      await program.methods
        .supersedeXclaim(
          toCertArg(m.cert),
          toCheckpointArg(openCp),
          cpSigsFor(openCp)
        )
        .accountsPartial({ xmatch: m.xmatchPda })
        .rpc();
    } catch {
      equalRejected = true;
    }
    assert.isTrue(equalRejected, "equal-step supersede must revert");

    const termCp: Checkpoint = {
      matchLiveDigest: m.liveDigest,
      stepCount: 4,
      p1Commit: keccak256(new TextEncoder().encode("p1c")),
      p2Commit: keccak256(new TextEncoder().encode("p2c")),
      p1Guess: 1, // correct (== matchup_type)
      p2Guess: 0, // wrong
      firstCommitter: 1,
      matchupType: 1,
      transcriptHash: keccak256(new TextEncoder().encode("t4")),
      rMatchup,
    };
    const expectedKind = deriveTerminalOutcome({
      stepCount: 4,
      matchupType: 1,
      p1Guess: 1,
      p2Guess: 0,
      firstCommitter: 1,
    });

    await program.methods
      .supersedeXclaim(
        toCertArg(m.cert),
        toCheckpointArg(termCp),
        cpSigsFor(termCp)
      )
      .accountsPartial({ xmatch: m.xmatchPda })
      .rpc();

    let xm = await program.account.xChainMatch.fetch(m.xmatchPda);
    assert.equal(xm.bestStepCount, 4);
    assert.equal(xm.bestOutcomeKind, expectedKind); // HeteroP1Wins (4)

    await warpTo(m.windowEnd + 1);
    const before = await getBalance(player.publicKey);
    await settle(m);
    const after = await getBalance(player.publicKey);

    // Local player (P1) wins the terminal match → stake + tranche.
    assert.equal(after - before, STAKE.add(TRANCHE).toNumber());
    xm = await program.account.xChainMatch.fetch(m.xmatchPda);
    assert.deepEqual(xm.status, { claimSettled: {} });
  });
});
