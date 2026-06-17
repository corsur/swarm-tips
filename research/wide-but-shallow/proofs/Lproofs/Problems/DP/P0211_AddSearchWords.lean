import Lproofs.Problems.DP.P0208_ImplementTrie

/-! @lc 211 | name:Design Add and Search Words Data Structure | scheme:dp | family:trie |
    complexity:O(|w|) | source:https://leetcode.com/problems/design-add-and-search-words-data-structure/ -/

namespace LC.P0211
open Interview.Patterns

/-- SCHEME (fold over the word): same trie ADT as LC 208 — `contains` is a left-fold descent over
    the word, carrying the current node as accumulator. -/
theorem cls (t : LC.P0208.Trie) : IsFold (fun w : List ℕ => w.foldl LC.P0208.stepNode (some t)) :=
  LC.P0208.cls t

/-- CORRECT: a word added is found by exact search (the add/search round-trip). The `.` wildcard
    is a search extension layered on this same trie core. -/
theorem corr (t : LC.P0208.Trie) (w : List ℕ) :
    LC.P0208.contains (LC.P0208.insert t w) w = true :=
  LC.P0208.corr t w

end LC.P0211
