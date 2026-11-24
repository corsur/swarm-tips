import Mathlib.Data.Nat.Basic
import Mathlib.Data.List.Basic

/-!
# Smart-Contract Invariant Verification: `coordination-game` Cross-Chain `XPool` Solvency

Formal multi-step state-machine model of Swarm Tips' cross-chain `coordination-game`
`XPayoutPool` (`xpool_deposit`, `xpool_withdraw`, `lock_xtranche`, `settle_xmatch` /
`resolve_xleg`, and `refund_xmatch_timeout` / `refund_nocert`).

The Solana `XPool` PDA holds liquid float (`freeReserve`), tranche commitments locked for
live cross-chain matches (`totalLocked`), and a fixed rent-exempt floor (`rentFloor`).
Across any sequence of pool deposits, operator withdrawals, tranche locks, match settlements
over all four terminal outcome classes (`bothWin`, `p1Wins`, `p2Wins`, `bothLose`) with
protocol fee `feeBps ≤ 1000` (≤ 10%), and no-cert/timeout refunds, the state machine must
preserve exact pool PDA backing, global cross-chain lamport conservation, intermediate-debit
rent-floor solvency, and the cumulative protocol fee bound.
-/

/-- The four terminal outcome classes for cross-chain leg settlement. -/
inductive XOutcomeKind where
  | bothWin
  | p1Wins
  | p2Wins
  | bothLose

/-- Per-leg payout routing computed by `resolve_xleg`. -/
structure XLegSplit where
  toPlayer : ℕ
  toFee : ℕ
  toPrize : ℕ
  toPoolReimburse : ℕ
  trancheReleased : ℕ

/-- Route one cross-chain leg's `stake` and locked `tranche` given seat polarity and bps fee. -/
def resolveXLeg (stake tranche feeBps : ℕ) (seatIsP1 : Bool) (outcome : XOutcomeKind) : XLegSplit :=
  let feeCut := (stake * feeBps) / 10000
  match outcome with
  | .bothWin =>
      { toPlayer := stake
        toFee := 0
        toPrize := 0
        toPoolReimburse := 0
        trancheReleased := tranche }
  | .p1Wins =>
      if seatIsP1 then
        { toPlayer := stake + tranche
          toFee := 0
          toPrize := 0
          toPoolReimburse := 0
          trancheReleased := 0 }
      else
        { toPlayer := 0
          toFee := feeCut
          toPrize := 0
          toPoolReimburse := stake - feeCut
          trancheReleased := tranche }
  | .p2Wins =>
      if seatIsP1 then
        { toPlayer := 0
          toFee := feeCut
          toPrize := 0
          toPoolReimburse := stake - feeCut
          trancheReleased := tranche }
      else
        { toPlayer := stake + tranche
          toFee := 0
          toPrize := 0
          toPoolReimburse := 0
          trancheReleased := 0 }
  | .bothLose =>
      { toPlayer := 0
        toFee := feeCut
        toPrize := stake - feeCut
        toPoolReimburse := 0
        trancheReleased := tranche }

/-- Validated operations against the cross-chain `XPool` state machine. -/
inductive XPoolOp where
  | deposit (amount : ℕ)
  | withdraw (amount : ℕ)
  | lockTranche (tranche : ℕ)
  | settle (stake tranche : ℕ) (seatIsP1 : Bool) (outcome : XOutcomeKind)
  | refundNoCert (stake tranche : ℕ)

/-- Combined PDA lamport balance and accounting ledger for `XPool`. -/
structure XPoolState where
  poolLamports : ℕ
  freeReserve : ℕ
  totalLocked : ℕ
  totalDeposited : ℕ
  totalWithdrawn : ℕ
  totalMatchStakesIn : ℕ
  totalPaidOut : ℕ
  totalFees : ℕ
  totalPrize : ℕ
  minPoolLamports : ℕ

