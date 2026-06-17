import Lproofs.Schemes.Fold

/-! @lc 57 | name:Insert Interval | scheme:fold | family:merge-intervals | complexity:O(n) |
    source:https://leetcode.com/problems/insert-interval/

    Insert a new interval into a sorted list of disjoint intervals, merging overlaps. A single pass
    splits into "before", "merged with the new one", and "after". The pass is a genuine right fold
    (catamorphism) over the intervals whose accumulator is a *function* of the pending state
    `Option (ℤ × ℤ)` — `some nv` carries the still-unplaced (and possibly growing) new interval,
    `none` means it has been placed and the remaining intervals pass through unchanged. This is the
    classic "state-threading recursion as a fold" (cf. P0115/P0097). Correctness: the set of points
    covered by the output is exactly the points covered by the input together with the new
    interval — merging changes the representation, not the covered set. -/

namespace LC.P0057
open Interview.Patterns

/-- Insert `new`, merging every interval that overlaps it (the natural early-exiting recursion). -/
def insert : List (ℤ × ℤ) → (ℤ × ℤ) → List (ℤ × ℤ)
  | [], new => [new]
  | (lo, hi) :: rest, (nlo, nhi) =>
    if hi < nlo then (lo, hi) :: insert rest (nlo, nhi)
    else if nhi < lo then (nlo, nhi) :: (lo, hi) :: rest
    else insert rest (min lo nlo, max hi nhi)

/-- The same pass written as a catamorphism: the `Option (ℤ × ℤ)` state is the accumulator.
    `none` = the new interval is placed, so emit the rest unchanged. -/
def insertF : List (ℤ × ℤ) → Option (ℤ × ℤ) → List (ℤ × ℤ)
  | [], st => match st with | none => [] | some nv => [nv]
  | (lo, hi) :: rest, st =>
    match st with
    | none => (lo, hi) :: insertF rest none
    | some (nlo, nhi) =>
      if hi < nlo then (lo, hi) :: insertF rest (some (nlo, nhi))
      else if nhi < lo then (nlo, nhi) :: (lo, hi) :: insertF rest none
      else insertF rest (some (min lo nlo, max hi nhi))

/-- The fold step: one interval, given the recursive result as a state-function. -/
def stepI (I : ℤ × ℤ) (rec : Option (ℤ × ℤ) → List (ℤ × ℤ)) : Option (ℤ × ℤ) → List (ℤ × ℤ) :=
  fun st =>
    match st, I with
    | none, (lo, hi) => (lo, hi) :: rec none
    | some (nlo, nhi), (lo, hi) =>
      if hi < nlo then (lo, hi) :: rec (some (nlo, nhi))
      else if nhi < lo then (nlo, nhi) :: (lo, hi) :: rec none
      else rec (some (min lo nlo, max hi nhi))

/-- The fold base: with the new interval placed, nothing; otherwise it is the whole output. -/
def baseI : Option (ℤ × ℤ) → List (ℤ × ℤ) := fun st => match st with | none => [] | some nv => [nv]

/-- The insertion returned to the caller. -/
def sol (ints : List (ℤ × ℤ)) (new : ℤ × ℤ) : List (ℤ × ℤ) := insert ints new

/-- A point `p` lies in some interval of the list. -/
def covered (ints : List (ℤ × ℤ)) (p : ℤ) : Prop := ∃ I ∈ ints, I.1 ≤ p ∧ p ≤ I.2

/-- SCHEME (fold = catamorphism): `insertF` is a genuine right fold over the intervals, carrying the
    pending-interval state `Option (ℤ × ℤ) → List (ℤ × ℤ)` as its accumulator — proven, not
    assumed. -/
theorem cls : IsRightFold (insertF : List (ℤ × ℤ) → (Option (ℤ × ℤ) → List (ℤ × ℤ))) := by
  refine ⟨stepI, baseI, fun ints => ?_⟩
  induction ints with
  | nil => funext st; cases st <;> rfl
  | cons I rest ih =>
    rw [List.foldr_cons, ← ih]
    funext st
    obtain ⟨lo, hi⟩ := I
    cases st with
    | none => rfl
    | some nv => obtain ⟨nlo, nhi⟩ := nv; rfl

/-- With the new interval already placed, the catamorphism is the identity (pass-through). -/
theorem insertF_none : ∀ ints : List (ℤ × ℤ), insertF ints none = ints
  | [] => rfl
  | (_, _) :: rest => by rw [insertF]; rw [insertF_none rest]

/-- The natural recursion and the catamorphism agree: `insert` IS `insertF` from `some new`. -/
theorem insert_eq_insertF : ∀ (ints : List (ℤ × ℤ)) (new : ℤ × ℤ),
    insert ints new = insertF ints (some new)
  | [], _ => rfl
  | (lo, hi) :: rest, (nlo, nhi) => by
    rw [insert, insertF]
    by_cases h1 : hi < nlo
    · rw [if_pos h1, if_pos h1, insert_eq_insertF rest (nlo, nhi)]
    · rw [if_neg h1, if_neg h1]
      by_cases h2 : nhi < lo
      · rw [if_pos h2, if_pos h2, insertF_none rest]
      · rw [if_neg h2, if_neg h2, insert_eq_insertF rest (min lo nlo, max hi nhi)]

/-- The classified catamorphism, started from the pending new interval, is exactly `sol`. -/
theorem sol_eq (ints : List (ℤ × ℤ)) (new : ℤ × ℤ) : sol ints new = insertF ints (some new) :=
  insert_eq_insertF ints new

theorem covered_cons (I : ℤ × ℤ) (rest : List (ℤ × ℤ)) (p : ℤ) :
    covered (I :: rest) p ↔ (I.1 ≤ p ∧ p ≤ I.2) ∨ covered rest p := by
  simp [covered]

/-- CORRECT: the output covers exactly the input's points together with the new interval's. -/
theorem corr : ∀ (ints : List (ℤ × ℤ)) (new : ℤ × ℤ) (p : ℤ),
    covered (sol ints new) p ↔ covered ints p ∨ (new.1 ≤ p ∧ p ≤ new.2) := by
  unfold sol
  intro ints
  induction ints with
  | nil => intro new p; simp [insert, covered]
  | cons I rest ih =>
    intro new p
    obtain ⟨lo, hi⟩ := I
    obtain ⟨nlo, nhi⟩ := new
    simp only [insert]
    by_cases h1 : hi < nlo
    · simp only [if_pos h1, covered_cons, ih (nlo, nhi) p]; tauto
    · by_cases h2 : nhi < lo
      · simp only [if_neg h1, if_pos h2, covered_cons]; tauto
      · simp only [if_neg h1, if_neg h2, ih (min lo nlo, max hi nhi) p, covered_cons]
        have hmerge : (min lo nlo ≤ p ∧ p ≤ max hi nhi) ↔
            ((lo ≤ p ∧ p ≤ hi) ∨ (nlo ≤ p ∧ p ≤ nhi)) := by
          push_neg at h1 h2; omega
        simp only [hmerge]; tauto

end LC.P0057
