use anchor_lang::prelude::*;

/// Cross-chain match state machine (the Solana leg; mirrors the EVM
/// CrossChainGame contract). Each chain holds one player's native-coin
/// stake and settles only its own leg of a co-signed certificate.
///
/// ```text
///   None --create_xmatch--> Funded --lock_xtranche--> Locked --settle_xmatch--> Settled
///           (player+stake)    |        (operator)        |       (terminal 3+3-sig)
///           +matchmaker       |                          |
///                             |                          +--open_xclaim--> Claiming
///                             |                          |   (2-sig checkpoint)  |
///                             |                          |   supersede / equivocation
///                             |                          |                       |
///                             |                          |   settle_xclaim (window closed)
///                             |                          |                       v
///                             |                          |                  ClaimSettled
///   Funded --refund_xmatch_nocert--> RefundedNoCert      |
///           (t > fund_deadline,                          +--refund_xmatch_timeout-->
///            never locked)                                  RefundedTimeout
///                                                           (t > match_deadline +
///                                                            max_claim_window + 2*skew)
/// ```
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum XChainStatus {
    None,
    Funded,
    Locked,
    Claiming,
    Settled,
    ClaimSettled,
    RefundedNoCert,
    RefundedTimeout,
}

/// Per-match cross-chain escrow + claim state. Doubles as the stake
/// lamport vault. Cross-chain deadlines are unix seconds (NOT slots) so
/// the certificate means the same thing on both legs.
///
/// Seeds: `["xmatch", match_id]`
#[account]
pub struct XChainMatch {
    pub match_id: [u8; 32],
    pub tournament_id: u64,
    /// Local player wallet (payout/refund recipient).
    pub player: Pubkey,
    /// 1 when this player is P1 in the payoff matrix, else 0.
    pub player_is_p1: u8,
    /// This player's per-match secp256k1 session key (eth-address form).
    pub session_key: [u8; 20],
    /// Counterparty's session key (cert cross-check).
    pub counter_session_key: [u8; 20],
    pub stake_lamports: u64,
    /// 0 until locked. Max cross-chain payout from the float pool.
    pub tranche_lamports: u64,
    pub fund_deadline: i64,
    pub match_deadline: i64,
    /// Quote-freshness anchor; set at lock.
    pub locked_at: i64,
    /// Anchored claim window end: match_deadline + claim_window_secs.
    pub claim_window_end: i64,
    pub claim_window_secs: u32,
    pub match_live_digest: [u8; 32],
    /// 255 once an equivocation verdict is final.
    pub best_step_count: u8,
    pub best_outcome_kind: u8,
    /// Equivocation flags — verdict is order-independent (mirrors the EVM
    /// fix: never inferred from best_outcome_kind).
    pub local_equivocated: bool,
    pub counter_equivocated: bool,
    pub status: XChainStatus,
    pub bump: u8,
}

impl XChainMatch {
    pub const SPACE: usize = 8  // discriminator
        + 32 // match_id
        + 8  // tournament_id
        + 32 // player
        + 1  // player_is_p1
        + 20 // session_key
        + 20 // counter_session_key
        + 8  // stake_lamports
        + 8  // tranche_lamports
        + 8  // fund_deadline
        + 8  // match_deadline
        + 8  // locked_at
        + 8  // claim_window_end
        + 4  // claim_window_secs
        + 32 // match_live_digest
        + 1  // best_step_count
        + 1  // best_outcome_kind
        + 1  // local_equivocated
        + 1  // counter_equivocated
        + 1  // status (enum tag)
        + 1; // bump
}

/// Singleton operator float pool — the on-chain collateral that pays
/// cross-chain winners. Lamport vault tracked by free/locked split.
///
/// Seeds: `["xpool"]`
#[account]
pub struct XPayoutPool {
    /// Operator (float manager + tranche locker).
    pub operator: Pubkey,
    /// Dedicated secp256k1 certificate signer (eth-address form). NOT the
    /// operator key, NOT the program upgrade authority.
    pub operator_signer: [u8; 20],
    pub free_lamports: u64,
    pub locked_lamports: u64,
    pub max_tranche_lamports: u64,
    pub max_claim_window_secs: u32,
    pub skew_margin_secs: u32,
    pub bump: u8,
}

impl XPayoutPool {
    pub const SPACE: usize = 8  // discriminator
        + 32 // operator
        + 20 // operator_signer
        + 8  // free_lamports
        + 8  // locked_lamports
        + 8  // max_tranche_lamports
        + 4  // max_claim_window_secs
        + 4  // skew_margin_secs
        + 1; // bump
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_variants_distinct() {
        assert_ne!(XChainStatus::Funded, XChainStatus::Locked);
        assert_ne!(XChainStatus::Settled, XChainStatus::ClaimSettled);
        assert_ne!(XChainStatus::RefundedNoCert, XChainStatus::RefundedTimeout);
    }
}
