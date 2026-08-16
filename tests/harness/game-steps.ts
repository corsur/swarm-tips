// Composable SAME-CHAIN coordination-game steps for the harness. Same pattern as
// xchain-steps.ts but for the single-chain commit-reveal game: each step performs
// ONE action through a SolanaRuntime and asserts its state-machine transition;
// scenarios compose them and run the property battery over the realized
// Ledger + Transcript. Works on any SolanaRuntime (bankrun OR validator), which
// is what makes a same-chain scenario portable + metamorphically checkable.

import { BN } from "@coral-xyz/anchor";
import { Keypair, PublicKey, SystemProgram } from "@solana/web3.js";
import { createHash, randomBytes } from "crypto";
import {
  Transcript as OracleInput,
  deriveClaimOutcome,
  TERMINAL_STEP_COUNT,
} from "../helpers/outcome-oracle";
import { SolanaRuntime } from "./target";
import {
  StateView,
  TransitionGraph,
  assertLegalTransition,
} from "./assertions";

/** Allowed same-chain game status transitions. Resolved is terminal. */
export const GAME_GRAPH: TransitionGraph = {
  Pending: ["Active"],
  Active: ["Committing", "Resolved"], // Resolved = timeout, both forfeit
  Committing: ["Revealing", "Resolved"],
  // self-loop: the FIRST reveal keeps the game in Revealing; the SECOND resolves.
  Revealing: ["Revealing", "Resolved"],
  Resolved: [],
};

export interface Commit {
  commitment: number[]; // SHA-256(R)
  r: number[]; // preimage; low bit of R[31] encodes the guess/matchup bit
}

/** Random 32-byte preimage with `bit` in the last bit; commitment = SHA-256(R). */
export function generateCommit(bit: 0 | 1): Commit {
  const r = randomBytes(32);
  r[31] = (r[31] & 0xfe) | bit;
  return {
    commitment: Array.from(createHash("sha256").update(r).digest()),
    r: Array.from(r),
  };
}

function u64le(n: number | bigint): Buffer {
  const b = Buffer.alloc(8);
  b.writeBigUInt64LE(BigInt(n));
  return b;
}

/** Anchor enum object ({ committing: {} }) -> canonical label ("Committing"). */
export function gameStatusLabel(state: Record<string, unknown>): string {
  const key = Object.keys(state)[0];
  return key.charAt(0).toUpperCase() + key.slice(1);
}

function profilePda(
  rt: SolanaRuntime,
  tournamentId: BN,
  player: PublicKey
): PublicKey {
  return rt.pda([
    Buffer.from("player"),
    u64le(BigInt(tournamentId.toString())),
    player.toBuffer(),
  ]);
}

function escrowPda(
  rt: SolanaRuntime,
  tournamentId: BN,
  player: PublicKey
): PublicKey {
  return rt.pda([
    Buffer.from("escrow"),
    u64le(BigInt(tournamentId.toString())),
    player.toBuffer(),
  ]);
}

export interface GameCtx {
  rt: SolanaRuntime;
  gameCounterPda: PublicKey;
  globalConfigPda: PublicKey;
  treasury: PublicKey;
}

/** Idempotent singletons. `treasury` is recorded in GlobalConfig; keep it distinct
 *  from the runtime payer so conservation ledgers don't conflate payout with gas. */
export async function setupGame(
  rt: SolanaRuntime,
  p: { treasury: PublicKey; splitBps?: number }
): Promise<GameCtx> {
  const authority = rt.payer;
  const gameCounterPda = rt.pda([Buffer.from("game_counter")]);
  const globalConfigPda = rt.pda([Buffer.from("global_config")]);
  await rt.program.methods
    .initialize()
    .accountsPartial({
      gameCounter: gameCounterPda,
      authority,
      systemProgram: SystemProgram.programId,
    })
    .rpc();
  await rt.program.methods
    .initializeConfig(p.splitBps ?? 5000)
    .accountsPartial({
      globalConfig: globalConfigPda,
      authority,
      matchmaker: authority,
      treasury: p.treasury,
      systemProgram: SystemProgram.programId,
    })
    .rpc();
  return { rt, gameCounterPda, globalConfigPda, treasury: p.treasury };
}

