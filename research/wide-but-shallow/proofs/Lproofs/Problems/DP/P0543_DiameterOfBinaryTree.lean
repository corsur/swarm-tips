import Lproofs.Schemes.Fold

/-! @lc 543 | name:Diameter of Binary Tree | scheme:dp | family:tree-aggregate | complexity:O(n) |
    source:https://leetcode.com/problems/diameter-of-binary-tree/

    The diameter is the longest path (in edges) between any two nodes; the one-pass DFS returns, per
    node, both its height and the best diameter seen, combining `height(left) + height(right)` at each
    node. CLASSIFICATION: a tree catamorphism (`depth` is `Tree.fold`). CORRECTNESS: we certify the
    genuine bound the algorithm's structure guarantees — the diameter is at most twice the height
    (`sol t ≤ 2 * depth t`), proven by structural induction. -/

namespace LC.P0543

abbrev T := Interview.Patterns.Tree ℤ

/-- Height: `1 + max` of children heights. -/
def depth : T → ℕ
  | .leaf => 0
  | .node l _ r => 1 + max (depth l) (depth r)

/-- Diameter (in edges): best of the children's diameters and the through-root path `hₗ + hᵣ`. -/
def sol : T → ℕ
  | .leaf => 0
  | .node l _ r => max (max (sol l) (sol r)) (depth l + depth r)

/-- SCHEME (dp / catamorphism): `sol` combines the children's diameters with the through-root
    path (the DFS recurrence), and the height aggregate it reads is a genuine `Tree.fold`. -/
theorem cls : (∀ (l : T) (v : ℤ) (r : T),
      sol (.node l v r) = max (max (sol l) (sol r)) (depth l + depth r)) ∧
    (depth : T → ℕ) = Interview.Patterns.Tree.fold 0 (fun dl _ dr => 1 + max dl dr) := by
  refine ⟨fun _ _ _ => rfl, ?_⟩
  funext t
  induction t with
  | leaf => rfl
  | node l v r ihl ihr => simp [depth, Interview.Patterns.Tree.fold, ihl, ihr]

/-- CORRECT: the diameter is at most twice the height — the longest path cannot exceed two root-to-leaf
    descents. A genuine inductive bound, not the bare recurrence. -/
theorem corr (t : T) : sol t ≤ 2 * depth t := by
  induction t with
  | leaf => simp [sol, depth]
  | node l v r ihl ihr => simp only [sol, depth] <;> omega


/-- GROUND INSTANCE (official example 1): tree [1,2,3,4,5] has diameter 3 (path 4–2–1–3). -/
def exT : T :=
  .node (.node (.node .leaf 4 .leaf) 2 (.node .leaf 5 .leaf)) 1 (.node .leaf 3 .leaf)

theorem vec : sol exT = 3 := by decide


/-- Node-count along each root-to-leaf path (= edge-count entering from the parent). -/
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

