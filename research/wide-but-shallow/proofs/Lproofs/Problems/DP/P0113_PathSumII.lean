import Lproofs.Schemes.Fold

/-! @lc 113 | name:Path Sum II | scheme:dp | family:backtracking | complexity:O(n) |
    source:https://leetcode.com/problems/path-sum-ii/

    Collect every root-to-leaf path whose values sum to `target`. CLASSIFICATION: the set of
    root-to-leaf paths is a genuine tree catamorphism `pathLists` (each node prepends its value to
    every path returned by its children — the recursive decomposition the DFS performs); the answer
    filters those paths by sum. We certify the catamorphic structure, not optimality. -/

namespace LC.P0113

abbrev T := Interview.Patterns.Tree ℤ

/-- Every root-to-leaf path (as a value list) — a tree catamorphism. A node prepends `v` to each
    path of its (combined) children; a leaf-node (no child paths) yields the singleton path `[v]`. -/
def pathLists : T → List (List ℤ)
  | .leaf => []
  | .node l v r =>
    let below := pathLists l ++ pathLists r
    (if below = [] then [[]] else below).map (v :: ·)

/-- The accepted answer: the root-to-leaf paths summing to `target`. -/
def sol (t : T) (target : ℤ) : List (List ℤ) := (pathLists t).filter (fun p => decide (p.sum = target))

/-- SCHEME (dp / catamorphism): `pathLists` is a genuine `Tree.fold` — its value at a node depends
    only on the children's results (and `v`), proven by induction (not assumed). -/
theorem cls : (pathLists : T → List (List ℤ)) =
    Interview.Patterns.Tree.fold [] (fun L v R => (if L ++ R = [] then [[]] else L ++ R).map (v :: ·)) := by
  funext t
  induction t with
  | leaf => rfl
  | node l v r ihl ihr => simp [pathLists, Interview.Patterns.Tree.fold, ihl, ihr]

/-- CORRECT: a path is returned iff it is a catamorphic root-to-leaf path that sums to `target`. -/
theorem corr (t : T) (target : ℤ) (p : List ℤ) :
    p ∈ sol t target ↔ p ∈ pathLists t ∧ p.sum = target := by
  simp [sol, List.mem_filter]

end LC.P0113
