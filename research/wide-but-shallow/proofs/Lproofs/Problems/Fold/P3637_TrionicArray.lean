import Lproofs.Schemes.Fold

/-! @lc 3637 | name:Trionic Array I | scheme:fold | family:two-pointers | complexity:O(n) |
    source:https://leetcode.com/problems/trionic-array-i/

    An array is *trionic* when it splits into three contiguous strictly-monotone runs — increasing,
    then decreasing, then increasing — each nonempty. Read off the adjacency comparisons
    (`signs`: `<`/`=`/`>` between consecutive elements), this is exactly the regular pattern
    `lt⁺ gt⁺ lt⁺` with no `eq`. The accepted linear scan is the run of a 5-state DFA over those
    signs, which is a genuine streaming left fold over the array (carrying the previous element and
    the DFA state). `cls` certifies that fold; `corr` proves the DFA accepts iff the sign-sequence
    matches the `lt⁺ gt⁺ lt⁺` pattern — the faithful trionic specification.

        sign:   lt        gt        lt
        s0 ──▶ inc1 ──▶ dec ──▶ inc2   (accept)
               │ lt↺     │ gt↺   │ lt↺
        anything else / eq  ──▶ dead -/

namespace LC.P3637
open Interview.Patterns

/-- DFA states for the run `increasing⁺ decreasing⁺ increasing⁺`. -/
inductive St where | s0 | inc1 | dec | inc2 | dead
  deriving DecidableEq

/-- DFA transition on one adjacency comparison. -/
def trans : St → Ordering → St
  | .s0,   .lt => .inc1
  | .inc1, .lt => .inc1
  | .inc1, .gt => .dec
  | .dec,  .gt => .dec
  | .dec,  .lt => .inc2
  | .inc2, .lt => .inc2
  | _,     _   => .dead

/-- Adjacency comparisons between consecutive elements. -/
def signs (a : List ℤ) : List Ordering := (a.zip a.tail).map (fun p => compare p.1 p.2)

/-- Running the DFA over a sign-sequence. -/
def run (l : List Ordering) : St := l.foldl trans .s0

/-- One fold step over the array: thread the previous element and the DFA state. -/
def stepA : Option ℤ × St → ℤ → Option ℤ × St
  | (none, st),   x => (some x, st)
  | (some p, st), x => (some x, trans st (compare p x))

/-- The real accepted scan: fold the DFA across the array; accept iff it ends in `inc2`. -/
def sol (a : List ℤ) : Bool := decide ((a.foldl stepA (none, St.s0)).2 = St.inc2)

/-- Faithful trionic spec: the signs are `lt⁺ gt⁺ lt⁺` (three nonempty monotone runs, no `eq`). -/
def spec (a : List ℤ) : Prop :=
  ∃ i j k, 1 ≤ i ∧ 1 ≤ j ∧ 1 ≤ k ∧
    signs a = List.replicate i .lt ++ List.replicate j .gt ++ List.replicate k .lt

/-- SCHEME (fold): the accepted scan is a single streaming left fold over the array, carrying the
    previous element and the DFA state — `sol` reads its final state. -/
theorem cls : IsFold (fun a : List ℤ => a.foldl stepA (none, St.s0)) :=
  ⟨stepA, (none, St.s0), fun _ => rfl⟩

/-! ### Bridging the array fold to the sign DFA -/

theorem genA : ∀ (xs : List ℤ) (p : ℤ) (st : St),
    (xs.foldl stepA (some p, st)).2
      = (((p :: xs).zip xs).map (fun q => compare q.1 q.2)).foldl trans st
  | [], _, _ => by simp
  | y :: ys, p, st => by
    simp only [List.foldl_cons, stepA, List.zip_cons_cons, List.map_cons]
    rw [genA ys y (trans st (compare p y))]

theorem runA_eq_run : ∀ a : List ℤ, (a.foldl stepA (none, St.s0)).2 = run (signs a)
  | [] => by simp [signs, run]
  | x :: xs => by
    simp only [List.foldl_cons, stepA]
    rw [genA xs x St.s0]
    simp only [run, signs, List.tail_cons]

/-! ### Homogeneous-block runs -/

theorem run_lt_inc1 : ∀ m, (List.replicate m .lt).foldl trans .inc1 = .inc1
  | 0 => rfl
  | m + 1 => by rw [List.replicate_succ, List.foldl_cons]; exact run_lt_inc1 m

theorem run_gt_dec : ∀ m, (List.replicate m .gt).foldl trans .dec = .dec
  | 0 => rfl
  | m + 1 => by rw [List.replicate_succ, List.foldl_cons]; exact run_gt_dec m

theorem run_lt_inc2 : ∀ m, (List.replicate m .lt).foldl trans .inc2 = .inc2
  | 0 => rfl
  | m + 1 => by rw [List.replicate_succ, List.foldl_cons]; exact run_lt_inc2 m

theorem blk_lt_s0 (i) (hi : 1 ≤ i) : (List.replicate i .lt).foldl trans .s0 = .inc1 := by
  obtain ⟨m, rfl⟩ := Nat.exists_eq_add_of_le hi
  rw [List.replicate_add, List.foldl_append, List.replicate_one, List.foldl_cons, List.foldl_nil]
  exact run_lt_inc1 m

theorem blk_gt_inc1 (j) (hj : 1 ≤ j) : (List.replicate j .gt).foldl trans .inc1 = .dec := by
  obtain ⟨m, rfl⟩ := Nat.exists_eq_add_of_le hj
  rw [List.replicate_add, List.foldl_append, List.replicate_one, List.foldl_cons, List.foldl_nil]
  exact run_gt_dec m

