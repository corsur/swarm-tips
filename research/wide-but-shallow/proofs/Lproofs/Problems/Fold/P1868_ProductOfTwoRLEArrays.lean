import Lproofs.Schemes.Fold

/-! @lc 1868 | name:Product of Two Run-Length Encoded Arrays | scheme:fold | family:two-pointers |
    complexity:O(m+n) | source:https://leetcode.com/problems/product-of-two-run-length-encoded-arrays/

    Two run-length-encoded arrays are multiplied by a two-pointer sweep over their runs. CLASSIFICATION
    (fold): decoding (and the sweep) is a streaming fold expanding each run. CORRECTNESS: we certify the
    run-length invariant the whole approach depends on — a decoded array's length equals the sum of its
    run counts, so the two encodings line up over the same index range. -/

namespace LC.P1868
open Interview.Patterns

/-- Decode a run-length encoding `[(value, count), …]` into the flat array. -/
def decode (rle : List (ℤ × ℕ)) : List ℤ := rle.flatMap fun p => List.replicate p.2 p.1

/-- SCHEME (fold): decoding streams run by run — expand the head run and prepend to the rest. -/
theorem cls (p : ℤ × ℕ) (t : List (ℤ × ℕ)) :
    decode (p :: t) = List.replicate p.2 p.1 ++ decode t := by
  simp [decode, List.flatMap_cons]

/-- CORRECT: the decoded length is the sum of the run counts — the index alignment the product relies on. -/
theorem corr (rle : List (ℤ × ℕ)) : (decode rle).length = (rle.map Prod.snd).sum := by
  simp [decode, List.length_flatMap, List.length_replicate]

end LC.P1868
