import Lproofs.Schemes.Fold

/-! @lc 208 | name:Implement Trie (Prefix Tree) | scheme:dp | family:trie | complexity:O(|w|) |
    source:https://leetcode.com/problems/implement-trie-prefix-tree/ -/

namespace LC.P0208
open Interview.Patterns

/-- A trie node: a flag (a word ends here) and children indexed by character (`ℕ`). -/
inductive Trie where
  | node (isEnd : Bool) (child : ℕ → Option Trie)

/-- The empty trie. -/
def empty : Trie := .node false (fun _ => none)

/-- Insert a word (structural recursion on the word). -/
def insert : Trie → List ℕ → Trie
  | .node _ c, [] => .node true c
  | .node e c, x :: xs =>
    .node e (fun y => if y = x then some (insert ((c x).getD empty) xs) else c y)

/-- Search for a word. -/
def contains : Trie → List ℕ → Bool
  | .node e _, [] => e
  | .node _ c, x :: xs => match c x with
    | some t => contains t xs
    | none => false

/-- One descent step: from the current node (if any), follow the edge labelled `x`. -/
def stepNode (st : Option Trie) (x : ℕ) : Option Trie :=
  st.bind (fun t => match t with | .node _ c => c x)

/-- SCHEME (fold over the word): `contains` is a single left-to-right descent — a left fold over
    the word carrying the current trie node (`Option Trie`) as its accumulator. -/
theorem cls (t : Trie) : IsFold (fun w : List ℕ => w.foldl stepNode (some t)) :=
  ⟨stepNode, some t, fun _ => rfl⟩

/-- A broken path (`none`) stays broken. -/
theorem foldl_none (w : List ℕ) : w.foldl stepNode none = none := by
  induction w with
  | nil => rfl
  | cons x xs ih => simpa [stepNode] using ih

/-- `contains` is exactly the descent fold, read off at the final node's end-flag. -/
theorem contains_eq : ∀ (t : Trie) (w : List ℕ),
    contains t w = (match w.foldl stepNode (some t) with | some (.node e _) => e | none => false) := by
  intro t w
  induction w generalizing t with
  | nil => cases t; rfl
  | cons x xs ih =>
    cases t with
    | node e c =>
      simp only [contains, List.foldl_cons, stepNode, Option.bind_some]
      cases hcx : c x with
      | none => simp [foldl_none]
      | some t' => exact ih t'

/-- CORRECT: a word is found after it is inserted (the insert/contains round-trip). -/
theorem corr (t : Trie) (w : List ℕ) : contains (insert t w) w = true := by
  induction w generalizing t with
  | nil => cases t; rfl
  | cons x xs ih =>
    cases t with
    | node e c =>
      simp only [insert, contains, if_pos]
      exact ih _

end LC.P0208
