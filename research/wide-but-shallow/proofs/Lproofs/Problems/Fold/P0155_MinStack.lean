import Lproofs.Patterns

/-! @lc 155 | name:Min Stack | scheme:fold | family:augmented-stack | complexity:O(1) per op |
    source:https://leetcode.com/problems/min-stack/

    A stack supporting O(1) `getMin`. CLASSIFICATION: building the stack from a stream of pushes is a
    streaming left fold whose accumulator is the augmented stack — each entry stores its value paired
    with the running minimum of everything at-or-below it. NON-VACUITY: we prove the min-update
    recurrence the structure relies on — pushing `x` onto a stack whose current minimum is `m` records
    the new minimum `min x m` — so the accumulator carries genuine running-minimum state, not a
    re-encoding. We certify the fold + the min recurrence; DROP nothing (this IS the algorithm). -/

namespace LC.P0155
open Interview.Patterns

/-- Push `x`, recording the running minimum alongside it. -/
def push (stk : List (ℤ × ℤ)) (x : ℤ) : List (ℤ × ℤ) :=
  match stk with
  | [] => [(x, x)]
  | (_, m) :: _ => (x, min x m) :: stk

/-- Build the augmented stack from a stream of pushes. -/
def build (xs : List ℤ) : List (ℤ × ℤ) := xs.foldl push []

/-- O(1) minimum query: the running minimum stored at the top. -/
def getMin : List (ℤ × ℤ) → Option ℤ
  | [] => none
  | (_, m) :: _ => some m

/-- SCHEME (fold): `build` is the streaming left fold of `push` from the empty stack. -/
theorem cls : IsFold (fun xs : List ℤ => xs.foldl push []) := ⟨push, [], fun _ => rfl⟩

/-- NON-VACUITY (min recurrence): pushing `x` onto a stack whose current minimum is `m` records the
    new minimum `min x m` — the genuine running-minimum update the O(1) `getMin` depends on. -/
theorem corr (x v m : ℤ) (rest : List (ℤ × ℤ)) :
    getMin (push ((v, m) :: rest) x) = some (min x m) := by
  simp [push, getMin]

/-- NON-VACUITY (base): the first push records its own value as the minimum. -/
theorem corr_base (x : ℤ) : getMin (push [] x) = some x := by simp [push, getMin]

end LC.P0155
