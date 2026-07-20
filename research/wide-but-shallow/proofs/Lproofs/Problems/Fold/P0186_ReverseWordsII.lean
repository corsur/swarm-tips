import Lproofs.Schemes.Fold

/-! @lc 186 | name:Reverse Words in a String II | scheme:fold | family:string-other | complexity:O(n) |
    source:https://leetcode.com/problems/reverse-words-in-a-string-ii/

    Reverse the order of the words in a character array, in place. The accepted O(1)-space solution
    reverses the entire array, then reverses each word back. CLASSIFICATION (fold): each reversal is a
    streaming fold over the characters. CORRECTNESS (the identity that makes the in-place trick work):
    we certify that reversing the whole concatenation equals reversing the word order and each word ---
    `reverse (flatten ws) = flatten (reverse (map reverse ws))` --- which is exactly why the
    two-reversal algorithm restores each word's characters while reversing the word order. -/

namespace LC.P0186

/-- Pass 1 of the in-place algorithm: reverse the entire flattened character array. -/
def sol {α : Type*} (ws : List (List α)) : List α := ws.flatten.reverse

/-- SCHEME (fold): the whole-array reversal `sol` performs is a streaming left fold (each element
    conses onto the carried accumulator — `Interview.Patterns.fold_reverse`). -/
theorem cls {α : Type*} : Interview.Patterns.IsFold (List.reverse : List α → List α) ∧
    ∀ ws : List (List α), sol ws = ws.flatten.reverse :=
  ⟨Interview.Patterns.fold_reverse, fun _ => rfl⟩

/-- CORRECT: pass 1 (`sol`) equals the words in reversed order with each word itself reversed —
    so reversing each word in place (pass 2) recovers the reversed word order. This is the
    two-reversal identity the in-place algorithm relies on. -/
theorem corr {α : Type*} (ws : List (List α)) :
    sol ws = (ws.map List.reverse).reverse.flatten := by
  unfold sol
  induction ws with
  | nil => rfl
  | cons w t ih =>
    simp only [List.flatten_cons, List.reverse_append, ih, List.map_cons,
      List.reverse_cons, List.flatten_append, List.flatten_cons, List.flatten_nil,
      List.append_nil, List.reverse_reverse]


/-- GROUND INSTANCE (official example, first two words): reversing the flattened "the sky"
    gives "yks eht" — reversed word order with each word reversed. -/
theorem vec : sol ["the".toList, "sky".toList] = "ykseht".toList := by decide

end LC.P0186
