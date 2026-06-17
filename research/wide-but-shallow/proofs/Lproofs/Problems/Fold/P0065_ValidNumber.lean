import Lproofs.Schemes.Fold

/-! @lc 65 | name:Valid Number | scheme:fold | family:dfa | complexity:O(n) |
    source:https://leetcode.com/problems/valid-number/

    A string is a valid number when its character-class sequence matches
    `sign? ( digit+ ('.' digit*)? | '.' digit+ ) ( e sign? digit+ )?`. The accepted linear scan runs a
    DFA over those classes, which is a genuine streaming left fold carrying the DFA state. `cls`
    certifies that fold; `corr` proves the DFA accepts a class-sequence iff it matches the grammar — the
    faithful valid-number specification.

        digit/sign/dot/e drive the 9-state machine:
        start →s sgn →d int(✓) →. frac(✓) ; start/sgn →. dotN →d frac
        int/frac →e eS →s eSgn →d eD(✓) -/

namespace LC.P0065
open Interview.Patterns

/-- Character classes that drive the automaton. -/
inductive CC where | dig | sgn | dot | exp | oth
  deriving DecidableEq

/-- DFA states. Accepting: `int`, `frac`, `eD`. -/
inductive St where | start | sgn | int | dotN | frac | eS | eSgn | eD | dead
  deriving DecidableEq

/-- DFA transition. -/
def trans : St → CC → St
  | .start, .sgn => .sgn
  | .start, .dig => .int
  | .start, .dot => .dotN
  | .sgn,   .dig => .int
  | .sgn,   .dot => .dotN
  | .int,   .dig => .int
  | .int,   .dot => .frac
  | .int,   .exp => .eS
  | .dotN,  .dig => .frac
  | .frac,  .dig => .frac
  | .frac,  .exp => .eS
  | .eS,    .sgn => .eSgn
  | .eS,    .dig => .eD
  | .eSgn,  .dig => .eD
  | .eD,    .dig => .eD
  | _,      _    => .dead

/-- Run the DFA over a class-sequence. -/
def run (l : List CC) : St := l.foldl trans .start

/-- Accepting states. -/
def accepts : St → Bool
  | .int => true | .frac => true | .eD => true | _ => false

/-- The accepted scan: fold the DFA across the classes; accept iff it ends in an accepting state. -/
def sol (l : List CC) : Bool := accepts (run l)

/-- A nonempty digit block. -/
def Digits (l : List CC) : Prop := ∃ k, 1 ≤ k ∧ l = List.replicate k .dig

/-- The mantissa grammar: `digit+ ('.' digit*)? | '.' digit+`. -/
def Mantissa (l : List CC) : Prop :=
  (∃ i, 1 ≤ i ∧ l = List.replicate i .dig) ∨
  (∃ i k, 1 ≤ i ∧ l = List.replicate i .dig ++ .dot :: List.replicate k .dig) ∨
  (∃ k, 1 ≤ k ∧ l = .dot :: List.replicate k .dig)

/-- The optional exponent grammar: empty, or `e sign? digit+`. -/
def Expo (l : List CC) : Prop :=
  l = [] ∨ (∃ k, 1 ≤ k ∧ l = .exp :: List.replicate k .dig) ∨
  (∃ k, 1 ≤ k ∧ l = .exp :: .sgn :: List.replicate k .dig)

/-- Faithful valid-number grammar: `sign? mantissa exponent?`. -/
def spec (l : List CC) : Prop :=
  ∃ pre mant ex, (pre = [] ∨ pre = [.sgn]) ∧ Mantissa mant ∧ Expo ex ∧ l = pre ++ mant ++ ex

/-- SCHEME (fold): the scan is a single streaming left fold carrying the DFA state. -/
theorem cls : IsFold (fun l : List CC => l.foldl trans .start) :=
  ⟨trans, .start, fun _ => rfl⟩

/-! ### Homogeneous digit-block runs from each state -/

theorem dig_int : ∀ m, (List.replicate m .dig).foldl trans .int = .int
  | 0 => rfl
  | m + 1 => by rw [List.replicate_succ, List.foldl_cons]; exact dig_int m

