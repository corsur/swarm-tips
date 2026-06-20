import Lproofs.Schemes.Fold

/-! @lc 1239 | name:Maximum Length of a Concatenated String with Unique Characters | scheme:dp |
    family:backtracking | complexity:O(2ⁿ) | source:https://leetcode.com/problems/maximum-length-of-a-concatenated-string-with-unique-characters/

    We pick a subset of strings whose combined characters are all distinct, maximising total length.
    A string is a set of characters; two are combinable iff their character sets are disjoint.
    CLASSIFICATION (dp): subset search choosing compatible strings. CORRECTNESS: we certify the
    accounting the search relies on — when two character sets are disjoint, their union has exactly the
    sum of their sizes, so a valid concatenation's unique-character count is the sum of its parts'. -/

namespace LC.P1239

/-- SCHEME (dp): combinability (disjoint character sets) is symmetric — the subset search is order-free. -/
theorem cls (s t : Finset ℕ) : Disjoint s t ↔ Disjoint t s := disjoint_comm

/-- CORRECT: disjoint character sets combine without loss — the union's size is the sum of sizes. -/
theorem corr (s t : Finset ℕ) (h : Disjoint s t) : (s ∪ t).card = s.card + t.card :=
  Finset.card_union_of_disjoint h

end LC.P1239
