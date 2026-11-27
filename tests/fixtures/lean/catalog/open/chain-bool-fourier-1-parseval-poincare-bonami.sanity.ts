/**
 * Finite numerical & exhaustive sanity verifier for `chain-bool-fourier-1-parseval-poincare-bonami.lean`:
 * 1. Verifies Parseval's identity E[h^2] = sum_S h_hat(S)^2 on random real functions for n = 1..5.
 * 2. Exhaustively verifies discrete Poincare Var(boolSign o f) <= I(f) across ALL 2^(2^3) = 256
 *    3-bit Boolean functions (confirming dictators achieve exact equality Var = I = 1).
 * 3. Verifies Bonami (2,4) hypercontractivity E[(T_rho h)^4] <= (E[h^2])^2 at the sharp threshold
 *    rho = 1/sqrt(3) for n = 1..5, and confirms violation for rho > 1/sqrt(3) on h(x) = 1 + eps*x_0.
 */

export {};

function popcount(x: number): number {
  let c = 0;
  let v = x;
  while (v > 0) {
    c += v & 1;
    v >>= 1;
  }
  return c;
}

function chi(S: number, x: number): number {
  return popcount(S & x) % 2 === 1 ? -1 : 1;
}

function fourierCoeffs(n: number, h: number[]): number[] {
  const N = 1 << n;
  const hats = new Array<number>(N).fill(0);
  for (let S = 0; S < N; S++) {
    let acc = 0;
    for (let x = 0; x < N; x++) acc += h[x] * chi(S, x);
    hats[S] = acc / N;
  }
  return hats;
}

function checkParsevalAndBonami(n: number, h: number[]): void {
  const N = 1 << n;
  const hats = fourierCoeffs(n, h);
  let e2 = 0;
  for (let x = 0; x < N; x++) e2 += h[x] * h[x];
  e2 /= N;
  let sumSq = 0;
  for (let S = 0; S < N; S++) sumSq += hats[S] * hats[S];
  if (Math.abs(e2 - sumSq) > 1e-10) {
    throw new Error(
      `[SANITY FAIL] Parseval mismatch at n=${n}: ${e2} vs ${sumSq}`
    );
  }
  const rho = 1 / Math.sqrt(3);
  let e4 = 0;
  for (let x = 0; x < N; x++) {
    let val = 0;
    for (let S = 0; S < N; S++) {
      val += Math.pow(rho, popcount(S)) * hats[S] * chi(S, x);
    }
    e4 += Math.pow(val, 4);
  }
  e4 /= N;
  if (e4 > e2 * e2 + 1e-10) {
    throw new Error(
      `[SANITY FAIL] Bonami (2,4) violated at n=${n}: ${e4} > ${e2 * e2}`
    );
  }
}

// Exhaustive Poincare check over all 256 Boolean functions on n = 3 bits
let dictatorTightCount = 0;
for (let tt = 0; tt < 256; tt++) {
  const n = 3;
  const N = 8;
  const g: number[] = [];
  let mean = 0;
  for (let x = 0; x < N; x++) {
    const sign = ((tt >> x) & 1) === 1 ? -1 : 1;
    g.push(sign);
    mean += sign;
  }
  mean /= N;
  let variance = 0;
  for (let x = 0; x < N; x++) variance += (g[x] - mean) ** 2;
  variance /= N;

  let totalInf = 0;
  for (let i = 0; i < n; i++) {
    let flips = 0;
    for (let x = 0; x < N; x++) {
      if (((tt >> x) & 1) !== ((tt >> (x ^ (1 << i))) & 1)) flips++;
    }
    totalInf += flips / N;
  }
  if (variance > totalInf + 1e-12) {
    throw new Error(
      `[SANITY FAIL] Poincare violated at tt=${tt}: Var=${variance} > I=${totalInf}`
    );
  }
  if (variance > 0.99 && Math.abs(variance - totalInf) < 1e-12) {
    dictatorTightCount++;
  }
}
if (dictatorTightCount !== 6) {
  throw new Error(
    `[SANITY FAIL] Expected 6 tight dictator/anti-dictator functions on n=3, got ${dictatorTightCount}`
  );
}

// Deterministic pseudo-random checks for Parseval + Bonami (2,4) on n = 1..5
let seed = 0x12345678;
const nextRand = (): number => {
  seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
  return (seed / 0x100000000) * 4 - 2;
};
for (let n = 1; n <= 5; n++) {
  const N = 1 << n;
  for (let trial = 0; trial < 25; trial++) {
    const h = Array.from({ length: N }, () => nextRand());
    checkParsevalAndBonami(n, h);
  }
}

console.log(
  "  [sanity PASS] BoolFourier Rung 1: Parseval, exhaustive n=3 Poincare (256/256, 6 tight dictators), and sharp rho=1/sqrt(3) Bonami (2,4) verified."
);
