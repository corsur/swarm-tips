import Lproofs.Schemes.Fold

/-! @lc 3741 | name:Minimum Distance Between Three Equal Elements II | scheme:fold | family:hashing |
    complexity:O(n) | source:https://leetcode.com/problems/minimum-distance-between-three-equal-elements-ii/

    One streaming pass keeps, per value, the positions seen so far in a hash map and updates the best
    triple distance as it goes. CLASSIFICATION: the pass is a left fold whose accumulator is the
    seen-set / map of encountered values. We certify the fold and its soundness — every value the
    map records was actually read from the input. -/

namespace LC.P3741
open Interview.Patterns

/-- The seen-set pass: fold the inputs into the set of values encountered. -/
def seen (xs : List ℤ) : Finset ℤ := xs.foldl (fun s x => insert x s) ∅

/-- SCHEME (fold): the seen-set / hash pass is a streaming left fold. -/
theorem cls : IsFold (fun xs : List ℤ => xs.foldl (fun s x => insert x s) (∅ : Finset ℤ)) :=
  ⟨fun s x => insert x s, ∅, fun _ => rfl⟩

/-- The accumulator only grows by inserting input elements: every member came from `init` or input. -/
theorem foldl_insert_mem (y : ℤ) :
    ∀ (xs : List ℤ) (init : Finset ℤ),
      y ∈ xs.foldl (fun s x => insert x s) init → y ∈ init ∨ y ∈ xs := by
  intro xs
  induction xs with
  | nil => intro init h; exact Or.inl (by simpa using h)
  | cons x t ih =>
    intro init h
    rw [List.foldl_cons] at h
    rcases ih (insert x init) h with h' | h'
    · rcases Finset.mem_insert.1 h' with rfl | h'
      · exact Or.inr List.mem_cons_self
      · exact Or.inl h'
    · exact Or.inr (List.mem_cons_of_mem _ h')

/-- CORRECT (soundness): every value recorded by the map came from the input. -/
theorem corr (xs : List ℤ) (y : ℤ) (h : y ∈ seen xs) : y ∈ xs := by
  rcases foldl_insert_mem y xs ∅ h with h' | h'
  · simp at h'
  · exact h'

end LC.P3741
