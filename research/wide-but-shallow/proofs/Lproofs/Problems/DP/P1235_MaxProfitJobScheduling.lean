import Lproofs.Schemes.Fold

/-! @lc 1235 | name:Maximum Profit in Job Scheduling | scheme:dp | family:dp-interval |
    complexity:O(n²) | source:https://leetcode.com/problems/maximum-profit-in-job-scheduling/

    Each job is `(start, end, profit)`. A valid schedule is a set of pairwise non-overlapping jobs;
    over the end-sorted input this is a chain in which each job starts no earlier than the previous
    one ends. The DP keeps, per job, the best-profit schedule ending with it; streaming left to right
    we carry a table of `(lastEnd, witnessSchedule)` entries and report the best total profit.
    Soundness (achievability): the reported profit is realized by an actual non-overlapping schedule
    drawn from the input. -/

namespace LC.P1235
open Interview.Patterns

abbrev Job := ℤ × ℤ × ℤ

/-- A compatible schedule: each job's end is no later than the next job's start. -/
def Compat : List Job → Prop
  | [] => True
  | [_] => True
  | a :: b :: r => a.2.1 ≤ b.1 ∧ Compat (b :: r)

/-- Total profit of a schedule. -/
def value (c : List Job) : ℤ := (c.map (·.2.2)).sum

/-- The pick functional: keep the higher-profit of `acc` and a compatible witness schedule. -/
def pick (x : Job) (acc : List Job) (e : ℤ × List Job) : List Job :=
  if e.1 ≤ x.1 ∧ value acc < value e.2 then e.2 else acc

/-- The best compatible schedule among table entries ending no later than `x`'s start, plus `x`. -/
def best (table : List (ℤ × List Job)) (x : Job) : List Job := (table.foldl (pick x) []) ++ [x]

/-- One streaming step: record `x` with its best schedule, keyed by `x`'s end. -/
def step (table : List (ℤ × List Job)) (x : Job) : List (ℤ × List Job) := (x.2.1, best table x) :: table

/-- The answer: the best total profit recorded. -/
def sol (jobs : List Job) : ℤ := (jobs.foldl step []).foldl (fun m e => max m (value e.2)) 0

/-- SCHEME (dp = streaming fold): the witness table is built by a single left fold. -/
theorem cls : IsFold (fun jobs : List Job => jobs.foldl step []) := ⟨step, [], fun _ => rfl⟩

theorem value_concat (c : List Job) (x : Job) : value (c ++ [x]) = value c + x.2.2 := by
  simp [value]

/-- Appending one job that starts after the schedule's last end preserves `Compat`. -/
theorem compat_concat : ∀ {c : List Job} {x : Job}, Compat c →
    (c = [] ∨ (c.getLastD (0, 0, 0)).2.1 ≤ x.1) → Compat (c ++ [x])
  | [], _, _, _ => trivial
  | [a], x, _, h => by
      rcases h with h | h
      · simp at h
      · exact ⟨by simpa [List.getLastD] using h, trivial⟩
  | a :: b :: r, x, ⟨hab, hrest⟩, h =>
      ⟨hab, compat_concat hrest (Or.inr (by simpa [List.getLastD_cons] using h))⟩

/-- Invariant: each recorded entry is a nonempty compatible schedule drawn from the seen prefix,
    whose last job's end is the recorded key. -/
def Valid (seen : List Job) (e : ℤ × List Job) : Prop :=
  Compat e.2 ∧ e.2 ≠ [] ∧ (e.2.getLastD (0, 0, 0)).2.1 = e.1 ∧ ∀ z ∈ e.2, z ∈ seen

/-- A "good accumulator" for the selection fold: empty, or a compatible schedule from the prefix
    whose last end clears `x`'s start. -/
def Good (seen : List Job) (x : Job) (acc : List Job) : Prop :=
  acc = [] ∨ (Compat acc ∧ (acc.getLastD (0, 0, 0)).2.1 ≤ x.1 ∧ ∀ z ∈ acc, z ∈ seen)

