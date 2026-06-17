import Lproofs.Schemes.Fold

/-! @lc 101 | name:Symmetric Tree | scheme:dp | family:tree-aggregate | complexity:O(n) |
    source:https://leetcode.com/problems/symmetric-tree/

    A tree is symmetric iff it equals its own mirror. CLASSIFICATION: the mirror is a genuine tree
    catamorphism (`Tree.fold` with `(ml, v, mr) ↦ node mr v ml` — swap the folded children), proven by
    structural induction (`cls`). NON-VACUITY: we prove mirroring is an involution — `mirror (mirror t)
    = t` (`corr`) — so the catamorphism does real structural work; the symmetry check is `t = mirror t`.
    -/

namespace LC.P0101

abbrev T := Interview.Patterns.Tree ℤ

/-- Mirror a tree: recursively swap left and right children. -/
def mirror : T → T
  | .leaf => .leaf
  | .node l v r => .node (mirror r) v (mirror l)

/-- SCHEME (dp / catamorphism): the mirror is a genuine `Tree.fold`, proven by induction. -/
theorem cls : (mirror : T → T) = Interview.Patterns.Tree.fold .leaf (fun ml v mr => .node mr v ml) := by
  funext t
  induction t with
  | leaf => rfl
  | node l v r ihl ihr => simp [mirror, Interview.Patterns.Tree.fold, ihl, ihr]

/-- NON-VACUITY (involution): mirroring twice is the identity — genuine structural recursion. -/
theorem corr : ∀ t : T, mirror (mirror t) = t
  | .leaf => rfl
  | .node l v r => by rw [mirror, mirror, corr l, corr r]

end LC.P0101
