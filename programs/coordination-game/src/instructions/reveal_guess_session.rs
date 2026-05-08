use crate::errors::CoordinationError;
use crate::events::{GameResolved, GuessRevealed};
use crate::instructions::utils::{
    apply_finalize_effects, compute_finalization, distribute_finalize_lamports,
};
use crate::state::{
    Game, GameState, GlobalConfig, PlayerProfile, SessionAuthority, Tournament, GUESS_UNREVEALED,
    MATCHUP_TYPE_UNSET,
};
use anchor_lang::prelude::*;
use solana_sha256_hasher::hashv;

/// Session-delegated variant of `reveal_guess`. The session key signs instead
/// of the player wallet. The first revealer must also provide `r_matchup` to
/// reveal the matchup type (if still unset).
pub fn reveal_guess_session(
    ctx: Context<RevealGuessSession>,
    r: [u8; 32],
    r_matchup: Option<[u8; 32]>,
) -> Result<()> {
    crate::instructions::session_utils::validate_session_authority(
        &ctx.accounts.session_authority,
        &ctx.accounts.player.key(),
        &ctx.accounts.session_signer.key(),
    )?;

    require!(
        ctx.accounts.game.state == GameState::Revealing,
        CoordinationError::InvalidGameState,
    );

    let player_key = ctx.accounts.player.key();
    let game = &ctx.accounts.game;
    let is_p1 = player_key == game.player_one;
    let is_p2 = player_key == game.player_two;
    require!(is_p1 || is_p2, CoordinationError::NotAParticipant);

    if is_p1 {
        require!(
            game.p1_guess == GUESS_UNREVEALED,
            CoordinationError::AlreadyRevealed
        );
    } else {
        require!(
            game.p2_guess == GUESS_UNREVEALED,
            CoordinationError::AlreadyRevealed
        );
    }

    // Verify commitment: SHA-256(r) via sol_sha256 syscall
    let computed: [u8; 32] = hashv(&[r.as_ref()]).to_bytes();
    let stored = if is_p1 {
        game.p1_commit
    } else {
        game.p2_commit
    };
    require!(computed == stored, CoordinationError::CommitmentMismatch);

    // Extract guess from the last bit of r
    let guess = r[31] & 1;
    require!(guess <= 1, CoordinationError::InvalidGuessValue);

    let game = &mut ctx.accounts.game;
    if is_p1 {
        game.p1_guess = guess;
    } else {
        game.p2_guess = guess;
    }

    // Reveal matchup type if still unset (first revealer provides r_matchup)
    if game.matchup_type == MATCHUP_TYPE_UNSET {
        let r_mu = r_matchup.ok_or(error!(CoordinationError::InvalidGameState))?;
        let computed_commitment: [u8; 32] = hashv(&[r_mu.as_ref()]).to_bytes();
        require!(
            computed_commitment == game.matchup_commitment,
            CoordinationError::CommitmentMismatch
        );
        let matchup_type = r_mu[31] & 1;
        require!(matchup_type <= 1, CoordinationError::InvalidGameState);
        game.matchup_type = matchup_type;
    } else {
        // See `reveal_guess.rs` for the rationale: simpler rule, second
        // revealer must NOT pass r_matchup.
        require!(r_matchup.is_none(), CoordinationError::RMatchupMismatch);
    }

    emit!(GuessRevealed {
        game_id: game.game_id,
        player: player_key,
    });

    let both_revealed = game.p1_guess != GUESS_UNREVEALED && game.p2_guess != GUESS_UNREVEALED;

    if both_revealed {
        finalize_game_session(ctx)?;
    }

    Ok(())
}

fn finalize_game_session(ctx: Context<RevealGuessSession>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let game_id = ctx.accounts.game.game_id;
    let tournament_id = ctx.accounts.game.tournament_id;
    let outcome = compute_finalization(
        &ctx.accounts.game,
        ctx.accounts.tournament.end_time,
        ctx.accounts.global_config.treasury_split_bps,
        now,
    )?;
    apply_finalize_effects(
        &mut ctx.accounts.tournament,
        &mut ctx.accounts.p1_profile,
        &mut ctx.accounts.p2_profile,
        &mut ctx.accounts.game,
        &outcome,
        tournament_id,
        now,
    )?;
    let p1_guess = ctx.accounts.game.p1_guess;
    let p2_guess = ctx.accounts.game.p2_guess;
    distribute_finalize_lamports(
        &ctx.accounts.game.to_account_info(),
        &ctx.accounts.player_one_wallet.to_account_info(),
        &ctx.accounts.player_two_wallet.to_account_info(),
        &ctx.accounts.treasury.to_account_info(),
        &ctx.accounts.tournament.to_account_info(),
        &outcome,
    )?;
    emit!(GameResolved {
        game_id,
        p1_guess,
        p2_guess,
        p1_return: outcome.p1_return,
        p2_return: outcome.p2_return,
        tournament_gain: outcome.tournament_gain,
        treasury_gain: outcome.treasury_share,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct RevealGuessSession<'info> {
    #[account(
        mut,
        seeds = [b"game", game.game_id.to_le_bytes().as_ref()],
        bump = game.bump,
    )]
    pub game: Account<'info, Game>,
    /// CHECK: The player wallet. Not a signer — the session key signs instead.
    /// Verified against session_authority.player and game participants in the handler.
    pub player: UncheckedAccount<'info>,
    #[account(
        seeds = [
            b"game_session",
            player.key().as_ref(),
            session_signer.key().as_ref(),
        ],
        bump = session_authority.bump,
    )]
    pub session_authority: Account<'info, SessionAuthority>,
    pub session_signer: Signer<'info>,
    #[account(
        mut,
        seeds = [
            b"player",
            tournament.tournament_id.to_le_bytes().as_ref(),
            game.player_one.as_ref(),
        ],
        bump = p1_profile.bump,
        constraint = p1_profile.wallet == game.player_one,
    )]
    pub p1_profile: Account<'info, PlayerProfile>,
    #[account(
        mut,
        seeds = [
            b"player",
            tournament.tournament_id.to_le_bytes().as_ref(),
            game.player_two.as_ref(),
        ],
        bump = p2_profile.bump,
        constraint = p2_profile.wallet == game.player_two,
    )]
    pub p2_profile: Account<'info, PlayerProfile>,
    #[account(
        mut,
        seeds = [b"tournament", game.tournament_id.to_le_bytes().as_ref()],
        bump = tournament.bump,
    )]
    pub tournament: Account<'info, Tournament>,
    #[account(
        seeds = [b"global_config"],
        bump = global_config.bump,
    )]
    pub global_config: Account<'info, GlobalConfig>,
    /// CHECK: DAO treasury — validated against global_config.treasury
    #[account(mut, address = global_config.treasury)]
    pub treasury: UncheckedAccount<'info>,
    /// CHECK: Destination for player one's stake return — verified by game.player_one
    #[account(mut, address = game.player_one)]
    pub player_one_wallet: UncheckedAccount<'info>,
    /// CHECK: Destination for player two's stake return — verified by game.player_two
    #[account(mut, address = game.player_two)]
    pub player_two_wallet: UncheckedAccount<'info>,
}
