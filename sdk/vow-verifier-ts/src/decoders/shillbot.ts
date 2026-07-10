import { PublicKey } from "@solana/web3.js";
import type {
  AccountDecoder,
  DecodedAccount,
  ProtocolHandler,
  StateResolver,
} from "../types.js";

/**
 * Shillbot Task account decoder (post-#12 layout).
 *
 * Accepts the on-chain Task account body — i.e. the bytes AFTER the
 * 8-byte Anchor discriminator. Layout mirrors
 * `programs/shillbot/src/state/task.rs::Task` exactly. Only the
 * fields the VOW verifier needs are decoded; the rest of the body
 * (escrow_lamports, deadlines, overrides, _reserved, bump) is
 * skipped via byte offsets.
 *
 * Field offsets within the body (post-discriminator):
 *
 *   off  size  field
 *     0     8  task_id            (u64 LE)
 *     8    32  client             (Pubkey)
 *    40    32  agent              (Pubkey)
 *    72     1  state              (TaskState u8)
 *    73     1  platform           (u8)
 *    74     8  escrow_lamports    (u64 LE)         skipped
 *    82    32  content_hash       (sha256)
 *   114    32  content_id_hash    (sha256)
 *   146    16  task_nonce         (skipped)
 *   162     8  composite_score    (u64 LE)
 *   170     8  payment_amount     (skipped)
 *   178     8  fee_amount         (skipped)
 *   186     8  deadline           (skipped)
 *   194     8  submit_margin      (skipped)
 *   202     8  claim_buffer       (skipped)
 *   210     8  created_at         (skipped)
 *   218     8  submitted_at       (skipped)
 *   226     8  verified_at        (i64 LE)
 *   234     8  challenge_deadline (skipped)
 *   242    12  three u32 overrides (skipped)
 *   254    32  verification_hash  (sha256)
 *   286    20  _reserved          (skipped)
 *   306     1  bump               (skipped)
 *
 * Total body = 307 bytes. With the 8-byte discriminator, the full
 * account is 315 bytes (matches `Task::SPACE` in the on-chain crate).
 */
export const decodeShillbotTask: AccountDecoder = (data, accountKind) => {
  if (accountKind !== "Task") {
    throw new Error(
      `decodeShillbotTask only handles account_kind="Task", got "${accountKind}"`
    );
  }
  if (data.length < 286) {
    throw new Error(
      `Shillbot Task body too short: ${data.length} bytes (need >= 286 for the VOW-relevant fields)`
    );
  }

  const view = new DataView(data.buffer, data.byteOffset, data.byteLength);

  const task_id = view.getBigUint64(0, /* littleEndian */ true);
  const client = new PublicKey(data.slice(8, 40)).toBase58();
  const agent = new PublicKey(data.slice(40, 72)).toBase58();
  const state = data[72];
  // platform is at offset 73 but unused by the verifier (the wire
  // format carries it directly; verifier's field-equality check
  // covers it via the v1 §4 step 5 platform comparison done by the
  // caller, not here).
  const content_hash = bytesToHex(data.slice(82, 114));
  const content_id_hash = bytesToHex(data.slice(114, 146));
  const composite_score = view.getBigUint64(162, true);
  const verified_at = view.getBigInt64(226, true);
  const verification_hash = bytesToHex(data.slice(254, 286));

  const decoded: DecodedAccount = {
    task_id,
    client,
    agent,
    state,
    composite_score,
    verified_at,
    verification_hash,
    content_hash,
    content_id_hash,
  };
  return decoded;
};

/**
 * Shillbot TaskState enum → lowercase name. Mirrors
 * `programs/shillbot/src/state/task.rs::TaskState` discriminants
 * exactly. `Approved = 7` was appended in Phase 3 blocker #3a.
 */
export const resolveShillbotState: StateResolver = (state, accountKind) => {
  if (accountKind !== "Task") return null;
  switch (state) {
    case 0:
      return "open";
    case 1:
      return "claimed";
    case 2:
      return "submitted";
    case 3:
      return "verified";
    case 4:
      return "finalized";
    case 5:
      return "disputed";
    case 6:
      return "resolved";
    case 7:
      return "approved";
    default:
      return null;
  }
};

/** Shillbot's "states valid for attestation" policy, per `docs/specs/vow-v1.md`
 *  §6 capture window: only Verified is attestable. Finalize closes the on-chain
 *  account, so post-finalize the attestation is permanently unverifiable. */
export const shillbotValidStates = (accountKind: string): readonly string[] => {
  if (accountKind !== "Task") return [];
  return ["verified"];
};

/** Convenience bundle for callers that just want the canonical Shillbot triple. */
export const shillbotProtocol: ProtocolHandler = {
  decode: decodeShillbotTask,
  resolveState: resolveShillbotState,
  validStates: shillbotValidStates,
};

function bytesToHex(b: Uint8Array): string {
  let s = "";
  for (let i = 0; i < b.length; i++) {
    s += b[i].toString(16).padStart(2, "0");
  }
  return s;
}
