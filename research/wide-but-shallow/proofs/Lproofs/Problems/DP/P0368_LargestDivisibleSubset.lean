import Lproofs.Schemes.Fold

/-! @lc 368 | name:Largest Divisible Subset | scheme:dp | family:dp-linear | complexity:O(n²) |
    source:https://leetcode.com/problems/largest-divisible-subset/

    Over the sorted input, the DP keeps, for each element, the longest divisible chain ending at it
    (a "chain" being a list in which each element divides the next). Streaming left to right we carry
    a table of `(lastValue, witnessChain)` entries; the answer is the longest chain's length.
    Soundness (achievability): the reported length is realized by an actual divisible subset of the
    input — every entry's witness is a genuine divisible chain drawn from the input. -/

namespace LC.P0368
open Interview.Patterns

/-- A divisible chain: each element divides the next. -/
def DivChain : List ℤ → Prop
  | [] => True
  | [_] => True
  | x :: y :: r => x ∣ y ∧ DivChain (y :: r)

/-- The pick functional used by `best`: keep the longer of `acc` and a divisor-ending witness. -/
def pick (x : ℤ) (acc : List ℤ) (e : ℤ × List ℤ) : List ℤ :=
  if e.1 ∣ x ∧ acc.length < e.2.length then e.2 else acc

/-- The longest witness chain among table entries whose last value divides `x`, extended by `x`. -/
def best (table : List (ℤ × List ℤ)) (x : ℤ) : List ℤ := (table.foldl (pick x) []) ++ [x]

/-- One streaming step: record `x` with its best divisible chain. -/
def step (table : List (ℤ × List ℤ)) (x : ℤ) : List (ℤ × List ℤ) := (x, best table x) :: table

/-- The answer: the length of the longest chain recorded. -/
def sol (a : List ℤ) : ℕ := (a.foldl step []).foldl (fun m e => max m e.2.length) 0

/-- SCHEME (dp = streaming fold): the witness table is built by a single left fold. -/
theorem cls : IsFold (fun a : List ℤ => a.foldl step []) := ⟨step, [], fun _ => rfl⟩

/-- Appending one element that the chain's last value divides preserves `DivChain`. -/
theorem divChain_concat : ∀ {c : List ℤ} {x : ℤ}, DivChain c → (c = [] ∨ c.getLastD 0 ∣ x) →
    DivChain (c ++ [x])
  | [], _, _, _ => trivial
  | [a], x, _, h => by
      rcases h with h | h
      · simp at h
      · exact ⟨by simpa [List.getLastD] using h, trivial⟩
  | a :: b :: r, x, ⟨hab, hrest⟩, h =>
      ⟨hab, divChain_concat hrest (Or.inr (by simpa [List.getLastD_cons] using h))⟩

/-- Invariant: every recorded entry is a nonempty divisible chain drawn from the seen prefix, whose
    last value is the recorded key. -/
def Valid (seen : List ℤ) (e : ℤ × List ℤ) : Prop :=
  DivChain e.2 ∧ e.2 ≠ [] ∧ e.2.getLastD 0 = e.1 ∧ ∀ z ∈ e.2, z ∈ seen

/-- A "good accumulator" for the selection fold: empty, or a divisor-ending chain from the prefix. -/
def Good (seen : List ℤ) (x : ℤ) (acc : List ℤ) : Prop :=
  acc = [] ∨ (DivChain acc ∧ acc.getLastD 0 ∣ x ∧ ∀ z ∈ acc, z ∈ seen)

/-- The selection fold preserves `Good`, given all table entries are `Valid`. -/
theorem foldl_pick_good : ∀ (table : List (ℤ × List ℤ)) (acc : List ℤ) (seen : List ℤ) (x : ℤ),
    Good seen x acc → (∀ e ∈ table, Valid seen e) → Good seen x (table.foldl (pick x) acc)
  | [], acc, _, _, hacc, _ => hacc
  | e :: es, acc, seen, x, hacc, hV => by
      rw [List.foldl_cons]
      refine foldl_pick_good es (pick x acc e) seen x ?_ (fun e' he' => hV e' (List.mem_cons_of_mem _ he'))
      obtain ⟨hdc, _, hgl, hmem⟩ := hV e (List.mem_cons_self ..)
      unfold pick
      by_cases hc : e.1 ∣ x ∧ acc.length < e.2.length
      · rw [if_pos hc]; exact Or.inr ⟨hdc, hgl ▸ hc.1, hmem⟩
      · rw [if_neg hc]; exact hacc

