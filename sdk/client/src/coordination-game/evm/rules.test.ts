import { describe, it, expect } from "vitest";
import {
  revealMatchupArg,
  timeoutDeadline,
  timeoutClaimableAt,
  deriveResumeState,
  matchIsOnConnectedChain,
  evmGameResult,
  MATCHUP_UNSET,
  ZERO_BYTES32,
  STATUS_ACTIVE,
  STATUS_COMMITTING,
  STATUS_REVEALING,
  type EvmGameTimes,
  type EvmResumeGame,
} from "./rules.js";
import type { Hex } from "viem";

const R_MATCHUP = `0x${"ab".repeat(32)}` as Hex;

describe("revealMatchupArg (reveal-order — the CertMismatch bug)", () => {
  it("FIRST revealer (matchup unbound) passes the real r_matchup to bind it", () => {
    expect(revealMatchupArg(MATCHUP_UNSET, R_MATCHUP)).toBe(R_MATCHUP);
  });

  it("SECOND revealer (matchup already bound) MUST pass the zero sentinel", () => {
    // This is the exact case that reverted: opponent revealed first (bound
    // matchupType to 0 or 1), so we're second and must NOT re-pass r_matchup.
    expect(revealMatchupArg(0, R_MATCHUP)).toBe(ZERO_BYTES32);
    expect(revealMatchupArg(1, R_MATCHUP)).toBe(ZERO_BYTES32);
  });

  it("does not depend on creator/joiner role — only on-chain binding state", () => {
    // Same inputs, whoever we are: bound → zero, unbound → r_matchup.
    expect(revealMatchupArg(1, R_MATCHUP)).toBe(ZERO_BYTES32);
    expect(revealMatchupArg(MATCHUP_UNSET, R_MATCHUP)).toBe(R_MATCHUP);
  });

  it("first revealer without r_matchup yet returns null (not ready)", () => {
    expect(revealMatchupArg(MATCHUP_UNSET, null)).toBeNull();
  });
});

function times(over: Partial<EvmGameTimes>): EvmGameTimes {
  return {
    status: STATUS_REVEALING,
    activatedAt: 1000n,
    firstCommitAt: 1100n,
    bothCommitAt: 1200n,
    commitWindowSecs: 300n,
    revealWindowSecs: 600n,
    ...over,
  };
}

describe("timeoutDeadline (mirrors resolveTimeout stage anchors)", () => {
  it("Active times out on join time + commit window", () => {
    expect(timeoutDeadline(times({ status: STATUS_ACTIVE }))).toBe(1000n + 300n);
  });
  it("Committing times out on first-commit time + commit window", () => {
    expect(timeoutDeadline(times({ status: STATUS_COMMITTING }))).toBe(1100n + 300n);
  });
  it("Revealing times out on both-commit time + reveal window", () => {
    expect(timeoutDeadline(times({ status: STATUS_REVEALING }))).toBe(1200n + 600n);
  });
  it("non-timeout statuses (Pending/Resolved) have no deadline", () => {
    expect(timeoutDeadline(times({ status: 1 }))).toBeNull(); // Pending
    expect(timeoutDeadline(times({ status: 5 }))).toBeNull(); // Resolved
  });
});

describe("timeoutClaimableAt (stranded-stake recovery gate)", () => {
  const g = times({ status: STATUS_REVEALING }); // deadline = 1800

  it("not claimable before the window elapses", () => {
    expect(timeoutClaimableAt(g, 1799n)).toBe(false);
  });
  it("claimable exactly at and after the deadline", () => {
    expect(timeoutClaimableAt(g, 1800n)).toBe(true);
    expect(timeoutClaimableAt(g, 5000n)).toBe(true);
  });
  it("never claimable for a status with no timeout", () => {
    expect(timeoutClaimableAt(times({ status: 5 }), 9_999_999n)).toBe(false);
  });
});

