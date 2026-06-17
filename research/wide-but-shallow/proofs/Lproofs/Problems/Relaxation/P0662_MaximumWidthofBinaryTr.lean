import Lproofs.Schemes.Fold

/-! @lc 662 | name:Maximum Width of Binary Tree | scheme:dp | family:tree-aggregate |
    complexity:O(n) | source:https://leetcode.com/problems/maximum-width-of-binary-tree/

    The width of a level is `rightmost − leftmost + 1` using *heap indexing*: the root is position
    `1`, and a node at position `p` has children `2p` (left) and `2p+1` (right). The answer is the
    maximum level width. The accepted scan assigns these indices while descending and tracks, per
    level, the first/last index — a tree catamorphism over the structure (`cls`). Its only subtle
    correctness fact, which `corr` proves, is that the catamorphic per-level index list is *exactly*
    the heap indices of the genuine depth-`d` nodes (`posFrom` along each existing path). Given that,
    the level span and the level-max are read off directly. -/

namespace LC.P0662

/-- The binary tree (values are irrelevant to width). -/
abbrev T := Interview.Patterns.Tree ℤ

/-- A path from the root: `false` = left, `true` = right. -/
abbrev Path := List Bool

/-- Heap index reached by following `path` from a node at position `p` (left `2p`, right `2p+1`). -/
def posFrom (p : ℕ) : Path → ℕ
  | [] => p
  | false :: rest => posFrom (2 * p) rest
  | true :: rest => posFrom (2 * p + 1) rest

/-- The depth-`d` frontier: every path of length `d` that lands on an actual node. -/
def paths : T → ℕ → List Path
  | .leaf, _ => []
  | .node _ _ _, 0 => [[]]
  | .node l _ r, d + 1 => (paths l d).map (false :: ·) ++ (paths r d).map (true :: ·)

/-- Catamorphic per-level index list: heap indices of all depth-`d` nodes, this subtree rooted at
    position `p`. The node contributes `p` at depth `0`; deeper levels split into the two children
    at `2p` and `2p+1`. -/
def posList : T → ℕ → ℕ → List ℕ
  | .leaf, _, _ => []
  | .node _ _ _, p, 0 => [p]
  | .node l _ r, p, d + 1 => posList l (2 * p) d ++ posList r (2 * p + 1) d

/-- Width of one level from its index list: `max − min + 1`, or `0` when the level is empty. -/
def widthOf (idxs : List ℕ) : ℕ :=
  match idxs.max?, idxs.min? with
  | some hi, some lo => hi - lo + 1
  | _, _ => 0

/-- Number of levels. -/
def height : T → ℕ
  | .leaf => 0
  | .node l _ r => max (height l) (height r) + 1

/-- The accepted answer: the maximum width over all levels. -/
def sol (t : T) : ℕ :=
  ((List.range (height t)).map (fun d => widthOf (posList t 1 d))).foldr max 0

/-- SCHEME (dp / tree catamorphism): the per-level index list is built by the recursive split —
    a node's depth-`d+1` indices are its left subtree's depth-`d` indices (rooted at `2p`) followed
    by its right subtree's (rooted at `2p+1`). -/
theorem cls (l r : T) (v : ℤ) (p d : ℕ) :
    posList (.node l v r) p (d + 1) = posList l (2 * p) d ++ posList r (2 * p + 1) d :=
  rfl

/-- The catamorphic index list equals the genuine heap indices of the actual depth-`d` nodes. -/
theorem posList_eq : ∀ (t : T) (p d : ℕ), posList t p d = (paths t d).map (posFrom p)
  | .leaf, _, _ => rfl
  | .node _ _ _, _, 0 => rfl
  | .node l v r, p, d + 1 => by
    show posList l (2 * p) d ++ posList r (2 * p + 1) d
        = ((paths l d).map (false :: ·) ++ (paths r d).map (true :: ·)).map (posFrom p)
    rw [List.map_append, List.map_map, List.map_map, posList_eq l (2 * p) d,
      posList_eq r (2 * p + 1) d]
    congr 1 <;> exact List.map_congr_left fun a _ => rfl

/-- CORRECT (the crux): the index list the width scan reads at each level is exactly the heap
    indices `posFrom 1` of the genuine depth-`d` nodes — so each level's `widthOf` is the true span,
    and `sol` the true maximum width. -/
theorem corr (t : T) (d : ℕ) : posList t 1 d = (paths t d).map (posFrom 1) :=
  posList_eq t 1 d

end LC.P0662
