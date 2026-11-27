/-
Copyright 2026 Swarm Tips contributors
SPDX-License-Identifier: Apache-2.0
-/

/-!
# Mechanically Extracted Rust Settlement Core (`crates/protocol-math/src/lib.rs`)

DO NOT EDIT BY HAND.
Generated deterministically by `cargo xtask extract-protocol-math-lean` directly
from the parsed Rust AST (`syn`) of `crates/protocol-math/src/lib.rs`.
Source SHA-256: `596e0cb72bcbb451af98057984f4925e1516a155e371755615428baf2775c540`

Models exact Rust `u64` (`0 .. 2^64 - 1`) and `u128` checked/saturating arithmetic
semantics (`Option UInt64` for overflow/underflow/div-by-zero rejection), eliminating
the hand-transcription gap between live Rust settlement code and Lean 4 proofs.
-/

/-- Exact `2^64` modulus matching Rust `u64::MAX + 1`. -/
def U64_MOD : Nat := 18446744073709551616

/-- Exact `2^128` modulus matching Rust `u128::MAX + 1`. -/
def U128_MOD : Nat := 340282366920938463463374607431768211456

/-- Rust `u64::checked_add`. -/
def u64CheckedAdd (a b : UInt64) : Option UInt64 :=
  if a.toNat + b.toNat < U64_MOD then some (a + b) else none

/-- Rust `u64::checked_sub`. -/
def u64CheckedSub (a b : UInt64) : Option UInt64 :=
  if b.toNat ≤ a.toNat then some (a - b) else none

/-- Rust `u64::checked_mul`. -/
def u64CheckedMul (a b : UInt64) : Option UInt64 :=
  if a.toNat * b.toNat < U64_MOD then some (a * b) else none

/-- Rust `u64::checked_div`. -/
def u64CheckedDiv (a b : UInt64) : Option UInt64 :=
  if b.toNat = 0 then none else some (a / b)

/-- Rust `u64::saturating_add`. -/
def u64SaturatingAdd (a b : UInt64) : UInt64 :=
  if a.toNat + b.toNat < U64_MOD then a + b else UInt64.ofNat (U64_MOD - 1)

/-- Rust `u64::saturating_sub`. -/
def u64SaturatingSub (a b : UInt64) : UInt64 :=
  if b.toNat ≤ a.toNat then a - b else (0 : UInt64)

/-- Rust `Ord::min` on `u64`. -/
def u64Min (a b : UInt64) : UInt64 :=
  if a.toNat ≤ b.toNat then a else b

/-- Rust `u128::checked_mul` (modeled in `Nat` with exact `2^128` ceiling). -/
def u128CheckedMul (a b : Nat) : Option Nat :=
  if a * b < U128_MOD then some (a * b) else none

/-- Rust `u128::checked_div`. -/
def u128CheckedDiv (a b : Nat) : Option Nat :=
  if b = 0 then none else some (a / b)

/-- Rust `u64::try_from(x: u128).ok()`. -/
def u128ToU64 (x : Nat) : Option UInt64 :=
  if x < U64_MOD then some (UInt64.ofNat x) else none

def BPS_DENOMINATOR : UInt64 := (10000 : UInt64)

def MAX_PROTOCOL_FEE_BPS : UInt64 := (1000 : UInt64)

structure AdvanceSplit where
  to_backer : UInt64
  to_recipient : UInt64
  new_recouped : UInt64
  remaining_balance : UInt64
  deriving Repr, DecidableEq

structure TaskPayoutSplit where
  payment_amount : UInt64
  fee_amount : UInt64
  client_refund : UInt64
  deriving Repr, DecidableEq

inductive XOutcome : Type where
  | BothWin : XOutcome
  | P1Wins : XOutcome
  | P2Wins : XOutcome
  | BothLose : XOutcome
  deriving Repr, DecidableEq

structure XLegSplit where
  to_player : UInt64
  to_fee : UInt64
  to_prize : UInt64
  to_pool_reimburse : UInt64
  tranche_released : UInt64
  deriving Repr, DecidableEq

def mul_div_u64 (a : UInt64) (b : UInt64) (denom : UInt64) : Option UInt64 :=
  if (denom == (0 : UInt64)) then
    none
  else
    match u128CheckedMul (((a).toNat)) ((b).toNat) with
    | none => none
    | some prod =>
    match u128CheckedDiv (prod) ((denom).toNat) with
    | none => none
    | some quot =>
    u128ToU64 (quot)

def compute_route_and_recoup (balance : UInt64) (rent_floor : UInt64) (advance_lamports : UInt64) (recouped_lamports : UInt64) : Option AdvanceSplit :=
  let available := u64SaturatingSub (balance) (rent_floor)
  match u64CheckedSub (advance_lamports) (recouped_lamports) with
  | none => none
  | some outstanding =>
  let to_backer := u64Min (available) (outstanding)
  match u64CheckedSub (available) (to_backer) with
  | none => none
  | some to_recipient =>
  match u64CheckedAdd (recouped_lamports) (to_backer) with
  | none => none
  | some new_recouped =>
  match u64CheckedSub (balance) (to_backer) with
  | none => none
  | some after_backer =>
  match u64CheckedSub (after_backer) (to_recipient) with
  | none => none
  | some remaining_balance =>
  some (({ to_backer := to_backer, to_recipient := to_recipient, new_recouped := new_recouped, remaining_balance := remaining_balance } : AdvanceSplit))

