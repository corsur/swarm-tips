import { describe, it, expect, vi } from "vitest";
import {
  gameStatusExists,
  sendConfirmedTx,
  sendCreateGameConfirmingEffect,
  shouldAutoFund,
  type TxSender,
  type PinnedReader,
  type FallbackReader,
} from "./transactions.js";
import { type Chain } from "viem";

const CHAIN = { id: 84532 } as Chain;
const ADDR = "0x70D922d213235a78c7a5c66d7F56Cb7226339433" as `0x${string}`;
const FAST = { confirmMs: 6, pollMs: 3, attempts: 3 };
const NOT_MINED = () => {
  throw new Error("TransactionReceiptNotFound");
};

function sender(hashes: string[]): { wallet: TxSender; send: ReturnType<typeof vi.fn> } {
  let i = 0;
  const send = vi.fn(async () => hashes[Math.min(i++, hashes.length - 1)] as `0x${string}`);
  return { wallet: { account: { address: ADDR }, sendTransaction: send }, send };
}
function reader(over: Partial<PinnedReader> = {}): PinnedReader {
  return {
    getTransactionCount: vi.fn(async () => 5),
    getTransactionReceipt: vi.fn(async () => ({ status: "success" }) as never),
    estimateFeesPerGas: vi.fn(async () => ({
      maxFeePerGas: 1_000_000n,
      maxPriorityFeePerGas: 100_000n,
    })),
    ...over,
  };
}

describe("sendConfirmedTx — confirm-or-replace (Base Sepolia drop/nonce resilience)", () => {
  const call = { to: "0xC0" as `0x${string}`, data: "0xda7a" as `0x${string}`, value: 0n };

  it("sends once with the pinned nonce and returns on a successful receipt", async () => {
    const { wallet, send } = sender(["0xh1"]);
    const r = reader();
    await expect(sendConfirmedTx(wallet, r, CHAIN, call, "createGame", FAST)).resolves.toMatchObject({
      status: "success",
    });
    expect(send).toHaveBeenCalledTimes(1);
    expect(send.mock.calls[0][0]).toMatchObject({ nonce: 5 }); // pinned nonce
  });

  it("throws on a reverted receipt (never a false success)", async () => {
    const { wallet } = sender(["0xh1"]);
    const r = reader({ getTransactionReceipt: vi.fn(async () => ({ status: "reverted" }) as never) });
    await expect(sendConfirmedTx(wallet, r, CHAIN, call, "revealGuess", FAST)).rejects.toThrow(
      /reverted/,
    );
  });

  it("RESUBMITS a dropped tx at the SAME nonce with bumped fees, then confirms", async () => {
    const { wallet, send } = sender(["0xdropped", "0xreplacement"]);
    let calls = 0;
    // First attempt never mines (poll throws); second attempt confirms.
    const getTransactionReceipt = vi.fn(async () => {
      calls++;
      if (calls <= Math.ceil(FAST.confirmMs / FAST.pollMs)) NOT_MINED();
      return { status: "success" } as never;
    });
    const r = reader({ getTransactionReceipt });

    await expect(sendConfirmedTx(wallet, r, CHAIN, call, "createGame", FAST)).resolves.toMatchObject({
      status: "success",
    });
    expect(send).toHaveBeenCalledTimes(2);
    // Both sends use the SAME nonce (a replacement, never a second game)...
    expect(send.mock.calls[0][0]).toMatchObject({ nonce: 5 });
    expect(send.mock.calls[1][0]).toMatchObject({ nonce: 5 });
    // ...and the resubmit bumped the fee so the replacement is accepted.
    expect((send.mock.calls[1][0] as { maxFeePerGas: bigint }).maxFeePerGas).toBeGreaterThan(
      1_000_000n,
    );
  });

  it("throws after exhausting attempts if the tx never mines", async () => {
    const { wallet } = sender(["0xh1"]);
    const r = reader({ getTransactionReceipt: vi.fn(async () => NOT_MINED() as never) });
    await expect(
      sendConfirmedTx(wallet, r, CHAIN, call, "createGame", { ...FAST, attempts: 2 }),
    ).rejects.toThrow(/did not confirm after 2 attempts/);
  });

  it("pinned poll always times out BUT fallback finds a success receipt → resolves (mainnet lag)", async () => {
    const { wallet } = sender(["0xmined"]);
    const pinned = reader({ getTransactionReceipt: vi.fn(async () => NOT_MINED() as never) });
    const getTransactionReceipt = vi.fn(async () => ({ status: "success" }) as never);
    const fallback: FallbackReader = { getTransactionReceipt };
    await expect(
      sendConfirmedTx(wallet, pinned, CHAIN, call, "commitGuess", { ...FAST, attempts: 2 }, fallback),
    ).resolves.toMatchObject({ status: "success" });
    // The authoritative fallback re-check runs exactly once, on the tx we sent.
    expect(getTransactionReceipt).toHaveBeenCalledTimes(1);
    expect(getTransactionReceipt).toHaveBeenCalledWith({ hash: "0xmined" });
  });

  it("pinned AND fallback both never find the receipt → still throws 'did not confirm'", async () => {
    const { wallet } = sender(["0xh1"]);
    const pinned = reader({ getTransactionReceipt: vi.fn(async () => NOT_MINED() as never) });
    const fallback: FallbackReader = { getTransactionReceipt: vi.fn(async () => NOT_MINED() as never) };
    await expect(
      sendConfirmedTx(wallet, pinned, CHAIN, call, "createGame", { ...FAST, attempts: 2 }, fallback),
    ).rejects.toThrow(/did not confirm after 2 attempts/);
  });

  it("fallback returns a REVERTED receipt → throws reverted (non-success stays fatal)", async () => {
    const { wallet } = sender(["0xh1"]);
    const pinned = reader({ getTransactionReceipt: vi.fn(async () => NOT_MINED() as never) });
    const fallback: FallbackReader = {
      getTransactionReceipt: vi.fn(async () => ({ status: "reverted" }) as never),
    };
    await expect(
      sendConfirmedTx(wallet, pinned, CHAIN, call, "revealGuess", { ...FAST, attempts: 2 }, fallback),
    ).rejects.toThrow(/reverted/);
  });
});

