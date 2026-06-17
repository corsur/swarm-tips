import Lproofs.Schemes.Fold

/-! @lc 907 | name:Sum of Subarray Minimums | scheme:fold | family:monotonic-stack | complexity:O(n) |
    source:https://leetcode.com/problems/sum-of-subarray-minimums/

    Sum, over every nonempty contiguous subarray, of its minimum. Grouped by *start* position, the
    contribution of start `i` is the running minimum of the subarrays `[i..i], [i..i+1], …` — a
    streaming fold over the array (right-to-left), carrying the running total and the list of minima
    of the subarrays that start at the current position. `cls` certifies that fold; `corr` proves it
    equals `subMinSum`, the sum of minima over all nonempty contiguous subarrays. The accepted
    monotonic-stack solution is exactly this fold with the minimum-list compressed by its monotone
    structure into O(n); we certify the un-compressed canonical fold (same scheme), with full
    correctness. -/

namespace LC.P0907
open Interview.Patterns

/-- Prefix minimums: the minima of the subarrays that *start* at the head, `[x], [x,xs₀], …`. -/
def startMins : List ℤ → List ℤ
  | [] => []
  | x :: xs => x :: (startMins xs).map (min x)

/-- Sum of minima over all nonempty contiguous subarrays, grouped by start position. -/
def subMinSum : List ℤ → ℤ
  | [] => 0
  | x :: xs => (startMins (x :: xs)).sum + subMinSum xs

/-- One streaming step (right-to-left): the new head starts the subarray `[x]` and extends every
    subarray starting one position right by prepending `x` (lowering each minimum to `min x ·`). -/
def step (x : ℤ) (s : ℤ × List ℤ) : ℤ × List ℤ :=
  (s.1 + (x :: s.2.map (min x)).sum, x :: s.2.map (min x))

/-- The accepted answer: total sum of subarray minimums. -/
def sol (a : List ℤ) : ℤ := (a.foldr step (0, [])).1

/-- SCHEME (fold): the sum is a single fold over the array, carrying the running total and the minima
    of the subarrays starting at the current position. -/
theorem cls : IsRightFold (fun a : List ℤ => a.foldr step (0, [])) :=
  ⟨step, (0, []), fun _ => rfl⟩

/-- The fold accumulates exactly `(subMinSum, startMins)` at every prefix. -/
theorem fold_eq : ∀ a : List ℤ, a.foldr step (0, []) = (subMinSum a, startMins a)
  | [] => rfl
  | x :: xs => by
    rw [List.foldr_cons, fold_eq xs]
    simp only [step, startMins, subMinSum, Prod.mk.injEq]
    exact ⟨add_comm _ _, trivial⟩

/-- CORRECT: the fold's total equals the sum of minima over all nonempty contiguous subarrays. -/
theorem corr (a : List ℤ) : sol a = subMinSum a := by
  rw [sol, fold_eq]

end LC.P0907