theorem blk_lt_dec (k) (hk : 1 ≤ k) : (List.replicate k .lt).foldl trans .dec = .inc2 := by
  obtain ⟨m, rfl⟩ := Nat.exists_eq_add_of_le hk
  rw [List.replicate_add, List.foldl_append, List.replicate_one, List.foldl_cons, List.foldl_nil]
  exact run_lt_inc2 m

/-- Backward: a `lt⁺ gt⁺ lt⁺` sign-sequence is accepted. -/
theorem run_block (i j k) (hi : 1 ≤ i) (hj : 1 ≤ j) (hk : 1 ≤ k) :
    run (List.replicate i .lt ++ List.replicate j .gt ++ List.replicate k .lt) = .inc2 := by
  rw [run, List.foldl_append, List.foldl_append, blk_lt_s0 i hi, blk_gt_inc1 j hj, blk_lt_dec k hk]

/-! ### Forward characterizations (reverse induction) -/

theorem run_s0_mp (l : List Ordering) : run l = .s0 → l = [] := by
  induction l using List.reverseRecOn with
  | nil => intro _; rfl
  | append_singleton l o _ =>
    intro h
    rw [run, List.foldl_append, List.foldl_cons, List.foldl_nil] at h
    exfalso; revert h; cases hr : l.foldl trans .s0 <;> cases o <;> simp [trans]

theorem run_inc1_mp (l : List Ordering) :
    run l = .inc1 → ∃ i, 1 ≤ i ∧ l = List.replicate i .lt := by
  induction l using List.reverseRecOn with
  | nil => intro h; simp only [run, List.foldl_nil] at h; exact absurd h (by decide)
  | append_singleton l o ih =>
    intro h
    rw [run, List.foldl_append, List.foldl_cons, List.foldl_nil] at h
    have hoeq : o = Ordering.lt := by
      revert h; cases hr : l.foldl trans .s0 <;> cases o <;> simp [trans]
    have hrl : l.foldl trans .s0 = .s0 ∨ l.foldl trans .s0 = .inc1 := by
      revert h; cases hr : l.foldl trans .s0 <;> cases o <;> simp [trans]
    rw [hoeq]
    rcases hrl with hs0 | hinc1
    · rw [run_s0_mp l hs0]; exact ⟨1, le_refl _, rfl⟩
    · obtain ⟨i, hi, hl⟩ := ih hinc1
      exact ⟨i + 1, Nat.le_succ_of_le hi, by rw [hl, List.replicate_succ']⟩

theorem run_dec_mp (l : List Ordering) :
    run l = .dec → ∃ i j, 1 ≤ i ∧ 1 ≤ j ∧
      l = List.replicate i .lt ++ List.replicate j .gt := by
  induction l using List.reverseRecOn with
  | nil => intro h; simp only [run, List.foldl_nil] at h; exact absurd h (by decide)
  | append_singleton l o ih =>
    intro h
    rw [run, List.foldl_append, List.foldl_cons, List.foldl_nil] at h
    have hoeq : o = Ordering.gt := by
      revert h; cases hr : l.foldl trans .s0 <;> cases o <;> simp [trans]
    have hrl : l.foldl trans .s0 = .inc1 ∨ l.foldl trans .s0 = .dec := by
      revert h; cases hr : l.foldl trans .s0 <;> cases o <;> simp [trans]
    rw [hoeq]
    rcases hrl with hinc1 | hdec
    · obtain ⟨i, hi, hl⟩ := run_inc1_mp l hinc1
      exact ⟨i, 1, hi, le_refl _, by rw [hl, List.replicate_one]⟩
    · obtain ⟨i, j, hi, hj, hl⟩ := ih hdec
      exact ⟨i, j + 1, hi, Nat.le_succ_of_le hj, by
        rw [hl, List.append_assoc, ← List.replicate_succ']⟩

theorem run_inc2_mp (l : List Ordering) :
    run l = .inc2 → ∃ i j k, 1 ≤ i ∧ 1 ≤ j ∧ 1 ≤ k ∧
      l = List.replicate i .lt ++ List.replicate j .gt ++ List.replicate k .lt := by
  induction l using List.reverseRecOn with
  | nil => intro h; simp only [run, List.foldl_nil] at h; exact absurd h (by decide)
  | append_singleton l o ih =>
    intro h
    rw [run, List.foldl_append, List.foldl_cons, List.foldl_nil] at h
    have hoeq : o = Ordering.lt := by
      revert h; cases hr : l.foldl trans .s0 <;> cases o <;> simp [trans]
    have hrl : l.foldl trans .s0 = .dec ∨ l.foldl trans .s0 = .inc2 := by
      revert h; cases hr : l.foldl trans .s0 <;> cases o <;> simp [trans]
    rw [hoeq]
    rcases hrl with hdec | hinc2
    · obtain ⟨i, j, hi, hj, hl⟩ := run_dec_mp l hdec
      exact ⟨i, j, 1, hi, hj, le_refl _, by rw [hl, List.replicate_one]⟩
    · obtain ⟨i, j, k, hi, hj, hk, hl⟩ := ih hinc2
      exact ⟨i, j, k + 1, hi, hj, Nat.le_succ_of_le hk, by
        rw [hl, List.append_assoc, ← List.replicate_succ']⟩

/-- CORRECT: the scan accepts iff the array is trionic (signs match `lt⁺ gt⁺ lt⁺`). -/
theorem corr (a : List ℤ) : sol a = true ↔ spec a := by
  rw [sol, decide_eq_true_iff, runA_eq_run, spec]
  constructor
  · exact run_inc2_mp (signs a)
  · rintro ⟨i, j, k, hi, hj, hk, hl⟩
    rw [hl]; exact run_block i j k hi hj hk

end LC.P3637
