import Lproofs.Schemes.Fold

/-! @lc 97 | name:Interleaving String | scheme:dp | family:dp-grid | complexity:O(mn) |
    source:https://leetcode.com/problems/interleaving-string/

    Decide whether `s3` is an interleaving of `s1` and `s2`. The DP recurrence consumes `s3`'s head
    from whichever of `s1`/`s2` matches. Correctness: it returns `true` exactly when an actual
    interleaving exists — a choice of which source each position of `s3` came from that reproduces
    `s1` (the chosen positions) and `s2` (the rest). -/

namespace LC.P0097
open Interview.Patterns

/-- DP recurrence: consume `s3`'s head from `s1` or `s2`. -/
def canInter : List Char → List Char → List Char → Bool
  | s1, s2, [] => s1.isEmpty && s2.isEmpty
  | s1, s2, c :: s3 =>
    (match s1 with | a :: s1' => (a == c) && canInter s1' s2 s3 | [] => false) ||
    (match s2 with | b :: s2' => (b == c) && canInter s1 s2' s3 | [] => false)

/-- The `s3` positions assigned to source `b`. -/
def pick (b : Bool) : List Bool → List Char → List Char
  | x :: ch, c :: s3 => if x = b then c :: pick b ch s3 else pick b ch s3
  | _, _ => []

/-- An actual interleaving witness: a per-position source choice reproducing `s1` and `s2`. -/
def spec (s1 s2 s3 : List Char) : Prop :=
  ∃ ch : List Bool, ch.length = s3.length ∧ pick true ch s3 = s1 ∧ pick false ch s3 = s2

def sol (s1 s2 s3 : List Char) : Bool := canInter s1 s2 s3

/-- One catamorphism step over `s3`: the accumulator is the DP plane `(List Char × List Char) → Bool`
    (the answer as a function of the remaining `s1`/`s2`). -/
def stepI (c : Char) (rec : List Char × List Char → Bool) : List Char × List Char → Bool :=
  fun p => (match p.1 with | a :: s1' => (a == c) && rec (s1', p.2) | [] => false) ||
           (match p.2 with | b :: s2' => (b == c) && rec (p.1, s2') | [] => false)

/-- SCHEME (dp = recursive decomposition): `canInter` is a genuine right fold (catamorphism) over
    `s3`, carrying the DP plane `(List Char × List Char) → Bool` as its accumulator. This is `sol`. -/
theorem cls : IsRightFold (fun s3 : List Char => (fun p : List Char × List Char => canInter p.1 p.2 s3)) := by
  refine ⟨stepI, (fun p => p.1.isEmpty && p.2.isEmpty), fun s3 => ?_⟩
  induction s3 with
  | nil => funext p; obtain ⟨s1, s2⟩ := p; rfl
  | cons c s3 ih =>
    rw [List.foldr_cons, ← ih]
    funext p; obtain ⟨s1, s2⟩ := p; rfl

/-- CORRECT: the recurrence holds exactly when a real interleaving exists. -/
theorem corr : ∀ (s3 s1 s2 : List Char), canInter s1 s2 s3 = true ↔ spec s1 s2 s3 := by
  intro s3
  induction s3 with
  | nil =>
    intro s1 s2
    constructor
    · intro h
      simp only [canInter, Bool.and_eq_true] at h
      have e1 : s1 = [] := by cases s1 with | nil => rfl | cons => simp at h
      have e2 : s2 = [] := by cases s2 with | nil => rfl | cons => simp at h
      exact ⟨[], rfl, by rw [e1]; rfl, by rw [e2]; rfl⟩
    · rintro ⟨ch, hlen, hp1, hp2⟩
      have hch : ch = [] := by cases ch with | nil => rfl | cons => simp at hlen
      subst hch
      simp only [pick] at hp1 hp2
      simp [canInter, ← hp1, ← hp2]
  | cons c s3' ih =>
    intro s1 s2
    simp only [canInter, Bool.or_eq_true]
    constructor
    · rintro (h | h)
      · cases s1 with
        | nil => simp at h
        | cons a s1' =>
          simp only [Bool.and_eq_true, beq_iff_eq] at h
          obtain ⟨rfl, hci⟩ := h
          obtain ⟨ch, hlen, hp1, hp2⟩ := (ih s1' s2).mp hci
          exact ⟨true :: ch, by simp [hlen], by simp [pick, hp1], by simp [pick, hp2]⟩
      · cases s2 with
        | nil => simp at h
        | cons b s2' =>
          simp only [Bool.and_eq_true, beq_iff_eq] at h
          obtain ⟨rfl, hci⟩ := h
          obtain ⟨ch, hlen, hp1, hp2⟩ := (ih s1 s2').mp hci
          exact ⟨false :: ch, by simp [hlen], by simp [pick, hp1], by simp [pick, hp2]⟩
    · rintro ⟨ch, hlen, hp1, hp2⟩
      cases ch with
      | nil => simp at hlen
      | cons x ch' =>
        simp only [List.length_cons, Nat.add_right_cancel_iff] at hlen
        cases x with
        | true =>
          simp only [pick, if_pos] at hp1
          simp only [pick, Bool.true_eq_false, if_false] at hp2
          left
          rw [← hp1]
          have hci : canInter (pick true ch' s3') s2 s3' = true := (ih _ s2).mpr ⟨ch', hlen, rfl, hp2⟩
          simp [hci]
        | false =>
          simp only [pick, Bool.false_eq_true, if_false] at hp1
          simp only [pick, if_pos] at hp2
          right
          rw [← hp2]
          have hci : canInter s1 (pick false ch' s3') s3' = true := (ih s1 _).mpr ⟨ch', hlen, hp1, rfl⟩
          simp [hci]

end LC.P0097
