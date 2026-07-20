import Lproofs.Schemes.Fold

/-! @lc 101 | name:Symmetric Tree | scheme:dp | family:tree-aggregate | complexity:O(n) |
    source:https://leetcode.com/problems/symmetric-tree/

    A tree is symmetric iff it equals its own sol. CLASSIFICATION: the sol is a genuine tree
    catamorphism (`Tree.fold` with `(ml, v, mr) ↦ node mr v ml` — swap the folded children), proven by
    structural induction (`cls`). NON-VACUITY: we prove mirroring is an involution — `sol (sol t)
    = t` (`corr`) — so the catamorphism does real structural work; the symmetry check is `t = sol t`.
    -/

namespace LC.P0101

abbrev T := Interview.Patterns.Tree ℤ

/-- Mirror a tree: recursively swap left and right children. -/
def sol : T → T
  | .leaf => .leaf
  | .node l v r => .node (sol r) v (sol l)

/-- SCHEME (dp / catamorphism): the sol is a genuine `Tree.fold`, proven by induction. -/
theorem cls : (sol : T → T) = Interview.Patterns.Tree.fold .leaf (fun ml v mr => .node mr v ml) := by
  funext t
  induction t with
  | leaf => rfl
  | node l v r ihl ihr => simp [sol, Interview.Patterns.Tree.fold, ihl, ihr]

/-- NON-VACUITY (involution): mirroring twice is the identity — genuine structural recursion. -/
theorem corr : ∀ t : T, sol (sol t) = t
  | .leaf => rfl
  | .node l v r => by rw [sol, sol, corr l, corr r]


/-- Official example 1: [1,2,2,3,4,4,3] — a symmetric tree. -/
def exT : T :=
  .node (.node (.node .leaf 3 .leaf) 2 (.node .leaf 4 .leaf)) 1
    (.node (.node .leaf 4 .leaf) 2 (.node .leaf 3 .leaf))

/-- Official example 2: [1,2,2,null,3,null,3] — not symmetric. -/
def exT2 : T :=
  .node (.node .leaf 2 (.node .leaf 3 .leaf)) 1 (.node .leaf 2 (.node .leaf 3 .leaf))

/-- GROUND INSTANCE: the symmetric example is its own mirror; the asymmetric one mirrors to its
    reflection (3-subtrees flip to the left), witnessing the swap really happens. -/
theorem vec : sol exT = exT ∧
    sol exT2 = .node (.node (.node .leaf 3 .leaf) 2 .leaf) 1
      (.node (.node .leaf 3 .leaf) 2 .leaf) := ⟨rfl, rfl⟩


/-- In-order values (left–root–right). -/
def inorder : T → List ℤ
  | .leaf => []
  | .node l v r => inorder l ++ v :: inorder r

/-- EFFECT (upgrades the involution): mirroring reverses the in-order reading — the precise
    left-right swap the symmetric-tree check relies on. -/
theorem mirror_inorder (t : T) : inorder (sol t) = (inorder t).reverse := by
  induction t with
  | leaf => rfl
  | node l v r ihl ihr =>
    simp only [sol, inorder, List.reverse_append, List.reverse_cons, ihl, ihr]
    simp [List.append_assoc]

end LC.P0101
