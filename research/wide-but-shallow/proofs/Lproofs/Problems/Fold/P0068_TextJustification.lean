import Lproofs.Schemes.Fold

/-! @lc 68 | name:Text Justification | scheme:fold | family:greedy | complexity:O(n) |
    source:https://leetcode.com/problems/text-justification/

    Pack words greedily into lines: keep adding the next word to the current line while it still
    fits within `maxW` (counting one mandatory space between words), and start a new line otherwise.
    The space-distribution that fully justifies each line is mechanical formatting layered on top of
    this grouping (not modelled here). The grouping is a single streaming left fold over the words,
    carrying `(completed lines, current line, current minimal width)` — `cls` certifies that fold.
    Its core correctness, which `corr` proves, is that *every emitted line fits*: its words plus the
    mandatory single spaces never exceed `maxW` (given each word itself fits). We model a word by its
    length (`ℕ`); the minimal width of a line `L` is `L.sum + (L.length − 1)`.

        word ──▶ cur empty?          ──yes──▶ start line [word]
                 │ no
                 ▼
            w + 1 + word ≤ maxW ? ──yes──▶ append to current line
                 │ no
                 ▼
            close current line, start new line [word] -/

namespace LC.P0068
open Interview.Patterns

/-- Packing state: completed lines, current line, current minimal width. -/
abbrev Acc := List (List ℕ) × List ℕ × ℕ

/-- Greedy step: extend the current line if the next word fits, else close it and start a new one. -/
def step (maxW : ℕ) : Acc → ℕ → Acc
  | (lines, [], _), word => (lines, [word], word)
  | (lines, cur, w), word =>
    if w + 1 + word ≤ maxW then (lines, cur ++ [word], w + 1 + word)
    else (lines ++ [cur], [word], word)

/-- The line grouping returned to the formatter (flush the last open line). -/
def sol (maxW : ℕ) (words : List ℕ) : List (List ℕ) :=
  match words.foldl (step maxW) ([], [], 0) with
  | (lines, [], _) => lines
  | (lines, cur, _) => lines ++ [cur]

/-- A line `L` fits when its words plus the mandatory single spaces are within `maxW`. -/
def fits (maxW : ℕ) (L : List ℕ) : Prop := L.sum + (L.length - 1) ≤ maxW

/-- Loop invariant: every completed line fits, and the open line's tracked width is its true minimal
    width and is within `maxW`. -/
def Good (maxW : ℕ) : Acc → Prop
  | (lines, cur, w) =>
    (∀ L ∈ lines, fits maxW L) ∧ (cur = [] ∨ (cur.sum + (cur.length - 1) = w ∧ w ≤ maxW))

/-- SCHEME (fold): the greedy grouping is a single streaming left fold over the words. -/
theorem cls (maxW : ℕ) : IsFold (fun words : List ℕ => words.foldl (step maxW) ([], [], 0)) :=
  ⟨step maxW, ([], [], 0), fun _ => rfl⟩

/-- The step preserves the invariant when the incoming word itself fits. -/
theorem good_step (maxW : ℕ) (s : Acc) (word : ℕ) (hw : word ≤ maxW) (hs : Good maxW s) :
    Good maxW (step maxW s word) := by
  obtain ⟨lines, cur, w⟩ := s
  obtain ⟨hlines, hcur⟩ := hs
  cases cur with
  | nil =>
    simp only [step, Good]
    exact ⟨hlines, Or.inr ⟨by simp, hw⟩⟩
  | cons c cs =>
    rcases hcur with hc | ⟨hlw, hle⟩
    · exact absurd hc (by simp)
    · simp only [step]
      by_cases hfit : w + 1 + word ≤ maxW
      · rw [if_pos hfit]
        refine ⟨hlines, Or.inr ⟨?_, hfit⟩⟩
        simp only [List.sum_append, List.length_append, List.sum_cons, List.length_cons,
          List.sum_nil, List.length_nil] at hlw ⊢
        omega
      · rw [if_neg hfit]
        refine ⟨?_, Or.inr ⟨by simp, hw⟩⟩
        intro L hL
        rw [List.mem_append, List.mem_singleton] at hL
        rcases hL with h | rfl
        · exact hlines L h
        · show (c :: cs).sum + ((c :: cs).length - 1) ≤ maxW
          rw [hlw]; exact hle

/-- Folding the greedy step over all words preserves the invariant. -/
theorem good_fold (maxW : ℕ) (words : List ℕ) (hw : ∀ word ∈ words, word ≤ maxW) :
    Good maxW (words.foldl (step maxW) ([], [], 0)) := by
  suffices H : ∀ (ws : List ℕ) (s : Acc), (∀ word ∈ ws, word ≤ maxW) → Good maxW s →
      Good maxW (ws.foldl (step maxW) s) by
    exact H words _ hw ⟨by simp, Or.inl rfl⟩
  intro ws
  induction ws with
  | nil => intro s _ hs; exact hs
  | cons a as ih =>
    intro s hwa hs
    rw [List.foldl_cons]
    exact ih _ (fun w hw => hwa w (List.mem_cons_of_mem _ hw))
      (good_step maxW s a (hwa a (by simp)) hs)

/-- CORRECT: every line the grouping emits fits within `maxW` (each word assumed to fit). -/
theorem corr (maxW : ℕ) (words : List ℕ) (hw : ∀ word ∈ words, word ≤ maxW) :
    ∀ L ∈ sol maxW words, fits maxW L := by
  have hg := good_fold maxW words hw
  unfold sol
  generalize hfold : words.foldl (step maxW) ([], [], 0) = s at hg ⊢
  obtain ⟨lines, cur, w⟩ := s
  obtain ⟨hlines, hcur⟩ := hg
  cases cur with
  | nil => intro L hL; exact hlines L hL
  | cons c cs =>
    intro L hL
    rw [List.mem_append, List.mem_singleton] at hL
    rcases hL with h | rfl
    · exact hlines L h
    · rcases hcur with hc | ⟨hlw, hle⟩
      · exact absurd hc (by simp)
      · show (c :: cs).sum + ((c :: cs).length - 1) ≤ maxW
        rw [hlw]; exact hle

end LC.P0068
