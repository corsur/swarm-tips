import Lproofs.Problems.Fold.P0151_ReverseWords

/-! @lc 824 | name:Goat Latin | scheme:fold | family:string-other | complexity:O(n) |
    source:https://leetcode.com/problems/goat-latin/

    Transform each word independently (vowel-initial words keep their order, others rotate the first
    letter to the end, then every word gains a `"ma"` suffix plus index-many `'a'`s) and rejoin.
    Correctness: the transform is per-word and never introduces spaces, so the output has exactly as
    many words as the input. Reuses the verified word-splitter from `LC.P0151`. -/

namespace LC.P0824
open Interview.Patterns

def isVowel (c : Char) : Bool := c ∈ ['a', 'e', 'i', 'o', 'u', 'A', 'E', 'I', 'O', 'U']

/-- Goat-Latin transform of the `i`-th word. -/
def transform (w : List Char) (i : ℕ) : List Char :=
  (if w.head?.elim false isVowel then w else w.drop 1 ++ w.take 1) ++
    ('m' :: 'a' :: List.replicate (i + 1) 'a')

def goatLatin (s : List Char) : List Char :=
  LC.P0151.joinSp ((LC.P0151.words s).zipIdx.map (fun p => transform p.1 p.2))

def sol (s : List Char) : List Char := goatLatin s

/-- SCHEME (fold): the splitting pass is a streaming left fold over the characters. -/
theorem cls : IsFold (fun s : List Char =>
    s.foldl (fun (acc : List (List Char)) c => if c = ' ' then ([] : List Char) :: acc else acc)
      ([] : List (List Char))) :=
  ⟨_, _, fun _ => rfl⟩

/-- Each transformed word is nonempty and space-free. -/
theorem transform_props (w : List Char) (hw : ' ' ∉ w) (i : ℕ) :
    transform w i ≠ [] ∧ ' ' ∉ transform w i := by
  refine ⟨?_, ?_⟩
  · have : 'm' ∈ transform w i := by
      simp only [transform]; exact List.mem_append_right _ (List.mem_cons_self ..)
    exact List.ne_nil_of_mem this
  · simp only [transform, List.mem_append, not_or]
    refine ⟨?_, ?_⟩
    · split
      · exact hw
      · simp only [List.mem_append, not_or]
        exact ⟨fun h => hw (List.mem_of_mem_drop h), fun h => hw (List.mem_of_mem_take h)⟩
    · simp [List.mem_replicate]

/-- CORRECT: the output has exactly as many words as the input. -/
theorem corr (s : List Char) :
    (LC.P0151.words (sol s)).length = (LC.P0151.words s).length := by
  unfold sol goatLatin
  rw [LC.P0151.words_joinSp]
  · rw [List.length_map, List.length_zipIdx]
  · intro x hx
    rw [List.mem_map] at hx
    obtain ⟨p, hp, rfl⟩ := hx
    have hfst : p.1 ∈ LC.P0151.words s := by
      have := List.mem_map_of_mem (f := Prod.fst) hp
      rwa [List.zipIdx_map_fst] at this
    have hns : ' ' ∉ p.1 :=
      LC.P0151.splitSp_no_space s p.1 (List.mem_of_mem_filter hfst)
    exact transform_props p.1 hns p.2

end LC.P0824
