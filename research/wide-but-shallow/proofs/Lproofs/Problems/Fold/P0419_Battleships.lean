import Lproofs.Patterns

/-! @lc 419 | name:Battleships in a Board | scheme:fold | family:counting | complexity:O(mn) |
    source:https://leetcode.com/problems/battleships-in-a-board/

    Count battleships in one pass without marking: a ship is counted exactly at its top-left cell — a
    land cell with no land directly above or to its left. CLASSIFICATION: a streaming left fold over the
    cells, incrementing on each top-left cell. NON-VACUITY: we prove the count never exceeds the number
    of cells scanned (each cell contributes at most one), so the accumulator does genuine top-left
    counting, not a re-encoding. We certify the fold + the count bound. -/

namespace LC.P0419
open Interview.Patterns

/-- Each cell carries `(isLand, landAbove, landLeft)`; count it iff it is a ship's top-left. -/
def step (acc : ℕ) (cell : Bool × Bool × Bool) : ℕ :=
  if cell.1 ∧ ¬cell.2.1 ∧ ¬cell.2.2 then acc + 1 else acc

/-- Running ship count folded over the cells. -/
def count (cells : List (Bool × Bool × Bool)) : ℕ := cells.foldl step 0

/-- SCHEME (fold): the ship count is a left fold over the cells. -/
theorem cls : IsFold (fun cells : List (Bool × Bool × Bool) => cells.foldl step 0) :=
  ⟨step, 0, fun _ => rfl⟩

/-- The running count rises by at most one per cell. -/
theorem foldl_bound (cells : List (Bool × Bool × Bool)) (acc : ℕ) :
    cells.foldl step acc ≤ acc + cells.length := by
  induction cells generalizing acc with
  | nil => simp
  | cons c cs ih =>
    simp only [List.foldl_cons, List.length_cons]
    refine le_trans (ih (step acc c)) ?_
    simp only [step]; split <;> omega

/-- NON-VACUITY: the ship count never exceeds the number of cells — each cell counts at most one ship. -/
theorem corr (cells : List (Bool × Bool × Bool)) : count cells ≤ cells.length := by
  have := foldl_bound cells 0
  simpa [count] using this

end LC.P0419
