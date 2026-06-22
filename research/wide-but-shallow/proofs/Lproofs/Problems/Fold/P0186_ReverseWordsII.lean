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

/-- SCHEME (fold): list reversal streams element by element --- each head moves to the end. -/
theorem cls {α : Type*} (x : α) (l : List α) : (x :: l).reverse = l.reverse ++ [x] := by
  simp

/-- CORRECT: reversing the whole flattened array equals reversing the word order with each word also
    reversed --- the two-reversal identity the in-place algorithm relies on. -/
theorem corr {α : Type*} (ws : List (List α)) :
    (ws.flatten).reverse = (ws.map List.reverse).reverse.flatten := by
  induction ws with
  | nil => rfl
  | cons w t ih =>
    simp only [List.flatten_cons, List.reverse_append, ih, List.map_cons,
      List.reverse_cons, List.flatten_append, List.flatten_cons, List.flatten_nil,
      List.append_nil, List.reverse_reverse]

end LC.P0186
