//! The Coordination Game's rules, defined ONCE.
//!
//! WHY THIS MODULE EXISTS
//! ----------------------
//! The game was implemented twice — the Solana program and the Solidity
//! contract each declared the nine outcome kinds and each computed the payoff
//! matrix independently:
//!
//!   programs/coordination-game/src/payoff.rs:194-202   XKIND_* constants
//!   evm/src/CertLib.sol:33-40                          the same nine, again
//!   programs/…/payoff.rs::resolve_game                 the matrix
//!   evm/src/CoordinationGame.sol::_amounts             the matrix, again
//!
//! Nothing enforced that they agreed. Only the cross-chain *certificate* was
//! unified (`cert_schema` ↔ `CertLib`, pinned by `tests/fixtures/cert-vectors.json`);
//! the same-chain rules were left as two implementations agreeing by inspection.
//!
//! This module is the single Rust definition. Solidity mirrors it, and
//! `tests/fixtures/game-vectors.json` holds both sides to it EXHAUSTIVELY —
//! the outcome domain is nine kinds, so the vectors cover all of them rather
//! than sampling. A divergence on any single row fails CI.
//!
//! DEPENDENCY DISCIPLINE
//! ---------------------
//! Same rule as `cert_schema`: pure, zero default dependencies, so it compiles
//! into the BPF program unchanged. That is why nothing here returns an Anchor
//! `Result` — the program maps [`GameError`] onto its own error enum at the
//! boundary.

/// What can go wrong evaluating the rules. Deliberately dependency-free; the
/// Solana program maps these onto `CoordinationError` at the call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameError {
    /// A stake of zero has no defined payoff.
    ZeroStake,
    /// Checked arithmetic overflowed.
    Overflow,
    /// Outcome kind outside 0..=8.
    UnknownOutcome,
    /// Score is undefined with no games played.
    ZeroGames,
}

// ---------------------------------------------------------------------------
// Outcome kinds — the ONE definition. Mirrored by CertLib.sol:33-40.
// ---------------------------------------------------------------------------

/// Same team, both guessed correctly — both keep their stake.
pub const HOMOG_BOTH_CORRECT: u8 = 0;
/// Same team, only P1 correct.
pub const HOMOG_P1_CORRECT: u8 = 1;
/// Same team, only P2 correct.
pub const HOMOG_P2_CORRECT: u8 = 2;
/// Neither guessed correctly — both forfeit.
pub const BOTH_WRONG: u8 = 3;
/// Different teams, P1 correct — P1 takes the pot.
pub const HETERO_P1_WINS: u8 = 4;
/// Different teams, P2 correct — P2 takes the pot.
pub const HETERO_P2_WINS: u8 = 5;
/// P2 timed out; P1 takes the pot.
pub const TIMEOUT_P1_WINS: u8 = 6;
/// P1 timed out; P2 takes the pot.
pub const TIMEOUT_P2_WINS: u8 = 7;
/// Neither acted in time — both forfeit.
pub const TIMEOUT_BOTH_FORFEIT: u8 = 8;

/// Every kind, in value order. Lets tests enumerate the whole domain rather
/// than hand-listing it and missing one.
pub const ALL_OUTCOME_KINDS: [u8; 9] = [
    HOMOG_BOTH_CORRECT,
    HOMOG_P1_CORRECT,
    HOMOG_P2_CORRECT,
    BOTH_WRONG,
    HETERO_P1_WINS,
    HETERO_P2_WINS,
    TIMEOUT_P1_WINS,
    TIMEOUT_P2_WINS,
    TIMEOUT_BOTH_FORFEIT,
];

/// How one resolved game splits its pot.
///
/// `p1 + p2 + gain == 2 * stake` always — see [`assert_conservation`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Payout {
    pub p1: u64,
    pub p2: u64,
    /// The tournament's take: the prize pool + treasury share before splitting.
    pub gain: u64,
}

/// The canonical payoff matrix.
///
/// Integer division is deliberate and matches the deployed behaviour on both
/// chains: the half-stake case truncates, and the loser's forfeit absorbs the
/// remainder via `2*stake - half`, so conservation holds exactly for odd stakes.
pub fn amounts_for_kind(kind: u8, stake: u64) -> Result<Payout, GameError> {
    if stake == 0 {
        return Err(GameError::ZeroStake);
    }
    let two = stake.checked_mul(2).ok_or(GameError::Overflow)?;
    let half = stake.checked_div(2).ok_or(GameError::Overflow)?;
    let two_minus_half = two.checked_sub(half).ok_or(GameError::Overflow)?;

    let payout = match kind {
        HOMOG_BOTH_CORRECT => Payout {
            p1: stake,
            p2: stake,
            gain: 0,
        },
        HOMOG_P1_CORRECT => Payout {
            p1: half,
            p2: 0,
            gain: two_minus_half,
        },
        HOMOG_P2_CORRECT => Payout {
            p1: 0,
            p2: half,
            gain: two_minus_half,
        },
        BOTH_WRONG | TIMEOUT_BOTH_FORFEIT => Payout {
            p1: 0,
            p2: 0,
            gain: two,
        },
        HETERO_P1_WINS | TIMEOUT_P1_WINS => Payout {
            p1: two,
            p2: 0,
            gain: 0,
        },
        HETERO_P2_WINS | TIMEOUT_P2_WINS => Payout {
            p1: 0,
            p2: two,
            gain: 0,
        },
        _ => return Err(GameError::UnknownOutcome),
    };

    assert_conservation(&payout, stake)?;
    Ok(payout)
}

