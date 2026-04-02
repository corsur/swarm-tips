/**
 * Constructs a mock Switchboard PullFeedAccountData buffer for local testing.
 *
 * The real Switchboard program doesn't exist on a local/bankrun validator.
 * This helper creates a byte buffer with the correct discriminator and layout
 * so that `PullFeedAccountData::parse()` and `get_value()` succeed.
 *
 * Layout reference: switchboard-on-demand 0.11.3
 *   programs/shillbot/src/instructions/verify_task.rs (consumer)
 *   ~/.cargo/registry/.../switchboard-on-demand-0.11.3/src/on_demand/accounts/pull_feed.rs (source)
 */

const DISCRIMINATOR = Buffer.from([196, 27, 108, 196, 10, 215, 219, 40]);

// Total size: 8 (discriminator) + 3200 (struct) = 3208 bytes.
const ACCOUNT_SIZE = 3208;

// Field offsets from start of buffer (including 8-byte discriminator).
const OFF_SUBMISSIONS = 8; // submissions[32], each 64 bytes
const OFF_RESULT = 2264; // CurrentResult struct (128 bytes)
const OFF_MAX_STALENESS = 2392; // u32

// OracleSubmission offsets (within each 64-byte entry).
const SUB_ORACLE = 0; // Pubkey (32)
const SUB_SLOT = 32; // u64
const SUB_LANDED_AT = 40; // u64
const SUB_VALUE = 48; // i128

// CurrentResult offsets (within the 128-byte struct).
const RES_VALUE = 0; // i128
const RES_NUM_SAMPLES = 96; // u8
const RES_SUBMISSION_IDX = 97; // u8
const RES_SLOT = 104; // u64
const RES_MIN_SLOT = 112; // u64
const RES_MAX_SLOT = 120; // u64

/**
 * Write a 128-bit signed integer in little-endian to a buffer.
 * JavaScript BigInt handles arbitrarily large integers.
 */
function writeI128LE(buf: Buffer, offset: number, value: bigint): void {
  // Convert to two's complement 128-bit LE
  const mask = (1n << 128n) - 1n;
  const unsigned = value < 0n ? (mask + value + 1n) & mask : value & mask;
  const lo = unsigned & ((1n << 64n) - 1n);
  const hi = (unsigned >> 64n) & ((1n << 64n) - 1n);
  buf.writeBigUInt64LE(lo, offset);
  buf.writeBigUInt64LE(hi, offset + 8);
}

/**
 * Create a mock PullFeedAccountData buffer with one oracle submission.
 *
 * @param compositeScore - The score value (e.g., 800000 for 80% of MAX_SCORE).
 *   Stored in the feed as `compositeScore * 10^18` (Switchboard precision).
 * @param slot - The Solana slot number at which the submission was made.
 *   Must be within `maxStaleness` of the current clock slot when verify_task runs.
 * @param maxStaleness - Maximum slot age before data is considered stale (default: 10000).
 */
export function createMockPullFeedData(
  compositeScore: number,
  slot: bigint,
  maxStaleness: number = 10000
): Buffer {
  const buf = Buffer.alloc(ACCOUNT_SIZE);

  // Discriminator
  DISCRIMINATOR.copy(buf, 0);

  // Scale score to Switchboard's 10^18 precision
  const scaledValue = BigInt(compositeScore) * 10n ** 18n;

  // submissions[0]: one valid oracle submission
  const sub0 = OFF_SUBMISSIONS;
  // oracle pubkey: leave as zeros (any 32 bytes)
  buf.writeBigUInt64LE(slot, sub0 + SUB_SLOT);
  buf.writeBigUInt64LE(slot, sub0 + SUB_LANDED_AT);
  writeI128LE(buf, sub0 + SUB_VALUE, scaledValue);

  // result (CurrentResult): aggregated median
  const res = OFF_RESULT;
  writeI128LE(buf, res + RES_VALUE, scaledValue); // value
  // std_dev, mean, range, min_value, max_value: leave as 0
  buf.writeUInt8(1, res + RES_NUM_SAMPLES); // num_samples = 1
  buf.writeUInt8(0, res + RES_SUBMISSION_IDX); // submission_idx = 0
  buf.writeBigUInt64LE(slot, res + RES_SLOT);
  buf.writeBigUInt64LE(slot, res + RES_MIN_SLOT);
  buf.writeBigUInt64LE(slot, res + RES_MAX_SLOT);

  // max_staleness: clamp to slot value to prevent underflow in
  // get_value() which does `clock_slot - max_staleness` without checked math.
  const safeMaxStaleness =
    slot < BigInt(maxStaleness) ? Number(slot) : maxStaleness;
  buf.writeUInt32LE(safeMaxStaleness, OFF_MAX_STALENESS);

  return buf;
}
