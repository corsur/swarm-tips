/**
 * VOW v1 reference verifier — pure-step unit tests.
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
  verifyV1,
  verifyV1Schema,
  verifyV1OnChain,
  shillbotProtocol,
} from "./index.js";
import type { AasV1Attestation, ProtocolHandler } from "./index.js";
import { Connection, PublicKey } from "@solana/web3.js";

/** A minimal valid v1 attestation. Uses real-shaped pubkeys / hashes. */
function fixtureAttestation(
  overrides: Partial<AasV1Attestation> = {}
): AasV1Attestation {
  return {
    version: "vow/v1",
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
    const a = fixtureAttestation({ version: "vow/v0" as "vow/v1" });
    expect(checkSchema(a)).toBe("schema_invalid:version");
  });

  it("rejects non-base58 pubkeys", () => {
    expect(
      checkSchema(fixtureAttestation({ program_id: "not-base58!!" }))
    ).toBe("schema_invalid:program_id");
  });

  it("rejects oracle_feed that is neither pubkey nor null", () => {
    expect(checkSchema(fixtureAttestation({ oracle_feed: "" }))).toBe(
      "schema_invalid:oracle_feed"
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
      "schema_invalid:platform"
    );
    expect(checkSchema(fixtureAttestation({ platform: -1 }))).toBe(
      "schema_invalid:platform"
    );
  });

  it("rejects task_id with leading zero", () => {
    expect(checkSchema(fixtureAttestation({ task_id: "042" }))).toBe(
      "schema_invalid:task_id"
    );
  });

  it("accepts the literal '0' for task_id", () => {
    expect(checkSchema(fixtureAttestation({ task_id: "0" }))).toBeNull();
  });

  it("rejects task_id that overflows u64", () => {
    // u64 max + 1
    expect(
      checkSchema(fixtureAttestation({ task_id: "18446744073709551616" }))
    ).toBe("schema_invalid:task_id");
  });

  it("rejects uppercase hex-32", () => {
    expect(
      checkSchema(fixtureAttestation({ verification_hash: "A".repeat(64) }))
    ).toBe("schema_invalid:verification_hash");
  });

  it("rejects 0x-prefixed hex", () => {
    expect(
      checkSchema(
        fixtureAttestation({ verification_hash: "0x" + "a".repeat(62) })
      )
    ).toBe("schema_invalid:verification_hash");
  });

  it("rejects RFC 3339 with fractional seconds (per spec §3 row)", () => {
    expect(
      checkSchema(
        fixtureAttestation({ verified_at: "2026-05-02T12:00:00.123Z" })
      )
    ).toBe("schema_invalid:verified_at");
  });

  it("accepts both Z and +00:00 RFC 3339 forms", () => {
    expect(
      checkSchema(
        fixtureAttestation({ verified_at: "2026-05-02T12:00:00+00:00" })
      )
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
    // The literal canonical bytes: sha256("account:Task")[0..8]. A real
    // regression guard — recomputing-twice only proved determinism.
    expect(Array.from(disc)).toEqual([79, 34, 229, 55, 88, 90, 55, 84]);
  });
});

describe("verifyV1OnChain (steps 2-5+7) with mocked RPC", () => {
  /** Minimal Connection mock returning whatever the test sets up. */
  function mockRpc(
    account: { owner: string; data: Uint8Array } | null
  ): Connection {
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
      mockRpc(null)
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
      })
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
      })
    );
    expect(verdict).toBe("discriminator_mismatch");
  });
});

// ----------------------------------------------------------------------
// Happy path + step-5 negatives + step-7 negative
//
// Builds 315 bytes of synthetic on-chain Task account data that matches
// the wire-format attestation, then mutates one field at a time to
// confirm each `field_mismatch:<field>` reason is reachable. Without
// this, a regression in the decoder offsets or any step-5 comparison
// would slip through silently — only the negative steps 2-4 are
// exercised above.
// ----------------------------------------------------------------------

