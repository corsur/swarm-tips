import Lproofs.Schemes.Fold

/-! @lc 1166 | name:Design File System | scheme:dp | family:trie | complexity:O(|path|) |
    source:https://leetcode.com/problems/design-file-system/

    A file system maps slash-separated paths to values, stored as a trie keyed by path component.
    CLASSIFICATION (dp): lookup is a left fold descending the path one component at a time (a trie
    catamorphism). CORRECTNESS: we certify the key–value contract — after `createPath path val`, looking
    up that exact `path` returns `some val`. -/

namespace LC.P1166

/-- A path trie node: an optional value here, and children keyed by the next path component. -/
inductive FS where
  | node (val : Option ℤ) (children : ℕ → Option FS)

def empty : FS := .node none (fun _ => none)

/-- Look up the value stored at `path`. -/
def get : FS → List ℕ → Option ℤ
  | .node v _, [] => v
  | .node _ c, x :: xs => match c x with
    | some t => get t xs
    | none => none

/-- Store `val` at `path`, creating intermediate nodes as needed. -/
def createPath : FS → List ℕ → ℤ → FS
  | .node _ c, [], val => .node (some val) c
  | .node v c, x :: xs, val =>
    .node v fun y => if y = x then some (createPath ((c x).getD empty) xs val) else c y

/-- SCHEME (dp): the lookup decomposes head-then-tail — descend into the child for the first component
    and recurse, the trie catamorphism's per-step. -/
theorem cls (v : Option ℤ) (c : ℕ → Option FS) (x : ℕ) (xs : List ℕ) :
    get (.node v c) (x :: xs) = (c x).bind fun t => get t xs := by
  cases h : c x <;> simp [get, h]

/-- CORRECT: after creating `path` with value `val`, looking up that path returns exactly `val`. -/
theorem corr (t : FS) (path : List ℕ) (val : ℤ) : get (createPath t path val) path = some val := by
  induction path generalizing t with
  | nil => cases t; simp [createPath, get]
  | cons x xs ih =>
    cases t with
    | node v c =>
      simp only [createPath, get, if_pos rfl]
      exact ih _

end LC.P1166
