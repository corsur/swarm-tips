import Lproofs.Schemes.Fold

/-! @lc 17 | name:Letter Combinations of a Phone Number | scheme:dp | family:enumeration |
    complexity:O(4ⁿ) | source:https://leetcode.com/problems/letter-combinations-of-a-phone-number/

    Enumerate every string formed by choosing one letter per digit. CLASSIFICATION: the accepted
    backtracking is a recursive decomposition — for the first digit, pick each of its letters and
    prepend it to every combination of the remaining digits (`cls`). NON-VACUITY: we prove soundness —
    every enumerated combination has exactly one character per digit (`corr`) — so the decomposition
    builds genuine per-digit choices, not a re-encoding. We certify the decomposition + the length
    invariant; DROP any counting claim. -/

namespace LC.P0017

/-- Combinations: choose one of `letters d` for the head digit, prepend to each tail-combination. -/
def combos (letters : Char → List Char) : List Char → List (List Char)
  | [] => [[]]
  | d :: ds => (letters d).flatMap (fun c => (combos letters ds).map (c :: ·))

/-- SCHEME (dp / recursive decomposition): combinations of `d :: ds` pick each letter of `d` and
    prepend it to every combination of `ds`. The genuine per-digit choice decomposition. -/
theorem cls (letters : Char → List Char) (d : Char) (ds : List Char) :
    combos letters (d :: ds) = (letters d).flatMap (fun c => (combos letters ds).map (c :: ·)) := rfl

/-- NON-VACUITY (soundness): every enumerated combination has exactly one character per digit. -/
theorem corr : ∀ (letters : Char → List Char) (ds : List Char) (combo : List Char),
    combo ∈ combos letters ds → combo.length = ds.length := by
  intro letters ds
  induction ds with
  | nil => intro combo h; simp only [combos, List.mem_singleton] at h; subst h; rfl
  | cons d ds ih =>
    intro combo h
    rw [cls, List.mem_flatMap] at h
    obtain ⟨c, _, hc⟩ := h
    rw [List.mem_map] at hc
    obtain ⟨combo', hcombo', rfl⟩ := hc
    simp only [List.length_cons, ih combo' hcombo']

end LC.P0017
