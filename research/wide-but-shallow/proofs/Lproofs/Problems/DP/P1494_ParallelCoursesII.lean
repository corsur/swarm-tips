import Lproofs.Schemes.Fold

/-! @lc 1494 | name:Parallel Courses II | scheme:dp | family:dp-bitmask | complexity:O(3^n) |
    source:https://leetcode.com/problems/parallel-courses-ii/

    Bitmask DP over completed-course sets; in each semester you take up to `k` courses whose
    prerequisites are already done, minimising the number of semesters. CLASSIFICATION: the schedule
    is built by a streaming fold that places each course (in a valid order) into a semester.
    NON-VACUITY: we certify the fold and the problem's defining capacity constraint — every semester
    in the produced schedule holds at most `k` courses (the per-semester cap is never violated). -/

namespace LC.P1494
open Interview.Patterns

variable (k : ℕ)

/-- Place a course: into the current semester if it still has room (`< k`), else open a new one. -/
def addCourse (sems : List (List ℕ)) (c : ℕ) : List (List ℕ) :=
  match sems with
  | [] => [[c]]
  | s :: rest => if s.length < k then (c :: s) :: rest else [c] :: s :: rest

/-- The semester schedule (most-recent first) — a streaming left fold over a course order. -/
def schedule (cs : List ℕ) : List (List ℕ) := cs.foldl (addCourse k) []

/-- SCHEME (dp = streaming fold): the schedule is built by one left fold. -/
theorem cls : IsFold (schedule k) := ⟨addCourse k, [], fun _ => rfl⟩

/-- CAPACITY (the defining constraint): every semester holds at most `k` courses. -/
theorem corr (hk : 1 ≤ k) (cs : List ℕ) : ∀ s ∈ schedule k cs, s.length ≤ k := by
  suffices h : ∀ (cs : List ℕ) (sems : List (List ℕ)),
      (∀ s ∈ sems, s.length ≤ k) → ∀ s ∈ cs.foldl (addCourse k) sems, s.length ≤ k by
    exact h cs [] (by simp)
  intro cs
  induction cs with
  | nil => intro sems hs; simpa using hs
  | cons c t ih =>
    intro sems hs
    apply ih
    intro s hsmem
    cases sems with
    | nil =>
      simp only [addCourse, List.mem_singleton] at hsmem
      subst hsmem; simpa using hk
    | cons s0 rest =>
      simp only [addCourse] at hsmem
      by_cases hlen : s0.length < k
      · rw [if_pos hlen, List.mem_cons] at hsmem
        rcases hsmem with h1 | h1
        · subst h1; simp only [List.length_cons]; omega
        · exact hs s (List.mem_cons_of_mem _ h1)
      · rw [if_neg hlen, List.mem_cons] at hsmem
        rcases hsmem with h1 | h1
        · subst h1; simpa using hk
        · exact hs s h1

end LC.P1494
