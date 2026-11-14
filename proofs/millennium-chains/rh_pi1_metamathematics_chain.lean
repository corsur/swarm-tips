import Mathlib.Data.Nat.Factorial.Basic
import Mathlib.Data.Nat.Pairing
import Mathlib.Data.Rat.Cast.Order
import Mathlib.Analysis.SpecialFunctions.Log.Basic
import Mathlib.Algebra.BigOperators.Group.Finset.Basic
import Mathlib.SetTheory.ZFC.VonNeumann
import Mathlib.SetTheory.ZFC.Ordinal
import Mathlib.SetTheory.ZFC.Cardinal
import Mathlib.SetTheory.Cardinal.Regular

open scoped BigOperators

/-- Rational divisor sum σ_ℚ(n) = ∑_{d ∈ [1, n], d ∣ n} d. -/
def sigmaRat (n : ℕ) : ℚ :=
  ∑ d ∈ (Finset.Icc 1 n).filter (fun d => d ∣ n), (d : ℚ)

/-- Rational n-th harmonic number H_ℚ(n) = ∑_{k=1}^n 1/k. -/
def harmonicRat (n : ℕ) : ℚ :=
  ∑ k ∈ Finset.Icc 1 n, (1 : ℚ) / (k : ℚ)

/-- Constructive rational Taylor upper bound on exp(x) of degree M. -/
def expUpperRat (x : ℚ) (M : ℕ) : ℚ :=
  (∑ j ∈ Finset.range M, (x ^ j) / (Nat.factorial j : ℚ)) +
    2 * (x ^ M) / (Nat.factorial M : ℚ)

/-- Constructive rational hyperbolic-arctanh upper bound on log(x) of order M. -/
def logUpperRat (x : ℚ) (M : ℕ) : ℚ :=
  let t : ℚ := (x - 1) / (x + 1)
  (2 * ∑ j ∈ Finset.range M, (t ^ (2 * j + 1)) / ((2 * j + 1 : ℕ) : ℚ)) +
    (x + 1) * (t ^ (2 * M + 1)) / ((2 * M + 1 : ℕ) : ℚ)

/-- Purely rational Lagarias upper envelope U_Lag(n, K) ∈ ℚ at truncation M = 2n + K. -/
def lagariasEnvelopeRat (n K : ℕ) : ℚ :=
  let h := harmonicRat n
  let M := 2 * n + K
  h + expUpperRat h M * logUpperRat h M

/-- Single-integer decidable Π₁⁰ matrix for Lagarias's RH criterion via Cantor unpairing
    `m ↦ (n, K) = ((unpair m).1 + 1, (unpair m).2 + 1)`. -/
def lagariasDecidableCheck (m : ℕ) : Bool :=
  decide
    (sigmaRat ((Nat.unpair m).1 + 1) ≤
      lagariasEnvelopeRat ((Nat.unpair m).1 + 1) ((Nat.unpair m).2 + 1))

/-- Real divisor sum σ(n) matching `millennium-rh-lagarias-divisor.lean`. -/
noncomputable def sigmaDivisors (n : ℕ) : ℝ :=
  ∑ d ∈ (Finset.Icc 1 n).filter (fun d => d ∣ n), (d : ℝ)

/-- Real n-th harmonic number H_n matching `millennium-rh-lagarias-divisor.lean`. -/
noncomputable def harmonicNum (n : ℕ) : ℝ :=
  ∑ k ∈ Finset.Icc 1 n, (1 : ℝ) / (k : ℝ)

/-- Lagarias's transcendental inequality at n over ℝ. -/
def lagariasBound (n : ℕ) : Prop :=
  sigmaDivisors n ≤ harmonicNum n + Real.exp (harmonicNum n) * Real.log (harmonicNum n)

/-- Live Mainnet Millennium Apex (`catalog:millennium-rh-lagarias-divisor`). -/
def MillenniumRHLagarias : Prop :=
  ∀ (n : ℕ), 1 ≤ n → lagariasBound n

/-- Rung E1: Pointwise rational Π₁⁰ envelope equivalence for Lagarias's RH criterion. -/
def RungE1 : Prop :=
  ∀ (n : ℕ), 1 ≤ n →
    (lagariasBound n ↔ ∀ (K : ℕ), 1 ≤ K → sigmaRat n ≤ lagariasEnvelopeRat n K)

/-- Zermelo–Grothendieck ZFC universe predicate for `V_{κ.ord}` inside `ZFSet.{0}`. -/
def isZfcReplacementUniverse (κ : Cardinal.{0}) : Prop :=
  Cardinal.aleph0 < κ ∧
  (Ordinal.omega0).toZFSet ∈ ZFSet.vonNeumann κ.ord ∧
  (∀ x ∈ ZFSet.vonNeumann κ.ord, ZFSet.card x < κ) ∧
  (∀ (x : ZFSet.{0}) (f : ZFSet.{0} → ZFSet.{0}) [ZFSet.Definable₁ f],
    x ∈ ZFSet.vonNeumann κ.ord →
    (∀ y ∈ x, f y ∈ ZFSet.vonNeumann κ.ord) →
    ZFSet.image f x ∈ ZFSet.vonNeumann κ.ord)

