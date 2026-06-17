import Lproofs.Schemes.Fold

/-! @lc 53 | name:Maximum Subarray | scheme:dp | family:dp-linear | complexity:O(n) |
    source:https://leetcode.com/problems/maximum-subarray/

    Kadane's algorithm. The canonical maximum-segment-sum derivation: the running state
    `(best, cur)` carries the maximum nonempty contiguous-segment sum and the maximum nonempty
    suffix sum, extended one element at a time. Correctness is the gold-standard pair —
    ACHIEVABLE (the answer is the sum of a real segment) and OPTIMAL (no segment beats it). -/

namespace LC.P0053
open Interview.Patterns

/-- A nonempty suffix sum of `p`: the sum of `p.drop k` for some `k < p.length`
    (a contiguous block ending at the last element). -/
def IsSuf (p : List ℤ) (v : ℤ) : Prop := ∃ k, k < p.length ∧ (p.drop k).sum = v

/-- A nonempty contiguous-segment sum of `p`: `(p.drop i).take len` with `1 ≤ len` and the
    block fitting inside `p`. -/
def IsSeg (p : List ℤ) (v : ℤ) : Prop :=
  ∃ i len, 1 ≤ len ∧ i + len ≤ p.length ∧ ((p.drop i).take len).sum = v

/-- Kadane invariant for a processed prefix `p`: `c` is the greatest nonempty suffix sum and
    `b` the greatest nonempty segment sum, each both achieved and optimal. -/
structure Valid (p : List ℤ) (b c : ℤ) : Prop where
  cAch : IsSuf p c
  cOpt : ∀ v, IsSuf p v → v ≤ c
  bAch : IsSeg p b
  bOpt : ∀ v, IsSeg p v → v ≤ b

/-- One Kadane step: extend the best suffix, then the best segment. -/
def step (st : ℤ × ℤ) (y : ℤ) : ℤ × ℤ :=
  (max st.1 (max y (st.2 + y)), max y (st.2 + y))

/-- The answer: best segment sum (LeetCode guarantees a nonempty array; `[]` returns `0`). -/
def sol : List ℤ → ℤ
  | [] => 0
  | x :: xs => (xs.foldl step (x, x)).1

/-- A suffix sum is in particular a segment sum (it fits with `i + len = length`). -/
theorem isSuf_isSeg {p : List ℤ} {v : ℤ} (h : IsSuf p v) : IsSeg p v := by
  obtain ⟨k, hk, hs⟩ := h
  refine ⟨k, p.length - k, by omega, by omega, ?_⟩
  have he : (p.drop k).take (p.length - k) = p.drop k := by
    rw [← List.length_drop]; exact List.take_length
  rw [he]; exact hs

/-- Base case: a one-element prefix `[x]` has `[x]` as its only nonempty suffix and segment. -/
theorem valid_single (x : ℤ) : Valid [x] x x := by
  refine ⟨⟨0, by simp, by simp⟩, ?_, ⟨0, 1, le_refl 1, by simp, by simp⟩, ?_⟩
  · rintro v ⟨k, hk, hs⟩
    simp only [List.length_singleton] at hk
    interval_cases k
    simpa using hs.symm.le
  · rintro v ⟨i, len, hlen, hbound, hs⟩
    simp only [List.length_singleton] at hbound
    have hi : i = 0 := by omega
    have hl : len = 1 := by omega
    subst hi; subst hl
    simpa using hs.symm.le

