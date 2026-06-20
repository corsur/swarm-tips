import Lproofs.Problems.DP.P0208_ImplementTrie

/-! @lc 211 | name:Design Add and Search Words Data Structure | scheme:dp | family:trie |
    complexity:O(|w|) | source:https://leetcode.com/problems/design-add-and-search-words-data-structure/

    Add words to a trie; search supports the `.` wildcard matching any single character.
    CLASSIFICATION: exact descent is a left fold over the word (the same trie core as LC 208).
    CORRECTNESS: we model the actual wildcard search — `.` descends to ANY existing child (an
    existential over the node's branches) — and prove that after adding a word `w`, the all-wildcard
    pattern of length `|w|` matches it, so the `.` wildcard genuinely descends to the inserted word. -/

namespace LC.P0211
open Interview.Patterns LC.P0208

/-- Wildcard search: `some x` follows the edge labelled `x`; `none` (the `.` wildcard) descends to
    ANY existing child. The empty pattern matches iff a word ends at the current node. -/
def wmatch : Trie → List (Option ℕ) → Prop
  | .node e _, [] => e = true
  | .node _ c, some x :: ps => match c x with | some t => wmatch t ps | none => False
  | .node _ c, none :: ps => ∃ x t, c x = some t ∧ wmatch t ps

/-- SCHEME (fold over the word): exact descent is a left fold carrying the current node. -/
theorem cls (t : Trie) : IsFold (fun w : List ℕ => w.foldl stepNode (some t)) := LC.P0208.cls t

/-- CORRECT: after adding `w`, the all-wildcard pattern of length `|w|` matches it — the `.` wildcard
    descends edge-by-edge to the just-inserted word. -/
theorem corr (t : Trie) (w : List ℕ) :
    wmatch (LC.P0208.insert t w) (List.replicate w.length none) := by
  induction w generalizing t with
  | nil => cases t with | node e c => simp [LC.P0208.insert, wmatch]
  | cons x xs ih =>
    cases t with
    | node e c =>
      simp only [LC.P0208.insert, List.length_cons, List.replicate_succ, wmatch]
      exact ⟨x, LC.P0208.insert ((c x).getD LC.P0208.empty) xs, by simp, ih _⟩

end LC.P0211
