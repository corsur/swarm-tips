import * as anchor from "@coral-xyz/anchor";
import { Program, BN } from "@coral-xyz/anchor";
import type { CoordinationGame } from "../../target/types/coordination_game";
import { createHash, randomBytes } from "crypto";
import { Keypair, PublicKey, SystemProgram } from "@solana/web3.js";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

export const TOURNAMENT_ID = new BN(1);
export const STAKE = new BN(50_000_000); // 0.05 SOL
export const GUESS_SAME_TEAM = 0;
export const GUESS_DIFF_TEAM = 1;

// ---------------------------------------------------------------------------
// Commit helpers
// ---------------------------------------------------------------------------

export interface Commit {
  commitment: number[]; // SHA-256(R), 32 bytes
  r: number[]; // random preimage, 32 bytes
}

export function generateCommit(guess: 0 | 1): Commit {
  const r = randomBytes(32);
  r[31] = (r[31] & 0xfe) | guess; // encode guess in the last bit
  const commitment = createHash("sha256").update(r).digest();
  return { commitment: Array.from(commitment), r: Array.from(r) };
}

export interface MatchupCommit {
  commitment: number[]; // SHA-256(R_matchup), 32 bytes
  r: number[]; // random preimage, 32 bytes
}

export function generateMatchupCommit(matchupType: 0 | 1): MatchupCommit {
  const r = randomBytes(32);
  r[31] = (r[31] & 0xfe) | matchupType;
  const commitment = createHash("sha256").update(r).digest();
  return { commitment: Array.from(commitment), r: Array.from(r) };
}

export function tournamentIdBuf(tournamentId: BN = TOURNAMENT_ID): Buffer {
  return tournamentId.toArrayLike(Buffer, "le", 8);
}

export function escrowPda(
  programId: PublicKey,
  tournamentId: BN,
  player: PublicKey
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [
      Buffer.from("escrow"),
      tournamentId.toArrayLike(Buffer, "le", 8),
      player.toBuffer(),
    ],
    programId
  );
}

/** Deposit stake into escrow for a player in a given tournament. */
export async function depositStake(
  program: Program<CoordinationGame>,
  tournamentId: BN,
  tournamentPdaKey: PublicKey,
  player: Keypair
): Promise<void> {
  const [escrow] = escrowPda(program.programId, tournamentId, player.publicKey);
  await program.methods
    .depositStake()
    .accountsPartial({
      escrow,
      tournament: tournamentPdaKey,
      player: player.publicKey,
      systemProgram: SystemProgram.programId,
    })
    .signers([player])
    .rpc();
}

/** Creates a game on-chain with P1. Returns [gamePda, gameId, matchupRPreimage]. */
export async function createGameOnChain(
  program: Program<CoordinationGame>,
  gameCounterPda: PublicKey,
  globalConfigPda: PublicKey,
  matchmakerKey: PublicKey,
  tournamentPdaKey: PublicKey,
  matchupType: number,
  player: Keypair
): Promise<[PublicKey, BN, number[]]> {
  const matchupCommit = generateMatchupCommit(matchupType as 0 | 1);

  // Deposit stake for P1 before creating the game
  const tournamentData = await program.account.tournament.fetch(
    tournamentPdaKey
  );
  const tournamentId = tournamentData.tournamentId as BN;
  await depositStake(program, tournamentId, tournamentPdaKey, player);

  const counter = await program.account.gameCounter.fetch(gameCounterPda);
  const gameId = counter.count as BN;
  const [gPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("game"), gameId.toArrayLike(Buffer, "le", 8)],
    program.programId
  );
  const [profilePda] = PublicKey.findProgramAddressSync(
    [
      Buffer.from("player"),
      tournamentId.toArrayLike(Buffer, "le", 8),
      player.publicKey.toBuffer(),
    ],
    program.programId
  );
  const [escrow] = escrowPda(program.programId, tournamentId, player.publicKey);
  await program.methods
    .createGame(STAKE, matchupCommit.commitment as any)
    .accountsPartial({
      game: gPda,
      gameCounter: gameCounterPda,
      playerProfile: profilePda,
      escrow,
      tournament: tournamentPdaKey,
      globalConfig: globalConfigPda,
      matchmaker: matchmakerKey,
      player: player.publicKey,
      systemProgram: SystemProgram.programId,
    })
    .signers([player])
    .rpc();
  return [gPda, gameId, matchupCommit.r];
}

/** Player 2 joins an existing game (deposits escrow + joins). */
export async function joinGameOnChain(
  program: Program<CoordinationGame>,
  globalConfigPda: PublicKey,
  matchmakerKey: PublicKey,
  gamePdaKey: PublicKey,
  tournamentId: BN,
  tournamentPdaKey: PublicKey,
  player: Keypair
): Promise<PublicKey> {
  await depositStake(program, tournamentId, tournamentPdaKey, player);
  const [profilePda] = PublicKey.findProgramAddressSync(
    [
      Buffer.from("player"),
      tournamentId.toArrayLike(Buffer, "le", 8),
      player.publicKey.toBuffer(),
    ],
    program.programId
  );
  const [escrow] = escrowPda(program.programId, tournamentId, player.publicKey);
  await program.methods
    .joinGame()
    .accountsPartial({
      game: gamePdaKey,
      playerProfile: profilePda,
      escrow,
      tournament: tournamentPdaKey,
      globalConfig: globalConfigPda,
      matchmaker: matchmakerKey,
      player: player.publicKey,
      systemProgram: SystemProgram.programId,
    })
    .signers([player])
    .rpc();
  return profilePda;
}

/** Ensure globalConfig and gameCounter are initialized (idempotent helper). */
export async function ensureConfigInitialized(
  program: Program<CoordinationGame>,
  provider: anchor.AnchorProvider,
  treasury: PublicKey
): Promise<{ gameCounterPda: PublicKey; globalConfigPda: PublicKey }> {
  const [gameCounterPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("game_counter")],
    program.programId
  );
  const [globalConfigPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("global_config")],
    program.programId
  );

  try {
    await program.account.gameCounter.fetch(gameCounterPda);
  } catch {
    await program.methods
      .initialize()
      .accountsPartial({
        gameCounter: gameCounterPda,
        authority: provider.wallet.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();
  }

  try {
    await program.account.globalConfig.fetch(globalConfigPda);
  } catch {
    await program.methods
      .initializeConfig(5000)
      .accountsPartial({
        globalConfig: globalConfigPda,
        authority: provider.wallet.publicKey,
        matchmaker: provider.wallet.publicKey,
        treasury,
        systemProgram: SystemProgram.programId,
      })
      .rpc();
  }

  return { gameCounterPda, globalConfigPda };
}
