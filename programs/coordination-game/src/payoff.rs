use crate::errors::CoordinationError;
use anchor_lang::prelude::*;

pub struct Resolution {
    pub p1_return: u64,
    pub p2_return: u64,
    pub tournament_gain: u64,
}

/// Computes payoffs for a same-team (homogenous) matchup.
///
/// Correct guess = GUESS_SAME_TEAM (0), since both players are on the same team.
///
/// Payoffs:
///   Both correct: each receives full stake back; tournament gains nothing. Both earn a win.
///   One correct, one wrong: correct gets 0.5S, wrong gets 0, tournament gets 1.5S.
///   Both wrong: both forfeit; tournament takes 2S.
///
/// Invariant: p1_return + p2_return + tournament_gain == 2 * stake_lamports
pub fn resolve_homogenous(p1_guess: u8, p2_guess: u8, stake_lamports: u64) -> Result<Resolution> {
    resolve_game(
        chain_core::game::MATCHUP_HOMOGENEOUS,
        p1_guess,
        p2_guess,
        stake_lamports,
        1, // unused for homogeneous; any valid committer id
    )
}

/// Heterogeneous matchup. `first_committer` (1 or 2, never 0) breaks the tie
/// when BOTH players guess correctly — only one can take the pot.
pub fn resolve_heterogeneous(
    p1_guess: u8,
    p2_guess: u8,
    stake_lamports: u64,
    first_committer: u8,
) -> Result<Resolution> {
    resolve_game(
        chain_core::game::MATCHUP_HETEROGENEOUS,
        p1_guess,
        p2_guess,
        stake_lamports,
        first_committer,
    )
}

/// Resolve a finished game into its lamport split.
///
/// DELEGATES ENTIRELY to `chain_core::game` — this function used to contain the
/// payoff matrix, which was also written out in
/// `evm/src/CoordinationGame.sol::_amounts` with nothing enforcing agreement.
/// The matrix now has ONE definition; this is the Anchor-facing adapter that
/// maps the core's dependency-free `GameError` onto `CoordinationError`.
///
/// Deleting the local copy was made safe by the equivalence test in
/// `shared_core_equivalence` below, which enumerated the whole reachable domain
/// against the core BEFORE this delegation and is tamper-verified. It stays as
/// the regression guard.
pub fn resolve_game(
    matchup_type: u8,
    p1_guess: u8,
    p2_guess: u8,
    stake_lamports: u64,
    first_committer: u8,
) -> Result<Resolution> {
    use chain_core::game::{self, GameError};

    let map = |e: GameError| match e {
        GameError::ZeroStake | GameError::Overflow | GameError::ZeroGames => {
            error!(CoordinationError::ArithmeticOverflow)
        }
        GameError::UnknownOutcome => error!(CoordinationError::InvalidGameState),
    };

    let kind = game::derive_kind(matchup_type, p1_guess, p2_guess, first_committer).map_err(map)?;
    let payout = game::amounts_for_kind(kind, stake_lamports).map_err(map)?;

    Ok(Resolution {
        p1_return: payout.p1,
        p2_return: payout.p2,
        tournament_gain: payout.gain,
    })
}

// ---------------------------------------------------------------------------
// Cross-chain per-leg payoff (mirrors evm/src/CrossChainGame.sol _classify +
// _execute). Each chain settles only ITS leg of a co-signed certificate from
// the locally-recorded stake/tranche. Outcome-kind values are the cross-chain
// wire format defined in crates/chain-core cert_schema — never reorder.
// ---------------------------------------------------------------------------

// RE-EXPORTED, NOT REDECLARED. These nine values used to be written out here
// AND again in evm/src/CertLib.sol:33-40, with nothing enforcing agreement.
// `chain_core::game` is now the single definition; Solidity mirrors it and
// tests/fixtures/game-vectors.json holds both sides to it exhaustively.
//
// The `XKIND_` prefix is kept so existing call sites read unchanged; the
// values come from the shared core and cannot drift from it.
pub use chain_core::game::{
    BOTH_WRONG as XKIND_BOTH_WRONG, HETERO_P1_WINS as XKIND_HETERO_P1_WINS,
    HETERO_P2_WINS as XKIND_HETERO_P2_WINS, HOMOG_BOTH_CORRECT as XKIND_HOMOG_BOTH_CORRECT,
    HOMOG_P1_CORRECT as XKIND_HOMOG_P1_CORRECT, HOMOG_P2_CORRECT as XKIND_HOMOG_P2_CORRECT,
    TIMEOUT_BOTH_FORFEIT as XKIND_TIMEOUT_BOTH_FORFEIT, TIMEOUT_P1_WINS as XKIND_TIMEOUT_P1_WINS,
    TIMEOUT_P2_WINS as XKIND_TIMEOUT_P2_WINS,
};

