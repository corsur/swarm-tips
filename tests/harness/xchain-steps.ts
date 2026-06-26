// Composable cross-chain (xchain) steps for the harness. Each step performs ONE
// on-chain action through a SolanaRuntime and asserts its local state-machine
// invariant; scenarios compose them and then run the property battery
// (assertions.ts) over the realized Ledger + Transcript. Extracted from the
// inlined logic that tests/xchain.ts and tests/xchain-contested.ts duplicated
// (cert build+sign, PDA derivation, status reads, claim-window math).

import { BN } from "@coral-xyz/anchor";
import { Keypair, PublicKey, SystemProgram } from "@solana/web3.js";
import {
  CertLeg,
  MatchLiveCert,
  Checkpoint,
  SessionSigner,
  matchLiveDigest,
  checkpointDigest,
  keccak256,
  signDigest,
  toArray,
} from "../helpers/xchain-cert";
import { Transcript as OracleInput } from "../helpers/outcome-oracle";
import { SolanaRuntime } from "./target";
import {
  StateView,
  TransitionGraph,
  assertLegalTransition,
} from "./assertions";

export const SOLANA_CHAIN_TAG = keccak256(
  new TextEncoder().encode("solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1")
);
export const EVM_CHAIN_TAG = keccak256(
  new TextEncoder().encode("eip155:84532")
);

/** Allowed cross-chain match status transitions. Terminal statuses map to []. */
export const XCHAIN_GRAPH: TransitionGraph = {
  Funded: ["Locked", "Refunded"],
  Locked: ["Claiming", "Settled"],
  Claiming: ["Claiming", "ClaimSettled"], // self-loop = supersede
  ClaimSettled: [],
  Settled: [],
  Refunded: [],
};

/** The three secp256k1 session keys a cross-chain match is co-signed by. */
export interface SessionBundle {
  operator: SessionSigner;
  local: SessionSigner; // leg A (Solana) player session key
  counter: SessionSigner; // leg B (EVM) player session key
}

function u64le(n: number | bigint): Buffer {
  const b = Buffer.alloc(8);
  b.writeBigUInt64LE(BigInt(n));
  return b;
}

/** Anchor enum object ({ claimSettled: {} }) -> canonical label ("ClaimSettled"). */
export function xchainStatusLabel(status: Record<string, unknown>): string {
  const key = Object.keys(status)[0];
  return key.charAt(0).toUpperCase() + key.slice(1);
}

// --- arg marshalling: Uint8Array -> number[] for the Anchor client ----------

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

