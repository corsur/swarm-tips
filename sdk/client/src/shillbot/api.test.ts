import { describe, expect, it, vi } from "vitest";
import { Keypair } from "@solana/web3.js";
import { buildCreate } from "./index.js";
import { permittedPreparedProgramIds, ShillbotApiClient, validatePreparedTransaction } from "./api.js";

describe("ShillbotApiClient", () => {
  it("injects auth and routes typed creator/earner endpoints", async () => {
    const fetch = vi.fn(async () => new Response(JSON.stringify({ transaction: "tx" }), { status: 200 }));
    const api = new ShillbotApiClient({ baseUrl: "https://shill.test", fetch, network: "devnet", getToken: () => "jwt" });
    await api.approveTask("task/a");
    const calls = fetch.mock.calls as unknown as Array<[RequestInfo | URL, RequestInit]>;
    expect(calls[0][0]).toBe("https://shill.test/tasks/task%2Fa/approve?network=devnet");
    expect((calls[0][1].headers as Record<string, string>).Authorization).toBe("Bearer jwt");
  });

  it("allows only the network-specific Switchboard verification programs", () => {
    expect(permittedPreparedProgramIds("verify", "devnet")).toContain("Aio4gaXjXzJNVLtzwtNVmSqGKpANtXhybbkhtAC94ji2");
    expect(permittedPreparedProgramIds("verify", "devnet")).not.toContain("SBondMDrcV3K4kxZR1HNVT7osZxAHVHgYXL5Ze1oMUv");
    expect(permittedPreparedProgramIds("verify", "mainnet")).toEqual([]);
    expect(permittedPreparedProgramIds("claim", "devnet")).toEqual([]);
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

  it("P1-8: routes listPendingApproval to GET /client/pending-approval with auth headers and optional network param", async () => {
    const seen: Array<{ url: string; method?: string; auth: string | null }> = [];
    const fetchImpl = async (input: RequestInfo | URL, init?: RequestInit) => {
      seen.push({
        url: String(input),
        method: init?.method,
        auth: new Headers(init?.headers).get("Authorization"),
      });
      return new Response(JSON.stringify({ tasks: [{ task_id: "pending-1" }] }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      });
    };

    const mainnetApi = new ShillbotApiClient({
      baseUrl: "https://shill.test",
      fetch: fetchImpl as typeof fetch,
      network: "mainnet",
      getToken: () => "client-jwt",
    });
    const res1 = await mainnetApi.listPendingApproval();
    expect(res1).toEqual({ tasks: [{ task_id: "pending-1" }] });
    expect(seen[0]).toEqual({
      url: "https://shill.test/client/pending-approval",
      method: "GET",
      auth: "Bearer client-jwt",
    });

    await mainnetApi.listPendingApproval("devnet");
    expect(seen[1]).toEqual({
      url: "https://shill.test/client/pending-approval?network=devnet",
      method: "GET",
      auth: "Bearer client-jwt",
    });

    const devnetApi = new ShillbotApiClient({
      baseUrl: "https://shill.test",
      fetch: fetchImpl as typeof fetch,
      network: "devnet",
      getToken: () => "dev-jwt",
    });
    await devnetApi.listPendingApproval();
    expect(seen[2]).toEqual({
      url: "https://shill.test/client/pending-approval?network=devnet",
      method: "GET",
      auth: "Bearer dev-jwt",
    });

    await devnetApi.listPendingApproval({ network: "mainnet", token: "override-jwt" });
    expect(seen[3]).toEqual({
      url: "https://shill.test/client/pending-approval?network=mainnet",
      method: "GET",
      auth: "Bearer override-jwt",
    });
  });
});
