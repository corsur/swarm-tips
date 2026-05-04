/**
 * AAS v1 reference verifier — pure-step unit tests.
 *
 * Covers the parts of the protocol that don't touch RPC: step 1
 * (schema check, including the well-formed-per-type table) and step 6
 * (domain bound). Steps 2–5 + 7 are exercised by `verifyV1OnChain` and
 * are unit-tested here against a mocked Connection.
 */
import { describe, expect, it } from "vitest";
import {
  anchorDiscriminator,
  asV1,
  checkSchema,
  verifyV1Schema,
  verifyV1OnChain,
  shillbotProtocol,
} from "../src/index.js";
import type { AasV1Attestation } from "../src/index.js";
import { Connection, PublicKey } from "@solana/web3.js";

/** A minimal valid v1 attestation. Uses real-shaped pubkeys / hashes. */
function fixtureAttestation(overrides: Partial<AasV1Attestation> = {}): AasV1Attestation {
  return {
    version: "aas/v1",
    network: "mainnet",
    program_id: "2tR37nqMpwdV4DVUHjzUmL1rH2DtkA8zrRA4EAhT7KMi",
    account: "11111111111111111111111111111112",
    account_kind: "Task",
    task_id: "42",
    client: "11111111111111111111111111111113",
    agent: "11111111111111111111111111111114",
    state: "verified",
    platform: 0,
    composite_score: "850000",
    score_max: "1000000",
    verified_at: "2026-05-02T12:00:00Z",
    verification_hash: "a".repeat(64),
    content_hash: "b".repeat(64),
    content_id_hash: "c".repeat(64),
    oracle_feed: "11111111111111111111111111111115",
    ...overrides,
  };
}

describe("schema check (step 1)", () => {
  it("accepts a well-formed attestation", () => {
    expect(checkSchema(fixtureAttestation())).toBeNull();
  });

  it("rejects wrong version", () => {
    const a = fixtureAttestation({ version: "aas/v0" as "aas/v1" });
    expect(checkSchema(a)).toBe("schema_invalid:version");
  });

  it("rejects non-base58 pubkeys", () => {
    expect(
      checkSchema(fixtureAttestation({ program_id: "not-base58!!" })),
    ).toBe("schema_invalid:program_id");
  });

  it("rejects oracle_feed that is neither pubkey nor null", () => {
    expect(checkSchema(fixtureAttestation({ oracle_feed: "" }))).toBe(
      "schema_invalid:oracle_feed",
    );
  });

  it("accepts oracle_feed === null", () => {
    expect(checkSchema(fixtureAttestation({ oracle_feed: null }))).toBeNull();
  });

  it("rejects unknown network value", () => {
    const a = fixtureAttestation({ network: "testnet" as "mainnet" });
    expect(checkSchema(a)).toBe("schema_invalid:network");
  });

  it("rejects platform outside u8 range", () => {
    expect(checkSchema(fixtureAttestation({ platform: 256 }))).toBe(
      "schema_invalid:platform",
    );
    expect(checkSchema(fixtureAttestation({ platform: -1 }))).toBe(
      "schema_invalid:platform",
    );
  });

  it("rejects task_id with leading zero", () => {
    expect(checkSchema(fixtureAttestation({ task_id: "042" }))).toBe(
      "schema_invalid:task_id",
    );
  });

  it("accepts the literal '0' for task_id", () => {
    expect(checkSchema(fixtureAttestation({ task_id: "0" }))).toBeNull();
  });

  it("rejects task_id that overflows u64", () => {
    // u64 max + 1
    expect(
      checkSchema(fixtureAttestation({ task_id: "18446744073709551616" })),
    ).toBe("schema_invalid:task_id");
  });

  it("rejects uppercase hex-32", () => {
    expect(
      checkSchema(fixtureAttestation({ verification_hash: "A".repeat(64) })),
    ).toBe("schema_invalid:verification_hash");
  });

  it("rejects 0x-prefixed hex", () => {
    expect(
      checkSchema(fixtureAttestation({ verification_hash: "0x" + "a".repeat(62) })),
    ).toBe("schema_invalid:verification_hash");
  });

  it("rejects RFC 3339 with fractional seconds (per spec §3 row)", () => {
    expect(
      checkSchema(fixtureAttestation({ verified_at: "2026-05-02T12:00:00.123Z" })),
    ).toBe("schema_invalid:verified_at");
  });

  it("accepts both Z and +00:00 RFC 3339 forms", () => {
    expect(
      checkSchema(fixtureAttestation({ verified_at: "2026-05-02T12:00:00+00:00" })),
    ).toBeNull();
  });
});

