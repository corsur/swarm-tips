import { describe, expect, it, vi } from "vitest";
import { Keypair } from "@solana/web3.js";
import { buildCreate } from "./index.js";
import { ShillbotApiClient, validatePreparedTransaction } from "./api.js";

describe("ShillbotApiClient", () => {
  it("injects auth and routes typed creator/earner endpoints", async () => {
    const fetch = vi.fn(async () => new Response(JSON.stringify({ transaction: "tx" }), { status: 200 }));
    const api = new ShillbotApiClient({ baseUrl: "https://shill.test", fetch, network: "devnet", getToken: () => "jwt" });
    await api.approveTask("task/a");
    const calls = fetch.mock.calls as unknown as Array<[RequestInfo | URL, RequestInit]>;
    expect(calls[0][0]).toBe("https://shill.test/tasks/task%2Fa/approve?network=devnet");
    expect((calls[0][1].headers as Record<string, string>).Authorization).toBe("Bearer jwt");
  });

  it("validates action, signer, task PDA, program set, and create amount before signing", () => {
    const wallet = Keypair.generate().publicKey.toBase58();
    const built = buildCreate({
      wallet,
      network: "devnet",
      recentBlockhash: "11111111111111111111111111111111",
      nonce: "7",
      escrowLamports: "68500000",
      contentHash: "11".repeat(32),
      deadline: "100",
      submitMargin: "10",
      claimBuffer: "10",
      platform: 0,
      attestationDelayOverride: 0,
      challengeWindowOverride: 0,
      verificationTimeoutOverride: 0,
      requiresApproval: true,
      verificationKind: 0,
    });
    expect(validatePreparedTransaction(built.unsigned_tx, { action: "create", network: "devnet", wallet, taskPda: built.transaction_intent.task_pda, escrowLamports: 68_500_000n })).toMatchObject({ fee_payer: wallet });
    expect(() => validatePreparedTransaction(built.unsigned_tx, { action: "create", network: "devnet", wallet, escrowLamports: 1n })).toThrow(/Escrow amount/);
    expect(() => validatePreparedTransaction(built.unsigned_tx, { action: "claim", network: "devnet", wallet })).toThrow(/exactly one/);
  });
});
