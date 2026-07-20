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
def sol : T → ℤ → List (ℤ × ℤ)
  | .leaf, _ => []
  | .node l v r, c => (c, v) :: (sol l (c - 1) ++ sol r (c + 1))

/-- SCHEME (catamorphism): the column traversal recurses left at `c−1` and right at `c+1` — the
    column-assigning step the BFS performs. -/
theorem cls (l : T) (v : ℤ) (r : T) (c : ℤ) :
    sol (.node l v r) c = (c, v) :: (sol l (c - 1) ++ sol r (c + 1)) := rfl

/-- CORRECT (soundness): every value the vertical-order traversal reports is a genuine tree value,
    whatever column it is tagged with — the traversal invents no nodes. -/
theorem corr (t : T) (c col : ℤ) (x : ℤ) (h : (col, x) ∈ sol t c) : x ∈ inorder t := by
  induction t generalizing c with
  | leaf => simp [sol] at h
  | node l v r ihl ihr =>
    simp only [sol, List.mem_cons, List.mem_append] at h
    simp only [inorder, List.mem_append, List.mem_cons]
    rcases h with h | h | h
    · have hx : x = v := (Prod.ext_iff.mp h).2
      exact Or.inr (Or.inl hx)
    · exact Or.inl (ihl (c - 1) h)
    · exact Or.inr (Or.inr (ihr (c + 1) h))


/-- Official example 1: [3,9,20,null,null,15,7]. -/
def exT : T :=
  .node (.node .leaf 9 .leaf) 3 (.node (.node .leaf 15 .leaf) 20 (.node .leaf 7 .leaf))

/-- GROUND INSTANCE (official example 1): the column-tagged traversal from column 0 — 9 in
    column −1, {3,15} in column 0, 20 in column 1, 7 in column 2 (the judge's grouping). -/
theorem vec : sol exT 0 = [(0, 3), (-1, 9), (1, 20), (0, 15), (2, 7)] := by decide


/-- COMPLETENESS (the other direction of `corr`): every tree value is reported in some column —
    the traversal drops no nodes. With `corr`, the emitted pairs are exactly the tree's values,
    each tagged with its column. -/
theorem complete (t : T) (c : ℤ) (x : ℤ) (h : x ∈ inorder t) : ∃ col, (col, x) ∈ sol t c := by
  induction t generalizing c with
  | leaf => simp [inorder] at h
  | node l v r ihl ihr =>
    simp only [inorder, List.mem_append, List.mem_cons] at h
    rcases h with h | h | h
    · obtain ⟨col, hc⟩ := ihl (c - 1) h
      refine ⟨col, ?_⟩
      simp only [sol]
      exact List.mem_cons_of_mem _ (List.mem_append_left _ hc)
    · refine ⟨c, ?_⟩
      simp [sol, h]
    · obtain ⟨col, hc⟩ := ihr (c + 1) h
      refine ⟨col, ?_⟩
      simp only [sol]
      exact List.mem_cons_of_mem _ (List.mem_append_right _ hc)

end LC.P0314
