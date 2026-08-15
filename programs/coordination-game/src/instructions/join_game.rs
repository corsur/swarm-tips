use crate::errors::CoordinationError;
use crate::events::GameStarted;
use crate::instructions::utils::transfer_lamports;
use crate::state::{Game, GameState, GlobalConfig, PlayerProfile, StakeEscrow, Tournament};
use anchor_lang::prelude::*;

/// Player 2 joins an existing game. P1 is set at create_game time, so this
/// instruction is P2-only. Transitions the game from Pending to Active.
///
/// The matchmaker co-signs (like create_game) so only the wallet the matchmaker
/// paired against P1 can take the P2 slot. Without it, join_game was
/// permissionless while Pending — any funded wallet that saw the game_id (in the
/// GameCreated event / mempool) could front-run the intended opponent and strand
/// them in a game they never staked into. The co-sign proves nothing about who
/// P2's wallet is to P1 (game-api signs it server-side), so anonymity holds.
pub fn join_game(ctx: Context<JoinGame>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let player_key = ctx.accounts.player.key();
    let tournament_id = ctx.accounts.tournament.tournament_id;
    // Checks
    validate_join_inputs(
        &ctx.accounts.game,
        &ctx.accounts.tournament,
        &ctx.accounts.escrow,
        &player_key,
        ctx.accounts.matchmaker.key(),
        ctx.accounts.global_config.matchmaker,
        now,
    )?;
    init_player_profile_if_new(
        &mut ctx.accounts.player_profile,
        player_key,
        tournament_id,
        ctx.bumps.player_profile,
    )?;
    // Effects
    let stake_lamports = ctx.accounts.game.stake_lamports;
    let current_slot = Clock::get()?.slot;
    commit_join_state(
        &mut ctx.accounts.game,
        &mut ctx.accounts.escrow,
        player_key,
        current_slot,
    )?;
    let game_id = ctx.accounts.game.game_id;
    let player_one = ctx.accounts.game.player_one;
    // Interactions
    transfer_lamports(
        &ctx.accounts.escrow.to_account_info(),
        &ctx.accounts.game.to_account_info(),
        stake_lamports,
    )?;
    emit!(GameStarted {
        game_id,
        tournament_id,
        player_one,
        player_two: player_key,
    });
    Ok(())
}

/// Pure check phase: state, tournament window, not-self-join, matchmaker
/// authorization, escrow valid. The matchmaker check mirrors create_game: only
/// the configured matchmaker key may co-sign a join, so a stranger cannot join a
/// game they were not paired into.
#[allow(clippy::too_many_arguments)]
pub(crate) fn validate_join_inputs(
    game: &Game,
    tournament: &Tournament,
    escrow: &StakeEscrow,
    player_key: &Pubkey,
    matchmaker_key: Pubkey,
    expected_matchmaker: Pubkey,
    now: i64,
) -> Result<()> {
    require!(
        game.state == GameState::Pending,
        CoordinationError::InvalidGameState
    );
    require!(
        tournament.is_active(now),
        CoordinationError::OutsideTournamentWindow,
    );
    require!(
        *player_key != game.player_one,
        CoordinationError::CannotJoinOwnGame,
    );
    require!(
        matchmaker_key == expected_matchmaker,
        CoordinationError::NotMatchmaker,
    );
    require!(
        escrow.validate_for_game(player_key, tournament.tournament_id, game.stake_lamports),
        CoordinationError::EscrowInvalid,
    );
    Ok(())
}

/// Initialize the PlayerProfile PDA the first time this player participates
/// in this tournament. Returns Ok if the profile is already initialized for
/// this tournament; errors if it's set to a DIFFERENT tournament.
pub(crate) fn init_player_profile_if_new(
    profile: &mut PlayerProfile,
    player_key: Pubkey,
    tournament_id: u64,
    bump: u8,
) -> Result<()> {
    profile.init_if_new(player_key, tournament_id, bump);
    require!(
        profile.tournament_id == tournament_id,
        CoordinationError::ProfileTournamentMismatch,
    );
    Ok(())
}

/// Pure effect phase. Sets P2, transitions to Active, consumes escrow.
/// Asserts the postcondition (Active state + both player slots filled).
pub(crate) fn commit_join_state(
    game: &mut Game,
    escrow: &mut StakeEscrow,
    player_key: Pubkey,
    current_slot: u64,
) -> Result<()> {
    game.player_two = player_key;
    game.state = GameState::Active;
    game.activated_at_slot = current_slot;
    escrow.consumed = true;
    require!(
        game.state == GameState::Active,
        CoordinationError::InvalidGameState
    );
    require!(
        game.player_two != Pubkey::default(),
        CoordinationError::InvalidGameState
    );
    Ok(())
}

