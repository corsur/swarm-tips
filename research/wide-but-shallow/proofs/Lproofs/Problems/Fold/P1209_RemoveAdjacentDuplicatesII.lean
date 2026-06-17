import Lproofs.Schemes.Fold

/-! @lc 1209 | name:Remove All Adjacent Duplicates in String II | scheme:fold | family:pairing-stack |
    complexity:O(n) | source:https://leetcode.com/problems/remove-all-adjacent-duplicates-in-string-ii/

    Repeatedly delete `k` adjacent equal characters until none remain. A single streaming pass with
    a stack of `(char, run-length)` blocks does it: bump the top block on a match (popping it at
    `k`), else push a new block. Correctness: the result is fully reduced — every block has length
    in `[1, k)` and adjacent blocks differ — so no `k` equal characters remain adjacent. -/

namespace LC.P1209
open Interview.Patterns

/-- Push one character onto the run-length stack, collapsing a completed run of `k`. -/
def push (k : ℕ) (st : List (Char × ℕ)) (c : Char) : List (Char × ℕ) :=
  match st with
  | (d, n) :: rest => if c = d then (if n + 1 = k then rest else (d, n + 1) :: rest)
                      else (c, 1) :: (d, n) :: rest
  | [] => [(c, 1)]

/-- Reduce the whole string by streaming it through the stack. -/
def reduce (k : ℕ) (s : List Char) : List (Char × ℕ) := s.foldl (push k) []

/-- A fully reduced stack: every block length is in `[1, k)` and adjacent blocks differ. -/
def Reduced (k : ℕ) : List (Char × ℕ) → Prop
  | [] => True
  | [(_, n)] => 1 ≤ n ∧ n < k
  | (c, n) :: (d, m) :: rest => 1 ≤ n ∧ n < k ∧ c ≠ d ∧ Reduced k ((d, m) :: rest)

/-- The reduction returned to the caller. -/
def sol (k : ℕ) (s : List Char) : List (Char × ℕ) := reduce k s

/-- SCHEME (fold): the reduction is a single streaming pass (a left fold) over the string. -/
theorem cls (k : ℕ) : IsFold (fun s : List Char => s.foldl (push k) []) :=
  ⟨_, _, fun _ => rfl⟩

/-- One push preserves the reduced invariant. -/
theorem push_reduced {k : ℕ} (hk : 2 ≤ k) (c : Char) {st : List (Char × ℕ)}
    (h : Reduced k st) : Reduced k (push k st c) := by
  match st with
  | [] => simp only [push]; exact ⟨by omega, by omega⟩
  | [(d, n)] =>
    simp only [push]
    by_cases hcd : c = d
    · subst hcd
      simp only [if_pos rfl]
      by_cases hnk : n + 1 = k
      · simp only [if_pos hnk]; exact trivial
      · simp only [if_neg hnk, Reduced] at h ⊢; exact ⟨by omega, by omega⟩
    · simp only [if_neg hcd, Reduced] at h ⊢
      exact ⟨by omega, by omega, hcd, h⟩
  | (d, n) :: (e, p) :: rest =>
    simp only [push]
    by_cases hcd : c = d
    · subst hcd
      simp only [if_pos rfl]
      by_cases hnk : n + 1 = k
      · simp only [if_pos hnk, Reduced] at h ⊢; exact h.2.2.2
      · simp only [if_neg hnk, Reduced] at h ⊢
        exact ⟨by omega, by omega, h.2.2.1, h.2.2.2⟩
    · simp only [if_neg hcd, Reduced] at h ⊢
      exact ⟨by omega, by omega, hcd, h⟩

/-- CORRECT: the reduction is fully reduced — every block is in `[1, k)` and adjacent blocks
    differ, so no `k` equal characters remain adjacent. -/
theorem corr {k : ℕ} (hk : 2 ≤ k) (s : List Char) : Reduced k (sol k s) := by
  have key : ∀ (t : List Char) st, Reduced k st → Reduced k (t.foldl (push k) st) := by
    intro t
    induction t with
    | nil => intro st h; exact h
    | cons c rest ih => intro st h; exact ih _ (push_reduced hk c h)
  exact key s [] trivial

end LC.P1209
