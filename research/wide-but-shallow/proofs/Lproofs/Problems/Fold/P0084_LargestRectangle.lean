import Lproofs.Schemes.Fold

/-! @lc 84 | name:Largest Rectangle in Histogram | scheme:fold | family:monotonic-stack |
    complexity:O(n) | source:https://leetcode.com/problems/largest-rectangle-in-histogram/

    The largest axis-aligned rectangle under a histogram spanning columns `[i..j]` has height
    `min(heights[i..j])` and width `j−i+1`, so its area is `min × length`; the answer is the maximum
    of `min × length` over all nonempty contiguous subarrays. Grouped by *start* position, the
    contribution of start `i` is the running `(min, length)` of the subarrays `[i..i], [i..i+1], …`,
    and we want the best area. A streaming fold over the array (right-to-left) carries the running
    best area and the list of `(min, length)` of subarrays starting at the current position. `cls`
    certifies that fold; `corr` proves it equals `bestRect`, the max of `min × length` over all
    nonempty contiguous subarrays (or 0; heights are nonnegative so the `max 0` floor never binds).
    The accepted monotonic-stack solution is this same fold with the `(min, length)` list compressed
    by its monotone structure into O(n). -/

namespace LC.P0084
open Interview.Patterns

/-- Extend a subarray's `(min, length)` by prepending `x`. -/
def ext (x : ℤ) (p : ℤ × ℤ) : ℤ × ℤ := (min x p.1, p.2 + 1)

/-- `(min, length)` of every subarray that starts at the head, in order of increasing length. -/
def startPairs : List ℤ → List (ℤ × ℤ)
  | [] => []
  | x :: xs => (x, 1) :: (startPairs xs).map (ext x)

/-- Max of `min × length` over a list of `(min, length)` pairs (0 on the empty list). -/
def lmaxArea : List (ℤ × ℤ) → ℤ
  | [] => 0
  | p :: ps => ps.foldl (fun acc q => max acc (q.1 * q.2)) (p.1 * p.2)

/-- Max rectangle area over all nonempty contiguous subarrays (floored at 0). -/
def bestRect : List ℤ → ℤ
  | [] => 0
  | x :: xs => max (bestRect xs) (lmaxArea (startPairs (x :: xs)))

/-- One streaming step (right-to-left): the new head starts `[x]` (height x, width 1) and extends
    every subarray that started one position right; the running best absorbs the new areas. -/
def step (x : ℤ) (s : ℤ × List (ℤ × ℤ)) : ℤ × List (ℤ × ℤ) :=
  let lst := (x, 1) :: s.2.map (ext x)
  (max s.1 (lmaxArea lst), lst)

/-- The accepted answer: the largest rectangle area. -/
def sol (a : List ℤ) : ℤ := (a.foldr step (0, [])).1

/-- SCHEME (fold): the answer is a single fold over the array, carrying the running best area and the
    `(min, length)` of the subarrays starting at the current position. -/
theorem cls : IsRightFold (fun a : List ℤ => a.foldr step (0, [])) :=
  ⟨step, (0, []), fun _ => rfl⟩

/-- The fold accumulates exactly `(bestRect, startPairs)` at every prefix. -/
theorem fold_eq : ∀ a : List ℤ, a.foldr step (0, []) = (bestRect a, startPairs a)
  | [] => rfl
  | x :: xs => by rw [List.foldr_cons, fold_eq xs]; rfl

/-- CORRECT: the fold's running best equals the max of `min × length` over all nonempty subarrays. -/
theorem corr (a : List ℤ) : sol a = bestRect a := by rw [sol, fold_eq]

end LC.P0084
