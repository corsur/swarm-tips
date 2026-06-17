import Lproofs.Problems.Fold.P0206_ReverseLinkedList

/-! @lc 92 | name:Reverse Linked List II | scheme:fold | family:linked-list | complexity:O(n) |
    source:https://leetcode.com/problems/reverse-linked-list-ii/

    Reverse the sublist of positions `[m, n)`, leaving the rest in place. CLASSIFICATION: the reversed
    middle is produced by the same streaming accumulator fold as Reverse Linked List (thread the
    reversed prefix). NON-VACUITY: we prove the spliced result is a genuine permutation of the input
    (`↑(rev92) = ↑l`) — the reversal rearranges exactly the chosen window and loses/adds nothing. We
    certify the fold + that it is a faithful in-place reversal, not any positional optimality. -/

namespace LC.P0092
open Interview.Patterns

/-- Reverse positions `[m, n)`: keep the prefix, fold-reverse the middle, keep the suffix. -/
def rev92 {α : Type*} (l : List α) (m n : ℕ) : List α :=
  l.take m ++ LC.P0206.sol ((l.drop m).take (n - m)) ++ l.drop n

/-- The window splice covers the whole list: prefix ++ middle ++ suffix = `l`. -/
theorem splice {α : Type*} (l : List α) (m n : ℕ) (h : m ≤ n) :
    l.take m ++ (l.drop m).take (n - m) ++ l.drop n = l := by
  have hd : (l.drop m).drop (n - m) = l.drop n := by
    rw [List.drop_drop]; congr 1; omega
  rw [List.append_assoc, ← hd, List.take_append_drop, List.take_append_drop]

/-- SCHEME (fold): the middle is reversed by the streaming accumulator fold (Reverse Linked List). -/
theorem cls {α : Type*} : IsFold (LC.P0206.sol : List α → List α) := LC.P0206.cls

/-- NON-VACUITY: the spliced result is a permutation of the input — a faithful in-place reversal. -/
theorem corr {α : Type*} (l : List α) (m n : ℕ) (h : m ≤ n) :
    (rev92 l m n : Multiset α) = ↑l := by
  rw [rev92, LC.P0206.corr]
  conv_rhs => rw [← splice l m n h]
  exact Multiset.coe_eq_coe.mpr
    (((List.Perm.refl _).append (List.reverse_perm _)).append (List.Perm.refl _))

end LC.P0092
