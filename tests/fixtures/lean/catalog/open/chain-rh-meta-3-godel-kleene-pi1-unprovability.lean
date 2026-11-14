import Mathlib.Computability.PartrecCode
import Mathlib.Computability.RE

/-- A partial-recursive program `c` on input `0` is halting-free (the canonical Kleene Π₁⁰
    normal form `∀ k, evaln k c 0 = none` asserting zero finite-step counterexamples exist). -/
def isHaltingFree (c : Nat.Partrec.Code) : Prop :=
  ∀ (k : ℕ), Nat.Partrec.Code.evaln k c 0 = none

/-- Track E [Rung 3/4 → Π₁⁰ RH Metamathematics] (Gödel–Kleene Π₁⁰ Unprovability & Conservativity Barrier):
    (1) Every computable Boolean check `P : ℕ → Bool` compiles to a counterexample-search code `c`
        such that `(∀ m : ℕ, P m = true) ↔ isHaltingFree c`;
    (2) For every recursively enumerable proof predicate `ProvPos : Code → Prop` that is sound on
        halting-free codes (`∀ c, ProvPos c → isHaltingFree c`), there exists a code `c₀` with zero
        counterexamples in `ℕ` (`isHaltingFree c₀`) that `ProvPos` cannot prove (`¬ ProvPos c₀`),
        that NO Π₁⁰-conservative extension of `ProvPos` can prove, and that a strictly stronger
        sound `REPred` extension `ProvExt` DOES prove. -/
def statementProp : Prop :=
  (∀ (P : ℕ → Bool), Computable P →
    ∃ (c : Nat.Partrec.Code), (∀ m : ℕ, P m = true) ↔ isHaltingFree c) ∧
  (∀ (ProvPos : Nat.Partrec.Code → Prop),
    REPred ProvPos →
    (∀ c : Nat.Partrec.Code, ProvPos c → isHaltingFree c) →
    ∃ (c₀ : Nat.Partrec.Code),
      isHaltingFree c₀ ∧
      ¬ ProvPos c₀ ∧
      (∀ (ProvCons : Nat.Partrec.Code → Prop),
        (∀ c : Nat.Partrec.Code, ProvCons c → ProvPos c) → ¬ ProvCons c₀) ∧
      ∃ (ProvExt : Nat.Partrec.Code → Prop),
        REPred ProvExt ∧
        (∀ c : Nat.Partrec.Code, ProvExt c → isHaltingFree c) ∧
        (∀ c : Nat.Partrec.Code, ProvPos c → ProvExt c) ∧
        ProvExt c₀)
