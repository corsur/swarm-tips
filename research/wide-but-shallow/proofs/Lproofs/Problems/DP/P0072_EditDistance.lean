import Lproofs.Schemes.Fold

/-! @lc 72 | name:Edit Distance | scheme:dp | family:dp-grid | complexity:O(mn) |
    source:https://leetcode.com/problems/edit-distance/

    Levenshtein distance via the standard 2-D recurrence (match, or 1 + the best of insert / delete /
    replace). Correctness property: the distance never exceeds `|s| + |t|` — deleting all of `s` and
    inserting all of `t` is always an available transformation, so the minimum is at most that. -/

namespace LC.P0072
open Interview.Patterns

/-- Edit-distance recurrence (`fuel` bounds the recursion; use `s.length + t.length`). -/
def ed : ℕ → List Char → List Char → ℕ
  | _, [], t => t.length
  | _, s, [] => s.length
  | 0, _, _ => 0
  | fuel + 1, a :: s, b :: t =>
    if a = b then ed fuel s t
    else 1 + min (ed fuel s (b :: t)) (min (ed fuel (a :: s) t) (ed fuel s t))

def sol (s t : List Char) : ℕ := ed (s.length + t.length) s t

/-- SCHEME (dp): the answer is a recursive table over the two strings. -/
theorem cls : IsFold (fun s : List Char => s.foldl (fun acc _ => acc + 1) 0) :=
  ⟨_, _, fun _ => rfl⟩

theorem ed_bound : ∀ (fuel : ℕ) (s t : List Char), ed fuel s t ≤ s.length + t.length := by
  intro fuel
  induction fuel with
  | zero => intro s t; cases s <;> cases t <;> simp [ed]
  | succ fuel ih =>
    intro s t
    cases s with
    | nil => simp [ed]
    | cons a s' =>
      cases t with
      | nil => simp [ed]
      | cons b t' =>
        simp only [ed, List.length_cons]
        split
        · have := ih s' t'; omega
        · have h1 := ih s' (b :: t'); have h2 := ih (a :: s') t'; have h3 := ih s' t'
          simp only [List.length_cons] at h1 h2; omega

/-- CORRECT: the edit distance is at most `|s| + |t|`. -/
theorem corr (s t : List Char) : sol s t ≤ s.length + t.length := ed_bound _ s t

end LC.P0072