/-- Initial `XPool` PDA holding only its rent-exempt floor. -/
def initXPool (rentFloor : ℕ) : XPoolState :=
  { poolLamports := rentFloor
    freeReserve := 0
    totalLocked := 0
    totalDeposited := 0
    totalWithdrawn := 0
    totalMatchStakesIn := 0
    totalPaidOut := 0
    totalFees := 0
    totalPrize := 0
    minPoolLamports := rentFloor }

/-- Single checked transition of `XPool`; out-of-guard operations revert (leave state unchanged). -/
def stepXPool (feeBps : ℕ) (s : XPoolState) : XPoolOp → XPoolState
  | .deposit amount =>
      { s with
        poolLamports := s.poolLamports + amount
        freeReserve := s.freeReserve + amount
        totalDeposited := s.totalDeposited + amount }
  | .withdraw amount =>
      if amount ≤ s.freeReserve then
        let balAfter := s.poolLamports - amount
        { s with
          poolLamports := balAfter
          freeReserve := s.freeReserve - amount
          totalWithdrawn := s.totalWithdrawn + amount
          minPoolLamports := min s.minPoolLamports balAfter }
      else s
  | .lockTranche tranche =>
      if tranche ≤ s.freeReserve then
        { s with
          freeReserve := s.freeReserve - tranche
          totalLocked := s.totalLocked + tranche }
      else s
  | .settle stake tranche seatIsP1 outcome =>
      if tranche ≤ s.totalLocked then
        let r := resolveXLeg stake tranche feeBps seatIsP1 outcome
        let trancheConsumed := tranche - r.trancheReleased
        let balAfterDebit := s.poolLamports - trancheConsumed
        let balFinal := balAfterDebit + r.toPoolReimburse
        { s with
          poolLamports := balFinal
          freeReserve := s.freeReserve + r.trancheReleased + r.toPoolReimburse
          totalLocked := s.totalLocked - tranche
          totalMatchStakesIn := s.totalMatchStakesIn + stake
          totalPaidOut := s.totalPaidOut + r.toPlayer
          totalFees := s.totalFees + r.toFee
          totalPrize := s.totalPrize + r.toPrize
          minPoolLamports := min s.minPoolLamports balAfterDebit }
      else s
  | .refundNoCert stake tranche =>
      if tranche ≤ s.totalLocked then
        { s with
          freeReserve := s.freeReserve + tranche
          totalLocked := s.totalLocked - tranche
          totalMatchStakesIn := s.totalMatchStakesIn + stake
          totalPaidOut := s.totalPaidOut + stake }
      else s

/-- Execute an arbitrary sequence of `XPool` operations. -/
def runXPoolTrace (feeBps : ℕ) (s : XPoolState) : List XPoolOp → XPoolState
  | [] => s
  | op :: rest => runXPoolTrace feeBps (stepXPool feeBps s op) rest

/-- Four simultaneous multi-step solvency & conservation invariants for `XPool`:
    1. Exact PDA lamport-to-ledger backing (`poolLamports = rentFloor + freeReserve + totalLocked`),
    2. Global cross-chain value conservation across pool float, withdrawals, payouts, fees, and prizes,
    3. Solvency & rent-floor preservation at every intermediate debit (`rentFloor ≤ minPoolLamports`
       and `totalLocked ≤ poolLamports - rentFloor`),
    4. Cumulative protocol fee bound (`totalFees * 10000 ≤ totalMatchStakesIn * feeBps`). -/
def statementProp : Prop :=
  ∀ (rentFloor feeBps : ℕ), feeBps ≤ 1000 →
    ∀ (ops : List XPoolOp),
      let finalState := runXPoolTrace feeBps (initXPool rentFloor) ops
      finalState.poolLamports = rentFloor + finalState.freeReserve + finalState.totalLocked ∧
      (finalState.poolLamports - rentFloor) + finalState.totalWithdrawn + finalState.totalPaidOut +
        finalState.totalFees + finalState.totalPrize =
        finalState.totalDeposited + finalState.totalMatchStakesIn ∧
      rentFloor ≤ finalState.minPoolLamports ∧
      finalState.totalLocked ≤ finalState.poolLamports - rentFloor ∧
      finalState.totalFees * 10000 ≤ finalState.totalMatchStakesIn * feeBps
