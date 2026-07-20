import Lproofs.Schemes.Fold

/-! @lc 111 | name:Minimum Depth of Binary Tree | scheme:dp | family:tree-aggregate | complexity:O(n) |
    source:https://leetcode.com/problems/minimum-depth-of-binary-tree/

    The minimum depth is the fewest nodes from the root to the nearest leaf, taking the one present
    child when a node has only one. CLASSIFICATION: a tree catamorphism (`depth` is `Tree.fold`).
    CORRECTNESS: we certify the genuine relationship that makes "minimum" meaningful — the minimum depth
    never exceeds the maximum depth (`sol t ≤ depth t`), proven by structural induction handling the
    one-child case that distinguishes this problem from `maxDepth`. -/

namespace LC.P0111

abbrev T := Interview.Patterns.Tree ℤ

/-- Maximum depth (height): `1 + max` of the children's depths. -/
def depth : T → ℕ
  | .leaf => 0
  | .node l _ r => 1 + max (depth l) (depth r)

/-- Minimum depth: shortest root-to-leaf; a one-child node must descend into the present child. -/
def sol : T → ℕ
  | .leaf => 0
  | .node l _ r =>
    match l, r with
    | .leaf, .leaf => 1
    | .leaf, r => 1 + sol r
    | l, .leaf => 1 + sol l
    | l, r => 1 + min (sol l) (sol r)

/-- SCHEME (dp / catamorphism): on a two-child node `sol` recurses into both children and takes
    the min — the genuine decomposition; the height aggregate it is bounded by is a `Tree.fold`. -/
theorem cls : (∀ (ll : T) (lv : ℤ) (lr : T) (v : ℤ) (rl : T) (rv : ℤ) (rr : T),
      sol (.node (.node ll lv lr) v (.node rl rv rr)) =
        1 + min (sol (.node ll lv lr)) (sol (.node rl rv rr))) ∧
    (depth : T → ℕ) = Interview.Patterns.Tree.fold 0 (fun dl _ dr => 1 + max dl dr) := by
  refine ⟨fun _ _ _ _ _ _ _ => rfl, ?_⟩
  funext t
  induction t with
  | leaf => rfl
  | node l v r ihl ihr => simp [depth, Interview.Patterns.Tree.fold, ihl, ihr]

/-- CORRECT: the minimum depth never exceeds the maximum depth — the shortest root-to-leaf path is no
    longer than the longest. The one-child case is where `sol` genuinely differs from `maxDepth`. -/
theorem corr (t : T) : sol t ≤ depth t := by
  have h0 : depth (.leaf : T) = 0 := rfl
  induction t with
  | leaf => rfl
  | node l v r ihl ihr =>
    cases l with
    | leaf =>
      cases r with
      | leaf => rfl
      | node rl rv rr =>
        show 1 + sol (.node rl rv rr) ≤ 1 + max (depth (.leaf : T)) (depth (.node rl rv rr))
        omega
    | node ll lv lr =>
      cases r with
      | leaf =>
        show 1 + sol (.node ll lv lr) ≤ 1 + max (depth (.node ll lv lr)) (depth (.leaf : T))
        omega
      | node rl rv rr =>
        show 1 + min (sol (.node ll lv lr)) (sol (.node rl rv rr))
          ≤ 1 + max (depth (.node ll lv lr)) (depth (.node rl rv rr))
        omega


/-- GROUND INSTANCE (official example 1): tree [3,9,20,null,null,15,7] has minimum depth 2
    (root → 9), while its height is 3. -/
def exT : T :=
  .node (.node .leaf 9 .leaf) 3 (.node (.node .leaf 15 .leaf) 20 (.node .leaf 7 .leaf))

theorem vec : sol exT = 2 := by decide


/-- Root-to-leaf path lengths: nodes counted along each path to a childless node. -/
def leafDepths : T → List ℕ
  | .leaf => []
  | .node l _ r =>
    if (leafDepths l ++ leafDepths r).isEmpty then [1]
    else (leafDepths l ++ leafDepths r).map (· + 1)

theorem leafDepths_ne_nil (l : T) (v : ℤ) (r : T) : leafDepths (.node l v r) ≠ [] := by
  simp only [leafDepths]
  split
  · simp
  · rename_i h
    simp only [ne_eq, List.map_eq_nil_iff]
    simpa [List.isEmpty_iff] using h

