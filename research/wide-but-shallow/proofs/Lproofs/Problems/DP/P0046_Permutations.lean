import Lproofs.Schemes.Fold

/-! @lc 46 | name:Permutations | scheme:dp | family:enumeration | complexity:O(n·n!) |
    source:https://leetcode.com/problems/permutations/

    Enumerate all permutations. CLASSIFICATION: the accepted recursion is a recursive decomposition —
    permute the tail, then insert the head into every position of each tail-permutation (`cls`).
    NON-VACUITY: we prove soundness — every enumerated permutation has the same length as the input
    (`corr`) — so the decomposition builds genuine arrangements of all elements, not a re-encoding. We
    certify the decomposition + the length invariant; DROP the count/distinctness claims. -/

namespace LC.P0046

variable {α : Type*}

/-- Every way to insert `x` into the list `l`. -/
def insertEverywhere (x : α) : List α → List (List α)
  | [] => [[x]]
  | y :: ys => (x :: y :: ys) :: (insertEverywhere x ys).map (y :: ·)

/-- Permutations by recursive decomposition: permute the tail, insert the head everywhere. -/
def perms : List α → List (List α)
  | [] => [[]]
  | x :: xs => (perms xs).flatMap (insertEverywhere x)

/-- SCHEME (dp / recursive decomposition): permutations of `x :: xs` insert `x` into every position of
    each permutation of `xs`. -/
theorem cls (x : α) (xs : List α) :
    perms (x :: xs) = (perms xs).flatMap (insertEverywhere x) := rfl

/-- Inserting `x` anywhere into `l` yields a list one longer. -/
theorem insert_len (x : α) : ∀ (l q : List α), q ∈ insertEverywhere x l → q.length = l.length + 1
  | [], q, hq => by simp only [insertEverywhere, List.mem_singleton] at hq; subst hq; rfl
  | y :: ys, q, hq => by
    simp only [insertEverywhere, List.mem_cons, List.mem_map] at hq
    rcases hq with rfl | ⟨q', hq', rfl⟩
    · simp
    · have := insert_len x ys q' hq'; simp only [List.length_cons] at *; omega

/-- NON-VACUITY (soundness): every enumerated permutation has the input's length. -/
theorem corr : ∀ (l q : List α), q ∈ perms l → q.length = l.length
  | [], q, hq => by simp only [perms, List.mem_singleton] at hq; subst hq; rfl
  | x :: xs, q, hq => by
    rw [cls, List.mem_flatMap] at hq
    obtain ⟨p', hp', hq⟩ := hq
    have h1 := insert_len x p' q hq
    have h2 := corr xs p' hp'
    simp only [List.length_cons]; omega

end LC.P0046
