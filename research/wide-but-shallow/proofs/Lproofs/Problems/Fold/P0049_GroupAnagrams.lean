import Lproofs.Schemes.Fold

/-! @lc 49 | name:Group Anagrams | scheme:fold | family:grouping | complexity:O(n·k) |
    source:https://leetcode.com/problems/group-anagrams/

    Group strings that are anagrams (equal sorted-character keys). CLASSIFICATION: the grouping is a
    streaming left fold over the strings whose accumulator is the bucket list keyed by anagram key —
    each string is appended to its key's bucket (creating one if absent). NON-VACUITY: we prove the
    grouping loses nothing — the total count across all buckets equals the number of strings processed —
    so the accumulator does genuine bucketing work, not a re-encoding. We certify the fold + the
    count-conservation invariant. -/

namespace LC.P0049
open Interview.Patterns

variable {K V : Type*} [DecidableEq K]

/-- Append `v` to the bucket for key `k`, creating the bucket if it does not yet exist. -/
def bucketInsert : List (K × List V) → K → V → List (K × List V)
  | [], k, v => [(k, [v])]
  | (k', vs) :: rest, k, v =>
      if k' = k then (k', v :: vs) :: rest else (k', vs) :: bucketInsert rest k v

/-- Total number of values across all buckets. -/
def totalCount (g : List (K × List V)) : ℕ := (g.map (fun p => p.2.length)).sum

/-- Inserting one value increases the total bucket count by exactly one. -/
theorem insert_count (g : List (K × List V)) (k : K) (v : V) :
    totalCount (bucketInsert g k v) = totalCount g + 1 := by
  induction g with
  | nil => simp [bucketInsert, totalCount]
  | cons p rest ih =>
    obtain ⟨k', vs⟩ := p
    simp only [bucketInsert]
    split
    · simp only [totalCount, List.map_cons, List.sum_cons, List.length_cons]; omega
    · simp only [totalCount, List.map_cons, List.sum_cons] at ih ⊢
      omega

/-- Group strings by anagram key via a streaming fold. -/
def group (key : V → K) (vs : List V) : List (K × List V) :=
  vs.foldl (fun g v => bucketInsert g (key v) v) []

/-- SCHEME (fold): the grouping is a left fold with the keyed-bucket accumulator. -/
theorem cls (key : V → K) :
    IsFold (fun vs : List V => vs.foldl (fun g v => bucketInsert g (key v) v) []) :=
  ⟨(fun g v => bucketInsert g (key v) v), [], fun _ => rfl⟩

/-- NON-VACUITY: every string lands in exactly one bucket — the total bucket count equals the input
    size, so the grouping loses nothing. -/
theorem corr (key : V → K) (vs : List V) : totalCount (group key vs) = vs.length := by
  have step : ∀ (xs : List V) g,
      totalCount (xs.foldl (fun g v => bucketInsert g (key v) v) g) = totalCount g + xs.length := by
    intro xs
    induction xs with
    | nil => intro g; simp
    | cons x rest ih =>
      intro g
      simp only [List.foldl_cons, List.length_cons]
      rw [ih (bucketInsert g (key x) x), insert_count]
      omega
  have := step vs []
  simpa [group, totalCount] using this

end LC.P0049