/-- The Kadane step preserves the invariant when one element `y` is appended. -/
theorem valid_step {p : List ℤ} {b c : ℤ} (y : ℤ) (h : Valid p b c) :
    Valid (p ++ [y]) (step (b, c) y).1 (step (b, c) y).2 := by
  set n := p.length with hn
  have hlen : (p ++ [y]).length = n + 1 := by simp [hn]
  have hdrop_last : (p ++ [y]).drop n = [y] := by
    rw [List.drop_append_of_le_length (le_refl _)]; simp
  have hdrop_lt : ∀ k, k ≤ n → (p ++ [y]).drop k = p.drop k ++ [y] := by
    intro k hk; rw [List.drop_append_of_le_length hk]
  have hsum_lt : ∀ k, k ≤ n → ((p ++ [y]).drop k).sum = (p.drop k).sum + y := by
    intro k hk; rw [hdrop_lt k hk]; simp
  set c' := max y (c + y) with hc'
  have hc'_ach : IsSuf (p ++ [y]) c' := by
    rcases max_choice y (c + y) with hcase | hcase
    · refine ⟨n, by omega, ?_⟩
      rw [hdrop_last]; simp [hc', hcase]
    · obtain ⟨k, hk, hs⟩ := h.cAch
      refine ⟨k, by omega, ?_⟩
      rw [hsum_lt k (by omega), hs, hc', hcase]
  have hc'_opt : ∀ v, IsSuf (p ++ [y]) v → v ≤ c' := by
    rintro v ⟨k, hk, hs⟩
    rw [hlen] at hk
    rcases Nat.lt_or_ge k n with hkn | hkn
    · rw [hsum_lt k (by omega)] at hs
      have hb := h.cOpt _ ⟨k, hkn, rfl⟩
      rw [← hs]; exact le_trans (by linarith) (le_max_right _ _)
    · have hkn' : k = n := by omega
      subst hkn'; rw [hdrop_last] at hs; simp at hs
      rw [← hs]; exact le_max_left _ _
  have hseg_embed : ∀ v, IsSeg p v → IsSeg (p ++ [y]) v := by
    rintro v ⟨i, len, hlen', hb, hs⟩
    refine ⟨i, len, hlen', by simp only [hlen]; omega, ?_⟩
    rw [List.drop_append_of_le_length (by omega), List.take_append_of_le_length (by
      rw [List.length_drop]; omega)]
    exact hs
  refine ⟨hc'_ach, hc'_opt, ?_, ?_⟩
  · rcases max_choice b c' with hcase | hcase
    · rw [step]; simp only [← hc', hcase]; exact hseg_embed _ h.bAch
    · rw [step]; simp only [← hc', hcase]; exact isSuf_isSeg hc'_ach
  · rintro v ⟨i, len, hlen', hb, hs⟩
    rw [hlen] at hb
    rw [step]; simp only
    rcases Nat.lt_or_ge (i + len) (n + 1) with hcase | hcase
    · have hin : IsSeg p v := by
        refine ⟨i, len, hlen', by omega, ?_⟩
        rw [List.drop_append_of_le_length (by omega),
          List.take_append_of_le_length (by rw [List.length_drop]; omega)] at hs
        exact hs
      exact le_trans (h.bOpt _ hin) (le_max_left _ _)
    · have hsuf : IsSuf (p ++ [y]) v := by
        refine ⟨i, by omega, ?_⟩
        have htk : ((p ++ [y]).drop i).take len = (p ++ [y]).drop i :=
          List.take_of_length_le (by rw [List.length_drop, hlen]; omega)
        rw [htk] at hs; exact hs
      exact le_trans (hc'_opt _ hsuf) (le_max_right _ _)

/-- Folding the Kadane step over the tail preserves the invariant for the whole prefix. -/
theorem valid_fold (x : ℤ) (xs : List ℤ) :
    Valid (x :: xs) (xs.foldl step (x, x)).1 (xs.foldl step (x, x)).2 := by
  suffices h : ∀ ys p b c, Valid p b c →
      Valid (p ++ ys) (ys.foldl step (b, c)).1 (ys.foldl step (b, c)).2 by
    have hx := h xs [x] x x (valid_single x)
    simpa using hx
  intro ys
  induction ys with
  | nil => intro p b c hv; simpa using hv
  | cons y ys ih =>
    intro p b c hv
    have hstep := valid_step y hv
    have hrec := ih (p ++ [y]) (step (b, c) y).1 (step (b, c) y).2 hstep
    simpa [List.foldl_cons, List.append_assoc] using hrec

/-- SCHEME (dp): the answer is a single streaming fold (the Kadane recurrence). -/
theorem cls : IsFold (fun a : List ℤ =>
    a.foldl (fun st y => (max st.1 (max y (st.2 + y)), max y (st.2 + y))) ((0 : ℤ), (0 : ℤ))) :=
  ⟨_, _, fun _ => rfl⟩

/-- CORRECT: for a nonempty array, `sol` is both the sum of a real nonempty contiguous segment
    (ACHIEVABLE) and an upper bound on every such sum (OPTIMAL) — i.e. the maximum subarray sum. -/
theorem corr (x : ℤ) (xs : List ℤ) :
    IsSeg (x :: xs) (sol (x :: xs)) ∧ ∀ v, IsSeg (x :: xs) v → v ≤ sol (x :: xs) := by
  have hv := valid_fold x xs
  exact ⟨hv.bAch, fun v hvseg => hv.bOpt v hvseg⟩

end LC.P0053
