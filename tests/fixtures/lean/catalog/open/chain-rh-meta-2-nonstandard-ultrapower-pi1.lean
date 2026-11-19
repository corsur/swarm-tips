-- @lean-policy: 4
-- @depends-on: chain-rh-meta-1-lagarias-pi1-rational-envelope
import Mathlib.Order.Filter.Ultrafilter.Basic
import SwarmProofs.Chains.RHLagarias.Defs.V1

/-- Shift de Bruijn variable indices `≥ cutoff` up by `1` inside an `ArithTerm`. -/
def shiftArithTerm (cutoff : ℕ) : ArithTerm → ArithTerm
  | .var v => if v < cutoff then .var v else .var (v + 1)
  | .const k => .const k
  | .add t₁ t₂ => .add (shiftArithTerm cutoff t₁) (shiftArithTerm cutoff t₂)
  | .mul t₁ t₂ => .mul (shiftArithTerm cutoff t₁) (shiftArithTerm cutoff t₂)

/-- Shift free de Bruijn variable indices `≥ cutoff` up by `1` inside a `BoundedArithFormula`. -/
def shiftBoundedFormula (cutoff : ℕ) : BoundedArithFormula → BoundedArithFormula
  | .eq t₁ t₂ => .eq (shiftArithTerm cutoff t₁) (shiftArithTerm cutoff t₂)
  | .le t₁ t₂ => .le (shiftArithTerm cutoff t₁) (shiftArithTerm cutoff t₂)
  | .not φ => .not (shiftBoundedFormula cutoff φ)
  | .and φ ψ => .and (shiftBoundedFormula cutoff φ) (shiftBoundedFormula cutoff ψ)
  | .imp φ ψ => .imp (shiftBoundedFormula cutoff φ) (shiftBoundedFormula cutoff ψ)
  | .boundedForall bound body =>
      .boundedForall (shiftArithTerm cutoff bound) (shiftBoundedFormula (cutoff + 1) body)

/-- Bounded initial-segment quantification `∀ y ≤ v₀, ψ(y)` over de Bruijn variable `0`,
    weakening `ψ` above index `0` so outer bound variable `v₀` does not capture `ψ`'s parameters. -/
def forallLeVar0 (ψ : BoundedArithFormula) : BoundedArithFormula :=
  .boundedForall (.var 0) (shiftBoundedFormula 1 ψ)

/-- Track E [Rung 2/4 → Π₁⁰ RH Metamathematics] (Δ₀ Łoś Ultrapower Theorem & Robinson Δ₀ Overspill):
    Establishes bounded-arithmetic (`Δ₀`) ultrapower model theory and Robinson overspill across
    every non-principal ultrafilter `𝒰 : Ultrafilter ℕ` (`(↑𝒰 : Filter ℕ) ≤ Filter.cofinite`):
    (1) Full Łoś's Theorem for `Δ₀` Bounded Arithmetic Formulas (`BoundedArithFormula` with de Bruijn
        `boundedForall` and `consArithEnv`): for every formula `φ` and internal assignment
        `env : ℕ → (ℕ → ℕ)`, `evalBoundedFormulaUltra 𝒰 env φ` holds in `ℕ^ℕ / 𝒰` iff
        `{i : ℕ | evalBoundedFormula (fun v => env v i) φ} ∈ 𝒰`;
    (2a) Standard-to-Internal Π₁⁰ Initial-Segment Equivalence: for every `Δ₀` formula `ψ` and standard
         parameter environment `env : ℕ → ℕ`, `∀ N : ℕ, ℕ ⊨ (∀ y ≤ N, ψ(y))` holds iff for every
         infinite internal sequence `H : ℕ → ℕ` (`∀ m : ℕ, {i | m < H i} ∈ 𝒰`),
         `ℕ^ℕ / 𝒰 ⊨ (∀ y ≤ H, ψ(y))`;
    (2b) Internal Robinson `Δ₀` Overspill Principle: for every `Δ₀` formula `ψ` and internal
         (possibly nonstandard) parameter environment `env : ℕ → (ℕ → ℕ)`, whenever `ψ` holds on every
         standard natural `n : ℕ` inside `ℕ^ℕ / 𝒰`, there exists an infinite internal sequence
         `ω : ℕ → ℕ` (`∀ m : ℕ, {i | m < ω i} ∈ 𝒰`) such that `forallLeVar0 ψ` holds up to `ω` in
         `ℕ^ℕ / 𝒰`.
    Because Rung E1's rational Lagarias envelope bound (`sigmaRat n ≤ lagariasEnvelopeRat n K`) is a
    bounded `Δ₀` relation, Part (2a) yields the direct Π₁⁰ nonstandard transfer bridge for Rung E1.
    Solver proofs must start with `-- SPDX-License-Identifier: Apache-2.0` for registry promotion. -/
def statementProp : Prop :=
  ∀ (𝒰 : Ultrafilter ℕ), (↑𝒰 : Filter ℕ) ≤ Filter.cofinite →
    (∀ (φ : BoundedArithFormula) (env : ℕ → (ℕ → ℕ)),
      evalBoundedFormulaUltra 𝒰 env φ ↔
        {i : ℕ | evalBoundedFormula (fun v => env v i) φ} ∈ 𝒰) ∧
    (∀ (ψ : BoundedArithFormula) (env : ℕ → ℕ),
      (∀ (N : ℕ), evalBoundedFormula (consArithEnv N env) (forallLeVar0 ψ)) ↔
        ∀ (H : ℕ → ℕ), (∀ m : ℕ, {i : ℕ | m < H i} ∈ 𝒰) →
          evalBoundedFormulaUltra 𝒰 (consArithEnv H (fun v _ => env v)) (forallLeVar0 ψ)) ∧
    (∀ (ψ : BoundedArithFormula) (env : ℕ → (ℕ → ℕ)),
      (∀ (n : ℕ), evalBoundedFormulaUltra 𝒰 (consArithEnv (fun _ => n) env) ψ) →
        ∃ (ω : ℕ → ℕ),
          (∀ m : ℕ, {i : ℕ | m < ω i} ∈ 𝒰) ∧
          evalBoundedFormulaUltra 𝒰 (consArithEnv ω env) (forallLeVar0 ψ))
