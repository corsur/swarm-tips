import Lproofs.Schemes.Fold

/-! @lc 71 | name:Simplify Path | scheme:fold | family:pairing-stack | complexity:O(n) |
    source:https://leetcode.com/problems/simplify-path/

    A Unix path is canonicalised by scanning its `/`-separated components onto a stack: `""` and `"."`
    are skipped, `".."` pops, any real name is pushed. CLASSIFICATION (fold): the canonical stack is a
    streaming left fold over the components. CORRECTNESS: we certify the canonical-form invariant — every
    component of the simplified path is a real name, never `""`, `"."`, or `".."` — so the output is in
    canonical form. -/

namespace LC.P0071

/-- A component is a real name (not empty, `"."`, or `".."`). -/
def valid (s : String) : Prop := s ≠ "" ∧ s ≠ "." ∧ s ≠ ".."

/-- One scan step: skip `""`/`"."`, pop on `".."`, otherwise push the name. -/
def step (stack : List String) (c : String) : List String :=
  if c = "" ∨ c = "." then stack
  else if c = ".." then stack.dropLast
  else stack ++ [c]

/-- Canonical components of the path. -/
def simplify (comps : List String) : List String := comps.foldl step []

/-- SCHEME (fold): the canonical stack is a left fold of the scan step over the components. -/
theorem cls (comps : List String) : simplify comps = comps.foldl step [] := rfl

/-- A scan step preserves the "all components are real names" invariant. -/
theorem step_preserves (stack : List String) (c : String) (h : ∀ s ∈ stack, valid s) :
    ∀ s ∈ step stack c, valid s := by
  unfold step
  by_cases h1 : c = "" ∨ c = "."
  · rw [if_pos h1]; exact h
  · rw [if_neg h1]
    by_cases h2 : c = ".."
    · rw [if_pos h2]; exact fun s hs => h s (List.dropLast_subset stack hs)
    · rw [if_neg h2]; intro s hs
      rw [List.mem_append, List.mem_singleton] at hs
      rcases hs with hs | rfl
      · exact h s hs
      · push_neg at h1; exact ⟨h1.1, h1.2, h2⟩

theorem foldl_preserves (comps : List String) :
    ∀ stack, (∀ s ∈ stack, valid s) → ∀ s ∈ comps.foldl step stack, valid s := by
  induction comps with
  | nil => exact fun _ h => h
  | cons c t ih => exact fun stack h => ih (step stack c) (step_preserves stack c h)

/-- CORRECT: every component of the simplified path is a real name — the canonical-form invariant. -/
theorem corr (comps : List String) : ∀ s ∈ simplify comps, valid s :=
  foldl_preserves comps [] (by simp)

end LC.P0071
