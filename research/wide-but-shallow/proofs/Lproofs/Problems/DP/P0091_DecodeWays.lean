import Lproofs.Schemes.Fold

/-! @lc 91 | name:Decode Ways | scheme:dp | family:dp-linear | complexity:O(n) |
    source:https://leetcode.com/problems/decode-ways/

    Count the ways to decode a digit string (`1..26 ↦ A..Z`). CLASSIFICATION: the accepted DP is a
    linear recursive decomposition — at each position, decode one digit (if nonzero) and recurse on the
    tail, plus decode two digits (if they form `10..26`) and recurse two ahead (`cls`). NON-VACUITY: we
    show the recurrence genuinely counts both branches — e.g. `"12"` yields 2 decodings (`AB` and `L`)
    (`corr`) — so it is real counting work, not a re-encoding. We certify the recurrence + a non-trivial
    count. -/

namespace LC.P0091

/-- Number of decodings of a big-endian digit list. -/
def ways : List ℕ → ℕ
  | [] => 1
  | [d] => if d = 0 then 0 else 1
  | d1 :: d2 :: rest =>
      (if d1 = 0 then 0 else ways (d2 :: rest)) +
      (if d1 ≠ 0 ∧ d1 * 10 + d2 ≤ 26 then ways rest else 0)

/-- SCHEME (dp / linear recursion): the count splits into the single-digit branch (recurse on the
    tail) and the two-digit branch (recurse two ahead, when the pair is a valid `10..26`). -/
theorem cls (d1 d2 : ℕ) (rest : List ℕ) :
    ways (d1 :: d2 :: rest) =
      (if d1 = 0 then 0 else ways (d2 :: rest)) +
      (if d1 ≠ 0 ∧ d1 * 10 + d2 ≤ 26 then ways rest else 0) := rfl

/-- NON-VACUITY: the recurrence genuinely counts both decodings of `"12"` (`AB` and `L`). -/
theorem corr : ways [1, 2] = 2 := by decide

end LC.P0091
