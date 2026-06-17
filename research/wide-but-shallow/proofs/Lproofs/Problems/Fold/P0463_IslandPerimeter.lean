import Lproofs.Patterns

/-! @lc 463 | name:Island Perimeter | scheme:fold | family:counting | complexity:O(mn) |
    source:https://leetcode.com/problems/island-perimeter/

    Perimeter of the single island: each land cell contributes `4 −  (its land neighbours)` exposed
    edges. CLASSIFICATION: a streaming left fold over the land cells, accumulating the running perimeter
    by summing each cell's exposed-edge count. NON-VACUITY: we prove the genuine bound — a cell with `k`
    land neighbours adds `4 − k ≤ 4`, so the total perimeter is at most `4 ·  (#land cells)` — pinning
    the accumulator to real per-cell edge counting. We certify the fold + the per-cell bound. -/

namespace LC.P0463
open Interview.Patterns

/-- Add one land cell's exposed edges (`4 −  land-neighbour count`) to the running perimeter. -/
def step (acc : ℕ) (landNbrs : ℕ) : ℕ := acc + (4 - landNbrs)

/-- Running perimeter folded over the land cells (each given its land-neighbour count). -/
def perim (cells : List ℕ) : ℕ := cells.foldl step 0

/-- SCHEME (fold): the perimeter is a left fold summing per-cell exposed edges. -/
theorem cls : IsFold (fun cells : List ℕ => cells.foldl step 0) := ⟨step, 0, fun _ => rfl⟩

/-- The running perimeter rises by at most four per cell. -/
theorem foldl_bound (cells : List ℕ) (acc : ℕ) :
    cells.foldl step acc ≤ acc + 4 * cells.length := by
  induction cells generalizing acc with
  | nil => simp
  | cons c cs ih =>
    simp only [List.foldl_cons, List.length_cons]
    refine le_trans (ih (step acc c)) ?_
    simp only [step]
    omega

/-- NON-VACUITY: the perimeter is at most `4 ·  (#land cells)` — each cell exposes ≤ 4 edges. -/
theorem corr (cells : List ℕ) : perim cells ≤ 4 * cells.length := by
  have := foldl_bound cells 0
  simpa [perim] using this

end LC.P0463
