import Lproofs.Schemes.Fold

/-! @lc 1856 | name:Maximum Subarray Min-Product | scheme:fold | family:monotonic-stack |
    complexity:O(n) | source:https://leetcode.com/problems/maximum-subarray-min-product/

    The min-product of a subarray is `min(subarray) × sum(subarray)`; the answer is the maximum over
    all nonempty contiguous subarrays. Grouped by *start* position, the contribution of start `i` is
    the running `(min, sum)` of the subarrays `[i..i], [i..i+1], …`, and we want the best product. A
    streaming fold over the array (right-to-left) carries the running best-product and the list of
    `(min, sum)` of subarrays starting at the current position. `cls` certifies that fold; `corr`
    proves it equals `bestProd`, the max of `min × sum` over all nonempty contiguous subarrays (or 0).
    Inputs are positive (LeetCode `1 ≤ nums[i]`), so the `max 0` floor never binds and the final
    mod-`1e9+7` is mechanical formatting. The accepted prefix-sum + monotonic-stack solution is this
    same fold with the `(min, sum)` list compressed by its monotone structure into O(n). -/

namespace LC.P1856
open Interview.Patterns

/-- Extend a subarray's `(min, sum)` by prepending `x`. -/
def ext (x : ℤ) (p : ℤ × ℤ) : ℤ × ℤ := (min x p.1, x + p.2)

/-- `(min, sum)` of every subarray that starts at the head, in order of increasing length. -/
def startPairs : List ℤ → List (ℤ × ℤ)
  | [] => []
  | x :: xs => (x, x) :: (startPairs xs).map (ext x)

/-- Max of `min × sum` over a list of `(min, sum)` pairs (0 on the empty list). -/
def lmaxProd : List (ℤ × ℤ) → ℤ
  | [] => 0
  | p :: ps => ps.foldl (fun acc q => max acc (q.1 * q.2)) (p.1 * p.2)

/-- Max of `min × sum` over all nonempty contiguous subarrays (floored at 0). -/
def bestProd : List ℤ → ℤ
  | [] => 0
  | x :: xs => max (bestProd xs) (lmaxProd (startPairs (x :: xs)))

/-- One streaming step (right-to-left): the new head starts `[x]` and extends every subarray that
    started one position right; the running best absorbs all the new subarrays' products. -/
def step (x : ℤ) (s : ℤ × List (ℤ × ℤ)) : ℤ × List (ℤ × ℤ) :=
  let lst := (x, x) :: s.2.map (ext x)
  (max s.1 (lmaxProd lst), lst)

/-- The accepted answer (pre-modulo): the maximum subarray min-product. -/
def sol (a : List ℤ) : ℤ := (a.foldr step (0, [])).1

/-- SCHEME (fold): the answer is a single fold over the array, carrying the running best product and
    the `(min, sum)` of the subarrays starting at the current position. -/
theorem cls : IsRightFold (fun a : List ℤ => a.foldr step (0, [])) :=
  ⟨step, (0, []), fun _ => rfl⟩

/-- The fold accumulates exactly `(bestProd, startPairs)` at every prefix. -/
theorem fold_eq : ∀ a : List ℤ, a.foldr step (0, []) = (bestProd a, startPairs a)
  | [] => rfl
  | x :: xs => by rw [List.foldr_cons, fold_eq xs]; rfl

/-- CORRECT: the fold's running best equals the max of `min × sum` over all nonempty subarrays. -/
theorem corr (a : List ℤ) : sol a = bestProd a := by rw [sol, fold_eq]

end LC.P1856
