import { describe, it, expect, vi } from "vitest";
import {
  setupEvmSessionIfNeeded,
  evmSessionGasBufferWei,
  evmSessionRecordIsLive,
  SESSION_MIN_REMAINING_SECS,
  EVM_SESSION_GAS_BUFFER_L1_WEI,
  EVM_SESSION_GAS_BUFFER_L2_WEI,
  waitForSessionFunding,
} from "./session-funding.js";
import { generateEvmSessionKey, evmSessionAddress } from "./session.js";

/** Fixed clock so expiry assertions are deterministic. */
const NOW = 1_785_000_000;

const MAIN = "0x996213ed4099707059b8b5d7489ffF23dAC9770d" as const;
const STAKE = 500_000_000_000_000n; // 0.0005 ETH
const CAIP2 = "eip155:84532"; // Base Sepolia

/** The chain the player SELECTED (Base Sepolia). Distinct from the stale chain
 *  the wallet client was bound to at connect time (Ethereum) — the tx must be
 *  built for the selected chain, not the client's stale binding. */
const SELECTED_CHAIN = { id: 84532, name: "Base Sepolia" } as never;

function mockWalletClient(txHash: string) {
  return {
    account: { address: MAIN },
    chain: { id: 1, name: "Ethereum" }, // stale connect-time binding
    sendTransaction: vi.fn().mockResolvedValue(txHash),
  } as never;
}

describe("evmSessionGasBufferWei (chain-aware gas headroom)", () => {
  it("budgets the large L1 buffer for Ethereum mainnet", () => {
    expect(evmSessionGasBufferWei("eip155:1")).toBe(EVM_SESSION_GAS_BUFFER_L1_WEI);
    // Ethereum Sepolia (eip155:11155111) is no longer a supported chain, so it
    // now falls through to the L2 default like any other unknown chain. It ran a
    // pre-v3 contract and was removed as duplicated, misleading coverage.
    expect(evmSessionGasBufferWei("eip155:11155111")).toBe(
      EVM_SESSION_GAS_BUFFER_L2_WEI,
    );
  });

  it("keeps the small L2 buffer for Base mainnet + Sepolia", () => {
    expect(evmSessionGasBufferWei("eip155:8453")).toBe(EVM_SESSION_GAS_BUFFER_L2_WEI);
    expect(evmSessionGasBufferWei("eip155:84532")).toBe(EVM_SESSION_GAS_BUFFER_L2_WEI);
  });

  it("L1 buffer is materially larger than L2 (L1 gas is ~10-100x)", () => {
    expect(EVM_SESSION_GAS_BUFFER_L1_WEI).toBeGreaterThan(EVM_SESSION_GAS_BUFFER_L2_WEI);
  });
});

const CONTRACT = "0x9E344F6FD80f4b2a20329a8C0dD4E16f70Bcd5ED" as const;
// openSessionAndDeposit(address,uint64,uint256) selector — the funding tx must
// target the CONTRACT. Plain openSession (0x1ad3c7ff) is no longer what the
// browser calls: it forwards everything to the session EOA, which is the
// custody model the escrow work removed.
const OPEN_SESSION_SELECTOR = "0x3e1900d7";

