import Lproofs.Schemes.Fold

/-! @lc 51 | name:N-Queens | scheme:dp | family:backtracking | complexity:exponential |
    source:https://leetcode.com/problems/n-queens/

    Place `n` non-attacking queens. CLASSIFICATION: the accepted backtracking is a recursive
    decomposition — place one queen per row, extending the partial placement only with columns that
    are safe against every queen already placed, then recurse on the remaining rows. NON-VACUITY: we
    prove every enumerated placement is genuinely valid (no two queens share a column or diagonal),
    pinning the decomposition to real constraint-checking. We certify the decomposition + soundness,
    not completeness/count. -/

namespace LC.P0051

/-- Is column `c` safe against the already-placed columns `qs` (head = the adjacent row below, at
    diagonal distance `d`)? No shared column (`c ≠ q`) and no diagonal (`c + d ≠ q`, `q + d ≠ c`). -/
def safeAux : ℕ → ℕ → List ℕ → Bool
  | _, _, [] => true
  | c, d, q :: qs => (c ≠ q) && (c + d ≠ q) && (q + d ≠ c) && safeAux c (d + 1) qs

def safe (c : ℕ) (qs : List ℕ) : Bool := safeAux c 1 qs

/-- Enumerate placements for `rows` further rows on an `n`-wide board, extending `qs`. -/
def place : ℕ → ℕ → List ℕ → List (List ℕ)
  | 0, _, qs => [qs]
  | rows + 1, n, qs =>
    (List.range n).flatMap (fun c => if safe c qs then place rows n (c :: qs) else [])

/-- The accepted answer: all full placements. -/
def sol (n : ℕ) : List (List ℕ) := place n n []

/-- A placement is valid when every queen is safe against all queens placed before it. -/
def valid : List ℕ → Prop
  | [] => True
  | c :: qs => safe c qs = true ∧ valid qs

/-- SCHEME (dp / recursive decomposition): placing the next row extends the partial placement with
    each safe column and recurses on the remaining rows. -/
theorem cls (rows n : ℕ) (qs : List ℕ) :
    place (rows + 1) n qs =
      (List.range n).flatMap (fun c => if safe c qs then place rows n (c :: qs) else []) :=
  rfl

/-- NON-VACUITY (soundness): every enumerated placement is valid (no two queens attack). -/
theorem corr : ∀ (rows n : ℕ) (qs : List ℕ), valid qs → ∀ out ∈ place rows n qs, valid out := by
  intro rows
  induction rows with
  | zero => intro n qs hqs out hmem; simp only [place, List.mem_singleton] at hmem; subst hmem; exact hqs
  | succ r ih =>
    intro n qs hqs out hmem
    rw [cls, List.mem_flatMap] at hmem
    obtain ⟨c, _, hcmem⟩ := hmem
    split at hcmem
    · rename_i hsafe
      exact ih n (c :: qs) ⟨hsafe, hqs⟩ out hcmem
    · simp at hcmem

end LC.P0051