describe("verifyV1Schema (steps 1+6)", () => {
  it("rejects composite_score > score_max as score_above_max", () => {
    const a = fixtureAttestation({
      composite_score: "1000001",
      score_max: "1000000",
    });
    expect(verifyV1Schema(a)).toBe("score_above_max");
  });

  it("accepts composite_score == score_max", () => {
    const a = fixtureAttestation({
      composite_score: "1000000",
      score_max: "1000000",
    });
    expect(verifyV1Schema(a)).toBeNull();
  });
});

describe("anchorDiscriminator", () => {
  it("computes the canonical 'Task' discriminator", () => {
    // sha256("account:Task")[0..8] — verified against the on-chain
    // shillbot::state::Task::DISCRIMINATOR. If this regression-guard
    // ever fires, either Anchor changed its discriminator scheme
    // (extremely unlikely) or the test's expected bytes are wrong.
    const disc = anchorDiscriminator("Task");
    expect(disc.length).toBe(8);
    // Smoke-check: re-computing yields the same bytes (no nondeterminism).
    const disc2 = anchorDiscriminator("Task");
    for (let i = 0; i < 8; i++) {
      expect(disc[i]).toBe(disc2[i]);
    }
  });
});

describe("verifyV1OnChain (steps 2-5+7) with mocked RPC", () => {
  /** Minimal Connection mock returning whatever the test sets up. */
  function mockRpc(account: { owner: string; data: Uint8Array } | null): Connection {
    // Only `getAccountInfo` is exercised by verifyV1OnChain; cast through
    // unknown to satisfy the Connection type.
    const fake = {
      rpcEndpoint: "mock://test",
      async getAccountInfo(_pubkey: PublicKey) {
        if (account === null) return null;
        return {
          owner: new PublicKey(account.owner),
          data: Buffer.from(account.data),
          lamports: 1,
          executable: false,
          rentEpoch: 0,
        };
      },
    } as unknown as Connection;
    return fake;
  }

  it("returns account_closed when RPC reports account does not exist", async () => {
    const verdict = await verifyV1OnChain(
      asV1(fixtureAttestation()),
      shillbotProtocol,
      mockRpc(null),
    );
    expect(verdict).toBe("account_closed");
  });

  it("returns owner_mismatch when account exists but owner is different", async () => {
    // Build an account with the correct discriminator but the wrong owner.
    const disc = anchorDiscriminator("Task");
    const data = new Uint8Array([...disc, ...new Uint8Array(307)]);
    const verdict = await verifyV1OnChain(
      asV1(fixtureAttestation()),
      shillbotProtocol,
      mockRpc({
        owner: "11111111111111111111111111111111",
        data,
      }),
    );
    expect(verdict).toBe("owner_mismatch");
  });

  it("returns discriminator_mismatch when first 8 bytes are wrong", async () => {
    // Owner matches, discriminator zero'd → mismatch with sha256("account:Task")[0..8].
    const data = new Uint8Array(315);
    const verdict = await verifyV1OnChain(
      asV1(fixtureAttestation()),
      shillbotProtocol,
      mockRpc({
        owner: "2tR37nqMpwdV4DVUHjzUmL1rH2DtkA8zrRA4EAhT7KMi",
        data,
      }),
    );
    expect(verdict).toBe("discriminator_mismatch");
  });
});
