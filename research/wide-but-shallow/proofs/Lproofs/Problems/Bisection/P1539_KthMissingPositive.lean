import Lproofs.Schemes.Bisect
import Mathlib.Data.List.Nodup

/-! @lc 1539 | name:Kth Missing Positive Number | scheme:bisection | family:binary-search |
    complexity:O(log n) | source:https://leetcode.com/problems/kth-missing-positive-number/

    `arr` is strictly increasing (hence `Nodup`). The count of missing positives in `[1, m]` is
    `missing m = m - |{a ∈ arr : a ≤ m}|`, which is non-decreasing in `m` (each step adds one
    candidate integer and, since `arr` has no duplicates, removes at most one). So "at least `k`
    missing by `m`" is a monotone threshold and the accepted binary search returns its least `m`.
    CLASSIFICATION ONLY: we certify the threshold structure (the real missing-count predicate is
    monotone, answer is its `Nat.find`), not a separate optimality claim. -/

namespace LC.P1539
open Interview.Patterns

/-- Number of array entries `≤ m` (the real prefix count the binary search compares against). -/
def countLE (arr : List ℕ) (m : ℕ) : ℕ := (arr.filter (· ≤ m)).length

/-- Missing positive integers in `[1, m]`. -/
def missing (arr : List ℕ) (m : ℕ) : ℕ := m - countLE arr m

/-- `feasible m` = at least `k` positives are missing by `m`. -/
def feasible (arr : List ℕ) (k m : ℕ) : Prop := k ≤ missing arr m

instance (arr : List ℕ) (k : ℕ) : DecidablePred (feasible arr k) :=
  fun m => inferInstanceAs (Decidable (k ≤ missing arr m))

/-- The accepted binary-search answer: the least `m` with `k` missing positives by it. -/
def sol (arr : List ℕ) (k : ℕ) (h : ∃ m, feasible arr k m) : ℕ := Nat.find h

/-- The `≤ m+1` prefix count is the `≤ m` count plus the entries equal to `m+1`. -/
theorem countLE_succ (arr : List ℕ) (m : ℕ) :
    countLE arr (m + 1) = countLE arr m + (arr.filter (· = m + 1)).length := by
  induction arr with
  | nil => simp [countLE]
  | cons a as ih =>
    simp only [countLE] at ih ⊢
    simp only [List.filter_cons]
    by_cases h2 : a ≤ m
    · have h1 : a ≤ m + 1 := by omega
      have h3 : ¬ a = m + 1 := by omega
      simp [h1, h2, h3]; omega
    · by_cases h3 : a = m + 1
      · have h1 : a ≤ m + 1 := by omega
        simp [h1, h2, h3]; omega
      · have h1 : ¬ a ≤ m + 1 := by omega
        simp [h1, h2, h3]; omega

/-- In a `Nodup` list at most one entry equals a given value. -/
theorem filter_eq_le_one (arr : List ℕ) (hnd : arr.Nodup) (v : ℕ) :
    (arr.filter (· = v)).length ≤ 1 := by
  induction arr with
  | nil => simp
  | cons a as ih =>
    rw [List.nodup_cons] at hnd
    obtain ⟨ha, hnd'⟩ := hnd
    simp only [List.filter_cons]
    by_cases h : a = v
    · subst h
      have hnil : as.filter (· = a) = [] := by
        rw [List.filter_eq_nil_iff]
        intro x hx
        simp only [decide_eq_true_eq]
        intro hxa
        subst hxa
        exact ha hx
      simp [hnil]
    · simp only [h, decide_false, Bool.false_eq_true, if_false]
      exact ih hnd'

/-- NON-VACUITY (monotone threshold): the missing-count is non-decreasing, since each unit of `m`
    adds one candidate and removes at most one array entry (`Nodup`). -/
theorem missing_mono (arr : List ℕ) (hnd : arr.Nodup) : Monotone (missing arr) := by
  apply monotone_nat_of_le_succ
  intro m
  have hstep := countLE_succ arr m
  have hle := filter_eq_le_one arr hnd (m + 1)
  simp only [missing]
  omega

theorem feasible_mono (arr : List ℕ) (k : ℕ) (hnd : arr.Nodup) :
    ∀ a b, a ≤ b → feasible arr k a → feasible arr k b :=
  fun _ _ hab ha => le_trans ha (missing_mono arr hnd hab)

/-- SCHEME (bisection): the missing-count predicate is a monotone threshold —
    `feasible n ↔ (least feasible m) ≤ n` — exactly what the binary search exploits. -/
theorem cls (arr : List ℕ) (k : ℕ) (hnd : arr.Nodup) (h : ∃ m, feasible arr k m) (n : ℕ) :
    feasible arr k n ↔ sol arr k h ≤ n :=
  bisection_threshold (feasible arr k) (feasible_mono arr k hnd) h n

/-- The answer is the least feasible `m` (the threshold). -/
theorem corr (arr : List ℕ) (k : ℕ) (h : ∃ m, feasible arr k m) :
    IsLeast {m | feasible arr k m} (sol arr k h) :=
  bisection_isLeast (feasible arr k) h

end LC.P1539
