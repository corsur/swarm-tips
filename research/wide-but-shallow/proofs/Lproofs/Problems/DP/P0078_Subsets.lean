import Lproofs.Schemes.Fold

/-! @lc 78 | name:Subsets | scheme:dp | family:enumeration | complexity:O(n·2ⁿ) |
    source:https://leetcode.com/problems/subsets/

    Enumerate the power set. CLASSIFICATION: the accepted recursion is a genuine recursive
    decomposition — the subsets of `x :: xs` are the subsets of `xs` (those omitting `x`) together with
    those same subsets each extended by `x` (`cls`). NON-VACUITY: we prove soundness — every enumerated
    list is a genuine sublist (subsequence) of the input (`corr`) — so the decomposition explores real
    subsets, not a re-encoding. We certify the decomposition + soundness; DROP any counting claim. -/

namespace LC.P0078

variable {α : Type*}

/-- Power set by recursive decomposition: omit the head, or include it in each tail-subset. -/
def subsets : List α → List (List α)
  | [] => [[]]
  | x :: xs => subsets xs ++ (subsets xs).map (x :: ·)

/-- SCHEME (dp / recursive decomposition): subsets of `x :: xs` = subsets of `xs` (omitting `x`) ++
    those subsets each prefixed with `x` (including `x`). The genuine include/exclude split. -/
theorem cls (x : α) (xs : List α) :
    subsets (x :: xs) = subsets xs ++ (subsets xs).map (x :: ·) := rfl

/-- NON-VACUITY (soundness): every enumerated list is a genuine sublist (subset) of the input. -/
theorem corr : ∀ (l : List α) (s : List α), s ∈ subsets l → s.Sublist l
  | [], s, h => by simp only [subsets, List.mem_singleton] at h; subst h; exact .slnil
  | x :: xs, s, h => by
    rw [cls, List.mem_append, List.mem_map] at h
    rcases h with h | ⟨s', hs', rfl⟩
    · exact (corr xs s h).cons x
    · exact (corr xs s' hs').cons₂ x

end LC.P0078