/-- Rung E4: Zermelo–Grothendieck equivalence `IsInaccessible κ ↔ isZfcReplacementUniverse κ`. -/
def RungE4 : Prop :=
  ∀ (κ : Cardinal.{0}), Cardinal.IsInaccessible κ ↔ isZfcReplacementUniverse κ

/-- Master Synthesis Theorem (Track E — Π₁⁰ Metamathematics of RH):
    (1) `RungE1` reduces `MillenniumRHLagarias` over `ℝ` (`↔`) to the single-quantifier decidable
        Boolean Π₁⁰ sentence `∀ m : ℕ, lagariasDecidableCheck m = true`;
    (2) Formalized Kreisel Counterexample Principle: for ANY refutation predicate `ProvRefutation`
        that is Σ₁⁰-complete on finite rational violations (`lagariasDecidableCheck m = false`),
        unprovability of a counterexample (`¬ ProvRefutation`) implies `MillenniumRHLagarias`;
    (3) ω-Standard Universe Reflection: inside any inaccessible `V_{κ.ord}` (`RungE4`), `(ω).toZFSet`
        lies in `V_{κ.ord}` and quantifying `lagariasDecidableCheck` over von Neumann naturals
        `z ∈ (ω).toZFSet` is strictly equivalent (`↔`) to `MillenniumRHLagarias` in `ℕ`. -/
def statementProp : Prop :=
  (RungE1 → (MillenniumRHLagarias ↔ ∀ (m : ℕ), lagariasDecidableCheck m = true)) ∧
  (RungE1 →
    ∀ (ProvRefutation : Prop),
      (∀ m : ℕ, lagariasDecidableCheck m = false → ProvRefutation) →
      (¬ ProvRefutation → MillenniumRHLagarias)) ∧
  (RungE1 → RungE4 →
    ∀ (κ : Cardinal.{0}), Cardinal.IsInaccessible κ →
      (Ordinal.omega0).toZFSet ∈ ZFSet.vonNeumann κ.ord ∧
      (MillenniumRHLagarias ↔
        ∀ z ∈ (Ordinal.omega0).toZFSet,
          ∀ m : ℕ, (m : Ordinal.{0}).toZFSet = z → lagariasDecidableCheck m = true))

lemma rh_iff_decidable_of_e1 (hE1 : RungE1) :
    MillenniumRHLagarias ↔ ∀ (m : ℕ), lagariasDecidableCheck m = true := by
  constructor
  · intro hRH m
    unfold lagariasDecidableCheck
    rw [decide_eq_true_eq]
    have hn1 : 1 ≤ (Nat.unpair m).1 + 1 := Nat.succ_le_succ (Nat.zero_le _)
    have hK1 : 1 ≤ (Nat.unpair m).2 + 1 := Nat.succ_le_succ (Nat.zero_le _)
    exact (hE1 ((Nat.unpair m).1 + 1) hn1).mp (hRH ((Nat.unpair m).1 + 1) hn1)
      ((Nat.unpair m).2 + 1) hK1
  · intro hDec n hn1
    rw [hE1 n hn1]
    intro K hK1
    let m : ℕ := Nat.pair (n - 1) (K - 1)
    have hm : lagariasDecidableCheck m = true := hDec m
    unfold lagariasDecidableCheck at hm
    rw [decide_eq_true_eq] at hm
    have hup : Nat.unpair m = (n - 1, K - 1) := Nat.unpair_pair (n - 1) (K - 1)
    have hn_eq : (Nat.unpair m).1 + 1 = n := by
      rw [hup]
      omega
    have hK_eq : (Nat.unpair m).2 + 1 = K := by
      rw [hup]
      omega
    rw [hn_eq, hK_eq] at hm
    exact hm

theorem proof : statementProp := by
  refine ⟨rh_iff_decidable_of_e1, ?_, ?_⟩
  · intro hE1 ProvRefutation hSigmaComplete hNoRefutation
    rw [rh_iff_decidable_of_e1 hE1]
    intro m
    cases hcheck : lagariasDecidableCheck m
    · exfalso
      exact hNoRefutation (hSigmaComplete m hcheck)
    · rfl
  · intro hE1 hE4 κ hInacc
    have hUniv : isZfcReplacementUniverse κ := (hE4 κ).mp hInacc
    refine ⟨hUniv.2.1, ?_⟩
    rw [rh_iff_decidable_of_e1 hE1]
    constructor
    · intro hAll z _ m _
      exact hAll m
    · intro hOmega m
      have hm_lt : (m : Ordinal.{0}) < Ordinal.omega0 := Ordinal.natCast_lt_omega0 m
      have hm_mem : (m : Ordinal.{0}).toZFSet ∈ (Ordinal.omega0).toZFSet :=
        Ordinal.toZFSet_mem_toZFSet_iff.mpr hm_lt
      exact hOmega ((m : Ordinal.{0}).toZFSet) hm_mem m rfl