/** Create a fresh tournament (each match uses its own so prize deltas isolate). */
export async function createTournament(
  ctx: GameCtx,
  tournamentId: BN
): Promise<PublicKey> {
  const tournamentPda = ctx.rt.pda([
    Buffer.from("tournament"),
    u64le(BigInt(tournamentId.toString())),
  ]);
  const now = await ctx.rt.now();
  await ctx.rt.program.methods
    .createTournament(tournamentId, new BN(now - 60), new BN(now + 86400))
    .accountsPartial({
      tournament: tournamentPda,
      authority: ctx.rt.payer,
      systemProgram: SystemProgram.programId,
    })
    .rpc();
  return tournamentPda;
}

async function depositStake(
  ctx: GameCtx,
  tournamentId: BN,
  tournamentPda: PublicKey,
  player: Keypair
): Promise<void> {
  await ctx.rt.program.methods
    .depositStake()
    .accountsPartial({
      escrow: escrowPda(ctx.rt, tournamentId, player.publicKey),
      tournament: tournamentPda,
      player: player.publicKey,
      systemProgram: SystemProgram.programId,
    })
    .signers([player])
    .rpc();
}

export interface GameMatch {
  gamePda: PublicKey;
  tournamentId: BN;
  tournamentPda: PublicKey;
  p1: Keypair;
  p2: Keypair;
  p1Profile: PublicKey;
  p2Profile: PublicKey;
  rMatchup: number[]; // first-revealer matchup preimage
}

/** create_game (P1, matchmaker co-signs) -> join_game (P2): Pending -> Active. */
export async function startMatch(
  ctx: GameCtx,
  args: {
    tournamentId: BN;
    tournamentPda: PublicKey;
    p1: Keypair;
    p2: Keypair;
    stake: BN;
    matchupType: 0 | 1;
  }
): Promise<GameMatch> {
  const { rt } = ctx;
  const matchup = generateCommit(args.matchupType);

  await depositStake(ctx, args.tournamentId, args.tournamentPda, args.p1);
  const counter = await rt.program.account.gameCounter.fetch(
    ctx.gameCounterPda
  );
  const gameId = counter.count as BN;
  const gamePda = rt.pda([
    Buffer.from("game"),
    u64le(BigInt(gameId.toString())),
  ]);
  const p1Profile = profilePda(rt, args.tournamentId, args.p1.publicKey);
  const p2Profile = profilePda(rt, args.tournamentId, args.p2.publicKey);

  await rt.program.methods
    .createGame(args.stake, matchup.commitment as never)
    .accountsPartial({
      game: gamePda,
      gameCounter: ctx.gameCounterPda,
      playerProfile: p1Profile,
      escrow: escrowPda(rt, args.tournamentId, args.p1.publicKey),
      tournament: args.tournamentPda,
      globalConfig: ctx.globalConfigPda,
      matchmaker: rt.payer,
      player: args.p1.publicKey,
      systemProgram: SystemProgram.programId,
    })
    .signers([args.p1])
    .rpc();

  await depositStake(ctx, args.tournamentId, args.tournamentPda, args.p2);
  await rt.program.methods
    .joinGame()
    .accountsPartial({
      game: gamePda,
      playerProfile: p2Profile,
      escrow: escrowPda(rt, args.tournamentId, args.p2.publicKey),
      tournament: args.tournamentPda,
      globalConfig: ctx.globalConfigPda,
      matchmaker: rt.payer,
      player: args.p2.publicKey,
      systemProgram: SystemProgram.programId,
    })
    .signers([args.p2])
    .rpc();
  assertLegalTransition(GAME_GRAPH, "Pending", "Active");

  return {
    gamePda,
    tournamentId: args.tournamentId,
    tournamentPda: args.tournamentPda,
    p1: args.p1,
    p2: args.p2,
    p1Profile,
    p2Profile,
    rMatchup: matchup.r,
  };
}

