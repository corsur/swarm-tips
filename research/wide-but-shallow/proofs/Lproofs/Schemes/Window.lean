import Lproofs.Schemes.Fold

/-!
# Sliding window — verified structural invariant

A sliding-window solution scans left to right; the window is a left fold whose accumulator is the
current window contents. Each step appends the new element (expand) then drops a front prefix
(contract). The reusable correctness property: the window is always a contiguous infix of the
processed prefix — i.e. a *valid subarray*. Per-problem `corr`s instantiate this (every window the
algorithm inspects is a genuine subarray of the input), which is the one-directional guarantee the
two-pointer/sliding-window family rests on.
-/

namespace Interview.Schemes

/-- A contraction step drops only from the front: its output is a suffix of its input. -/
def DropsFront {α : Type*} (shrink : List α → List α) : Prop := ∀ l, (shrink l).IsSuffix l

/-- The window after a sliding-window fold: append each element, then contract from the front. -/
def windowFold {α : Type*} (shrink : List α → List α) (xs : List α) : List α :=
  xs.foldl (fun w x => shrink (w ++ [x])) []

/-- Appending the same element on the right preserves the suffix relation. -/
theorem isSuffix_append_right {α : Type*} {w p : List α} (h : w <:+ p) (x : α) :
    (w ++ [x]) <:+ (p ++ [x]) := by
  obtain ⟨t, ht⟩ := h
  exact ⟨t, by rw [← List.append_assoc, ht]⟩

/-- INVARIANT: the window is always a suffix of the processed prefix, hence a contiguous subarray
    of the input. This is the structural soundness the sliding-window scheme rests on. -/
theorem window_isSuffix {α : Type*} (shrink : List α → List α) (hs : DropsFront shrink) :
    ∀ xs : List α, (windowFold shrink xs) <:+ xs := by
  suffices h : ∀ (xs w p : List α), w <:+ p →
      (xs.foldl (fun w x => shrink (w ++ [x])) w) <:+ (p ++ xs) by
    intro xs
    have := h xs [] [] List.nil_suffix
    simpa [windowFold] using this
  intro xs
  induction xs with
  | nil => intro w p hw; simpa using hw
  | cons x t ih =>
    intro w p hw
    simp only [List.foldl_cons]
    have h2 : shrink (w ++ [x]) <:+ (p ++ [x]) := (hs (w ++ [x])).trans (isSuffix_append_right hw x)
    have := ih (shrink (w ++ [x])) (p ++ [x]) h2
    simpa using this

/-- The sliding-window scan is a streaming left fold. -/
theorem windowFold_isFold {α : Type*} (shrink : List α → List α) :
    Interview.Patterns.IsFold (windowFold shrink) :=
  ⟨fun w x => shrink (w ++ [x]), [], fun _ => rfl⟩

/-- Dropping a front run (`dropWhile`) returns a suffix. -/
theorem dropWhile_suffix {α : Type*} (p : α → Bool) : ∀ l : List α, (l.dropWhile p) <:+ l := by
  intro l
  induction l with
  | nil => simp
  | cons a t ih =>
    simp only [List.dropWhile_cons]
    split
    · exact ih.trans (List.suffix_cons a t)
    · exact List.suffix_refl _

/-- The canonical left-pointer advance — contract by dropping a leading run — drops only from the
    front, so it is a valid `DropsFront` contraction for `window_isSuffix`. -/
theorem dropWhile_dropsFront {α : Type*} (p : α → Bool) :
    DropsFront (fun w : List α => w.dropWhile p) := dropWhile_suffix p

end Interview.Schemes
