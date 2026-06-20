import Lproofs.Schemes.Fold

/-! @lc 2444 | name:Count Subarrays With Fixed Bounds | scheme:fold | family:sliding-window |
    complexity:O(n) | source:https://leetcode.com/problems/count-subarrays-with-fixed-bounds/

    A subarray is valid iff its minimum is exactly `minK`, its maximum exactly `maxK` — i.e. every
    element lies in `[minK, maxK]` AND both `minK` and `maxK` occur. CLASSIFICATION: the count is a
    streaming tally (a right fold over the subarrays). CORRECTNESS: we certify the inclusion–exclusion
    identity the O(n) scan computes — the valid count equals
        bounded[minK,maxK] − bounded[minK+1,maxK] − bounded[minK,maxK−1] + bounded[minK+1,maxK−1]
    where `bounded[lo,hi]` counts subarrays with all elements in `[lo,hi]`. Dropping the `minK` (resp.
    `maxK`) bound removes exactly the subarrays missing that value, so the four terms sieve out
    precisely the subarrays that contain both bounds. Proved over the concrete subarray enumeration. -/

namespace LC.P2444
open Interview.Patterns

/-- All nonempty contiguous subarrays of `xs`. -/
def subs (xs : List ℤ) : List (List ℤ) :=
  (List.range xs.length).flatMap fun i =>
    (List.range (xs.length - i)).map fun l => (xs.drop i).take (l + 1)

/-- Every element of `s` lies in `[lo, hi]`. -/
def allIn (lo hi : ℤ) (s : List ℤ) : Prop := ∀ x ∈ s, lo ≤ x ∧ x ≤ hi

instance (lo hi : ℤ) (s : List ℤ) : Decidable (allIn lo hi s) :=
  inferInstanceAs (Decidable (∀ x ∈ s, lo ≤ x ∧ x ≤ hi))

/-- #subarrays of `xs` with every element in `[lo, hi]`. -/
def boundedCount (xs : List ℤ) (lo hi : ℤ) : ℕ :=
  (subs xs).countP fun s => decide (allIn lo hi s)

/-- #valid subarrays: all elements in `[minK, maxK]`, both bounds occurring (the problem's answer). -/
def exactCount (xs : List ℤ) (minK maxK : ℤ) : ℕ :=
  (subs xs).countP fun s => decide (allIn minK maxK s ∧ minK ∈ s ∧ maxK ∈ s)

/-- Raising the low bound past `minK` keeps exactly the bounded subarrays that avoid `minK`. -/
theorem allIn_no_min (s : List ℤ) (minK maxK : ℤ) :
    allIn (minK + 1) maxK s ↔ allIn minK maxK s ∧ minK ∉ s := by
  constructor
  · intro h
    refine ⟨fun x hx => ?_, fun hm => ?_⟩
    · have := h x hx; omega
    · have := h minK hm; omega
  · rintro ⟨h, hm⟩ x hx
    have hb := h x hx
    have hne : x ≠ minK := by rintro rfl; exact hm hx
    omega

/-- Lowering the high bound past `maxK` keeps exactly the bounded subarrays that avoid `maxK`. -/
theorem allIn_no_max (s : List ℤ) (minK maxK : ℤ) :
    allIn minK (maxK - 1) s ↔ allIn minK maxK s ∧ maxK ∉ s := by
  constructor
  · intro h
    refine ⟨fun x hx => ?_, fun hm => ?_⟩
    · have := h x hx; omega
    · have := h maxK hm; omega
  · rintro ⟨h, hm⟩ x hx
    have hb := h x hx
    have hne : x ≠ maxK := by rintro rfl; exact hm hx
    omega

/-- Tightening both bounds keeps exactly the bounded subarrays that avoid both `minK` and `maxK`. -/
theorem allIn_no_both (s : List ℤ) (minK maxK : ℤ) :
    allIn (minK + 1) (maxK - 1) s ↔ allIn minK maxK s ∧ minK ∉ s ∧ maxK ∉ s := by
  constructor
  · intro h
    refine ⟨fun x hx => ?_, fun hm => ?_, fun hm => ?_⟩
    · have := h x hx; omega
    · have := h minK hm; omega
    · have := h maxK hm; omega
  · rintro ⟨h, hmn, hmx⟩ x hx
    have hb := h x hx
    have h1 : x ≠ minK := by rintro rfl; exact hmn hx
    have h2 : x ≠ maxK := by rintro rfl; exact hmx hx
    omega

/-- Pointwise tally identity lifts to `countP`: if the 0/1 indicators satisfy `a+b+c = d+e` at every
    element, the counts do too. -/
theorem countP_pentad {α : Type*} (a b c d e : α → Bool)
    (h : ∀ x, (if a x then 1 else 0) + (if b x then 1 else 0) + (if c x then 1 else 0)
            = (if d x then 1 else 0) + (if e x then 1 else 0)) (l : List α) :
    l.countP a + l.countP b + l.countP c = l.countP d + l.countP e := by
  induction l with
  | nil => simp
  | cons x t ih =>
    have hx := h x
    simp only [List.countP_cons]
    omega

theorem boundedCount_no_min (xs : List ℤ) (minK maxK : ℤ) :
    boundedCount xs (minK + 1) maxK
      = (subs xs).countP fun s => decide (allIn minK maxK s ∧ minK ∉ s) := by
  apply List.countP_congr
  intro s _; simp only [allIn_no_min]

theorem boundedCount_no_max (xs : List ℤ) (minK maxK : ℤ) :
    boundedCount xs minK (maxK - 1)
      = (subs xs).countP fun s => decide (allIn minK maxK s ∧ maxK ∉ s) := by
  apply List.countP_congr
  intro s _; simp only [allIn_no_max]

theorem boundedCount_no_both (xs : List ℤ) (minK maxK : ℤ) :
    boundedCount xs (minK + 1) (maxK - 1)
      = (subs xs).countP fun s => decide (allIn minK maxK s ∧ minK ∉ s ∧ maxK ∉ s) := by
  apply List.countP_congr
  intro s _; simp only [allIn_no_both]

/-- SCHEME (fold): the valid-subarray count is a tally — a streaming right fold over subarrays. -/
theorem cls (minK maxK : ℤ) :
    IsRightFold (fun L : List (List ℤ) =>
      L.countP fun s => decide (allIn minK maxK s ∧ minK ∈ s ∧ maxK ∈ s)) := by
  refine ⟨fun s n => if decide (allIn minK maxK s ∧ minK ∈ s ∧ maxK ∈ s) then n + 1 else n, 0,
    fun L => ?_⟩
  induction L with
  | nil => rfl
  | cons s t ih => simp only [List.countP_cons, List.foldr_cons, ih]; split <;> omega

/-- CORRECT: the valid count is the inclusion–exclusion sieve over the four bounded counts — exactly
    the subarrays whose min is `minK` and max is `maxK`. -/
theorem corr (xs : List ℤ) (minK maxK : ℤ) :
    exactCount xs minK maxK + boundedCount xs (minK + 1) maxK + boundedCount xs minK (maxK - 1)
      = boundedCount xs minK maxK + boundedCount xs (minK + 1) (maxK - 1) := by
  rw [boundedCount_no_min, boundedCount_no_max, boundedCount_no_both]
  unfold exactCount boundedCount
  apply countP_pentad
  intro s
  by_cases hB : allIn minK maxK s <;> by_cases hP : minK ∈ s <;> by_cases hQ : maxK ∈ s <;>
    simp [hB, hP, hQ]

end LC.P2444
