import { ComputeBudgetProgram, Keypair, PublicKey, TransactionInstruction } from "@solana/web3.js";
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  SHILLBOT_PROGRAM_ID,
  buildTransaction,
  inspectTransaction,
  verifyIntent,
  type BuildRequest,
} from "./index.js";

const wallet = Keypair.fromSeed(Uint8Array.from({ length: 32 }, (_, i) => i + 1)).publicKey;
const sponsor = Keypair.fromSeed(Uint8Array.from({ length: 32 }, (_, i) => 32 - i)).publicKey;
const task = new PublicKey(Uint8Array.from({ length: 32 }, () => 7));
const feed = new PublicKey(Uint8Array.from({ length: 32 }, () => 8));
const recentBlockhash = PublicKey.default.toBase58();
const golden = JSON.parse(
  readFileSync(new URL("../test-vectors.json", import.meta.url), "utf8"),
) as {
  vectors: Array<{ action: string; accounts: string[]; data_base64: string }>;
  sponsored: Array<{ action: string; fee_payer: string; signers: string[] }>;
};

function base(action: BuildRequest["action"]) {
  return {
    action,
    wallet: wallet.toBase58(),
    network: "devnet" as const,
    recentBlockhash,
  };
}

function fixtures(): BuildRequest[] {
  return [
    {
      ...base("create"),
      action: "create",
      nonce: "42",
      escrowLamports: "1000000",
      contentHash: "11".repeat(32),
      deadline: "1900000000",
      submitMargin: "14400",
      claimBuffer: "14400",
      platform: 0,
      attestationDelayOverride: 0,
      challengeWindowOverride: 0,
      verificationTimeoutOverride: 0,
      requiresApproval: true,
      verificationKind: 0,
    },
    { ...base("claim"), action: "claim", taskPda: task.toBase58() },
    { ...base("submit"), action: "submit", taskPda: task.toBase58(), contentId: "video-123" },
    { ...base("approve"), action: "approve", taskPda: task.toBase58() },
    {
      ...base("verify"),
      action: "verify",
      taskPda: task.toBase58(),
      switchboardFeed: feed.toBase58(),
      compositeScore: "900000",
      verificationHash: "22".repeat(32),
      crankInstructions: [ComputeBudgetProgram.setComputeUnitLimit({ units: 400_000 })],
    },
    {
      ...base("finalize"),
      action: "finalize",
      taskPda: task.toBase58(),
      agent: wallet.toBase58(),
      client: sponsor.toBase58(),
      treasury: feed.toBase58(),
    },
  ];
}

describe("transaction construction", () => {
  it("matches the Rust instruction golden vectors for every action", () => {
    for (const request of fixtures()) {
      const vector = golden.vectors.find((candidate) => candidate.action === request.action)!;
      const instruction = buildTransaction(request).transaction_intent;
      const decoded = inspectTransaction(buildTransaction(request).unsigned_tx);
      const lifecycle = decoded.instructions.find(
        (ix) => ix.program_id === SHILLBOT_PROGRAM_ID.toBase58(),
      )!;
      expect(instruction.accounts).toEqual(vector.accounts);
      expect(lifecycle.data_base64).toBe(vector.data_base64);
    }
  });

  it.each(fixtures().flatMap((request) => [
    { request, version: "legacy" as const },
    { request, version: "v0" as const },
  ]))("builds and inspects $request.action $version", ({ request, version }) => {
    const built = buildTransaction({ ...request, version } as BuildRequest);
    const inspection = inspectTransaction(built.unsigned_tx);
    expect(inspection.version).toBe(version);
    expect(inspection.fee_payer).toBe(wallet.toBase58());
    expect(inspection.instructions.some((ix) => ix.program_id === SHILLBOT_PROGRAM_ID.toBase58())).toBe(true);
    expect(verifyIntent(built.unsigned_tx, built.transaction_intent)).toEqual(inspection);
  });

  it("constructs sponsored claim and submit messages without any private key", () => {
    for (const request of fixtures().filter((candidate) => candidate.action === "claim" || candidate.action === "submit")) {
      const built = buildTransaction({ ...request, sponsor: sponsor.toBase58() } as BuildRequest);
      const inspection = inspectTransaction(built.unsigned_tx);
      expect(inspection.fee_payer).toBe(sponsor.toBase58());
      expect(inspection.signers).toEqual([sponsor.toBase58(), wallet.toBase58()]);
      const vector = golden.sponsored.find((candidate) => candidate.action === request.action)!;
      expect(inspection.fee_payer).toBe(vector.fee_payer);
      expect(inspection.signers).toEqual(vector.signers);
    }
  });

  it("binds an optional sponsored-claim payout route to the task and agent", () => {
    const built = buildTransaction({
      ...fixtures()[1],
      action: "claim",
      sponsor: sponsor.toBase58(),
      payoutTo: feed.toBase58(),
    } as BuildRequest);
    const inspection = verifyIntent(built.unsigned_tx, built.transaction_intent);
    const shillbotInstructions = inspection.instructions.filter(
      (ix) => ix.program_id === SHILLBOT_PROGRAM_ID.toBase58(),
    );
    expect(shillbotInstructions).toHaveLength(2);
    expect(shillbotInstructions[0].accounts).toEqual([task.toBase58(), wallet.toBase58()]);
  });

  it("detects intent tampering", () => {
    const built = buildTransaction(fixtures()[1]);
    const altered = {
      ...built.transaction_intent,
      accounts: [PublicKey.default.toBase58(), ...built.transaction_intent.accounts.slice(1)],
    };
    expect(() => verifyIntent(built.unsigned_tx, altered)).toThrow("intent digest mismatch");
  });

  it("does not accept arbitrary companion instructions as a second lifecycle", () => {
    const request = fixtures()[1];
    const extra = new TransactionInstruction({
      programId: SHILLBOT_PROGRAM_ID,
      keys: [],
      data: Buffer.alloc(8),
    });
    const verifyRequest = {
      ...base("verify"),
      action: "verify" as const,
      taskPda: task.toBase58(),
      switchboardFeed: feed.toBase58(),
      compositeScore: "1",
      verificationHash: "00".repeat(32),
      crankInstructions: [extra],
    };
    const built = buildTransaction(verifyRequest);
    expect(() => verifyIntent(built.unsigned_tx, built.transaction_intent)).toThrow(
      "unexpected Shillbot companion",
    );
    expect(request.action).toBe("claim");
  });
});
