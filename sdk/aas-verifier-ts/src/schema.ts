import { PublicKey } from "@solana/web3.js";
import type { AasV1Attestation, FailureReason } from "./types.js";

/**
 * Step 1 — schema check. Implements the well-formed-per-type table
 * from `docs/specs/aas-v1.md` §4. Returns the first failure reason
 * encountered, or null if the schema check passes.
 *
 * Pure — no RPC, no I/O.
 */
export function checkSchema(input: unknown): FailureReason | null {
  if (typeof input !== "object" || input === null || Array.isArray(input)) {
    return "schema_invalid:<root>";
  }
  const a = input as Record<string, unknown>;

  if (a.version !== "aas/v1") return "schema_invalid:version";
  if (!isPubkey(a.program_id)) return "schema_invalid:program_id";
  if (!isPubkey(a.account)) return "schema_invalid:account";
  if (!isPubkey(a.client)) return "schema_invalid:client";
  if (!isPubkey(a.agent)) return "schema_invalid:agent";
  if (a.oracle_feed !== null && !isPubkey(a.oracle_feed)) {
    return "schema_invalid:oracle_feed";
  }

  if (a.network !== "mainnet" && a.network !== "devnet") {
    return "schema_invalid:network";
  }

  if (!isU8Number(a.platform)) return "schema_invalid:platform";

  if (!isDecimalU64(a.task_id)) return "schema_invalid:task_id";
  if (!isDecimalU64(a.composite_score)) return "schema_invalid:composite_score";
  if (!isDecimalU64(a.score_max)) return "schema_invalid:score_max";

  if (!isHex32(a.verification_hash)) return "schema_invalid:verification_hash";
  if (!isHex32(a.content_hash)) return "schema_invalid:content_hash";
  if (!isHex32(a.content_id_hash)) return "schema_invalid:content_id_hash";

  if (!isEnumString(a.state)) return "schema_invalid:state";
  if (!isEnumString(a.account_kind)) return "schema_invalid:account_kind";

  if (!isRfc3339NoFraction(a.verified_at)) {
    return "schema_invalid:verified_at";
  }

  if (a.extensions !== undefined && !isPlainObject(a.extensions)) {
    return "schema_invalid:extensions";
  }

  if (
    a.verifier_instructions !== undefined &&
    typeof a.verifier_instructions !== "string"
  ) {
    return "schema_invalid:verifier_instructions";
  }

  return null;
}

/** Base58, decodes to exactly 32 bytes. */
function isPubkey(v: unknown): boolean {
  if (typeof v !== "string" || v.length === 0) return false;
  try {
    const bytes = new PublicKey(v).toBytes();
    return bytes.length === 32;
  } catch {
    return false;
  }
}

/** JSON number in [0, 255]. */
function isU8Number(v: unknown): boolean {
  return typeof v === "number" && Number.isInteger(v) && v >= 0 && v <= 255;
}

/**
 * Decimal string matching ^[0-9]+$, no leading zeros except the literal
 * "0", fits in u64 (max 18446744073709551615 = 2^64 - 1).
 */
function isDecimalU64(v: unknown): boolean {
  if (typeof v !== "string") return false;
  if (!/^[0-9]+$/.test(v)) return false;
  if (v.length > 1 && v.startsWith("0")) return false;
  // Compare as BigInt to avoid Number precision loss.
  try {
    const n = BigInt(v);
    return n >= 0n && n <= U64_MAX;
  } catch {
    return false;
  }
}

const U64_MAX = (1n << 64n) - 1n;

/** Lowercase hex of length 64 (= 32 bytes). */
function isHex32(v: unknown): boolean {
  return typeof v === "string" && /^[0-9a-f]{64}$/.test(v);
}

/** Non-empty, no whitespace. Lowercase optional (verifiers tolerate). */
function isEnumString(v: unknown): boolean {
  return typeof v === "string" && v.length > 0 && !/\s/.test(v);
}

/**
 * RFC 3339 timestamp with NO fractional seconds. Either `Z` or `+00:00`
 * UTC offset (per spec §3 row for `verified_at`). Verifiers MUST accept
 * both forms.
 */
function isRfc3339NoFraction(v: unknown): boolean {
  if (typeof v !== "string") return false;
  // Anchor pattern: YYYY-MM-DDTHH:MM:SS{Z|+00:00}
  // No fractional seconds (".123" is rejected).
  const pattern =
    /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(Z|\+00:00)$/;
  if (!pattern.test(v)) return false;
  // Round-trip via Date to confirm the components describe a real
  // moment (rejects e.g. "2026-13-32T25:61:99Z").
  const ms = Date.parse(v);
  return !Number.isNaN(ms);
}

function isPlainObject(v: unknown): boolean {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

/**
 * Once `checkSchema` passes, narrow the input to `AasV1Attestation`.
 * Pure runtime cast — no defensive checks (those already ran).
 */
export function asV1(a: unknown): AasV1Attestation {
  return a as AasV1Attestation;
}
