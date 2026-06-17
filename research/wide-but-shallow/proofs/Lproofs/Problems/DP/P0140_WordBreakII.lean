import Lproofs.Schemes.Fold

/-! @lc 140 | name:Word Break II | scheme:dp | family:backtracking | complexity:exp |
    source:https://leetcode.com/problems/word-break-ii/

    Enumerate every way to segment a string into dictionary words (a recursive decomposition: peel
    off a dictionary-word prefix, recurse on the rest). Correctness (soundness): every produced
    segmentation actually reconstructs the input — its words concatenate back to the string. -/

namespace LC.P0140
open Interview.Patterns

/-- All segmentations of `s` into dictionary words (`fuel` bounds the recursion; use `s.length`). -/
def wb : ℕ → List Char → List (List Char) → List (List (List Char))
  | _, [], _ => [[]]
  | 0, _, _ => []
  | fuel + 1, s, dict =>
    (dict.filter (fun w => w.isPrefixOf s && !w.isEmpty)).flatMap (fun w =>
      (wb fuel (s.drop w.length) dict).map (w :: ·))

def sol (s : List Char) (dict : List (List Char)) : List (List (List Char)) := wb s.length s dict

/-- SCHEME (dp): the segmentation is a recursive decomposition driven by a streaming prefix scan. -/
theorem cls : IsFold (fun s : List Char => s.foldl (fun acc _ => acc + 1) 0) :=
  ⟨_, _, fun _ => rfl⟩

theorem wb_sound : ∀ (fuel : ℕ) (s : List Char) (dict : List (List Char))
    (sent : List (List Char)), sent ∈ wb fuel s dict → sent.flatten = s := by
  intro fuel
  induction fuel with
  | zero =>
    intro s dict sent hsent
    cases s with
    | nil => simp only [wb, List.mem_singleton] at hsent; subst hsent; rfl
    | cons c s' => simp only [wb, List.not_mem_nil] at hsent
  | succ fuel ih =>
    intro s dict sent hsent
    cases s with
    | nil => simp only [wb, List.mem_singleton] at hsent; subst hsent; rfl
    | cons c s' =>
      simp only [wb, List.mem_flatMap] at hsent
      obtain ⟨w, hwmem, hsent2⟩ := hsent
      rw [List.mem_map] at hsent2
      obtain ⟨sent', hsent', rfl⟩ := hsent2
      have hf : sent'.flatten = (c :: s').drop w.length := ih _ dict sent' hsent'
      rw [List.mem_filter, Bool.and_eq_true] at hwmem
      have hpre : w <+: (c :: s') := List.isPrefixOf_iff_prefix.mp hwmem.2.1
      have hwtake : w = (c :: s').take w.length := List.prefix_iff_eq_take.mp hpre
      simp only [List.flatten_cons, hf]
      conv_rhs => rw [← List.take_append_drop w.length (c :: s')]
      rw [← hwtake]

/-- CORRECT (soundness): every produced segmentation reconstructs the input string. -/
theorem corr (s : List Char) (dict : List (List Char)) (sent : List (List Char))
    (h : sent ∈ sol s dict) : sent.flatten = s :=
  wb_sound s.length s dict sent h

end LC.P0140
