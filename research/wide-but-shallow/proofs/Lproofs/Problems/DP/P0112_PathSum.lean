import Lproofs.Schemes.Fold

/-! @lc 112 | name:Path Sum | scheme:dp | family:tree-aggregate | complexity:O(n) |
    source:https://leetcode.com/problems/path-sum/ -/

namespace LC.P0112
open Interview.Patterns

/-- The list of all root-to-leaf path sums — a tree catamorphism. A node is a leaf when both
    children contribute no paths (`below = []`); then the only path is `[v]`. -/
def pathSums : Interview.Patterns.Tree ℤ → List ℤ
  | .leaf => []
  | .node l v r =>
    let below := pathSums l ++ pathSums r
    (if below = [] then [0] else below).map (v + ·)

/-- Editorial recursion: at a leaf, check the remainder; otherwise subtract and recurse. -/
def hasPathSum : Interview.Patterns.Tree ℤ → ℤ → Bool
  | .leaf, _ => false
  | .node l v r, t =>
    if pathSums l = [] ∧ pathSums r = [] then decide (v = t)
    else hasPathSum l (t - v) || hasPathSum r (t - v)

def sol (t : Interview.Patterns.Tree ℤ) (target : ℤ) : Bool := hasPathSum t target

/-- Spec: some root-to-leaf path sums to `target`. -/
def spec (t : Interview.Patterns.Tree ℤ) (target : ℤ) : Prop := target ∈ pathSums t

private theorem mem_map_add (v target : ℤ) (xs : List ℤ) :
    target ∈ xs.map (v + ·) ↔ target - v ∈ xs := by
  simp only [List.mem_map]
  constructor
  · rintro ⟨a, ha, rfl⟩; simpa using ha
  · intro h; exact ⟨target - v, h, by ring⟩

/-- SCHEME (dp/catamorphism) + CORRECT: the editorial decides membership in the catamorphic
    path-sum set. -/
theorem cls (t : Interview.Patterns.Tree ℤ) (target : ℤ) : sol t target = true ↔ target ∈ pathSums t := by
  unfold sol
  induction t generalizing target with
  | leaf => simp [hasPathSum, pathSums]
  | node l v r ihl ihr =>
    rw [hasPathSum]
    by_cases hb : pathSums l = [] ∧ pathSums r = []
    · simp [hb, pathSums, eq_comm]
    · have hne : ¬ (pathSums l ++ pathSums r = []) := by
        simp only [List.append_eq_nil_iff]; exact hb
      simp only [hb, if_false, Bool.or_eq_true, pathSums, hne, mem_map_add, List.mem_append]
      rw [ihl (target - v), ihr (target - v)]

theorem corr (t : Interview.Patterns.Tree ℤ) (target : ℤ) : sol t target = true ↔ spec t target := cls t target

end LC.P0112
