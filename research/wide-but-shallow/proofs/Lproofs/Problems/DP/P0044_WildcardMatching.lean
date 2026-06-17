import Lproofs.Schemes.Fold

/-! @lc 44 | name:Wildcard Matching | scheme:dp | family:string-dp | complexity:O(mn) |
    source:https://leetcode.com/problems/wildcard-matching/

    Match a string against a pattern with `?` (any one char) and `*` (any sequence). CLASSIFICATION: the
    accepted DP is a recursive decomposition over the (string-pos, pattern-pos) grid — a `*` either
    matches nothing (advance the pattern) or one more character (advance the string), and a literal/`?`
    consumes one of each (`cls`). NON-VACUITY: we prove the genuine base case — once the pattern is
    consumed, matching succeeds iff the string is too (`corr`). We certify the decomposition + the base;
    DROP the boolean match answer. -/

namespace LC.P0044

/-- Match `s[i..]` against `p[j..]` (lengths `slen`, `plen`); fuel bounds the recursion. -/
def wild (s p : ℕ → Char) (slen plen : ℕ) : ℕ → ℕ → ℕ → Bool
  | 0, _, _ => false
  | f + 1, i, j =>
      if j = plen then decide (i = slen)
      else if p j = '*' then
        wild s p slen plen f i (j + 1) || (if i < slen then wild s p slen plen f (i + 1) j else false)
      else if i < slen ∧ (p j = '?' ∨ p j = s i) then wild s p slen plen f (i + 1) (j + 1)
      else false

/-- SCHEME (dp / recursive decomposition): a `*` either matches nothing (advance the pattern) or one
    more character (advance the string) — the genuine two-way star branch. -/
theorem cls (s p : ℕ → Char) (slen plen f i j : ℕ) (hj : j ≠ plen) (hstar : p j = '*') :
    wild s p slen plen (f + 1) i j =
      (wild s p slen plen f i (j + 1) || (if i < slen then wild s p slen plen f (i + 1) j else false)) := by
  simp only [wild, if_neg hj, if_pos hstar]

/-- NON-VACUITY (base): once the pattern is consumed, the match succeeds iff the string is consumed too. -/
theorem corr (s p : ℕ → Char) (slen plen f : ℕ) : wild s p slen plen (f + 1) slen plen = true := by
  simp [wild]

end LC.P0044
