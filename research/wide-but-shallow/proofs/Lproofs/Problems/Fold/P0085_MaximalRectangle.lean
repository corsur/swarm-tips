import Lproofs.Schemes.Fold

/-! @lc 85 | name:Maximal Rectangle | scheme:fold | family:histogram-stack | complexity:O(mn) |
    source:https://leetcode.com/problems/maximal-rectangle/

    The accepted solution sweeps the matrix row by row, maintaining a vector of running column heights
    (reset to 0 on a `0` cell, else incremented), and applies Largest-Rectangle-in-Histogram (84) to
    each. CLASSIFICATION: the running-height sweep is a streaming left fold over the rows whose
    accumulator is the width-`w` height vector. NON-VACUITY: we prove the accumulator stays a genuine
    width-`w` vector (the dp state is real and bounded). We certify the height-fold, not the rectangle
    optimisation (deferred to 84). -/

namespace LC.P0085
open Interview.Patterns

/-- Update the running column heights against a `0/1` row: increment where the cell is `1`, else reset. -/
def heightStep (prev : List ℕ) (row : List ℕ) : List ℕ :=
  prev.zipWith (fun h c => if c = 1 then h + 1 else 0) row

/-- Sweep the matrix, threading the height vector (seeded with `w` zeros). -/
def heights (w : ℕ) (matrix : List (List ℕ)) : List ℕ :=
  matrix.foldl heightStep (List.replicate w 0)

/-- SCHEME (fold): the height sweep is a streaming left fold over the rows. -/
theorem cls (w : ℕ) : IsFold (fun matrix : List (List ℕ) => matrix.foldl heightStep (List.replicate w 0)) :=
  ⟨heightStep, List.replicate w 0, fun _ => rfl⟩

/-- One height update preserves the vector width. -/
theorem heightStep_length (prev row : List ℕ) (hp : prev.length = w) (hr : row.length = w) :
    (heightStep prev row).length = w := by
  simp only [heightStep, List.length_zipWith, hp, hr, Nat.min_self]

/-- The sweep keeps the accumulator at width `w` for any width-`w` seed and width-`w` rows. -/
theorem foldl_length (w : ℕ) : ∀ (matrix : List (List ℕ)) (acc : List ℕ),
    acc.length = w → (∀ r ∈ matrix, r.length = w) → (matrix.foldl heightStep acc).length = w
  | [], acc, hacc, _ => by simpa using hacc
  | r :: rs, acc, hacc, hm => by
    rw [List.foldl_cons]
    exact foldl_length w rs (heightStep acc r)
      (heightStep_length acc r hacc (hm r (by simp)))
      (fun x hx => hm x (List.mem_cons_of_mem r hx))

/-- NON-VACUITY: the swept accumulator stays a genuine width-`w` height vector (real bounded dp state). -/
theorem corr (w : ℕ) (matrix : List (List ℕ)) (hm : ∀ r ∈ matrix, r.length = w) :
    (heights w matrix).length = w :=
  foldl_length w matrix (List.replicate w 0) (by simp) hm

end LC.P0085
