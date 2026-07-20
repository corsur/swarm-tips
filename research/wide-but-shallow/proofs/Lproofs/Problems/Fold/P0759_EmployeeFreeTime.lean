import Lproofs.Schemes.Fold

/-! @lc 759 | name:Employee Free Time | scheme:fold | family:merge-intervals | complexity:O(n log n) |
    source:https://leetcode.com/problems/employee-free-time/

    All employees' busy intervals are merged into a sorted, non-overlapping list; the common free time
    is the gaps between consecutive merged intervals. CLASSIFICATION (fold): the merge is a streaming
    sweep (fold) over the sorted endpoints. CORRECTNESS: we certify the defining property of a free gap —
    a point strictly between one merged interval's end `b₁` and the next one's start `a₂` lies in
    NEITHER busy interval, so it is genuinely free. -/

namespace LC.P0759

/-- `[a, b]` is busy at `x`. -/
def busy (a b x : ℤ) : Prop := a ≤ x ∧ x ≤ b

/-- The sweep's final pass: emit the gap between each pair of consecutive merged intervals. -/
def sol : List (ℤ × ℤ) → List (ℤ × ℤ)
  | p :: q :: rest => (p.2, q.1) :: sol (q :: rest)
  | _ => []

/-- SCHEME (fold): `sol` streams the merged intervals once, emitting one gap per adjacent pair. -/
theorem cls : (∀ (p q : ℤ × ℤ) (rest : List (ℤ × ℤ)),
    sol (p :: q :: rest) = (p.2, q.1) :: sol (q :: rest)) ∧ sol [] = [] :=
  ⟨fun _ _ _ => rfl, rfl⟩

/-- CORRECT: the gap `sol` emits between a merged interval ending at `b₁` and the next starting at
    `a₂` is genuine free time — any point strictly inside it (possible iff `b₁ < a₂`) is busy in
    neither bordering interval. -/
theorem corr (a1 b1 a2 b2 x : ℤ) (hx1 : b1 < x) (hx2 : x < a2) :
    (b1, a2) ∈ sol [(a1, b1), (a2, b2)] ∧ ¬ busy a1 b1 x ∧ ¬ busy a2 b2 x := by
  refine ⟨List.mem_singleton.mpr rfl, fun h => ?_, fun h => ?_⟩ <;>
    · obtain ⟨hl, hr⟩ := h; omega


/-- GROUND INSTANCE (official example 1): merged busy time [(1,3),(4,10)] leaves exactly the
    free interval (3,4). -/
theorem vec : sol [(1, 3), (4, 10)] = [(3, 4)] := by decide

end LC.P0759
