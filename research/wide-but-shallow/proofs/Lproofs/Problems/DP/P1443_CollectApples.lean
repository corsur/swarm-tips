import Lproofs.Schemes.Fold

/-! @lc 1443 | name:Minimum Time to Collect All Apples in a Tree | scheme:dp | family:dp-tree |
    complexity:O(n) | source:https://leetcode.com/problems/minimum-time-to-collect-all-apples-in-a-tree/ -/

namespace LC.P1443

/-- Node value = "this vertex has an apple". `collect` returns (time to gather every apple in the
    subtree, whether the subtree contains an apple): each child contributes its own time plus 2 (the
    edge there and back) when that child's subtree holds an apple. -/
def collect : Interview.Patterns.Tree Bool → ℤ × Bool
  | .leaf => (0, false)
  | .node l v r =>
    let cl := collect l
    let cr := collect r
    (cl.1 + (if cl.2 then 2 else 0) + cr.1 + (if cr.2 then 2 else 0), v || cl.2 || cr.2)

def sol (t : Interview.Patterns.Tree Bool) : ℤ := (collect t).1

/-- A tree with no apples anywhere. -/
def noApple : Interview.Patterns.Tree Bool → Prop
  | .leaf => True
  | .node l v r => v = false ∧ noApple l ∧ noApple r

/-- SCHEME (dp / catamorphism): `collect` is exactly a fold over the tree. -/
theorem cls : collect = Interview.Patterns.Tree.fold (0, false)
    (fun cl (v : Bool) cr =>
      (cl.1 + (if cl.2 then 2 else 0) + cr.1 + (if cr.2 then 2 else 0), v || cl.2 || cr.2)) := by
  funext t
  induction t with
  | leaf => rfl
  | node l v r ihl ihr => simp [collect, Interview.Patterns.Tree.fold, ihl, ihr]

private theorem noApple_snd (t : Interview.Patterns.Tree Bool) (h : noApple t) : (collect t).2 = false := by
  induction t with
  | leaf => rfl
  | node l v r ihl ihr =>
    obtain ⟨hv, hl, hr⟩ := h
    simp [collect, hv, ihl hl, ihr hr]

/-- CORRECT: a tree with no apples needs zero time. -/
theorem corr (t : Interview.Patterns.Tree Bool) (h : noApple t) : sol t = 0 := by
  induction t with
  | leaf => rfl
  | node l v r ihl ihr =>
    obtain ⟨_, hl, hr⟩ := h
    have h1 : (collect l).1 = 0 := ihl hl
    have h2 : (collect r).1 = 0 := ihr hr
    have h3 : (collect l).2 = false := noApple_snd l hl
    have h4 : (collect r).2 = false := noApple_snd r hr
    simp [sol, collect, h1, h2, h3, h4]

end LC.P1443