/// The local player's result on this leg.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum XResult {
    WinAll,            // own stake + opponent counter-value (from tranche)
    LoseAll,           // full forfeit: treasury cut + pool reimbursement
    KeepStake,         // homogeneous both-correct
    HalfBack,          // homogeneous, locally correct
    ForfeitToTreasury, // both-wrong / both-forfeit: treasury + prize pool
}

/// How a cross-chain leg routes its escrowed stake and locked tranche.
/// Conservation invariant (asserted): `stake + tranche == to_player +
/// to_treasury + to_prize + to_pool_reimburse + tranche_released`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct XLegResolution {
    pub to_player: u64,
    pub to_treasury: u64,
    pub to_prize: u64,
    pub to_pool_reimburse: u64,
    pub tranche_released: u64,
}

fn classify_xleg(outcome_kind: u8, local_is_p1: bool) -> Result<XResult> {
    let result = match outcome_kind {
        XKIND_HOMOG_BOTH_CORRECT => XResult::KeepStake,
        XKIND_BOTH_WRONG | XKIND_TIMEOUT_BOTH_FORFEIT => XResult::ForfeitToTreasury,
        XKIND_HOMOG_P1_CORRECT => {
            if local_is_p1 {
                XResult::HalfBack
            } else {
                XResult::ForfeitToTreasury
            }
        }
        XKIND_HOMOG_P2_CORRECT => {
            if local_is_p1 {
                XResult::ForfeitToTreasury
            } else {
                XResult::HalfBack
            }
        }
        XKIND_HETERO_P1_WINS | XKIND_TIMEOUT_P1_WINS => {
            if local_is_p1 {
                XResult::WinAll
            } else {
                XResult::LoseAll
            }
        }
        XKIND_HETERO_P2_WINS | XKIND_TIMEOUT_P2_WINS => {
            if local_is_p1 {
                XResult::LoseAll
            } else {
                XResult::WinAll
            }
        }
        _ => return Err(error!(CoordinationError::InvalidGameState)),
    };
    Ok(result)
}

/// Split `amount` into the treasury cut and its remainder. The remainder
/// (which absorbs rounding dust) always goes to the complementary bucket.
fn treasury_cut(amount: u64, treasury_split_bps: u16) -> Result<(u64, u64)> {
    let cut = amount
        .checked_mul(treasury_split_bps as u64)
        .and_then(|v| v.checked_div(10_000))
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    let remainder = amount
        .checked_sub(cut)
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    Ok((cut, remainder))
}