#[derive(Accounts)]
pub struct JoinGame<'info> {
    #[account(
        mut,
        seeds = [b"game", game.game_id.to_le_bytes().as_ref()],
        bump = game.bump,
    )]
    pub game: Account<'info, Game>,
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
        seeds = [b"tournament", game.tournament_id.to_le_bytes().as_ref()],
        bump = tournament.bump,
    )]
    pub tournament: Account<'info, Tournament>,
    #[account(
        seeds = [b"global_config"],
        bump = global_config.bump,
    )]
    pub global_config: Account<'info, GlobalConfig>,
    /// Matchmaker co-signs to attest this is the paired opponent. Verified
    /// against GlobalConfig.matchmaker. Does not pay gas.
    pub matchmaker: Signer<'info>,
    #[account(mut)]
    pub player: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{COMMIT_TIMEOUT_SLOTS, GUESS_UNREVEALED, MATCHUP_TYPE_UNSET};

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

    fn pending_game(player_one: Pubkey, stake: u64) -> Game {
        Game {
            game_id: 1,
            tournament_id: 1,
            player_one,
            player_two: Pubkey::default(),
            state: GameState::Pending,
            stake_lamports: stake,
            p1_commit: [0u8; 32],
            p2_commit: [0u8; 32],
            p1_guess: GUESS_UNREVEALED,
            p2_guess: GUESS_UNREVEALED,
            first_committer: 0,
            p1_commit_slot: 0,
            p2_commit_slot: 0,
            commit_timeout_slots: COMMIT_TIMEOUT_SLOTS,
            created_at: 0,
            resolved_at: 0,
            activated_at_slot: 0,
            matchup_commitment: [0u8; 32],
            matchup_type: MATCHUP_TYPE_UNSET,
            bump: 254,
        }
    }

    fn escrow_for(player: Pubkey, stake: u64) -> StakeEscrow {
        StakeEscrow {
            player,
            tournament_id: 1,
            amount: stake,
            consumed: false,
            bump: 254,
        }
    }

    /// The paired opponent, with the real matchmaker co-signing, joins fine.
    #[test]
    fn paired_opponent_joins_with_matchmaker_cosign() {
        let now = 1_700_000_000;
        let stake = crate::state::DEFAULT_STAKE_LAMPORTS;
        let p1 = Pubkey::new_unique();
        let p2 = Pubkey::new_unique();
        let matchmaker = Pubkey::new_unique();
        assert!(validate_join_inputs(
            &pending_game(p1, stake),
            &tournament(now),
            &escrow_for(p2, stake),
            &p2,
            matchmaker, // co-signer key
            matchmaker, // == GlobalConfig.matchmaker
            now,
        )
        .is_ok());
    }

    /// The regression this guard exists for: a funded STRANGER who front-runs the
    /// real P2 cannot join, because they cannot produce the matchmaker's
    /// signature — game-api only co-signs a join for the session's real P2. Here
    /// the interloper supplies a non-matchmaker key and is rejected NotMatchmaker.
    #[test]
    fn interloper_without_matchmaker_cosign_is_rejected() {
        let now = 1_700_000_000;
        let stake = crate::state::DEFAULT_STAKE_LAMPORTS;
        let p1 = Pubkey::new_unique();
        let interloper = Pubkey::new_unique();
        let real_matchmaker = Pubkey::new_unique();
        let not_matchmaker = Pubkey::new_unique();
        let err = validate_join_inputs(
            &pending_game(p1, stake),
            &tournament(now),
            &escrow_for(interloper, stake), // interloper genuinely funded a stake
            &interloper,
            not_matchmaker,
            real_matchmaker,
            now,
        )
        .unwrap_err();
        assert_eq!(err, CoordinationError::NotMatchmaker.into());
    }

    /// A player still can't join their own game even holding a matchmaker cosign.
    #[test]
    fn cannot_join_own_game_even_with_matchmaker() {
        let now = 1_700_000_000;
        let stake = crate::state::DEFAULT_STAKE_LAMPORTS;
        let p1 = Pubkey::new_unique();
        let matchmaker = Pubkey::new_unique();
        let err = validate_join_inputs(
            &pending_game(p1, stake),
            &tournament(now),
            &escrow_for(p1, stake),
            &p1,
            matchmaker,
            matchmaker,
            now,
        )
        .unwrap_err();
        assert_eq!(err, CoordinationError::CannotJoinOwnGame.into());
    }
}