describe("setupEvmSessionIfNeeded", () => {
  it("sends stake + gas to the CONTRACT, which forwards ONLY gas to the session EOA", async () => {
    const key = generateEvmSessionKey();
    const txHash = `0x${"ab".repeat(32)}`;
    const wallet = mockWalletClient(txHash);
    // Empty for the pre-funding check (two conservative reads), then funded —
    // openSession really does raise the balance, and the client now waits for
    // that to become READABLE before spending from the session key.
    const getBalance = vi
      .fn()
      .mockResolvedValueOnce(0n)
      .mockResolvedValueOnce(0n)
      // GAS ONLY after funding. The stake is credited to withdrawable[wallet]
      // inside the same tx, so it never appears in the session EOA's balance —
      // if this test ever passes with STAKE + GAS here, the stake is back in
      // the ephemeral key and the custody fix has regressed.
      .mockResolvedValue(EVM_SESSION_GAS_BUFFER_L2_WEI);
    const waitForTransactionReceipt = vi.fn().mockResolvedValue({ status: "success" });

    const result = await setupEvmSessionIfNeeded(wallet, key, CONTRACT, STAKE, CAIP2, {
      readClient: { getBalance, waitForTransactionReceipt, readContract: vi.fn() },
      chain: SELECTED_CHAIN,
    });

    expect(result).toBe(txHash);
    const sendTx = (wallet as { sendTransaction: ReturnType<typeof vi.fn> })
      .sendTransaction;
    // A CONTRACT call (to == contract) carrying BOTH halves: the contract splits
    // them, forwarding `gasAmount` to the session EOA and crediting the rest to
    // withdrawable[wallet]. The tx is built for the SELECTED chain, not the
    // wallet client's stale connect-time chain (Ethereum here) — the live
    // chain-mismatch bug.
    expect(sendTx).toHaveBeenCalledWith(
      expect.objectContaining({
        to: CONTRACT,
        value: STAKE + EVM_SESSION_GAS_BUFFER_L2_WEI,
        chain: SELECTED_CHAIN,
      }),
    );
    // THE CUSTODY ASSERTION. The third argument is `gasAmount`, and it must be
    // the gas buffer ALONE — that is the only thing that leaves for the
    // ephemeral key. Encoding the full value here would put the stake back in
    // the session key while every other assertion still passed.
    const gasArg = BigInt(
      "0x" + (sendTx.mock.calls[0][0] as { data: string }).data.slice(-64),
    );
    expect(gasArg).toBe(EVM_SESSION_GAS_BUFFER_L2_WEI);
    const call = sendTx.mock.calls[0][0] as { data: string };
    expect(call.data.startsWith(OPEN_SESSION_SELECTOR)).toBe(true);
    // The registered session key is encoded in the calldata.
    expect(call.data.toLowerCase()).toContain(
      evmSessionAddress(key).slice(2).toLowerCase(),
    );
  });

  it("throws when openSession REVERTS instead of reporting setup as successful", async () => {
    const key = generateEvmSessionKey();
    const txHash = `0x${"cd".repeat(32)}`;
    const wallet = mockWalletClient(txHash);
    const getBalance = vi
      .fn()
      .mockResolvedValueOnce(0n)
      .mockResolvedValueOnce(0n)
      .mockResolvedValue(STAKE + EVM_SESSION_GAS_BUFFER_L2_WEI);
    // viem resolves for reverted txs too — status is the only signal.
    const waitForTransactionReceipt = vi
      .fn()
      .mockResolvedValue({ status: "reverted" });

    await expect(
      setupEvmSessionIfNeeded(wallet, key, CONTRACT, STAKE, CAIP2, {
        readClient: { getBalance, waitForTransactionReceipt, readContract: vi.fn() },
        chain: SELECTED_CHAIN,
      }),
    ).rejects.toThrow(/reverted/i);
  });

  // sessions(wallet) → [sessionKey, expiry]; withdrawable(wallet) → escrow bigint.
  // Untyped vi.fn() (like the other readContract mocks) so viem's generic
  // readContract signature accepts it; the impl is set separately.
  const liveSessionAndEscrow = (key: `0x${string}`, escrow: bigint) => {
    const fn = vi.fn();
    fn.mockImplementation(async (args: { functionName?: string }) =>
      args?.functionName === "withdrawable"
        ? escrow
        : [evmSessionAddress(key), BigInt(NOW + 24 * 3600)],
    );
    return fn;
  };

  it("no-ops when the session is funded, live, AND the escrow covers the stake (0 popups)", async () => {
    const key = generateEvmSessionKey();
    const wallet = mockWalletClient("0xunused");
    const getBalance = vi.fn().mockResolvedValue(STAKE + EVM_SESSION_GAS_BUFFER_L2_WEI);
    // Registered to THIS key, valid, AND withdrawable already holds the stake.
    const readContract = liveSessionAndEscrow(key, STAKE);

    const result = await setupEvmSessionIfNeeded(wallet, key, CONTRACT, STAKE, CAIP2, {
      readClient: { getBalance, waitForTransactionReceipt: vi.fn(), readContract },
      chain: SELECTED_CHAIN,
      nowSecs: () => NOW,
    });

    expect(result).toBeNull();
    expect((wallet as { sendTransaction: ReturnType<typeof vi.fn> }).sendTransaction)
      .not.toHaveBeenCalled();
  });

  it("re-deposits (1 popup) when the session is funded+live but the escrow was consumed", async () => {
    // The stake-before-queue correctness case: after game 1 spends the escrow via
    // _takeStake, gas + registration still look fine, but withdrawable is 0 — so
    // game 2 MUST re-deposit or createGame/joinGame reverts on an empty escrow.
    const key = generateEvmSessionKey();
    const txHash = `0x${"ab".repeat(32)}`;
    const wallet = mockWalletClient(txHash);
    const getBalance = vi.fn().mockResolvedValue(STAKE + EVM_SESSION_GAS_BUFFER_L2_WEI);
    const readContract = liveSessionAndEscrow(key, 0n); // escrow drained
    const waitForTransactionReceipt = vi.fn().mockResolvedValue({ status: "success" });

    const result = await setupEvmSessionIfNeeded(wallet, key, CONTRACT, STAKE, CAIP2, {
      readClient: { getBalance, waitForTransactionReceipt, readContract },
      chain: SELECTED_CHAIN,
      nowSecs: () => NOW,
    });

    expect(result).toBe(txHash); // re-opened
    expect((wallet as { sendTransaction: ReturnType<typeof vi.fn> }).sendTransaction)
      .toHaveBeenCalledTimes(1);
  });

  it("does NOT skip on a STALE-HIGH withdrawable read — takes the conservative lower of two", async () => {
    // The fallback RPC can route one read to a lagging node still reporting the
    // PREVIOUS game's withdrawable (stale-high). A single read would wrongly skip
    // the deposit and leave createGame to revert BadStake on an empty escrow.
    // The conservative min of two reads (stale-high, fresh-low=0) must re-deposit.
    const key = generateEvmSessionKey();
    const txHash = `0x${"ef".repeat(32)}`;
    const wallet = mockWalletClient(txHash);
    const getBalance = vi.fn().mockResolvedValue(STAKE + EVM_SESSION_GAS_BUFFER_L2_WEI);
    let wdCall = 0;
    const readContract = vi.fn();
    readContract.mockImplementation(async (args: { functionName?: string }) => {
      if (args?.functionName === "withdrawable") return [STAKE, 0n][wdCall++] ?? 0n; // stale-high, then fresh-low
      return [evmSessionAddress(key), BigInt(NOW + 24 * 3600)]; // sessions() → live
    });
    const waitForTransactionReceipt = vi.fn().mockResolvedValue({ status: "success" });

    const result = await setupEvmSessionIfNeeded(wallet, key, CONTRACT, STAKE, CAIP2, {
      readClient: { getBalance, waitForTransactionReceipt, readContract },
      chain: SELECTED_CHAIN,
      nowSecs: () => NOW,
    });

    expect(result).toBe(txHash); // re-deposited despite the stale-high read
    expect((wallet as { sendTransaction: ReturnType<typeof vi.fn> }).sendTransaction)
      .toHaveBeenCalledTimes(1);
  });
});

