import * as anchor from "@coral-xyz/anchor";
import { Program, BN } from "@coral-xyz/anchor";
import type { Shillbot } from "../../target/types/shillbot";
import { createHash, randomBytes } from "crypto";
import { Keypair, PublicKey, SystemProgram } from "@solana/web3.js";

// ---------------------------------------------------------------------------
// Constants matching the on-chain program
// ---------------------------------------------------------------------------

export const MAX_SCORE = 1_000_000;
export const CHALLENGE_WINDOW_SECONDS = 86_400;
export const PROTOCOL_FEE_BPS = 1000; // 10%
export const QUALITY_THRESHOLD = new BN(200_000);
// Test escrow value. Arbitrary 0.36 SOL so historical test scenarios continue
// to use the same numeric values.
export const ESCROW_LAMPORTS = new BN(360_000_000); // 0.36 SOL

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

export function contentHash(data: string): number[] {
  return Array.from(createHash("sha256").update(data).digest());
}

/** Derive the GlobalState PDA. */
export function globalStatePda(programId: PublicKey): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("shillbot_global")],
    programId
  );
}

/** Derive a Task PDA from its nonce (formerly the global counter) and client. */
export function taskPda(
  taskId: BN,
  client: PublicKey,
  programId: PublicKey
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [
      Buffer.from("task"),
      taskId.toArrayLike(Buffer, "le", 8),
      client.toBuffer(),
    ],
    programId
  );
}

/** A fresh random u64 Task nonce — client-provided PDA seed so concurrent creates don't collide. */
export function newTaskNonce(): BN {
  return new BN(randomBytes(8));
}

/** Derive a Challenge PDA from task ID and challenger. */
export function challengePda(
  taskId: BN,
  challenger: PublicKey,
  programId: PublicKey
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [
      Buffer.from("challenge"),
      taskId.toArrayLike(Buffer, "le", 8),
      challenger.toBuffer(),
    ],
    programId
  );
}

/** Derive an AgentState PDA. */
export function agentStatePda(
  agent: PublicKey,
  programId: PublicKey
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("agent_state"), agent.toBuffer()],
    programId
  );
}

/** Derive a SessionDelegate PDA. */
export function sessionPda(
  agent: PublicKey,
  delegate: PublicKey,
  programId: PublicKey
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("session"), agent.toBuffer(), delegate.toBuffer()],
    programId
  );
}

/** Airdrop SOL and confirm. */
export async function airdrop(
  connection: anchor.web3.Connection,
  pubkey: PublicKey,
  lamports: number
): Promise<void> {
  const sig = await connection.requestAirdrop(pubkey, lamports);
  await connection.confirmTransaction(sig);
}

/** Get current on-chain clock unix timestamp. */
export async function getClockTimestamp(
  connection: anchor.web3.Connection
): Promise<number> {
  const slot = await connection.getSlot();
  const blockTime = await connection.getBlockTime(slot);
  return blockTime ?? Math.floor(Date.now() / 1000);
}

/** Ensure GlobalState is initialized (idempotent helper). */
export async function ensureShillbotInitialized(
  program: Program<Shillbot>,
  provider: anchor.AnchorProvider,
  authority: Keypair,
  treasury: Keypair
): Promise<PublicKey> {
  const [globalPda] = globalStatePda(program.programId);
  try {
    await program.account.globalState.fetch(globalPda);
  } catch {
    await program.methods
      .initialize(
        PROTOCOL_FEE_BPS,
        QUALITY_THRESHOLD,
        new BN(0),
        authority.publicKey
      )
      .accountsPartial({
        globalState: globalPda,
        authority: authority.publicKey,
        treasury: treasury.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();
  }
  return globalPda;
}
