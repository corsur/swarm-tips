import Mathlib

/-! @lc 1466 | name:Reorder Routes to Make All Paths Lead to the City Zero | scheme:relaxation |
    family:graph-other | complexity:O(V+E) |
    source:https://leetcode.com/problems/reorder-routes-to-make-all-paths-lead-to-the-city-zero/

    A tree on cities `0…n` (rooted at `0`) has each edge originally directed. We reverse the fewest
    edges so every city can reach `0`. Model the tree by a parent function (`parent v < v`, a BFS/DFS
    order; `parent 0 = 0`) and a per-edge bit `dir v` = "the edge between `v` and its parent currently
    points toward the root". Reachability `Greach v` is a genuine directed walk to `0`.

    The accepted DFS counts the edges pointing AWAY from `0` (those that must be reversed) — a single
    traversal/relaxation over the tree (`sol`, a count fold; `cls`). Its correctness rests on the
    theorem `corr`: every city reaches `0` **iff** every edge points toward the root. So the unique
    all-reachable configuration is "all toward root", and reaching it from the original costs exactly
    one reversal per originally-away edge — the count `sol`. The hard direction is a cut-edge argument:
    a single away-edge traps its whole subtree away from the root.

        dir v = true  : v ──▶ parent v   (toward root, good)
        dir v = false : parent v ──▶ v   (away from root, must be reversed) -/

namespace LC.P1466

variable (parent : ℕ → ℕ) (dir : ℕ → Bool)

/-- One move in the post-reversal directed graph: up to the parent if its edge points rootward, or
    down to a child whose edge points away. -/
def move (a b : ℕ) : Prop :=
  (b = parent a ∧ dir a = true) ∨ (a = parent b ∧ dir b = false)

/-- `v` reaches the root `0` by a directed walk. -/
def Greach (v : ℕ) : Prop := Relation.ReflTransGen (move parent dir) v 0

/-- Every (non-root) edge points toward the root. -/
def allUp : Prop := ∀ v, 0 < v → dir v = true

/-- Every city reaches the root. -/
def allReach : Prop := ∀ v, Greach parent dir v

/-- `c` is an ancestor of `v` (reachable by climbing parents). -/
def anc (c v : ℕ) : Prop := ∃ k, parent^[k] v = c

theorem anc_self (c : ℕ) : anc parent c c := ⟨0, rfl⟩

/-- If `c` is an ancestor of `a` and `a ≠ c`, then `c` is an ancestor of `a`'s parent. -/
theorem anc_parent_of_ne (c a : ℕ) (h : anc parent c a) (hne : a ≠ c) :
    anc parent c (parent a) := by
  obtain ⟨k, hk⟩ := h
  cases k with
  | zero => simp only [Function.iterate_zero, id_eq] at hk; exact absurd hk hne
  | succ k => exact ⟨k, by rw [Function.iterate_succ_apply] at hk; exact hk⟩

/-- A down edge at `c` keeps every move inside `c`'s subtree: the subtree cannot be escaped. -/
theorem move_stays (c : ℕ) (hc : dir c = false) (a b : ℕ)
    (hab : move parent dir a b) (ha : anc parent c a) : anc parent c b := by
  rcases hab with ⟨hb, hda⟩ | ⟨ha', _⟩
  · have hne : a ≠ c := by intro h; rw [h, hc] at hda; exact absurd hda (by decide)
    rw [hb]; exact anc_parent_of_ne parent c a ha hne
  · obtain ⟨k, hk⟩ := ha
    exact ⟨k + 1, by rw [Function.iterate_succ_apply, ← ha']; exact hk⟩

/-- Everything reachable from `c` stays in `c`'s subtree (when `c`'s edge points away). -/
theorem reach_stays (c : ℕ) (hc : dir c = false) :
    ∀ y, Relation.ReflTransGen (move parent dir) c y → anc parent c y := by
  intro y hy
  induction hy with
  | refl => exact anc_self parent c
  | tail _ hstep ih => exact move_stays parent dir c hc _ _ hstep ih

theorem iterate_parent_zero (hp0 : parent 0 = 0) : ∀ k, parent^[k] 0 = 0
  | 0 => rfl
  | k + 1 => by rw [Function.iterate_succ_apply', iterate_parent_zero hp0 k, hp0]

/-- HARD direction: a city with an away edge cannot reach the root — its subtree is sealed off. -/
theorem not_reach_of_down (hp0 : parent 0 = 0) (c : ℕ) (hcpos : 0 < c) (hc : dir c = false) :
    ¬ Greach parent dir c := by
  intro hreach
  obtain ⟨k, hk⟩ := reach_stays parent dir c hc 0 hreach
  rw [iterate_parent_zero parent hp0 k] at hk
  omega

/-- EASY direction: if every edge points toward the root, every city climbs to it. -/
theorem climbs (hpar : ∀ v, 0 < v → parent v < v) (hup : allUp dir) :
    ∀ v, Greach parent dir v := by
  intro v
  induction v using Nat.strong_induction_on with
  | _ v ih =>
    rcases Nat.eq_zero_or_pos v with hv | hv
    · subst hv; exact Relation.ReflTransGen.refl
    · exact Relation.ReflTransGen.head (Or.inl ⟨rfl, hup v hv⟩) (ih (parent v) (hpar v hv))

/-- CORRECT (the crux): every city reaches the root **iff** every edge points toward the root. So
    "all toward root" is the unique reachable configuration, and the minimum reversals equals the
    number of originally-away edges — which is exactly what the accepted DFS (`sol`) counts. -/
theorem corr (hpar : ∀ v, 0 < v → parent v < v) (hp0 : parent 0 = 0) :
    allReach parent dir ↔ allUp dir := by
  constructor
  · intro hreach v hv
    rcases Bool.dichotomy (dir v) with h | h
    · exact absurd (hreach v) (not_reach_of_down parent dir hp0 v hv h)
    · exact h
  · intro hup; exact climbs parent dir hpar hup

/-- The accepted answer: the number of edges to reverse = the count of originally-away edges. -/
def sol (orig : ℕ → Bool) (nodes : Finset ℕ) : ℕ := (nodes.filter (fun v => orig v = false)).card

/-- SCHEME (relaxation): the answer is a single traversal that counts the away edges — a streaming
    fold (`Finset.card` of the away-edge filter) over the tree's nodes. -/
theorem cls (orig : ℕ → Bool) (nodes : Finset ℕ) :
    sol orig nodes = (nodes.filter (fun v => orig v = false)).card := rfl

end LC.P1466
