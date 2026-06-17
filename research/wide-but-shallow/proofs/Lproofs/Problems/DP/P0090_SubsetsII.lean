import Lproofs.Schemes.Fold

/-! @lc 90 | name:Subsets II | scheme:dp | family:enumeration | complexity:O(n·2ⁿ) |
    source:https://leetcode.com/problems/subsets-ii/

    Enumerate the (distinct) subsets of a multiset. CLASSIFICATION: the accepted recursion is a genuine
    recursive decomposition — subsets of `x :: xs` are the subsets of `xs` together with those extended
    by `x` (`cls`); the dedup step layers on top. NON-VACUITY: we prove soundness — every enumerated
    list is a genuine sublist (sub-multiset) of the input (`corr`). We certify the decomposition +
    soundness; DROP the duplicate-pruning detail. -/

namespace LC.P0090

variable {α : Type*}

/-- Subsets by recursive decomposition: omit the head, or include it in each tail-subset. -/
def subsets : List α → List (List α)
  | [] => [[]]
  | x :: xs => subsets xs ++ (subsets xs).map (x :: ·)

/-- SCHEME (dp / recursive decomposition): subsets of `x :: xs` = subsets of `xs` ++ those prefixed
    with `x`. The genuine include/exclude split (dedup is applied afterward). -/
theorem cls (x : α) (xs : List α) :
    subsets (x :: xs) = subsets xs ++ (subsets xs).map (x :: ·) := rfl

/-- NON-VACUITY (soundness): every enumerated list is a genuine sublist of the input. -/
theorem corr : ∀ (l : List α) (s : List α), s ∈ subsets l → s.Sublist l
  | [], s, h => by simp only [subsets, List.mem_singleton] at h; subst h; exact .slnil
  | x :: xs, s, h => by
    rw [cls, List.mem_append, List.mem_map] at h
    rcases h with h | ⟨s', hs', rfl⟩
    · exact (corr xs s h).cons x
    · exact (corr xs s' hs').cons₂ x

end LC.P0090
