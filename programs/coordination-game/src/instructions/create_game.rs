use crate::errors::CoordinationError;
use crate::events::GameCreated;
use crate::instructions::utils::{init_game, transfer_lamports, validate_tournament_cutoff};
use crate::state::{
    Game, GameCounter, GameState, GlobalConfig, PlayerProfile, StakeEscrow, Tournament,
    MATCHUP_TYPE_UNSET,
};
use anchor_lang::prelude::*;

/// Player creates a game with a matchmaker-attested matchup commitment.
///
/// The matchmaker co-signs to prove the commitment is legitimate (prevents players
/// from forging their own commitment and knowing the matchup type). The player
/// pays all gas. The matchmaker never pays.
///
/// NOTE ON WHERE THAT PROPERTY ACTUALLY LIVES: this program only checks THAT the
/// matchmaker signed (`matchmaker: Signer` + the `expected_matchmaker` equality
/// below). It cannot check WHAT was attested to, because it has no record of the
/// session's commitment. The forgery guard therefore lives entirely in game-api's
/// `/games/cosign`, which compares the create_game `matchup_commitment` argument
/// against the paired session's before it signs.
///
/// This distinction is not academic: until 2026-08-14 that comparison did not
/// exist, so a matched player could have a self-chosen commitment co-signed,
/// learn `matchup_type` before committing, and leave their opponent unable to
/// reveal (`CommitmentMismatch`) before taking the pot via `resolve_timeout`.
/// If the cosign check is ever weakened, this docstring becomes false again.
///
/// The matchup_commitment is SHA-256(R_matchup) where R_matchup[31] & 1 encodes
/// the matchup type (0 = same team, 1 = different teams). The actual matchup_type
/// is revealed during the first guess reveal, after both players have committed
/// their guesses (so neither can change their guess based on matchup knowledge).
pub fn create_game(
    ctx: Context<CreateGame>,
    stake_lamports: u64,
    matchup_commitment: [u8; 32],
) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let player_key = ctx.accounts.player.key();
    let tournament_id = ctx.accounts.tournament.tournament_id;
    // Checks
    validate_create_inputs(
        stake_lamports,
        matchup_commitment,
        ctx.accounts.matchmaker.key(),
        ctx.accounts.global_config.matchmaker,
        &ctx.accounts.tournament,
        &ctx.accounts.escrow,
        &player_key,
        tournament_id,
        now,
        ctx.accounts.global_config.stake_lamports,
    )?;
    // Effects
    let game_id = commit_creation_state(
        &mut ctx.accounts.game_counter,
        &mut ctx.accounts.game,
        &mut ctx.accounts.player_profile,
        &mut ctx.accounts.escrow,
        tournament_id,
        player_key,
        stake_lamports,
        matchup_commitment,
        now,
        ctx.bumps.game,
        ctx.bumps.player_profile,
    )?;
    // Interactions: move the stake from escrow to the Game PDA.
    transfer_lamports(
        &ctx.accounts.escrow.to_account_info(),
        &ctx.accounts.game.to_account_info(),
        stake_lamports,
    )?;
    emit!(GameCreated {
        game_id,
        tournament_id,
        player_one: player_key,
        stake_lamports,
    });
    Ok(())
}

/// Pure check phase shared between `create_game` and `create_game_session`
/// (the session variant has its own `validate_session_authority` step in
/// front; the rule set after that is identical).
#[allow(clippy::too_many_arguments)]
pub(crate) fn validate_create_inputs(
    stake_lamports: u64,
    matchup_commitment: [u8; 32],
    matchmaker_key: Pubkey,
    expected_matchmaker: Pubkey,
    tournament: &Tournament,
    escrow: &StakeEscrow,
    player_key: &Pubkey,
    tournament_id: u64,
    now: i64,
    expected_stake: u64,
) -> Result<()> {
    // The live configured stake, not a compile-time constant — re-pegging must
    // not require a program upgrade (that asymmetry is why Solana drifted to
    // $3.64 against a $5 EVM anchor).
    require!(
        stake_lamports == expected_stake,
        CoordinationError::StakeMismatch
    );
    require!(
        matchmaker_key == expected_matchmaker,
        CoordinationError::NotMatchmaker
    );
    require!(
        matchup_commitment != [0u8; 32],
        CoordinationError::InvalidGameState
    );
    require!(
        tournament.is_active(now),
        CoordinationError::OutsideTournamentWindow,
    );
    validate_tournament_cutoff(now, tournament.end_time)?;
    require!(
        escrow.validate_for_game(player_key, tournament_id, expected_stake),
        CoordinationError::EscrowInvalid,
    );
    Ok(())
}

