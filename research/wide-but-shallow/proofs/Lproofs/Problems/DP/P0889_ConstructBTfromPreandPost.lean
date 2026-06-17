import Lproofs.Schemes.Fold

/-! @lc 889 | name:Construct Binary Tree from Preorder and Postorder Traversal | scheme:dp |
    family:tree-construct | complexity:O(n²) |
    source:https://leetcode.com/problems/construct-binary-tree-from-preorder-and-postorder-traversal/

    Rebuild a full binary tree (every node has 0 or 2 children, distinct values) from its preorder
    and postorder traversals: the head of preorder is the root; the next preorder value is the left
    child's root, whose position in postorder pins the size of the left subtree, and the two halves
    recurse — a divide-and-conquer / recursive decomposition (`cls`). Correctness (`corr`): the
    construction inverts the traversals, `build (preorder t) (postorder t) = t` for every full tree
    with distinct values, so it is lossless. -/

namespace LC.P0889

/-- Full binary tree: leaves carry a value; internal nodes have exactly two subtrees. -/
inductive FBT where
  | leaf : ℤ → FBT
  | node : FBT → ℤ → FBT → FBT

/-- Preorder: root, then left, then right. -/
def pre : FBT → List ℤ
  | .leaf v => [v]
  | .node l v r => v :: (pre l ++ pre r)

/-- Postorder: left, then right, then root. -/
def post : FBT → List ℤ
  | .leaf v => [v]
  | .node l v r => post l ++ post r ++ [v]

/-- Node count. -/
def sz : FBT → ℕ
  | .leaf _ => 1
  | .node l _ r => sz l + sz r + 1

/-- The root value (head of preorder, last of postorder). -/
def rootOf : FBT → ℤ
  | .leaf v => v
  | .node _ v _ => v

theorem sz_pos : ∀ t, 0 < sz t
  | .leaf _ => Nat.one_pos
  | .node l _ r => by simp only [sz]; omega

theorem pre_len : ∀ t, (pre t).length = sz t
  | .leaf _ => rfl
  | .node l _ r => by
    simp only [pre, sz, List.length_cons, List.length_append, pre_len l, pre_len r, Nat.add_comm,
      Nat.add_left_comm]

theorem post_len : ∀ t, (post t).length = sz t
  | .leaf _ => rfl
  | .node l _ r => by
    simp only [post, sz, List.length_append, List.length_cons, List.length_nil, post_len l,
      post_len r, Nat.add_comm, Nat.add_left_comm]

theorem pre_ne_nil : ∀ t, pre t ≠ []
  | .leaf _ => by simp [pre]
  | .node _ _ _ => by simp [pre]

/-- The head of `pre t` is the root. -/
theorem pre_head : ∀ t, pre t = rootOf t :: (pre t).tail
  | .leaf v => rfl
  | .node l v r => rfl

/-- `pre (node l v r)` exposed as `root :: leftRoot :: rest`, so `build`'s node pattern fires. -/
theorem pre_node_cons (l : FBT) (v : ℤ) (r : FBT) :
    pre (.node l v r) = v :: rootOf l :: ((pre l).tail ++ pre r) := by
  conv_lhs => rw [pre, pre_head l]
  simp

/-- The root is the last postorder value. -/
theorem post_eq_dropLast : ∀ t : FBT, post t = (post t).dropLast ++ [rootOf t]
  | .leaf v => rfl
  | .node l v r => by
    simp only [post, rootOf, ← List.append_assoc, List.dropLast_concat]

theorem rootOf_mem_post : ∀ t : FBT, rootOf t ∈ post t
  | .leaf v => by simp [post, rootOf]
  | .node l v r => by simp [post, rootOf]

