import Lproofs.Schemes.Fold

/-! @lc 151 | name:Reverse Words in a String | scheme:fold | family:string-other | complexity:O(n) |
    source:https://leetcode.com/problems/reverse-words-in-a-string/

    Split on spaces, drop empty pieces, reverse the word list, rejoin with single spaces.
    Correctness: the word list of the output is exactly the reversed word list of the input. -/

namespace LC.P0151
open Interview.Patterns

/-- Prepend a character to the first piece of a split. -/
def consFirst (c : Char) : List (List Char) → List (List Char)
  | [] => [[c]]
  | w :: ws => (c :: w) :: ws

/-- Split a character list on `' '`, keeping every piece (including empties). -/
def splitSp : List Char → List (List Char)
  | [] => [[]]
  | c :: rest => if c = ' ' then [] :: splitSp rest else consFirst c (splitSp rest)

/-- The words of a string: split on spaces, drop empties. -/
def words (s : List Char) : List (List Char) := (splitSp s).filter (fun w => !w.isEmpty)

/-- Rejoin words with single spaces. -/
def joinSp : List (List Char) → List Char
  | [] => []
  | [w] => w
  | w :: ws => w ++ ' ' :: joinSp ws

/-- Reverse the word order, rejoining with single spaces. -/
def reverseWords (s : List Char) : List Char := joinSp (words s).reverse

/-- The reduction returned to the caller. -/
def sol (s : List Char) : List Char := reverseWords s

/-- SCHEME (fold): the actual word-splitter `splitSp` is a genuine right fold over the characters —
    each step either opens a new piece (on a space) or prepends to the current head piece. This is
    exactly the splitter `sol` uses (via `words`), not a proxy. -/
theorem cls : IsRightFold splitSp :=
  ⟨fun c acc => if c = ' ' then ([] : List Char) :: acc else consFirst c acc, [[]], by
    intro s
    induction s with
    | nil => rfl
    | cons c rest ih => rw [List.foldr_cons, ← ih]; rfl⟩

theorem consFirst_no_space {c : Char} (hc : c ≠ ' ') {l : List (List Char)}
    (hl : ∀ v ∈ l, ' ' ∉ v) : ∀ w ∈ consFirst c l, ' ' ∉ w := by
  intro w hw
  cases l with
  | nil =>
    simp only [consFirst, List.mem_singleton] at hw; subst hw
    intro h; rw [List.mem_singleton] at h; exact hc h.symm
  | cons v vs =>
    simp only [consFirst, List.mem_cons] at hw
    rcases hw with rfl | hw
    · simp only [List.mem_cons, not_or]; exact ⟨Ne.symm hc, hl v (by simp)⟩
    · exact hl w (List.mem_cons_of_mem _ hw)

/-- Pieces of a split never contain the separator. -/
theorem splitSp_no_space : ∀ (s w : List Char), w ∈ splitSp s → ' ' ∉ w := by
  intro s
  induction s with
  | nil => intro w hw; simp only [splitSp, List.mem_singleton] at hw; subst hw; exact List.not_mem_nil
  | cons c rest ih =>
    intro w hw
    rw [splitSp] at hw
    rcases eq_or_ne c ' ' with hc | hc
    · rw [if_pos hc] at hw
      simp only [List.mem_cons] at hw
      rcases hw with rfl | hw
      · exact List.not_mem_nil
      · exact ih w hw
    · rw [if_neg hc] at hw; exact consFirst_no_space hc ih w hw

/-- A space-free word splits to itself. -/
theorem splitSp_space_free : ∀ (w : List Char), ' ' ∉ w → splitSp w = [w] := by
  intro w
  induction w with
  | nil => intro _; rfl
  | cons c w' ih =>
    intro hns
    have hc : c ≠ ' ' := fun h => hns (by rw [h]; exact List.mem_cons_self ..)
    have hns' : ' ' ∉ w' := fun h => hns (List.mem_cons_of_mem _ h)
    rw [splitSp, if_neg hc, ih hns']; rfl

/-- Appending `' ' :: t` to a space-free word splits cleanly. -/
theorem splitSp_append_space : ∀ (w : List Char), ' ' ∉ w → ∀ (t : List Char),
    splitSp (w ++ ' ' :: t) = w :: splitSp t := by
  intro w
  induction w with
  | nil => intro _ t; rw [List.nil_append, splitSp, if_pos rfl]
  | cons c w' ih =>
    intro hns t
    have hc : c ≠ ' ' := fun h => hns (by rw [h]; exact List.mem_cons_self ..)
    have hns' : ' ' ∉ w' := fun h => hns (List.mem_cons_of_mem _ h)
    rw [List.cons_append, splitSp, if_neg hc, ih hns' t]; rfl

/-- Rejoining nonempty space-free words and re-splitting recovers them. -/
theorem words_joinSp : ∀ (ws : List (List Char)),
    (∀ w ∈ ws, w ≠ [] ∧ ' ' ∉ w) → words (joinSp ws) = ws := by
  intro ws
  induction ws with
  | nil => intro _; rfl
  | cons w ws ih =>
    intro hall
    have hw := hall w (List.mem_cons_self ..)
    have hwne : (!w.isEmpty) = true := by
      simp only [Bool.not_eq_true', List.isEmpty_eq_false_iff]; exact hw.1
    cases ws with
    | nil =>
      show words w = [w]
      rw [words, splitSp_space_free w hw.2]
      simp only [List.filter_cons, hwne, if_true, List.filter_nil]
    | cons w2 ws2 =>
      have hrest : ∀ x ∈ w2 :: ws2, x ≠ [] ∧ ' ' ∉ x :=
        fun x hx => hall x (List.mem_cons_of_mem _ hx)
      show words (w ++ ' ' :: joinSp (w2 :: ws2)) = w :: w2 :: ws2
      rw [words, splitSp_append_space w hw.2]
      simp only [List.filter_cons, hwne, if_true]
      have := ih hrest
      rw [words] at this
      rw [this]

/-- CORRECT: the word list of the output is exactly the reversed word list of the input. -/
theorem corr (s : List Char) : words (sol s) = (words s).reverse := by
  unfold sol reverseWords
  apply words_joinSp
  intro w hw
  rw [List.mem_reverse, words, List.mem_filter] at hw
  refine ⟨?_, splitSp_no_space s w hw.1⟩
  simpa only [Bool.not_eq_true', List.isEmpty_eq_false_iff] using hw.2

end LC.P0151