/// Pure effect phase shared between `create_game` and `create_game_session`.
/// Increments the global counter, inits the Game PDA, inits or refreshes
/// the PlayerProfile, consumes the escrow, asserts the postconditions
/// (Pending state + UNSET matchup). Returns the assigned `game_id`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn commit_creation_state(
    counter: &mut GameCounter,
    game: &mut Game,
    player_profile: &mut PlayerProfile,
    escrow: &mut StakeEscrow,
    tournament_id: u64,
    player_key: Pubkey,
    stake_lamports: u64,
    matchup_commitment: [u8; 32],
    now: i64,
    game_bump: u8,
    profile_bump: u8,
) -> Result<u64> {
    let game_id = counter.count;
    counter.count = counter
        .count
        .checked_add(1)
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    init_game(
        game,
        game_id,
        tournament_id,
        player_key,
        stake_lamports,
        matchup_commitment,
        now,
        game_bump,
    );
    player_profile.init_if_new(player_key, tournament_id, profile_bump);
    require!(
        player_profile.tournament_id == tournament_id,
        CoordinationError::ProfileTournamentMismatch,
    );
    escrow.consumed = true;
    require!(
        game.state == GameState::Pending,
        CoordinationError::InvalidGameState
    );
    require!(
        game.matchup_type == MATCHUP_TYPE_UNSET,
        CoordinationError::InvalidGameState
    );
    Ok(game_id)
}

