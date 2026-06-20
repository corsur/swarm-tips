import Lproofs.Schemes.Fold

/-! @lc 992 | name:Subarrays with K Different Integers | scheme:fold | family:sliding-window |
    complexity:O(n) | source:https://leetcode.com/problems/subarrays-with-k-different-integers/

    The accepted O(n) solution computes the answer as `atMost(K) − atMost(K−1)`, where `atMost(k)` counts
    subarrays with at most `k` distinct values (each `atMost` is one sliding-window pass). CLASSIFICATION:
    `atMost` is a tally — a streaming fold over the subarrays. CORRECTNESS: we certify the counting
    identity the whole approach rests on — `atMost(k) = atMost(k−1) + exactly(k)` over the concrete
    subarrays — so the answer `atMost(K) − atMost(K−1)` is exactly the number of subarrays with K
    distinct values. -/

namespace LC.P0992
open Interview.Patterns

/-- All nonempty contiguous subarrays of `xs`. -/
def subs (xs : List ℤ) : List (List ℤ) :=
  (List.range xs.length).flatMap fun i =>
    (List.range (xs.length - i)).map fun l => (xs.drop i).take (l + 1)

/-- Distinct-value count of a subarray. -/
def distinct (s : List ℤ) : ℕ := s.dedup.length

/-- Subarrays with at most `k` distinct values. -/
def atMost (xs : List ℤ) (k : ℕ) : ℕ := (subs xs).countP fun s => decide (distinct s ≤ k)

/-- Subarrays with exactly `k` distinct values. -/
def exactly (xs : List ℤ) (k : ℕ) : ℕ := (subs xs).countP fun s => decide (distinct s = k)

/-- SCHEME (fold): `atMost` is a tally — a streaming right fold over the subarrays. -/
theorem cls (xs : List ℤ) (k : ℕ) :
    IsRightFold (fun L : List (List ℤ) => L.countP fun s => decide (distinct s ≤ k)) := by
  refine ⟨fun s n => if decide (distinct s ≤ k) then n + 1 else n, 0, fun L => ?_⟩
  induction L with
  | nil => rfl
  | cons s t ih => simp only [List.countP_cons, List.foldr_cons, ih]; split <;> omega

/-- The partition identity over any list: `#{≤k} = #{≤k-1} + #{=k}` when `k ≥ 1`. -/
theorem countP_le_split {α : Type*} (f : α → ℕ) (k : ℕ) (hk : 1 ≤ k) (l : List α) :
    l.countP (fun a => decide (f a ≤ k)) =
      l.countP (fun a => decide (f a ≤ k - 1)) + l.countP (fun a => decide (f a = k)) := by
  induction l with
  | nil => simp
  | cons a t ih =>
    simp only [List.countP_cons, ih, decide_eq_true_eq]
    split_ifs <;> omega

/-- CORRECT: the answer `atMost(k) − atMost(k−1)` is exactly the count of subarrays with `k` distinct
    values — the counting identity `atMost(k) = atMost(k−1) + exactly(k)` the algorithm relies on. -/
theorem corr (xs : List ℤ) (k : ℕ) (hk : 1 ≤ k) :
    atMost xs k = atMost xs (k - 1) + exactly xs k :=
  countP_le_split distinct k hk (subs xs)

end LC.P0992