/**
 * The lockout this section pins down was live on Base Sepolia: a session key
 * still holding 0.0064 ETH against an expiry 17.7 hours in the past. Every
 * createGame/joinGame reverted BadSession, and the client never re-opened
 * because the funded check reported "already set up".
 *
 * The previous version of the test above ("no-ops when the session is already
 * funded") asserted exactly that behavior, so the suite was protecting the bug
 * rather than the invariant. Being funded is not being authorized.
 */
describe("setupEvmSessionIfNeeded — expiry, not just funding", () => {
  const liveRecord = (key: `0x${string}`, expiresIn: number) =>
    vi.fn().mockResolvedValue([evmSessionAddress(key), BigInt(NOW + expiresIn)]);

  it("REOPENS a funded session whose on-chain registration has expired", async () => {
    const key = generateEvmSessionKey();
    const txHash = `0x${"cd".repeat(32)}`;
    const wallet = mockWalletClient(txHash);
    const getBalance = vi.fn().mockResolvedValue(STAKE + EVM_SESSION_GAS_BUFFER_L2_WEI);
    const readContract = liveRecord(key, -17 * 3600); // expired 17h ago

    const result = await setupEvmSessionIfNeeded(wallet, key, CONTRACT, STAKE, CAIP2, {
      readClient: {
        getBalance,
        waitForTransactionReceipt: vi.fn().mockResolvedValue({ status: "success" }),
        readContract,
      },
      chain: SELECTED_CHAIN,
      nowSecs: () => NOW,
    });

    expect(result).toBe(txHash);
    const send = (wallet as { sendTransaction: ReturnType<typeof vi.fn> }).sendTransaction;
    expect(send).toHaveBeenCalledTimes(1);
    // The session key is already gas-funded, so the gas top-up is 0 (`need -
    // balance` was NEGATIVE here, which viem rejects — the recovery path would
    // have thrown). The STAKE still rides along: reopening happens per game, and
    // each game escrows its own stake.
    expect(send.mock.calls[0][0].value).toBe(STAKE);
    const gasArg = BigInt("0x" + send.mock.calls[0][0].data.slice(-64));
    expect(gasArg, "already funded, so nothing more goes to the session EOA").toBe(0n);
    expect(send.mock.calls[0][0].to).toBe(CONTRACT);
  });

  it("REOPENS when the wallet is registered to a DIFFERENT session key", async () => {
    const key = generateEvmSessionKey();
    const other = generateEvmSessionKey();
    const wallet = mockWalletClient(`0x${"ef".repeat(32)}`);
    const getBalance = vi.fn().mockResolvedValue(STAKE + EVM_SESSION_GAS_BUFFER_L2_WEI);
    // Valid, unexpired — but authorizes someone else, so it cannot act for us.
    const readContract = liveRecord(other, 24 * 3600);

    const result = await setupEvmSessionIfNeeded(wallet, key, CONTRACT, STAKE, CAIP2, {
      readClient: {
        getBalance,
        waitForTransactionReceipt: vi.fn().mockResolvedValue({ status: "success" }),
        readContract,
      },
      chain: SELECTED_CHAIN,
      nowSecs: () => NOW,
    });

    expect(result).not.toBeNull();
  });

  it("REOPENS rather than assuming live when sessions() is unreadable", async () => {
    const key = generateEvmSessionKey();
    const wallet = mockWalletClient(`0x${"11".repeat(32)}`);
    const getBalance = vi.fn().mockResolvedValue(STAKE + EVM_SESSION_GAS_BUFFER_L2_WEI);
    const readContract = vi.fn().mockRejectedValue(new Error("RPC down"));

    const result = await setupEvmSessionIfNeeded(wallet, key, CONTRACT, STAKE, CAIP2, {
      readClient: {
        getBalance,
        waitForTransactionReceipt: vi.fn().mockResolvedValue({ status: "success" }),
        readContract,
      },
      chain: SELECTED_CHAIN,
      nowSecs: () => NOW,
    });

    // One extra popup is recoverable; assuming "live" is the lockout.
    expect(result).not.toBeNull();
  });

  it("does not read sessions() to DECIDE when the key is UNDERFUNDED", async () => {
    const key = generateEvmSessionKey();
    const wallet = mockWalletClient(`0x${"22".repeat(32)}`);
    // Underfunded on the pre-check, funded once openSession lands.
    const getBalance = vi
      .fn()
      .mockResolvedValueOnce(0n)
      .mockResolvedValueOnce(0n)
      .mockResolvedValue(STAKE + EVM_SESSION_GAS_BUFFER_L2_WEI);
    const readContract = vi.fn();

    await setupEvmSessionIfNeeded(wallet, key, CONTRACT, STAKE, CAIP2, {
      readClient: {
        getBalance,
        waitForTransactionReceipt: vi.fn().mockResolvedValue({ status: "success" }),
        readContract,
      },
      chain: SELECTED_CHAIN,
      nowSecs: () => NOW,
    });

    // It must open regardless, so spending an RPC round-trip to confirm what it
    // already knows is waste on the common first-time path.
    // Underfunded means "fund regardless", so liveness is irrelevant to the
    // DECISION and reading it would be a wasted round-trip. The one permitted
    // call is the post-funding diagnostic, which logs whether the address the
    // contract records matches the key we sign with — the data that was missing
    // when a session appeared funded at one address and broke at another.
    expect(readContract.mock.calls.length).toBeLessThanOrEqual(1);
  });
});

