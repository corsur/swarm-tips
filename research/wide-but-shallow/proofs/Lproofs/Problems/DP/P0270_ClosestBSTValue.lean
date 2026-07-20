import Lproofs.Schemes.Fold

/-! @lc 270 | name:Closest Binary Search Tree Value | scheme:bisection | family:bst | complexity:O(h) |
    source:https://leetcode.com/problems/closest-binary-search-tree-value/
    (editorial-relabelled dp->bisection 2026-06-22: the canonical accepted solution is the binary
    descent down the BST; file kept under DP/ for git history.) -/

namespace LC.P0270

/-- All node values of the tree — a catamorphism. (The BST traversal visits a subset guided by the
    ordering; the value returned is the global closest, which we characterise here.) -/
def values : Interview.Patterns.Tree ℤ → List ℤ
  | .leaf => []
  | .node l v r => v :: (values l ++ values r)

/-- Editorial answer: the tree value closest to `target`. -/
def sol (t : Interview.Patterns.Tree ℤ) (target : ℤ) : Option ℤ :=
  (values t).argmin (fun v => (v - target).natAbs)

/-- Spec: the returned value is a tree value and is at least as close to `target` as any other. -/
def spec (t : Interview.Patterns.Tree ℤ) (target : ℤ) (r : ℤ) : Prop :=
  r ∈ values t ∧ ∀ v ∈ values t, (r - target).natAbs ≤ (v - target).natAbs

/-- SCHEME (dp / catamorphism): the node values are a fold over the tree. -/
theorem cls : (values = Interview.Patterns.Tree.fold ([] : List ℤ)
    (fun (sl : List ℤ) v sr => v :: (sl ++ sr))) ∧
    ∀ (t : Interview.Patterns.Tree ℤ) (target : ℤ),
      sol t target = (values t).argmin (fun v => (v - target).natAbs) := by
  refine ⟨?_, fun _ _ => rfl⟩
  funext t
  induction t with
  | leaf => rfl
  | node l v r ihl ihr => simp [values, Interview.Patterns.Tree.fold, ihl, ihr]

/-- CORRECT: whenever the search returns a value, it is a tree value minimising the distance. -/
theorem corr (t : Interview.Patterns.Tree ℤ) (target : ℤ) {r : ℤ} (h : sol t target = some r) :
    spec t target r := by
  simp only [sol] at h
  have hr : r ∈ (values t).argmin (fun v => (v - target).natAbs) := h
  exact ⟨List.argmin_mem hr,
    fun v hv => List.le_of_mem_argmin (f := fun v => (v - target).natAbs) hv hr⟩


/-- Official example 1 BST [4,2,5,1,3], values scaled ×10⁶ so the fractional target 3.714286
    stays integral. -/
def exT : Interview.Patterns.Tree ℤ :=
  .node
    (.node (.node .leaf 1000000 .leaf) 2000000 (.node .leaf 3000000 .leaf))
    4000000
    (.node .leaf 5000000 .leaf)

/-- GROUND INSTANCE (official example 1): the closest value to 3.714286 is 4 (scaled). -/
theorem vec : sol exT 3714286 = some 4000000 := by decide

end LC.P0270
