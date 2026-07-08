// Shared devnet-e2e harness — factors the boilerplate every scripts/e2e/*.ts
// duplicated (keypair loading, provider/Program bootstrap, PDA derivers, the
// updateParams snapshot/restore dance, the check()/failures mini-assert) into
// one place, and exposes a devnet SolanaRuntime<Shillbot> so the SAME composable
// steps the bankrun matrix uses (tests/harness/shillbot-steps.ts) drive live
// devnet too — the single source of truth for "how a task lifecycle runs".
//
// Keys (persistent, per the no-ephemeral-keypair rule — never Keypair.generate()
// where value flows): client/authority = id.json, attester = test.json (the
// devnet oracle_authority), agent = shillbot-game-platform-agent.json (a real
// arms-length key, distinct from client + attester as kind-1 requires).
//
// NOT a CI test (real devnet, mutates global params). Run individual scripts
// manually; each exits 0 on pass.

import * as anchor from "@coral-xyz/anchor";
import { BN } from "@coral-xyz/anchor";
import {
  Connection,
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  SystemProgram,
  Transaction,
} from "@solana/web3.js";
import { readFileSync } from "fs";
import { homedir } from "os";
import { join } from "path";
import type { Shillbot } from "../../target/types/shillbot";
import type { SolanaRuntime } from "../../tests/harness/target";
import {
  ShillbotCtx,
  globalStatePda,
} from "../../tests/harness/shillbot-steps";

export const DEVNET_RPC =
  process.env.DEVNET_RPC ??
  process.env.RPC_URL ??
  "https://api.devnet.solana.com";

/** Load a keypair from ~/.config/solana/<name>.json. */
export function loadKeypair(name: string): Keypair {
  const path = join(homedir(), ".config/solana", name);
  return Keypair.fromSecretKey(
    Uint8Array.from(JSON.parse(readFileSync(path, "utf8")) as number[])
  );
}

/** A running tally of assertions — replaces the copy-pasted check()/failures. */
export class Checker {
  failures = 0;
  check(cond: boolean, msg: string): void {
    console.log(`  ${cond ? "✓" : "✗"} ${msg}`);
    if (!cond) this.failures++;
  }
  /** Print PASS/FAIL and exit with the matching code. */
  finish(): never {
    console.log(
      `\n${this.failures === 0 ? "PASS" : "FAIL"} — ${
        this.failures
      } check(s) failed`
    );
    process.exit(this.failures === 0 ? 0 : 1);
  }
}

export const sleep = (ms: number): Promise<void> =>
  new Promise((r) => setTimeout(r, ms));

export interface DevnetHarness {
  connection: Connection;
  provider: anchor.AnchorProvider;
  program: anchor.Program<Shillbot>;
  runtime: SolanaRuntime<Shillbot>;
  globalPda: PublicKey;
  /** id.json — GlobalState authority AND the default fee payer / client. */
  authority: Keypair;
  /** test.json — the devnet oracle_authority (attester for kind-1). */
  attester: Keypair;
  /** shillbot-game-platform-agent.json — arms-length worker. */
  agent: Keypair;
}

/** Bootstrap a devnet connection + Program + a SolanaRuntime<Shillbot> adapter. */
export function connectDevnet(): DevnetHarness {
  const authority = loadKeypair("id.json");
  const attester = loadKeypair("test.json");
  const agent = loadKeypair("shillbot-game-platform-agent.json");
  const connection = new Connection(DEVNET_RPC, "confirmed");
  const provider = new anchor.AnchorProvider(
    connection,
    new anchor.Wallet(authority),
    { commitment: "confirmed" }
  );
  anchor.setProvider(provider);
  const program = new anchor.Program(
    JSON.parse(
      readFileSync(
        join(__dirname, "..", "..", "target", "idl", "shillbot.json"),
        "utf8"
      )
    ) as anchor.Idl,
    provider
  ) as unknown as anchor.Program<Shillbot>;
  const globalPda = globalStatePda(program.programId);

  const runtime: SolanaRuntime<Shillbot> = {
    program,
    payer: authority.publicKey,
    pda(seeds) {
      return PublicKey.findProgramAddressSync(seeds, program.programId)[0];
    },
    async getBalance(pk) {
      return BigInt(await connection.getBalance(pk, "confirmed"));
    },
    async now() {
      return Math.floor(Date.now() / 1000);
    },
    async warpTo() {
      throw new Error(
        "warpTo unsupported on devnet — use sleep() + short windows"
      );
    },
    async warpBySlots() {
      throw new Error("warpBySlots unsupported on devnet");
    },
    async fund(recipient, lamports) {
      await provider.sendAndConfirm(
        new Transaction().add(
          SystemProgram.transfer({
            fromPubkey: authority.publicKey,
            toPubkey: recipient,
            lamports: Number(lamports),
          })
        )
      );
    },
  };

  return {
    connection,
    provider,
    program,
    runtime,
    globalPda,
    authority,
    attester,
    agent,
  };
}

