import { PublicKey } from "@solana/web3.js";
import { COORDINATION_GAME_PROGRAM_ID } from "../contracts/index.js";

const encoder = new TextEncoder();

export const COORDINATION_GAME_TOURNAMENT_ID = 1n;
export const GUESS_UNREVEALED = 255;
export const REVEAL_TIMEOUT_SLOTS = 14_400n;

export function toGuessBit(value: number): 0 | 1 {
  if (value === 0 || value === 1) return value;
  throw new RangeError(`Expected 0 or 1, got ${value}`);
}

export function u64LE(value: bigint): Uint8Array {
  if (value < 0n || value > 0xffff_ffff_ffff_ffffn) {
    throw new RangeError("u64 value is out of range");
  }
  const bytes = new Uint8Array(8);
  let remaining = value;
  for (let index = 0; index < bytes.length; index += 1) {
    bytes[index] = Number(remaining & 0xffn);
    remaining >>= 8n;
  }
  return bytes;
}

function programId(programId?: PublicKey | string): PublicKey {
  if (programId instanceof PublicKey) return programId;
  return programId
    ? new PublicKey(programId)
    : COORDINATION_GAME_PROGRAM_ID;
}

export function globalConfigPda(id?: PublicKey | string): [PublicKey, number] {
  return PublicKey.findProgramAddressSync([encoder.encode("global_config")], programId(id));
}

export function gameCounterPda(id?: PublicKey | string): [PublicKey, number] {
  return PublicKey.findProgramAddressSync([encoder.encode("game_counter")], programId(id));
}

export function gamePda(gameId: bigint, id?: PublicKey | string): [PublicKey, number] {
  return PublicKey.findProgramAddressSync([encoder.encode("game"), u64LE(gameId)], programId(id));
}

export function tournamentPda(tournamentId: bigint, id?: PublicKey | string): [PublicKey, number] {
  return PublicKey.findProgramAddressSync([encoder.encode("tournament"), u64LE(tournamentId)], programId(id));
}

export function playerProfilePda(
  tournamentId: bigint,
  wallet: PublicKey,
  id?: PublicKey | string,
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [encoder.encode("player"), u64LE(tournamentId), wallet.toBytes()],
    programId(id),
  );
}

export function escrowPda(
  tournamentId: bigint,
  wallet: PublicKey,
  id?: PublicKey | string,
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [encoder.encode("escrow"), u64LE(tournamentId), wallet.toBytes()],
    programId(id),
  );
}

export function playerSessionPda(
  wallet: PublicKey,
  session: PublicKey,
  id?: PublicKey | string,
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [encoder.encode("game_session"), wallet.toBytes(), session.toBytes()],
    programId(id),
  );
}

export interface GlobalConfigData {
  authority: PublicKey;
  matchmaker: PublicKey;
  treasury: PublicKey;
  treasurySplitBps: number;
  stakeLamports: bigint;
}

export interface TournamentData {
  tournamentId: bigint;
  startTime: bigint;
  endTime: bigint;
  prizeLamports: bigint;
  gameCount: bigint;
  finalized: boolean;
}

function viewOf(data: Uint8Array): DataView {
  return new DataView(data.buffer, data.byteOffset, data.byteLength);
}

export function decodeGlobalConfig(data: Uint8Array): GlobalConfigData {
  const stakeOffset = 107;
  if (data.length < stakeOffset + 8) {
    throw new RangeError(
      `GlobalConfig is the pre-migration ${data.length}-byte layout and carries no stake_lamports`,
    );
  }
  return {
    authority: new PublicKey(data.subarray(8, 40)),
    matchmaker: new PublicKey(data.subarray(40, 72)),
    treasury: new PublicKey(data.subarray(72, 104)),
    treasurySplitBps: data[104] | (data[105] << 8),
    stakeLamports: new DataView(data.buffer, data.byteOffset + stakeOffset, 8).getBigUint64(0, true),
  };
}

export function decodeTournament(data: Uint8Array): TournamentData {
  const minimumLength = 8 + 8 + 32 + 8 + 8 + 8 + 8 + 1;
  if (data.length < minimumLength) {
    throw new RangeError(`Tournament account data too short: ${data.length} bytes`);
  }
  const view = viewOf(data);
  return {
    tournamentId: view.getBigUint64(8, true),
    startTime: view.getBigInt64(48, true),
    endTime: view.getBigInt64(56, true),
    prizeLamports: view.getBigUint64(64, true),
    gameCount: view.getBigUint64(72, true),
    finalized: data[80] !== 0,
  };
}

export function decodeTournamentStats(data: Uint8Array): {
  gameCount: number;
  prizeLamports: bigint;
} {
  const tournament = decodeTournament(data);
  return { gameCount: Number(tournament.gameCount), prizeLamports: tournament.prizeLamports };
}

export interface SolanaResumeGame {
  state: string;
  playerOne: string;
  playerTwo: string;
  p1Commit: readonly number[] | Uint8Array;
  p2Commit: readonly number[] | Uint8Array;
  p1Guess: number;
  p2Guess: number;
}

export interface SolanaResumeState {
  phase: "active" | "committing" | "revealing";
  iAmP1: boolean;
  bothCommitted: boolean;
}

function hasCommitted(commit: readonly number[] | Uint8Array): boolean {
  return Array.from(commit).some((byte) => byte !== 0);
}

export function deriveSolanaResumeState(
  game: SolanaResumeGame,
  wallet: string,
): SolanaResumeState | null {
  if (!["active", "committing", "revealing"].includes(game.state)) return null;
  const iAmP1 = game.playerOne === wallet;
  const iAmP2 = game.playerTwo === wallet;
  if (!iAmP1 && !iAmP2) return null;
  if ((iAmP1 ? game.p1Guess : game.p2Guess) !== GUESS_UNREVEALED) return null;
  const mine = hasCommitted(iAmP1 ? game.p1Commit : game.p2Commit);
  const bothCommitted = hasCommitted(game.p1Commit) && hasCommitted(game.p2Commit);
  return {
    phase: !mine ? "active" : bothCommitted ? "revealing" : "committing",
    iAmP1,
    bothCommitted,
  };
}

export interface SolanaGameTimes {
  state: string;
  activatedAtSlot: bigint;
  p1CommitSlot: bigint;
  p2CommitSlot: bigint;
  commitTimeoutSlots: bigint;
}

export function timeoutDeadlineSlot(game: SolanaGameTimes): bigint | null {
  const later = game.p1CommitSlot > game.p2CommitSlot ? game.p1CommitSlot : game.p2CommitSlot;
  if (game.state === "active") return game.activatedAtSlot + game.commitTimeoutSlots;
  if (game.state === "committing") return later + game.commitTimeoutSlots;
  if (game.state === "revealing") return later + REVEAL_TIMEOUT_SLOTS;
  return null;
}

export function timeoutClaimableAtSlot(game: SolanaGameTimes, currentSlot: bigint): boolean {
  const deadline = timeoutDeadlineSlot(game);
  return deadline !== null && currentSlot >= deadline;
}
