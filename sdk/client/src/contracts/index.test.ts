import { BN } from "@coral-xyz/anchor";
import {
  Keypair,
  PublicKey,
  SystemProgram,
  SYSVAR_SLOT_HASHES_PUBKEY,
} from "@solana/web3.js";
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  COORDINATION_GAME_IDL,
  COORDINATION_GAME_PROGRAM_ID,
  SHILLBOT_IDL,
  SHILLBOT_PROGRAM_ID,
  agentStatePda,
  buildShillbotInstruction,
  clientStatePda,
  globalStatePda,
  taskPda,
} from "./index.js";

const wallet = Keypair.fromSeed(
  Uint8Array.from({ length: 32 }, (_, i) => i + 1)
).publicKey;
const sponsor = Keypair.fromSeed(
  Uint8Array.from({ length: 32 }, (_, i) => 32 - i)
).publicKey;
const task = new PublicKey(Uint8Array.from({ length: 32 }, () => 7));
const feed = new PublicKey(Uint8Array.from({ length: 32 }, () => 8));
const golden = JSON.parse(
  readFileSync(
    new URL("../../test/transaction-vectors.json", import.meta.url),
    "utf8"
  )
) as {
  vectors: Array<{ action: string; accounts: string[]; data_base64: string }>;
};

const cases = [
  {
    action: "create",
    name: "create_task",
    accounts: {
      global_state: globalStatePda(),
      task: taskPda(42n, wallet),
      client_state: clientStatePda(wallet),
      client: wallet,
      slot_hashes: SYSVAR_SLOT_HASHES_PUBKEY,
      system_program: SystemProgram.programId,
    },
    args: {
      nonce: new BN(42),
      escrow_lamports: new BN(1_000_000),
      content_hash: Array(32).fill(0x11),
      deadline: new BN(1_900_000_000),
      submit_margin: new BN(14_400),
      claim_buffer: new BN(14_400),
      platform: 0,
      attestation_delay_override: 0,
      challenge_window_override: 0,
      verification_timeout_override: 0,
      requires_approval: true,
      verification_kind: 0,
    },
  },
  {
    action: "claim",
    name: "claim_task",
    accounts: {
      task,
      global_state: globalStatePda(),
      agent_state: agentStatePda(wallet),
      agent: wallet,
      system_program: SystemProgram.programId,
    },
    args: {},
  },
  {
    action: "submit",
    name: "submit_work",
    accounts: {
      task,
      global_state: globalStatePda(),
      agent_state: agentStatePda(wallet),
      agent: wallet,
    },
    args: { content_id: Buffer.from("video-123") },
  },
  {
    action: "approve",
    name: "approve_task",
    accounts: { task, client: wallet },
    args: {},
  },
  {
    action: "verify",
    name: "verify_task",
    accounts: { task, global_state: globalStatePda(), switchboard_feed: feed },
    args: {
      composite_score: new BN(900_000),
      verification_hash: Array(32).fill(0x22),
    },
  },
  {
    action: "finalize",
    name: "finalize_task",
    accounts: {
      task,
      global_state: globalStatePda(),
      agent: wallet,
      client: sponsor,
      treasury: feed,
    },
    args: {},
  },
] as const;

describe("generated contract surface", () => {
  it("ships both canonical IDLs and program addresses", () => {
    expect(SHILLBOT_IDL.address).toBe(SHILLBOT_PROGRAM_ID.toBase58());
    expect(COORDINATION_GAME_IDL.address).toBe(
      COORDINATION_GAME_PROGRAM_ID.toBase58()
    );
    expect(
      SHILLBOT_IDL.instructions.some((ix) => ix.name === "claim_task")
    ).toBe(true);
    expect(COORDINATION_GAME_IDL.instructions.length).toBeGreaterThan(0);
  });

  it("derives the canonical Shillbot PDAs", () => {
    expect(globalStatePda().toBase58()).not.toBe(PublicKey.default.toBase58());
    expect(taskPda(42n, wallet)).toEqual(taskPda(42n, wallet));
    expect(clientStatePda(wallet)).toEqual(clientStatePda(wallet));
    expect(agentStatePda(wallet)).toEqual(agentStatePda(wallet));
  });

  it("encodes every supported lifecycle action from the generated IDL", () => {
    for (const fixture of cases) {
      const instruction = buildShillbotInstruction({
        name: fixture.name,
        accounts: fixture.accounts,
        args: fixture.args,
      });
      const vector = golden.vectors.find(
        (candidate) => candidate.action === fixture.action
      )!;
      expect(instruction.keys.map((key) => key.pubkey.toBase58())).toEqual(
        vector.accounts
      );
      expect(instruction.data.toString("base64")).toBe(vector.data_base64);
    }
  });

  it("rejects ABI names that are not present in the generated IDL", () => {
    expect(() =>
      buildShillbotInstruction({
        name: "approve_task",
        accounts: { task, client: wallet, surprise: sponsor },
      })
    ).toThrow("unknown account surprise");
    expect(() =>
      buildShillbotInstruction({
        name: "submit_work",
        accounts: {
          task,
          global_state: globalStatePda(),
          agent_state: agentStatePda(wallet),
          agent: wallet,
        },
      })
    ).toThrow("missing argument content_id");
  });
});
