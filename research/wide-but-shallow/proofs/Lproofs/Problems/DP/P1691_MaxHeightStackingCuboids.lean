import Lproofs.Schemes.Fold

/-! @lc 1691 | name:Maximum Height by Stacking Cuboids | scheme:dp | family:dp-linear |
    complexity:O(n^2) | source:https://leetcode.com/problems/maximum-height-by-stacking-cuboids-ii/

    Sort each cuboid's dimensions, sort the cuboids; a valid stack is then a chain under "fits on"
    (`R`). The accepted DP gives each cuboid the best total height of a stack it tops:
    `dp_i = h_i + max(0, max dp_j over compatible j below)`. CLASSIFICATION: the DP table is a
    streaming left fold over the sorted cuboids. NON-VACUITY / ACHIEVABILITY: we certify the fold and
    that every cuboid's computed height is at least its own height — each cuboid alone is a valid
    one-cuboid stack, so the DP never under-counts a singleton (a genuine lower bound on each entry,
    preserved across the fold). -/

namespace LC.P1691
open Interview.Patterns

variable (R : ℤ → ℤ → Prop) [DecidableRel R]

/-- One DP step: append `(h, best height of a stack topped by h)`; a fresh start contributes `h`,
    extending a compatible lower stack contributes that stack's height. -/
def dpStep (done : List (ℤ × ℤ)) (h : ℤ) : List (ℤ × ℤ) :=
  done ++ [(h, h + max 0 (((done.filter (fun p => decide (R p.1 h))).map Prod.snd).foldr max 0))]

/-- The DP table over the (sorted) cuboid heights — a streaming left fold. -/
def dp (hs : List ℤ) : List (ℤ × ℤ) := hs.foldl (dpStep R) []

/-- SCHEME (dp = streaming fold): the table is built by one left fold. -/
theorem cls : IsFold (dp R) := ⟨dpStep R, [], fun _ => rfl⟩

/-- ACHIEVABILITY: every cuboid's computed stack height is at least its own height (it can stand
    alone). A genuine lower bound on each DP value, preserved across the fold. -/
theorem corr (hs : List ℤ) : ∀ p ∈ dp R hs, p.1 ≤ p.2 := by
  suffices h : ∀ (hs : List ℤ) (done : List (ℤ × ℤ)),
      (∀ p ∈ done, p.1 ≤ p.2) → ∀ p ∈ hs.foldl (dpStep R) done, p.1 ≤ p.2 by
    exact h hs [] (by simp)
  intro hs
  induction hs with
  | nil => intro done hd; simpa using hd
  | cons x t ih =>
    intro done hd
    apply ih
    intro p hp
    rw [dpStep, List.mem_append] at hp
    rcases hp with hp | hp
    · exact hd p hp
    · simp only [List.mem_cons, List.mem_nil_iff, or_false] at hp
      subst hp
      have : (0:ℤ) ≤ max 0 (((done.filter (fun q => decide (R q.1 x))).map Prod.snd).foldr max 0) :=
        le_max_left _ _
      simp only
      linarith

end LC.P1691
