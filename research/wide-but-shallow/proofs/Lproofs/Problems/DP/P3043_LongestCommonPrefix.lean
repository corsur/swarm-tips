import Lproofs.Schemes.Fold

/-! @lc 3043 | name:Find the Length of the Longest Common Prefix | scheme:dp | family:trie |
    complexity:O(Σ|digits|) | source:https://leetcode.com/problems/find-the-length-of-the-longest-common-prefix/

    Each number is treated as its digit string; the longest common prefix of two numbers is the longest
    shared leading run (the trie depth at which they diverge). CLASSIFICATION (dp): `lcp` is a
    structural recursion consuming both digit strings in lockstep. CORRECTNESS: we certify the defining
    property — `lcp xs ys` is a genuine common prefix, i.e. a prefix of BOTH `xs` and `ys`. -/

namespace LC.P3043

/-- Longest common prefix of two digit strings: shared leading run. -/
def lcp : List ℕ → List ℕ → List ℕ
  | x :: xs, y :: ys => if x = y then x :: lcp xs ys else []
  | _, _ => []

/-- SCHEME (dp): consuming a matching head recurses on the tails (the digit-by-digit descent). -/
theorem cls (x : ℕ) (xs ys : List ℕ) : lcp (x :: xs) (x :: ys) = x :: lcp xs ys := by
  simp [lcp]

/-- CORRECT: `lcp xs ys` is a common prefix — a prefix of both inputs. -/
theorem corr (xs ys : List ℕ) : lcp xs ys <+: xs ∧ lcp xs ys <+: ys := by
  induction xs generalizing ys with
  | nil => simp [lcp]
  | cons x xs ih =>
    cases ys with
    | nil => simp [lcp]
    | cons y ys =>
      by_cases hxy : x = y
      · subst hxy
        obtain ⟨⟨t1, h1⟩, ⟨t2, h2⟩⟩ := ih ys
        simp only [lcp, if_pos rfl]
        exact ⟨⟨t1, by show x :: (lcp xs ys ++ t1) = x :: xs; rw [h1]⟩,
               ⟨t2, by show x :: (lcp xs ys ++ t2) = x :: ys; rw [h2]⟩⟩
      · simp [lcp, hxy]

end LC.P3043
