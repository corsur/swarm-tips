import Lproofs.Schemes.Fold

/-! @lc 941 | name:Valid Mountain Array | scheme:fold | family:two-pointers | complexity:O(n) |
    source:https://leetcode.com/problems/valid-mountain-array/

    An array is a *mountain* when it strictly increases to a peak and then strictly decreases, both
    runs nonempty (so length ≥ 3, peak interior). Read off the adjacency comparisons
    (`signs`: `<`/`=`/`>` between consecutive elements), this is exactly the regular pattern
    `lt⁺ gt⁺` with no `eq`. The accepted linear scan is the run of a 4-state DFA over those signs — a
    genuine streaming left fold over the array (carrying the previous element and the DFA state).
    `cls` certifies that fold; `corr` proves the DFA accepts iff the sign-sequence matches `lt⁺ gt⁺`.

        sign:   lt        gt
        s0 ──▶ inc ──▶ dec   (accept)
               │ lt↺    │ gt↺
        anything else / eq ──▶ dead -/

namespace LC.P0941
open Interview.Patterns

/-- DFA states for the run `increasing⁺ decreasing⁺`. -/
inductive St where | s0 | inc | dec | dead
  deriving DecidableEq

/-- DFA transition on one adjacency comparison. -/
def trans : St → Ordering → St
  | .s0,  .lt => .inc
  | .inc, .lt => .inc
  | .inc, .gt => .dec
  | .dec, .gt => .dec
  | _,    _   => .dead

/-- Adjacency comparisons between consecutive elements. -/
def signs (a : List ℤ) : List Ordering := (a.zip a.tail).map (fun p => compare p.1 p.2)

/-- Running the DFA over a sign-sequence. -/
def run (l : List Ordering) : St := l.foldl trans .s0

/-- One fold step over the array: thread the previous element and the DFA state. -/
def stepA : Option ℤ × St → ℤ → Option ℤ × St
  | (none, st),   x => (some x, st)
  | (some p, st), x => (some x, trans st (compare p x))

/-- The real accepted scan: fold the DFA across the array; accept iff it ends in `dec`. -/
def sol (a : List ℤ) : Bool := decide ((a.foldl stepA (none, St.s0)).2 = St.dec)

/-- Faithful mountain spec: the signs are `lt⁺ gt⁺` (two nonempty monotone runs, no `eq`). -/
def spec (a : List ℤ) : Prop :=
  ∃ i j, 1 ≤ i ∧ 1 ≤ j ∧ signs a = List.replicate i .lt ++ List.replicate j .gt

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

theorem run_lt_inc : ∀ m, (List.replicate m .lt).foldl trans .inc = .inc
  | 0 => rfl
  | m + 1 => by rw [List.replicate_succ, List.foldl_cons]; exact run_lt_inc m

theorem run_gt_dec : ∀ m, (List.replicate m .gt).foldl trans .dec = .dec
  | 0 => rfl
  | m + 1 => by rw [List.replicate_succ, List.foldl_cons]; exact run_gt_dec m

theorem blk_lt_s0 (i) (hi : 1 ≤ i) : (List.replicate i .lt).foldl trans .s0 = .inc := by
  obtain ⟨m, rfl⟩ := Nat.exists_eq_add_of_le hi
  rw [List.replicate_add, List.foldl_append, List.replicate_one, List.foldl_cons, List.foldl_nil]
  exact run_lt_inc m

theorem blk_gt_inc (j) (hj : 1 ≤ j) : (List.replicate j .gt).foldl trans .inc = .dec := by
  obtain ⟨m, rfl⟩ := Nat.exists_eq_add_of_le hj
  rw [List.replicate_add, List.foldl_append, List.replicate_one, List.foldl_cons, List.foldl_nil]
  exact run_gt_dec m

/-- Backward: a `lt⁺ gt⁺` sign-sequence is accepted. -/
theorem run_block (i j) (hi : 1 ≤ i) (hj : 1 ≤ j) :
    run (List.replicate i .lt ++ List.replicate j .gt) = .dec := by
  rw [run, List.foldl_append, blk_lt_s0 i hi, blk_gt_inc j hj]

/-! ### Forward characterizations (reverse induction) -/

theorem run_s0_mp (l : List Ordering) : run l = .s0 → l = [] := by
  induction l using List.reverseRecOn with
  | nil => intro _; rfl
  | append_singleton l o _ =>
    intro h
    rw [run, List.foldl_append, List.foldl_cons, List.foldl_nil] at h
    exfalso; revert h; cases hr : l.foldl trans .s0 <;> cases o <;> simp [trans]

theorem run_inc_mp (l : List Ordering) :
    run l = .inc → ∃ i, 1 ≤ i ∧ l = List.replicate i .lt := by
  induction l using List.reverseRecOn with
  | nil => intro h; simp only [run, List.foldl_nil] at h; exact absurd h (by decide)
  | append_singleton l o ih =>
    intro h
    rw [run, List.foldl_append, List.foldl_cons, List.foldl_nil] at h
    have hoeq : o = Ordering.lt := by
      revert h; cases hr : l.foldl trans .s0 <;> cases o <;> simp [trans]
    have hrl : l.foldl trans .s0 = .s0 ∨ l.foldl trans .s0 = .inc := by
      revert h; cases hr : l.foldl trans .s0 <;> cases o <;> simp [trans]
    rw [hoeq]
    rcases hrl with hs0 | hinc
    · rw [run_s0_mp l hs0]; exact ⟨1, le_refl _, rfl⟩
    · obtain ⟨i, hi, hl⟩ := ih hinc
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
    have hrl : l.foldl trans .s0 = .inc ∨ l.foldl trans .s0 = .dec := by
      revert h; cases hr : l.foldl trans .s0 <;> cases o <;> simp [trans]
    rw [hoeq]
    rcases hrl with hinc | hdec
    · obtain ⟨i, hi, hl⟩ := run_inc_mp l hinc
      exact ⟨i, 1, hi, le_refl _, by rw [hl, List.replicate_one]⟩
    · obtain ⟨i, j, hi, hj, hl⟩ := ih hdec
      exact ⟨i, j + 1, hi, Nat.le_succ_of_le hj, by
        rw [hl, List.append_assoc, ← List.replicate_succ']⟩

/-- CORRECT: the scan accepts iff the array is a mountain (signs match `lt⁺ gt⁺`). -/
theorem corr (a : List ℤ) : sol a = true ↔ spec a := by
  rw [sol, decide_eq_true_iff, runA_eq_run, spec]
  constructor
  · exact run_dec_mp (signs a)
  · rintro ⟨i, j, hi, hj, hl⟩
    rw [hl]; exact run_block i j hi hj

end LC.P0941
