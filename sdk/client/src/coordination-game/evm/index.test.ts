import http from "node:http";
import type { AddressInfo } from "node:net";
import { describe, expect, it } from "vitest";
import { decodeFunctionData, keccak256, parseTransaction, toBytes, toHex } from "viem";
import { baseSepolia } from "viem/chains";
import { CoordinationGameApiClient, type EvmMatchPayload } from "../api.js";
import {
  buildEvmFundCall,
  buildEvmLockTrancheCall,
  buildEvmSettleCall,
  certTupleFromMatchPayload,
  clearEvmGuess,
  conservativeBalance,
  CROSS_CHAIN_GAME_ABI,
  deriveResumeState,
  EVM_SESSION_STORAGE_KEY,
  evmGameResult,
  evmSessionAddress,
  findCrossChainMatch,
  findEvmMatch,
  generateEvmSessionKey,
  getStoredEvmSession,
  loadEvmGuess,
  loadSessionKey,
  normalizeEvmSigV,
  persistEvmGuess,
  revealMatchupArg,
  saveSessionKey,
  storeEvmSession,
  SWEEP_GAS_LIMIT,
  sweepEvmSession,
  sweepValueWei,
  timeoutClaimableAt,
  unpadAddress,
  verifyEvmFundTarget,
  ZERO_BYTES32,
  type SessionStorageLike,
  type XMatchPayload,
} from "./index.js";

class MemoryStorage implements SessionStorageLike {
  readonly values = new Map<string, string>();
  getItem(key: string) { return this.values.get(key) ?? null; }
  setItem(key: string, value: string) { this.values.set(key, value); }
  removeItem(key: string) { this.values.delete(key); }
}

class ThrowingStorage implements SessionStorageLike {
  getItem(_key: string): string | null { throw new Error("SecurityError: storage disabled"); }
  setItem(_key: string, _value: string): void { throw new Error("SecurityError: storage disabled"); }
  removeItem(_key: string): void { throw new Error("SecurityError: storage disabled"); }
}

const relaySig0 = `0x${"aa".repeat(64)}00`;
const relaySig1 = `0x${"bb".repeat(64)}01`;
const evmSig27 = `0x${"cc".repeat(64)}1b`;

const match: XMatchPayload = {
  match_id: `0x${"11".repeat(32)}`,
  tournament_id: 1,
  matchup_commitment: `0x${"99".repeat(32)}`,
  match_live_digest: `0x${"22".repeat(32)}`,
  operator_signature: relaySig0,
  create_match_operator_sig: relaySig0,
  quote_timestamp: 1_765_000_000,
  quote_max_age_secs: 600,
  fund_deadline: 10,
  match_deadline: 20,
  claim_window_secs: 30,
  a_is_p1: 1,
  leg_a: { chain: "solana:x", contract: `0x${"00".repeat(32)}`, player: `0x${"00".repeat(32)}`, session_key: `0x${"00".repeat(12)}${"12".repeat(20)}`, stake_base_units: "1", tranche_base_units: "1" },
  leg_b: { chain: "eip155:84532", contract: `0x${"00".repeat(12)}${"34".repeat(20)}`, player: `0x${"00".repeat(12)}${"56".repeat(20)}`, session_key: `0x${"00".repeat(12)}${"78".repeat(20)}`, stake_base_units: "3200", tranche_base_units: "1" },
};

