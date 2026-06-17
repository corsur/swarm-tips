import Lproofs.Schemes.Fold

/-! @lc 549 | name:Binary Tree Longest Consecutive Sequence II | scheme:dp | family:tree-dp |
    complexity:O(n) | source:https://leetcode.com/problems/binary-tree-longest-consecutive-sequence-ii/

    Longest path of consecutive values (±1 steps), possibly bending at a node (increasing down one
    side, decreasing down the other). CLASSIFICATION: a tree catamorphism — each node returns a
    summary `(value, inc, dec)` (longest increasing / decreasing run starting at it and going down),
    combined from its children's summaries by comparing their values to its own, while threading the
    running best `inc + dec - 1`. We certify the catamorphic combine + that the runs are genuine
    (carry the node value, length ≥ 1), not optimality of the reported maximum. -/

namespace LC.P0549

abbrev T := Interview.Patterns.Tree ℤ

/-- Extend an increasing run through a child whose value is one above the parent's. -/
def extUp : Option (ℤ × ℕ × ℕ) → ℤ → ℕ
  | some (cv, inc, _), v => if cv = v + 1 then inc else 0
  | none, _ => 0

/-- Extend a decreasing run through a child whose value is one below the parent's. -/
def extDown : Option (ℤ × ℕ × ℕ) → ℤ → ℕ
  | some (cv, _, dec), v => if cv = v - 1 then dec else 0
  | none, _ => 0

/-- Per-node summary `(value, inc, dec)` and the running best — the tree catamorphism the DFS runs. -/
def analyze : T → Option (ℤ × ℕ × ℕ) × ℕ
  | .leaf => (none, 0)
  | .node l v r =>
    let inc := 1 + max (extUp (analyze l).1 v) (extUp (analyze r).1 v)
    let dec := 1 + max (extDown (analyze l).1 v) (extDown (analyze r).1 v)
    (some (v, inc, dec), max (max (analyze l).2 (analyze r).2) (inc + dec - 1))

/-- The accepted answer: the longest consecutive path length. -/
def sol (t : T) : ℕ := (analyze t).2

/-- SCHEME (dp / catamorphism): a node's summary is built from its children's summaries — it carries
    the node value and extends each child's run by one when the child is consecutive. -/
theorem cls (l : T) (v : ℤ) (r : T) :
    (analyze (.node l v r)).1 =
      some (v, 1 + max (extUp (analyze l).1 v) (extUp (analyze r).1 v),
               1 + max (extDown (analyze l).1 v) (extDown (analyze r).1 v)) :=
  rfl

/-- NON-VACUITY: each node's summary genuinely carries its value and has both run lengths ≥ 1 (the
    node itself is a length-1 path), and the running best dominates this node's through-path. -/
theorem corr (l : T) (v : ℤ) (r : T) :
    ∃ inc dec, (analyze (.node l v r)).1 = some (v, inc, dec) ∧ 1 ≤ inc ∧ 1 ≤ dec ∧
      inc + dec - 1 ≤ (analyze (.node l v r)).2 := by
  refine ⟨_, _, rfl, ?_, ?_, ?_⟩
  · omega
  · omega
  · simp only [analyze]; exact le_max_right _ _

end LC.P0549