theorem foldl_pick_good : ∀ (table : List (ℤ × List Job)) (acc : List Job) (seen : List Job) (x : Job),
    Good seen x acc → (∀ e ∈ table, Valid seen e) → Good seen x (table.foldl (pick x) acc)
  | [], acc, _, _, hacc, _ => hacc
  | e :: es, acc, seen, x, hacc, hV => by
      rw [List.foldl_cons]
      refine foldl_pick_good es (pick x acc e) seen x ?_ (fun e' he' => hV e' (List.mem_cons_of_mem _ he'))
      obtain ⟨hdc, _, hgl, hmem⟩ := hV e (List.mem_cons_self ..)
      unfold pick
      by_cases hc : e.1 ≤ x.1 ∧ value acc < value e.2
      · rw [if_pos hc]; exact Or.inr ⟨hdc, hgl ▸ hc.1, hmem⟩
      · rw [if_neg hc]; exact hacc

theorem inv_step (seen : List Job) (table : List (ℤ × List Job)) (x : Job)
    (hV : ∀ e ∈ table, Valid seen e) : ∀ e ∈ step table x, Valid (seen ++ [x]) e := by
  intro e he
  have hmono : ∀ e' ∈ table, Valid (seen ++ [x]) e' := by
    intro e' he'; obtain ⟨a, b, c, d⟩ := hV e' he'
    exact ⟨a, b, c, fun z hz => List.mem_append_left _ (d z hz)⟩
  rcases List.mem_cons.mp he with rfl | hold
  · have hgood : Good seen x (table.foldl (pick x) []) :=
      foldl_pick_good table [] seen x (Or.inl rfl) hV
    refine ⟨?_, by simp [best], ?_, ?_⟩
    · rcases hgood with h | ⟨hdc, hcmp, _⟩
      · simp only [best, h, List.nil_append]; exact trivial
      · exact compat_concat hdc (Or.inr hcmp)
    · simp only [best, List.getLastD_eq_getLast?, List.getLast?_concat, Option.getD_some]
    · intro z hz
      simp only [best, List.mem_append, List.mem_singleton] at hz
      rcases hz with hz | rfl
      · rcases hgood with h | ⟨_, _, hmem⟩
        · rw [h] at hz; simp at hz
        · exact List.mem_append_left _ (hmem z hz)
      · exact List.mem_append_right _ (by simp)
  · exact hmono e hold

theorem inv_fold : ∀ (a seen : List Job) (table : List (ℤ × List Job)),
    (∀ e ∈ table, Valid seen e) → ∀ e ∈ a.foldl step table, Valid (seen ++ a) e
  | [], seen, table, hV => by simpa using hV
  | x :: a, seen, table, hV => by
      have := inv_fold a (seen ++ [x]) (step table x) (inv_step seen table x hV)
      simpa [List.append_assoc] using this

theorem maxVal_achieved : ∀ (l : List (ℤ × List Job)) (m₀ : ℤ),
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

/-- CORRECT (achievability): the reported profit is realized by a genuine non-overlapping schedule
    drawn from the input jobs. -/
theorem corr (jobs : List Job) :
    ∃ c : List Job, Compat c ∧ (∀ z ∈ c, z ∈ jobs) ∧ value c = sol jobs := by
  have hV : ∀ e ∈ jobs.foldl step [], Valid jobs e := by
    have := inv_fold jobs [] [] (by simp)
    simpa using this
  rcases maxVal_achieved (jobs.foldl step []) 0 with h0 | ⟨e, he, h⟩
  · refine ⟨[], trivial, by simp, ?_⟩
    rw [sol, h0]; simp [value]
  · obtain ⟨hdc, _, _, hmem⟩ := hV e he
    exact ⟨e.2, hdc, hmem, by rw [sol, h]⟩

end LC.P1235
