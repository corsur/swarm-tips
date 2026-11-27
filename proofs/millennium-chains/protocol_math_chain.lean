import Mathlib.Data.Nat.Basic
import Mathlib.Tactic.Linarith

/-- Exact `2^64` modulus matching Rust `u64::MAX + 1`. -/
def U64_MOD : Nat := 18446744073709551616

/-- Exact `2^128` modulus matching Rust `u128::MAX + 1`. -/
def U128_MOD : Nat := 340282366920938463463374607431768211456

def u64CheckedAdd (a b : UInt64) : Option UInt64 :=
  if a.toNat + b.toNat < U64_MOD then some (a + b) else none

def u64CheckedSub (a b : UInt64) : Option UInt64 :=
  if b.toNat ≤ a.toNat then some (a - b) else none

def u64SaturatingSub (a b : UInt64) : UInt64 :=
  if b.toNat ≤ a.toNat then a - b else (0 : UInt64)

def u64Min (a b : UInt64) : UInt64 :=
  if a.toNat ≤ b.toNat then a else b

def u128CheckedMul (a b : Nat) : Option Nat :=
  if a * b < U128_MOD then some (a * b) else none

def u128CheckedDiv (a b : Nat) : Option Nat :=
  if b = 0 then none else some (a / b)

def u128ToU64 (x : Nat) : Option UInt64 :=
  if x < U64_MOD then some (UInt64.ofNat x) else none

def BPS_DENOMINATOR : UInt64 := (10000 : UInt64)
def MAX_PROTOCOL_FEE_BPS : UInt64 := (1000 : UInt64)

structure AdvanceSplit where
  to_backer : UInt64
  to_recipient : UInt64
  new_recouped : UInt64
  remaining_balance : UInt64

structure TaskPayoutSplit where
  payment_amount : UInt64
  fee_amount : UInt64
  client_refund : UInt64

inductive XOutcome : Type where
  | BothWin : XOutcome
  | P1Wins : XOutcome
  | P2Wins : XOutcome
  | BothLose : XOutcome

structure XLegSplit where
  to_player : UInt64
  to_fee : UInt64
  to_prize : UInt64
  to_pool_reimburse : UInt64
  tranche_released : UInt64

def mul_div_u64 (a b denom : UInt64) : Option UInt64 :=
  if denom == (0 : UInt64) then none
  else
    match u128CheckedMul a.toNat b.toNat with
    | none => none
    | some prod =>
    match u128CheckedDiv prod denom.toNat with
    | none => none
    | some quot =>
    u128ToU64 quot

def compute_route_and_recoup (balance rent_floor advance_lamports recouped_lamports : UInt64) :
    Option AdvanceSplit :=
  let available := u64SaturatingSub balance rent_floor
  match u64CheckedSub advance_lamports recouped_lamports with
  | none => none
  | some outstanding =>
  let to_backer := u64Min available outstanding
  match u64CheckedSub available to_backer with
  | none => none
  | some to_recipient =>
  match u64CheckedAdd recouped_lamports to_backer with
  | none => none
  | some new_recouped =>
  match u64CheckedSub balance to_backer with
  | none => none
  | some after_backer =>
  match u64CheckedSub after_backer to_recipient with
  | none => none
  | some remaining_balance =>
  some { to_backer, to_recipient, new_recouped, remaining_balance }

def compute_task_payout (escrow_lamports score quality_threshold max_score fee_bps : UInt64) :
    Option TaskPayoutSplit :=
  if max_score == (0 : UInt64) then none
  else if score > max_score then none
  else if fee_bps > MAX_PROTOCOL_FEE_BPS then none
  else if score < quality_threshold then
    some { payment_amount := 0, fee_amount := 0, client_refund := escrow_lamports }
  else
    match mul_div_u64 escrow_lamports score max_score with
    | none => none
    | some gross =>
    match mul_div_u64 gross fee_bps BPS_DENOMINATOR with
    | none => none
    | some fee_amount =>
    match u64CheckedSub gross fee_amount with
    | none => none
    | some payment_amount =>
    match u64CheckedAdd payment_amount fee_amount with
    | none => none
    | some total_out =>
    match u64CheckedSub escrow_lamports total_out with
    | none => none
    | some client_refund =>
    some { payment_amount, fee_amount, client_refund }

def compute_xleg_split (stake tranche fee_bps : UInt64) (seat_is_p1 : Bool) (outcome : XOutcome) :
    Option XLegSplit :=
  if fee_bps > MAX_PROTOCOL_FEE_BPS then none
  else
    match mul_div_u64 stake fee_bps BPS_DENOMINATOR with
    | none => none
    | some fee_cut =>
    match u64CheckedSub stake fee_cut with
    | none => none
    | some net_stake =>
    match outcome with
    | .BothWin =>
      some { to_player := stake, to_fee := 0, to_prize := 0, to_pool_reimburse := 0, tranche_released := tranche }
    | .P1Wins =>
      if seat_is_p1 then
        match u64CheckedAdd stake tranche with
        | none => none
        | some total_win =>
        some { to_player := total_win, to_fee := 0, to_prize := 0, to_pool_reimburse := 0, tranche_released := 0 }
      else
        some { to_player := 0, to_fee := fee_cut, to_prize := 0, to_pool_reimburse := net_stake, tranche_released := tranche }
    | .P2Wins =>
      if seat_is_p1 then
        some { to_player := 0, to_fee := fee_cut, to_prize := 0, to_pool_reimburse := net_stake, tranche_released := tranche }
      else
        match u64CheckedAdd stake tranche with
        | none => none
        | some total_win =>
        some { to_player := total_win, to_fee := 0, to_prize := 0, to_pool_reimburse := 0, tranche_released := 0 }
    | .BothLose =>
      some { to_player := 0, to_fee := fee_cut, to_prize := net_stake, to_pool_reimburse := 0, tranche_released := tranche }