describe("evmSessionRecordIsLive", () => {
  const ADDR = "0x70D922d213235a78c7a5c66d7F56Cb7226339433";

  it("rejects a session that expires DURING a game, not just an expired one", () => {
    // The contract only asks "valid now"; the client must ask "valid until the
    // game ends", or commit/reveal reverts with the stake already locked.
    expect(
      evmSessionRecordIsLive(
        { sessionKey: ADDR, expiry: BigInt(NOW + 90) },
        ADDR,
        NOW,
      ),
    ).toBe(false);
    expect(
      evmSessionRecordIsLive(
        { sessionKey: ADDR, expiry: BigInt(NOW + SESSION_MIN_REMAINING_SECS + 60) },
        ADDR,
        NOW,
      ),
    ).toBe(true);
  });

  it("compares addresses case-insensitively", () => {
    // Two checksummed sources; a raw === on checksummed addresses is precisely
    // the bug that made the e2e harness verify the wrong player.
    expect(
      evmSessionRecordIsLive(
        { sessionKey: ADDR.toLowerCase(), expiry: BigInt(NOW + 24 * 3600) },
        ADDR,
        NOW,
      ),
    ).toBe(true);
  });

  it("rejects the zero address (never-registered wallet)", () => {
    expect(
      evmSessionRecordIsLive(
        { sessionKey: `0x${"00".repeat(20)}`, expiry: 0n },
        ADDR,
        NOW,
      ),
    ).toBe(false);
  });
});

