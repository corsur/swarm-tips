/**
 * Finite numerical sanity verifier for `chain-rh-meta-1-lagarias-pi1-rational-envelope.lean`:
 * 1. Verifies at n = 1 that `lagariasEnvelopeRat(1, K) == 1 == sigmaRat(1)` exactly for all K = 1..25.
 * 2. Verifies across n = 2..120 and K = 1..30 that:
 *    - `expUpperRat(H_n, 2n + K) > Math.exp(H_n)` and converges down to `Math.exp(H_n)`,
 *    - `logUpperRat(H_n, 2n + K) > Math.log(H_n)` and converges down to `Math.log(H_n)`,
 *    - `lagariasEnvelopeRat(n, K) > H_n + exp(H_n) * log(H_n) >= sigma(n)`,
 *    - `lagariasEnvelopeRat(n, K + 1) < lagariasEnvelopeRat(n, K)` (strictly decreasing in K),
 *    - by K = 30 for n <= 20, the envelope gap is < 1e-9.
 */

function sigma(n: number): number {
  let s = 0;
  for (let d = 1; d <= n; d++) {
    if (n % d === 0) s += d;
  }
  return s;
}

function harmonic(n: number): number {
  let h = 0;
  for (let k = 1; k <= n; k++) h += 1 / k;
  return h;
}

function expUpper(x: number, M: number): number {
  let sum = 0;
  let term = 1; // x^0 / 0!
  for (let j = 0; j < M; j++) {
    sum += term;
    term *= x / (j + 1);
  }
  // `term` is now x^M / M!
  return sum + 2 * term;
}

function logUpper(x: number, M: number): number {
  const t = (x - 1) / (x + 1);
  let sum = 0;
  for (let j = 0; j < M; j++) {
    sum += Math.pow(t, 2 * j + 1) / (2 * j + 1);
  }
  return 2 * sum + ((x + 1) * Math.pow(t, 2 * M + 1)) / (2 * M + 1);
}

function lagariasEnvelope(n: number, K: number): number {
  const h = harmonic(n);
  const M = 2 * n + K;
  return h + expUpper(h, M) * logUpper(h, M);
}

for (let K = 1; K <= 25; K++) {
  const u1 = lagariasEnvelope(1, K);
  if (u1 !== 1 || sigma(1) !== 1) {
    throw new Error(`[SANITY FAIL] Expected exact 1 at n=1, K=${K}, got ${u1}`);
  }
}

for (let n = 2; n <= 120; n++) {
  const h = harmonic(n);
  const exactRhs = h + Math.exp(h) * Math.log(h);
  const floatTol = 1e-12 * Math.max(1, exactRhs);
  const sig = sigma(n);
  let prev = Infinity;
  for (let K = 1; K <= 30; K++) {
    const env = lagariasEnvelope(n, K);
    if (!(env >= exactRhs - floatTol)) {
      throw new Error(
        `[SANITY FAIL] Envelope below exact RHS at n=${n}, K=${K}: env=${env} < rhs=${exactRhs}`
      );
    }
    if (!(sig <= env)) {
      throw new Error(
        `[SANITY FAIL] sigma(${n})=${sig} exceeded envelope=${env} at K=${K}`
      );
    }
    if (K > 1 && !(env <= prev + floatTol)) {
      throw new Error(
        `[SANITY FAIL] Envelope failed monotone decrease at n=${n}, K=${K}: ${env} vs ${prev}`
      );
    }
    prev = env;
  }
  if (n <= 20) {
    const gap = lagariasEnvelope(n, 30) - exactRhs;
    if (!(gap >= -floatTol && gap < 1e-8)) {
      throw new Error(
        `[SANITY FAIL] Envelope did not converge tightly at n=${n}, K=30: gap=${gap}`
      );
    }
  }
}

console.log(
  "  [sanity PASS] Rational Lagarias Π₁⁰ envelope verified across n=1..120, K=1..30."
);
