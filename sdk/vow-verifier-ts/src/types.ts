/**
 * VOW v1 wire format. Mirrors the type table in
 * `docs/specs/vow-v1.md` §3. All u64-typed on-chain values are
 * decimal strings on the wire (per v1 §3 to avoid JS
 * Number.MAX_SAFE_INTEGER truncation).
 */
export interface VowV1Attestation {
  version: "vow/v1";
  network: "mainnet" | "devnet";
  program_id: string;
  account: string;
  account_kind: string;
  task_id: string;
  client: string;
  agent: string;
  state: string;
  platform: number;
  composite_score: string;
  score_max: string;
  verified_at: string;
  verification_hash: string;
  content_hash: string;
  content_id_hash: string;
  oracle_feed: string | null;
  verifier_instructions?: string;
  extensions?: Record<string, unknown>;
}

/**
 * Closed taxonomy of failure reasons, matching `docs/specs/vow-v1.md`
 * §4 step-to-reason table. The string is the entire taxonomy — a
 * verifier MUST return one of these strings (with the named field
 * substituted into `field_mismatch:<field>` and `schema_invalid:<field>`).
 */
export type FailureReason =
  | `schema_invalid:${string}`
  | "account_closed"
  | "owner_mismatch"
  | "discriminator_mismatch"
  | `field_mismatch:${string}`
  | "score_above_max"
  | "state_invalid";

/**
 * @deprecated Renamed to {@link VowV1Attestation} when the standard became VOW
 * (Verifiable On-chain Work) on 2026-07-10. The wire format is `vow/v1`; this
 * alias only keeps existing importers compiling and will be removed.
 */
export type AasV1Attestation = VowV1Attestation;

export type Verdict =
  | {
      valid: true;
      attestation: AasV1Attestation;
      checked_at: string;
      rpc_endpoint: string;
    }
  | {
      valid: false;
      attestation: AasV1Attestation;
      checked_at: string;
      rpc_endpoint: string;
      failure_reason: FailureReason;
    };

/**
 * Decoded representation of the on-chain account body (sans the 8-byte
 * Anchor discriminator). The reference shillbot decoder lives in
 * `decoders/shillbot.ts`; protocols emit their own decoder for VOW v1
 * conformance. The verifier compares each field below to the
 * attestation's wire-format claim.
 */
export interface DecodedAccount {
  task_id: bigint;
  client: string;
  agent: string;
  state: number;
  composite_score: bigint;
  verified_at: bigint;
  verification_hash: string;
  content_hash: string;
  content_id_hash: string;
}

/**
 * Per-protocol pluggable decoder. The verifier calls this with the
 * raw account bytes (after stripping the 8-byte Anchor discriminator)
 * and the attestation's `account_kind` for routing.
 */
export type AccountDecoder = (
  data: Uint8Array,
  accountKind: string
) => DecodedAccount;

/**
 * Per-protocol state-name resolver. The verifier calls this to map the
 * decoded `state` u8 (e.g. 3) to the published lower-case enum name
 * (e.g. "verified"). Protocols that publish their state enum under a
 * different naming convention can pass a custom resolver.
 *
 * Returns `null` for unknown discriminants — the verifier treats that
 * as `state_invalid`.
 */
export type StateResolver = (
  state: number,
  accountKind: string
) => string | null;

/**
 * Per-protocol "states valid for attestation" lookup. Per spec §4 step
 * 7, protocols publish which on-chain states are attestable. The
 * verifier rejects an attestation whose `state` isn't in this list.
 *
 * Shillbot exports `["verified"]` (only Verified is attestable; finalize
 * closes the on-chain account, see spec §6 capture window). Other
 * protocols MAY allow multiple states.
 */
export type ValidStates = (accountKind: string) => readonly string[];

/**
 * The three pluggable hooks a non-Shillbot protocol passes when calling
 * the verifier. For Shillbot, `sdk/vow-verifier-ts/src/decoders/shillbot.ts`
 * exports all three with the canonical Task layout, TaskState enum,
 * and the `["verified"]`-only attestation policy.
 */
export interface ProtocolHandler {
  decode: AccountDecoder;
  resolveState: StateResolver;
  validStates: ValidStates;
}
