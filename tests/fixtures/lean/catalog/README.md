# LeanProof bounty catalog (policy v2 — mathlib)

Statements posted as on-chain LeanProof campaigns (`lean_policy: 2`). Two kinds:

- **`open/`** — famous unsolved problems, posted as small standing bounties.
  They function as advertisement; nobody is expected to claim the escrow. Each
  statement is a well-formed `Prop` (elaboration-gated) but has no known proof.
- **`solvable/`** — provable statements with a known proof (`<name>.proof.lean`),
  used to demonstrate real settlements on the board.

## Hard rules

- **Targeted imports only.** The umbrella `import Mathlib` loads ~5GB of oleans
  and exceeds the runner's 240s wall-clock cap. Every statement imports narrowly
  (e.g. `import Mathlib.Data.Nat.Prime.Basic`), which elaborates in ~30–90s.
- **Every statement is elaboration-gated** against the live runner before it is
  seeded: `open/` statements must type-check as a `Prop` (submitting them with a
  `by sorry` proof yields `verdict: fail, detail: "proof uses sorry"` — the
  telltale that the *statement* elaborated and only the proof is missing; a
  malformed statement instead yields an `unknownIdentifier`/type error).
  `solvable/` statements must pass with their proof.
- **The statement IS the attack surface.** A subtly-false statement that is
  trivially provable = fraud-by-technicality. Only post statements whose meaning
  is faithful to the named problem — either a mathlib definition is cited, or the
  Prop is hand-authored from first principles and reviewed (see provenance below).

## Do NOT post (not faithfully stateable)

P vs NP, Navier–Stokes existence/smoothness, the Hodge conjecture, Yang–Mills
mass gap. These require formalizing objects (Turing machines + polynomial-time,
PDE regularity, Hodge classes, quantum field theories) whose faithful Lean
statement is itself an open research problem — a mis-statement would let a
technicality-proof drain the bounty. Riemann Hypothesis is the exception: mathlib
carries a vetted `RiemannHypothesis` def, so we cite it directly (if its import
elaborates under the cap; otherwise it stays here as deferred).

## Provenance (per statement)

| file | problem | basis |
|---|---|---|
| open/twin-primes.lean | Twin prime conjecture | hand-authored: ∀ n, ∃ prime p ≥ n with p+2 prime |
| open/goldbach.lean | Goldbach's conjecture | hand-authored: every even n>2 is a sum of two primes |
| open/legendre.lean | Legendre's conjecture | hand-authored: a prime between n² and (n+1)² |
| open/erdos-straus.lean | Erdős–Straus conjecture | hand-authored: 4/n = 1/a+1/b+1/c (cleared of denominators) |
| open/collatz.lean | Collatz conjecture | hand-authored collatz iterate reaches 1 |
| solvable/prime-101.lean | 101 is prime | `Nat.Prime 101`, proof `by norm_num` |
| solvable/prime-1000003.lean | 1000003 is prime | `Nat.Prime 1000003`, proof `by norm_num` |