/// Route one cross-chain leg's funds. `treasury_split_bps` must be in
/// [MIN, MAX] (same bound as same-chain). Asserts conservation on exit.
pub fn resolve_xleg(
    outcome_kind: u8,
    local_is_p1: bool,
    stake: u64,
    tranche: u64,
    treasury_split_bps: u16,
) -> Result<XLegResolution> {
    require!(stake > 0, CoordinationError::ArithmeticOverflow);
    require!(
        (crate::state::MIN_TREASURY_SPLIT_BPS..=crate::state::MAX_TREASURY_SPLIT_BPS)
            .contains(&treasury_split_bps),
        CoordinationError::InvalidGameState
    );

    let mut res = XLegResolution {
        to_player: 0,
        to_treasury: 0,
        to_prize: 0,
        to_pool_reimburse: 0,
        tranche_released: tranche, // released unless a win consumes it
    };

    match classify_xleg(outcome_kind, local_is_p1)? {
        XResult::WinAll => {
            // Winner takes own stake + the cross-chain counter-value of the
            // opponent's stake, paid from the locked tranche.
            res.to_player = stake
                .checked_add(tranche)
                .ok_or(CoordinationError::ArithmeticOverflow)?;
            res.tranche_released = 0;
        }
        XResult::LoseAll => {
            // Full local forfeit: treasury cut funds the flywheel, the
            // remainder reimburses the operator pool.
            let (cut, remainder) = treasury_cut(stake, treasury_split_bps)?;
            res.to_treasury = cut;
            res.to_pool_reimburse = remainder;
        }
        XResult::KeepStake => res.to_player = stake,
        XResult::HalfBack => {
            // Half the local stake back; the rest forfeits to treasury +
            // local prize pool (same shape as same-chain homogeneous). The
            // odd-stake dust lands in the forfeit and is conserved.
            let half = stake
                .checked_div(2)
                .ok_or(CoordinationError::ArithmeticOverflow)?;
            let forfeit = stake
                .checked_sub(half)
                .ok_or(CoordinationError::ArithmeticOverflow)?;
            let (cut, remainder) = treasury_cut(forfeit, treasury_split_bps)?;
            res.to_player = half;
            res.to_treasury = cut;
            res.to_prize = remainder;
        }
        XResult::ForfeitToTreasury => {
            let (cut, remainder) = treasury_cut(stake, treasury_split_bps)?;
            res.to_treasury = cut;
            res.to_prize = remainder;
        }
    }

    // Postcondition: every lamport of stake + locked tranche routed once.
    let out = res
        .to_player
        .checked_add(res.to_treasury)
        .and_then(|v| v.checked_add(res.to_prize))
        .and_then(|v| v.checked_add(res.to_pool_reimburse))
        .and_then(|v| v.checked_add(res.tranche_released))
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    let total = stake
        .checked_add(tranche)
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    require!(out == total, CoordinationError::ArithmeticOverflow);

    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{GUESS_DIFF_TEAM, GUESS_SAME_TEAM};

    fn assert_invariant(r: &Resolution, stake: u64) {
        let total = r
            .p1_return
            .checked_add(r.p2_return)
            .unwrap()
            .checked_add(r.tournament_gain)
            .unwrap();
        assert_eq!(
            total,
            stake.checked_mul(2).unwrap(),
            "lamports must be conserved"
        );
    }

    // ---------------------------------------------------------------------------
    // resolve_homogenous
    // ---------------------------------------------------------------------------

    #[test]
    fn both_correct_full_refund() {
        let stake = 1_000_000; // 0.001 SOL
        let r = resolve_homogenous(GUESS_SAME_TEAM, GUESS_SAME_TEAM, stake).unwrap();
        assert_eq!(r.p1_return, stake);
        assert_eq!(r.p2_return, stake);
        assert_eq!(r.tournament_gain, 0);
        assert_invariant(&r, stake);
    }

    #[test]
    fn p1_correct_p2_wrong_asymmetric() {
        let stake = 1_000_000;
        let r = resolve_homogenous(GUESS_SAME_TEAM, GUESS_DIFF_TEAM, stake).unwrap();
        // P1 correct: gets half stake back (500_000)
        // P2 wrong: gets 0
        // Tournament: 2_000_000 - 500_000 = 1_500_000
        assert_eq!(r.p1_return, 500_000);
        assert_eq!(r.p2_return, 0);
        assert_eq!(r.tournament_gain, 1_500_000);
        assert_invariant(&r, stake);
    }

    #[test]
    fn p2_correct_p1_wrong_asymmetric() {
        let stake = 1_000_000;
        let r = resolve_homogenous(GUESS_DIFF_TEAM, GUESS_SAME_TEAM, stake).unwrap();
        // P1 wrong: gets 0
        // P2 correct: gets half stake back (500_000)
        assert_eq!(r.p1_return, 0);
        assert_eq!(r.p2_return, 500_000);
        assert_eq!(r.tournament_gain, 1_500_000);
        assert_invariant(&r, stake);
    }

    #[test]
    fn both_wrong_both_forfeit() {
        let stake = 1_000_000;
        let r = resolve_homogenous(GUESS_DIFF_TEAM, GUESS_DIFF_TEAM, stake).unwrap();
        assert_eq!(r.p1_return, 0);
        assert_eq!(r.p2_return, 0);
        assert_eq!(r.tournament_gain, 2_000_000);
        assert_invariant(&r, stake);
    }

    #[test]
    fn homogenous_lamports_conserved_various_stakes() {
        for stake in [100, 999, 1_000_000, 10_000_000_000u64] {
            let r = resolve_homogenous(GUESS_SAME_TEAM, GUESS_SAME_TEAM, stake).unwrap();
            assert_invariant(&r, stake);
            let r2 = resolve_homogenous(GUESS_DIFF_TEAM, GUESS_SAME_TEAM, stake).unwrap();
            assert_invariant(&r2, stake);
        }
    }

    #[test]
    fn homogenous_zero_stake_errors() {
        assert!(resolve_homogenous(GUESS_SAME_TEAM, GUESS_SAME_TEAM, 0).is_err());
    }

    // ---------------------------------------------------------------------------
    // resolve_heterogeneous
    // ---------------------------------------------------------------------------

    #[test]
    fn hetero_both_correct_p1_commits_first_p1_wins() {
        let stake = 1_000_000;
        // Both correct, p1 committed first → p1 wins full pot
        let r = resolve_heterogeneous(GUESS_DIFF_TEAM, GUESS_DIFF_TEAM, stake, 1).unwrap();
        assert_eq!(r.p1_return, 2_000_000); // 2× stake
        assert_eq!(r.p2_return, 0);
        assert_eq!(r.tournament_gain, 0);
        assert_invariant(&r, stake);
    }

    #[test]
    fn hetero_both_correct_p2_commits_first_p2_wins() {
        let stake = 1_000_000;
        // Both correct, p2 committed first → p2 wins full pot
        let r = resolve_heterogeneous(GUESS_DIFF_TEAM, GUESS_DIFF_TEAM, stake, 2).unwrap();
        assert_eq!(r.p1_return, 0);
        assert_eq!(r.p2_return, 2_000_000);
        assert_eq!(r.tournament_gain, 0);
        assert_invariant(&r, stake);
    }

    #[test]
    fn hetero_p1_wrong_p2_correct_p2_wins_regardless_of_commit_order() {
        let stake = 1_000_000;
        // P1 wrong, p2 correct → p2 wins regardless of who committed first
        let r1 = resolve_heterogeneous(GUESS_SAME_TEAM, GUESS_DIFF_TEAM, stake, 1).unwrap();
        assert_eq!(r1.p1_return, 0);
        assert_eq!(r1.p2_return, 2_000_000);
        assert_eq!(r1.tournament_gain, 0);
        assert_invariant(&r1, stake);

        let r2 = resolve_heterogeneous(GUESS_SAME_TEAM, GUESS_DIFF_TEAM, stake, 2).unwrap();
        assert_eq!(r2.p1_return, 0);
        assert_eq!(r2.p2_return, 2_000_000);
        assert_eq!(r2.tournament_gain, 0);
        assert_invariant(&r2, stake);
    }

    #[test]
    fn hetero_p2_wrong_p1_correct_p1_wins_regardless_of_commit_order() {
        let stake = 1_000_000;
        // P2 wrong, p1 correct → p1 wins regardless of who committed first
        let r1 = resolve_heterogeneous(GUESS_DIFF_TEAM, GUESS_SAME_TEAM, stake, 1).unwrap();
        assert_eq!(r1.p1_return, 2_000_000);
        assert_eq!(r1.p2_return, 0);
        assert_eq!(r1.tournament_gain, 0);
        assert_invariant(&r1, stake);

        let r2 = resolve_heterogeneous(GUESS_DIFF_TEAM, GUESS_SAME_TEAM, stake, 2).unwrap();
        assert_eq!(r2.p1_return, 2_000_000);
        assert_eq!(r2.p2_return, 0);
        assert_eq!(r2.tournament_gain, 0);
        assert_invariant(&r2, stake);
    }

    #[test]
    fn hetero_both_wrong_full_forfeiture() {
        let stake = 1_000_000;
        // Both wrong → full forfeiture to tournament; commit order doesn't matter
        let r = resolve_heterogeneous(GUESS_SAME_TEAM, GUESS_SAME_TEAM, stake, 1).unwrap();
        assert_eq!(r.p1_return, 0);
        assert_eq!(r.p2_return, 0);
        assert_eq!(r.tournament_gain, 2_000_000);
        assert_invariant(&r, stake);
    }

    #[test]
    fn hetero_lamports_conserved_various_stakes() {
        for stake in [100, 999, 1_000_000, 10_000_000_000u64] {
            for first_committer in [1u8, 2u8] {
                for (g1, g2) in [
                    (GUESS_DIFF_TEAM, GUESS_DIFF_TEAM),
                    (GUESS_SAME_TEAM, GUESS_SAME_TEAM),
                    (GUESS_DIFF_TEAM, GUESS_SAME_TEAM),
                    (GUESS_SAME_TEAM, GUESS_DIFF_TEAM),
                ] {
                    let r = resolve_heterogeneous(g1, g2, stake, first_committer).unwrap();
                    assert_invariant(&r, stake);
                }
            }
        }
    }

    #[test]
    fn hetero_zero_stake_errors() {
        assert!(resolve_heterogeneous(GUESS_DIFF_TEAM, GUESS_DIFF_TEAM, 0, 1).is_err());
    }

    #[test]
    fn hetero_invalid_first_committer_errors() {
        assert!(resolve_heterogeneous(GUESS_DIFF_TEAM, GUESS_DIFF_TEAM, 1_000_000, 0).is_err());
        assert!(resolve_heterogeneous(GUESS_DIFF_TEAM, GUESS_DIFF_TEAM, 1_000_000, 3).is_err());
    }

    // ---------------------------------------------------------------------------
    // resolve_game router
    // ---------------------------------------------------------------------------

    #[test]
    fn resolve_game_routes_homogenous() {
        let stake = 1_000_000;
        let r = resolve_game(0, GUESS_SAME_TEAM, GUESS_SAME_TEAM, stake, 1).unwrap();
        assert_eq!(r.p1_return, stake);
        assert_eq!(r.p2_return, stake);
        assert_eq!(r.tournament_gain, 0);
    }

    #[test]
    fn resolve_game_routes_heterogeneous() {
        let stake = 1_000_000;
        let r = resolve_game(1, GUESS_DIFF_TEAM, GUESS_DIFF_TEAM, stake, 1).unwrap();
        assert_eq!(r.p1_return, 2_000_000);
        assert_eq!(r.p2_return, 0);
        assert_eq!(r.tournament_gain, 0);
    }

    #[test]
    fn resolve_game_invalid_matchup_type_errors() {
        assert!(resolve_game(2, GUESS_SAME_TEAM, GUESS_SAME_TEAM, 1_000_000, 1).is_err());
    }

    // ---------------------------------------------------------------------------
    // resolve_xleg — cross-chain per-leg payoff
    // ---------------------------------------------------------------------------

    fn assert_xleg_conserved(r: &XLegResolution, stake: u64, tranche: u64) {
        let out = r
            .to_player
            .checked_add(r.to_treasury)
            .and_then(|v| v.checked_add(r.to_prize))
            .and_then(|v| v.checked_add(r.to_pool_reimburse))
            .and_then(|v| v.checked_add(r.tranche_released))
            .unwrap();
        assert_eq!(
            out,
            stake.checked_add(tranche).unwrap(),
            "xleg lamports must be conserved"
        );
    }

    #[test]
    fn xleg_win_pays_stake_plus_tranche_and_consumes_tranche() {
        let stake = 50_000_000;
        let tranche = 40_000_000;
        // Local is P1, hetero P1 wins.
        let r = resolve_xleg(XKIND_HETERO_P1_WINS, true, stake, tranche, 5000).unwrap();
        assert_eq!(r.to_player, stake + tranche);
        assert_eq!(r.tranche_released, 0);
        assert_eq!(r.to_pool_reimburse, 0);
        assert_xleg_conserved(&r, stake, tranche);
    }

    #[test]
    fn xleg_loss_splits_treasury_and_reimburses_pool_releasing_tranche() {
        let stake = 50_000_000;
        let tranche = 40_000_000;
        // Local is P1, hetero P2 wins → local loses.
        let r = resolve_xleg(XKIND_HETERO_P2_WINS, true, stake, tranche, 5000).unwrap();
        assert_eq!(r.to_player, 0);
        assert_eq!(r.to_treasury, stake / 2); // 50% split
        assert_eq!(r.to_pool_reimburse, stake - stake / 2);
        assert_eq!(
            r.tranche_released, tranche,
            "loss never consumes the tranche"
        );
        assert_xleg_conserved(&r, stake, tranche);
    }

    #[test]
    fn xleg_timeout_winner_mapping_respects_seat() {
        let stake = 1_000_000;
        let tranche = 1_000_000;
        // P2 timeout-wins; local is P2 → win.
        let r = resolve_xleg(XKIND_TIMEOUT_P2_WINS, false, stake, tranche, 5000).unwrap();
        assert_eq!(r.to_player, stake + tranche);
        // P2 timeout-wins; local is P1 → lose.
        let r2 = resolve_xleg(XKIND_TIMEOUT_P2_WINS, true, stake, tranche, 5000).unwrap();
        assert_eq!(r2.to_player, 0);
        assert_xleg_conserved(&r, stake, tranche);
        assert_xleg_conserved(&r2, stake, tranche);
    }

    #[test]
    fn xleg_homogeneous_outcomes_never_touch_the_pool() {
        let stake = 1_000_001; // odd → exercises dust
        let tranche = 7_000_000;
        for (kind, local_is_p1) in [
            (XKIND_HOMOG_BOTH_CORRECT, true),
            (XKIND_HOMOG_P1_CORRECT, true),
            (XKIND_HOMOG_P2_CORRECT, false),
            (XKIND_BOTH_WRONG, true),
            (XKIND_TIMEOUT_BOTH_FORFEIT, false),
        ] {
            let r = resolve_xleg(kind, local_is_p1, stake, tranche, 5000).unwrap();
            assert_eq!(
                r.to_pool_reimburse, 0,
                "homogeneous must not reimburse pool"
            );
            assert_eq!(
                r.tranche_released, tranche,
                "homogeneous releases full tranche"
            );
            assert_xleg_conserved(&r, stake, tranche);
        }
    }

    #[test]
    fn xleg_conserved_across_all_kinds_seats_and_splits() {
        for stake in [1u64, 999, 50_000_000, 10_000_000_000] {
            for tranche in [0u64, 1, 40_000_000] {
                for kind in 0u8..=8 {
                    for local_is_p1 in [true, false] {
                        for bps in [2000u16, 5000, 8000] {
                            let r = resolve_xleg(kind, local_is_p1, stake, tranche, bps).unwrap();
                            assert_xleg_conserved(&r, stake, tranche);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn xleg_rejects_bad_inputs() {
        assert!(resolve_xleg(9, true, 1, 1, 5000).is_err(), "unknown kind");
        assert!(resolve_xleg(0, true, 0, 1, 5000).is_err(), "zero stake");
        assert!(
            resolve_xleg(0, true, 1, 1, 1999).is_err(),
            "split below min"
        );
        assert!(
            resolve_xleg(0, true, 1, 1, 8001).is_err(),
            "split above max"
        );
    }
}

#[cfg(test)]
mod shared_core_equivalence {
    use super::*;
    use chain_core::game;

    /// The program's `resolve_game` and the shared core's `amounts_for_kind`
    /// must produce IDENTICAL splits for every reachable input.
    ///
    /// This is the bridge that makes deleting the local matrix safe. The
    /// constants and the score formula already moved to `chain_core::game`, but
    /// the matrix itself still lives in both places, so until this test exists
    /// "they agree" is an inspection claim — exactly the state that let the
    /// Solana and Solidity implementations drift apart unnoticed.
    ///
    /// Enumerates the whole reachable domain rather than sampling: both matchup
    /// types x both guesses x both first-committer values x several stakes,
    /// including odd stakes where integer division could leak a lamport.
    #[test]
    fn resolve_game_matches_the_shared_core_for_every_input() {
        const SAME: u8 = crate::state::GUESS_SAME_TEAM;
        const DIFF: u8 = 1;

        for stake in [1u64, 2, 3, 7, 50_000_000, 68_482_585] {
            for (m, p1, p2, first, expected_kind) in [
                // Homogeneous (matchup 0): correct guess is SAME.
                (0u8, SAME, SAME, 1u8, game::HOMOG_BOTH_CORRECT),
                (0, SAME, DIFF, 1, game::HOMOG_P1_CORRECT),
                (0, DIFF, SAME, 1, game::HOMOG_P2_CORRECT),
                (0, DIFF, DIFF, 1, game::BOTH_WRONG),
                // Heterogeneous (matchup 1): correct guess is DIFFERENT.
                (1, DIFF, SAME, 1, game::HETERO_P1_WINS),
                (1, SAME, DIFF, 1, game::HETERO_P2_WINS),
                (1, SAME, SAME, 1, game::BOTH_WRONG),
                // Both correct in a hetero match: the FIRST COMMITTER takes the
                // pot. first_committer is 1 or 2, never 0.
                (1, DIFF, DIFF, 1, game::HETERO_P1_WINS),
                (1, DIFF, DIFF, 2, game::HETERO_P2_WINS),
            ] {
                let local = resolve_game(m, p1, p2, stake, first).unwrap_or_else(|e| {
                    panic!("resolve_game({m},{p1},{p2},{stake},{first}): {e:?}")
                });
                let core = game::amounts_for_kind(expected_kind, stake)
                    .unwrap_or_else(|e| panic!("core kind {expected_kind} stake {stake}: {e:?}"));

                assert_eq!(
                    (local.p1_return, local.p2_return, local.tournament_gain),
                    (core.p1, core.p2, core.gain),
                    "DIVERGENCE at matchup={m} p1={p1} p2={p2} first={first} stake={stake} \
                     (expected kind {expected_kind})"
                );
            }
        }
    }
}
