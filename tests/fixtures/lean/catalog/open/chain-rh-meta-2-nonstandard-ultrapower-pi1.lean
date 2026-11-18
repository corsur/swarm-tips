-- @lean-policy: 4
-- @depends-on: chain-rh-meta-1-lagarias-pi1-rational-envelope
import Mathlib.Order.Filter.Ultrafilter.Basic
import SwarmProofs.Chains.RHLagarias.Defs.V1

/-- Track E [Rung 2/4 → Π₁⁰ RH Metamathematics] (Δ₀ Łoś Ultrapower Theorem & Nonstandard Lagarias Transfer):
    Establishes bounded-arithmetic (`Δ₀`) ultrapower semantics and nonstandard truncation transfer
    across all non-principal ultrafilters `𝒰 : Ultrafilter ℕ` (`↑𝒰 ≤ Filter.cofinite`):
    (1) Non-vacuity: there exists a non-principal ultrafilter `𝒰 : Ultrafilter ℕ` refining `Filter.cofinite`;
    (2) Full Łoś's Theorem for `Δ₀` Bounded Arithmetic Formulas (`BoundedArithFormula` with de Bruijn
        `boundedForall` and `consArithEnv`): for every non-principal `𝒰`, formula `φ`, and internal
        assignment `env : ℕ → (ℕ → ℕ)`, `evalBoundedFormulaUltra 𝒰 env φ` holds iff
        `{i : ℕ | evalBoundedFormula (fun v => env v i) φ} ∈ 𝒰`;
    (3) Nonstandard Lagarias Envelope Transfer from Rung E1: assuming standard pointwise witnessability
        `∀ n ≥ 1, ∃ K ≥ 1, sigmaRat n ≤ lagariasEnvelopeRat n K`, for every internal nonstandard
        sequence `x : ℕ → ℕ` dominating all standard naturals modulo `𝒰` (`∀ m : ℕ, {i | m < x i} ∈ 𝒰`),
        there exists an internal truncation sequence `κ : ℕ → ℕ` with `{i | 1 ≤ κ i} ∈ 𝒰` and
        `{i | sigmaRat (x i) ≤ lagariasEnvelopeRat (x i) (κ i)} ∈ 𝒰`.
    Solver proofs must start with `-- SPDX-License-Identifier: Apache-2.0` for registry promotion. -/
def statementProp : Prop :=
  (∃ (𝒰 : Ultrafilter ℕ), (↑𝒰 : Filter ℕ) ≤ Filter.cofinite) ∧
  (∀ (𝒰 : Ultrafilter ℕ), (↑𝒰 : Filter ℕ) ≤ Filter.cofinite →
    (∀ (φ : BoundedArithFormula) (env : ℕ → (ℕ → ℕ)),
      evalBoundedFormulaUltra 𝒰 env φ ↔
        {i : ℕ | evalBoundedFormula (fun v => env v i) φ} ∈ 𝒰) ∧
    ((∀ (n : ℕ), 1 ≤ n → ∃ (K : ℕ), 1 ≤ K ∧ sigmaRat n ≤ lagariasEnvelopeRat n K) →
      ∀ (x : ℕ → ℕ), (∀ m : ℕ, {i : ℕ | m < x i} ∈ 𝒰) →
        ∃ (κ : ℕ → ℕ),
          {i : ℕ | 1 ≤ κ i} ∈ 𝒰 ∧
          {i : ℕ | sigmaRat (x i) ≤ lagariasEnvelopeRat (x i) (κ i)} ∈ 𝒰))
