import Lproofs.Schemes.Fold

/-! @lc 161 | name:One Edit Distance | scheme:fold | family:two-pointer | complexity:O(n) |
    source:https://leetcode.com/problems/one-edit-distance/

    Decide whether two strings are exactly one edit apart; the accepted scan first consumes their common
    prefix, then checks the single remaining difference. CLASSIFICATION: the common-prefix consumption is
    a streaming pairwise fold whose accumulator is the running common prefix. NON-VACUITY: we prove the
    per-step `lcp` is a genuine common prefix of both arguments (a prefix of each), so the scan advances
    over real shared characters before the one-edit check. We certify the fold + the common-prefix
    property; DROP the final edit-type decision. -/

namespace LC.P0161
open Interview.Patterns

/-- Longest common prefix of two strings (the prefix the scan consumes before the edit check). -/
def lcp : List Char → List Char → List Char
  | a :: as, b :: bs => if a = b then a :: lcp as bs else []
  | _, _ => []

/-- SCHEME (fold): the common-prefix scan over the two strings is a streaming left fold. -/
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

/-- NON-VACUITY: the consumed prefix is a genuine common prefix of both strings. -/
theorem corr (a b : List Char) : lcp a b <+: a ∧ lcp a b <+: b :=
  ⟨lcp_pre_l a b, lcp_pre_r a b⟩

end LC.P0161
