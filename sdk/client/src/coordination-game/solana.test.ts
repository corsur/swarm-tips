import { describe, expect, it, vi } from "vitest";
import { Keypair, SystemProgram, type Connection } from "@solana/web3.js";
import {
  CoordinationGameSolanaClient,
  sessionBalanceFloor,
  sessionFundLamports,
} from "./solana.js";

describe("CoordinationGameSolanaClient", () => {
  it("builds the wallet-funded session setup with an auth memo", async () => {
    const wallet = Keypair.generate();
    const session = Keypair.generate();
    const connection = {
      getAccountInfo: vi.fn(async () => ({ data: new Uint8Array() })),
    } as unknown as Connection;
    const client = new CoordinationGameSolanaClient({
      connection,
      wallet: {
        publicKey: wallet.publicKey,
        signTransaction: async (transaction) => transaction,
      },
    });
    const transaction = await client.buildSessionSetupTransaction({
      sessionPublicKey: session.publicKey,
      lamports: 20,
      memoNonce: "nonce-1",
    });
    expect(transaction.instructions).toHaveLength(2);
    expect(transaction.instructions[0].programId.equals(SystemProgram.programId)).toBe(true);
    expect(new TextDecoder().decode(transaction.instructions[1].data)).toBe("nonce-1");
  });

  it("derives funding and balance floors from the live stake", () => {
    expect(sessionFundLamports(68_500_000n)).toBe(78_500_000);
    expect(sessionBalanceFloor(68_500_000n)).toBe(73_500_000);
  });

  it("uses an injected clock/sleep boundary while polling confirmations", async () => {
    const getSignatureStatus = vi
      .fn()
      .mockResolvedValueOnce({ value: null })
      .mockResolvedValueOnce({ value: { confirmationStatus: "confirmed" } });
    const client = new CoordinationGameSolanaClient({
      connection: { getSignatureStatus } as unknown as Connection,
    });
    const sleep = vi.fn(async () => undefined);
    await client.waitForSignature("sig", { attempts: 2, intervalMs: 1, sleep });
    expect(sleep).toHaveBeenCalledTimes(2);
  });
});