describe("coordination-game EVM", () => {
  it("ports reveal, timeout, resume, and result rules", () => {
    expect(revealMatchupArg(0, `0x${"ff".repeat(32)}`)).toBe(ZERO_BYTES32);
    expect(timeoutClaimableAt({ status: 2, activatedAt: 10n, firstCommitAt: 0n, bothCommitAt: 0n, commitWindowSecs: 5n, revealWindowSecs: 5n }, 15n)).toBe(true);
    expect(deriveResumeState({ status: 4, player1: "0xAA", player2: "0xbb", p1Commit: `0x${"01".repeat(32)}`, p2Commit: `0x${"01".repeat(32)}`, p1Guess: 255, p2Guess: 0 }, "0xaa")).toEqual({ phase: "committed", bothCommitted: true });
    expect(evmGameResult({ player1: "0xaa", player2: "0xbb", p1Guess: 1, p2Guess: 0, matchupType: 1, firstCommitter: 1 }, "0xbb")).toMatchObject({ myGuess: 0, opponentGuess: 1, iAmP1: false });
  });

  it("stores sessions only through injected storage and clocks", () => {
    const storage = new MemoryStorage();
    const key = `0x${"01".repeat(32)}` as const;
    storeEvmSession(storage, key, "0xabc", () => 100);
    expect(getStoredEvmSession(storage, "0xABC", () => 101)).toBe(key);
    expect(getStoredEvmSession(storage, "0xdef", () => 101)).toBeNull();
  });

  it("persists EVM session keys and pending guesses across simulated tab close, migrates legacy sessionStorage keys, and falls back when localStorage throws (P1-5)", () => {
    const localStorage = new MemoryStorage();
    const sessionStorage = new MemoryStorage();
    const wallet = "0xAbCdEf0000000000000000000000000000001234";
    const sessionKey = generateEvmSessionKey();

    saveSessionKey(sessionKey, wallet, { localStorage, sessionStorage, now: () => 1_000 });
    expect(localStorage.getItem(EVM_SESSION_STORAGE_KEY)).toBeTruthy();
    expect(sessionStorage.getItem(EVM_SESSION_STORAGE_KEY)).toBeTruthy();

    // Simulate browser tab close + reopen (sessionStorage wiped, localStorage survives; >24h elapsed).
    sessionStorage.values.clear();
    expect(sessionStorage.getItem(EVM_SESSION_STORAGE_KEY)).toBeNull();

    const sevenDaysLater = 1_000 + 7 * 86_400_000;
    expect(
      getStoredEvmSession({ localStorage, sessionStorage }, wallet.toLowerCase(), () => sevenDaysLater),
    ).toBe(sessionKey);
    expect(loadSessionKey(wallet, { localStorage, sessionStorage })).toBe(sessionKey);
    // Re-mirrored back into the new tab's sessionStorage.
    expect(sessionStorage.getItem(EVM_SESSION_STORAGE_KEY)).toBeTruthy();

    // Legacy migration: key originally written only to sessionStorage is migrated to localStorage on read.
    const legacyLocal = new MemoryStorage();
    const legacySession = new MemoryStorage();
    const legacyKey = generateEvmSessionKey();
    legacySession.setItem(
      EVM_SESSION_STORAGE_KEY,
      JSON.stringify({ privateKey: legacyKey, createdAt: 500, wallet }),
    );
    expect(loadSessionKey(wallet, { localStorage: legacyLocal, sessionStorage: legacySession })).toBe(legacyKey);
    expect(legacyLocal.getItem(EVM_SESSION_STORAGE_KEY)).toBeTruthy();
    legacySession.values.clear();
    expect(loadSessionKey(wallet, { localStorage: legacyLocal, sessionStorage: legacySession })).toBe(legacyKey);

    // Fallback when localStorage throws SecurityError (e.g. strict privacy mode).
    const fallbackSession = new MemoryStorage();
    const fallbackPair = { localStorage: new ThrowingStorage(), sessionStorage: fallbackSession };
    saveSessionKey(sessionKey, wallet, fallbackPair);
    expect(loadSessionKey(wallet, fallbackPair)).toBe(sessionKey);

    // Committed guess preimage also survives simulated tab close.
    const preimage = new Uint8Array(32).fill(0x5a);
    persistEvmGuess({ localStorage, sessionStorage }, "game-77", preimage);
    sessionStorage.values.clear();
    expect(loadEvmGuess({ localStorage, sessionStorage }, "game-77")).toEqual(preimage);
    clearEvmGuess({ localStorage, sessionStorage });
    expect(loadEvmGuess({ localStorage, sessionStorage }, "game-77")).toBeNull();
  });

  it("retains EVM session keys in localStorage until on-chain ETH balance is swept to 0 over real JSON-RPC (P1-5)", async () => {
    const localStorage = new MemoryStorage();
    const sessionStorage = new MemoryStorage();
    const storagePair = { localStorage, sessionStorage };
    const mainWallet = "0x1111111111111111111111111111111111111111" as const;
    const sessionKey = generateEvmSessionKey();
    const sessionAddr = evmSessionAddress(sessionKey).toLowerCase();
    saveSessionKey(sessionKey, mainWallet, storagePair);

    const maxFeePerGas = 1_000_000_000n; // 1 gwei
    let currentBalance = 1_000_000_000_000_000n; // 0.001 ETH
    let nextReceiptStatus: "0x1" | "0x0" = "0x1";
    const broadcastTxs: ReturnType<typeof parseTransaction>[] = [];

    const rpcServer = http.createServer(async (req, res) => {
      const chunks: Buffer[] = [];
      for await (const chunk of req) chunks.push(Buffer.from(chunk));
      const payload = JSON.parse(Buffer.concat(chunks).toString("utf8")) as
        | { id: number; method: string; params?: unknown[] }
        | Array<{ id: number; method: string; params?: unknown[] }>;

      const handleOne = (item: { id: number; method: string; params?: unknown[] }) => {
        switch (item.method) {
          case "eth_chainId":
            return { jsonrpc: "2.0", id: item.id, result: toHex(baseSepolia.id) };
          case "eth_getBalance": {
            const addr = String(item.params?.[0] ?? "").toLowerCase();
            const bal = addr === sessionAddr ? currentBalance : 0n;
            return { jsonrpc: "2.0", id: item.id, result: toHex(bal) };
          }
          case "eth_getBlockByNumber":
            return {
              jsonrpc: "2.0",
              id: item.id,
              result: {
                number: "0x10",
                hash: `0x${"aa".repeat(32)}`,
                parentHash: `0x${"00".repeat(32)}`,
                nonce: "0x0000000000000000",
                sha3Uncles: `0x${"00".repeat(32)}`,
                logsBloom: `0x${"00".repeat(256)}`,
                transactionsRoot: `0x${"00".repeat(32)}`,
                stateRoot: `0x${"00".repeat(32)}`,
                receiptsRoot: `0x${"00".repeat(32)}`,
                miner: mainWallet,
                difficulty: "0x0",
                totalDifficulty: "0x0",
                extraData: "0x",
                size: "0x100",
                gasLimit: toHex(30_000_000n),
                gasUsed: "0x0",
                timestamp: toHex(1_765_000_000n),
                transactions: [],
                uncles: [],
                baseFeePerGas: toHex(500_000_000n),
              },
            };
          case "eth_maxPriorityFeePerGas":
            return { jsonrpc: "2.0", id: item.id, result: "0x0" };
          case "eth_gasPrice":
            return { jsonrpc: "2.0", id: item.id, result: toHex(maxFeePerGas) };
          case "eth_getTransactionCount":
            return { jsonrpc: "2.0", id: item.id, result: toHex(BigInt(broadcastTxs.length)) };
          case "eth_sendRawTransaction": {
            const rawTx = String(item.params?.[0] ?? "") as `0x${string}`;
            const parsed = parseTransaction(rawTx);
            broadcastTxs.push(parsed);
            if (nextReceiptStatus === "0x1") {
              const txGasCost = SWEEP_GAS_LIMIT * (parsed.maxFeePerGas ?? maxFeePerGas);
              const deducted = (parsed.value ?? 0n) + txGasCost;
              currentBalance = currentBalance > deducted ? currentBalance - deducted : 0n;
            }
            return { jsonrpc: "2.0", id: item.id, result: keccak256(rawTx) };
          }
          case "eth_getTransactionReceipt": {
            const txHash = String(item.params?.[0] ?? `0x${"11".repeat(32)}`);
            return {
              jsonrpc: "2.0",
              id: item.id,
              result: {
                transactionHash: txHash,
                transactionIndex: "0x0",
                blockHash: `0x${"aa".repeat(32)}`,
                blockNumber: "0x11",
                from: sessionAddr,
                to: mainWallet,
                cumulativeGasUsed: toHex(SWEEP_GAS_LIMIT),
                gasUsed: toHex(SWEEP_GAS_LIMIT),
                contractAddress: null,
                logs: [],
                logsBloom: `0x${"00".repeat(256)}`,
                status: nextReceiptStatus,
                effectiveGasPrice: toHex(maxFeePerGas),
                type: "0x2",
              },
            };
          }
          default:
            return {
              jsonrpc: "2.0",
              id: item.id,
              error: { code: -32601, message: `Method not supported: ${item.method}` },
            };
        }
      };

      const bodyOut = Array.isArray(payload) ? payload.map(handleOne) : handleOne(payload);
      res.writeHead(200, { "content-type": "application/json" });
      res.end(JSON.stringify(bodyOut));
    });

    await new Promise<void>((resolve) => rpcServer.listen(0, "127.0.0.1", () => resolve()));
    const rpcUrl = `http://127.0.0.1:${(rpcServer.address() as AddressInfo).port}`;

    try {
      // 1. Reverted sweep MUST throw and retain the session key in storage.
      nextReceiptStatus = "0x0";
      await expect(
        sweepEvmSession(sessionKey, mainWallet, baseSepolia, [rpcUrl], { storage: storagePair }),
      ).rejects.toThrow(/reverted/);
      expect(loadSessionKey(mainWallet, storagePair)).toBe(sessionKey);

      // 2. Conservative sweep leaves dust (> 0n): session key MUST still be retained.
      nextReceiptStatus = "0x1";
      const estimatedMaxFeePerGas = 600_000_000n; // viem EIP-1559: 1.2 * 500_000_000n baseFee + 0 priorityFee
      const expectedConservative = sweepValueWei(currentBalance, estimatedMaxFeePerGas);
      const sweptConservative = await sweepEvmSession(
        sessionKey,
        mainWallet,
        baseSepolia,
        [rpcUrl],
        { storage: storagePair },
      );
      expect(sweptConservative).toBe(expectedConservative);
      expect(currentBalance).toBeGreaterThan(0n);
      expect(loadSessionKey(mainWallet, storagePair)).toBe(sessionKey);

      // 3. Exact sweep drains funded balance to 0n: storage MUST now be cleared in both stores.
      currentBalance = 500_000_000_000_000n;
      const sweptExact = await sweepEvmSession(
        sessionKey,
        mainWallet,
        baseSepolia,
        [rpcUrl],
        { storage: storagePair, exact: true },
      );
      expect(sweptExact).toBeGreaterThan(0n);
      expect(currentBalance).toBe(0n);
      expect(loadSessionKey(mainWallet, storagePair)).toBeNull();
      expect(localStorage.getItem(EVM_SESSION_STORAGE_KEY)).toBeNull();
      expect(sessionStorage.getItem(EVM_SESSION_STORAGE_KEY)).toBeNull();
      expect(broadcastTxs.at(-1)?.to?.toLowerCase()).toBe(mainWallet.toLowerCase());
    } finally {
      await new Promise<void>((resolve) => rpcServer.close(() => resolve()));
    }
  });

  it("forwards optional token and isAi in findCrossChainMatch and findEvmMatch (P2-8)", async () => {
    const requests: Array<{
      method: string;
      url: string;
      auth: string | undefined;
      body: Record<string, unknown> | null;
    }> = [];

    const evmMatch: EvmMatchPayload = {
      game_id: "77",
      matchup_commitment: `0x${"88".repeat(32)}`,
      chain: "eip155:84532",
      contract: `0x${"34".repeat(20)}`,
      stake_base_units: "3200",
      create_call: { player: `0x${"11".repeat(20)}`, to: `0x${"34".repeat(20)}`, data: "0x01", value_wei: "3200" },
      join_call: { player: `0x${"22".repeat(20)}`, to: `0x${"34".repeat(20)}`, data: "0x02", value_wei: "3200" },
    };

    const server = http.createServer(async (req, res) => {
      const chunks: Buffer[] = [];
      for await (const chunk of req) chunks.push(Buffer.from(chunk));
      const raw = Buffer.concat(chunks).toString("utf8");
      const parsedBody = raw ? (JSON.parse(raw) as Record<string, unknown>) : null;
      requests.push({
        method: req.method ?? "GET",
        url: req.url ?? "/",
        auth: req.headers.authorization,
        body: parsedBody,
      });

      res.writeHead(200, { "content-type": "application/json" });
      if (req.url === "/internal/xqueue/join") {
        res.end(JSON.stringify({ status: "waiting", poll_handle: "x-handle-1" }));
      } else if (req.url?.startsWith("/internal/xqueue/status")) {
        res.end(JSON.stringify({ status: "matched", match, poll_handle: "x-handle-1" }));
      } else if (req.url === "/internal/evmgame/join") {
        res.end(JSON.stringify({ status: "waiting", session_id: "evm-sess-1" }));
      } else if (req.url?.startsWith("/internal/evmgame/status")) {
        res.end(JSON.stringify({ status: "matched", match: evmMatch, session_id: "evm-sess-1" }));
      } else {
        res.end(JSON.stringify({ ok: true }));
      }
    });

    await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", () => resolve()));
    const baseUrl = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;

    try {
      const api = new CoordinationGameApiClient({ baseUrl });
      const xRes = await findCrossChainMatch(
        api,
        {
          wallet: "0x1111111111111111111111111111111111111111",
          sessionKeyAddress: "0x2222222222222222222222222222222222222222",
          tournamentId: 1,
          chain: "eip155:84532",
          isAi: true,
          token: "jwt-cross-chain",
        },
        { pollMs: 1, maxPolls: 3, sleep: async () => {} },
      );
      expect(xRes.handle).toBe("x-handle-1");
      expect(xRes.match.match_id).toBe(match.match_id);

      const evmRes = await findEvmMatch(
        api,
        {
          wallet: "0x2222222222222222222222222222222222222222",
          playerWallet: "0x1111111111111111111111111111111111111111",
          tournamentId: 2,
          chain: "eip155:84532",
          isAi: true,
          token: "jwt-evm-game",
        },
        { pollMs: 1, maxPolls: 3, sleep: async () => {} },
      );
      expect(evmRes.sessionId).toBe("evm-sess-1");
      expect(evmRes.match.game_id).toBe("77");

      expect(requests).toHaveLength(4);
      expect(requests[0]).toMatchObject({
        method: "POST",
        url: "/internal/xqueue/join",
        auth: "Bearer jwt-cross-chain",
        body: {
          wallet: "0x1111111111111111111111111111111111111111",
          session_key: "0x2222222222222222222222222222222222222222",
          tournament_id: 1,
          chain: "eip155:84532",
          is_ai: true,
        },
      });
      expect(requests[0]?.body).not.toHaveProperty("isAi");
      expect(requests[1]).toMatchObject({
        method: "GET",
        url: "/internal/xqueue/status?handle=x-handle-1",
        auth: "Bearer jwt-cross-chain",
      });

      expect(requests[2]).toMatchObject({
        method: "POST",
        url: "/internal/evmgame/join",
        auth: "Bearer jwt-evm-game",
        body: {
          wallet: "0x2222222222222222222222222222222222222222",
          player_wallet: "0x1111111111111111111111111111111111111111",
          tournament_id: 2,
          chain: "eip155:84532",
          is_ai: true,
        },
      });
      expect(requests[2]?.body).not.toHaveProperty("isAi");
      expect(requests[3]).toMatchObject({
        method: "GET",
        url: "/internal/evmgame/status?wallet=0x2222222222222222222222222222222222222222",
        auth: "Bearer jwt-evm-game",
      });
    } finally {
      await new Promise<void>((resolve) => server.close(() => resolve()));
    }
  });

  it("pins and quorum-validates cross-chain funding", async () => {
    const pin = { contract: `0x${"34".repeat(20)}` as const, stakeWei: 3200n };
    expect(buildEvmFundCall(match, pin)).toMatchObject({ to: pin.contract, value: 3200n });
    await expect(verifyEvmFundTarget(match, pin, [
      { stakeWei: async () => 3200n, matchStatus: async () => 0 },
      { stakeWei: async () => 3200n, matchStatus: async () => 0 },
    ])).resolves.toBeUndefined();
    await expect(verifyEvmFundTarget(match, pin, [
      { stakeWei: async () => 3200n, matchStatus: async () => 0 },
      { stakeWei: async () => 3201n, matchStatus: async () => 0 },
    ])).rejects.toThrow(/disagree/);
    expect(conservativeBalance([5n, 3n, 4n])).toBe(3n);
  });

  it("reconstructs cert tuples, normalizes v, and encodes lockTranche and settle", () => {
    const pin = { contract: `0x${"34".repeat(20)}` as const, stakeWei: 3200n };
    expect(normalizeEvmSigV(relaySig0)).toBe(`0x${"aa".repeat(64)}1b`);
    expect(normalizeEvmSigV(relaySig1)).toBe(`0x${"bb".repeat(64)}1c`);
    expect(normalizeEvmSigV(evmSig27)).toBe(evmSig27);

    const cert = certTupleFromMatchPayload(match);
    expect(cert.legA.chainTag).toBe(keccak256(toBytes("solana:x")));
    expect(cert.legB.chainTag).toBe(keccak256(toBytes("eip155:84532")));
    expect(cert.legB.sessionKey).toBe(`0x${"78".repeat(20)}`);

    const lockCall = buildEvmLockTrancheCall(match, pin);
    expect(lockCall).toMatchObject({ to: pin.contract, value: 0n });
    const decodedLock = decodeFunctionData({
      abi: CROSS_CHAIN_GAME_ABI,
      data: lockCall.data,
    });
    expect(decodedLock.functionName).toBe("lockTranche");
    expect(decodedLock.args[1]).toBe(`0x${"aa".repeat(64)}1b`);

    const bundle = {
      outcome: {
        match_id: match.match_id,
        match_live_digest: match.match_live_digest,
        outcome_kind: 4,
        step_count: 4,
        p1_guess: 1,
        p2_guess: 0,
        first_committer: 1,
        matchup_type: 1,
        transcript_hash: `0x${"dd".repeat(32)}`,
      },
      live_sigs: [relaySig0, relaySig1, evmSig27] as const,
      oc_sigs: [relaySig1, relaySig0, evmSig27] as const,
    };

    const settleCall = buildEvmSettleCall(match, bundle, pin);
    expect(settleCall).toMatchObject({ to: pin.contract, value: 0n });
    const decodedSettle = decodeFunctionData({
      abi: CROSS_CHAIN_GAME_ABI,
      data: settleCall.data,
    });
    expect(decodedSettle.functionName).toBe("settle");
    expect(decodedSettle.args[2]).toEqual([
      `0x${"aa".repeat(64)}1b`,
      `0x${"bb".repeat(64)}1c`,
      evmSig27,
    ]);

    expect(() =>
      buildEvmSettleCall(
        match,
        {
          ...bundle,
          outcome: { ...bundle.outcome, match_id: `0x${"ff".repeat(32)}` },
        },
        pin,
      ),
    ).toThrow(/match_id/);
  });

  it("P1-17: pins sessionKey, caps deadlines, validates unpadAddress, and normalizes create_match_operator_sig", () => {
    const pin = {
      contract: `0x${"34".repeat(20)}` as const,
      stakeWei: 3200n,
      sessionKey: `0x${"78".repeat(20)}` as const,
      nowSec: 1_000,
    };

    // Valid call normalizes relay v=00 -> 0x1b inside createMatch calldata!
    const validCall = buildEvmFundCall(match, pin);
    const decoded = decodeFunctionData({
      abi: CROSS_CHAIN_GAME_ABI,
      data: validCall.data,
    });
    expect(decoded.functionName).toBe("createMatch");
    expect(decoded.args[1]).toBe(`0x${"78".repeat(20)}`);
    expect(decoded.args[6]).toBe(`0x${"aa".repeat(64)}1b`);

    // 1. Mismatched leg_b.session_key MUST reject when pin.sessionKey is provided
    expect(() =>
      buildEvmFundCall(
        {
          ...match,
          leg_b: { ...match.leg_b, session_key: `0x${"00".repeat(12)}${"99".repeat(20)}` },
        },
        pin,
      ),
    ).toThrow(/session_key/);

    // 2. Far-future (>7d) or inverted or zero deadlines MUST reject
    expect(() =>
      buildEvmFundCall(
        { ...match, fund_deadline: 1_000 + 8 * 86_400, match_deadline: 1_000 + 9 * 86_400 },
        pin,
      ),
    ).toThrow(/7-day horizon/);
    expect(() =>
      buildEvmFundCall({ ...match, fund_deadline: 200, match_deadline: 100 }, pin),
    ).toThrow(/match_deadline.*fund_deadline/);
    expect(() =>
      buildEvmFundCall({ ...match, fund_deadline: 0, match_deadline: 20 }, pin),
    ).toThrow(/must be > 0/);

    // 3. Invalid / non-zero-padded / zero addresses in unpadAddress MUST reject
    expect(() => unpadAddress("0x1234")).toThrow(/invalid EVM address/);
    expect(() => unpadAddress(`0x${"00".repeat(32)}`)).toThrow(/zero address/);
    expect(() => unpadAddress(`0x${"11".repeat(12)}${"34".repeat(20)}`)).toThrow(/non-zero high 12 bytes/);
    expect(() => unpadAddress(`0x${"zz".repeat(20)}`)).toThrow(/invalid EVM address/);
    expect(unpadAddress(`0x${"00".repeat(12)}${"Ab".repeat(20)}`)).toBe(`0x${"ab".repeat(20)}`);

    // 4. Truncated or invalid-v create_match_operator_sig MUST reject
    expect(() =>
      buildEvmFundCall({ ...match, create_match_operator_sig: "0x1234" }, pin),
    ).toThrow(/65-byte/);
    expect(() => normalizeEvmSigV(`0x${"aa".repeat(64)}02`)).toThrow(/recovery id/);
    expect(() => normalizeEvmSigV(`0x${"aa".repeat(64)}1d`)).toThrow(/recovery id/);
  });
});
