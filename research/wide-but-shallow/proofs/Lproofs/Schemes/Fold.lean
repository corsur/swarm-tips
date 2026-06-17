import Lproofs.Patterns

/-!
# Scheme 1 — streaming fold: verified combinators

The reusable core for per-problem fold certificates. A fold-shaped solution's correctness
(`corr : sol = spec`) typically reduces to a single inductive invariant on the accumulator;
`fold_invariant` is that workhorse. `IsFold` (the scheme-membership predicate) is re-exported
from `Interview.Patterns`.
-/

namespace Interview.Schemes

export Interview.Patterns (IsFold)

/-- **Fold invariant.** A predicate `P` true at `init` and preserved by every `step` holds at the
    final accumulator. Reduces a fold-shaped `corr` to "P at init" plus "P preserved". -/
theorem fold_invariant {α β : Type*} (P : β → Prop) (step : β → α → β) (init : β)
    (h0 : P init) (hs : ∀ b a, P b → P (step b a)) :
    ∀ xs : List α, P (xs.foldl step init) := by
  intro xs
  induction xs generalizing init with
  | nil => exact h0
  | cons x xs ih => exact ih (step init x) (hs init x h0)

/-- **Filter is a fold.** Building the kept-elements list by a left fold (the canonical
    write-pointer / two-pointer in-place compaction) equals `List.filter`. Unlocks the whole
    remove/keep/compact family (remove element, remove list elements, move zeroes, …). -/
theorem foldl_filter_append {α : Type*} (p : α → Bool) (xs acc : List α) :
    xs.foldl (fun a x => if p x then a ++ [x] else a) acc = acc ++ xs.filter p := by
  induction xs generalizing acc with
  | nil => simp
  | cons x xs ih =>
    simp only [List.foldl_cons, List.filter_cons]
    cases h : p x <;> simp [ih, h, List.append_assoc]

end Interview.Schemes
