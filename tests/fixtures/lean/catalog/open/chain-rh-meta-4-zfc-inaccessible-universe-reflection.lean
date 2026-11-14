import Mathlib.SetTheory.ZFC.VonNeumann
import Mathlib.SetTheory.ZFC.Ordinal
import Mathlib.SetTheory.ZFC.Cardinal
import Mathlib.SetTheory.Cardinal.Regular

/-- A cardinal `κ` indexes a Zermelo–Grothendieck ZFC universe `V_{κ.ord}` inside `ZFSet.{0}` if
    `ℵ₀ < κ`, the standard von Neumann natural-number set `ω` lies inside `V_{κ.ord}`, every set in
    `V_{κ.ord}` has cardinality `< κ`, and `V_{κ.ord}` is closed under definable ZFC Replacement. -/
def isZfcReplacementUniverse (κ : Cardinal.{0}) : Prop :=
  Cardinal.aleph0 < κ ∧
  (Ordinal.omega0).toZFSet ∈ ZFSet.vonNeumann κ.ord ∧
  (∀ x ∈ ZFSet.vonNeumann κ.ord, ZFSet.card x < κ) ∧
  (∀ (x : ZFSet.{0}) (f : ZFSet.{0} → ZFSet.{0}) [ZFSet.Definable₁ f],
    x ∈ ZFSet.vonNeumann κ.ord →
    (∀ y ∈ x, f y ∈ ZFSet.vonNeumann κ.ord) →
    ZFSet.image f x ∈ ZFSet.vonNeumann κ.ord)

/-- Track E [Rung 4/4 → Π₁⁰ RH Metamathematics] (Zermelo–Grothendieck V_κ Universe Equivalence, 1930):
    Proves that for every cardinal `κ : Cardinal.{0}`, `κ` is strongly inaccessible
    (`Cardinal.IsInaccessible κ`) if and only if the cumulative rank stage `V_{κ.ord}` is an
    ω-containing transitive ZFC Replacement universe of `< κ`-sized sets. -/
def statementProp : Prop :=
  ∀ (κ : Cardinal.{0}), Cardinal.IsInaccessible κ ↔ isZfcReplacementUniverse κ
