import Lproofs.Schemes.Fold

/-! @lc 494 | name:Target Sum | scheme:dp | family:dp-linear | complexity:O(n·sum) |
    source:https://leetcode.com/problems/target-sum/

    Count the ways to assign `+`/`−` to each number so the signed sum equals a target. CLASSIFICATION:
    the accepted DP is a recursive decomposition — for each number, branch on its sign and recurse on
    the rest with the residual target (`cls`). NON-VACUITY: we enumerate the genuine sign assignments
    and prove soundness — every enumerated assignment's signed sum equals the target (`corr`) — so the
    decomposition explores real assignments, not a re-encoding. We certify the decomposition +
    soundness; DROP the count. -/

namespace LC.P0494

/-- Signed assignments of `l` whose signed sum is `t` — branch `+x` or `−x` per element. -/
def assigns : List ℤ → ℤ → List (List ℤ)
  | [], t => if t = 0 then [[]] else []
  | x :: xs, t => (assigns xs (t - x)).map (x :: ·) ++ (assigns xs (t + x)).map ((-x) :: ·)

/-- SCHEME (dp / recursive decomposition): branch on the head's sign (recurse with `t − x` for `+x`,
    with `t + x` for `−x`) — the genuine signed-subset-sum split. -/
theorem cls (x : ℤ) (xs : List ℤ) (t : ℤ) :
    assigns (x :: xs) t =
      (assigns xs (t - x)).map (x :: ·) ++ (assigns xs (t + x)).map ((-x) :: ·) := rfl

/-- NON-VACUITY (soundness): every enumerated assignment's signed sum equals the target. -/
theorem corr : ∀ (l : List ℤ) (t : ℤ) (a : List ℤ), a ∈ assigns l t → a.sum = t := by
  intro l
  induction l with
  | nil =>
    intro t a ha
    by_cases h : t = 0
    · subst h; simp only [assigns, if_pos, List.mem_singleton] at ha; subst ha; rfl
    · simp [assigns, h] at ha
  | cons x xs ih =>
    intro t a ha
    rw [cls, List.mem_append] at ha
    rcases ha with h | h
    · rw [List.mem_map] at h
      obtain ⟨a', ha', rfl⟩ := h
      rw [List.sum_cons, ih (t - x) a' ha']; ring
    · rw [List.mem_map] at h
      obtain ⟨a', ha', rfl⟩ := h
      rw [List.sum_cons, ih (t + x) a' ha']; ring

end LC.P0494
