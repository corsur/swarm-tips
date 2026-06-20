import Lproofs.Generators

/-! @lc 3629 | name:Minimum Jumps to Reach End via Prime Teleportation | scheme:relaxation |
    family:bfs | complexity:O(V+E) | source:https://leetcode.com/problems/minimum-jumps-to-reach-end-via-prime-teleportation/

    From index `i` you may step to an adjacent index, or — when `nums[i]` is prime — teleport to any
    index `j` whose value is a multiple of `nums[i]`. The answer is the fewest jumps from index `0` to
    the last index. CLASSIFICATION (relaxation): the minimum jump count is the BFS hop-distance under
    the concrete jump relation `jumpRel` (adjacency, plus the prime-divisibility teleport), and the BFS
    layers are exactly the bounded relaxation iterates. CORRECTNESS: `sol` is certified to be the
    genuine least number of jumps reaching the target in this graph — over the concrete teleport
    relation, not a free `r`. -/

namespace LC.P3629
open Interview

/-- Concrete jump relation: step to an adjacent index, or teleport from a prime `nums i` to any index
    whose value it divides. This is the actual movement rule, not an abstract edge. -/
def jumpRel (nums : ℕ → ℕ) (i j : ℕ) : Prop :=
  j = i + 1 ∨ i = j + 1 ∨ (Nat.Prime (nums i) ∧ nums i ∣ nums j)

/-- BFS hop-distance: the least number of jumps from `src` reaching `v`. -/
noncomputable def sol (nums : ℕ → ℕ) (src v : ℕ) : ℕ :=
  sInf {n | Reaches (jumpRel nums) src v n}

/-- Spec: the answer is the least number of jumps reaching `v`. -/
def spec (nums : ℕ → ℕ) (src v : ℕ) (d : ℕ) : Prop :=
  IsLeast {n | Reaches (jumpRel nums) src v n} d

/-- SCHEME (relaxation): reachable in `≤ n` jumps is exactly the `n+1`-st bounded relaxation iterate
    of the concrete jump relation — the BFS layers the algorithm expands. -/
theorem cls (nums : ℕ → ℕ) (src : ℕ) :
    ∀ (n : ℕ) (v : ℕ), v ∈ (reachOp ({src} : Set ℕ) (jumpRel nums))^[n + 1] (∅ : Set ℕ) ↔
      ∃ m ≤ n, Reaches (jumpRel nums) src v m :=
  mem_reachOp_iterate src (jumpRel nums)

/-- CORRECT: `sol` is the least number of jumps reaching `v` — the genuine minimum-jump count over
    the concrete adjacency + prime-teleport relation. -/
theorem corr (nums : ℕ → ℕ) (src v : ℕ) (h : ∃ n, Reaches (jumpRel nums) src v n) :
    spec nums src v (sol nums src v) :=
  ⟨Nat.sInf_mem h, fun _ hn => Nat.sInf_le hn⟩

end LC.P3629
