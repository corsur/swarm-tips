import Lproofs.Schemes.Fold

/-! @lc 1657 | name:Determine if Two Strings Are Close | scheme:fold | family:hashing | complexity:O(n) |
    source:https://leetcode.com/problems/determine-if-two-strings-are-close/ -/

namespace LC.P1657
open Interview.Patterns

/-- Spec: two strings are "close" iff they have the same set of distinct characters and the same
    multiset of character frequencies (swaps permute positions; transforms permute which letter has
    which frequency). -/
def spec (a b : List Char) : Prop :=
  a.toFinset = b.toFinset ∧ a.toFinset.val.map a.count = b.toFinset.val.map b.count

/-- Editorial frequency solution: compare distinct-character sets and frequency multisets (both
    built by a streaming fold). -/
def sol (a b : List Char) : Bool :=
  decide (a.toFinset = b.toFinset ∧ a.toFinset.val.map a.count = b.toFinset.val.map b.count)

/-- The distinct-character set `sol` compares is itself built by the streaming seen-set fold. -/
theorem toFinset_fold (xs : List Char) :
    xs.foldl (fun s c => insert c s) (∅ : Finset Char) = xs.toFinset := by
  have h : ∀ init : Finset Char,
      xs.foldl (fun s c => insert c s) init = init ∪ xs.toFinset := by
    intro init
    induction xs generalizing init with
    | nil => simp
    | cons y ys ih =>
      rw [List.foldl_cons, ih, List.toFinset_cons]
      ext a
      simp only [Finset.mem_union, Finset.mem_insert]
      tauto
  simpa using h ∅

/-- SCHEME (fold): the character frequencies are a streaming fold, and `sol` compares exactly the
    data that fold family computes — the seen-set fold builds the distinct-character sets `sol`
    reads. -/
theorem cls : IsFold (fun xs : List Char => xs.foldl (fun m c => insert c m) (0 : Multiset Char)) ∧
    ∀ a b : List Char, sol a b = decide
      ((a.foldl (fun s c => insert c s) (∅ : Finset Char)) =
        b.foldl (fun s c => insert c s) ∅ ∧
       (a.foldl (fun s c => insert c s) (∅ : Finset Char)).val.map a.count =
        (b.foldl (fun s c => insert c s) (∅ : Finset Char)).val.map b.count) := by
  refine ⟨fold_charCount, fun a b => ?_⟩
  simp only [sol]
  rw [decide_eq_decide, toFinset_fold a, toFinset_fold b]

/-- CORRECT: the boolean answer matches the closeness predicate. -/
theorem corr (a b : List Char) : sol a b = true ↔ spec a b := by
  simp [sol, spec]

/-- GROUND INSTANCE (official example 2): "abc" and "bca" are close (same letters, same
    frequency multiset). -/
theorem vec : sol "abc".toList "bca".toList = true := by decide

end LC.P1657