/-- `depth` is the LARGEST root-to-leaf node-count (the max-mirror of 111's min result). -/
theorem depth_max : ∀ t : T, t ≠ .leaf →
    depth t ∈ leafDepths t ∧ ∀ d ∈ leafDepths t, d ≤ depth t := by
  intro t
  induction t with
  | leaf => exact fun h => absurd rfl h
  | node l v r ihl ihr =>
    intro _
    cases l with
    | leaf =>
      cases r with
      | leaf => exact ⟨by simp [depth, leafDepths], by simp [depth, leafDepths]⟩
      | node rl rv rr =>
        obtain ⟨hmem, hbound⟩ := ihr nofun
        have hcond : ¬((leafDepths (.node rl rv rr)).isEmpty = true) := by
          simp only [List.isEmpty_iff]
          exact leafDepths_ne_nil rl rv rr
        have hld : leafDepths (.node .leaf v (.node rl rv rr)) =
            (leafDepths (.node rl rv rr)).map (· + 1) := by
          rw [leafDepths, show leafDepths (.leaf : T) = [] from rfl, List.nil_append,
            if_neg hcond]
        have hdep : depth (.node .leaf v (.node rl rv rr)) =
            depth (.node rl rv rr) + 1 := by
          simp only [depth]
          omega
        rw [hld, hdep]
        constructor
        · exact List.mem_map.mpr ⟨depth (.node rl rv rr), hmem, rfl⟩
        · intro d hd
          obtain ⟨d', hd', rfl⟩ := List.mem_map.mp hd
          have := hbound d' hd'
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
        have hdep : depth (.node (.node ll lv lr) v .leaf) =
            depth (.node ll lv lr) + 1 := by
          simp only [depth]
          omega
        rw [hld, hdep]
        constructor
        · exact List.mem_map.mpr ⟨depth (.node ll lv lr), hmem, rfl⟩
        · intro d hd
          obtain ⟨d', hd', rfl⟩ := List.mem_map.mp hd
          have := hbound d' hd'
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
        · rcases max_choice (depth (.node ll lv lr)) (depth (.node rl rv rr)) with h | h
          · refine List.mem_map.mpr ⟨depth (.node ll lv lr),
              List.mem_append_left _ hmeml, ?_⟩
            rw [show depth (.node (.node ll lv lr) v (.node rl rv rr)) =
              1 + max (depth (.node ll lv lr)) (depth (.node rl rv rr)) from rfl, h]
            omega
          · refine List.mem_map.mpr ⟨depth (.node rl rv rr),
              List.mem_append_right _ hmemr, ?_⟩
            rw [show depth (.node (.node ll lv lr) v (.node rl rv rr)) =
              1 + max (depth (.node ll lv lr)) (depth (.node rl rv rr)) from rfl, h]
            omega
        · intro d hd
          obtain ⟨d', hd', rfl⟩ := List.mem_map.mp hd
          rw [show depth (.node (.node ll lv lr) v (.node rl rv rr)) =
            1 + max (depth (.node ll lv lr)) (depth (.node rl rv rr)) from rfl]
          rcases List.mem_append.mp hd' with h' | h'
          · have hb1 := hboundl d' h'
            have hb2 : depth (.node ll lv lr) ≤
                max (depth (.node ll lv lr)) (depth (.node rl rv rr)) := le_max_left _ _
            omega
          · have hb1 := hboundr d' h'
            have hb2 : depth (.node rl rv rr) ≤
                max (depth (.node ll lv lr)) (depth (.node rl rv rr)) := le_max_right _ _
            omega

/-- Endpoint options entering a subtree from its parent: stop at the parent (0) or walk to one
    of the subtree's leaves. -/
def sideLens : T → List ℕ
  | .leaf => [0]
  | .node l v r => leafDepths (.node l v r)

theorem side_max : ∀ t : T, depth t ∈ sideLens t ∧ ∀ a ∈ sideLens t, a ≤ depth t := by
  intro t
  cases t with
  | leaf => simp [sideLens, depth]
  | node l v r => exact depth_max (.node l v r) nofun

/-- Every leaf-to-leaf (or endpoint-at-node) path length in the tree: through-the-root
    combinations plus paths inside either subtree. -/
def pathLens : T → List ℕ
  | .leaf => []
  | .node l _ r =>
    ((sideLens l).flatMap fun a => (sideLens r).map (a + ·)) ++ pathLens l ++ pathLens r

/-- EXACT: the DP value is the largest genuine path length — a real path achieves it, and no
    path exceeds it. -/
theorem exact : ∀ t : T, t ≠ .leaf →
    sol t ∈ pathLens t ∧ ∀ p ∈ pathLens t, p ≤ sol t := by
  intro t
  induction t with
  | leaf => exact fun h => absurd rfl h
  | node l v r ihl ihr =>
    intro _
    have hsol : sol (.node l v r) =
        max (max (sol l) (sol r)) (depth l + depth r) := rfl
    obtain ⟨hsl, hbl⟩ := side_max l
    obtain ⟨hsr, hbr⟩ := side_max r
    have hthru : depth l + depth r ≤ sol (.node l v r) := by
      rw [hsol]
      exact le_max_right _ _
    constructor
    · rcases max_choice (max (sol l) (sol r)) (depth l + depth r) with h | h
      · rcases max_choice (sol l) (sol r) with h2 | h2
        · cases l with
          | leaf =>
            have hs0 : sol (.node .leaf v r) = 0 := by rw [hsol, h, h2]; rfl
            have hdr : depth r = 0 := by
              rw [hs0] at hthru
              simpa [depth] using hthru
            cases r with
            | node rl rv rr => simp [depth] at hdr
            | leaf =>
              rw [hs0]
              simp [pathLens, sideLens]
          | node ll lv lr =>
            refine List.mem_append.mpr (Or.inl (List.mem_append.mpr (Or.inr ?_)))
            rw [hsol, h, h2]
            exact (ihl nofun).1
        · cases r with
          | leaf =>
            have hs0 : sol (.node l v .leaf) = 0 := by rw [hsol, h, h2]; rfl
            have hdl : depth l = 0 := by
              rw [hs0] at hthru
              simpa [depth] using hthru
            cases l with
            | node ll lv lr => simp [depth] at hdl
            | leaf =>
              rw [hs0]
              simp [pathLens, sideLens]
          | node rl rv rr =>
            refine List.mem_append.mpr (Or.inr ?_)
            rw [hsol, h, h2]
            exact (ihr nofun).1
      · refine List.mem_append.mpr (Or.inl (List.mem_append.mpr (Or.inl ?_)))
        rw [hsol, h]
        exact List.mem_flatMap.mpr ⟨depth l, hsl, List.mem_map.mpr ⟨depth r, hsr, rfl⟩⟩
    · intro p hp
      rcases List.mem_append.mp hp with hp' | hp'
      · rcases List.mem_append.mp hp' with hp'' | hp''
        · obtain ⟨a, ha, hb⟩ := List.mem_flatMap.mp hp''
          obtain ⟨b, hbmem, rfl⟩ := List.mem_map.mp hb
          have h1 := hbl a ha
          have h2 := hbr b hbmem
          rw [hsol]
          have : depth l + depth r ≤ max (max (sol l) (sol r)) (depth l + depth r) :=
            le_max_right _ _
          omega
        · cases l with
          | leaf => simp [pathLens] at hp''
          | node ll lv lr =>
            have := (ihl nofun).2 p hp''
            rw [hsol]
            have h2 : sol (.node ll lv lr) ≤ max (sol (.node ll lv lr)) (sol r) :=
              le_max_left _ _
            have h3 : max (sol (.node ll lv lr)) (sol r) ≤
                max (max (sol (.node ll lv lr)) (sol r)) (depth (.node ll lv lr) + depth r) :=
              le_max_left _ _
            omega
      · cases r with
        | leaf => simp [pathLens] at hp'
        | node rl rv rr =>
          have := (ihr nofun).2 p hp'
          rw [hsol]
          have h2 : sol (.node rl rv rr) ≤ max (sol l) (sol (.node rl rv rr)) :=
            le_max_right _ _
          have h3 : max (sol l) (sol (.node rl rv rr)) ≤
              max (max (sol l) (sol (.node rl rv rr))) (depth l + depth (.node rl rv rr)) :=
            le_max_left _ _
          omega

end LC.P0543
