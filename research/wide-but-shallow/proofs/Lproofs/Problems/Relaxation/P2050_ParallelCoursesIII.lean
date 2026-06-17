import Lproofs.Schemes.Fold

/-! @lc 2050 | name:Parallel Courses III | scheme:relaxation | family:topo-sort | complexity:O(V+E) |
    source:https://leetcode.com/problems/parallel-courses-iii/

    Each course has a duration and may start only after its prerequisites finish; the minimum time
    to finish everything is the longest dependency chain by total duration (the critical path). The
    DP keeps, per course, the best-duration chain ending at it; streaming left to right we carry a
    table of `(lastId, witnessChain)` entries and report the longest total duration. Soundness
    (achievability): the reported time is realized by an actual prerequisite chain of the input. -/

namespace LC.P2050
open Interview.Patterns
open Classical

noncomputable section

abbrev Course := ℤ × ℤ  -- (id, duration)

variable (R : ℤ → ℤ → Prop)

/-- A dependency chain: each course is a (direct) prerequisite of the next. -/
def Chain : List Course → Prop
  | [] => True
  | [_] => True
  | a :: b :: r => R a.1 b.1 ∧ Chain (b :: r)

/-- Total duration of a chain. -/
def value (c : List Course) : ℤ := (c.map (·.2)).sum

/-- The pick functional: keep the longer-duration of `acc` and a prerequisite-ending witness chain. -/
def pick (x : Course) (acc : List Course) (e : ℤ × List Course) : List Course :=
  if R e.1 x.1 ∧ value acc < value e.2 then e.2 else acc

/-- The best prerequisite chain among table entries that precede `x`, extended by `x`. -/
def best (table : List (ℤ × List Course)) (x : Course) : List Course :=
  (table.foldl (pick R x) []) ++ [x]

/-- One streaming step: record `x` with its best chain, keyed by `x`'s id. -/
def step (table : List (ℤ × List Course)) (x : Course) : List (ℤ × List Course) :=
  (x.1, best R table x) :: table

/-- The answer: the longest total duration recorded. -/
def sol (courses : List Course) : ℤ :=
  (courses.foldl (step R) []).foldl (fun m e => max m (value e.2)) 0

/-- SCHEME (relaxation / dp): the witness table is built by a single left fold. -/
theorem cls : IsFold (fun courses : List Course => courses.foldl (step R) []) := ⟨step R, [], fun _ => rfl⟩

theorem value_concat (c : List Course) (x : Course) : value (c ++ [x]) = value c + x.2 := by
  simp [value]

/-- Appending one course that the chain's last course precedes preserves `Chain`. -/
theorem chain_concat : ∀ {c : List Course} {x : Course}, Chain R c →
    (c = [] ∨ R (c.getLastD (0, 0)).1 x.1) → Chain R (c ++ [x])
  | [], _, _, _ => trivial
  | [a], x, _, h => by
      rcases h with h | h
      · simp at h
      · exact ⟨by simpa [List.getLastD] using h, trivial⟩
  | a :: b :: r, x, ⟨hab, hrest⟩, h =>
      ⟨hab, chain_concat hrest (Or.inr (by simpa [List.getLastD_cons] using h))⟩

/-- Invariant: each entry is a nonempty dependency chain from the seen prefix, keyed by its last id. -/
def Valid (seen : List Course) (e : ℤ × List Course) : Prop :=
  Chain R e.2 ∧ e.2 ≠ [] ∧ (e.2.getLastD (0, 0)).1 = e.1 ∧ ∀ z ∈ e.2, z ∈ seen

/-- A "good accumulator": empty, or a dependency chain from the prefix whose last course precedes `x`. -/
def Good (seen : List Course) (x : Course) (acc : List Course) : Prop :=
  acc = [] ∨ (Chain R acc ∧ R (acc.getLastD (0, 0)).1 x.1 ∧ ∀ z ∈ acc, z ∈ seen)

