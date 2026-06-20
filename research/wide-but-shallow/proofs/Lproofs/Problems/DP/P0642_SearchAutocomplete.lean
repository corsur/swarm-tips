import Lproofs.Problems.DP.P0208_ImplementTrie

/-! @lc 642 | name:Design Search Autocomplete System | scheme:dp | family:trie |
    complexity:O(|w|) | source:https://leetcode.com/problems/design-search-autocomplete-system/

    Autocomplete stores sentences in a trie and, given a typed prefix, descends to that prefix node and
    returns the completions beneath it. CLASSIFICATION: the prefix descent is a left fold over the typed
    characters (the same trie core as LC 208). CORRECTNESS: we model prefix navigation (`hasPrefix`:
    the trie can descend the given prefix) and prove that after a sentence `w` is inserted, its full
    path is navigable — so every prefix of `w` autocompletes to it. -/

namespace LC.P0642
open Interview.Patterns LC.P0208

/-- The trie can navigate the prefix `p` from the current node (autocomplete descends the prefix). -/
def hasPrefix : Trie → List ℕ → Prop
  | _, [] => True
  | .node _ c, x :: xs => ∃ t, c x = some t ∧ hasPrefix t xs

/-- SCHEME (fold over the typed prefix): the descent is a left fold carrying the current node. -/
theorem cls (t : Trie) : IsFold (fun w : List ℕ => w.foldl stepNode (some t)) := LC.P0208.cls t

/-- CORRECT: after inserting sentence `w`, its full path is navigable from the root — so every prefix
    of `w` descends to a node beneath which `w` is a completion. -/
theorem corr (t : Trie) (w : List ℕ) : hasPrefix (LC.P0208.insert t w) w := by
  induction w generalizing t with
  | nil => trivial
  | cons x xs ih =>
    cases t with
    | node e c =>
      simp only [LC.P0208.insert, hasPrefix]
      exact ⟨LC.P0208.insert ((c x).getD LC.P0208.empty) xs, by simp, ih _⟩

end LC.P0642
