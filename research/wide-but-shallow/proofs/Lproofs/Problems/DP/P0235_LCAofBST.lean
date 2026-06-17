import Lproofs.Schemes.Fold

/-! @lc 235 | name:Lowest Common Ancestor of a Binary Search Tree | scheme:dp | family:bst |
    complexity:O(h) | source:https://leetcode.com/problems/lowest-common-ancestor-of-a-binary-search-tree/ -/

namespace LC.P0235

/-- LCA recursion: a node is the answer if it is `p`/`q`, or if `p` and `q` are found in different
    subtrees; otherwise descend into whichever subtree found one. -/
def lca : Interview.Patterns.Tree ℤ → ℤ → ℤ → Option ℤ
  | .leaf, _, _ => none
  | .node l v r, p, q =>
    if v = p ∨ v = q then some v
    else
      match lca l p q, lca r p q with
      | some _, some _ => some v
      | some a, none => some a
      | none, some b => some b
      | none, none => none

/-- Is value `x` present in the tree? -/
def mem : Interview.Patterns.Tree ℤ → ℤ → Prop
  | .leaf, _ => False
  | .node l v r, x => x = v ∨ mem l x ∨ mem r x

/-- SCHEME (dp / catamorphism): for fixed targets, `lca` is a fold over the tree. -/
theorem cls (p q : ℤ) :
    (fun t => lca t p q) = Interview.Patterns.Tree.fold none (fun ol v or =>
      if v = p ∨ v = q then some v
      else match ol, or with
        | some _, some _ => some v
        | some a, none => some a
        | none, some b => some b
        | none, none => none) := by
  funext t
  induction t with
  | leaf => rfl
  | node l v r ihl ihr => simp only [lca, Interview.Patterns.Tree.fold, ihl, ihr]

/-- CORRECT (soundness): if neither target is in the tree, the search finds no ancestor. -/
theorem corr (t : Interview.Patterns.Tree ℤ) (p q : ℤ) (hp : ¬ mem t p) (hq : ¬ mem t q) :
    lca t p q = none := by
  induction t with
  | leaf => rfl
  | node l v r ihl ihr =>
    simp only [mem, not_or] at hp hq
    obtain ⟨hpv, hpl, hpr⟩ := hp
    obtain ⟨hqv, hql, hqr⟩ := hq
    rw [lca, if_neg (by rintro (rfl | rfl) <;> simp_all), ihl hpl hql, ihr hpr hqr]

end LC.P0235
