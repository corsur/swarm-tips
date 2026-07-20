import Lproofs.Schemes.Fold

/-! @lc 1438 | name:Longest Continuous Subarray With Absolute Diff ≤ Limit | scheme:fold |
    family:monotonic-stack | complexity:O(n) |
    source:https://leetcode.com/problems/longest-continuous-subarray-with-absolute-diff-less-than-or-equal-to-limit/

    The answer is the length of the longest contiguous subarray whose max minus min is `≤ limit`.
    Equivalently it is the maximum, over *all* nonempty contiguous subarrays, of
    `(length if max − min ≤ limit else 0)` — invalid subarrays contribute nothing, valid ones their
    length. Grouped by *start* position, the contribution of start `i` is the running `(max, min,
    length)` of the subarrays `[i..i], [i..i+1], …`. A streaming fold over the array (right-to-left)
    carries the running best length and the list of `(max, min, length)` of subarrays starting at the
    current position. `cls` certifies that fold; `corr` proves it equals `bestLen`, that maximum over
    all subarrays. (Singletons always satisfy `max − min = 0 ≤ limit`, so on a nonempty input the
    answer is ≥ 1.) The accepted sliding-window + monotonic-deque solution is this same fold with the
    `(max, min, length)` list compressed by its monotone structure into O(n). -/

namespace LC.P1438
open Interview.Patterns

/-- Extend a subarray's `(max, min, length)` by prepending `x`. -/
def ext (x : ℤ) (p : ℤ × ℤ × ℤ) : ℤ × ℤ × ℤ := (max x p.1, min x p.2.1, p.2.2 + 1)

/-- `(max, min, length)` of every subarray that starts at the head, in increasing length. -/
def startTriples : List ℤ → List (ℤ × ℤ × ℤ)
  | [] => []
  | x :: xs => (x, x, 1) :: (startTriples xs).map (ext x)

/-- Value of a subarray: its length if `max − min ≤ limit`, else 0. -/
def valOf (limit : ℤ) (p : ℤ × ℤ × ℤ) : ℤ := if p.1 - p.2.1 ≤ limit then p.2.2 else 0

/-- Max valid length over a list of `(max, min, length)` triples (0 on the empty list). -/
def lmaxLen (limit : ℤ) : List (ℤ × ℤ × ℤ) → ℤ
  | [] => 0
  | p :: ps => ps.foldl (fun acc q => max acc (valOf limit q)) (valOf limit p)

/-- Max valid length over all nonempty contiguous subarrays (floored at 0). -/
def bestLen (limit : ℤ) : List ℤ → ℤ
  | [] => 0
  | x :: xs => max (bestLen limit xs) (lmaxLen limit (startTriples (x :: xs)))

/-- One streaming step (right-to-left): the new head starts `[x]` and extends every subarray that
    started one position right; the running best absorbs the new subarrays' valid lengths. -/
def step (limit : ℤ) (x : ℤ) (s : ℤ × List (ℤ × ℤ × ℤ)) : ℤ × List (ℤ × ℤ × ℤ) :=
  let lst := (x, x, 1) :: s.2.map (ext x)
  (max s.1 (lmaxLen limit lst), lst)

/-- The accepted answer: the longest valid-subarray length. -/
def sol (limit : ℤ) (a : List ℤ) : ℤ := (a.foldr (step limit) (0, [])).1

/-- SCHEME (fold): the answer is a single fold over the array, carrying the running best length and
    the `(max, min, length)` of the subarrays starting at the current position. -/
theorem cls (limit : ℤ) : IsRightFold (fun a : List ℤ => a.foldr (step limit) (0, [])) ∧
    ∀ a : List ℤ, sol limit a = (a.foldr (step limit) (0, [])).1 :=
  ⟨⟨step limit, (0, []), fun _ => rfl⟩, fun _ => rfl⟩

/-- The fold accumulates exactly `(bestLen, startTriples)` at every prefix. -/
theorem fold_eq (limit : ℤ) : ∀ a : List ℤ,
    a.foldr (step limit) (0, []) = (bestLen limit a, startTriples a)
  | [] => rfl
  | x :: xs => by rw [List.foldr_cons, fold_eq limit xs]; rfl

/-- CORRECT: the fold's running best equals the max valid length over all nonempty subarrays. -/
theorem corr (limit : ℤ) (a : List ℤ) : sol limit a = bestLen limit a := by rw [sol, fold_eq]


/-- GROUND INSTANCE (official example 1): nums [8,2,4,7], limit 4 — longest valid subarray
    length 2. -/
theorem vec : sol 4 [8, 2, 4, 7] = 2 := by decide

end LC.P1438
