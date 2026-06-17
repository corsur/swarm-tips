import Lproofs.Schemes.Fold
import Mathlib.Data.Rat.Defs

/-! @lc 799 | name:Champagne Tower | scheme:dp | family:grid-dp | complexity:O(r²) |
    source:https://leetcode.com/problems/champagne-tower/

    Pour champagne into the top glass; each glass passes its excess (over capacity 1) half-and-half to
    the two glasses below. The accepted solution simulates row by row. CLASSIFICATION: a streaming
    left fold over the rows whose accumulator is the current row — each glass receives the right-spill
    of its left parent plus the left-spill of its right parent. NON-VACUITY: we prove the row genuinely
    *widens by one* each level (row `r` has `r+1` glasses), so the accumulator is the real triangular
    dp state. We certify the fold + the widening, not the overflow values. -/

namespace LC.P0799
open Interview.Patterns

/-- Excess a glass spills to each child: half of whatever exceeds capacity `1`. -/
def overflow (x : ℚ) : ℚ := max 0 (x - 1) / 2

/-- Pour one level down: a child gets its left parent's spill (`0 ::`) plus its right parent's
    (`++ [0]`), so the row widens by exactly one glass. -/
def stepRow (row : List ℚ) : List ℚ :=
  (0 :: row.map overflow).zipWith (· + ·) (row.map overflow ++ [0])

/-- The tower after `r` levels, seeded with the poured amount. -/
def tower (poured : ℚ) (r : ℕ) : List ℚ :=
  (List.range r).foldl (fun row _ => stepRow row) [poured]

/-- SCHEME (dp / fold over rows): each level is one fold step applying `stepRow`. -/
theorem cls (poured : ℚ) :
    IsFold (fun rng : List ℕ => rng.foldl (fun row _ => stepRow row) [poured]) :=
  ⟨fun row _ => stepRow row, [poured], fun _ => rfl⟩

/-- One level widens the row by exactly one glass. -/
theorem stepRow_length (row : List ℚ) : (stepRow row).length = row.length + 1 := by
  simp only [stepRow, List.length_zipWith, List.length_cons, List.length_map, List.length_append,
    List.length_nil]
  omega

/-- Folding `r` levels from `init` widens by `r`. -/
theorem fold_length (init : List ℚ) : ∀ (rng : List ℕ),
    (rng.foldl (fun row _ => stepRow row) init).length = init.length + rng.length
  | [] => by simp
  | _ :: rs => by
    rw [List.foldl_cons, fold_length (stepRow init) rs, stepRow_length, List.length_cons]
    omega

/-- NON-VACUITY: row `r` has exactly `r + 1` glasses — the genuine triangular dp shape. -/
theorem corr (poured : ℚ) (r : ℕ) : (tower poured r).length = r + 1 := by
  rw [tower, fold_length, List.length_singleton, List.length_range]; omega

end LC.P0799
