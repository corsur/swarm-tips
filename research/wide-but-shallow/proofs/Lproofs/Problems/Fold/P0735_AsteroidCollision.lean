import Lproofs.Schemes.Fold

/-! @lc 735 | name:Asteroid Collision | scheme:fold | family:pairing-stack | complexity:O(n) |
    source:https://leetcode.com/problems/asteroid-collision/

    Asteroids stream left to right; a right-mover (`> 0`) followed by a left-mover (`< 0`) collide,
    the smaller exploding. A stack resolves this in one pass. Correctness: the surviving stack is
    *stable* — no left-mover sits immediately above a right-mover — which is exactly "no two
    surviving asteroids can still collide". (Stack top = most recently survived; reading order is
    its reverse.) -/

namespace LC.P0735
open Interview.Patterns

/-- Resolve one asteroid `a` against the stack (top first). -/
def collide : List ℤ → ℤ → List ℤ
  | top :: rest, a =>
    if 0 < top ∧ a < 0 then
      if top < -a then collide rest a
      else if top = -a then rest
      else top :: rest
    else a :: top :: rest
  | [], a => [a]

/-- Stream all asteroids through the stack. -/
def run (xs : List ℤ) : List ℤ := xs.foldl collide []

/-- Stable stack: no left-mover (`< 0`) directly above a right-mover (`> 0`). -/
def StableRun : List ℤ → Prop
  | [] => True
  | [_] => True
  | x :: y :: rest => ¬(x < 0 ∧ y > 0) ∧ StableRun (y :: rest)

/-- The survivor stack returned to the caller (reading order is its reverse). -/
def sol (xs : List ℤ) : List ℤ := run xs

/-- SCHEME (fold): the collision resolution is a single streaming pass (a left fold). -/
theorem cls : IsFold (fun xs : List ℤ => xs.foldl collide []) :=
  ⟨_, _, fun _ => rfl⟩

theorem stableRun_tail {x : ℤ} {rest : List ℤ} (h : StableRun (x :: rest)) : StableRun rest := by
  match rest with
  | [] => exact trivial
  | _ :: _ => exact h.2

/-- One collision step preserves stability. -/
theorem collide_stable : ∀ (st : List ℤ) (a : ℤ), StableRun st → StableRun (collide st a) := by
  intro st
  induction st with
  | nil => intro a _; exact trivial
  | cons top rest ih =>
    intro a h
    simp only [collide]
    by_cases hcol : 0 < top ∧ a < 0
    · simp only [if_pos hcol]
      by_cases h1 : top < -a
      · simp only [if_pos h1]; exact ih a (stableRun_tail h)
      · simp only [if_neg h1]
        by_cases h2 : top = -a
        · simp only [if_pos h2]; exact stableRun_tail h
        · simp only [if_neg h2]; exact h
    · simp only [if_neg hcol]
      refine ⟨?_, h⟩
      rintro ⟨ha, htop⟩
      exact hcol ⟨htop, ha⟩

/-- CORRECT: the surviving stack is stable — no left-mover sits above a right-mover, i.e. no two
    survivors can still collide. -/
theorem corr (xs : List ℤ) : StableRun (sol xs) := by
  unfold sol run
  have key : ∀ (ys : List ℤ) st, StableRun st → StableRun (ys.foldl collide st) := by
    intro ys
    induction ys with
    | nil => intro st h; exact h
    | cons a rest ih => intro st h; exact ih _ (collide_stable st a h)
  exact key xs [] trivial

end LC.P0735
