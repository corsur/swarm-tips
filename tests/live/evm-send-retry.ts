/**
 * Send-retry policy for live EVM testnet scripts.
 *
 * WHY
 * ---
 * viem's `writeContract` auto-fills the nonce from the `latest` block, which
 * lags the mempool. Back-to-back sends from one account therefore reuse a nonce
 * and the node rejects the second with:
 *
 *   Nonce provided for the transaction is lower than the current nonce
 *   of the account.
 *
 * In the shillbot escrow matrix that killed cells mid-run, and the same shape
 * killed 19 homogeneous game cells in the coordination-app harness. Public
 * testnet RPCs add two more transient shapes on top: outright request failure
 * and "the request took too long to respond".
 *
 * The rule is deliberately split from the I/O so it can be unit-tested: a retry
 * policy that silently reclassifies a revert as retryable would resend a
 * value-moving transaction, so this decision deserves tests rather than
 * inline plumbing.
 *
 * NOTE: coordination-app has an equivalent module for its own harness. These are
 * separate repos, and reaching across a repo boundary for source is banned by
 * CLAUDE.md, so the policy is restated here rather than shared. Keep the two in
 * sync by behaviour, not by import.
 */

export type SendFailureKind = "nonce" | "underpriced" | "transient" | "fatal";

/**
 * Classify a send failure by its message.
 *
 * `fatal` means the transaction was actually executed and reverted — resending
 * it cannot help and may double-spend intent, so it is never retried. Anything
 * unrecognized is treated as `transient`: on a public testnet the overwhelmingly
 * common unknown failure is a flaky endpoint, and a retry is cheap. A revert is
 * checked FIRST so that a revert message that happens to contain the word
 * "nonce" is still classified fatal.
 */
export function classifySendFailure(message: string): SendFailureKind {
  const m = message.toLowerCase();
  if (m.includes("revert") || m.includes("execution reverted")) return "fatal";
  if (m.includes("nonce")) return "nonce";
  if (m.includes("underpriced") || m.includes("replacement transaction")) {
    return "underpriced";
  }
  return "transient";
}

export type RetryStep = {
  retry: boolean;
  /** Re-read the nonce with the `pending` tag before resending. */
  refreshNonce: boolean;
  backoffMs: number;
};

/**
 * Decide what to do after attempt `attempt` (1-based) failed with `kind`.
 *
 * Nonce and underpriced failures are the two that a stale nonce causes, so both
 * force a `pending`-tag refresh. Backoff is exponential and capped so a slow
 * endpoint cannot stretch a cell past its deadline.
 */
export function planRetry(
  attempt: number,
  maxAttempts: number,
  kind: SendFailureKind
): RetryStep {
  if (attempt < 1) throw new Error(`attempt must be >= 1, got ${attempt}`);
  if (kind === "fatal")
    return { retry: false, refreshNonce: false, backoffMs: 0 };
  if (attempt >= maxAttempts) {
    return { retry: false, refreshNonce: false, backoffMs: 0 };
  }
  const refreshNonce = kind === "nonce" || kind === "underpriced";
  const backoffMs = Math.min(1000 * 2 ** (attempt - 1), 8000);
  return { retry: true, refreshNonce, backoffMs };
}
