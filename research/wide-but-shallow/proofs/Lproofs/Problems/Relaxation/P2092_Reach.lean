import Lproofs.Generators

/-! @lc 2092 | name:Find All People With Secret | scheme:relaxation | family:secret-spread |
    complexity:O(V+E) | source:https://leetcode.com/problems/find-all-people-with-secret/

    Person 0 and `firstPerson` know the secret from time 0; in each meeting `(a, b, t)` the secret
    passes both ways IFF a participant already knows it BY time `t`. CLASSIFICATION (relaxation): the
    secret-bearing meetings are the reachable set, from the source meetings, of the TIME-RESPECTING
    relation `spread` — a step `a → b` needs a shared participant AND `time a ≤ time b`, so the
    secret cannot travel backwards in time. This is the genuine model: plain (time-blind)
    reachability over-approximates, admitting a later→earlier chain the real spread forbids. `cls`
    certifies the relaxation fixpoint; `corr` proves it is exactly the time-ordered reachable set. -/

namespace LC.P2092
open Interview

/-- A meeting: two participants and a timestamp. -/
abbrev Meeting := ℕ × ℕ × ℕ

def person1 (m : Meeting) : ℕ := m.1
def person2 (m : Meeting) : ℕ := m.2.1
def mtime (m : Meeting) : ℕ := m.2.2

/-- Two meetings share a participant. -/
def shares (a b : Meeting) : Prop :=
  person1 a = person1 b ∨ person1 a = person2 b ∨ person2 a = person1 b ∨ person2 a = person2 b

/-- TIME-RESPECTING spread step: meeting `b` can receive the secret from meeting `a` only when they
    share a participant AND `b` is no earlier than `a` — the secret never moves back in time. -/
def spread (M : ℕ → Meeting) (a b : ℕ) : Prop := shares (M a) (M b) ∧ mtime (M a) ≤ mtime (M b)

/-- The source meetings: those a from-time-0 secret-holder (person 0 or `firstPerson`) attends, so
    the secret enters the meeting graph there regardless of the meeting's time. -/
def sources (M : ℕ → Meeting) (firstPerson : ℕ) : Set ℕ :=
  {i | person1 (M i) = 0 ∨ person2 (M i) = 0 ∨
       person1 (M i) = firstPerson ∨ person2 (M i) = firstPerson}

/-- The secret-bearing meetings: reachable from sources under time-respecting `spread`, as the
    least fixpoint of one-step relaxation. -/
def sol (M : ℕ → Meeting) (firstPerson : ℕ) : Set ℕ :=
  OrderHom.lfp (reachOp (sources M firstPerson) (spread M))

/-- Spec: exactly the meetings reachable from a source by a time-non-decreasing chain of meetings. -/
def spec (M : ℕ → Meeting) (firstPerson : ℕ) (S : Set ℕ) : Prop :=
  S = {j | ∃ i ∈ sources M firstPerson, Relation.ReflTransGen (spread M) i j}

/-- SCHEME (relaxation): the secret-bearing set is a fixpoint of one-step relaxation of `spread`. -/
theorem cls (M : ℕ → Meeting) (firstPerson : ℕ) :
    reachOp (sources M firstPerson) (spread M) (sol M firstPerson) = sol M firstPerson :=
  reach_is_dp_fixpoint (sources M firstPerson) (spread M)

/-- CORRECT: the relaxation lfp is exactly the time-ordered reachable set — meetings reached from a
    source by a chain whose timestamps never decrease. Time-blind reachability would over-count. -/
theorem corr (M : ℕ → Meeting) (firstPerson : ℕ) : spec M firstPerson (sol M firstPerson) := by
  unfold spec sol
  rw [lfp_reachOp_eq_reachable]

end LC.P2092
