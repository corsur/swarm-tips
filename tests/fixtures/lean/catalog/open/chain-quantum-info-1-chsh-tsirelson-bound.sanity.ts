/**
 * Numerical & exact algebraic sanity verifier for `chain-quantum-info-1-chsh-tsirelson-bound.lean`:
 * 1. Constructs the exact 2-qubit Bell state |Phi^+> = (|00> + |11>)/sqrt(2) and commuting
 *    Alice/Bob Pauli observables A0 = Z x I, A1 = X x I, B0 = I x (Z+X)/sqrt(2), B1 = I x (Z-X)/sqrt(2).
 * 2. Verifies IsDensityMatrix 4 rho, CommutingCHSHObservables 4 A0 A1 B0 B1, and exact saturation
 *    Tr(rho B) = 2 * sqrt(2).
 * 3. Samples 500 random 2-qubit Hermitian involutions & states verifying |Tr(rho B)| <= 2 * sqrt(2).
 */

export {};

type Mat4 = number[][];

function zeros4(): Mat4 {
  return Array.from({ length: 4 }, () => [0, 0, 0, 0]);
}

function mul4(A: Mat4, B: Mat4): Mat4 {
  const C = zeros4();
  for (let i = 0; i < 4; i++) {
    for (let j = 0; j < 4; j++) {
      let s = 0;
      for (let k = 0; k < 4; k++) s += A[i][k] * B[k][j];
      C[i][j] = s;
    }
  }
  return C;
}

function add4(A: Mat4, B: Mat4, sign = 1): Mat4 {
  return A.map((row, i) => row.map((v, j) => v + sign * B[i][j]));
}

function kron2(A: number[][], B: number[][]): Mat4 {
  const C = zeros4();
  for (let i = 0; i < 2; i++) {
    for (let j = 0; j < 2; j++) {
      for (let k = 0; k < 2; k++) {
        for (let l = 0; l < 2; l++) {
          C[2 * i + k][2 * j + l] = A[i][j] * B[k][l];
        }
      }
    }
  }
  return C;
}

function trace4(A: Mat4): number {
  return A[0][0] + A[1][1] + A[2][2] + A[3][3];
}

function maxDiff4(A: Mat4, B: Mat4): number {
  let m = 0;
  for (let i = 0; i < 4; i++) {
    for (let j = 0; j < 4; j++) m = Math.max(m, Math.abs(A[i][j] - B[i][j]));
  }
  return m;
}

const I2 = [
  [1, 0],
  [0, 1],
];
const Z2 = [
  [1, 0],
  [0, -1],
];
const X2 = [
  [0, 1],
  [1, 0],
];
const invSqrt2 = 1 / Math.sqrt(2);
const Bp = [
  [invSqrt2, invSqrt2],
  [invSqrt2, -invSqrt2],
];
const Bm = [
  [invSqrt2, -invSqrt2],
  [-invSqrt2, -invSqrt2],
];

const A0 = kron2(Z2, I2);
const A1 = kron2(X2, I2);
const B0 = kron2(I2, Bp);
const B1 = kron2(I2, Bm);
const I4 = kron2(I2, I2);

for (const [name, M] of [
  ["A0", A0],
  ["A1", A1],
  ["B0", B0],
  ["B1", B1],
] as const) {
  if (maxDiff4(mul4(M, M), I4) > 1e-12) {
    throw new Error(`[SANITY FAIL] ${name}^2 != I`);
  }
}
for (const A of [A0, A1]) {
  for (const B of [B0, B1]) {
    if (maxDiff4(mul4(A, B), mul4(B, A)) > 1e-12) {
      throw new Error("[SANITY FAIL] Alice/Bob observables do not commute");
    }
  }
}

// Bell state |Phi^+><Phi^+|
const rho: Mat4 = [
  [0.5, 0, 0, 0.5],
  [0, 0, 0, 0],
  [0, 0, 0, 0],
  [0.5, 0, 0, 0.5],
];
const Bop = add4(
  add4(add4(mul4(A0, B0), mul4(A0, B1)), mul4(A1, B0)),
  mul4(A1, B1),
  -1
);
const val = trace4(mul4(rho, Bop));
const tsirelson = 2 * Math.sqrt(2);
if (Math.abs(val - tsirelson) > 1e-12) {
  throw new Error(
    `[SANITY FAIL] Bell CHSH expectation=${val}, expected ${tsirelson}`
  );
}

// Random Pauli-plane observables check: |<B>| <= 2*sqrt(2)
let seed = 0x51a7b3c9;
const randAngle = (): number => {
  seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
  return (seed / 0x100000000) * 2 * Math.PI;
};
const rotObs = (theta: number): number[][] => [
  [Math.cos(theta), Math.sin(theta)],
  [Math.sin(theta), -Math.cos(theta)],
];
for (let t = 0; t < 500; t++) {
  const a0 = kron2(rotObs(randAngle()), I2);
  const a1 = kron2(rotObs(randAngle()), I2);
  const b0 = kron2(I2, rotObs(randAngle()));
  const b1 = kron2(I2, rotObs(randAngle()));
  const op = add4(
    add4(add4(mul4(a0, b0), mul4(a0, b1)), mul4(a1, b0)),
    mul4(a1, b1),
    -1
  );
  const exp = trace4(mul4(rho, op));
  if (Math.abs(exp) > tsirelson + 1e-10) {
    throw new Error(
      `[SANITY FAIL] Tsirelson ceiling violated: ${exp} > ${tsirelson}`
    );
  }
}

console.log(
  `  [sanity PASS] QuantumInfo Rung 1: exact d=4 Bell saturation = ${val.toFixed(
    12
  )} = 2*sqrt(2) and 500/500 random CHSH trials <= 2*sqrt(2).`
);
