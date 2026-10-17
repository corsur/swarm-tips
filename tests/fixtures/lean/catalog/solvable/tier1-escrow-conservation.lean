import Mathlib.Data.Nat.Basic

/-- Tier-1 Warmup (Swarm Escrow Conservation):
    For any escrow amount `E : ℕ` and quality score `s ≤ 10000` (in basis points),
    the integer division payout `(E * s) / 10000` and remainder refund
    `E - (E * s) / 10000` never exceed `E` and sum identically to `E`. -/
def statementProp : Prop :=
  ∀ (E s : ℕ), s ≤ 10000 →
    let payout := (E * s) / 10000
    payout ≤ E ∧ payout + (E - payout) = E
