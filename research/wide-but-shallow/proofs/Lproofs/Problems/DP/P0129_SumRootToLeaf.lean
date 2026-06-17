import Lproofs.Schemes.Fold

/-! @lc 129 | name:Sum Root to Leaf Numbers | scheme:dp | family:tree-aggregate | complexity:O(n) |
    source:https://leetcode.com/problems/sum-root-to-leaf-numbers/

    Each root-to-leaf path spells a number (digits concatenated); sum them. CLASSIFICATION: the
    path-numbers are a tree catamorphism with an inherited accumulator `cur` — each node folds its
    digit into the running prefix (`cur ↦ cur*10 + v`) and defers to its children; a leaf-node emits
    the completed number. The answer sums them. We certify the catamorphic recursion, not optimality. -/

namespace LC.P0129

abbrev T := Interview.Patterns.Tree ℤ

/-- Path-numbers below a node, given the prefix `cur` accumulated from the root — the inherited-
    accumulator tree catamorphism the DFS performs. -/
def pathNums : T → ℤ → List ℤ
  | .leaf, _ => []
  | .node l v r, cur =>
    let cur' := cur * 10 + v
    let below := pathNums l cur' ++ pathNums r cur'
    if below = [] then [cur'] else below

/-- The accepted answer: the sum of all root-to-leaf numbers. -/
def sol (t : T) : ℤ := (pathNums t 0).sum

/-- SCHEME (dp / catamorphism): a node folds its digit into the prefix and defers to its children. -/
theorem cls (l : T) (v : ℤ) (r : T) (cur : ℤ) :
    pathNums (.node l v r) cur =
      (let cur' := cur * 10 + v
       let below := pathNums l cur' ++ pathNums r cur'
       if below = [] then [cur'] else below) :=
  rfl

/-- The answer reads off the catamorphic path-number list. -/
theorem corr (t : T) : sol t = (pathNums t 0).sum := rfl

end LC.P0129