/** A ShillbotCtx over the ALREADY-INITIALIZED devnet global (no `initialize`).
 *  `authority` is the GLOBAL authority (id.json) — the signer resolve_challenge
 *  and update_params require. verify_task_attested needs the oracle_authority
 *  (test.json) instead, so callers pass `h.attester` EXPLICITLY to
 *  verifyTaskAttested; on devnet these are two different keys. */
export async function devnetCtx(h: DevnetHarness): Promise<ShillbotCtx> {
  const g = await h.program.account.globalState.fetch(h.globalPda);
  return {
    rt: h.runtime,
    globalPda: h.globalPda,
    treasury: g.treasury as PublicKey,
    authority: h.authority,
  };
}

/** The 14-tuple GlobalState param snapshot, for save/restore around a demo. */
export interface ParamSnapshot {
  protocolFeeBps: number;
  qualityThreshold: BN;
  challengeWindowSeconds: BN;
  verificationTimeoutSeconds: BN;
  attestationDelaySeconds: BN;
  stalenessWindowSeconds: BN;
  maxConcurrentClaims: number;
  challengeBondMultiplierBps: number;
  bondSlashTreasuryBps: number;
  paused: boolean;
  pausedPlatforms: number;
  rateLimitWindowSeconds: BN;
  maxTasksPerRateWindow: number;
  disputeResolutionWindowSeconds: BN;
}

export async function snapshotParams(h: DevnetHarness): Promise<ParamSnapshot> {
  const s = await h.program.account.globalState.fetch(h.globalPda);
  return {
    protocolFeeBps: s.protocolFeeBps,
    qualityThreshold: s.qualityThreshold as BN,
    challengeWindowSeconds: s.challengeWindowSeconds as BN,
    verificationTimeoutSeconds: s.verificationTimeoutSeconds as BN,
    attestationDelaySeconds: s.attestationDelaySeconds as BN,
    stalenessWindowSeconds: s.stalenessWindowSeconds as BN,
    maxConcurrentClaims: s.maxConcurrentClaims,
    challengeBondMultiplierBps: s.challengeBondMultiplierBps,
    bondSlashTreasuryBps: s.bondSlashTreasuryBps,
    paused: s.paused,
    pausedPlatforms: s.pausedPlatforms,
    rateLimitWindowSeconds: s.rateLimitWindowSeconds as BN,
    maxTasksPerRateWindow: s.maxTasksPerRateWindow,
    disputeResolutionWindowSeconds: s.disputeResolutionWindowSeconds as BN,
  };
}

/** Apply a full param set (used by restore + the challenge-window override). */
export async function applyParams(
  h: DevnetHarness,
  p: ParamSnapshot
): Promise<void> {
  await h.program.methods
    .updateParams(
      p.protocolFeeBps,
      p.qualityThreshold,
      p.challengeWindowSeconds,
      p.verificationTimeoutSeconds,
      p.attestationDelaySeconds,
      p.stalenessWindowSeconds,
      p.maxConcurrentClaims,
      p.challengeBondMultiplierBps as unknown as number,
      p.bondSlashTreasuryBps,
      p.paused,
      p.pausedPlatforms,
      p.rateLimitWindowSeconds,
      p.maxTasksPerRateWindow,
      p.disputeResolutionWindowSeconds
    )
    .accountsPartial({
      globalState: h.globalPda,
      authority: h.authority.publicKey,
    })
    .rpc();
}

/** Run `body` with a shrunk challenge window, restoring the snapshot after. */
export async function withChallengeWindow(
  h: DevnetHarness,
  seconds: number,
  body: () => Promise<void>
): Promise<void> {
  const snap = await snapshotParams(h);
  await applyParams(h, { ...snap, challengeWindowSeconds: new BN(seconds) });
  try {
    await body();
  } finally {
    await applyParams(h, snap);
  }
}

/** Run `body` with the challenge + verification-timeout windows shrunk to
 *  seconds (both only need `> 0` on-chain), restoring the snapshot after — so
 *  the finalize / challenge / expire terminal outcomes resolve in-run. The
 *  dispute-resolution window is NOT shrunk: its on-chain MIN is 1 hour, so the
 *  default-resolve outcome can't run in-run live (it's covered by the bankrun
 *  matrix via clock-warp). */
export async function withShrunkWindows(
  h: DevnetHarness,
  secs: { challenge: number; verificationTimeout: number },
  body: () => Promise<void>
): Promise<void> {
  const snap = await snapshotParams(h);
  await applyParams(h, {
    ...snap,
    challengeWindowSeconds: new BN(secs.challenge),
    verificationTimeoutSeconds: new BN(secs.verificationTimeout),
  });
  try {
    await body();
  } finally {
    await applyParams(h, snap);
  }
}

/** Ensure a keypair holds at least `minLamports` (top up from the authority). */
export async function ensureFunded(
  h: DevnetHarness,
  kp: PublicKey,
  minLamports: number
): Promise<void> {
  const bal = await h.connection.getBalance(kp, "confirmed");
  if (bal >= minLamports) return;
  await h.runtime.fund(kp, minLamports - bal + LAMPORTS_PER_SOL / 1000);
}
