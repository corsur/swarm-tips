import Lproofs.Schemes.Fold
import Mathlib.Data.List.Nodup

/-! @lc 105 | name:Construct Binary Tree from Preorder and Inorder Traversal | scheme:dp |
    family:tree-construct | complexity:O(n²) |
    source:https://leetcode.com/problems/construct-binary-tree-from-preorder-and-inorder-traversal/

    Rebuild a binary tree (distinct values) from its preorder and inorder traversals: the head of
    preorder is the root; its position in inorder splits inorder into the left/right subtrees (and
    pins their sizes), so preorder splits the same way and the halves recurse — a divide-and-conquer
    recursive decomposition (`cls`). Correctness (`corr`): the construction inverts the traversals,
    `build (pre t) (inord t) = t` for every tree with distinct values, so it is lossless. -/

namespace LC.P0105

abbrev T := Interview.Patterns.Tree ℤ

/-- Preorder: root, then left, then right. -/
def pre : T → List ℤ
  | .leaf => []
  | .node l v r => v :: (pre l ++ pre r)

/-- Inorder: left, then root, then right. -/
def inord : T → List ℤ
  | .leaf => []
  | .node l v r => inord l ++ v :: inord r

/-- Node count. -/
def sz : T → ℕ
  | .leaf => 0
  | .node l _ r => sz l + sz r + 1

theorem pre_len : ∀ t, (pre t).length = sz t
  | .leaf => rfl
  | .node l _ r => by
    have := pre_len l; have := pre_len r
    simp only [pre, sz, List.length_cons, List.length_append]; omega

theorem inord_len : ∀ t, (inord t).length = sz t
  | .leaf => rfl
  | .node l _ r => by
    have := inord_len l; have := inord_len r
    simp only [inord, sz, List.length_append, List.length_cons]; omega

/-- Build a binary tree from preorder and inorder (fuel bounds the recursion). -/
def build : ℕ → List ℤ → List ℤ → T
  | _, [], _ => .leaf
  | fuel + 1, v :: pre', inord =>
    let k := inord.idxOf v
    .node (build fuel (pre'.take k) (inord.take k)) v
          (build fuel (pre'.drop k) ((inord.drop k).tail))
  | _, _, _ => .leaf

/-- SCHEME (dp / divide-and-conquer): the construction is a recursive decomposition — root from the
    head of preorder, its inorder position splitting both traversals into the recursing halves. -/
theorem cls (fuel : ℕ) (v : ℤ) (pre' inord : List ℤ) :
    build (fuel + 1) (v :: pre') inord =
      .node (build fuel (pre'.take (inord.idxOf v)) (inord.take (inord.idxOf v))) v
            (build fuel (pre'.drop (inord.idxOf v)) ((inord.drop (inord.idxOf v)).tail)) :=
  rfl

/-- The root sits at index `sz l` of the inorder (left subtree comes first, distinct values). -/
theorem idxOf_root (l : T) (v : ℤ) (r : T) (h : (inord (.node l v r)).Nodup) :
    (inord (.node l v r)).idxOf v = sz l := by
  have hsplit := List.nodup_append.mp (by simpa only [inord] using h)
  have hvl : v ∉ inord l := fun hc =>
    hsplit.2.2 v hc v List.mem_cons_self rfl
  simp only [inord]
  rw [List.idxOf_append, if_neg hvl, List.idxOf_cons_self, inord_len]
  omega

/-- CORRECT (round-trip): with enough fuel and distinct values, `build` inverts the traversals. -/
theorem build_round : ∀ (t : T) (fuel : ℕ), sz t ≤ fuel → (inord t).Nodup →
    build fuel (pre t) (inord t) = t := by
  intro t
  induction t with
  | leaf => intro fuel _ _; simp only [pre, build]
  | node l v r ihl ihr =>
    intro fuel hfuel hnd
    obtain ⟨f, rfl⟩ : ∃ f, fuel = f + 1 := ⟨fuel - 1, by simp only [sz] at hfuel; omega⟩
    simp only [sz] at hfuel
    have hk : (inord (.node l v r)).idxOf v = sz l := idxOf_root l v r hnd
    have hpre : pre (.node l v r) = v :: (pre l ++ pre r) := rfl
    have hsplit := List.nodup_append.mp (by simpa only [inord] using hnd)
    have hndl : (inord l).Nodup := hsplit.1
    have hndr : (inord r).Nodup := (List.nodup_cons.mp hsplit.2.1).2
    have ht1 : (pre l ++ pre r).take (sz l) = pre l := by rw [← pre_len l]; exact List.take_left
    have hd1 : (pre l ++ pre r).drop (sz l) = pre r := by rw [← pre_len l]; exact List.drop_left
    have ht2 : (inord (.node l v r)).take (sz l) = inord l := by
      simp only [inord]; rw [← inord_len l]; exact List.take_left
    have hd2 : ((inord (.node l v r)).drop (sz l)).tail = inord r := by
      simp only [inord]; rw [← inord_len l, List.drop_left]; simp
    rw [hpre, cls, hk, ht1, hd1, ht2, hd2]
    rw [Interview.Patterns.Tree.node.injEq]
    exact ⟨ihl f (by omega) hndl, rfl, ihr f (by omega) hndr⟩

/-- The classified construction. -/
def sol (t : T) : T := build (sz t) (pre t) (inord t)

/-- CORRECT: `sol` rebuilds the original tree from its traversals (distinct values). -/
theorem corr (t : T) (h : (inord t).Nodup) : sol t = t :=
  build_round t (sz t) (le_refl _) h

end LC.P0105
