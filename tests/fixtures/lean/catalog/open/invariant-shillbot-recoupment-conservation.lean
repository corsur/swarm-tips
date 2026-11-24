import Mathlib.Data.Nat.Basic
import Mathlib.Data.List.Basic

/-!
# Smart-Contract Invariant Verification: `extension-credit` Recoupment & Default-Sweep Safety

Formal multi-step state-machine model of Swarm Tips' Solana `extension-credit` Anchor
program (`route_and_recoup` and `mark_default`).

An `Advance` PDA holds an initial rent-exempt floor `rentFloor` and tracks `recouped`
against a fixed `principal`. On every payout deposit `d` processed by `route_and_recoup`,
the vault credits `d`, computes `available = vaultBalance - rentFloor` and
`outstanding = principal - recouped`, routes `toBacker = min available outstanding` to the
backer and `toRecipient = available - toBacker` to the recipient, and leaves `rentFloor`
intact. If an un-cranked `pending` deposit arrives and the backer attempts to front-run
`route_and_recoup` by calling `mark_default`, the identical principal cap prevents the
backer from seizing recipient surplus while keeping the vault balance at or above
`rentFloor` prior to account closure.

In the post-RH landscape, dogfooding real on-chain protocol invariants demonstrates that
autonomous Lean 4 provers + `nanoda` kernel verification directly secure DeFi smart contracts.
-/

/-- State of an `extension-credit` Advance PDA across arbitrary deposits and sweeps. -/
structure AdvanceState where
  principal : ℕ
  rentFloor : ℕ
  vaultBalance : ℕ
  recouped : ℕ
  totalToBacker : ℕ
  totalToRecipient : ℕ
  minVaultBalance : ℕ

/-- Initial Advance PDA funded with exact rent-exempt floor and zero recoupment. -/
def initAdvance (principal rentFloor : ℕ) : AdvanceState :=
  { principal := principal
    rentFloor := rentFloor
    vaultBalance := rentFloor
    recouped := 0
    totalToBacker := 0
    totalToRecipient := 0
    minVaultBalance := rentFloor }

/-- Single deposit followed by `route_and_recoup` crank. -/
def routeAndRecoupStep (s : AdvanceState) (deposit : ℕ) : AdvanceState :=
  let bal := s.vaultBalance + deposit
  let available := bal - s.rentFloor
  let outstanding := s.principal - s.recouped
  let toBacker := min available outstanding
  let toRecipient := available - toBacker
  let balAfter := bal - toBacker - toRecipient
  { principal := s.principal
    rentFloor := s.rentFloor
    vaultBalance := balAfter
    recouped := s.recouped + toBacker
    totalToBacker := s.totalToBacker + toBacker
    totalToRecipient := s.totalToRecipient + toRecipient
    minVaultBalance := min s.minVaultBalance balAfter }

/-- Execute an arbitrary sequence of payout deposits and `route_and_recoup` cranks. -/
def runRecoupTrace (s : AdvanceState) : List ℕ → AdvanceState
  | [] => s
  | d :: ds => runRecoupTrace (routeAndRecoupStep s d) ds

/-- Final `mark_default` sweep when an arbitrary un-cranked `pending` payout is sitting in
    the Advance PDA prior to account closure. -/
def markDefaultSweep (s : AdvanceState) (pending : ℕ) : AdvanceState :=
  let bal := s.vaultBalance + pending
  let available := bal - s.rentFloor
  let outstanding := s.principal - s.recouped
  let toBacker := min available outstanding
  let toRecipient := available - toBacker
  let balAfter := bal - toBacker - toRecipient
  { principal := s.principal
    rentFloor := s.rentFloor
    vaultBalance := balAfter
    recouped := min (s.recouped + toBacker) s.principal
    totalToBacker := s.totalToBacker + toBacker
    totalToRecipient := s.totalToRecipient + toRecipient
    minVaultBalance := min s.minVaultBalance balAfter }

/-- Four simultaneous multi-step safety invariants for `extension-credit`:
    1. Exact lamport conservation (`totalToBacker + totalToRecipient = deposits.sum + pending`),
    2. Backer-cap safety (`totalToBacker ≤ principal`),
    3. Full-recoupment optimality (`principal ≤ deposits.sum + pending → totalToBacker = principal`),
    4. Rent-floor preservation before close (`rentFloor ≤ minVaultBalance ∧ vaultBalance = rentFloor`). -/
def statementProp : Prop :=
  ∀ (principal rentFloor pending : ℕ) (deposits : List ℕ),
    let finalState := markDefaultSweep (runRecoupTrace (initAdvance principal rentFloor) deposits) pending
    finalState.totalToBacker + finalState.totalToRecipient = deposits.sum + pending ∧
    finalState.totalToBacker ≤ principal ∧
    (principal ≤ deposits.sum + pending → finalState.totalToBacker = principal) ∧
    rentFloor ≤ finalState.minVaultBalance ∧
    finalState.vaultBalance = rentFloor