/-- The step preserves the invariant: the new entry `(x, best …)` is `Valid` for `seen ++ [x]`. -/
theorem inv_step (seen : List ℤ) (table : List (ℤ × List ℤ)) (x : ℤ)
    (hV : ∀ e ∈ table, Valid seen e) : ∀ e ∈ step table x, Valid (seen ++ [x]) e := by
  intro e he
  have hmono : ∀ e' ∈ table, Valid (seen ++ [x]) e' := by
    intro e' he'; obtain ⟨a, b, c, d⟩ := hV e' he'
    exact ⟨a, b, c, fun z hz => List.mem_append_left _ (d z hz)⟩
  rcases List.mem_cons.mp he with rfl | hold
  · -- the fresh entry
    have hgood : Good seen x (table.foldl (pick x) []) :=
      foldl_pick_good table [] seen x (Or.inl rfl) hV
    refine ⟨?_, by simp [best], ?_, ?_⟩
    · rcases hgood with h | ⟨hdc, hdvd, _⟩
      · simp only [best, h, List.nil_append]; exact trivial
      · exact divChain_concat hdc (Or.inr hdvd)
    · simp only [best, List.getLastD_eq_getLast?, List.getLast?_concat, Option.getD_some]
    · intro z hz
      simp only [best, List.mem_append, List.mem_singleton] at hz
      rcases hz with hz | rfl
      · rcases hgood with h | ⟨_, _, hmem⟩
        · rw [h] at hz; simp at hz
        · exact List.mem_append_left _ (hmem z hz)
      · exact List.mem_append_right _ (by simp)
  · exact hmono e hold

/-- Folding the step over the input preserves the invariant. -/
theorem inv_fold : ∀ (a seen : List ℤ) (table : List (ℤ × List ℤ)),
    (∀ e ∈ table, Valid seen e) → ∀ e ∈ a.foldl step table, Valid (seen ++ a) e
  | [], seen, table, hV => by simpa using hV
  | x :: a, seen, table, hV => by
      have := inv_fold a (seen ++ [x]) (step table x) (inv_step seen table x hV)
      simpa [List.append_assoc] using this

/-- The `max`-fold over a table is `0` or achieved by one of its entries. -/
theorem maxLen_achieved : ∀ (l : List (ℤ × List ℤ)) (m₀ : ℕ),
    l.foldl (fun m e => max m e.2.length) m₀ = m₀ ∨
    ∃ e ∈ l, l.foldl (fun m e => max m e.2.length) m₀ = e.2.length
  | [], m₀ => Or.inl rfl
  | e :: es, m₀ => by
      rw [List.foldl_cons]
      rcases maxLen_achieved es (max m₀ e.2.length) with h | ⟨e', he', h⟩
      · rw [h]
        rcases max_choice m₀ e.2.length with hc | hc
        · exact Or.inl hc
        · exact Or.inr ⟨e, List.mem_cons_self .., hc⟩
      · exact Or.inr ⟨e', List.mem_cons_of_mem _ he', h⟩

/-- CORRECT (achievability): the reported length is realized by a genuine divisible subset of the
    input — a divisible chain whose elements all occur in `a`. -/
theorem corr (a : List ℤ) :
    ∃ c : List ℤ, DivChain c ∧ (∀ z ∈ c, z ∈ a) ∧ c.length = sol a := by
  have hV : ∀ e ∈ a.foldl step [], Valid a e := by
    have := inv_fold a [] [] (by simp)
    simpa using this
  rcases maxLen_achieved (a.foldl step []) 0 with h0 | ⟨e, he, h⟩
  · exact ⟨[], trivial, by simp, by simp [sol, h0]⟩
  · obtain ⟨hdc, _, _, hmem⟩ := hV e he
    exact ⟨e.2, hdc, hmem, by rw [sol, h]⟩

end LC.P0368
