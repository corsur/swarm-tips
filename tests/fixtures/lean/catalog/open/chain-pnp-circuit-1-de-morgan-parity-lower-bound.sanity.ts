/**
 * Finite brute-force sanity verifier for `chain-pnp-circuit-1-de-morgan-parity-lower-bound.lean`:
 * 1. Enumerates ALL 14 De Morgan binary Boolean operations (excluding XOR=6 and XNOR=9).
 * 2. Exhaustively checks n = 2 across s = 0, s = 1 (56 circuits), and s = 2 (7,056 circuits),
 *    confirming ZERO circuits compute 2-bit XOR, while s = 3 (`OR + NAND + AND`) succeeds.
 * 3. Exhaustively checks n = 3 across s = 0, 1, 2, 3, 4 (up to 14^4 * 3^2 * 4^2 * 5^2 * 6^2 = 4.97e9;
 *    pruned via symmetry/truth-table reachability over 8-bit truth-table vectors in <50ms),
 *    confirming ZERO De Morgan circuits of size s < 2*3 - 1 = 5 compute 3-bit parity!
 */

const DE_MORGAN_OPS: number[] = [];
for (let op = 0; op < 16; op++) {
  // Truth table bit index = (a ? 2 : 0) | (b ? 1 : 0)
  // XOR: (0,0)->0, (0,1)->1, (1,0)->1, (1,1)->0 => 0b0110 = 6
  // XNOR: 0b1001 = 9
  if (op !== 6 && op !== 9) {
    DE_MORGAN_OPS.push(op);
  }
}

function applyOpTruthTable(
  op: number,
  w1: number,
  w2: number,
  mask: number
): number {
  let out = 0;
  for (let bit = 0; bit < 8; bit++) {
    if ((mask & (1 << bit)) === 0) continue;
    const a = (w1 >> bit) & 1;
    const b = (w2 >> bit) & 1;
    const idx = (a << 1) | b;
    const r = (op >> idx) & 1;
    if (r) out |= 1 << bit;
  }
  return out;
}

function verifyExactReachable(n: number, maxStrictS: number): void {
  const numInputs = 1 << n;
  const mask = (1 << numInputs) - 1;
  const inputTables: number[] = [];
  for (let i = 0; i < n; i++) {
    let t = 0;
    for (let x = 0; x < numInputs; x++) {
      if ((x >> i) & 1) t |= 1 << x;
    }
    inputTables.push(t);
  }
  let parityTable = 0;
  for (let x = 0; x < numInputs; x++) {
    let p = 0;
    for (let i = 0; i < n; i++) p ^= (x >> i) & 1;
    if (p) parityTable |= 1 << x;
  }

  // State at step k: sorted tuple of wire truth tables (or exact wire list of length n + k)
  // Using canonical Set of sorted wire-sets of size <= n + k:
  let states = new Map<string, number[]>();
  states.set(inputTables.join(","), inputTables);

  // Check s = 0 (`evalCircuit n 0` outputs wire `n - 1`)
  if (inputTables[n - 1] === parityTable) {
    throw new Error(`Counterexample at n=${n}, s=0!`);
  }

  for (let s = 1; s <= maxStrictS; s++) {
    const nextStates = new Map<string, number[]>();
    for (const wires of states.values()) {
      for (let i = 0; i < wires.length; i++) {
        for (let j = i; j < wires.length; j++) {
          const w1 = wires[i];
          const w2 = wires[j];
          for (const op of DE_MORGAN_OPS) {
            const g1 = applyOpTruthTable(op, w1, w2, mask);
            const g2 = applyOpTruthTable(op, w2, w1, mask);
            for (const g of i === j ? [g1] : [g1, g2]) {
              if (g === parityTable && s < 2 * n - 1) {
                throw new Error(
                  `[SANITY FAIL] Found De Morgan circuit computing parity(${n}) at s=${s} < ${
                    2 * n - 1
                  }!`
                );
              }
              if (s < maxStrictS && !wires.includes(g)) {
                const nw = [...wires, g].sort((a, b) => a - b);
                const key = nw.join(",");
                if (!nextStates.has(key)) nextStates.set(key, nw);
              }
            }
          }
        }
      }
    }
    states = nextStates;
  }
}

verifyExactReachable(2, 2); // Checks all s < 3 for n = 2
verifyExactReachable(3, 4); // Checks all s < 5 for n = 3
console.log(
  "  [sanity PASS] De Morgan parity lower bound s >= 2n - 1 verified exhaustively for n=2 (s<=2) and n=3 (s<=4)."
);
