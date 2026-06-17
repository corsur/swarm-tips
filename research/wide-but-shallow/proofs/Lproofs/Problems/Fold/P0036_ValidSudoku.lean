import Lproofs.Schemes.Fold

/-! @lc 36 | name:Valid Sudoku | scheme:fold | family:hashing | complexity:O(1) |
    source:https://leetcode.com/problems/valid-sudoku/ -/

namespace LC.P0036
open Interview.Patterns

/-- The board's 27 units (9 rows, 9 columns, 9 boxes), blanks removed. The board is valid iff no
    unit repeats a digit. -/
def spec (units : List (List ℕ)) : Prop := ∀ u ∈ units, u.Nodup

/-- A unit's seen-set, built by a streaming fold (`= u.toFinset`). -/
def seen (u : List ℕ) : Finset ℕ := u.foldl (fun s x => insert x s) (∅ : Finset ℕ)

/-- Editorial hash-set solution: a unit is duplicate-free exactly when its seen-set has one entry
    per element — i.e. `(seen u).card = u.length`. The fold-built `seen u` is the actual structure. -/
def sol (units : List (List ℕ)) : Bool :=
  decide (∀ u ∈ units, (seen u).card = u.length)

/-- SCHEME (fold): the per-unit seen-set `sol` reads is the streaming fold. -/
theorem cls : IsFold (fun xs : List ℕ => xs.foldl (fun s x => insert x s) (∅ : Finset ℕ)) :=
  fold_seenSet

/-- A list is duplicate-free iff its de-duplicated finite-set has one entry per position. -/
theorem nodup_iff_card (u : List ℕ) : u.Nodup ↔ u.toFinset.card = u.length := by
  rw [List.card_toFinset]
  constructor
  · intro h; rw [List.dedup_eq_self.mpr h]
  · intro h; exact List.dedup_eq_self.mp ((List.dedup_sublist u).eq_of_length h)

/-- CORRECT: the boolean answer matches "every unit is duplicate-free". -/
theorem corr (units : List (List ℕ)) : sol units = true ↔ spec units := by
  simp only [sol, spec, seen, seenSet_eq_toFinset, decide_eq_true_eq]
  constructor
  · intro h u hu; exact (nodup_iff_card u).mpr (h u hu)
  · intro h u hu; exact (nodup_iff_card u).mp (h u hu)

end LC.P0036
