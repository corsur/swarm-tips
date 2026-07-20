import Lproofs.Schemes.Fold

/-! @lc 1614 | name:Maximum Nesting Depth of the Parentheses | scheme:fold | family:pairing-stack |
    complexity:O(n) | source:https://leetcode.com/problems/maximum-nesting-depth-of-the-parentheses/ -/

namespace LC.P1614
open Interview.Patterns

/-- `+1` for `(`, `-1` for `)`, `0` otherwise. -/
def d (c : Char) : ℤ := if c = '(' then 1 else if c = ')' then -1 else 0

/-- One pass carrying (running balance, max balance seen). -/
def step (a : ℤ × ℤ) (c : Char) : ℤ × ℤ := (a.1 + d c, max a.2 (a.1 + d c))

/-- Editorial: the max nesting depth is the maximum running balance. -/
def sol (s : List Char) : ℤ := (s.foldl step (0, 0)).2

/-- The maximum running balance over all prefixes (recursive form from a carried `(bal, max)`). -/
def peak : ℤ → ℤ → List Char → ℤ
  | _, mx, [] => mx
  | bal, mx, c :: cs => peak (bal + d c) (max mx (bal + d c)) cs

/-- Spec: the maximum prefix balance (the deepest nesting reached). -/
def spec (s : List Char) : ℤ := peak 0 0 s

/-- SCHEME (fold): the depth is computed by a single streaming fold. -/
theorem cls : IsFold (fun s : List Char => s.foldl step (0, 0)) ∧
    ∀ s : List Char, sol s = (s.foldl step (0, 0)).2 :=
  ⟨⟨step, (0, 0), fun _ => rfl⟩, fun _ => rfl⟩

/-- CORRECT: the carried max equals the maximum prefix balance. -/
theorem corr (s : List Char) : sol s = spec s := by
  unfold sol spec
  suffices h : ∀ (cs : List Char) (b m : ℤ), (cs.foldl step (b, m)).2 = peak b m cs from h s 0 0
  intro cs
  induction cs with
  | nil => intro b m; rfl
  | cons c t ih =>
    intro b m
    simp only [List.foldl_cons, step, peak]
    exact ih (b + d c) (max m (b + d c))


/-- GROUND INSTANCE (official example 1): "(1+(2*3)+((8)/4))+1" has max nesting depth 3. -/
theorem vec : sol "(1+(2*3)+((8)/4))+1".toList = 3 := by decide

end LC.P1614