export function toCertArg(c: MatchLiveCert) {
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

export function toCheckpointArg(cp: Checkpoint) {
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

/** Both session-key signatures over a checkpoint digest (the open/supersede sigs). */
export function cpSigs(bundle: SessionBundle, cp: Checkpoint): number[][] {
  const d = checkpointDigest(cp);
  return [signDigest(bundle.local, d), signDigest(bundle.counter, d)];
}

/** The oracle input a checkpoint claims — the realized transcript the on-chain
 *  derive_claim_outcome and the TS oracle must independently agree on. */
export function checkpointOracleInput(cp: Checkpoint): OracleInput {
  return {
    stepCount: cp.stepCount,
    matchupType: cp.matchupType,
    p1Guess: cp.p1Guess,
    p2Guess: cp.p2Guess,
    firstCommitter: cp.firstCommitter,
  };
}

// --- setup + steps ----------------------------------------------------------

export interface XchainCtx {
  rt: SolanaRuntime;
  bundle: SessionBundle;
  globalConfigPda: PublicKey;
  tournamentPda: PublicKey;
  poolPda: PublicKey;
  tournamentId: BN;
  stake: BN;
  tranche: BN;
  claimWindow: number;
}

export interface SetupParams {
  bundle: SessionBundle;
  tournamentId: BN;
  stake: BN;
  tranche: BN;
  claimWindow: number;
  /** Treasury account. Keep it DISTINCT from the runtime payer so it isn't the
   *  tx fee-payer — otherwise conservation ledgers can't tell a payout from gas.
   *  Defaults to the payer (back-compat) when conservation isn't asserted. */
  treasury?: PublicKey;
}

/** Idempotent singletons + tournament + funded xpool. Returns the shared context. */
export async function setupXchain(
  rt: SolanaRuntime,
  p: SetupParams
): Promise<XchainCtx> {
  const authority = rt.payer;
  const globalConfigPda = rt.pda([Buffer.from("global_config")]);
  const tournamentPda = rt.pda([
    Buffer.from("tournament"),
    u64le(BigInt(p.tournamentId.toString())),
  ]);
  const poolPda = rt.pda([Buffer.from("xpool")]);

  await rt.program.methods
    .initialize()
    .accountsPartial({
      gameCounter: rt.pda([Buffer.from("game_counter")]),
      authority,
      systemProgram: SystemProgram.programId,
    })
    .rpc();
  await rt.program.methods
    .initializeConfig(5000)
    .accountsPartial({
      globalConfig: globalConfigPda,
      authority,
      matchmaker: authority,
      treasury: p.treasury ?? authority,
      systemProgram: SystemProgram.programId,
    })
    .rpc();
  const now = await rt.now();
  await rt.program.methods
    .createTournament(p.tournamentId, new BN(now - 60), new BN(now + 86400))
    .accountsPartial({
      tournament: tournamentPda,
      authority,
      systemProgram: SystemProgram.programId,
    })
    .rpc();
  await rt.program.methods
    .initializeXpool(
      authority,
      toArray(p.bundle.operator.address),
      p.tranche.muln(8),
      p.claimWindow,
      900
    )
    .accountsPartial({
      pool: poolPda,
      globalConfig: globalConfigPda,
      authority,
      systemProgram: SystemProgram.programId,
    })
    .rpc();
  await rt.program.methods
    .xpoolDeposit(p.tranche.muln(8))
    .accountsPartial({
      pool: poolPda,
      funder: authority,
      systemProgram: SystemProgram.programId,
    })
    .rpc();

  return {
    rt,
    bundle: p.bundle,
    globalConfigPda,
    tournamentPda,
    poolPda,
    tournamentId: p.tournamentId,
    stake: p.stake,
    tranche: p.tranche,
    claimWindow: p.claimWindow,
  };
}

export interface LockedMatch {
  cert: MatchLiveCert;
  liveDigest: Uint8Array;
  liveSigs: number[][];
  xmatchPda: PublicKey;
  playerProfilePda: PublicKey;
  windowEnd: number;
}

/** create_xmatch (Funded) -> lock_xtranche (Locked). Player is leg A / P1. */
export async function lockMatch(
  ctx: XchainCtx,
  args: { seed: string; matchupCommitment: Uint8Array; player: Keypair }
): Promise<LockedMatch> {
  const { rt } = ctx;
  const matchId = Array.from(keccak256(new TextEncoder().encode(args.seed)));
  const xmatchPda = rt.pda([Buffer.from("xmatch"), Buffer.from(matchId)]);
  const playerProfilePda = rt.pda([
    Buffer.from("player"),
    u64le(BigInt(ctx.tournamentId.toString())),
    args.player.publicKey.toBuffer(),
  ]);

  const now = await rt.now();
  const matchDeadline = new BN(now + 7200);

  await rt.program.methods
    .createXmatch(matchId, {
      tournamentId: ctx.tournamentId,
      playerIsP1: true,
      sessionKey: toArray(ctx.bundle.local.address),
      counterSessionKey: toArray(ctx.bundle.counter.address),
      stakeLamports: ctx.stake,
      fundDeadline: new BN(now + 3600),
      matchDeadline,
    })
    .accountsPartial({
      xmatch: xmatchPda,
      globalConfig: ctx.globalConfigPda,
      matchmaker: rt.payer,
      player: args.player.publicKey,
      systemProgram: SystemProgram.programId,
    })
    .signers([args.player])
    .rpc();

  const legA: CertLeg = {
    chainTag: SOLANA_CHAIN_TAG,
    contract: rt.program.programId.toBytes(),
    player: args.player.publicKey.toBytes(),
    sessionKey: ctx.bundle.local.address,
    stake: BigInt(ctx.stake.toString()),
    tranche: BigInt(ctx.tranche.toString()),
  };
  const legB: CertLeg = {
    chainTag: EVM_CHAIN_TAG,
    contract: new Uint8Array(32),
    player: new Uint8Array(32),
    sessionKey: ctx.bundle.counter.address,
    stake: BigInt(ctx.stake.toString()),
    tranche: BigInt(ctx.tranche.toString()),
  };
  const cert: MatchLiveCert = {
    matchId: Uint8Array.from(matchId),
    tournamentId: BigInt(ctx.tournamentId.toString()),
    matchupCommitment: args.matchupCommitment,
    legA,
    legB,
    quoteTimestamp: BigInt(now),
    quoteMaxAgeSecs: 600,
    matchDeadline: BigInt(matchDeadline.toString()),
    claimWindowSecs: ctx.claimWindow,
    aIsP1: 1,
  };
  const liveDigest = matchLiveDigest(cert);
  const liveSigs = [
    signDigest(ctx.bundle.local, liveDigest),
    signDigest(ctx.bundle.counter, liveDigest),
    signDigest(ctx.bundle.operator, liveDigest),
  ];

  await rt.program.methods
    .lockXtranche(toCertArg(cert), liveSigs[2])
    .accountsPartial({
      xmatch: xmatchPda,
      pool: ctx.poolPda,
      cranker: rt.payer,
    })
    .rpc();

  assertLegalTransition(XCHAIN_GRAPH, "Funded", "Locked");

  return {
    cert,
    liveDigest,
    liveSigs,
    xmatchPda,
    playerProfilePda,
    windowEnd: Number(matchDeadline) + ctx.claimWindow,
  };
}

export interface XmatchState {
  statusLabel: string;
  bestStepCount: number;
  bestOutcomeKind: number;
}

export async function readXmatch(
  ctx: XchainCtx,
  xmatchPda: PublicKey
): Promise<XmatchState> {
  const xm = await ctx.rt.program.account.xChainMatch.fetch(xmatchPda);
  return {
    statusLabel: xchainStatusLabel(xm.status as Record<string, unknown>),
    bestStepCount: Number(xm.bestStepCount),
    bestOutcomeKind: Number(xm.bestOutcomeKind),
  };
}

/** A StateView over a cross-chain match for the property battery. */
export function xmatchView(ctx: XchainCtx, xmatchPda: PublicKey): StateView {
  return {
    async readStatus() {
      return (await readXmatch(ctx, xmatchPda)).statusLabel;
    },
    async readOutcomeKind() {
      const s = await readXmatch(ctx, xmatchPda);
      return s.statusLabel === "Funded" ? -1 : s.bestOutcomeKind;
    },
  };
}

/** open_xclaim: Locked -> Claiming with the claimant's checkpoint. */
export async function openXclaim(
  ctx: XchainCtx,
  m: LockedMatch,
  cp: Checkpoint
): Promise<void> {
  await ctx.rt.program.methods
    .openXclaim(
      toCertArg(m.cert),
      toCheckpointArg(cp),
      m.liveSigs,
      cpSigs(ctx.bundle, cp)
    )
    .accountsPartial({ xmatch: m.xmatchPda, pool: ctx.poolPda })
    .rpc();
  assertLegalTransition(XCHAIN_GRAPH, "Locked", "Claiming");
}

/** supersede_xclaim: a strictly-higher-step checkpoint overrides the claim. */
export async function supersedeXclaim(
  ctx: XchainCtx,
  m: LockedMatch,
  cp: Checkpoint
): Promise<void> {
  const before = (await readXmatch(ctx, m.xmatchPda)).bestStepCount;
  await ctx.rt.program.methods
    .supersedeXclaim(
      toCertArg(m.cert),
      toCheckpointArg(cp),
      cpSigs(ctx.bundle, cp)
    )
    .accountsPartial({ xmatch: m.xmatchPda })
    .rpc();
  assertLegalTransition(XCHAIN_GRAPH, "Claiming", "Claiming");
  const after = (await readXmatch(ctx, m.xmatchPda)).bestStepCount;
  if (after <= before) {
    throw new Error(
      `supersede did not increase step count (${before} -> ${after})`
    );
  }
}

/** settle_xclaim: Claiming -> ClaimSettled (permissionless; pass a distinct
 *  cranker to make a second settle a byte-distinct tx for bankrun dedup). */
export async function settleXclaim(
  ctx: XchainCtx,
  m: LockedMatch,
  opts: { cranker?: Keypair } = {}
): Promise<void> {
  const config = await ctx.rt.program.account.globalConfig.fetch(
    ctx.globalConfigPda
  );
  const xm = await ctx.rt.program.account.xChainMatch.fetch(m.xmatchPda);
  const b = ctx.rt.program.methods.settleXclaim().accountsPartial({
    xmatch: m.xmatchPda,
    pool: ctx.poolPda,
    tournament: ctx.tournamentPda,
    playerProfile: m.playerProfilePda,
    globalConfig: ctx.globalConfigPda,
    treasury: config.treasury,
    player: xm.player,
    cranker: opts.cranker ? opts.cranker.publicKey : ctx.rt.payer,
    systemProgram: SystemProgram.programId,
  });
  if (opts.cranker) {
    await b.signers([opts.cranker]).rpc();
  } else {
    await b.rpc();
  }
  assertLegalTransition(XCHAIN_GRAPH, "Claiming", "ClaimSettled");
}
