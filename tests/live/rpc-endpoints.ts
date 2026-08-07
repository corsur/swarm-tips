/**
 * Resolve the ordered RPC endpoint list for a live-network battery.
 *
 * WHY THIS IS A FUNCTION AND NOT TWO INLINE EXPRESSIONS
 * ----------------------------------------------------
 * The inline version had two defects that a live battery hides, because both
 * only bite under conditions a hand probe never reproduces.
 *
 * 1. `process.env.RPC_URL ?? CHAIN.rpc` — `??` falls back on null/undefined but
 *    NOT on "". A runner that exports RPC_URL from an unset variable
 *    (`RPC_URL="${SOME_VAR:-}"`, an easy thing to write) hands the battery an
 *    empty endpoint that `??` treats as configured. The failure is a connection
 *    error with no endpoint named in it.
 *
 * 2. When an override WAS set, the transport became `fallback([http(override)])`
 *    — a single endpoint, discarding the multi-endpoint failover that the
 *    surrounding comment says exists precisely so "a single dropped write
 *    mid-battery" does not read as a cell failure. Supplying a better endpoint
 *    silently removed the resilience.
 *
 * Correct behaviour is override-FIRST-then-defaults: the dedicated endpoint is
 * preferred, and the public ones remain as backups. That is strictly better
 * than either previous branch.
 *
 * This matters because the symptom is so misleading: Base Sepolia's public node
 * rate-limits under battery load and returns an internal error, which viem
 * surfaces as `The contract function "treasury" reverted`. The contract does
 * not revert — it returns the treasury address on every hand probe.
 */

/**
 * @param override  raw `process.env.RPC_URL` (may be undefined, empty, or padded)
 * @param chainRpcs the chain's built-in endpoints, in preference order
 * @returns deduped endpoint list, override first when usable; never empty
 */
export function resolveRpcEndpoints(
  override: string | undefined,
  chainRpcs: readonly string[]
): string[] {
  const trimmed = override?.trim();
  // Falsy check, NOT `??`: "" and "   " mean "not configured", not "use empty".
  const ordered = trimmed ? [trimmed, ...chainRpcs] : [...chainRpcs];

  const seen = new Set<string>();
  const deduped = ordered.filter((u) =>
    seen.has(u) ? false : (seen.add(u), true)
  );

  if (deduped.length === 0) {
    throw new Error(
      "no RPC endpoints resolved: the chain entry lists none and RPC_URL is unset. " +
        "A battery with no endpoint fails as a connection error that names nothing."
    );
  }
  return deduped;
}