/-- Build a full binary tree from preorder and postorder (fuel bounds the recursion). -/
def build : ℕ → List ℤ → List ℤ → FBT
  | _, [v], _ => .leaf v
  | fuel + 1, v :: w :: pre', post =>
    let k := post.idxOf w + 1
    .node (build fuel ((w :: pre').take k) (post.take k)) v
          (build fuel ((w :: pre').drop k) ((post.drop k).dropLast))
  | _, _, _ => .leaf 0

/-- SCHEME (dp / divide-and-conquer): the construction is a recursive decomposition — root from the
    head of preorder, then left and right subtrees built from the split halves. -/
theorem cls (fuel : ℕ) (v w : ℤ) (pre' postL : List ℤ) :
    build (fuel + 1) (v :: w :: pre') postL =
      .node (build fuel ((w :: pre').take (postL.idxOf w + 1)) (postL.take (postL.idxOf w + 1))) v
            (build fuel ((w :: pre').drop (postL.idxOf w + 1))
              ((postL.drop (postL.idxOf w + 1)).dropLast)) :=
  rfl

/-- `NoDup` of the whole postorder restricts to the left subtree. -/
theorem nodup_left (l : FBT) (v : ℤ) (r : FBT) (h : (post (.node l v r)).Nodup) :
    (post l).Nodup := by
  simp only [post, List.append_assoc] at h
  exact (List.nodup_append.mp h).1

/-- The left root sits at index `sz l − 1` of the postorder (its last appearance in `post l`). -/
theorem idxOf_left_root (l : FBT) (v : ℤ) (r : FBT) (h : (post (.node l v r)).Nodup) :
    (post (.node l v r)).idxOf (rootOf l) + 1 = sz l := by
  have hpl : (post l).Nodup := nodup_left l v r h
  have hmem : rootOf l ∈ post l := rootOf_mem_post l
  have hsplit : post l = (post l).dropLast ++ [rootOf l] := post_eq_dropLast l
  have hnotin : rootOf l ∉ (post l).dropLast := by
    rw [hsplit] at hpl
    exact fun hc => (List.disjoint_of_nodup_append hpl) hc (by simp)
  have hlen : (post l).dropLast.length = sz l - 1 := by
    have := post_len l
    have hpos := sz_pos l
    rw [List.length_dropLast, this]
  -- idxOf in `post l ++ (post r ++ [v])` lands inside `post l`
  have : (post (.node l v r)).idxOf (rootOf l) = (post l).dropLast.length := by
    simp only [post, List.append_assoc]
    rw [List.idxOf_append, if_pos hmem]
    conv_lhs => rw [hsplit]
    rw [List.idxOf_append, if_neg hnotin]
    simp [List.idxOf_cons_self]
  rw [this, hlen]
  have := sz_pos l; omega

/-- CORRECT (round-trip): with enough fuel and distinct values, `build` inverts the traversals. -/
theorem build_round : ∀ (t : FBT) (fuel : ℕ), sz t ≤ fuel → (post t).Nodup →
    build fuel (pre t) (post t) = t := by
  intro t
  induction t with
  | leaf v => intro fuel _ _; simp only [pre, post, build]
  | node l v r ihl ihr =>
    intro fuel hfuel hnd
    obtain ⟨f, rfl⟩ : ∃ f, fuel = f + 1 :=
      ⟨fuel - 1, by have := sz_pos (FBT.node l v r); omega⟩
    simp only [sz] at hfuel
    have hk : (post (.node l v r)).idxOf (rootOf l) + 1 = sz l := idxOf_left_root l v r hnd
    have hpreLR : rootOf l :: ((pre l).tail ++ pre r) = pre l ++ pre r := by
      conv_rhs => rw [pre_head l]
      rw [List.cons_append]
    have hndl : (post l).Nodup := nodup_left l v r hnd
    have hndr : (post r).Nodup := by
      simp only [post, List.append_assoc] at hnd
      exact (List.nodup_append.mp (List.nodup_append.mp hnd).2.1).1
    have ht1 : (pre l ++ pre r).take (sz l) = pre l := by rw [← pre_len l]; exact List.take_left
    have hd1 : (pre l ++ pre r).drop (sz l) = pre r := by rw [← pre_len l]; exact List.drop_left
    have ht2 : (post (.node l v r)).take (sz l) = post l := by
      simp only [post, List.append_assoc]; rw [← post_len l]; exact List.take_left
    have hd2 : ((post (.node l v r)).drop (sz l)).dropLast = post r := by
      simp only [post, List.append_assoc]; rw [← post_len l, List.drop_left, List.dropLast_concat]
    rw [pre_node_cons l v r, cls, hk, hpreLR, ht1, hd1, ht2, hd2, FBT.node.injEq]
    exact ⟨ihl f (by omega) hndl, rfl, ihr f (by omega) hndr⟩

/-- The classified construction. -/
def sol (t : FBT) : FBT := build (sz t) (pre t) (post t)

/-- CORRECT: `sol` rebuilds the original full tree from its traversals (distinct values). -/
theorem corr (t : FBT) (h : (post t).Nodup) : sol t = t :=
  build_round t (sz t) (le_refl _) h

end LC.P0889
