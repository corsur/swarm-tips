import Lproofs.Schemes.Fold

/-! @lc 349 | name:Intersection of Two Arrays | scheme:fold | family:hashing | complexity:O(n+m) |
    source:https://leetcode.com/problems/intersection-of-two-arrays/ -/

namespace LC.P0349
open Interview.Patterns

/-- The membership set of `b`, built by a streaming seen-set fold (`= b.toFinset`). -/
def seen (b : List ℕ) : Finset ℕ := b.foldl (fun s x => insert x s) (∅ : Finset ℕ)

/-- Editorial hash-set solution: build `b`'s membership set with a streaming fold, then keep the
    distinct values of `a` that occur in it. The fold-built `seen b` is the solution's structure. -/
def sol (a b : List ℕ) : List ℕ := a.dedup.filter (fun x => decide (x ∈ seen b))

/-- Spec: each reported value occurs in both arrays. -/
def spec (a b : List ℕ) (y : ℕ) : Prop := y ∈ a ∧ y ∈ b

/-- SCHEME (fold): the membership set `sol` reads is the streaming seen-set fold. -/
theorem cls : IsFold (fun xs : List ℕ => xs.foldl (fun s x => insert x s) (∅ : Finset ℕ)) :=
  fold_seenSet

/-- CORRECT: every reported value genuinely occurs in both arrays. -/
theorem corr (a b : List ℕ) {y : ℕ} (h : y ∈ sol a b) : spec a b y := by
  simp only [sol, seen, seenSet_eq_toFinset, List.mem_filter, List.mem_dedup,
    List.mem_toFinset, decide_eq_true_eq] at h
  exact h

end LC.P0349
