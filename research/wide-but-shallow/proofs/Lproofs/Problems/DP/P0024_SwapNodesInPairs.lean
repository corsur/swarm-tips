import Lproofs.Schemes.Fold

/-! @lc 24 | name:Swap Nodes in Pairs | scheme:dp | family:recursive-decomposition | complexity:O(n) |
    source:https://leetcode.com/problems/swap-nodes-in-pairs/

    Swap every adjacent pair of nodes. CLASSIFICATION: a recursive decomposition — swap the first two
    nodes, then recurse on the rest (`cls`). NON-VACUITY: we prove the swap conserves length — the
    output has exactly as many nodes as the input (`corr`) — so the decomposition rearranges genuine
    nodes, not a re-encoding. We certify the decomposition + length conservation. -/

namespace LC.P0024

/-- Swap adjacent pairs of nodes. -/
def swap : List ℤ → List ℤ
  | a :: b :: rest => b :: a :: swap rest
  | l => l

/-- SCHEME (dp / recursive decomposition): swap the leading pair, recurse on the remainder. -/
theorem cls (a b : ℤ) (rest : List ℤ) : swap (a :: b :: rest) = b :: a :: swap rest := rfl

/-- NON-VACUITY (conservation): swapping pairs preserves the number of nodes. -/
theorem corr : ∀ l : List ℤ, (swap l).length = l.length
  | [] => rfl
  | [_] => rfl
  | a :: b :: rest => by rw [cls, List.length_cons, List.length_cons, corr rest]; simp

end LC.P0024
