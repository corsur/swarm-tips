import Lproofs.Generators

/-! @lc 542 | name:01 Matrix | scheme:relaxation | family:bfs | complexity:O(V+E) |
    source:https://leetcode.com/problems/01-matrix/

    Each cell's answer is its distance to the nearest `0`, computed by a multi-source BFS seeded from
    every `0`-cell. We model that as the least number of grid-steps (`r`) reaching a cell `v` from
    the source set `s` (the zeros), tied to the relaxation scheme via the multi-source BFS-layer
    characterization (reachable in `≤ n` steps `=` `(reachOp s r)^[n+1] ∅`). -/

namespace LC.P0542
open Interview

/-- Distance to the nearest `0`: the least number of grid-steps from the source set `s` to `v`. -/
noncomputable def sol {V : Type*} (s : Set V) (r : V → V → Prop) (v : V) : ℕ :=
  sInf {n | ReachesS r s v n}

/-- Spec: the answer is the least number of steps from a `0`-cell reaching `v`. -/
def spec {V : Type*} (s : Set V) (r : V → V → Prop) (v : V) (d : ℕ) : Prop :=
  IsLeast {n | ReachesS r s v n} d

/-- SCHEME (relaxation): the BFS layers are exactly the bounded multi-source relaxation iterates —
    reachable in `≤ n` steps from `s` is `(reachOp s r)^[n+1] ∅`. The distance reads off these. -/
theorem cls {V : Type*} (s : Set V) (r : V → V → Prop) :
    ∀ (n : ℕ) (v : V), v ∈ (reachOp s r)^[n + 1] (∅ : Set V) ↔ ∃ m ≤ n, ReachesS r s v m :=
  mem_reachOp_iterate_set s r

/-- CORRECT: `sol` is the least number of steps from a `0`-cell reaching `v` — the genuine distance
    to the nearest zero. -/
theorem corr {V : Type*} (s : Set V) (r : V → V → Prop) (v : V) (h : ∃ n, ReachesS r s v n) :
    spec s r v (sol s r v) :=
  ⟨Nat.sInf_mem h, fun _ hn => Nat.sInf_le hn⟩

end LC.P0542
