import Lproofs.Schemes.Fold

/-! @lc 139 | name:Word Break | scheme:dp | family:dp-string | complexity:O(n²) |
    source:https://leetcode.com/problems/word-break/

    Decide whether a string splits into a sequence of dictionary words. CLASSIFICATION: the accepted DP
    is a recursive decomposition — at each position, try every dictionary word that is a prefix and
    recurse on the remaining suffix (`cls`). NON-VACUITY: we enumerate the genuine segmentations and
    prove soundness — every enumerated segmentation concatenates back to the original string (`corr`),
    so the decomposition explores real splits, not a re-encoding. We certify the decomposition +
    soundness; DROP the boolean feasibility answer. -/

namespace LC.P0139

/-- Strip word `w` off the front of `s`; `some rest` iff `w ++ rest = s` (i.e. `w` is a prefix). -/
def strip : List Char → List Char → Option (List Char)
  | [], s => some s
  | _ :: _, [] => none
  | a :: w, b :: s => if a = b then strip w s else none

/-- `strip` returns a genuine suffix: if it succeeds, the word prepended to it is the original. -/
theorem strip_sound : ∀ (w s rest : List Char), strip w s = some rest → w ++ rest = s
  | [], s, rest, h => by simp only [strip, Option.some.injEq] at h; rw [← h]; rfl
  | _ :: _, [], rest, h => by simp [strip] at h
  | a :: w, b :: s, rest, h => by
    simp only [strip] at h
    split at h
    · rename_i hab; subst hab
      rw [List.cons_append, strip_sound w s rest h]
    · exact absurd h (by simp)

/-- Enumerate segmentations of `s` into dictionary words (fuel bounds the recursion). -/
def sol (dict : List (List Char)) : ℕ → List Char → List (List (List Char))
  | 0, _ => []
  | _ + 1, [] => [[]]
  | fuel + 1, s =>
      dict.flatMap (fun w =>
        match strip w s with
        | some rest => (sol dict fuel rest).map (w :: ·)
        | none => [])

/-- SCHEME (dp / recursive decomposition): for a nonempty string, try each dictionary word as a
    prefix and recurse on the remaining suffix — the genuine subproblem decomposition. -/
theorem cls (dict : List (List Char)) (fuel : ℕ) (c : Char) (s : List Char) :
    sol dict (fuel + 1) (c :: s) =
      dict.flatMap (fun w =>
        match strip w (c :: s) with
        | some rest => (sol dict fuel rest).map (w :: ·)
        | none => []) :=
  rfl

/-- NON-VACUITY (soundness): every enumerated segmentation concatenates back to the original string. -/
theorem corr : ∀ (dict : List (List Char)) (fuel : ℕ) (s : List Char) (seg : List (List Char)),
    seg ∈ sol dict fuel s → seg.flatten = s := by
  intro dict fuel
  induction fuel with
  | zero => intro s seg h; simp [sol] at h
  | succ f ih =>
    intro s seg h
    match s with
    | [] => simp only [sol, List.mem_singleton] at h; subst h; rfl
    | c :: s' =>
      rw [cls, List.mem_flatMap] at h
      obtain ⟨w, _, hw⟩ := h
      split at hw
      · rename_i rest hstrip
        rw [List.mem_map] at hw
        obtain ⟨seg', hseg', rfl⟩ := hw
        rw [List.flatten_cons, ih rest seg' hseg', strip_sound w (c :: s') rest hstrip]
      · simp at hw


/-- GROUND INSTANCE (official example 1): "leetcode" segments over dict ["leet","code"] in
    exactly one way — the judge's yes-instance, with the witness enumerated. -/
theorem vec : sol ["leet".toList, "code".toList] 8 "leetcode".toList =
    [["leet".toList, "code".toList]] := by decide

end LC.P0139
