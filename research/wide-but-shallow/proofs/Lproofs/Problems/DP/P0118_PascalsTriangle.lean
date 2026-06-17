import Lproofs.Schemes.Fold

/-! @lc 118 | name:Pascal's Triangle | scheme:dp | family:dp-linear | complexity:O(n²) |
    source:https://leetcode.com/problems/pascals-triangle/

    Build the first `n` rows of Pascal's triangle. CLASSIFICATION: a streaming left fold over the row
    index whose accumulator is the current row — the next row is the elementwise sum of the row padded
    on the right (`row ++ [0]`) and on the left (`0 :: row`), giving the leading 1, the adjacent
    pairwise sums, and the trailing 1. NON-VACUITY: we prove the row genuinely *widens by one* each
    level (row `r` has `r+1` entries), the real triangular dp shape. We certify the fold + the widening. -/

namespace LC.P0118
open Interview.Patterns

/-- Next Pascal row = (row padded right) elementwise + (row padded left). -/
def stepRow (row : List ℕ) : List ℕ := (row ++ [0]).zipWith (· + ·) (0 :: row)

/-- The row after `r` levels, seeded with the apex `[1]`. -/
def lastRow (r : ℕ) : List ℕ := (List.range r).foldl (fun row _ => stepRow row) [1]

/-- SCHEME (dp / fold over rows): each level is one fold step applying `stepRow`. -/
theorem cls : IsFold (fun rng : List ℕ => rng.foldl (fun row _ => stepRow row) [1]) :=
  ⟨fun row _ => stepRow row, [1], fun _ => rfl⟩

/-- One level widens the row by exactly one entry (both padded lists have equal length). -/
theorem stepRow_length (row : List ℕ) : (stepRow row).length = row.length + 1 := by
  simp [stepRow, List.length_zipWith, List.length_append, List.length_cons]

/-- Folding `r` levels from `init` widens by `r`. -/
theorem fold_length (init : List ℕ) : ∀ (rng : List ℕ),
    (rng.foldl (fun row _ => stepRow row) init).length = init.length + rng.length
  | [] => by simp
  | _ :: rs => by
    rw [List.foldl_cons, fold_length (stepRow init) rs, stepRow_length, List.length_cons]; omega

/-- NON-VACUITY: row `r` has exactly `r + 1` entries — the genuine triangular dp shape. -/
theorem corr (r : ℕ) : (lastRow r).length = r + 1 := by
  rw [lastRow, fold_length, List.length_singleton, List.length_range]; omega

end LC.P0118
