import Lproofs.Patterns

/-! @lc 66 | name:Plus One | scheme:fold | family:carry-propagation | complexity:O(n) |
    source:https://leetcode.com/problems/plus-one/

    Increment a big-endian digit array by one. CLASSIFICATION: the accepted right-to-left carry pass
    is a genuine RIGHT FOLD whose accumulator is a *bounded* `(carry-flag, output-digits)` pair — the
    carry is one bit, not the deferred input. NON-VACUITY: we pin the fold to real incremental work by
    proving the value recurrence `toNat (plusOne ds) = toNat ds + 1` (so the step genuinely adds one
    with carry, not a re-encoding). We certify the fold structure + the increment, not any optimality. -/

namespace LC.P0066
open Interview.Patterns

/-- Value of a big-endian (most-significant-first) digit list. -/
def toNat : List ℕ → ℕ
  | [] => 0
  | d :: ds => d * 10 ^ ds.length + toNat ds

/-- One carry step (processing a digit, LSB-first under `foldr`): bounded `(carry?, out)` state. -/
def step (d : ℕ) (st : Bool × List ℕ) : Bool × List ℕ :=
  if st.1 then
    if d + 1 < 10 then (false, (d + 1) :: st.2) else (true, 0 :: st.2)
  else (false, d :: st.2)

/-- The carry pass: a right fold over the digits, seeded with an incoming carry (the `+1`). -/
def run (ds : List ℕ) : Bool × List ℕ := ds.foldr step (true, [])

/-- The accepted answer: the carry pass, prepending a leading `1` if a carry escapes the top. -/
def plusOne (ds : List ℕ) : List ℕ := let r := run ds; if r.1 then 1 :: r.2 else r.2

def sol (ds : List ℕ) : List ℕ := plusOne ds

/-- SCHEME (fold): the carry pass is a right fold with a bounded `(carry?, digits)` accumulator. -/
theorem cls : IsRightFold run := ⟨step, (true, []), fun _ => rfl⟩

/-- The output of `run` has the same length as its input (the carry pass is digit-aligned). -/
theorem run_length (ds : List ℕ) : (run ds).2.length = ds.length := by
  induction ds with
  | nil => rfl
  | cons d ds ih =>
    rw [show run (d :: ds) = step d (run ds) from rfl]
    rcases hr : run ds with ⟨c, out⟩
    simp only [hr] at ih
    cases c <;> by_cases hlt : d + 1 < 10 <;>
      simp [step, hlt, List.length_cons] <;> omega

/-- NON-VACUITY (the value recurrence): for a valid digit list (each digit `< 10`), the carry-fold's
    `(carry?, out)` state represents `toNat ds + 1`, with the carry weighted at the top position
    `10^|ds|`. This pins the step to genuine `+1`-with-carry work, not a re-encoding. -/
theorem run_spec (ds : List ℕ) (hv : ∀ x ∈ ds, x < 10) :
    toNat (run ds).2 + (if (run ds).1 then 10 ^ ds.length else 0) = toNat ds + 1 := by
  induction ds with
  | nil => simp [run, toNat]
  | cons d ds ih =>
    have hd : d < 10 := hv d (by simp)
    have hih := ih (fun x hx => hv x (by simp [hx]))
    have hlen := run_length ds
    rw [show run (d :: ds) = step d (run ds) from rfl]
    rcases hr : run ds with ⟨c, out⟩
    simp only [hr] at hih hlen
    cases c
    · simp [step, toNat, List.length_cons, hlen] at hih ⊢
      omega
    · by_cases hlt : d + 1 < 10
      · simp [step, hlt, toNat, List.length_cons, hlen, add_mul, one_mul] at hih ⊢
        omega
      · have hd9 : d = 9 := by omega
        subst hd9
        simp [step, hlt, toNat, List.length_cons, hlen, pow_succ, zero_mul] at hih ⊢
        omega

/-- CORRECT (increment): `plusOne` computes the successor of the represented number. -/
theorem corr (ds : List ℕ) (hv : ∀ x ∈ ds, x < 10) : toNat (plusOne ds) = toNat ds + 1 := by
  have h := run_spec ds hv
  have hl := run_length ds
  rcases hr : run ds with ⟨c, out⟩
  simp only [hr] at h hl
  unfold plusOne
  rw [hr]
  cases c <;>
    simp [toNat, hl, one_mul] at h ⊢ <;>
    omega

end LC.P0066
