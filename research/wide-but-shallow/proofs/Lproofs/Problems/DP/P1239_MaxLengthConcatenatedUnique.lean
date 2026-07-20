import Lproofs.Schemes.Fold

/-! @lc 1239 | name:Maximum Length of a Concatenated String with Unique Characters | scheme:dp |
    family:backtracking | complexity:O(2ⁿ) | source:https://leetcode.com/problems/maximum-length-of-a-concatenated-string-with-unique-characters/

    We pick a subset of strings whose combined characters are all distinct, maximising total length.
    A string is a set of characters; two are combinable iff their character sets are disjoint.
    CLASSIFICATION (dp): subset search choosing compatible strings. CORRECTNESS: we certify the
    accounting the search relies on — when two character sets are disjoint, their union has exactly the
    sum of their sizes, so a valid concatenation's unique-character count is the sum of its parts'. -/

namespace LC.P1239

/-- The objective the subset search maximises: the size of the combined character set of the
    chosen words. -/
def sol (chosen : List (Finset ℕ)) : ℕ := (chosen.foldl (· ∪ ·) ∅).card

/-- SCHEME (fold): the combined character set is a streaming union fold — and `sol` reads its
    size. (Combinability is symmetric, so the subset search is order-free.) -/
theorem cls : Interview.Patterns.IsFold (fun l : List (Finset ℕ) => l.foldl (· ∪ ·) ∅) ∧
    (∀ l : List (Finset ℕ), sol l = (l.foldl (· ∪ ·) ∅).card) ∧
    ∀ s t : Finset ℕ, Disjoint s t ↔ Disjoint t s :=
  ⟨⟨(· ∪ ·), ∅, fun _ => rfl⟩, fun _ => rfl, fun _ _ => disjoint_comm⟩

/-- CORRECT: disjoint character sets combine without loss — `sol` on two disjoint words is the
    sum of their sizes (unique characters are preserved by the concatenation). -/
theorem corr (s t : Finset ℕ) (h : Disjoint s t) : sol [s, t] = s.card + t.card := by
  simp only [sol, List.foldl_cons, List.foldl_nil, Finset.empty_union]
  exact Finset.card_union_of_disjoint h


/-- GROUND INSTANCE (official example 1, "un","iq" as {1,2},{3,4}): disjoint words combine to 4
    unique characters ("uniq"); overlapping sets collapse (card 3, not 4). -/
theorem vec : sol [{1, 2}, {3, 4}] = 4 ∧ sol [{1, 2}, {2, 3}] = 3 := by decide

end LC.P1239
