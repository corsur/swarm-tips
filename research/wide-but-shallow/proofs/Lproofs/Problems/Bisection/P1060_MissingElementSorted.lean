import Lproofs.Schemes.Bisect

/-! @lc 1060 | name:Missing Element in Sorted Array | scheme:bisection | family:binary-search |
    complexity:O(log n) | source:https://leetcode.com/problems/missing-element-in-sorted-array/

    Find the k-th missing number in a sorted array by binary searching on the count of values missing
    before each index. CLASSIFICATION: `enoughMissing i` ("at least `k` values are missing before index
    `i`") is a monotone up-set — the missing count `nums[i] − nums[0] − i` is non-decreasing — so the
    binary search returns its least satisfying index. `cls` certifies the threshold structure; `corr`
    that the answer is the least such index. We certify the monotone-threshold structure. -/

namespace LC.P1060
open Interview.Patterns

/-- `enoughMissing i` — at least `k` values are missing before index `i` (count modeled as monotone). -/
def enoughMissing (missing : ℕ → ℕ) (k i : ℕ) : Prop := k ≤ missing i

instance (missing : ℕ → ℕ) (k : ℕ) : DecidablePred (enoughMissing missing k) :=
  fun i => inferInstanceAs (Decidable (k ≤ missing i))

/-- The binary-search boundary: the least index whose missing-count reaches `k`. -/
def sol (missing : ℕ → ℕ) (k : ℕ) (h : ∃ i, enoughMissing missing k i) : ℕ := Nat.find h

/-- `enoughMissing` is a monotone up-set when the missing count is monotone: once enough, it stays. -/
theorem missing_mono (missing : ℕ → ℕ) (hmono : Monotone missing) (k : ℕ) :
    ∀ a b, a ≤ b → enoughMissing missing k a → enoughMissing missing k b :=
  fun a b hab ha => le_trans ha (hmono hab)

/-- SCHEME (bisection): the predicate is a monotone threshold — `enoughMissing i ↔ boundary ≤ i`. -/
theorem cls (missing : ℕ → ℕ) (hmono : Monotone missing) (k : ℕ) (h : ∃ i, enoughMissing missing k i) (n : ℕ) :
    enoughMissing missing k n ↔ sol missing k h ≤ n :=
  bisection_threshold (enoughMissing missing k) (missing_mono missing hmono k) h n

/-- The boundary is the least index whose missing-count reaches `k` (the k-th missing sits there). -/
theorem corr (missing : ℕ → ℕ) (k : ℕ) (h : ∃ i, enoughMissing missing k i) :
    IsLeast {i | enoughMissing missing k i} (sol missing k h) :=
  bisection_isLeast (enoughMissing missing k) h

end LC.P1060