def compute_mark_default (balance : UInt64) (rent_floor : UInt64) (advance_lamports : UInt64) (recouped_lamports : UInt64) : Option AdvanceSplit :=
  let available := u64SaturatingSub (balance) (rent_floor)
  match u64CheckedSub (advance_lamports) (recouped_lamports) with
  | none => none
  | some outstanding =>
  if (outstanding == (0 : UInt64)) then
    none
  else
    let to_backer := u64Min (available) (outstanding)
    match u64CheckedSub (available) (to_backer) with
    | none => none
    | some to_recipient =>
    let new_recouped := u64Min (u64SaturatingAdd (recouped_lamports) (to_backer)) (advance_lamports)
    match u64CheckedSub (balance) (to_backer) with
    | none => none
    | some after_backer =>
    match u64CheckedSub (after_backer) (to_recipient) with
    | none => none
    | some remaining_balance =>
    some (({ to_backer := to_backer, to_recipient := to_recipient, new_recouped := new_recouped, remaining_balance := remaining_balance } : AdvanceSplit))

def compute_task_payout (escrow_lamports : UInt64) (score : UInt64) (quality_threshold : UInt64) (max_score : UInt64) (fee_bps : UInt64) : Option TaskPayoutSplit :=
  if (max_score == (0 : UInt64)) then
    none
  else
    if (score > max_score) then
      none
    else
      if (fee_bps > MAX_PROTOCOL_FEE_BPS) then
        none
      else
        if (score < quality_threshold) then
          some (({ payment_amount := (0 : UInt64), fee_amount := (0 : UInt64), client_refund := escrow_lamports } : TaskPayoutSplit))
        else
          match (mul_div_u64 (escrow_lamports) (score) (max_score)) with
          | none => none
          | some gross =>
          match (mul_div_u64 (gross) (fee_bps) (BPS_DENOMINATOR)) with
          | none => none
          | some fee_amount =>
          match u64CheckedSub (gross) (fee_amount) with
          | none => none
          | some payment_amount =>
          match u64CheckedAdd (payment_amount) (fee_amount) with
          | none => none
          | some total_out =>
          match u64CheckedSub (escrow_lamports) (total_out) with
          | none => none
          | some client_refund =>
          some (({ payment_amount := payment_amount, fee_amount := fee_amount, client_refund := client_refund } : TaskPayoutSplit))

def compute_xleg_split (stake : UInt64) (tranche : UInt64) (fee_bps : UInt64) (seat_is_p1 : Bool) (outcome : XOutcome) : Option XLegSplit :=
  if (fee_bps > MAX_PROTOCOL_FEE_BPS) then
    none
  else
    match (mul_div_u64 (stake) (fee_bps) (BPS_DENOMINATOR)) with
    | none => none
    | some fee_cut =>
    match u64CheckedSub (stake) (fee_cut) with
    | none => none
    | some net_stake =>
    match outcome with
      | XOutcome.BothWin =>
        some (({ to_player := stake, to_fee := (0 : UInt64), to_prize := (0 : UInt64), to_pool_reimburse := (0 : UInt64), tranche_released := tranche } : XLegSplit))
      | XOutcome.P1Wins =>
        if seat_is_p1 then
          match u64CheckedAdd (stake) (tranche) with
          | none => none
          | some total_win =>
          some (({ to_player := total_win, to_fee := (0 : UInt64), to_prize := (0 : UInt64), to_pool_reimburse := (0 : UInt64), tranche_released := (0 : UInt64) } : XLegSplit))
          else
          some (({ to_player := (0 : UInt64), to_fee := fee_cut, to_prize := (0 : UInt64), to_pool_reimburse := net_stake, tranche_released := tranche } : XLegSplit))
      | XOutcome.P2Wins =>
        if seat_is_p1 then
          some (({ to_player := (0 : UInt64), to_fee := fee_cut, to_prize := (0 : UInt64), to_pool_reimburse := net_stake, tranche_released := tranche } : XLegSplit))
          else
          match u64CheckedAdd (stake) (tranche) with
          | none => none
          | some total_win =>
          some (({ to_player := total_win, to_fee := (0 : UInt64), to_prize := (0 : UInt64), to_pool_reimburse := (0 : UInt64), tranche_released := (0 : UInt64) } : XLegSplit))
      | XOutcome.BothLose =>
        some (({ to_player := (0 : UInt64), to_fee := fee_cut, to_prize := net_stake, to_pool_reimburse := (0 : UInt64), tranche_released := tranche } : XLegSplit))

-- Differential #guard sanity checks mirroring `crates/protocol-math` Rust unit tests.
#guard compute_route_and_recoup 6624280 1224280 2600000 0 =
some { to_backer := 2600000, to_recipient := 2800000, new_recouped := 2600000, remaining_balance := 1224280 }
#guard compute_route_and_recoup 2000000 1224280 100 101 = none
#guard compute_mark_default 1224280 1224280 2600000 2600000 = none
#guard compute_mark_default 6624280 1224280 2600000 1000000 =
some { to_backer := 1600000, to_recipient := 3800000, new_recouped := 2600000, remaining_balance := 1224280 }
#guard compute_task_payout 10000000 1000000 700000 1000000 1000 =
some { payment_amount := 9000000, fee_amount := 1000000, client_refund := 0 }
#guard compute_task_payout 10000000 699999 700000 1000000 1000 =
some { payment_amount := 0, fee_amount := 0, client_refund := 10000000 }
#guard compute_task_payout 1000 5 0 10 1001 = none
#guard compute_xleg_split 1000000 900000 1000 true XOutcome.P1Wins =
some { to_player := 1900000, to_fee := 0, to_prize := 0, to_pool_reimburse := 0, tranche_released := 0 }
#guard compute_xleg_split 18446744073709551615 1 1000 true XOutcome.P1Wins = none
