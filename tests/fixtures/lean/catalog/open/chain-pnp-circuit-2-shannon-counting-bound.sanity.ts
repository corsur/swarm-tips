/**
 * Finite numerical sanity check for `chain-pnp-circuit-2-shannon-counting-bound.lean`:
 * Verifies that for every n in [6, 64] and every s >= 1 with 4 * n * s <= 2^n,
 * log2((16 * (n + s)^2)^s) = s * (4 + 2 * log2(n + s)) < 2^n strictly!
 */
for (let n = 6; n <= 64; n++) {
  const twoPowN = Math.pow(2, n);
  const sMax = Math.floor(twoPowN / (4 * n));
  if (sMax < 1) {
    throw new Error(`[SANITY FAIL] sMax < 1 at n=${n}`);
  }
  const lhsLog2 = sMax * (4 + 2 * Math.log2(n + sMax));
  if (!(lhsLog2 < twoPowN)) {
    throw new Error(
      `[SANITY FAIL] Shannon counting inequality failed at n=${n}: lhsLog2=${lhsLog2} >= 2^n=${twoPowN}`
    );
  }
}
console.log(
  "  [sanity PASS] Shannon circuit counting inequality (16*(n+s)^2)^s < 2^(2^n) verified strictly for all n in [6, 64]."
);