/// The invariant every resolution must satisfy: nothing is created or lost.
///
/// Checked rather than debug-asserted because BPF release builds have no
/// overflow checks and a silent conservation break would mint or burn lamports.
pub fn assert_conservation(payout: &Payout, stake: u64) -> Result<(), GameError> {
    let two = stake.checked_mul(2).ok_or(GameError::Overflow)?;
    let total = payout
        .p1
        .checked_add(payout.p2)
        .ok_or(GameError::Overflow)?
        .checked_add(payout.gain)
        .ok_or(GameError::Overflow)?;
    if total == two {
        Ok(())
    } else {
        Err(GameError::Overflow)
    }
}

/// Who earned a "win" for leaderboard purposes.
///
/// Note the asymmetry with the payout: in `HOMOG_BOTH_CORRECT` BOTH players
/// earn a win even though neither gains lamports, because a win records a
/// correct read of the opponent, not a profit. Timeout kinds award a win to the
/// player who acted.
pub fn outcome_to_wins(kind: u8) -> Result<(bool, bool), GameError> {
    match kind {
        HOMOG_BOTH_CORRECT => Ok((true, true)),
        HOMOG_P1_CORRECT | HETERO_P1_WINS | TIMEOUT_P1_WINS => Ok((true, false)),
        HOMOG_P2_CORRECT | HETERO_P2_WINS | TIMEOUT_P2_WINS => Ok((false, true)),
        BOTH_WRONG | TIMEOUT_BOTH_FORFEIT => Ok((false, false)),
        _ => Err(GameError::UnknownOutcome),
    }
}

// ---------------------------------------------------------------------------
// Tournament eligibility + scoring
// ---------------------------------------------------------------------------

/// Games a player must have completed before any payout entitlement.
pub const MIN_GAMES_FOR_PAYOUT: u64 = 5;

/// `wins² / games`, integer division.
///
/// Squaring wins rewards a high win RATE; dividing by games stops a tiny
/// perfect sample from outranking a long strong record.
pub fn compute_score(wins: u64, games: u64) -> Result<u64, GameError> {
    if games == 0 {
        return Err(GameError::ZeroGames);
    }
    let wins_sq = wins.checked_mul(wins).ok_or(GameError::Overflow)?;
    wins_sq.checked_div(games).ok_or(GameError::Overflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Conservation over EVERY kind, not a sampled few. The domain is nine
    /// values, so exhaustive is cheap and sampling would be indefensible.
    #[test]
    fn every_outcome_kind_conserves_the_pot() {
        for stake in [1u64, 2, 3, 50_000_000, 68_482_585, u64::MAX / 4] {
            for kind in ALL_OUTCOME_KINDS {
                let p = amounts_for_kind(kind, stake)
                    .unwrap_or_else(|e| panic!("kind {kind} stake {stake}: {e:?}"));
                assert_eq!(
                    p.p1 as u128 + p.p2 as u128 + p.gain as u128,
                    stake as u128 * 2,
                    "kind {kind} broke conservation at stake {stake}"
                );
            }
        }
    }

    /// Odd stakes are where integer division could leak: half of 3 is 1, and
    /// the remainder must land in the tournament's share, not vanish.
    #[test]
    fn odd_stake_remainder_goes_to_the_pool() {
        let p = amounts_for_kind(HOMOG_P1_CORRECT, 3).unwrap();
        assert_eq!(p.p1, 1, "half of 3 truncates to 1");
        assert_eq!(p.p2, 0);
        assert_eq!(p.gain, 5, "6 - 1: the truncated lamport is not lost");
    }

    #[test]
    fn zero_stake_and_unknown_kind_are_rejected() {
        assert_eq!(
            amounts_for_kind(HOMOG_BOTH_CORRECT, 0),
            Err(GameError::ZeroStake)
        );
        assert_eq!(amounts_for_kind(9, 100), Err(GameError::UnknownOutcome));
        assert_eq!(outcome_to_wins(9), Err(GameError::UnknownOutcome));
    }

    /// Wins are not the same as gains — the case most likely to be mirrored
    /// wrongly in Solidity.
    #[test]
    fn both_correct_earns_two_wins_but_no_lamport_gain() {
        assert_eq!(outcome_to_wins(HOMOG_BOTH_CORRECT).unwrap(), (true, true));
        let p = amounts_for_kind(HOMOG_BOTH_CORRECT, 100).unwrap();
        assert_eq!((p.p1, p.p2, p.gain), (100, 100, 0));
    }

    #[test]
    fn every_kind_has_a_win_mapping() {
        for kind in ALL_OUTCOME_KINDS {
            outcome_to_wins(kind).unwrap_or_else(|e| panic!("kind {kind}: {e:?}"));
        }
    }

    #[test]
    fn score_is_wins_squared_over_games() {
        assert_eq!(compute_score(5, 5).unwrap(), 5);
        assert_eq!(compute_score(3, 10).unwrap(), 0, "9/10 truncates to 0");
        assert_eq!(compute_score(10, 20).unwrap(), 5);
        // A long strong record must outrank a tiny perfect one.
        assert!(compute_score(9, 10).unwrap() > compute_score(2, 2).unwrap());
        assert_eq!(compute_score(1, 0), Err(GameError::ZeroGames));
    }
}