describe("waitForSessionFunding — a receipt is not visibility", () => {
  it("returns once a lagging provider finally reports the funding", async () => {
    // The read client is a viem `fallback`: the first reads can hit a node that
    // has not seen the funding block yet. Observed live on Base mainnet —
    // openSession funded the session key with 0.003 ETH and createGame still
    // failed with "have 0" moments later.
    const getBalance = vi
      .fn()
      .mockResolvedValueOnce(0n)
      .mockResolvedValueOnce(0n)
      .mockResolvedValue(3_000_000_000_000_000n);

    await expect(
      waitForSessionFunding({ getBalance } as never, `0x${"ab".repeat(20)}`, 3_000_000_000_000_000n),
    ).resolves.toBeUndefined();
    expect(getBalance.mock.calls.length).toBeGreaterThan(2);
  });

  it("throws rather than sending a transaction that must revert", async () => {
    // Never becoming visible is a real failure. Proceeding anyway is what
    // produced the confusing "exceeds the balance of the account" from a wallet
    // that had just paid.
    const getBalance = vi.fn().mockResolvedValue(0n);
    await expect(
      waitForSessionFunding({ getBalance } as never, `0x${"cd".repeat(20)}`, 1n),
    ).rejects.toThrow(/still below/i);
  }, 20_000);
});

const KEY_FOR_SKIP = generateEvmSessionKey();

describe("skip path re-confirms against the settled balance", () => {
  it("funds when pending looks sufficient but settled does not", async () => {
    // The failure this closes: `conservativeBalance` takes the lower of two
    // reads, but both come from the SAME viem fallback client — routed to a
    // lagging node twice, they agree and the minimum is still stale-HIGH. The
    // skip fired, and because that branch returns before waitForSessionFunding,
    // nothing re-checked before the stake went out. Observed on Base Sepolia:
    // session holding exactly the 0.0003 gas buffer while createGame sent
    // 0.0032, with the funding wallet sitting on 0.0239.
    // `need` is the SESSION's requirement — gas only since the stake is escrowed.
    const need = EVM_SESSION_GAS_BUFFER_L2_WEI;
    // A stale-LOW settled reading has to be genuinely below `need`, or the skip
    // fires and the barrier this test exists for is never reached.
    const staleLow = need / 2n;
    // `pending` reads stale-HIGH (the bug); `latest` starts low and rises once
    // openSession lands — the barrier polls `latest`, so it must be allowed to
    // succeed or the test hangs rather than asserting.
    let latestReads = 0;
    const getBalance = vi.fn(({ blockTag }: { blockTag?: string }) => {
      if (blockTag !== "latest") return Promise.resolve(need);
      latestReads += 1;
      return Promise.resolve(
        latestReads <= 2 ? staleLow : need,
      );
    });
    const readContract = vi
      .fn()
      .mockResolvedValue([evmSessionAddress(KEY_FOR_SKIP), BigInt(NOW + 86_400)]);
    const txHash = `0x${"ee".repeat(32)}`;
    const wallet = mockWalletClient(txHash);

    const res = await setupEvmSessionIfNeeded(wallet, KEY_FOR_SKIP, CONTRACT, STAKE, CAIP2, {
      readClient: {
        getBalance,
        waitForTransactionReceipt: vi.fn().mockResolvedValue({ status: "success" }),
        readContract,
      },
      chain: SELECTED_CHAIN,
      nowSecs: () => NOW,
    });

    // It must NOT have skipped: a stale-high pending read cannot authorize
    // sending a stake the session cannot cover.
    expect(res).toBe(txHash);
  });
});