/-- EXACT (upgrades `corr`): the DP value is the least root-to-leaf path length — it is a
    genuine path length, and no path is shorter. -/
theorem exact : ∀ t : T, t ≠ .leaf → sol t ∈ leafDepths t ∧ ∀ d ∈ leafDepths t, sol t ≤ d := by
  intro t
  induction t with
  | leaf => exact fun h => absurd rfl h
  | node l v r ihl ihr =>
    intro _
    cases l with
    | leaf =>
      cases r with
      | leaf => refine ⟨by simp [sol, leafDepths], ?_⟩; simp [sol, leafDepths]
      | node rl rv rr =>
        obtain ⟨hmem, hbound⟩ := ihr nofun
        have hcond : ¬((leafDepths (.node rl rv rr)).isEmpty = true) := by
          simp only [List.isEmpty_iff]
          exact leafDepths_ne_nil rl rv rr
        have hld : leafDepths (.node .leaf v (.node rl rv rr)) =
            (leafDepths (.node rl rv rr)).map (· + 1) := by
          rw [leafDepths, show leafDepths (.leaf : T) = [] from rfl, List.nil_append,
            if_neg hcond]
        rw [hld]
        constructor
        · exact List.mem_map.mpr ⟨sol (.node rl rv rr), hmem, by simp only [sol]; omega⟩
        · intro d hd
          obtain ⟨d', hd', rfl⟩ := List.mem_map.mp hd
          have := hbound d' hd'
          simp only [sol]
          omega
    | node ll lv lr =>
      cases r with
      | leaf =>
        obtain ⟨hmem, hbound⟩ := ihl nofun
        have hcond : ¬((leafDepths (.node ll lv lr)).isEmpty = true) := by
          simp only [List.isEmpty_iff]
          exact leafDepths_ne_nil ll lv lr
        have hld : leafDepths (.node (.node ll lv lr) v .leaf) =
            (leafDepths (.node ll lv lr)).map (· + 1) := by
          rw [leafDepths, show leafDepths (.leaf : T) = [] from rfl, List.append_nil,
            if_neg hcond]
        rw [hld]
        constructor
        · exact List.mem_map.mpr ⟨sol (.node ll lv lr), hmem, by simp only [sol]; omega⟩
        · intro d hd
          obtain ⟨d', hd', rfl⟩ := List.mem_map.mp hd
          have := hbound d' hd'
          simp only [sol]
          omega
      | node rl rv rr =>
        obtain ⟨hmeml, hboundl⟩ := ihl nofun
        obtain ⟨hmemr, hboundr⟩ := ihr nofun
        have hcond : ¬(((leafDepths (.node ll lv lr) ++
            leafDepths (.node rl rv rr))).isEmpty = true) := by
          simp only [List.isEmpty_iff, List.append_eq_nil_iff]
          exact fun h => absurd h.1 (leafDepths_ne_nil ll lv lr)
        have hld : leafDepths (.node (.node ll lv lr) v (.node rl rv rr)) =
            (leafDepths (.node ll lv lr) ++ leafDepths (.node rl rv rr)).map (· + 1) := by
          rw [leafDepths, if_neg hcond]
        rw [hld]
        constructor
        · rcases le_total (sol (.node ll lv lr)) (sol (.node rl rv rr)) with h | h
          · refine List.mem_map.mpr ⟨sol (.node ll lv lr), List.mem_append_left _ hmeml, ?_⟩
            simp only [sol]
            rw [min_eq_left h]
            omega
          · refine List.mem_map.mpr ⟨sol (.node rl rv rr), List.mem_append_right _ hmemr, ?_⟩
            simp only [sol]
            rw [min_eq_right h]
            omega
        · intro d hd
          obtain ⟨d', hd', rfl⟩ := List.mem_map.mp hd
          simp only [sol]
          rcases List.mem_append.mp hd' with h' | h'
          · have := hboundl d' h'
            have hm : min (sol (.node ll lv lr)) (sol (.node rl rv rr)) ≤
                sol (.node ll lv lr) := min_le_left _ _
            omega
          · have := hboundr d' h'
            have hm : min (sol (.node ll lv lr)) (sol (.node rl rv rr)) ≤
                sol (.node rl rv rr) := min_le_right _ _
            omega

end LC.P0111
