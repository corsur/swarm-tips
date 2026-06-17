import Lproofs.Schemes.Fold
import Mathlib.Data.List.Sublists

/-! @lc 115 | name:Distinct Subsequences | scheme:dp | family:dp-grid | complexity:O(mn) |
    source:https://leetcode.com/problems/distinct-subsequences/

    Count how many distinct subsequences of `s` equal `t`. The standard DP recurrence either skips
    `s`'s head, or (when it matches `t`'s head) also consumes both. Correctness: this recurrence
    counts exactly the subsequence-embeddings of `t` into `s` — the sublists of `s` equal to `t`. -/

namespace LC.P0115
open Interview.Patterns

/-- Distinct-subsequence count (the DP recurrence). -/
def numDistinct : List Char → List Char → ℕ
  | _, [] => 1
  | [], _ :: _ => 0
  | x :: s, c :: t => numDistinct s (c :: t) + (if x = c then numDistinct s t else 0)

def sol (s t : List Char) : ℕ := numDistinct s t

/-- Base of the DP fold: the empty prefix admits the empty target once, nothing else. -/
def base : List Char → ℕ := fun t => if t = [] then 1 else 0

/-- One catamorphism step over `s`: the accumulator is the whole DP row `List Char → ℕ`
    (the count as a function of the remaining target) — the classic "DP table as a fold". -/
def stepS (x : Char) (rec : List Char → ℕ) : List Char → ℕ
  | [] => 1
  | c :: t' => rec (c :: t') + (if x = c then rec t' else 0)

/-- SCHEME (dp = recursive decomposition): `numDistinct` is a genuine right fold (catamorphism)
    over `s`, carrying the DP row `List Char → ℕ` as its accumulator. This is exactly `sol`. -/
theorem cls : IsRightFold (numDistinct : List Char → (List Char → ℕ)) := by
  refine ⟨stepS, base, fun s => ?_⟩
  induction s with
  | nil => funext t; cases t <;> rfl
  | cons x s ih =>
    rw [List.foldr_cons, ← ih]
    funext t; cases t <;> rfl

/-- Filtering `c :: t'` after prefixing every list with `c` counts the same as filtering `t'`. -/
theorem length_filter_map_cons (c : Char) (t' : List Char) (L : List (List Char)) :
    ((L.map (c :: ·)).filter (· == c :: t')).length = (L.filter (· == t')).length := by
  induction L with
  | nil => rfl
  | cons l ls ih =>
    by_cases h : l = t'
    · subst h; simp [List.filter_cons, List.map_cons, ih]
    · have h1 : (l == t') = false := by simpa using h
      simp [List.filter_cons, List.map_cons, h1, ih]

/-- A list prefixed with `x` can never equal `c :: t'` when `x ≠ c`. -/
theorem length_filter_map_cons_ne {x c : Char} (hxc : x ≠ c) (t' : List Char)
    (L : List (List Char)) : ((L.map (x :: ·)).filter (· == c :: t')).length = 0 := by
  rw [List.length_eq_zero_iff, List.filter_eq_nil_iff]
  intro a ha
  obtain ⟨b, _, rfl⟩ := List.mem_map.mp ha
  have hxcf : (x == c) = false := by simpa using hxc
  simp [hxcf]

/-- CORRECT: the recurrence counts exactly the sublists of `s` equal to `t`. -/
theorem corr : ∀ (s t : List Char), numDistinct s t = (s.sublists'.filter (· == t)).length := by
  intro s
  induction s with
  | nil => intro t; cases t <;> simp [numDistinct, List.sublists'_nil]
  | cons x s' ih =>
    intro t
    rw [List.sublists'_cons, List.filter_append, List.length_append]
    cases t with
    | nil =>
      have hz : ((s'.sublists'.map (x :: ·)).filter (· == [])).length = 0 := by
        rw [List.length_eq_zero_iff, List.filter_eq_nil_iff]
        intro a ha; obtain ⟨b, _, rfl⟩ := List.mem_map.mp ha; simp
      rw [hz, Nat.add_zero, ← ih []]
      simp only [numDistinct]
    | cons c t' =>
      rw [numDistinct, ih (c :: t')]
      by_cases hxc : x = c
      · subst hxc; rw [if_pos rfl, length_filter_map_cons, ih t']
      · rw [if_neg hxc, length_filter_map_cons_ne hxc]

end LC.P0115
