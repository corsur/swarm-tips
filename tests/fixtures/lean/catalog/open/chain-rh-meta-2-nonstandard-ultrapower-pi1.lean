import Mathlib.Order.Filter.Ultrafilter.Basic

/-- Track E [Rung 2/4 → Π₁⁰ RH Metamathematics] (Non-Standard Arithmetic Ultrapowers):
    Establishes the model-theoretic dichotomy for Π₁⁰ counterexamples using a non-principal
    ultrafilter `𝒰 : Ultrafilter ℕ` (`↑𝒰 ≤ Filter.cofinite`):
    (1) The diagonal element `id : ℕ → ℕ` dominates every standard natural `(fun _ => n)` modulo `𝒰`,
        yet pure ultrapowers of `ℕ` preserve every Π₁⁰ truth `(∀ n, P n) ↔ (∀ x : ℕ → ℕ, {i | P (x i)} ∈ 𝒰)`;
    (2) Conversely, for any family of stage predicates `P : ℕ → ℕ → Prop` where stage `i` holds on
        `[0, i]` (`∀ n ≤ i, P i n`) yet fails at `i + 1` (`¬ P i (i + 1)`), every standard natural
        satisfies `{i | P i n} ∈ 𝒰` while the non-standard element `h(i) = i + 1` strictly dominates
        all standard naturals modulo `𝒰` and violates `{i | P i (h i)} ∈ 𝒰`. -/
def statementProp : Prop :=
  ∃ (𝒰 : Ultrafilter ℕ),
    (↑𝒰 : Filter ℕ) ≤ Filter.cofinite ∧
    (∀ n : ℕ, {i : ℕ | n < i} ∈ 𝒰) ∧
    (∀ (P : ℕ → Prop), (∀ n : ℕ, P n) ↔ (∀ x : ℕ → ℕ, {i : ℕ | P (x i)} ∈ 𝒰)) ∧
    (∀ (P : ℕ → ℕ → Prop),
      (∀ i n : ℕ, n ≤ i → P i n) →
      (∀ i : ℕ, ¬ P i (i + 1)) →
      (∀ n : ℕ, {i : ℕ | P i n} ∈ 𝒰) ∧
      ∃ (h : ℕ → ℕ), (∀ n : ℕ, {i : ℕ | n < h i} ∈ 𝒰) ∧ {i : ℕ | P i (h i)} ∉ 𝒰)
