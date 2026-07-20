import Lproofs.Schemes.Fold

/-! @lc 20 | name:Valid Parentheses | scheme:fold | family:matching-stack | complexity:O(n) |
    source:https://leetcode.com/problems/valid-parentheses/

    Match brackets with a stack: push an opener, pop on a matching closer. CLASSIFICATION: the scan is
    a streaming left fold whose accumulator is the stack of unmatched openers. NON-VACUITY: we prove the
    stack invariant the matching relies on — at every point the stack holds only opening brackets — so
    the accumulator is the genuine unmatched-opener stack, not a re-encoding. We certify the fold + the
    only-openers invariant (modelled for a single bracket pair). -/

namespace LC.P0020
open Interview.Patterns

/-- One step over `(`/`)`: push an open bracket, pop on a close, ignore other characters. -/
def step (stk : List Char) (c : Char) : List Char :=
  if c = '(' then '(' :: stk else if c = ')' then stk.tail else stk

/-- Scan the string through the matching stack. -/
def sol (s : List Char) : List Char := s.foldl step []

/-- SCHEME (fold): the bracket matching is a left fold with the unmatched-opener stack — and
    `sol` is exactly that fold. -/
theorem cls : IsFold (fun s : List Char => s.foldl step []) ∧
    ∀ s : List Char, sol s = s.foldl step [] :=
  ⟨⟨step, [], fun _ => rfl⟩, fun _ => rfl⟩

/-- One step preserves "stack holds only openers". -/
theorem step_open (stk : List Char) (c : Char) (h : ∀ x ∈ stk, x = '(') :
    ∀ x ∈ step stk c, x = '(' := by
  unfold step
  split_ifs with h1 h2
  · intro x hx
    rcases List.mem_cons.mp hx with rfl | hr
    · rfl
    · exact h x hr
  · intro x hx; exact h x (List.tail_subset stk hx)
  · exact h

/-- NON-VACUITY: at every point the stack holds only opening brackets — the matching invariant. -/
theorem corr (s : List Char) : ∀ x ∈ sol s, x = '(' := by
  unfold sol
  have key : ∀ (xs : List Char) stk, (∀ x ∈ stk, x = '(') → ∀ x ∈ xs.foldl step stk, x = '(' := by
    intro xs
    induction xs with
    | nil => intro stk h; exact h
    | cons c rest ih => intro stk h; exact ih (step stk c) (step_open stk c h)
  exact key s [] (by simp)


/-- GROUND INSTANCE (official examples 1 and 3): "()" matches to an empty stack (valid);
    an unclosed "(" survives the scan (invalid). -/
theorem vec : sol "()".toList = [] ∧ sol "((".toList = ['(', '('] := by decide


/-! ### The strict checker and its full specification

Attempting the full "empty stack ↔ balanced" iff exposed a modelling gap: the lenient pass
above (`tail` on an empty stack) accepts `")"`, so its final-stack emptiness is only NECESSARY
for balancedness. The faithful accepted solution rejects a pop of the empty stack; `check`
models that strictly (an `Option ℕ` depth counter — still one streaming fold), and its full
specification below is exactly the Dyck-word condition Mathlib's
`Combinatorics.Enumerative.DyckWord` uses: no prefix closes more than it opens, and the totals
match. -/

/-- Strict step: track the open depth; a close at depth 0 fails permanently. -/
def stepC (st : Option ℕ) (c : Char) : Option ℕ :=
  st.bind fun n =>
    if c = '(' then some (n + 1)
    else if c = ')' then (match n with | 0 => none | m + 1 => some m)
    else some n

/-- The strict checker: a single fold; `some 0` means balanced. -/
def check (s : List Char) : Option ℕ := s.foldl stepC (some 0)

theorem foldl_stepC_none : ∀ s : List Char, s.foldl stepC none = none
  | [] => rfl
  | _ :: s => foldl_stepC_none s

/-- Mathlib's Dyck condition: every prefix closes at most as much as it opens, totals equal. -/
def IsBalanced (s : List Char) : Prop :=
  (∀ p, p <+: s → p.count ')' ≤ p.count '(') ∧ s.count '(' = s.count ')'

