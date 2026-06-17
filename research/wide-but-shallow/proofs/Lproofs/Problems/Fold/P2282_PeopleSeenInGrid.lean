import Lproofs.Schemes.Fold

/-! @lc 2282 | name:Number of People That Can Be Seen in a Grid | scheme:fold | family:monotonic-stack |
    complexity:O(n) | source:https://leetcode.com/problems/number-of-people-that-can-be-seen-in-a-grid/

    Per row and column, a decreasing monotonic stack counts how many people each person can see: pop
    strictly-shorter tops while scanning, then push. CLASSIFICATION: the scan is a streaming left
    fold whose accumulator is the monotonic stack. We certify the fold and its soundness — the stack
    only ever holds heights read from the input. -/

namespace LC.P2282
open Interview.Patterns

/-- One step of the decreasing monotonic stack: pop strictly-smaller tops, then push `x`. -/
def step (stk : List ℤ) (x : ℤ) : List ℤ := x :: stk.dropWhile (· < x)

/-- Stream all heights through the monotonic stack. -/
def run (xs : List ℤ) : List ℤ := xs.foldl step []

/-- SCHEME (fold): the monotonic-stack scan is a streaming left fold. -/
theorem cls : IsFold (fun xs : List ℤ => xs.foldl step []) := ⟨step, [], fun _ => rfl⟩

/-- `dropWhile` only drops, never adds: membership carries back to the original list. -/
theorem mem_of_mem_dropWhile {y : ℤ} {p : ℤ → Bool} :
    ∀ {l : List ℤ}, y ∈ l.dropWhile p → y ∈ l := by
  intro l
  induction l with
  | nil => simp
  | cons a t ih =>
    simp only [List.dropWhile_cons]
    split
    · intro h; exact List.mem_cons_of_mem _ (ih h)
    · intro h; exact h

/-- The stack only holds elements seen so far: every member came from `init` or the input. -/
theorem foldl_step_mem (y : ℤ) :
    ∀ (xs init : List ℤ), y ∈ xs.foldl step init → y ∈ init ∨ y ∈ xs := by
  intro xs
  induction xs with
  | nil => intro init h; exact Or.inl (by simpa using h)
  | cons x t ih =>
    intro init h
    rw [List.foldl_cons] at h
    rcases ih (step init x) h with h' | h'
    · rw [step, List.mem_cons] at h'
      rcases h' with rfl | h'
      · exact Or.inr List.mem_cons_self
      · exact Or.inl (mem_of_mem_dropWhile h')
    · exact Or.inr (List.mem_cons_of_mem _ h')

/-- CORRECT (soundness): every height on the stack was read from the input. -/
theorem corr (xs : List ℤ) (y : ℤ) (h : y ∈ run xs) : y ∈ xs := by
  rcases foldl_step_mem y xs [] h with h' | h'
  · simp at h'
  · exact h'

end LC.P2282
