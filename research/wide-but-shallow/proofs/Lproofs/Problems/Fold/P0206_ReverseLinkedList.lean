import Lproofs.Schemes.Fold

/-! @lc 206 | name:Reverse Linked List | scheme:fold | family:linked-list | complexity:O(n) |
    source:https://leetcode.com/problems/reverse-linked-list/ -/

namespace LC.P0206
open Interview.Patterns

/-- A singly linked list is modeled as a `List`; the spec is its reversal. -/
def spec {α : Type*} (xs : List α) : List α := xs.reverse

/-- Editorial iterative solution: thread the reversed prefix as the accumulator (one pass). -/
def sol {α : Type*} (xs : List α) : List α := xs.foldl (fun acc x => x :: acc) []

/-- SCHEME: the solution is a streaming fold. -/
theorem cls {α : Type*} : IsFold (sol : List α → List α) :=
  ⟨fun acc x => x :: acc, [], fun _ => rfl⟩

/-- CORRECT: the fold computes the reversal. -/
theorem corr {α : Type*} (xs : List α) : sol xs = spec xs := by
  rw [sol, spec, foldl_cons_eq xs [], List.append_nil]

end LC.P0206
