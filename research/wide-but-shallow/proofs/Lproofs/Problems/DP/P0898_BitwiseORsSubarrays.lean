import Lproofs.Schemes.Fold

/-! @lc 898 | name:Bitwise ORs of Subarrays | scheme:dp | family:dp-linear | complexity:O(n·W) |
    source:https://leetcode.com/problems/bitwise-ors-of-subarrays/

    Return how many distinct bitwise-OR values arise over all nonempty contiguous subarrays. The
    accepted O(n·W) solution streams left to right, maintaining the set of OR-values of subarrays
    ending at the current index (each extended by the new element, plus the new singleton) and
    accumulating every value seen. Soundness: every value the algorithm reports is genuinely the OR
    of some nonempty subarray. -/

namespace LC.P0898
open Interview.Patterns

/-- `v` is the bitwise OR of some nonempty contiguous subarray (infix) of `a`. -/
def IsSubarrayOR (a : List ℕ) (v : ℕ) : Prop :=
  ∃ mid, mid ≠ [] ∧ mid <:+: a ∧ mid.foldl (· ||| ·) 0 = v

/-- Streaming step: `st.2` = OR-values of subarrays ending at the current position; `st.1` = all
    OR-values seen so far. Extend each ending value by `x`, and open the new singleton `[x]`. -/
def step (st : Finset ℕ × Finset ℕ) (x : ℕ) : Finset ℕ × Finset ℕ :=
  let cur := insert x (st.2.image (· ||| x))
  (st.1 ∪ cur, cur)

/-- Accepted solution: the set of distinct subarray ORs (the answer is its cardinality). -/
def sol (a : List ℕ) : Finset ℕ := (a.foldl step ((∅ : Finset ℕ), (∅ : Finset ℕ))).1

/-- SCHEME (dp = streaming fold): the OR-set is built by a single left fold. -/
theorem cls : IsFold (fun a : List ℕ => a.foldl step ((∅ : Finset ℕ), (∅ : Finset ℕ))) :=
  ⟨step, (∅, ∅), fun _ => rfl⟩

theorem foldl_or_concat (mid : List ℕ) (x : ℕ) :
    (mid ++ [x]).foldl (· ||| ·) 0 = mid.foldl (· ||| ·) 0 ||| x := by
  rw [List.foldl_append]; rfl

/-- Invariant: the ending set holds ORs of nonempty suffixes of the seen prefix; the result set
    holds ORs of nonempty infixes. -/
def Inv (seen : List ℕ) (st : Finset ℕ × Finset ℕ) : Prop :=
  (∀ v ∈ st.2, ∃ mid, mid ≠ [] ∧ mid <:+ seen ∧ mid.foldl (· ||| ·) 0 = v) ∧
  (∀ v ∈ st.1, IsSubarrayOR seen v)

theorem inv_step (seen : List ℕ) (st : Finset ℕ × Finset ℕ) (x : ℕ) (h : Inv seen st) :
    Inv (seen ++ [x]) (step st x) := by
  obtain ⟨hC, hR⟩ := h
  constructor
  · intro v hv
    rw [step] at hv
    simp only [Finset.mem_insert, Finset.mem_image] at hv
    rcases hv with hvx | ⟨c, hc, hcv⟩
    · exact ⟨[x], by simp, ⟨seen, rfl⟩, by rw [hvx]; simp⟩
    · obtain ⟨mid, hne, ⟨t, ht⟩, hor⟩ := hC c hc
      refine ⟨mid ++ [x], by simp, ⟨t, by rw [← List.append_assoc, ht]⟩, ?_⟩
      rw [foldl_or_concat, hor, hcv]
  · intro v hv
    rw [step] at hv
    simp only [Finset.mem_union, Finset.mem_insert, Finset.mem_image] at hv
    rcases hv with hv | (hvx | ⟨c, hc, hcv⟩)
    · obtain ⟨mid, hne, ⟨s, t, hst⟩, hor⟩ := hR v hv
      exact ⟨mid, hne, ⟨s, t ++ [x], by rw [← hst]; simp [List.append_assoc]⟩, hor⟩
    · refine ⟨[x], by simp, List.IsSuffix.isInfix (List.suffix_append seen [x]), ?_⟩
      rw [hvx]; simp
    · obtain ⟨mid, hne, ⟨t, ht⟩, hor⟩ := hC c hc
      refine ⟨mid ++ [x], by simp,
        List.IsSuffix.isInfix ⟨t, by rw [← List.append_assoc, ht]⟩, ?_⟩
      rw [foldl_or_concat, hor, hcv]

theorem inv_fold : ∀ (a seen : List ℕ) (st : Finset ℕ × Finset ℕ),
    Inv seen st → Inv (seen ++ a) (a.foldl step st)
  | [], seen, st, h => by simpa using h
  | x :: a, seen, st, h => by
      have hrec := inv_fold a (seen ++ [x]) (step st x) (inv_step seen st x h)
      simpa [List.append_assoc] using hrec

/-- CORRECT (soundness): every reported value is the bitwise OR of a nonempty subarray. -/
theorem corr (a : List ℕ) {v : ℕ} (h : v ∈ sol a) : IsSubarrayOR a v := by
  have hbase : Inv [] ((∅ : Finset ℕ), (∅ : Finset ℕ)) := ⟨by simp, by simp⟩
  have hinv := inv_fold a [] (∅, ∅) hbase
  simp only [List.nil_append] at hinv
  exact hinv.2 v h

end LC.P0898
