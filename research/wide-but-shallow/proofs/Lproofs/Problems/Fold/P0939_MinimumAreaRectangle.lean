import Lproofs.Schemes.Fold

/-! @lc 939 | name:Minimum Area Rectangle | scheme:fold | family:hashing | complexity:O(n²) |
    source:https://leetcode.com/problems/minimum-area-rectangle/

    Given points, find the smallest axis-aligned rectangle with all four corners among them. The
    accepted solution scans pairs of points that could be a diagonal `(x₁,y₁)–(x₂,y₂)` (distinct x and
    y), and counts it when the other two corners `(x₁,y₂)` and `(x₂,y₁)` are in the point set (a hash
    set); the area is `|x₁−x₂|·|y₁−y₂|`. CLASSIFICATION (fold): the scan over candidate diagonals is a
    streaming fold against the membership set. CORRECTNESS (soundness, not minimality): we certify that
    every area the scan accepts genuinely comes from four points present in the set that form an
    axis-aligned rectangle of that area. -/

namespace LC.P0939
open Interview.Patterns

abbrev Pt := ℤ × ℤ

/-- The four corners implied by a diagonal `(x₁,y₁)–(x₂,y₂)` are all in the set (membership as `Bool`). -/
def formsRect (inS : Pt → Bool) (x1 y1 x2 y2 : ℤ) : Bool :=
  inS (x1, y1) && inS (x2, y2) && inS (x1, y2) && inS (x2, y1)

/-- The candidate accepted by the scan: a non-degenerate diagonal whose four corners are present. -/
def accept (inS : Pt → Bool) (x1 y1 x2 y2 : ℤ) : Bool :=
  decide (x1 ≠ x2) && decide (y1 ≠ y2) && formsRect inS x1 y1 x2 y2

/-- The reported area for that diagonal. -/
def area (x1 y1 x2 y2 : ℤ) : ℤ := |x1 - x2| * |y1 - y2|

/-- The accepted-diagonal tally the scan performs over the candidate list. -/
def sol (inS : Pt → Bool) (L : List (ℤ × ℤ × ℤ × ℤ)) : ℕ :=
  L.countP fun q => accept inS q.1 q.2.1 q.2.2.1 q.2.2.2

/-- SCHEME (fold): the area search's tally `sol` is a streaming right fold over the candidates. -/
theorem cls (inS : Pt → Bool) : IsRightFold (sol inS) := by
  refine ⟨fun q n => if accept inS q.1 q.2.1 q.2.2.1 q.2.2.2 then n + 1 else n, 0, fun L => ?_⟩
  unfold sol
  induction L with
  | nil => rfl
  | cons q t ih => simp only [List.countP_cons, List.foldr_cons, ih]; split <;> omega

/-- CORRECT (soundness): an accepted candidate genuinely has its four corners in the set, and its
    reported area is positive (a non-degenerate rectangle). -/
theorem corr (inS : Pt → Bool) (x1 y1 x2 y2 : ℤ) (h : accept inS x1 y1 x2 y2 = true) :
    sol inS [(x1, y1, x2, y2)] = 1 ∧ formsRect inS x1 y1 x2 y2 = true ∧
      0 < area x1 y1 x2 y2 := by
  refine ⟨by simp [sol, List.countP_cons, h], ?_⟩
  simp only [accept, Bool.and_eq_true, decide_eq_true_eq] at h
  obtain ⟨⟨hx, hy⟩, hrect⟩ := h
  refine ⟨hrect, ?_⟩
  have h1 : 0 < |x1 - x2| := by rwa [abs_pos, sub_ne_zero]
  have h2 : 0 < |y1 - y2| := by rwa [abs_pos, sub_ne_zero]
  exact mul_pos h1 h2


/-- Official example 1 point set: [[1,1],[1,3],[3,1],[3,3],[2,2]]. -/
def exInS : Pt → Bool := fun p => decide (p ∈ [((1 : ℤ), (1 : ℤ)), (1, 3), (3, 1), (3, 3), (2, 2)])

/-- GROUND INSTANCE (official example 1): the diagonal (1,1)–(3,3) is accepted with area 4 (the
    judge's answer); the diagonal (1,1)–(2,3) is rejected (corner (2,1) missing) — the tally
    counts exactly one of the two. -/
theorem vec : sol exInS [(1, 1, 3, 3), (1, 1, 2, 3)] = 1 ∧ area 1 1 3 3 = 4 := by decide

end LC.P0939
