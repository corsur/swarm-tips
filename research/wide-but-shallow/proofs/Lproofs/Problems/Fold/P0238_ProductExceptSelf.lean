import Lproofs.Schemes.Fold

/-! @lc 238 | name:Product of Array Except Self | scheme:fold | family:prefix-sum | complexity:O(n) |
    source:https://leetcode.com/problems/product-of-array-except-self/

    `out[i]` is the product of all entries except `nums[i]` = (prefix product before `i`) × (suffix
    product after `i`). The accepted O(n) solution streams prefix and suffix products. CLASSIFICATION:
    the prefix-product sequence is a streaming left scan (`scanl (*) 1`). NON-VACUITY: we prove the
    telescoping foundation — the `i`-th prefix product equals the product of the first `i` entries — so
    `out[i]` is a genuine product split. We certify the scan + the telescoping, not the elementwise out. -/

namespace LC.P0238

/-- Running prefix products `[1, x₀, x₀·x₁, …]` — the streaming left scan. -/
def prefixProds (xs : List ℤ) : List ℤ := xs.scanl (· * ·) 1

/-- SCHEME (fold): the prefix products are a streaming left scan. -/
theorem cls (x : ℤ) (xs : List ℤ) : prefixProds (x :: xs) = 1 :: xs.scanl (· * ·) x := by
  simp [prefixProds, List.scanl_cons]

/-- A left scan seeded with `s`, read at index `i`, is `s` times the product of the first `i` entries. -/
theorem scanl_get (xs : List ℤ) (s : ℤ) (i : ℕ) (hi : i ≤ xs.length) :
    (xs.scanl (· * ·) s)[i]? = some (s * (xs.take i).prod) := by
  induction xs generalizing s i with
  | nil =>
    simp only [List.length_nil, Nat.le_zero] at hi
    subst hi
    simp
  | cons x xs ih =>
    cases i with
    | zero => simp [List.scanl_cons]
    | succ j =>
      rw [List.scanl_cons]
      simp only [List.getElem?_cons_succ, List.take_succ_cons, List.prod_cons]
      rw [ih (s * x) j (by simp only [List.length_cons] at hi; omega)]
      simp only [Option.some.injEq]
      ring

/-- NON-VACUITY (telescoping): the `i`-th prefix product is the product of the first `i` entries, so
    each `out[i] = prefix[i] · suffix[i]` is a genuine product split. -/
theorem corr (xs : List ℤ) (i : ℕ) (hi : i ≤ xs.length) :
    (prefixProds xs)[i]? = some (xs.take i).prod := by
  rw [prefixProds, scanl_get xs 1 i hi, one_mul]

end LC.P0238