/-- Combined conservation assumptions corresponding to Rungs PM1, PM2, and PM3. -/
def RungPM1Conservation : Prop :=
  ∀ (bal rent adv rec : UInt64) (sp : AdvanceSplit),
    compute_route_and_recoup bal rent adv rec = some sp →
    sp.to_backer.toNat + sp.to_recipient.toNat + sp.remaining_balance.toNat = bal.toNat

def RungPM2Conservation : Prop :=
  ∀ (escrow score thr maxS fee : UInt64) (sp : TaskPayoutSplit),
    compute_task_payout escrow score thr maxS fee = some sp →
    sp.payment_amount.toNat + sp.fee_amount.toNat + sp.client_refund.toNat = escrow.toNat

def RungPM3Conservation : Prop :=
  ∀ (stake tranche fee : UInt64) (p1 : Bool) (out : XOutcome) (sp : XLegSplit),
    compute_xleg_split stake tranche fee p1 out = some sp →
    sp.to_player.toNat + sp.to_fee.toNat + sp.to_prize.toNat +
      sp.to_pool_reimburse.toNat + sp.tranche_released.toNat = stake.toNat + tranche.toNat

/-- Master Synthesis & Boundary Witness Theorem (Track II V1 — Extracted Rust Settlement Core):
    (1) Extreme `u64::MAX` boundary witnesses matching `crates/protocol-math/src/lib.rs`:
        - `compute_route_and_recoup u64::MAX 1224280 2600000 0` succeeds with exact rent floor;
        - `compute_task_payout u64::MAX u64::MAX 700000 u64::MAX 1000` succeeds without u128 overflow;
        - `compute_xleg_split u64::MAX 1 1000 true .P1Wins` cleanly rejects `u64` winner overflow (`none`);
    (2) Composition reduction: Rung PM1 + PM2 + PM3 jointly guarantee protocol-wide zero-inflation
        lamport conservation across any combined vault recoupment, task payout, and cross-chain leg. -/
def statementProp : Prop :=
  ((∃ sp, compute_route_and_recoup 18446744073709551615 1224280 2600000 0 = some sp ∧
      sp.remaining_balance = 1224280) ∧
    (∃ sp, compute_task_payout 18446744073709551615 18446744073709551615 700000 18446744073709551615 1000 = some sp ∧
      sp.client_refund = 0) ∧
    (compute_xleg_split 18446744073709551615 1 1000 true XOutcome.P1Wins = none)) ∧
  (RungPM1Conservation → RungPM2Conservation → RungPM3Conservation →
    ∀ (bal rent adv rec escrow score thr maxS feeA stake tranche feeB : UInt64)
      (p1 : Bool) (out : XOutcome)
      (sp1 : AdvanceSplit) (sp2 : TaskPayoutSplit) (sp3 : XLegSplit),
      compute_route_and_recoup bal rent adv rec = some sp1 →
      compute_task_payout escrow score thr maxS feeA = some sp2 →
      compute_xleg_split stake tranche feeB p1 out = some sp3 →
      (sp1.to_backer.toNat + sp1.to_recipient.toNat + sp1.remaining_balance.toNat) +
      (sp2.payment_amount.toNat + sp2.fee_amount.toNat + sp2.client_refund.toNat) +
      (sp3.to_player.toNat + sp3.to_fee.toNat + sp3.to_prize.toNat +
        sp3.to_pool_reimburse.toNat + sp3.tranche_released.toNat) =
      bal.toNat + escrow.toNat + stake.toNat + tranche.toNat)

theorem proof : statementProp := by
  refine ⟨⟨?_, ?_, ?_⟩, ?_⟩
  · refine ⟨{ to_backer := 2600000
              to_recipient := 18446744073705727335
              new_recouped := 2600000
              remaining_balance := 1224280 }, ?_, rfl⟩
    rfl
  · refine ⟨{ payment_amount := 16602069666338596454
              fee_amount := 1844674407370955161
              client_refund := 0 }, ?_, rfl⟩
    rfl
  · rfl
  · intro h1 h2 h3 bal rent adv rec escrow score thr maxS feeA stake tranche feeB p1 out
      sp1 sp2 sp3 hs1 hs2 hs3
    have e1 := h1 bal rent adv rec sp1 hs1
    have e2 := h2 escrow score thr maxS feeA sp2 hs2
    have e3 := h3 stake tranche feeB p1 out sp3 hs3
    omega