#[derive(Accounts)]
#[instruction(stake_lamports: u64, matchup_commitment: [u8; 32])]
pub struct CreateGame<'info> {
    #[account(
        init,
        payer = player,
        space = Game::SPACE,
        seeds = [b"game", game_counter.count.to_le_bytes().as_ref()],
        bump,
    )]
    pub game: Account<'info, Game>,
    #[account(
        mut,
        seeds = [b"game_counter"],
        bump = game_counter.bump,
    )]
    pub game_counter: Account<'info, GameCounter>,
    #[account(
        init_if_needed,
        payer = player,
        space = PlayerProfile::SPACE,
        seeds = [
            b"player",
            tournament.tournament_id.to_le_bytes().as_ref(),
            player.key().as_ref(),
        ],
        bump,
    )]
    pub player_profile: Account<'info, PlayerProfile>,
    #[account(
        mut,
        seeds = [
            b"escrow",
            tournament.tournament_id.to_le_bytes().as_ref(),
            player.key().as_ref(),
        ],
        bump = escrow.bump,
    )]
    pub escrow: Account<'info, StakeEscrow>,
    #[account(
        seeds = [b"tournament", tournament.tournament_id.to_le_bytes().as_ref()],
        bump = tournament.bump,
    )]
    pub tournament: Account<'info, Tournament>,
    #[account(
        seeds = [b"global_config"],
        bump = global_config.bump,
    )]
    pub global_config: Account<'info, GlobalConfig>,
    /// Matchmaker co-signs to attest the commitment is legitimate.
    /// Verified against GlobalConfig.matchmaker. Does not pay gas.
    pub matchmaker: Signer<'info>,
    #[account(mut)]
    pub player: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tournament(now: i64) -> Tournament {
        Tournament {
            tournament_id: 1,
            authority: Pubkey::new_unique(),
            start_time: now,
            end_time: now.saturating_add(86_400),
            prize_lamports: 0,
            game_count: 0,
            finalized: false,
            prize_snapshot: 0,
            merkle_root: [0u8; 32],
            bump: 254,
        }
    }

    fn escrow_at(player: Pubkey, amount: u64) -> StakeEscrow {
        StakeEscrow {
            player,
            tournament_id: 1,
            amount,
            consumed: false,
            bump: 254,
        }
    }

    /// THE ORDERING CONSTRAINT FOR A FLOATING STAKE.
    ///
    /// The stake-coherence plan's Phase 3 has two on-chain parts: give
    /// `deposit_stake` an amount parameter, and replace `create_game`'s
    /// equality-against-config with a bounds check. They are NOT independent,
    /// and the dependency runs the opposite way to the obvious reading:
    ///
    ///   deposit_stake -> escrow.amount = live_stake() = config.stake_lamports
    ///   create_game   -> require!(stake_lamports == expected_stake)   <-- config
    ///                 -> escrow.validate_for_game(.., expected_stake) <-- config
    ///
    /// Both of `create_game`'s gates compare against the CONFIG value, so an
    /// escrow funded at any other amount is rejected twice over. Adding the
    /// amount parameter to `deposit_stake` on its own therefore changes no
    /// behaviour whatsoever — it would be an instruction-signature change to a
    /// live-money mainnet program that buys exactly nothing.
    ///
    /// This test exists so that ordering is enforced rather than merely
    /// documented. It pins the gate that a floating stake must go through, so
    /// whoever implements the bounds check has to come here and state the new
    /// rule deliberately, instead of discovering on devnet that quoted stakes
    /// silently fail to pair.
    #[test]
    fn a_quoted_stake_cannot_enter_a_game_while_create_game_compares_against_config() {
        let now = 1_700_000_000;
        let t = tournament(now);
        let player = Pubkey::new_unique();
        let matchmaker = Pubkey::new_unique();
        let commitment = [7u8; 32];
        let configured = crate::state::DEFAULT_STAKE_LAMPORTS;

        // Baseline: at the configured amount, everything else about this call
        // is valid — so any failure below is attributable to the amount alone.
        assert!(validate_create_inputs(
            configured,
            commitment,
            matchmaker,
            matchmaker,
            &t,
            &escrow_at(player, configured),
            &player,
            1,
            now,
            configured,
        )
        .is_ok());

        // A per-match quote above the config: rejected, even though the player
        // genuinely funded that much and both players would quote identically.
        let quoted_high = configured.saturating_add(18_482_585);
        assert!(
            validate_create_inputs(
                quoted_high,
                commitment,
                matchmaker,
                matchmaker,
                &t,
                &escrow_at(player, quoted_high),
                &player,
                1,
                now,
                configured,
            )
            .is_err(),
            "a quoted stake above the configured one must be rejected until bounds replace equality"
        );

        // And below, so this is a fixed-point rule and not a ceiling.
        let quoted_low = configured.saturating_sub(1);
        assert!(
            validate_create_inputs(
                quoted_low,
                commitment,
                matchmaker,
                matchmaker,
                &t,
                &escrow_at(player, quoted_low),
                &player,
                1,
                now,
                configured,
            )
            .is_err(),
            "a quoted stake below the configured one must be rejected too"
        );

        // The escrow gate is INDEPENDENT of the argument gate: even when the
        // argument matches config, an escrow funded at a quoted amount fails.
        // This is the half that a `deposit_stake(amount)` change would hit
        // first, and the reason that change cannot land alone.
        assert!(
            validate_create_inputs(
                configured,
                commitment,
                matchmaker,
                matchmaker,
                &t,
                &escrow_at(player, quoted_high),
                &player,
                1,
                now,
                configured,
            )
            .is_err(),
            "an escrow funded at a quoted amount must be rejected by validate_for_game"
        );

        // And the mirror image, which pins the ARGUMENT gate on its own: escrow
        // at the configured amount, argument quoted. Without this case the test
        // passes with `stake_lamports == expected_stake` deleted entirely — the
        // escrow gate alone catches every other combination here, so the two
        // gates have to be separated to be individually protected.
        assert!(
            validate_create_inputs(
                quoted_high,
                commitment,
                matchmaker,
                matchmaker,
                &t,
                &escrow_at(player, configured),
                &player,
                1,
                now,
                configured,
            )
            .is_err(),
            "a quoted stake argument must be rejected by the config-equality gate"
        );
    }
}
