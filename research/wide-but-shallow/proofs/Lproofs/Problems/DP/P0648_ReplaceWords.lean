import Lproofs.Problems.DP.P0208_ImplementTrie

/-! @lc 648 | name:Replace Words | scheme:dp | family:trie | complexity:O(Σ|word|) |
    source:https://leetcode.com/problems/replace-words/

    Given a dictionary of roots (a trie) and a sentence, replace each word by the shortest root that
    is a prefix of it. CLASSIFICATION: the per-word lookup is a recursive decomposition descending the
    trie one character at a time, stopping at the first node that ends a root. NON-VACUITY: we prove
    the returned prefix is genuinely a root of the trie (`contains`) and an actual prefix of the word.
    We certify the decomposition + soundness, not the sentence-level rewrite. -/

namespace LC.P0648
open LC.P0208 (Trie)

/-- `LC.P0208.contains` was renamed to its gate-designated name `sol`; keep the local name. -/
abbrev contains : Trie → List ℕ → Bool := LC.P0208.sol

/-- Walk the trie along `w`, returning the matched prefix at the first root-ending node. -/
def findRoot : Trie → List ℕ → Option (List ℕ)
  | .node true _, _ => some []
  | .node false _, [] => none
  | .node false ch, x :: xs =>
    match ch x with
    | some t' => (findRoot t' xs).map (x :: ·)
    | none => none

/-- SCHEME (dp / recursive decomposition): the lookup descends one character into the trie and
    recurses on the rest of the word, prepending the consumed character to the matched prefix. -/
theorem cls (ch : ℕ → Option Trie) (x : ℕ) (xs : List ℕ) :
    findRoot (.node false ch) (x :: xs) =
      (match ch x with
       | some t' => (findRoot t' xs).map (x :: ·)
       | none => none) :=
  rfl

/-- NON-VACUITY (soundness): a returned prefix is a genuine root of the trie and a prefix of `w`. -/
theorem corr : ∀ (w : List ℕ) (t : Trie) (p : List ℕ),
    findRoot t w = some p → contains t p = true ∧ p <+: w := by
  intro w
  induction w with
  | nil =>
    intro t p hfr
    cases t with
    | node b ch =>
      cases b with
      | true => simp only [findRoot, Option.some.injEq] at hfr; subst hfr; exact ⟨rfl, List.nil_prefix⟩
      | false => simp [findRoot] at hfr
  | cons x xs ih =>
    intro t p hfr
    cases t with
    | node b ch =>
      cases b with
      | true => simp only [findRoot, Option.some.injEq] at hfr; subst hfr; exact ⟨rfl, List.nil_prefix⟩
      | false =>
        simp only [findRoot] at hfr
        cases hcx : ch x with
        | none => simp [hcx] at hfr
        | some t' =>
          simp only [hcx] at hfr
          cases hfind : findRoot t' xs with
          | none => rw [hfind] at hfr; simp at hfr
          | some p' =>
            rw [hfind] at hfr
            simp only [Option.map_some, Option.some.injEq] at hfr
            subst hfr
            obtain ⟨hcont, hpre⟩ := ih t' p' hfind
            obtain ⟨s, hs⟩ := hpre
            exact ⟨by simp only [contains, LC.P0208.sol, hcx]; exact hcont, ⟨s, by rw [List.cons_append, hs]⟩⟩

end LC.P0648
