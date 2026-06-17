import Lproofs.Patterns

/-! @lc 1004 | name:Max Consecutive Ones III | scheme:fold | family:sliding-window | complexity:O(n) |
    source:https://leetcode.com/problems/max-consecutive-ones-iii/

    Longest run of ones after flipping at most `k` zeros; the sliding window shrinks when it holds more
    than `k` zeros. CLASSIFICATION: the window's defining state — the count of zeros inside it — is a
    streaming left fold with a scalar accumulator (the running zero count). NON-VACUITY: we prove the
    zero count never exceeds the number of elements scanned (each element adds at most one), so the
    accumulator does genuine zero-counting work; the window is legal exactly while this count `≤ k`. We
    certify the fold + the count bound. -/

namespace LC.P1004
open Interview.Patterns

/-- Running count of zeros — the window-validity state the algorithm maintains. -/
def zeros (xs : List ℕ) : ℕ := xs.foldl (fun acc x => if x = 0 then acc + 1 else acc) 0

/-- SCHEME (fold): the zero count is a left fold with a scalar accumulator. -/
theorem cls : IsFold (fun xs : List ℕ => xs.foldl (fun acc x => if x = 0 then acc + 1 else acc) 0) :=
  ⟨(fun acc x => if x = 0 then acc + 1 else acc), 0, fun _ => rfl⟩

/-- The zero count rises by at most one per element. -/
theorem foldl_bound (xs : List ℕ) (acc : ℕ) :
    xs.foldl (fun a x => if x = 0 then a + 1 else a) acc ≤ acc + xs.length := by
  induction xs generalizing acc with
  | nil => simp
  | cons x xs ih =>
    simp only [List.foldl_cons, List.length_cons]
    have h := ih (if x = 0 then acc + 1 else acc)
    have hb : (if x = 0 then acc + 1 else acc) ≤ acc + 1 := by split <;> omega
    omega

/-- NON-VACUITY: the zero count never exceeds the number of elements scanned. -/
theorem corr (xs : List ℕ) : zeros xs ≤ xs.length := by
  have := foldl_bound xs 0
  simpa [zeros] using this

end LC.P1004
