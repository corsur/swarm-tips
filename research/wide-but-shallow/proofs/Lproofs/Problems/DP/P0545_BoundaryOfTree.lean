import Lproofs.Schemes.Fold

/-! @lc 545 | name:Boundary of Binary Tree | scheme:dp | family:tree-traversal | complexity:O(n) |
    source:https://leetcode.com/problems/boundary-of-binary-tree/

    The boundary is the root, then the left boundary (top-down internal nodes), then the leaves
    (left-to-right), then the right boundary (bottom-up internal nodes). CLASSIFICATION: each piece is a
    structural recursion over the tree (a catamorphism). CORRECTNESS: we build the actual boundary and
    prove it is SOUND — every value it reports is a genuine value of the tree (`∈ inorder`), so the
    boundary never invents a node. -/

namespace LC.P0545
open Interview.Patterns

abbrev T := Interview.Patterns.Tree ℤ

/-- In-order traversal (the tree's values). -/
def inorder : T → List ℤ
  | .leaf => []
  | .node l v r => inorder l ++ v :: inorder r

/-- Leaf values, left to right. -/
def leaves : T → List ℤ
  | .leaf => []
  | .node l v r => match l, r with
    | .leaf, .leaf => [v]
    | _, _ => leaves l ++ leaves r

/-- Left boundary: internal nodes down the left spine (go left if it exists, else right). -/
def leftBd : T → List ℤ
  | .leaf => []
  | .node l v r => match l, r with
    | .leaf, .leaf => []
    | .leaf, _ => v :: leftBd r
    | _, _ => v :: leftBd l

/-- Right boundary (top-down; reversed in `boundary`): internal nodes down the right spine. -/
def rightBd : T → List ℤ
  | .leaf => []
  | .node l v r => match l, r with
    | .leaf, .leaf => []
    | _, .leaf => v :: rightBd l
    | _, _ => v :: rightBd r

/-- The boundary: root, left boundary, leaves, right boundary (bottom-up). -/
def boundary : T → List ℤ
  | .leaf => []
  | .node l v r => v :: (leftBd l ++ leaves (.node l v r) ++ (rightBd r).reverse)

/-- SCHEME (dp / catamorphism): `inorder` is a tree catamorphism, proven by induction. -/
theorem cls : (inorder : T → List ℤ) = Tree.fold [] (fun l v r => l ++ v :: r) := by
  funext t
  induction t with
  | leaf => rfl
  | node l v r ihl ihr => simp [inorder, Tree.fold, ihl, ihr]

theorem leaves_sub : ∀ (t : T) (x : ℤ), x ∈ leaves t → x ∈ inorder t := by
  intro t
  induction t with
  | leaf => simp [leaves]
  | node l v r ihl ihr =>
    intro x hx
    simp only [inorder, List.mem_append, List.mem_cons]
    unfold leaves at hx
    split at hx
    · simp only [List.mem_singleton] at hx; exact Or.inr (Or.inl hx)
    · rw [List.mem_append] at hx
      rcases hx with h | h
      · exact Or.inl (ihl x h)
      · exact Or.inr (Or.inr (ihr x h))

theorem leftBd_sub : ∀ (t : T) (x : ℤ), x ∈ leftBd t → x ∈ inorder t := by
  intro t
  induction t with
  | leaf => simp [leftBd]
  | node l v r ihl ihr =>
    intro x hx
    simp only [inorder, List.mem_append, List.mem_cons]
    unfold leftBd at hx
    split at hx
    · simp at hx
    · rw [List.mem_cons] at hx
      rcases hx with rfl | h
      · exact Or.inr (Or.inl rfl)
      · exact Or.inr (Or.inr (ihr x h))
    · rw [List.mem_cons] at hx
      rcases hx with rfl | h
      · exact Or.inr (Or.inl rfl)
      · exact Or.inl (ihl x h)

theorem rightBd_sub : ∀ (t : T) (x : ℤ), x ∈ rightBd t → x ∈ inorder t := by
  intro t
  induction t with
  | leaf => simp [rightBd]
  | node l v r ihl ihr =>
    intro x hx
    simp only [inorder, List.mem_append, List.mem_cons]
    unfold rightBd at hx
    split at hx
    · simp at hx
    · rw [List.mem_cons] at hx
      rcases hx with rfl | h
      · exact Or.inr (Or.inl rfl)
      · exact Or.inl (ihl x h)
    · rw [List.mem_cons] at hx
      rcases hx with rfl | h
      · exact Or.inr (Or.inl rfl)
      · exact Or.inr (Or.inr (ihr x h))

/-- CORRECT (soundness): every value reported on the boundary is a genuine value of the tree. -/
theorem corr : ∀ (t : T) (x : ℤ), x ∈ boundary t → x ∈ inorder t := by
  intro t
  cases t with
  | leaf => simp [boundary]
  | node l v r =>
    intro x hx
    simp only [boundary, List.mem_cons, List.mem_append, List.mem_reverse] at hx
    simp only [inorder, List.mem_append, List.mem_cons]
    rcases hx with rfl | hrest
    · exact Or.inr (Or.inl rfl)
    rcases hrest with hlr | hright
    · rcases hlr with hl | hlv
      · exact Or.inl (leftBd_sub l x hl)
      · have := leaves_sub (.node l v r) x hlv; simpa [inorder] using this
    · exact Or.inr (Or.inr (rightBd_sub r x hright))

end LC.P0545
