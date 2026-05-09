use crate::errors::CoordinationError;
use crate::events::{GameResolved, GuessRevealed};
use crate::instructions::utils::{
    apply_finalize_effects, compute_finalization, distribute_finalize_lamports,
};
use crate::state::{
    Game, GameState, GlobalConfig, PlayerProfile, Tournament, GUESS_UNREVEALED, MATCHUP_TYPE_UNSET,
};
use anchor_lang::prelude::*;
use solana_sha256_hasher::hashv;

/// Reveal a guess. The first revealer must also provide `r_matchup` to reveal
/// the matchup type (if still unset). The second revealer can pass None.
pub fn reveal_guess(
    ctx: Context<RevealGuess>,
    r: [u8; 32],
    r_matchup: Option<[u8; 32]>,
) -> Result<()> {
    let player_key = ctx.accounts.player.key();

    // Checks: state + participant + commitment match.
    let is_p1 = validate_revealer(&ctx.accounts.game, &player_key)?;
    let guess = verify_and_extract_guess(&ctx.accounts.game, is_p1, &r)?;

    // Effects: record the player's guess and (if first revealer) the matchup.
    let game = &mut ctx.accounts.game;
    record_revealed_guess(game, is_p1, guess);
    reveal_matchup_if_first(game, r_matchup)?;

    emit!(GuessRevealed {
        game_id: game.game_id,
        player: player_key
    });

    // Interactions: if both revealed, finalize (transfers + state).
    let both_revealed = game.p1_guess != GUESS_UNREVEALED && game.p2_guess != GUESS_UNREVEALED;
    if both_revealed {
        finalize_game(ctx)?;
    }

    Ok(())
}

/// Pure check: game is in the right state, the caller is a participant, and
/// they haven't revealed yet. Returns `true` if the caller is player_one.
pub(crate) fn validate_revealer(game: &Game, player_key: &Pubkey) -> Result<bool> {
    require!(
        game.state == GameState::Revealing,
        CoordinationError::InvalidGameState,
    );
    let is_p1 = *player_key == game.player_one;
    let is_p2 = *player_key == game.player_two;
    require!(is_p1 || is_p2, CoordinationError::NotAParticipant);
    let already_revealed = if is_p1 {
        game.p1_guess != GUESS_UNREVEALED
    } else {
        game.p2_guess != GUESS_UNREVEALED
    };
    require!(!already_revealed, CoordinationError::AlreadyRevealed);
    Ok(is_p1)
}

/// Verify the caller's commitment matches `sha256(r)` and extract the
/// guess bit. Returns the 0/1 guess.
pub(crate) fn verify_and_extract_guess(game: &Game, is_p1: bool, r: &[u8; 32]) -> Result<u8> {
    let computed: [u8; 32] = hashv(&[r.as_ref()]).to_bytes();
    let stored = if is_p1 {
        game.p1_commit
    } else {
        game.p2_commit
    };
    require!(computed == stored, CoordinationError::CommitmentMismatch);
    let guess = r[31] & 1;
    require!(guess <= 1, CoordinationError::InvalidGuessValue);
    Ok(guess)
}

/// Pure effect: store the revealed guess on the correct player slot.
pub(crate) fn record_revealed_guess(game: &mut Game, is_p1: bool, guess: u8) {
    if is_p1 {
        game.p1_guess = guess;
    } else {
        game.p2_guess = guess;
    }
}

/// First revealer provides `r_matchup` to reveal the matchup type. Second
/// revealer MUST pass `None` — the simpler rule (reject any r_matchup the
/// second revealer passes) prevents a class of "what if it disagrees" bugs.
pub(crate) fn reveal_matchup_if_first(game: &mut Game, r_matchup: Option<[u8; 32]>) -> Result<()> {
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
        require!(r_matchup.is_none(), CoordinationError::RMatchupMismatch);
    }
    Ok(())
}

fn finalize_game(ctx: Context<RevealGuess>) -> Result<()> {
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
pub struct RevealGuess<'info> {
    #[account(
        mut,
        seeds = [b"game", game.game_id.to_le_bytes().as_ref()],
        bump = game.bump,
    )]
    pub game: Account<'info, Game>,
    pub player: Signer<'info>,
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
