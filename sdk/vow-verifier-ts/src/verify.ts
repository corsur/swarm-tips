import { Connection, PublicKey } from "@solana/web3.js";
import { anchorDiscriminator, bytesEqual } from "./discriminator.js";
import { asV1, checkSchema } from "./schema.js";
import type {
  AasV1Attestation,
  FailureReason,
  ProtocolHandler,
  Verdict,
} from "./types.js";

/**
 * Step-by-step verifier per `docs/specs/vow-v1.md` §4.
 *
 * Steps split:
 *   - `verifyV1Schema` — pure (steps 1, 6).
 *   - `verifyV1OnChain` — touches RPC (steps 2-5, 7).
 *   - `verifyV1` (this) — full 7-step pipeline.
 *
 * Splitting lets unit tests cover the pure parts without an RPC.
 */
export async function verifyV1(
  attestation: unknown,
  protocol: ProtocolHandler,
  rpc: Connection
): Promise<Verdict> {
  const checkedAt = nowRfc3339();
  const rpcEndpoint = rpc.rpcEndpoint;

  const schemaFail = verifyV1Schema(attestation);
  if (schemaFail) {
    return {
      valid: false,
      attestation: attestation as AasV1Attestation,
      checked_at: checkedAt,
      rpc_endpoint: rpcEndpoint,
      failure_reason: schemaFail,
    };
  }

  const a = asV1(attestation);
  const onChainFail = await verifyV1OnChain(a, protocol, rpc);
  if (onChainFail) {
    return {
      valid: false,
      attestation: a,
      checked_at: checkedAt,
      rpc_endpoint: rpcEndpoint,
      failure_reason: onChainFail,
    };
  }

  return {
    valid: true,
    attestation: a,
    checked_at: checkedAt,
    rpc_endpoint: rpcEndpoint,
  };
}

/**
 * Pure step 1 + step 6 of the protocol. Returns null on success or
 * the first failure reason.
 */
export function verifyV1Schema(attestation: unknown): FailureReason | null {
  const schemaFail = checkSchema(attestation);
  if (schemaFail) return schemaFail;

  const a = asV1(attestation);
  // Step 6 — domain bound. Both are decimal-string u64 (passed step 1).
  if (BigInt(a.composite_score) > BigInt(a.score_max)) {
    return "score_above_max";
  }
  return null;
}

/**
 * Steps 2-5 + 7 — touches RPC. Returns null on success or the first
 * failure reason.
 */
export async function verifyV1OnChain(
  a: AasV1Attestation,
  protocol: ProtocolHandler,
  rpc: Connection
): Promise<FailureReason | null> {
  // Step 2 — fetch the on-chain account.
  const accountInfo = await rpc.getAccountInfo(new PublicKey(a.account));
  if (accountInfo === null) {
    return "account_closed";
  }
  const data = new Uint8Array(accountInfo.data);

  // Step 3 — owner check.
  if (accountInfo.owner.toBase58() !== a.program_id) {
    return "owner_mismatch";
  }

  // Step 4 — Anchor discriminator check.
  if (data.length < 8) {
    return "discriminator_mismatch";
  }
  const expectedDisc = anchorDiscriminator(a.account_kind);
  if (!bytesEqual(expectedDisc, data.slice(0, 8))) {
    return "discriminator_mismatch";
  }

  // Step 5 — field equality. Decoder strips the 8-byte discriminator.
  const decoded = protocol.decode(data.slice(8), a.account_kind);

  if (decoded.task_id.toString() !== a.task_id) {
    return "field_mismatch:task_id";
  }
  if (decoded.client !== a.client) {
    return "field_mismatch:client";
  }
  if (decoded.agent !== a.agent) {
    return "field_mismatch:agent";
  }
  if (decoded.composite_score.toString() !== a.composite_score) {
    return "field_mismatch:composite_score";
  }
  if (decoded.verification_hash !== a.verification_hash) {
    return "field_mismatch:verification_hash";
  }
  if (decoded.content_hash !== a.content_hash) {
    return "field_mismatch:content_hash";
  }
  if (decoded.content_id_hash !== a.content_id_hash) {
    return "field_mismatch:content_id_hash";
  }

  // verified_at: chain stores i64 unix seconds; wire is RFC 3339 with
  // no fractional seconds. Convert and compare with ±1 second
  // tolerance (per spec §4 step 5).
  const wireSeconds = Math.floor(Date.parse(a.verified_at) / 1000);
  const chainSeconds = Number(decoded.verified_at);
  if (Math.abs(wireSeconds - chainSeconds) > 1) {
    return "field_mismatch:verified_at";
  }

  // Step 5 (state) — map decoded u8 to the protocol's published name.
  const resolved = protocol.resolveState(decoded.state, a.account_kind);
  if (resolved === null || resolved !== a.state) {
    return "field_mismatch:state";
  }

  // Step 7 — state validity. Spec §4 step 7 says protocols publish
  // which on-chain states are "valid for attestation"; the verifier
  // delegates to the protocol handler's `validStates` to avoid
  // hardcoding any one protocol's policy. Shillbot returns
  // `["verified"]`; other protocols can return larger sets without
  // forking this verifier.
  const valid = protocol.validStates(a.account_kind);
  if (!valid.includes(a.state)) {
    return "state_invalid";
  }

  return null;
}

function nowRfc3339(): string {
  return new Date().toISOString().replace(/\.\d+Z$/, "Z");
}
