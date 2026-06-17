import Lproofs.Schemes.Fold

/-! @lc 427 | name:Construct Quad Tree | scheme:dp | family:tree-construct | complexity:O(n²) |
    source:https://leetcode.com/problems/construct-quad-tree/

    Recursively split a `2ⁿ × 2ⁿ` grid into four quadrants (a recursive decomposition). Correctness:
    reading the tree back reconstructs the grid exactly — the construction is lossless. (LeetCode
    additionally collapses uniform quadrants to a single leaf; that compression preserves the
    reconstructed grid and is layered on top of this decomposition.) -/

namespace LC.P0427
open Interview.Patterns

inductive QT where
  | leaf : Bool → QT
  | node : QT → QT → QT → QT → QT

/-- A perfect `2ⁿ × 2ⁿ` grid. -/
def Perfect (n : ℕ) (g : List (List Bool)) : Prop := g.length = 2 ^ n ∧ ∀ r ∈ g, r.length = 2 ^ n

/-- Decompose the grid into a quad tree (split rows and columns in half). -/
def buildQT : ℕ → List (List Bool) → QT
  | 0, g => QT.leaf ((g.headD []).headD false)
  | n + 1, g =>
    let h := g.length / 2
    let w := (g.headD []).length / 2
    QT.node (buildQT n ((g.take h).map (·.take w))) (buildQT n ((g.take h).map (·.drop w)))
            (buildQT n ((g.drop h).map (·.take w))) (buildQT n ((g.drop h).map (·.drop w)))

/-- Read the tree back into a grid. -/
def readback : ℕ → QT → List (List Bool)
  | 0, QT.leaf v => [[v]]
  | n + 1, QT.node tl tr bl br =>
    List.zipWith (· ++ ·) (readback n tl) (readback n tr) ++
      List.zipWith (· ++ ·) (readback n bl) (readback n br)
  | _, _ => []

def sol (n : ℕ) (g : List (List Bool)) : QT := buildQT n g

/-- SCHEME (dp): the construction is a recursive decomposition into four sub-grids. -/
theorem cls (n : ℕ) (g : List (List Bool)) :
    buildQT (n + 1) g =
      QT.node (buildQT n ((g.take (g.length / 2)).map (·.take ((g.headD []).length / 2))))
        (buildQT n ((g.take (g.length / 2)).map (·.drop ((g.headD []).length / 2))))
        (buildQT n ((g.drop (g.length / 2)).map (·.take ((g.headD []).length / 2))))
        (buildQT n ((g.drop (g.length / 2)).map (·.drop ((g.headD []).length / 2)))) :=
  rfl

/-- Splitting each row at `w` and concatenating the halves recovers the rows. -/
theorem rows_reassemble (rows : List (List Bool)) (w : ℕ) :
    List.zipWith (· ++ ·) (rows.map (·.take w)) (rows.map (·.drop w)) = rows := by
  induction rows with
  | nil => rfl
  | cons r rs ih => simp only [List.map_cons, List.zipWith_cons_cons, List.take_append_drop, ih]

/-- Mapping a column-split over perfect rows yields a perfect sub-grid. -/
theorem perfect_split {n : ℕ} {rows : List (List Bool)} (hr : rows.length = 2 ^ n)
    (hrm : ∀ r ∈ rows, r.length = 2 ^ (n + 1)) (csel : List Bool → List Bool)
    (hc : ∀ r : List Bool, r.length = 2 ^ (n + 1) → (csel r).length = 2 ^ n) :
    Perfect n (rows.map csel) :=
  ⟨by rw [List.length_map, hr], fun r' hr' => by
    obtain ⟨r, hrmem, rfl⟩ := List.mem_map.mp hr'; exact hc r (hrm r hrmem)⟩

/-- CORRECT (lossless): reading the constructed quad tree back yields the original grid. -/
theorem corr : ∀ (n : ℕ) (g : List (List Bool)), Perfect n g → readback n (buildQT n g) = g := by
  intro n
  induction n with
  | zero =>
    rintro g ⟨hlen, hrow⟩
    obtain ⟨r, rfl⟩ := List.length_eq_one_iff.mp (by simpa using hlen)
    obtain ⟨c, rfl⟩ := List.length_eq_one_iff.mp (by simpa using hrow r (by simp))
    rfl
  | succ n ih =>
    rintro g ⟨hlen, hrow⟩
    have hgne : g ≠ [] := by intro h; rw [h] at hlen; simp at hlen; exact absurd hlen (by positivity)
    have hhead : (g.headD []).length = 2 ^ (n + 1) := by
      obtain ⟨g0, g', hg⟩ := List.exists_cons_of_ne_nil hgne
      rw [hg]; exact hrow g0 (by rw [hg]; simp)
    have hpow : (2 : ℕ) ^ (n + 1) = 2 * 2 ^ n := by rw [pow_succ]; ring
    have hh : g.length / 2 = 2 ^ n := by rw [hlen, hpow]; omega
    have hw : (g.headD []).length / 2 = 2 ^ n := by rw [hhead, hpow]; omega
    have htlen : (g.take (2 ^ n)).length = 2 ^ n := by rw [List.length_take, hlen]; omega
    have hdlen : (g.drop (2 ^ n)).length = 2 ^ n := by rw [List.length_drop, hlen]; omega
    have htm : ∀ r ∈ g.take (2 ^ n), r.length = 2 ^ (n + 1) := fun r hr =>
      hrow r (List.mem_of_mem_take hr)
    have hdm : ∀ r ∈ g.drop (2 ^ n), r.length = 2 ^ (n + 1) := fun r hr =>
      hrow r (List.mem_of_mem_drop hr)
    have hct : ∀ r : List Bool, r.length = 2 ^ (n + 1) → (r.take (2 ^ n)).length = 2 ^ n :=
      fun r hr => by rw [List.length_take, hr]; omega
    have hcd : ∀ r : List Bool, r.length = 2 ^ (n + 1) → (r.drop (2 ^ n)).length = 2 ^ n :=
      fun r hr => by rw [List.length_drop, hr]; omega
    rw [cls, readback, hh, hw,
      ih _ (perfect_split htlen htm _ hct), ih _ (perfect_split htlen htm _ hcd),
      ih _ (perfect_split hdlen hdm _ hct), ih _ (perfect_split hdlen hdm _ hcd),
      rows_reassemble, rows_reassemble, List.take_append_drop]

end LC.P0427
