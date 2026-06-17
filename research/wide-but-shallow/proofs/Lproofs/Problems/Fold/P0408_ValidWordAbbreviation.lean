import Lproofs.Schemes.Fold

/-! @lc 408 | name:Valid Word Abbreviation | scheme:fold | family:string-other | complexity:O(n) |
    source:https://leetcode.com/problems/valid-word-abbreviation/

    An abbreviation interleaves literal characters with skip-counts. Parsed into tokens, the
    greedy two-pointer matcher accepts a word exactly when the word *conforms* to the pattern:
    each literal accepts the next character, each skip-count consumes exactly that many characters. -/

namespace LC.P0408
open Interview.Patterns

/-- An abbreviation token: a literal character or a skip-count. -/
inductive Tok where
  | lit : Char → Tok
  | skip : ℕ → Tok

/-- Greedy two-pointer matcher. -/
def accepts : List Tok → List Char → Bool
  | [], w => w.isEmpty
  | Tok.lit c :: ts, w =>
    match w with
    | x :: w' => (x == c) && accepts ts w'
    | [] => false
  | Tok.skip k :: ts, w => if k ≤ w.length then accepts ts (w.drop k) else false

/-- The word conforms to the token pattern. -/
def Conforms : List Tok → List Char → Prop
  | [], w => w = []
  | Tok.lit c :: ts, w => ∃ w', w = c :: w' ∧ Conforms ts w'
  | Tok.skip k :: ts, w => ∃ pre w', w = pre ++ w' ∧ pre.length = k ∧ Conforms ts w'

/-- One streaming step over the tokens, carrying the remaining (unconsumed) word suffix as the
    two-pointer state; `none` once a literal mismatches or a skip overruns. -/
def stepTok (st : Option (List Char)) : Tok → Option (List Char)
  | Tok.lit c => st.bind (fun w => match w with | x :: w' => if x = c then some w' else none | [] => none)
  | Tok.skip k => st.bind (fun w => if k ≤ w.length then some (w.drop k) else none)

/-- The matcher as a left fold over the tokens: thread the remaining word, accept iff it is fully
    consumed at the end. -/
def sol (toks : List Tok) (w : List Char) : Bool :=
  match toks.foldl stepTok (some w) with
  | some w' => w'.isEmpty
  | none => false

/-- SCHEME (fold): the matcher is a single left fold over the tokens, carrying the remaining-word
    suffix (the two-pointer position) as its bounded accumulator. -/
theorem cls (w : List Char) : IsFold (fun toks : List Tok => toks.foldl stepTok (some w)) :=
  ⟨stepTok, some w, fun _ => rfl⟩

/-- Once the fold fails (`none`), it stays `none`. -/
theorem foldl_none (ts : List Tok) : ts.foldl stepTok none = none := by
  induction ts with
  | nil => rfl
  | cons hd tl ih => cases hd <;> simpa [stepTok] using ih

/-- The fold-form matcher agrees with the structural matcher `accepts`. -/
theorem sol_eq : ∀ (toks : List Tok) (w : List Char), sol toks w = accepts toks w := by
  intro toks
  induction toks with
  | nil => intro w; simp [sol, accepts, List.isEmpty_iff]
  | cons hd ts ih =>
    intro w
    cases hd with
    | lit c =>
      cases w with
      | nil => simp [sol, accepts, stepTok, List.foldl_cons, foldl_none]
      | cons x w' =>
        by_cases hx : x = c
        · subst hx
          simp only [sol, accepts, stepTok, List.foldl_cons, Option.bind_some, if_pos rfl,
            beq_self_eq_true, Bool.true_and]
          exact ih w'
        · simp only [sol, accepts, stepTok, List.foldl_cons, Option.bind_some, if_neg hx,
            foldl_none, beq_false_of_ne hx, Bool.false_and]
    | skip k =>
      by_cases hk : k ≤ w.length
      · simp only [sol, accepts, stepTok, List.foldl_cons, Option.bind_some, if_pos hk]
        exact ih (w.drop k)
      · simp only [sol, accepts, stepTok, List.foldl_cons, Option.bind_some, if_neg hk,
          foldl_none]

/-- CORRECT: the matcher accepts exactly the words conforming to the abbreviation pattern. -/
theorem corr : ∀ (toks : List Tok) (w : List Char), accepts toks w = true ↔ Conforms toks w := by
  intro toks
  induction toks with
  | nil => intro w; simp [accepts, Conforms, List.isEmpty_iff]
  | cons hd ts ih =>
    intro w
    cases hd with
    | lit c =>
      cases w with
      | nil => simp [accepts, Conforms]
      | cons x w' =>
        simp only [accepts, Conforms, Bool.and_eq_true, beq_iff_eq, List.cons.injEq, ih]
        constructor
        · rintro ⟨rfl, hc⟩; exact ⟨w', ⟨rfl, rfl⟩, hc⟩
        · rintro ⟨w'', ⟨rfl, rfl⟩, hc⟩; exact ⟨rfl, hc⟩
    | skip k =>
      simp only [accepts, Conforms]
      by_cases hk : k ≤ w.length
      · rw [if_pos hk, ih (w.drop k)]
        constructor
        · intro hc
          exact ⟨w.take k, w.drop k, (List.take_append_drop k w).symm,
            by rw [List.length_take]; omega, hc⟩
        · rintro ⟨pre, w', rfl, hplen, hconf⟩
          rwa [List.drop_left' hplen]
      · rw [if_neg hk]
        simp only [Bool.false_eq_true, false_iff]
        rintro ⟨pre, w', rfl, hplen, _⟩
        simp only [List.length_append] at hk; omega

end LC.P0408
