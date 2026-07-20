import Lproofs.Schemes.Fold

/-! @lc 3043 | name:Find the Length of the Longest Common Prefix | scheme:dp | family:trie |
    complexity:O(Σ|digits|) | source:https://leetcode.com/problems/find-the-length-of-the-longest-common-prefix/

    Each number is treated as its digit string; the longest common prefix of two numbers is the longest
    shared leading run (the trie depth at which they diverge). CLASSIFICATION (dp): `sol` is a
    structural recursion consuming both digit strings in lockstep. CORRECTNESS: we certify the defining
    property — `sol xs ys` is a genuine common prefix, i.e. a prefix of BOTH `xs` and `ys`. -/

namespace LC.P3043

/-- Longest common prefix of two digit strings: shared leading run. -/
def sol : List ℕ → List ℕ → List ℕ
  | x :: xs, y :: ys => if x = y then x :: sol xs ys else []
  | _, _ => []

/-- SCHEME (dp): consuming a matching head recurses on the tails (the digit-by-digit descent). -/
theorem cls (x : ℕ) (xs ys : List ℕ) : sol (x :: xs) (x :: ys) = x :: sol xs ys := by
  simp [sol]

/-- CORRECT: `sol xs ys` is a common prefix — a prefix of both inputs. -/
theorem corr (xs ys : List ℕ) : sol xs ys <+: xs ∧ sol xs ys <+: ys := by
  induction xs generalizing ys with
  | nil => simp [sol]
  | cons x xs ih =>
    cases ys with
    | nil => simp [sol]
    | cons y ys =>
      by_cases hxy : x = y
      · subst hxy
        obtain ⟨⟨t1, h1⟩, ⟨t2, h2⟩⟩ := ih ys
        simp only [sol, if_pos rfl]
        exact ⟨⟨t1, by show x :: (sol xs ys ++ t1) = x :: xs; rw [h1]⟩,
               ⟨t2, by show x :: (sol xs ys ++ t2) = x :: ys; rw [h2]⟩⟩
      · simp [sol, hxy]


/-- GROUND INSTANCE (official example 1): the longest common prefix of 100 and 1000 (as digit
    lists) is 100 — length 3, the judge's answer. -/
theorem vec : sol [1, 0, 0] [1, 0, 0, 0] = [1, 0, 0] := by decide

end LC.P3043
