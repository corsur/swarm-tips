import Lproofs.Schemes.Fold

/-! @lc 14 | name:Longest Common Prefix | scheme:fold | family:string | complexity:O(Σ|s|) |
    source:https://leetcode.com/problems/longest-common-prefix/

    Fold the pairwise longest-common-prefix across the strings. CLASSIFICATION: a streaming left fold
    whose accumulator is the running common prefix. NON-VACUITY: we prove the per-step `lcp` is a
    genuine common prefix of its two arguments (a prefix of each), so the folded result is built from
    real common prefixes. We certify the fold + the common-prefix property. -/

namespace LC.P0014
open Interview.Patterns

/-- Longest common prefix of two strings. -/
def lcp : List Char → List Char → List Char
  | a :: as, b :: bs => if a = b then a :: lcp as bs else []
  | _, _ => []

/-- Editorial: fold `lcp` across the strings, seeded with the first. -/
def sol : List (List Char) → List Char
  | [] => []
  | s :: rest => rest.foldl lcp s

/-- SCHEME (fold): the running common prefix is a streaming left fold across the strings. -/
theorem cls (s : List Char) : IsFold (fun rest : List (List Char) => rest.foldl lcp s) :=
  ⟨lcp, s, fun _ => rfl⟩

/-- `lcp a b` is a prefix of `a`. -/
theorem lcp_pre_l : ∀ (a b : List Char), lcp a b <+: a
  | [], _ => by simp [lcp]
  | _ :: _, [] => by simp [lcp]
  | a :: as, b :: bs => by
    simp only [lcp]
    split
    · rename_i h; subst h
      obtain ⟨t, ht⟩ := lcp_pre_l as bs
      exact ⟨t, by rw [List.cons_append, ht]⟩
    · exact List.nil_prefix

/-- `lcp a b` is a prefix of `b`. -/
theorem lcp_pre_r : ∀ (a b : List Char), lcp a b <+: b
  | [], _ => by simp [lcp]
  | _ :: _, [] => by simp [lcp]
  | a :: as, b :: bs => by
    simp only [lcp]
    split
    · rename_i h; subst h
      obtain ⟨t, ht⟩ := lcp_pre_r as bs
      exact ⟨t, by rw [List.cons_append, ht]⟩
    · exact List.nil_prefix

/-- NON-VACUITY: each `lcp` step yields a genuine common prefix of its two arguments. -/
theorem corr (a b : List Char) : lcp a b <+: a ∧ lcp a b <+: b :=
  ⟨lcp_pre_l a b, lcp_pre_r a b⟩

end LC.P0014