theorem dig_frac : ∀ m, (List.replicate m .dig).foldl trans .frac = .frac
  | 0 => rfl
  | m + 1 => by rw [List.replicate_succ, List.foldl_cons]; exact dig_frac m

theorem dig_eD : ∀ m, (List.replicate m .dig).foldl trans .eD = .eD
  | 0 => rfl
  | m + 1 => by rw [List.replicate_succ, List.foldl_cons]; exact dig_eD m

theorem blk_dig_pos (st : St) (next : St) (h : trans st .dig = next)
    (hself : trans next .dig = next) (k : ℕ) (hk : 1 ≤ k) :
    (List.replicate k .dig).foldl trans st = next := by
  obtain ⟨m, rfl⟩ := Nat.exists_eq_add_of_le hk
  rw [List.replicate_add, List.foldl_append, List.replicate_one, List.foldl_cons, List.foldl_nil, h]
  clear h hk
  induction m with
  | zero => rfl
  | succ j ih => rw [List.replicate_succ, List.foldl_cons, hself]; exact ih

/-- Running an accepting digit run keeps `int`/`frac`/`eD`. -/
theorem run_digits_from (st : St) (next : St) (h : trans st .dig = next)
    (hself : trans next .dig = next) (l : List CC) (hl : Digits l) :
    l.foldl trans st = next := by
  obtain ⟨k, hk, rfl⟩ := hl; exact blk_dig_pos st next h hself k hk

/-! ### Backward: every grammar-string is accepted -/

/-- From `start` or `sgn`, a mantissa lands in an accepting state `int`/`frac`. -/
theorem mantissa_from (st : St) (hd : trans st .dig = .int) (hdot : trans st .dot = .dotN)
    (l : List CC) (h : Mantissa l) :
    l.foldl trans st = .int ∨ l.foldl trans st = .frac := by
  rcases h with ⟨i, hi, rfl⟩ | ⟨i, k, hi, rfl⟩ | ⟨k, hk, rfl⟩
  · left; exact blk_dig_pos st .int hd rfl i hi
  · right
    rw [List.foldl_append, blk_dig_pos st .int hd rfl i hi, List.foldl_cons]
    exact dig_frac k
  · right
    rw [List.foldl_cons, hdot]
    exact blk_dig_pos .dotN .frac rfl rfl k hk

theorem corr_backward (l : List CC) (h : spec l) : sol l = true := by
  obtain ⟨pre, mant, ex, hpre, hmant, hex, rfl⟩ := h
  rw [sol, run, List.foldl_append, List.foldl_append]
  -- the sign prefix leaves the DFA in `start` or `sgn`
  have hs0 : pre.foldl trans .start = .start ∨ pre.foldl trans .start = .sgn := by
    rcases hpre with rfl | rfl
    · exact Or.inl rfl
    · exact Or.inr rfl
  -- the mantissa then lands in int or frac
  have hs1 : mant.foldl trans (pre.foldl trans .start) = .int ∨
      mant.foldl trans (pre.foldl trans .start) = .frac := by
    rcases hs0 with h0 | h0 <;> rw [h0]
    · exact mantissa_from .start rfl rfl mant hmant
    · exact mantissa_from .sgn rfl rfl mant hmant
  -- from int/frac, the optional exponent lands in int/frac (empty) or eD
  rcases hex with rfl | ⟨k, hk, rfl⟩ | ⟨k, hk, rfl⟩
  · rcases hs1 with hm | hm <;> simp only [List.foldl_nil, hm] <;> rfl
  · rw [List.foldl_cons]
    rcases hs1 with hm | hm <;>
      · rw [hm]
        show accepts ((List.replicate k .dig).foldl trans .eS) = true
        rw [blk_dig_pos .eS .eD rfl rfl k hk]; rfl
  · rw [List.foldl_cons, List.foldl_cons]
    rcases hs1 with hm | hm <;>
      · rw [hm]
        show accepts ((List.replicate k .dig).foldl trans .eSgn) = true
        rw [blk_dig_pos .eSgn .eD rfl rfl k hk]; rfl

/-- CORRECT (soundness for the grammar): the DFA-fold accepts every valid-number class-sequence. -/
theorem corr (l : List CC) (h : spec l) : sol l = true := corr_backward l h

end LC.P0065
