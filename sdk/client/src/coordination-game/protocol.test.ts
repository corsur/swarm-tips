import { describe, expect, it } from "vitest";
import { Keypair } from "@solana/web3.js";
import {
  decodeGlobalConfig,
  decodeTournament,
  deriveSolanaResumeState,
  escrowPda,
  timeoutClaimableAtSlot,
  tournamentPda,
  toGuessBit,
  u64LE,
} from "./protocol.js";

describe("coordination-game protocol", () => {
  it("preserves canonical PDA and little-endian vectors", () => {
    expect(tournamentPda(1n)[0].toBase58()).toBe("3YMVKaBf8WoFEVtestnAPR1yv27YzCtYjZHmtEmXkpZM");
    expect(Array.from(u64LE(0x0102_0304_0506_0708n))).toEqual([8, 7, 6, 5, 4, 3, 2, 1]);
    expect(escrowPda(1n, Keypair.generate().publicKey)[0].toBytes()).toHaveLength(32);
  });

  it("decodes tournament and migrated global-config layouts", () => {
    const tournament = new Uint8Array(81);
    const tv = new DataView(tournament.buffer);
    tv.setBigUint64(8, 1n, true);
    tv.setBigInt64(48, 10n, true);
    tv.setBigInt64(56, 20n, true);
    tv.setBigUint64(64, 68_500_000n, true);
    tv.setBigUint64(72, 44n, true);
    tournament[80] = 1;
    expect(decodeTournament(tournament)).toMatchObject({ tournamentId: 1n, prizeLamports: 68_500_000n, gameCount: 44n, finalized: true });

    const config = new Uint8Array(115);
    const keys = [Keypair.generate().publicKey, Keypair.generate().publicKey, Keypair.generate().publicKey];
    config.set(keys[0].toBytes(), 8);
    config.set(keys[1].toBytes(), 40);
    config.set(keys[2].toBytes(), 72);
    config[104] = 0x10;
    config[105] = 0x27;
    new DataView(config.buffer).setBigUint64(107, 42n, true);
    expect(decodeGlobalConfig(config)).toMatchObject({ treasurySplitBps: 10_000, stakeLamports: 42n });
    expect(() => decodeGlobalConfig(new Uint8Array(107))).toThrow(/pre-migration/);
  });

  it("derives resume and timeout boundaries from chain state", () => {
    expect(deriveSolanaResumeState({ state: "revealing", playerOne: "me", playerTwo: "them", p1Commit: [1], p2Commit: [1], p1Guess: 255, p2Guess: 0 }, "me")).toEqual({ phase: "revealing", iAmP1: true, bothCommitted: true });
    expect(timeoutClaimableAtSlot({ state: "active", activatedAtSlot: 10n, p1CommitSlot: 0n, p2CommitSlot: 0n, commitTimeoutSlots: 5n }, 15n)).toBe(true);
    expect(toGuessBit(1)).toBe(1);
    expect(() => toGuessBit(2)).toThrow(/Expected 0 or 1/);
  });
});