theorem checkGo_iff : ∀ (s : List Char) (n m : ℕ), (∀ c ∈ s, c = '(' ∨ c = ')') →
    (s.foldl stepC (some n) = some m ↔
      (∀ p, p <+: s → p.count ')' ≤ n + p.count '(') ∧
        n + s.count '(' = m + s.count ')') := by
  intro s
  induction s with
  | nil =>
    intro n m _
    simp only [List.foldl_nil, Option.some_inj, List.prefix_nil]
    constructor
    · rintro rfl
      exact ⟨fun p hp => by simp [hp], by simp⟩
    · rintro ⟨-, h⟩
      simpa using h
  | cons c s' ih =>
    intro n m hpo
    have hc := hpo c List.mem_cons_self
    have hrest : ∀ x ∈ s', x = '(' ∨ x = ')' :=
      fun x hx => hpo x (List.mem_cons_of_mem _ hx)
    rcases hc with rfl | rfl
    · rw [show (('(' :: s').foldl stepC (some n)) = s'.foldl stepC (some (n + 1)) from by
        simp [stepC]]
      rw [ih (n + 1) m hrest]
      constructor
      · rintro ⟨hpre, heq⟩
        refine ⟨?_, by simp [List.count_cons]; omega⟩
        intro p hp
        rcases List.prefix_cons_iff.mp hp with rfl | ⟨q, rfl, hq⟩
        · simp
        · have := hpre q hq
          simp [List.count_cons]
          omega
      · rintro ⟨hpre, heq⟩
        refine ⟨?_, by simp [List.count_cons] at heq; omega⟩
        intro p hp
        have := hpre ('(' :: p) (List.cons_prefix_cons.mpr ⟨rfl, hp⟩)
        simp [List.count_cons] at this
        omega
    · match n with
      | 0 =>
        rw [show ((')' :: s').foldl stepC (some 0)) = s'.foldl stepC none from by
          simp [stepC]]
        rw [foldl_stepC_none]
        constructor
        · intro h
          exact absurd h (by simp)
        · rintro ⟨hpre, -⟩
          have := hpre [')'] ⟨s', rfl⟩
          simp at this
      | k + 1 =>
        rw [show ((')' :: s').foldl stepC (some (k + 1))) = s'.foldl stepC (some k) from by
          simp [stepC]]
        rw [ih k m hrest]
        constructor
        · rintro ⟨hpre, heq⟩
          refine ⟨?_, by simp [List.count_cons]; omega⟩
          intro p hp
          rcases List.prefix_cons_iff.mp hp with rfl | ⟨q, rfl, hq⟩
          · simp
          · have := hpre q hq
            simp [List.count_cons]
            omega
        · rintro ⟨hpre, heq⟩
          refine ⟨?_, by simp [List.count_cons] at heq; omega⟩
          intro p hp
          have := hpre (')' :: p) (List.cons_prefix_cons.mpr ⟨rfl, hp⟩)
          simp [List.count_cons] at this
          omega

/-- FULL SPECIFICATION: the strict checker accepts exactly the balanced strings — the complete
    Dyck-word characterization the one-directional stack invariant could not deliver. -/
theorem check_iff_balanced (s : List Char) (hs : ∀ c ∈ s, c = '(' ∨ c = ')') :
    check s = some 0 ↔ IsBalanced s := by
  have h := checkGo_iff s 0 0 hs
  simp only [Nat.zero_add] at h
  exact h.trans (by unfold IsBalanced; exact Iff.rfl)

/-- Bridge to the lenient pass: whenever the strict checker reaches depth `n`, the lenient
    stack `sol` maintains exactly `n` pending openers — so `sol s = []` is the necessary half
    of balancedness that `corr`'s invariant certifies. -/
theorem check_sol_bridge : ∀ (s : List Char) (k n : ℕ), (∀ c ∈ s, c = '(' ∨ c = ')') →
    s.foldl stepC (some k) = some n →
    s.foldl step (List.replicate k '(') = List.replicate n '(' := by
  intro s
  induction s with
  | nil =>
    intro k n _ h
    simp only [List.foldl_nil, Option.some_inj] at h
    simp [h]
  | cons c s' ih =>
    intro k n hpo h
    have hrest : ∀ x ∈ s', x = '(' ∨ x = ')' :=
      fun x hx => hpo x (List.mem_cons_of_mem _ hx)
    rcases hpo c List.mem_cons_self with rfl | rfl
    · rw [show (('(' :: s').foldl stepC (some k)) = s'.foldl stepC (some (k + 1)) from by
        simp [stepC]] at h
      rw [show (('(' :: s').foldl step (List.replicate k '(')) =
          s'.foldl step (List.replicate (k + 1) '(') from by
        simp [step, List.replicate_succ]]
      exact ih (k + 1) n hrest h
    · match k with
      | 0 =>
        rw [show ((')' :: s').foldl stepC (some 0)) = s'.foldl stepC none from by
          simp [stepC], foldl_stepC_none] at h
        exact absurd h (by simp)
      | j + 1 =>
        rw [show ((')' :: s').foldl stepC (some (j + 1))) = s'.foldl stepC (some j) from by
          simp [stepC]] at h
        rw [show ((')' :: s').foldl step (List.replicate (j + 1) '(')) =
            s'.foldl step (List.replicate j '(') from by
          simp [step, List.replicate_succ]]
        exact ih j n hrest h

/-- GROUND INSTANCE (strict model): "()" balances; ")" is rejected (the case the lenient pass
    missed); "(" leaves one opener pending. -/
theorem vec_strict : check "()".toList = some 0 ∧ check ")".toList = none ∧
    check "(".toList = some 1 := by decide

end LC.P0020