describe("gameStatusExists — Status enum decode (None=0 vs anything else)", () => {
  it("is false only for None=0", () => {
    expect(gameStatusExists(0)).toBe(false); // None — createGame never landed
    expect(gameStatusExists(1)).toBe(true); // Pending
    expect(gameStatusExists(2)).toBe(true); // Active
    expect(gameStatusExists(3)).toBe(true); // Committing
    expect(gameStatusExists(4)).toBe(true); // Revealing
    expect(gameStatusExists(5)).toBe(true); // Resolved
  });
});

describe("sendCreateGameConfirmingEffect — verify the on-chain EFFECT, not the receipt", () => {
  const RECEIPT_LAG = () => {
    throw new Error("createGame did not confirm after 4 attempts");
  };

  it("send throws but game EXISTS on-chain (status 2) → resolves 'recovered' (proceed)", async () => {
    const send = vi.fn(RECEIPT_LAG);
    const gameExists = vi.fn(async () => gameStatusExists(2));
    await expect(sendCreateGameConfirmingEffect(send, gameExists)).resolves.toBe("recovered");
    expect(gameExists).toHaveBeenCalledTimes(1);
  });

  it("send throws and game does NOT exist (status 0) → re-throws", async () => {
    const send = vi.fn(RECEIPT_LAG);
    const gameExists = vi.fn(async () => gameStatusExists(0));
    await expect(sendCreateGameConfirmingEffect(send, gameExists)).rejects.toThrow(
      /did not confirm/,
    );
    expect(gameExists).toHaveBeenCalledTimes(1);
  });

  it("send succeeds normally → resolves 'sent' without reading the chain", async () => {
    const send = vi.fn(async () => "0xreceipt");
    const gameExists = vi.fn(async () => true);
    await expect(sendCreateGameConfirmingEffect(send, gameExists)).resolves.toBe("sent");
    expect(gameExists).not.toHaveBeenCalled();
  });
});

describe("shouldAutoFund — auto-advance out of matched, no button", () => {
  const G = "0x1234";
  const base = { phase: "matched", gameId: G, ready: true, lastFundedGameId: null };

  it("fires once when matched + ready + not yet funded for this game", () => {
    expect(shouldAutoFund(base)).toBe(true);
  });

  it("does NOT fire before matched (no game to fund yet)", () => {
    expect(shouldAutoFund({ ...base, phase: "finding" })).toBe(false);
    expect(shouldAutoFund({ ...base, phase: "idle" })).toBe(false);
  });

  it("does NOT fire after matched (already funded / playing)", () => {
    for (const phase of ["funded", "chat", "committed", "done"]) {
      expect(shouldAutoFund({ ...base, phase })).toBe(false);
    }
  });

  it("does NOT fire until every precondition is ready", () => {
    expect(shouldAutoFund({ ...base, ready: false })).toBe(false);
  });

  it("does NOT re-fire for a game_id already auto-funded (re-render guard)", () => {
    expect(shouldAutoFund({ ...base, lastFundedGameId: G })).toBe(false);
  });

  it("re-arms for a NEW match (different game_id)", () => {
    expect(shouldAutoFund({ ...base, lastFundedGameId: "0xold" })).toBe(true);
  });

  it("does NOT fire with no game_id yet", () => {
    expect(shouldAutoFund({ ...base, gameId: undefined })).toBe(false);
  });
});
