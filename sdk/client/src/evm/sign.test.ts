import { describe, expect, it } from "vitest";
import { privateKeyToAccount } from "viem/accounts";
import { recoverAddress } from "viem";
import { signDigestForRelay } from "./sign.js";
import { e2eTestModeEnabled, E2E_TEST_MODE_KEY } from "./testing.js";

const KEY = `0x${"11".repeat(32)}` as const;
const DIGEST = `0x${"ab".repeat(32)}` as const;

describe("signDigestForRelay", () => {
  it("emits v = 0 | 1, not the EVM 27 | 28", async () => {
    // This is the whole reason the helper exists: the game-api relay and the
    // Solana program verify with k256 recover_address (v = 0|1), while viem
    // signs with the EVM ecrecover convention (v = 27|28). Getting this
    // backwards makes every relayed signature fail to recover.
    const sig = await signDigestForRelay(KEY, DIGEST);
    expect(sig).toMatch(/^0x[0-9a-f]{130}$/);
    const v = parseInt(sig.slice(-2), 16);
    expect([0, 1]).toContain(v);
  });

  it("still recovers the signer once v is put back to EVM form", async () => {
    // Proves the normalisation is lossless: r||s are untouched and only v moved.
    const sig = await signDigestForRelay(KEY, DIGEST);
    const v = parseInt(sig.slice(-2), 16);
    const evmSig = `${sig.slice(0, -2)}${(v + 27).toString(
      16
    )}` as `0x${string}`;
    const recovered = await recoverAddress({ hash: DIGEST, signature: evmSig });
    expect(recovered.toLowerCase()).toBe(
      privateKeyToAccount(KEY).address.toLowerCase()
    );
  });
});

describe("e2eTestModeEnabled", () => {
  it("is false with no window (SSR) and fails closed on a throwing localStorage", () => {
    const original = globalThis.window;
    delete (globalThis as { window?: Window }).window;
    expect(e2eTestModeEnabled()).toBe(false);

    // A localStorage that throws (Safari private mode, blocked storage) must
    // DISABLE test mode, never enable it.
    globalThis.window = {
      localStorage: {
        getItem() {
          throw new Error("blocked");
        },
      } as unknown as Storage,
    } as Window & typeof globalThis;
    expect(e2eTestModeEnabled()).toBe(false);

    globalThis.window = {
      localStorage: {
        getItem: (k: string) => (k === E2E_TEST_MODE_KEY ? "1" : null),
      } as unknown as Storage,
    } as Window & typeof globalThis;
    expect(e2eTestModeEnabled()).toBe(true);

    globalThis.window = original;
  });
});
