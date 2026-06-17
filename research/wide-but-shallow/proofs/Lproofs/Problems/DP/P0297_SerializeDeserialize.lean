import Lproofs.Schemes.Fold

/-! @lc 297 | name:Serialize and Deserialize Binary Tree | scheme:dp | family:tree-construct |
    complexity:O(n) | source:https://leetcode.com/problems/serialize-and-deserialize-binary-tree/

    Serialize a tree to a preorder token stream (with explicit null markers); deserialize parses it
    back. CLASSIFICATION: deserialization is a divide-and-conquer recursive decomposition — read a
    value as the root, then recursively parse the left subtree, then the right, from the remaining
    stream (`cls`). NON-VACUITY: we prove the round-trip — `parse (serialize t ++ rest) = (t, rest)`
    for any tree and trailing tokens — so serialization is lossless and deserialization genuinely
    inverts it (`corr`), not a re-encoding. -/

namespace LC.P0297

abbrev T := Interview.Patterns.Tree ℤ

/-- Preorder token stream: a value token per node, a null marker per empty subtree. -/
inductive Tok | val (v : ℤ) | null
deriving DecidableEq

def serial : T → List Tok
  | .leaf => [.null]
  | .node l v r => .val v :: (serial l ++ serial r)

def sz : T → ℕ
  | .leaf => 0
  | .node l _ r => sz l + sz r + 1

/-- Parse one tree off the front of the stream, returning it and the unconsumed tail. -/
def parse : ℕ → List Tok → T × List Tok
  | _, [] => (.leaf, [])
  | _, .null :: rest => (.leaf, rest)
  | fuel + 1, .val v :: rest =>
      (.node (parse fuel rest).1 v (parse fuel (parse fuel rest).2).1,
       (parse fuel (parse fuel rest).2).2)
  | _, _ => (.leaf, [])

/-- SCHEME (dp / divide-and-conquer): a value token decomposes the parse into root + recursive
    parses of the left then the right subtree off the remaining stream. -/
theorem cls (fuel : ℕ) (v : ℤ) (rest : List Tok) :
    parse (fuel + 1) (.val v :: rest) =
      (.node (parse fuel rest).1 v (parse fuel (parse fuel rest).2).1,
       (parse fuel (parse fuel rest).2).2) :=
  rfl

/-- NON-VACUITY (round-trip): parsing a serialized tree (with any trailing tokens) returns it exactly
    and leaves the trailing tokens untouched — serialization is lossless. -/
theorem parse_serial : ∀ (t : T) (fuel : ℕ) (extra : List Tok), sz t ≤ fuel →
    parse fuel (serial t ++ extra) = (t, extra) := by
  intro t
  induction t with
  | leaf => intro fuel extra _; simp only [serial, List.cons_append, List.nil_append, parse]
  | node l v r ihl ihr =>
    intro fuel extra hfuel
    obtain ⟨f, rfl⟩ : ∃ f, fuel = f + 1 := ⟨fuel - 1, by simp only [sz] at hfuel; omega⟩
    simp only [sz] at hfuel
    simp only [serial, List.cons_append, List.append_assoc, cls]
    rw [ihl f (serial r ++ extra) (by omega), ihr f extra (by omega)]

/-- The classified deserialization (sized with enough fuel). -/
def sol (t : T) : T := (parse (sz t) (serial t)).1

/-- CORRECT: `sol` reconstructs the original tree from its serialization. -/
theorem corr (t : T) : sol t = t := by
  have h := parse_serial t (sz t) [] (le_refl _)
  simp only [List.append_nil] at h
  simp only [sol, h]

end LC.P0297
