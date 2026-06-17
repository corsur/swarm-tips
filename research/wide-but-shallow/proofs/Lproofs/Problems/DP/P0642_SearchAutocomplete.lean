import Lproofs.Problems.DP.P0208_ImplementTrie

/-! @lc 642 | name:Design Search Autocomplete System | scheme:dp | family:trie |
    complexity:O(|w|) | source:https://leetcode.com/problems/design-search-autocomplete-system/ -/

namespace LC.P0642
open Interview.Patterns

/-- SCHEME (fold over the word): sentences are stored in the same trie ADT (LC 208); lookup is the
    left-fold descent over the word. -/
theorem cls (t : LC.P0208.Trie) : IsFold (fun w : List ℕ => w.foldl LC.P0208.stepNode (some t)) :=
  LC.P0208.cls t

/-- CORRECT: a stored sentence is retrievable (the insert/lookup round-trip on the trie). The
    frequency ranking is an annotation layered on this core. -/
theorem corr (t : LC.P0208.Trie) (w : List ℕ) :
    LC.P0208.contains (LC.P0208.insert t w) w = true :=
  LC.P0208.corr t w

end LC.P0642
