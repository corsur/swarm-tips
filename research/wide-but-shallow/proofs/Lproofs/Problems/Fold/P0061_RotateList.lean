import Lproofs.Schemes.Fold

/-! @lc 61 | name:Rotate List | scheme:fold | family:linked-list | complexity:O(n) |
    source:https://leetcode.com/problems/rotate-list/

    Rotate the list right by `k`. The accepted solution makes one streaming pass to count nodes (a
    left fold), then relinks at offset `len - k % len` with a single split. CLASSIFICATION: the
    length pass is a fold; rotation is `drop s ++ take s`. We certify the fold, that the counting
    fold equals the true length, and that rotation preserves the node count (soundness). -/

namespace LC.P0061
open Interview.Patterns

/-- The node-counting pass: one fold incrementing a counter per node. -/
def countLen (xs : List ℤ) : ℕ := xs.foldl (fun n _ => n + 1) 0

/-- Rotate right by `k`, using the computed length to pick the split offset. -/
def rotate (xs : List ℤ) (k : ℕ) : List ℤ :=
  if xs.length = 0 then xs
  else xs.drop (xs.length - k % xs.length) ++ xs.take (xs.length - k % xs.length)

/-- SCHEME (fold): the node-counting pass is a streaming left fold. -/
theorem cls : IsFold (fun xs : List ℤ => xs.foldl (fun n _ => n + 1) 0) :=
  ⟨fun n _ => n + 1, 0, fun _ => rfl⟩

/-- The counting fold is correct: it equals the list length. -/
theorem countLen_eq (xs : List ℤ) : countLen xs = xs.length := by
  have h : ∀ (c : ℕ) (l : List ℤ), l.foldl (fun n _ => n + 1) c = c + l.length := by
    intro c l
    induction l generalizing c with
    | nil => simp
    | cons a t ih => simp only [List.foldl_cons, List.length_cons]; rw [ih]; omega
  show xs.foldl (fun n _ => n + 1) 0 = xs.length
  rw [h 0 xs, Nat.zero_add]

/-- CORRECT (soundness): rotation preserves the number of nodes. -/
theorem corr (xs : List ℤ) (k : ℕ) : (rotate xs k).length = xs.length := by
  unfold rotate
  split
  · rfl
  · rw [List.length_append, List.length_drop, List.length_take]
    omega

end LC.P0061
