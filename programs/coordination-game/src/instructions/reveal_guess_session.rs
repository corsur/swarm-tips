use crate::errors::CoordinationError;
use crate::events::{GameResolved, GuessRevealed};
use crate::instructions::utils::{compute_treasury_split, transfer_lamports};
use crate::payoff::resolve_game;
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
    let game = &ctx.accounts.game;
    let now = Clock::get()?.unix_timestamp;
    let game_id = game.game_id;
    let tournament_id = game.tournament_id;

    let (p1_return, p2_return, tournament_gain) =
        compute_returns(game, now, ctx.accounts.tournament.end_time)?;

    // Win = guessed correctly. In homogeneous both-correct, BOTH players win.
    let correct_guess = if game.matchup_type == 0 {
        crate::state::GUESS_SAME_TEAM
    } else {
        crate::state::GUESS_DIFF_TEAM
    };
    let p1_won = game.p1_guess == correct_guess;
    let p2_won = game.p2_guess == correct_guess;

    // Compute tournament share (after treasury split) for state update
    let tournament_share = if tournament_gain > 0 {
        compute_treasury_split(
            tournament_gain,
            ctx.accounts.global_config.treasury_split_bps,
        )?
        .tournament_share
    } else {
        0
    };

    // Effects: apply all state mutations before any lamport transfers
    apply_tournament_update(&mut ctx.accounts.tournament, tournament_share)?;
    ctx.accounts
        .p1_profile
        .update_after_game(p1_won, tournament_id)?;
    ctx.accounts
        .p2_profile
        .update_after_game(p2_won, tournament_id)?;
    ctx.accounts.game.state = GameState::Resolved;
    ctx.accounts.game.resolved_at = now;

    // Postcondition
    require!(
        ctx.accounts.game.state == GameState::Resolved,
        CoordinationError::InvalidGameState
    );
    require!(
        ctx.accounts.game.resolved_at == now,
        CoordinationError::InvalidGameState
    );

    // Interactions: lamport transfers after all state is committed
    let treasury_gain = distribute_lamports(&ctx, p1_return, p2_return, tournament_gain)?;

    emit!(GameResolved {
        game_id,
        p1_guess: ctx.accounts.game.p1_guess,
        p2_guess: ctx.accounts.game.p2_guess,
        p1_return,
        p2_return,
        tournament_gain,
        treasury_gain,
    });
    Ok(())
}

fn compute_returns(game: &Game, now: i64, tournament_end_time: i64) -> Result<(u64, u64, u64)> {
    if now > tournament_end_time {
        return Ok((game.stake_lamports, game.stake_lamports, 0u64));
    }
    let resolution = resolve_game(
        game.matchup_type,
        game.p1_guess,
        game.p2_guess,
        game.stake_lamports,
        game.first_committer,
    )?;
    Ok((
        resolution.p1_return,
        resolution.p2_return,
        resolution.tournament_gain,
    ))
}

/// Transfer resolved amounts from game PDA to player wallets, treasury, and tournament.
/// Returns the treasury_share for event emission.
fn distribute_lamports(
    ctx: &Context<RevealGuessSession>,
    p1_return: u64,
    p2_return: u64,
    tournament_gain: u64,
) -> Result<u64> {
    let game_info = ctx.accounts.game.to_account_info();

    transfer_lamports(
        &game_info,
        &ctx.accounts.player_one_wallet.to_account_info(),
        p1_return,
    )?;
    transfer_lamports(
        &game_info,
        &ctx.accounts.player_two_wallet.to_account_info(),
        p2_return,
    )?;

    if tournament_gain > 0 {
        let split = compute_treasury_split(
            tournament_gain,
            ctx.accounts.global_config.treasury_split_bps,
        )?;

        transfer_lamports(
            &game_info,
            &ctx.accounts.treasury.to_account_info(),
            split.treasury_share,
        )?;
        transfer_lamports(
            &game_info,
            &ctx.accounts.tournament.to_account_info(),
            split.tournament_share,
        )?;

        Ok(split.treasury_share)
    } else {
        Ok(0)
    }
}

/// Increment tournament game count and prize pool for every resolved game.
fn apply_tournament_update(tournament: &mut Tournament, tournament_share: u64) -> Result<()> {
    // Always increment game_count for ALL resolved games
    tournament.game_count = tournament
        .game_count
        .checked_add(1)
        .ok_or(CoordinationError::ArithmeticOverflow)?;

    if tournament_share > 0 {
        tournament.prize_lamports = tournament
            .prize_lamports
            .checked_add(tournament_share)
            .ok_or(CoordinationError::ArithmeticOverflow)?;
    }

    // Postcondition: game_count must have advanced
    require!(
        tournament.game_count >= 1,
        CoordinationError::ArithmeticOverflow,
    );
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
