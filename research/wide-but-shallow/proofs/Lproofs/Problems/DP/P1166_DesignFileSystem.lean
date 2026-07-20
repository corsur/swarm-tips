import Lproofs.Schemes.Fold

/-! @lc 1166 | name:Design File System | scheme:dp | family:trie | complexity:O(|path|) |
    source:https://leetcode.com/problems/design-file-system/

    A file system maps slash-separated paths to values, stored as a trie keyed by path component.
    CLASSIFICATION (dp): lookup is a left fold descending the path one component at a time (a trie
    catamorphism). CORRECTNESS: we certify the key–value contract — after `createPath path val`, looking
    up that exact `path` returns `some val`. -/

namespace LC.P1166

/-- A path trie node: an optional value here, and children keyed by the next path component. -/
inductive FS where
  | node (val : Option ℤ) (children : ℕ → Option FS)

def empty : FS := .node none (fun _ => none)

/-- Look up the value stored at `path`. -/
def sol : FS → List ℕ → Option ℤ
  | .node v _, [] => v
  | .node _ c, x :: xs => match c x with
    | some t => sol t xs
    | none => none

/-- Store `val` at `path`, creating intermediate nodes as needed. -/
def createPath : FS → List ℕ → ℤ → FS
  | .node _ c, [], val => .node (some val) c
  | .node v c, x :: xs, val =>
    .node v fun y => if y = x then some (createPath ((c x).getD empty) xs val) else c y

/-- SCHEME (dp): the lookup decomposes head-then-tail — descend into the child for the first component
    and recurse, the trie catamorphism's per-step. -/
theorem cls (v : Option ℤ) (c : ℕ → Option FS) (x : ℕ) (xs : List ℕ) :
    sol (.node v c) (x :: xs) = (c x).bind fun t => sol t xs := by
  cases h : c x <;> simp [sol, h]

/-- CORRECT: after creating `path` with value `val`, looking up that path returns exactly `val`. -/
theorem corr (t : FS) (path : List ℕ) (val : ℤ) : sol (createPath t path val) path = some val := by
  induction path generalizing t with
  | nil => cases t; simp [createPath, sol]
  | cons x xs ih =>
    cases t with
    | node v c =>
      simp only [createPath, sol, if_pos rfl]
      exact ih _


/-- GROUND INSTANCE (official example 2, components "leet","code" as 1,2): after createPath
    ("/leet",1) and ("/leet/code",2), get "/leet/code" returns 2 and an uncreated path returns
    none. -/
theorem vec : sol (createPath (createPath empty [1] 1) [1, 2] 2) [1, 2] = some 2 ∧
    sol (createPath (createPath empty [1] 1) [1, 2] 2) [3] = none := by decide


theorem sol_empty : ∀ p : List ℕ, sol empty p = none
  | [] => rfl
  | _ :: _ => rfl

/-- FRAME: creating one path leaves every other path's lookup untouched. -/
theorem sol_create_ne : ∀ (q p : List ℕ) (t : FS) (val : ℤ), q ≠ p →
    sol (createPath t p val) q = sol t q := by
  intro q
  induction q with
  | nil =>
    intro p t val hne
    cases t with
    | node v c =>
      match p with
      | [] => exact absurd rfl hne
      | _ :: _ => rfl
  | cons x qs ih =>
    intro p t val hne
    cases t with
    | node v c =>
      match p with
      | [] => rfl
      | y :: ps =>
        by_cases hxy : x = y
        · subst hxy
          have hqp : qs ≠ ps := fun h => hne (by rw [h])
          simp only [createPath, sol]
          rw [if_pos trivial]
          cases hcx : c x with
          | some t' =>
            show sol (createPath t' ps val) qs = sol t' qs
            exact ih ps t' val hqp
          | none =>
            show sol (createPath empty ps val) qs = none
            rw [ih ps empty val hqp, sol_empty]
        · simp only [createPath, sol]
          rw [if_neg hxy]

/-- EXACT SPEC (upgrades `corr`): one equation describing lookup after create completely — the
    created path holds the new value, everything else is exactly as before. -/
theorem sol_create (t : FS) (p q : List ℕ) (val : ℤ) :
    sol (createPath t p val) q = if q = p then some val else sol t q := by
  by_cases hqp : q = p
  · subst hqp
    simp [corr]
  · simp [hqp, sol_create_ne q p t val hqp]

end LC.P1166