/** commit_guess by one player; records the commit in the transcript. */
export async function commit(
  ctx: GameCtx,
  m: GameMatch,
  who: { playerNum: 1 | 2; commit: Commit },
  transcript: { commit: (n: 1 | 2) => void }
): Promise<void> {
  const player = who.playerNum === 1 ? m.p1 : m.p2;
  const before = gameStatusLabel(
    (await ctx.rt.program.account.game.fetch(m.gamePda)).state as Record<
      string,
      unknown
    >
  );
  await ctx.rt.program.methods
    .commitGuess(who.commit.commitment as never)
    .accountsPartial({ game: m.gamePda, player: player.publicKey })
    .signers([player])
    .rpc();
  transcript.commit(who.playerNum);
  const after = gameStatusLabel(
    (await ctx.rt.program.account.game.fetch(m.gamePda)).state as Record<
      string,
      unknown
    >
  );
  assertLegalTransition(GAME_GRAPH, before, after);
}

/** reveal_guess by one player; first revealer supplies r_matchup (else null). */
export async function reveal(
  ctx: GameCtx,
  m: GameMatch,
  who: { playerNum: 1 | 2; commit: Commit; guess: 0 | 1; first: boolean },
  transcript: { reveal: (n: 1 | 2, g: number, mt?: number) => void }
): Promise<void> {
  const player = who.playerNum === 1 ? m.p1 : m.p2;
  const before = gameStatusLabel(
    (await ctx.rt.program.account.game.fetch(m.gamePda)).state as Record<
      string,
      unknown
    >
  );
  await ctx.rt.program.methods
    .revealGuess(
      who.commit.r as never,
      who.first ? (m.rMatchup as never) : null
    )
    .accountsPartial({
      game: m.gamePda,
      p1Profile: m.p1Profile,
      p2Profile: m.p2Profile,
      tournament: m.tournamentPda,
      playerOneWallet: m.p1.publicKey,
      playerTwoWallet: m.p2.publicKey,
      globalConfig: ctx.globalConfigPda,
      treasury: ctx.treasury,
      player: player.publicKey,
    })
    .signers([player])
    .rpc();
  const matchupType = who.first ? m.rMatchup[31] & 1 : undefined;
  transcript.reveal(who.playerNum, who.guess, matchupType);
  const after = gameStatusLabel(
    (await ctx.rt.program.account.game.fetch(m.gamePda)).state as Record<
      string,
      unknown
    >
  );
  assertLegalTransition(GAME_GRAPH, before, after);
}

/** resolve_timeout (permissionless): crank a stalled game to Resolved after its
 *  slot deadline. The caller (any signer) pays the fee and receives no prize. */
export async function resolveTimeout(
  ctx: GameCtx,
  m: GameMatch
): Promise<void> {
  await ctx.rt.program.methods
    .resolveTimeout()
    .accountsPartial({
      game: m.gamePda,
      p1Profile: m.p1Profile,
      p2Profile: m.p2Profile,
      tournament: m.tournamentPda,
      globalConfig: ctx.globalConfigPda,
      treasury: ctx.treasury,
      playerOneWallet: m.p1.publicKey,
      playerTwoWallet: m.p2.publicKey,
      caller: ctx.rt.payer,
    })
    .rpc();
}

/** A StateView over a same-chain game for the property battery. */
export function gameView(ctx: GameCtx, gamePda: PublicKey): StateView {
  return {
    async readStatus() {
      const g = await ctx.rt.program.account.game.fetch(gamePda);
      return gameStatusLabel(g.state as Record<string, unknown>);
    },
    async readOutcomeKind() {
      const g = await ctx.rt.program.account.game.fetch(gamePda);
      if (gameStatusLabel(g.state as Record<string, unknown>) !== "Resolved") {
        return -1;
      }
      const input: OracleInput = {
        stepCount: TERMINAL_STEP_COUNT,
        matchupType: Number(g.matchupType),
        p1Guess: Number(g.p1Guess),
        p2Guess: Number(g.p2Guess),
        firstCommitter: Number(g.firstCommitter),
      };
      return deriveClaimOutcome(input);
    },
  };
}
