import { describe, expect, it } from "vitest";
import {
  buildEvmFundCall,
  conservativeBalance,
  deriveResumeState,
  evmGameResult,
  getStoredEvmSession,
  revealMatchupArg,
  storeEvmSession,
  timeoutClaimableAt,
  verifyEvmFundTarget,
  ZERO_BYTES32,
  type SessionStorageLike,
} from "./index.js";

class MemoryStorage implements SessionStorageLike {
  readonly values = new Map<string, string>();
  getItem(key: string) { return this.values.get(key) ?? null; }
  setItem(key: string, value: string) { this.values.set(key, value); }
  removeItem(key: string) { this.values.delete(key); }
}

const match = {
  match_id: `0x${"11".repeat(32)}`,
  tournament_id: 1,
  match_live_digest: `0x${"22".repeat(32)}`,
  operator_signature: "0x",
  create_match_operator_sig: "0x1234",
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
});