describe("deriveResumeState (refresh recovery)", () => {
  const ME = "0xaAaAAaAaaAAAAaAAAAaAAaaaAaAAaaaAAAaAAaaa";
  const OPP = "0xBbBbbBBbBBBBbBBBBbbBBbbbbbBBbBBBBBbBBBbb";
  const R = `0x${"11".repeat(32)}` as Hex;

  function game(over: Partial<EvmResumeGame>): EvmResumeGame {
    return {
      status: STATUS_REVEALING,
      player1: ME,
      player2: OPP,
      p1Commit: R,
      p2Commit: R,
      p1Guess: 255, // unrevealed
      p2Guess: 255,
      ...over,
    };
  }

  it("resumes a committed player who hasn't revealed into 'committed'", () => {
    expect(deriveResumeState(game({}), ME)).toEqual({
      phase: "committed",
      bothCommitted: true,
    });
  });

  it("resumes a staked player who hasn't committed into 'chat'", () => {
    const g = game({ status: STATUS_ACTIVE, p1Commit: ZERO_BYTES32, p2Commit: ZERO_BYTES32 });
    expect(deriveResumeState(g, ME)).toEqual({ phase: "chat", bothCommitted: false });
  });

  it("reflects the opponent not having committed yet (bothCommitted false)", () => {
    const g = game({ status: STATUS_COMMITTING, p2Commit: ZERO_BYTES32 });
    expect(deriveResumeState(g, ME)).toEqual({ phase: "committed", bothCommitted: false });
  });

  it("does NOT resume once we already revealed (game resolves + sweeps on its own)", () => {
    expect(deriveResumeState(game({ p1Guess: 1 }), ME)).toBeNull();
  });

  it("does NOT resume a resolved or pending game", () => {
    expect(deriveResumeState(game({ status: 5 }), ME)).toBeNull(); // Resolved
    expect(deriveResumeState(game({ status: 1 }), ME)).toBeNull(); // Pending
  });

  it("does NOT resume when we're not a participant", () => {
    expect(deriveResumeState(game({}), "0xcCcccCCCCccCCCCCcCccCcccCcCCCCcCcccccCccC")).toBeNull();
  });

  it("works from the joiner's (P2) perspective too", () => {
    const g = game({ p1Guess: 255, p2Guess: 255, p2Commit: R });
    expect(deriveResumeState(g, OPP)).toEqual({ phase: "committed", bothCommitted: true });
  });
});

// A stale match from ANOTHER chain must not be resumed. Observed on both
// mainnet e2e runs (2026-08-13): connected to eip155:1, the resume path read
// the BASE SEPOLIA proxy 0x4fbbceb9… because it took the contract from the
// match record but the client from the connected chain. It failed safe — the
// read returned "0x" and only warned — but the guard belongs here, not in luck.
describe("matchIsOnConnectedChain", () => {
  it("resumes a match on the chain we are actually connected to", () => {
    expect(matchIsOnConnectedChain("eip155:1", 1)).toBe(true);
    expect(matchIsOnConnectedChain("eip155:8453", 8453)).toBe(true);
  });

  it("REFUSES a match from a different chain (the observed mainnet case)", () => {
    // Base Sepolia match while connected to Ethereum mainnet.
    expect(matchIsOnConnectedChain("eip155:84532", 1)).toBe(false);
    // Base mainnet match while connected to Base Sepolia.
    expect(matchIsOnConnectedChain("eip155:8453", 84532)).toBe(false);
  });

  it("REFUSES a non-EVM (Solana) match rather than trying to parse it", () => {
    expect(matchIsOnConnectedChain("solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp", 1)).toBe(false);
  });

  it("REFUSES a missing or malformed chain instead of defaulting to allow", () => {
    expect(matchIsOnConnectedChain(undefined, 1)).toBe(false);
    expect(matchIsOnConnectedChain("", 1)).toBe(false);
    expect(matchIsOnConnectedChain("eip155:", 1)).toBe(false);
    expect(matchIsOnConnectedChain("1", 1)).toBe(false);
  });

  it("does not match on a numeric prefix (eip155:1 must not satisfy chain 11)", () => {
    expect(matchIsOnConnectedChain("eip155:1", 11)).toBe(false);
    expect(matchIsOnConnectedChain("eip155:11", 1)).toBe(false);
  });
});

// The EVM end-screen showed no outcome at all — just "the game settles
// on-chain". Solana has always told players whether they won. Reported
// 2026-08-13 after a real Base game.
describe("evmGameResult", () => {
  const P1 = "0x1111111111111111111111111111111111111111";
  const P2 = "0x2222222222222222222222222222222222222222";
  const base = { player1: P1, player2: P2, p1Guess: 1, p2Guess: 0, matchupType: 1, firstCommitter: 1 };

  it("describes the game from P1's seat", () => {
    expect(evmGameResult(base, P1)).toEqual({
      myGuess: 1, opponentGuess: 0, matchupType: 1, iAmP1: true, firstCommitter: 1,
    });
  });

  it("flips the seat for P2 — my guess must be MY guess", () => {
    expect(evmGameResult(base, P2)).toEqual({
      myGuess: 0, opponentGuess: 1, matchupType: 1, iAmP1: false, firstCommitter: 1,
    });
  });

  it("matches the address case-insensitively (checksummed vs lowercase)", () => {
    expect(evmGameResult(base, P1.toUpperCase().replace("0X", "0x"))?.iAmP1).toBe(true);
  });

  it("refuses to state an outcome that is not yet true", () => {
    expect(evmGameResult({ ...base, p1Guess: 255 }, P1)).toBeNull();   // unrevealed
    expect(evmGameResult({ ...base, p2Guess: 255 }, P1)).toBeNull();   // opponent unrevealed
    expect(evmGameResult({ ...base, matchupType: 255 }, P1)).toBeNull(); // matchup unbound
  });

  it("returns null for someone who is not in the game", () => {
    expect(evmGameResult(base, "0x9999999999999999999999999999999999999999")).toBeNull();
  });
});
