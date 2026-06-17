import Lproofs.Schemes.Fold

/-! @lc 797 | name:All Paths From Source to Target | scheme:dp | family:graph-enumeration |
    complexity:exponential | source:https://leetcode.com/problems/all-paths-source-target/

    Enumerate every path from node `u` to `target` in a DAG. The bookkeeping label is "graph", but the
    accepted algorithm is DFS backtracking — a recursive decomposition: a path from `u` is `u`
    prepended to a path from one of its successors `w ∈ g u`. NON-VACUITY: we prove every enumerated
    list is a genuine `u → target` path (starts at `u`, ends at `target`, consecutive nodes are edges).
    We certify the decomposition + path-soundness, not completeness/optimality. -/

namespace LC.P0797

/-- All paths from `u` to `target`; `fuel` bounds the depth. -/
def paths : ℕ → (ℕ → List ℕ) → ℕ → ℕ → List (List ℕ)
  | 0, _, _, _ => []
  | fuel + 1, g, u, target =>
    if u = target then [[u]]
    else (g u).flatMap (fun w => (paths fuel g w target).map (u :: ·))

/-- A list is a path in `g` when each consecutive pair is an edge. -/
def isPath (g : ℕ → List ℕ) : List ℕ → Prop
  | [] => True
  | [_] => True
  | x :: y :: rest => y ∈ g x ∧ isPath g (y :: rest)

/-- SCHEME (dp / recursive decomposition): a path from `u` is `u` prepended to a path from a
    successor `w ∈ g u` (or the trivial path when `u = target`). -/
theorem cls (fuel : ℕ) (g : ℕ → List ℕ) (u target : ℕ) :
    paths (fuel + 1) g u target =
      (if u = target then [[u]]
       else (g u).flatMap (fun w => (paths fuel g w target).map (u :: ·))) :=
  rfl

/-- NON-VACUITY (path-soundness): every enumerated list starts at `u`, ends at `target`, and is a
    genuine path of `g`. -/
theorem corr : ∀ (fuel : ℕ) (g : ℕ → List ℕ) (u target : ℕ) (out : List ℕ),
    out ∈ paths fuel g u target →
      out.head? = some u ∧ out.getLast? = some target ∧ isPath g out := by
  intro fuel
  induction fuel with
  | zero => intro g u target out hmem; simp [paths] at hmem
  | succ f ih =>
    intro g u target out hmem
    rw [cls] at hmem
    split at hmem
    · rename_i hut
      simp only [List.mem_singleton] at hmem
      subst hmem
      exact ⟨rfl, by simp [hut], trivial⟩
    · rw [List.mem_flatMap] at hmem
      obtain ⟨w, hw, hmem2⟩ := hmem
      rw [List.mem_map] at hmem2
      obtain ⟨out', hout', rfl⟩ := hmem2
      obtain ⟨hh, hl, hp⟩ := ih g w target out' hout'
      obtain ⟨rest, rfl⟩ : ∃ rest, out' = w :: rest := by
        cases out' with
        | nil => simp at hh
        | cons a as => simp only [List.head?_cons, Option.some.injEq] at hh; exact ⟨as, by rw [hh]⟩
      refine ⟨rfl, ?_, hw, hp⟩
      rw [List.getLast?_cons_cons]
      exact hl

end LC.P0797
