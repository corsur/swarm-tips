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
def sol : Trie → List ℕ → Bool
  | .node e _, [] => e
  | .node _ c, x :: xs => match c x with
    | some t => sol t xs
    | none => false

/-- One descent step: from the current node (if any), follow the edge labelled `x`. -/
def stepNode (st : Option Trie) (x : ℕ) : Option Trie :=
  st.bind (fun t => match t with | .node _ c => c x)

/-- A broken path (`none`) stays broken. -/
theorem foldl_none (w : List ℕ) : w.foldl stepNode none = none := by
  induction w with
  | nil => rfl
  | cons x xs ih => simpa [stepNode] using ih

/-- `sol` is exactly the descent fold, read off at the final node's end-flag. -/
theorem contains_eq : ∀ (t : Trie) (w : List ℕ),
    sol t w = (match w.foldl stepNode (some t) with | some (.node e _) => e | none => false) := by
  intro t w
  induction w generalizing t with
  | nil => cases t; rfl
  | cons x xs ih =>
    cases t with
    | node e c =>
      simp only [sol, List.foldl_cons, stepNode, Option.bind_some]
      cases hcx : c x with
      | none => simp [foldl_none]
      | some t' => exact ih t'

/-- SCHEME (fold over the word): the search is a single left-to-right descent — a left fold over
    the word carrying the current trie node — and `sol` reads exactly that fold's final node. -/
theorem cls (t : Trie) : IsFold (fun w : List ℕ => w.foldl stepNode (some t)) ∧
    ∀ w : List ℕ, sol t w =
      (match w.foldl stepNode (some t) with | some (.node e _) => e | none => false) :=
  ⟨⟨stepNode, some t, fun _ => rfl⟩, contains_eq t⟩

/-- CORRECT: a word is found after it is inserted (the insert/sol round-trip). -/
theorem corr (t : Trie) (w : List ℕ) : sol (insert t w) w = true := by
  induction w generalizing t with
  | nil => cases t; rfl
  | cons x xs ih =>
    cases t with
    | node e c =>
      simp only [insert, sol, if_pos]
      exact ih _

/-- GROUND INSTANCE (official example, letters a,p,l,e as 1,2,3,4): after inserting "apple",
    searching "apple" succeeds and the prefix "app" alone does not (no word ends there). -/
theorem vec : sol (insert empty [1, 2, 2, 3, 4]) [1, 2, 2, 3, 4] = true ∧
    sol (insert empty [1, 2, 2, 3, 4]) [1, 2, 2] = false := by decide

end LC.P0208
