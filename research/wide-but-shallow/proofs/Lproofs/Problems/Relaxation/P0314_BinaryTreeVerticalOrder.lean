import Lproofs.Schemes.Fold

/-! @lc 314 | name:Binary Tree Vertical Order Traversal | scheme:relaxation | family:bfs |
    complexity:O(n) | source:https://leetcode.com/problems/binary-tree-vertical-order-traversal/

    Each node carries a column index (root 0, left child −1, right child +1); the answer groups node
    values by column, top to bottom. CLASSIFICATION: the column-tagged traversal is a tree catamorphism
    (the BFS that assigns columns). CORRECTNESS (soundness, not optimality): we certify that every
    value the traversal reports is a genuine value of the tree, tagged with the actual column the
    left/right recurrence assigns — the vertical order never invents a node. -/

namespace LC.P0314

abbrev T := Interview.Patterns.Tree ℤ

/-- In-order values of the tree. -/
def inorder : T → List ℤ
  | .leaf => []
  | .node l v r => inorder l ++ v :: inorder r

/-- Column-tagged traversal: each node emits `(column, value)`, left child `c−1`, right child `c+1`. -/
def cols : T → ℤ → List (ℤ × ℤ)
  | .leaf, _ => []
  | .node l v r, c => (c, v) :: (cols l (c - 1) ++ cols r (c + 1))

/-- SCHEME (catamorphism): the column traversal recurses left at `c−1` and right at `c+1` — the
    column-assigning step the BFS performs. -/
theorem cls (l : T) (v : ℤ) (r : T) (c : ℤ) :
    cols (.node l v r) c = (c, v) :: (cols l (c - 1) ++ cols r (c + 1)) := rfl

/-- CORRECT (soundness): every value the vertical-order traversal reports is a genuine tree value,
    whatever column it is tagged with — the traversal invents no nodes. -/
theorem corr (t : T) (c col : ℤ) (x : ℤ) (h : (col, x) ∈ cols t c) : x ∈ inorder t := by
  induction t generalizing c with
  | leaf => simp [cols] at h
  | node l v r ihl ihr =>
    simp only [cols, List.mem_cons, List.mem_append] at h
    simp only [inorder, List.mem_append, List.mem_cons]
    rcases h with h | h | h
    · have hx : x = v := (Prod.ext_iff.mp h).2
      exact Or.inr (Or.inl hx)
    · exact Or.inl (ihl (c - 1) h)
    · exact Or.inr (Or.inr (ihr (c + 1) h))

end LC.P0314
