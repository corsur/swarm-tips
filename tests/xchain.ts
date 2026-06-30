import * as anchor from "@coral-xyz/anchor";
import { Program, BN } from "@coral-xyz/anchor";
import { CoordinationGame } from "../target/types/coordination_game";
import {
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  SystemProgram,
} from "@solana/web3.js";
import { readFileSync } from "fs";
import { assert } from "chai";
import {
  CertLeg,
  MatchLiveCert,
  Checkpoint,
  OutcomeCert,
  matchLiveDigest,
  checkpointDigest,
  outcomeDigest,
  keccak256,
  newSessionSigner,
  signDigest,
  toArray,
  SessionSigner,
  TERMINAL_STEP_COUNT,
} from "./helpers/xchain-cert";

// Outcome kinds (cross-chain wire format).
const HETERO_P1_WINS = 4;

const SOLANA_CHAIN_TAG = keccak256(
  new TextEncoder().encode("solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1")
);
const EVM_CHAIN_TAG = keccak256(new TextEncoder().encode("eip155:84532"));

function u64le(n: number | bigint): Buffer {
  const b = Buffer.alloc(8);
  b.writeBigUInt64LE(BigInt(n));
  return b;
}

describe("coordination-game cross-chain (xchain)", () => {
  anchor.setProvider(anchor.AnchorProvider.env());
  const provider = anchor.getProvider() as anchor.AnchorProvider;
  const program = anchor.workspace
    .CoordinationGame as Program<CoordinationGame>;

  const TOURNAMENT_ID = new BN(7777);
  const STAKE = new BN(0.02 * LAMPORTS_PER_SOL);
  const TRANCHE = new BN(0.02 * LAMPORTS_PER_SOL);

  const player = Keypair.generate();
  const operator = provider.wallet; // operator == matchmaker == authority in tests
  const operatorSigner = newSessionSigner(0x1111);
  const localSigner = newSessionSigner(0x2222); // player's session key
  const counterSigner = newSessionSigner(0x3333); // EVM-side session key

  let globalConfigPda: PublicKey;
  let tournamentPda: PublicKey;
  let poolPda: PublicKey;

  const pda = (seeds: (Buffer | Uint8Array)[]) =>
    PublicKey.findProgramAddressSync(seeds, program.programId)[0];

  // The local validator's clock lags wall-clock; base deadlines on it.
  async function chainNow(): Promise<number> {
    const slot = await provider.connection.getSlot();
    const t = await provider.connection.getBlockTime(slot);
    return t ?? Math.floor(Date.now() / 1000);
  }
  async function waitChainTime(target: number) {
    for (let i = 0; i < 60; i++) {
      if ((await chainNow()) > target) return;
      await new Promise((r) => setTimeout(r, 1000));
    }
    throw new Error("chain clock did not advance past target");
  }

  async function ensure(p: Promise<unknown>) {
    try {
      await p;
    } catch (e) {
      // Singleton already initialized by another test file — fine.
      if (!String(e).match(/already in use|custom program error: 0x0/)) throw e;
    }
  }

  before(async () => {
    const sig = await provider.connection.requestAirdrop(
      player.publicKey,
      2 * LAMPORTS_PER_SOL
    );
    await provider.connection.confirmTransaction(sig);

    globalConfigPda = pda([Buffer.from("global_config")]);
    tournamentPda = pda([Buffer.from("tournament"), u64le(7777)]);
    poolPda = pda([Buffer.from("xpool")]);

    // Idempotent setup (these may already exist from coordination-game.ts).
    await ensure(
      program.methods
        .initialize()
        .accountsPartial({
          gameCounter: pda([Buffer.from("game_counter")]),
          authority: provider.wallet.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .rpc()
    );
    await ensure(
      program.methods
        .initializeConfig(5000)
        .accountsPartial({
          globalConfig: globalConfigPda,
          authority: provider.wallet.publicKey,
          matchmaker: provider.wallet.publicKey,
          treasury: provider.wallet.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .rpc()
    );
    const now = Math.floor(Date.now() / 1000);
    await ensure(
      program.methods
        .createTournament(TOURNAMENT_ID, new BN(now - 60), new BN(now + 86400))
        .accountsPartial({
          tournament: tournamentPda,
          authority: provider.wallet.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .rpc()
    );
  });

  it("TS cert encoder agrees with the golden vectors (match-live + checkpoint + outcome)", () => {
    // Reconstruct the SAME sample certs chain-core writes to cert-vectors.json and
    // assert the TS encoder's keccak digest matches on all three — the 4th leg of
    // the Rust/BPF/Solidity/TS parity (the values mirror golden_vectors.rs: each
    // field distinct, aIsP1=0, p2Guess=255 UNREVEALED, leg tranche = 3x+offset).
    const golden = JSON.parse(
      readFileSync("tests/fixtures/cert-vectors.json", "utf8")
    );
    const fill = (n: number, len = 32) => new Uint8Array(len).fill(n);
    const expectDigest = (d: Uint8Array, g: string) =>
      assert.equal(Buffer.from(d).toString("hex"), g.replace("0x", ""));

    const leg = (s: number): CertLeg => ({
      chainTag: fill(s),
      contract: fill(s + 1),
      player: fill(s + 2),
      sessionKey: fill(s + 3, 20),
      stake: BigInt(s) * 1_000_000n + 11n,
      tranche: BigInt(s) * 3_000_000n + 22n,
    });
    const cert: MatchLiveCert = {
      matchId: fill(0xaa),
      tournamentId: 7n,
      matchupCommitment: fill(0xbb),
      legA: leg(0x10),
      legB: leg(0x20),
      quoteTimestamp: 1_765_000_000n,
      quoteMaxAgeSecs: 300,
      matchDeadline: 1_765_000_900n,
      claimWindowSecs: 3600,
      aIsP1: 0,
    };
    const mlDigest = matchLiveDigest(cert);
    expectDigest(mlDigest, golden.match_live.digest);

    const checkpoint: Checkpoint = {
      matchLiveDigest: mlDigest,
      stepCount: 3,
      p1Commit: fill(0xc1),
      p2Commit: fill(0xc2),
      p1Guess: 1,
      p2Guess: 255,
      firstCommitter: 2,
      matchupType: 0,
      transcriptHash: fill(0xd0),
      rMatchup: fill(0xe1),
    };
    expectDigest(checkpointDigest(checkpoint), golden.checkpoint.digest);

    const outcome: OutcomeCert = {
      matchId: fill(0xaa),
      matchLiveDigest: mlDigest,
      outcomeKind: 5, // HETERO_P2_WINS
      stepCount: 4,
      p1Guess: 1,
      p2Guess: 255,
      firstCommitter: 2,
      matchupType: 0,
      transcriptHash: fill(0xd0),
    };
    expectDigest(outcomeDigest(outcome), golden.outcome.digest);
  });

  it("initializes the payout pool and funds it", async () => {
    await program.methods
      .initializeXpool(
        operator.publicKey,
        toArray(operatorSigner.address),
        TRANCHE.muln(4), // max_tranche (u64 → BN)
        3600, // max_claim_window (u32 → number)
        900 // skew_margin (u32 → number)
      )
      .accountsPartial({
        pool: poolPda,
        globalConfig: globalConfigPda,
        authority: operator.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    await program.methods
      .xpoolDeposit(TRANCHE.muln(4))
      .accountsPartial({
        pool: poolPda,
        funder: operator.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    const pool = await program.account.xPayoutPool.fetch(poolPda);
    assert.equal(pool.freeLamports.toString(), TRANCHE.muln(4).toString());
  });

  it("runs a full cross-chain match: create → lock → settle (winner takes stake + tranche)", async () => {
    const matchId = Array.from(keccak256(new TextEncoder().encode("match-1")));
    const matchIdBuf = Buffer.from(matchId);
    const xmatchPda = pda([Buffer.from("xmatch"), matchIdBuf]);
    const playerProfilePda = pda([
      Buffer.from("player"),
      u64le(7777),
      player.publicKey.toBuffer(),
    ]);

    const now = Math.floor(Date.now() / 1000);
    const fundDeadline = new BN(now + 3600);
    const matchDeadline = new BN(now + 7200);

    // create_xmatch — player + matchmaker co-sign.
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
        matchmaker: provider.wallet.publicKey,
        player: player.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([player])
      .rpc();

    // Build the match-live certificate. Leg A is ALWAYS the Solana leg.
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
      matchupCommitment: keccak256(new TextEncoder().encode("commit")),
      legA,
      legB,
      quoteTimestamp: BigInt(now),
      quoteMaxAgeSecs: 600,
      matchDeadline: BigInt(matchDeadline.toString()),
      claimWindowSecs: 3600,
      aIsP1: 1,
    };
    const liveDigest = matchLiveDigest(cert);
    const liveSigs = [
      signDigest(localSigner, liveDigest),
      signDigest(counterSigner, liveDigest),
      signDigest(operatorSigner, liveDigest),
    ];

    // lock_xtranche — permissionless: the operator's match-live signature
    // (liveSigs[2]) authorizes; any cranker pays the fee. Must land before settle.
    await program.methods
      .lockXtranche(toCertArg(cert), liveSigs[2])
      .accountsPartial({
        xmatch: xmatchPda,
        pool: poolPda,
        cranker: provider.wallet.publicKey,
      })
      .rpc();

    // Terminal outcome: P1 (the local player) wins the heterogeneous match.
    const outcome: OutcomeCert = {
      matchId: Uint8Array.from(matchId),
      matchLiveDigest: liveDigest,
      outcomeKind: HETERO_P1_WINS,
      stepCount: TERMINAL_STEP_COUNT,
      p1Guess: 1,
      p2Guess: 0,
      firstCommitter: 1,
      matchupType: 1,
      transcriptHash: keccak256(new TextEncoder().encode("transcript")),
    };
    const ocDigest = outcomeDigest(outcome);
    const ocSigs = [
      signDigest(localSigner, ocDigest),
      signDigest(counterSigner, ocDigest),
      signDigest(operatorSigner, ocDigest),
    ];

    const certNoAArg = toCertNoAArg(cert);
    const outcomeArg = toOutcomeArg(outcome);

    // The singleton global_config may have been initialized by another test
    // file; settle's treasury must equal whatever it actually holds.
    const config = await program.account.globalConfig.fetch(globalConfigPda);

    const before = await provider.connection.getBalance(player.publicKey);
    await program.methods
      .settleXmatch(certNoAArg, outcomeArg, liveSigs, ocSigs)
      .accountsPartial({
        xmatch: xmatchPda,
        pool: poolPda,
        tournament: tournamentPda,
        playerProfile: playerProfilePda,
        globalConfig: globalConfigPda,
        treasury: config.treasury,
        player: player.publicKey,
        cranker: operator.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    const after = await provider.connection.getBalance(player.publicKey);
    // Winner receives stake + tranche.
    assert.equal(after - before, STAKE.add(TRANCHE).toNumber());

    const m = await program.account.xChainMatch.fetch(xmatchPda);
    assert.deepEqual(m.status, { settled: {} });
    const profile = await program.account.playerProfile.fetch(playerProfilePda);
    assert.equal(profile.wins.toString(), "1");
  });

  it("refunds a funded-but-unlocked match after the fund deadline", async () => {
    const matchId = Array.from(keccak256(new TextEncoder().encode("match-2")));
    const xmatchPda = pda([Buffer.from("xmatch"), Buffer.from(matchId)]);
    const cnow = await chainNow();
    const fundDeadline = cnow + 2;

    await program.methods
      .createXmatch(matchId, {
        tournamentId: TOURNAMENT_ID,
        playerIsP1: true,
        sessionKey: toArray(localSigner.address),
        counterSessionKey: toArray(counterSigner.address),
        stakeLamports: STAKE,
        fundDeadline: new BN(fundDeadline), // 2s out (chain time)
        matchDeadline: new BN(cnow + 7200),
      })
      .accountsPartial({
        xmatch: xmatchPda,
        globalConfig: globalConfigPda,
        matchmaker: provider.wallet.publicKey,
        player: player.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([player])
      .rpc();

    await waitChainTime(fundDeadline);

    const before = await provider.connection.getBalance(player.publicKey);
    await program.methods
      .refundXmatchNocert()
      .accountsPartial({ xmatch: xmatchPda, player: player.publicKey })
      .rpc();
    const after = await provider.connection.getBalance(player.publicKey);
    assert.equal(after - before, STAKE.toNumber());
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
  // settle_xmatch takes the cert WITHOUT leg A (reconstructed on-chain).
  function toCertNoAArg(c: MatchLiveCert) {
    return {
      matchId: toArray(c.matchId),
      tournamentId: new BN(c.tournamentId.toString()),
      matchupCommitment: toArray(c.matchupCommitment),
      legB: toLegArg(c.legB),
      quoteTimestamp: new BN(c.quoteTimestamp.toString()),
      quoteMaxAgeSecs: c.quoteMaxAgeSecs,
      matchDeadline: new BN(c.matchDeadline.toString()),
      claimWindowSecs: c.claimWindowSecs,
      aIsP1: c.aIsP1,
    };
  }
  // lock_xtranche takes the FULL cert (leg A's tranche isn't on-chain pre-lock).
  function toCertArg(c: MatchLiveCert) {
    return { ...toCertNoAArg(c), legA: toLegArg(c.legA) };
  }
  function toOutcomeArg(o: OutcomeCert) {
    return {
      matchId: toArray(o.matchId),
      matchLiveDigest: toArray(o.matchLiveDigest),
      outcomeKind: o.outcomeKind,
      stepCount: o.stepCount,
      p1Guess: o.p1Guess,
      p2Guess: o.p2Guess,
      firstCommitter: o.firstCommitter,
      matchupType: o.matchupType,
      transcriptHash: toArray(o.transcriptHash),
    };
  }
});
