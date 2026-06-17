import Lproofs.Schemes.Fold
import Mathlib.Data.Nat.Choose.Factorization

/-! @lc 172 | name:Factorial Trailing Zeroes | scheme:fold | family:math-bit | complexity:O(log n) |
    source:https://leetcode.com/problems/factorial-trailing-zeroes/

    The number of trailing zeros of `n!` equals the multiplicity of 5 in `n!` (there are always
    at least as many factors of 2 as of 5, so 5 is the binding prime). The editorial computes
    this by the streaming accumulation `count += n / 5^i` — a single fold over the powers of 5.
    Correctness is Legendre's theorem: that sum IS the factorization exponent of 5 in `n!`. -/

namespace LC.P0172
open Interview.Patterns

/-- Editorial answer: sum of `n / 5^i` over `i ≥ 1` (finite — terms vanish once `5^i > n`). -/
def sol (n : ℕ) : ℕ := ∑ i ∈ Finset.Ico 1 (n + 1), n / 5 ^ i

/-- SCHEME (fold): the count is a streaming accumulation over the indices — a left fold. -/
theorem cls (n : ℕ) :
    IsFold (fun idxs : List ℕ => idxs.foldl (fun acc i => acc + n / 5 ^ i) 0) :=
  ⟨_, _, fun _ => rfl⟩

/-- CORRECT (Legendre): the answer equals the exponent of 5 in the factorization of `n!`, which
    is exactly the number of trailing zeros of `n!`. -/
theorem corr (n : ℕ) : sol n = (Nat.factorial n).factorization 5 := by
  have h5 : Nat.Prime 5 := by norm_num
  have hb : Nat.log 5 n < n + 1 := Nat.lt_succ_of_le (Nat.log_le_self 5 n)
  rw [sol, ← Nat.factorization_factorial h5 hb]

end LC.P0172