theorem foldl_pick_good : ∀ (table : List (ℤ × List Course)) (acc : List Course) (seen : List Course)
    (x : Course), Good R seen x acc → (∀ e ∈ table, Valid R seen e) →
    Good R seen x (table.foldl (pick R x) acc)
  | [], acc, _, _, hacc, _ => hacc
  | e :: es, acc, seen, x, hacc, hV => by
      rw [List.foldl_cons]
      refine foldl_pick_good es (pick R x acc e) seen x ?_ (fun e' he' => hV e' (List.mem_cons_of_mem _ he'))
      obtain ⟨hdc, _, hgl, hmem⟩ := hV e (List.mem_cons_self ..)
      unfold pick
      by_cases hc : R e.1 x.1 ∧ value acc < value e.2
      · rw [if_pos hc]; exact Or.inr ⟨hdc, hgl ▸ hc.1, hmem⟩
      · rw [if_neg hc]; exact hacc

theorem inv_step (seen : List Course) (table : List (ℤ × List Course)) (x : Course)
    (hV : ∀ e ∈ table, Valid R seen e) : ∀ e ∈ step R table x, Valid R (seen ++ [x]) e := by
  intro e he
  have hmono : ∀ e' ∈ table, Valid R (seen ++ [x]) e' := by
    intro e' he'; obtain ⟨a, b, c, d⟩ := hV e' he'
    exact ⟨a, b, c, fun z hz => List.mem_append_left _ (d z hz)⟩
  rcases List.mem_cons.mp he with rfl | hold
  · have hgood : Good R seen x (table.foldl (pick R x) []) :=
      foldl_pick_good R table [] seen x (Or.inl rfl) hV
    refine ⟨?_, by simp [best], ?_, ?_⟩
    · rcases hgood with h | ⟨hdc, hcmp, _⟩
      · simp only [best, h, List.nil_append]; exact trivial
      · exact chain_concat R hdc (Or.inr hcmp)
    · simp only [best, List.getLastD_eq_getLast?, List.getLast?_concat, Option.getD_some]
    · intro z hz
      simp only [best, List.mem_append, List.mem_singleton] at hz
      rcases hz with hz | rfl
      · rcases hgood with h | ⟨_, _, hmem⟩
        · rw [h] at hz; simp at hz
        · exact List.mem_append_left _ (hmem z hz)
      · exact List.mem_append_right _ (by simp)
  · exact hmono e hold

theorem inv_fold : ∀ (a seen : List Course) (table : List (ℤ × List Course)),
    (∀ e ∈ table, Valid R seen e) → ∀ e ∈ a.foldl (step R) table, Valid R (seen ++ a) e
  | [], seen, table, hV => by simpa using hV
  | x :: a, seen, table, hV => by
      have := inv_fold a (seen ++ [x]) (step R table x) (inv_step R seen table x hV)
      simpa [List.append_assoc] using this

theorem maxVal_achieved : ∀ (l : List (ℤ × List Course)) (m₀ : ℤ),
    l.foldl (fun m e => max m (value e.2)) m₀ = m₀ ∨
    ∃ e ∈ l, l.foldl (fun m e => max m (value e.2)) m₀ = value e.2
  | [], m₀ => Or.inl rfl
  | e :: es, m₀ => by
      rw [List.foldl_cons]
      rcases maxVal_achieved es (max m₀ (value e.2)) with h | ⟨e', he', h⟩
      · rw [h]
        rcases max_choice m₀ (value e.2) with hc | hc
        · exact Or.inl hc
        · exact Or.inr ⟨e, List.mem_cons_self .., hc⟩
      · exact Or.inr ⟨e', List.mem_cons_of_mem _ he', h⟩

/-- CORRECT (achievability): the reported critical-path time is realized by a genuine prerequisite
    chain drawn from the input courses. -/
theorem corr (courses : List Course) :
    ∃ c : List Course, Chain R c ∧ (∀ z ∈ c, z ∈ courses) ∧ value c = sol R courses := by
  have hV : ∀ e ∈ courses.foldl (step R) [], Valid R courses e := by
    have := inv_fold (R := R) courses [] [] (by simp)
    simpa using this
  rcases maxVal_achieved (courses.foldl (step R) []) 0 with h0 | ⟨e, he, h⟩
  · refine ⟨[], trivial, by simp, ?_⟩
    rw [sol, h0]; simp [value]
  · obtain ⟨hdc, _, _, hmem⟩ := hV e he
    exact ⟨e.2, hdc, hmem, by rw [sol, h]⟩

end

end LC.P2050
