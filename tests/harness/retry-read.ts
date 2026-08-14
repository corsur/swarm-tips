// Bounded retry reads for the LIVE (devnet) path of the shillbot step
// helpers. Against a load-balanced RPC, a read issued right after a confirmed
// write can hit a node that has not applied the tx yet: the account reads as
// missing, or its state reads as the pre-tx value. The 2026-08-13 devnet
// outcome-matrix run failed 4/6 cells exactly this way ("illegal transition
// Claimed -> Claimed", "Account does not exist" for PDAs that existed minutes
// later). Bankrun reads are authoritative immediately, so both helpers cost
// zero extra reads there.
//
// Bounded by construction (attempts is a hard cap checked before each retry),
// per the fixed-upper-bound loop rule.

export interface RetryReadOpts {
  /** Max read attempts (default 12). */
  attempts?: number;
  /** Delay between attempts in ms (default 500 — ~6s total at defaults). */
  delayMs?: number;
}

const DEFAULT_ATTEMPTS = 12;
const DEFAULT_DELAY_MS = 500;

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/**
 * Retry `read` while it throws (account not yet visible on the node that
 * served the request). Surfaces the last error once attempts are exhausted.
 */
export async function readWhenVisible<T>(
  read: () => Promise<T>,
  opts: RetryReadOpts = {}
): Promise<T> {
  const attempts = opts.attempts ?? DEFAULT_ATTEMPTS;
  const delayMs = opts.delayMs ?? DEFAULT_DELAY_MS;
  let lastErr: unknown;
  for (let i = 0; i < attempts; i++) {
    try {
      return await read();
    } catch (e) {
      lastErr = e;
      if (i < attempts - 1) await sleep(delayMs);
    }
  }
  throw lastErr;
}

/**
 * Read a status until it differs from `before` (the write's pre-state), the
 * account becomes visible, or attempts run out. Returns the LAST read either
 * way: a genuine stall then fails loudly in the caller's transition assert,
 * carrying the real evidence, instead of being masked by a retry error here.
 */
export async function readWhenAdvanced(
  readStatus: () => Promise<string>,
  before: string,
  opts: RetryReadOpts = {}
): Promise<string> {
  const attempts = opts.attempts ?? DEFAULT_ATTEMPTS;
  const delayMs = opts.delayMs ?? DEFAULT_DELAY_MS;
  let last = before;
  let sawRead = false;
  for (let i = 0; i < attempts; i++) {
    try {
      last = await readStatus();
      sawRead = true;
      if (last !== before) return last;
    } catch {
      // not yet visible — treat like a stale read and retry
    }
    if (i < attempts - 1) await sleep(delayMs);
  }
  if (!sawRead) {
    // Never became visible: that is a visibility failure, not a stall.
    throw new Error(
      `readWhenAdvanced: account never became visible after ${attempts} attempts`
    );
  }
  return last;
}
