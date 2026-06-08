import { PublicKey } from "@solana/web3.js";

/**
 * extension-registry `Extension` account decoder — the mund-creanc-witer
 * obligation edge (extender vouches for recipient, backed by a bonded SOL
 * stake). Mirrors `programs/extension-registry/src/state/extension.rs` exactly.
 *
 * Unlike Shillbot's `Task`, an `Extension` has NO on-chain state enum: the
 * account exists only while the vouch is LIVE. Both terminal transitions
 * close it — `withdraw_extension` (extender reclaims bond) and
 * `default_extension` (slash to treasury) — emitting `ExtensionWithdrawn` /
 * `ExtensionDefaulted` events. So a present account is always "active"; a closed
 * one is terminal (the verifier returns `account_closed`), exactly mirroring a
 * finalized Task. The durable terminal history is the event stream, not an
 * account.
 *
 * NB: this exposes a typed decoder + discriminator/state helpers for downstream
 * consumers (the web-position indexer, an extension-aware verifier). It is NOT a
 * drop-in `ProtocolHandler`: that interface's `DecodedAccount` is Task-shaped
 * (task_id, composite_score, content hashes…), and `verify.ts` step 5 compares
 * those Task fields directly. Verifying an Extension *attestation* end-to-end
 * therefore needs the verifier's field-equality core generalized per
 * account_kind (and matching wire fields) — a deliberate change beyond this
 * decoder, tracked separately.
 *
 * Body offsets (after the 8-byte Anchor discriminator):
 *
 *   off  size  field
 *     0    32  extender        (Pubkey)
 *    32    32  recipient       (Pubkey)
 *    64     1  extension_type  (u8)
 *    65     8  bond_lamports   (u64 LE)
 *    73     8  created_at      (i64 LE, unix seconds)
 *    81     1  bump
 *    82    16  _reserved       (skipped)
 *
 * Body = 98 bytes; full account (with discriminator) = 106 = `Extension::SPACE`.
 */
export interface ExtensionRecord {
  extender: string;
  recipient: string;
  extension_type: number;
  bond_lamports: bigint;
  created_at: bigint;
  bump: number;
}

/** The Anchor account name (→ discriminator `sha256("account:Extension")[0..8]`). */
export const EXTENSION_ACCOUNT_KIND = "Extension";

/** Minimum body length carrying every decoded field (through `bump`). */
export const EXTENSION_MIN_BODY_LEN = 82;

/** A live Extension has no state byte — existence == active. */
export const EXTENSION_ACTIVE_STATE = "active";

/**
 * Decode an `Extension` account body (the bytes AFTER the 8-byte discriminator).
 * Throws on the wrong `accountKind` or a body too short for the decoded fields.
 */
export function decodeExtension(
  data: Uint8Array,
  accountKind: string
): ExtensionRecord {
  if (accountKind !== EXTENSION_ACCOUNT_KIND) {
    throw new Error(
      `decodeExtension only handles account_kind="Extension", got "${accountKind}"`
    );
  }
  if (data.length < EXTENSION_MIN_BODY_LEN) {
    throw new Error(
      `Extension body too short: ${data.length} bytes (need >= ${EXTENSION_MIN_BODY_LEN})`
    );
  }
  const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
  return {
    extender: new PublicKey(data.slice(0, 32)).toBase58(),
    recipient: new PublicKey(data.slice(32, 64)).toBase58(),
    extension_type: data[64],
    bond_lamports: view.getBigUint64(65, true),
    created_at: view.getBigInt64(73, true),
    bump: data[81],
  };
}

/**
 * Extension-type discriminant → name. Mirrors the taxonomy in
 * `programs/extension-registry/src/constants.rs`. MVP records only
 * CapabilityValidation (0); the rest are reserved for weaker-verifiability
 * handling. Returns `null` for unknown discriminants.
 */
export function resolveExtensionType(extensionType: number): string | null {
  switch (extensionType) {
    case 0:
      return "capability_validation";
    case 1:
      return "mentorship";
    case 2:
      return "sponsorship";
    case 3:
      return "validation";
    default:
      return null;
  }
}