const HAPPY_TASK_ID_LE_U64 = 42n;
const HAPPY_VERIFIED_AT_SECS = 1_746_201_600; // 2025-05-02T12:00:00Z roughly
const HAPPY_VERIFIED_AT_ISO = new Date(HAPPY_VERIFIED_AT_SECS * 1000)
  .toISOString()
  .replace(/\.\d+Z$/, "Z");
const HAPPY_COMPOSITE_SCORE = 850_000n;
const HAPPY_VERIFY_HASH = "a".repeat(64);
const HAPPY_CONTENT_HASH = "b".repeat(64);
const HAPPY_CONTENT_ID_HASH = "c".repeat(64);

const SHILLBOT_PROGRAM_ID = "2tR37nqMpwdV4DVUHjzUmL1rH2DtkA8zrRA4EAhT7KMi";
const HAPPY_ACCOUNT = "11111111111111111111111111111112";
const HAPPY_CLIENT = "11111111111111111111111111111113";
const HAPPY_AGENT = "11111111111111111111111111111114";

/** Hex string → Uint8Array (32 bytes). */
function hexToBytes(hex: string): Uint8Array {
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) {
    out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

/** Construct a 315-byte Task account whose decoded fields match the
 *  values used in `happyAttestation()`. Discriminator + 307-byte body.
 *  Layout per `decoders/shillbot.ts:14-42`. */
function buildHappyAccountBytes(): Uint8Array {
  const disc = anchorDiscriminator("Task");
  const body = new Uint8Array(307);
  const view = new DataView(body.buffer, body.byteOffset, body.byteLength);

  view.setBigUint64(0, HAPPY_TASK_ID_LE_U64, true);
  body.set(new PublicKey(HAPPY_CLIENT).toBytes(), 8);
  body.set(new PublicKey(HAPPY_AGENT).toBytes(), 40);
  body[72] = 3; // state = Verified
  body[73] = 0; // platform = YouTube
  // bytes 74-82: escrow_lamports (zero)
  body.set(hexToBytes(HAPPY_CONTENT_HASH), 82);
  body.set(hexToBytes(HAPPY_CONTENT_ID_HASH), 114);
  // bytes 146-162: task_nonce (zero)
  view.setBigUint64(162, HAPPY_COMPOSITE_SCORE, true);
  // bytes 170-226: payment_amount, fee_amount, deadline, submit_margin,
  // claim_buffer, created_at, submitted_at (all zero)
  view.setBigInt64(226, BigInt(HAPPY_VERIFIED_AT_SECS), true);
  // bytes 234-254: challenge_deadline + three u32 overrides (zero)
  body.set(hexToBytes(HAPPY_VERIFY_HASH), 254);
  // bytes 286-306: _reserved (zero)
  body[306] = 254; // bump

  const out = new Uint8Array(8 + body.length);
  out.set(disc, 0);
  out.set(body, 8);
  return out;
}

function happyAttestation(
  overrides: Partial<AasV1Attestation> = {}
): AasV1Attestation {
  return {
    version: "vow/v1",
    network: "mainnet",
    program_id: SHILLBOT_PROGRAM_ID,
    account: HAPPY_ACCOUNT,
    account_kind: "Task",
    task_id: HAPPY_TASK_ID_LE_U64.toString(),
    client: HAPPY_CLIENT,
    agent: HAPPY_AGENT,
    state: "verified",
    platform: 0,
    composite_score: HAPPY_COMPOSITE_SCORE.toString(),
    score_max: "1000000",
    verified_at: HAPPY_VERIFIED_AT_ISO,
    verification_hash: HAPPY_VERIFY_HASH,
    content_hash: HAPPY_CONTENT_HASH,
    content_id_hash: HAPPY_CONTENT_ID_HASH,
    oracle_feed: "11111111111111111111111111111115",
    ...overrides,
  };
}

function happyMockRpc(): Connection {
  const data = buildHappyAccountBytes();
  return {
    rpcEndpoint: "mock://test",
    async getAccountInfo(_pubkey: PublicKey) {
      return {
        owner: new PublicKey(SHILLBOT_PROGRAM_ID),
        data: Buffer.from(data),
        lamports: 1,
        executable: false,
        rentEpoch: 0,
      };
    },
  } as unknown as Connection;
}

describe("verifyV1 happy path + step-5 / step-7 negatives", () => {
  it("returns valid:true when wire and on-chain agree", async () => {
    const verdict = await verifyV1(
      happyAttestation(),
      shillbotProtocol,
      happyMockRpc()
    );
    expect(verdict.valid).toBe(true);
    if (verdict.valid) {
      expect(verdict.attestation.composite_score).toBe(
        HAPPY_COMPOSITE_SCORE.toString()
      );
    }
  });

  it("returns field_mismatch:task_id when wire claims a different task_id", async () => {
    const verdict = await verifyV1(
      happyAttestation({ task_id: "43" }),
      shillbotProtocol,
      happyMockRpc()
    );
    expect(verdict.valid).toBe(false);
    if (!verdict.valid) {
      expect(verdict.failure_reason).toBe("field_mismatch:task_id");
    }
  });

  it("returns field_mismatch:client when wire claims a different client pubkey", async () => {
    const verdict = await verifyV1(
      happyAttestation({ client: "11111111111111111111111111111119" }),
      shillbotProtocol,
      happyMockRpc()
    );
    expect(verdict.valid).toBe(false);
    if (!verdict.valid) {
      expect(verdict.failure_reason).toBe("field_mismatch:client");
    }
  });

  it("returns field_mismatch:agent when wire claims a different agent pubkey", async () => {
    const verdict = await verifyV1(
      happyAttestation({ agent: "11111111111111111111111111111119" }),
      shillbotProtocol,
      happyMockRpc()
    );
    expect(verdict.valid).toBe(false);
    if (!verdict.valid) {
      expect(verdict.failure_reason).toBe("field_mismatch:agent");
    }
  });

  it("returns field_mismatch:composite_score when wire score differs", async () => {
    const verdict = await verifyV1(
      happyAttestation({ composite_score: "850001" }),
      shillbotProtocol,
      happyMockRpc()
    );
    expect(verdict.valid).toBe(false);
    if (!verdict.valid) {
      expect(verdict.failure_reason).toBe("field_mismatch:composite_score");
    }
  });

  it("returns field_mismatch:verification_hash when wire hash differs", async () => {
    const verdict = await verifyV1(
      happyAttestation({ verification_hash: "d".repeat(64) }),
      shillbotProtocol,
      happyMockRpc()
    );
    expect(verdict.valid).toBe(false);
    if (!verdict.valid) {
      expect(verdict.failure_reason).toBe("field_mismatch:verification_hash");
    }
  });

  it("returns field_mismatch:content_hash when wire hash differs", async () => {
    const verdict = await verifyV1(
      happyAttestation({ content_hash: "d".repeat(64) }),
      shillbotProtocol,
      happyMockRpc()
    );
    expect(verdict.valid).toBe(false);
    if (!verdict.valid) {
      expect(verdict.failure_reason).toBe("field_mismatch:content_hash");
    }
  });

  it("returns field_mismatch:content_id_hash when wire hash differs", async () => {
    const verdict = await verifyV1(
      happyAttestation({ content_id_hash: "d".repeat(64) }),
      shillbotProtocol,
      happyMockRpc()
    );
    expect(verdict.valid).toBe(false);
    if (!verdict.valid) {
      expect(verdict.failure_reason).toBe("field_mismatch:content_id_hash");
    }
  });

  it("returns field_mismatch:verified_at when wire timestamp drifts > 1 second", async () => {
    // Wire claims 5 seconds AFTER the on-chain value; tolerance is ±1.
    const drifted = new Date((HAPPY_VERIFIED_AT_SECS + 5) * 1000)
      .toISOString()
      .replace(/\.\d+Z$/, "Z");
    const verdict = await verifyV1(
      happyAttestation({ verified_at: drifted }),
      shillbotProtocol,
      happyMockRpc()
    );
    expect(verdict.valid).toBe(false);
    if (!verdict.valid) {
      expect(verdict.failure_reason).toBe("field_mismatch:verified_at");
    }
  });

  it("accepts ±1 second drift on verified_at (defensive cover per spec §4 step 5)", async () => {
    const drifted = new Date((HAPPY_VERIFIED_AT_SECS + 1) * 1000)
      .toISOString()
      .replace(/\.\d+Z$/, "Z");
    const verdict = await verifyV1(
      happyAttestation({ verified_at: drifted }),
      shillbotProtocol,
      happyMockRpc()
    );
    expect(verdict.valid).toBe(true);
  });

  it("returns field_mismatch:state when wire state name disagrees with on-chain discriminant", async () => {
    // On-chain state byte is 3 (Verified). Wire claims "approved" — both
    // pass schema but step-5 state comparison fails after resolveState
    // maps the on-chain 3 → "verified" ≠ "approved".
    const verdict = await verifyV1(
      happyAttestation({ state: "approved" }),
      shillbotProtocol,
      happyMockRpc()
    );
    expect(verdict.valid).toBe(false);
    if (!verdict.valid) {
      expect(verdict.failure_reason).toBe("field_mismatch:state");
    }
  });

  it("returns state_invalid when wire+chain agree on a state that's not in protocol's validStates", async () => {
    // Build an account body where state byte is 7 (Approved). Wire also
    // claims "approved", so step 5 passes. But Shillbot's validStates
    // is ["verified"] only → step 7 rejects.
    const data = buildHappyAccountBytes();
    data[8 + 72] = 7; // overwrite the state byte in the body (post-discriminator offset 72)
    const rpc = {
      rpcEndpoint: "mock://test",
      async getAccountInfo(_pubkey: PublicKey) {
        return {
          owner: new PublicKey(SHILLBOT_PROGRAM_ID),
          data: Buffer.from(data),
          lamports: 1,
          executable: false,
          rentEpoch: 0,
        };
      },
    } as unknown as Connection;

    const verdict = await verifyV1(
      happyAttestation({ state: "approved" }),
      shillbotProtocol,
      rpc
    );
    expect(verdict.valid).toBe(false);
    if (!verdict.valid) {
      expect(verdict.failure_reason).toBe("state_invalid");
    }
  });

  it("custom protocol with multi-state validStates accepts what Shillbot would reject", async () => {
    // Demonstrates the validStates extension point isn't Shillbot-locked.
    // A custom handler that returns ["verified", "approved"] accepts the
    // approved-state attestation that Shillbot's handler rejects above.
    const data = buildHappyAccountBytes();
    data[8 + 72] = 7; // approved on-chain
    const rpc = {
      rpcEndpoint: "mock://test",
      async getAccountInfo(_pubkey: PublicKey) {
        return {
          owner: new PublicKey(SHILLBOT_PROGRAM_ID),
          data: Buffer.from(data),
          lamports: 1,
          executable: false,
          rentEpoch: 0,
        };
      },
    } as unknown as Connection;

    const liberalProtocol: ProtocolHandler = {
      decode: shillbotProtocol.decode,
      resolveState: shillbotProtocol.resolveState,
      validStates: () => ["verified", "approved"],
    };

    const verdict = await verifyV1(
      happyAttestation({ state: "approved" }),
      liberalProtocol,
      rpc
    );
    expect(verdict.valid).toBe(true);
  });
});

describe("schema check — optional fields", () => {
  it("rejects verifier_instructions of wrong type (number instead of string)", () => {
    const a = {
      ...happyAttestation(),
      verifier_instructions: 42 as unknown as string,
    };
    expect(checkSchema(a)).toBe("schema_invalid:verifier_instructions");
  });

  it("accepts a string verifier_instructions", () => {
    const a = {
      ...happyAttestation(),
      verifier_instructions: "re-read on-chain",
    };
    expect(checkSchema(a)).toBeNull();
  });
});
