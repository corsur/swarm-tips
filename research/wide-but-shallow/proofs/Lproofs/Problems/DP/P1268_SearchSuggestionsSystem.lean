import Lproofs.Schemes.Fold

/-! @lc 1268 | name:Search Suggestions System | scheme:dp | family:trie | complexity:O(Σ|p|) |
    source:https://leetcode.com/problems/search-suggestions-system/

    As each character of a search word is typed, the system suggests products that share the typed
    prefix (the trie descent, or sorted-array prefix match). CLASSIFICATION (dp / trie): consuming the
    typed prefix is a left fold narrowing the candidate set. CORRECTNESS (soundness, not optimality or
    ranking): we certify that every product the system suggests genuinely starts with the typed prefix —
    no suggestion violates the prefix the user typed. -/

namespace LC.P1268

/-- Suggestions for a typed prefix `p`: the products that have `p` as a prefix. -/
def sol (products : List (List Char)) (p : List Char) : List (List Char) :=
  products.filter fun w => decide (p <+: w)

/-- SCHEME (fold): suggestions are computed by a single streaming filter pass over the products (a
    fold keeping those that match the typed prefix). -/
theorem cls (products : List (List Char)) (p : List Char) :
    sol products p = products.filter (fun w => decide (p <+: w)) := rfl

/-- CORRECT (soundness): every suggested product genuinely starts with the typed prefix. -/
theorem corr (products : List (List Char)) (p w : List Char) (h : w ∈ sol products p) : p <+: w := by
  simp only [sol, List.mem_filter, decide_eq_true_eq] at h
  exact h.2


/-- GROUND INSTANCE (official example 1): typing "mou" against the five products suggests
    exactly ["mouse","mousepad"]. -/
theorem vec : sol ["mobile".toList, "mouse".toList, "moneypot".toList, "monitor".toList,
    "mousepad".toList] "mou".toList = ["mouse".toList, "mousepad".toList] := by decide

end LC.P1268
