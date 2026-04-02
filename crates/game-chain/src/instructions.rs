//! Instruction builders for the coordination game program.
//!
//! Each function constructs a complete `solana_sdk::Instruction` with the
//! correct accounts, data, and program ID. Uses the `coordination` crate's
//! typed instruction data (Anchor `InstructionData` trait) to produce the
//! correct discriminator + serialized args.

use anchor_lang::InstructionData;
use coordination::{
    instruction::{CommitGuess, CreateGame, DepositStake, JoinGame, RevealGuess},
    ID as PROGRAM_ID,
};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey,
    pubkey::Pubkey,
    signer::Signer,
};

use crate::pda;

/// System program ID, used in instructions that transfer lamports.
const SYSTEM_PROGRAM_ID: Pubkey = pubkey!("11111111111111111111111111111111");

/// Build the `CreateGame` instruction.
///
/// Player 1 creates a game with the matchmaker-attested matchup commitment.
/// The matchmaker co-signs (signature added separately via the `/games/cosign`
/// endpoint). The caller must read `game_counter.count` from on-chain to
/// derive the correct game PDA seed.
///
/// Account order matches `CreateGame<'info>` in the on-chain program.
pub fn build_create_game(
    stake_lamports: u64,
    matchup_commitment: [u8; 32],
    tournament_id: u64,
    game_counter_value: u64,
    player: &dyn Signer,
    matchmaker: &Pubkey,
) -> Instruction {
    assert!(tournament_id > 0, "tournament_id must be non-zero");
    assert!(
        matchup_commitment != [0u8; 32],
        "matchup_commitment must not be all zeros"
    );

    let (game_pda, _) = pda::game_pda(game_counter_value);
    let (game_counter_pda, _) = pda::game_counter_pda();
    let (tournament_pda, _) = pda::tournament_pda(tournament_id);
    let (profile_pda, _) = pda::player_profile_pda(tournament_id, &player.pubkey());
    let (escrow_pda, _) = pda::escrow_pda(tournament_id, &player.pubkey());
    let (global_config_pda, _) = pda::global_config_pda();

    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(game_pda, false),
            AccountMeta::new(game_counter_pda, false),
            AccountMeta::new(profile_pda, false),
            AccountMeta::new(escrow_pda, false),
            AccountMeta::new_readonly(tournament_pda, false),
            AccountMeta::new_readonly(global_config_pda, false),
            AccountMeta::new_readonly(*matchmaker, true), // signer (cosigned)
            AccountMeta::new(player.pubkey(), true),      // signer + payer
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ],
        data: CreateGame {
            stake_lamports,
            matchup_commitment,
        }
        .data(),
    }
}

/// Build the `DepositStake` instruction.
///
/// Deposits the fixed stake into the per-player escrow PDA for the
/// given tournament.
pub fn build_deposit_stake(tournament_id: u64, payer: &dyn Signer) -> Instruction {
    assert!(tournament_id > 0, "tournament_id must be non-zero");

    let (tournament_pda, _) = pda::tournament_pda(tournament_id);
    let (escrow_pda, _) = pda::escrow_pda(tournament_id, &payer.pubkey());

    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(escrow_pda, false),
            AccountMeta::new_readonly(tournament_pda, false),
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ],
        data: DepositStake {}.data(),
    }
}

/// Build the `JoinGame` instruction.
///
/// Joins an existing game as Player 2. The game must already have been
/// created by the matchmaker.
pub fn build_join_game(game_id: u64, tournament_id: u64, player: &dyn Signer) -> Instruction {
    assert!(game_id > 0, "game_id must be non-zero");
    assert!(tournament_id > 0, "tournament_id must be non-zero");

    let (game_pda, _) = pda::game_pda(game_id);
    let (tournament_pda, _) = pda::tournament_pda(tournament_id);
    let (profile_pda, _) = pda::player_profile_pda(tournament_id, &player.pubkey());
    let (escrow_pda, _) = pda::escrow_pda(tournament_id, &player.pubkey());

    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(game_pda, false),
            AccountMeta::new(profile_pda, false),
            AccountMeta::new(escrow_pda, false),
            AccountMeta::new_readonly(tournament_pda, false),
            AccountMeta::new(player.pubkey(), true),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ],
        data: JoinGame {}.data(),
    }
}

/// Build the `CommitGuess` instruction.
///
/// Submits a SHA-256 commitment of the player's guess. The commitment
/// must later be revealed via `build_reveal_guess`.
pub fn build_commit_guess(game_id: u64, commitment: [u8; 32], player: &dyn Signer) -> Instruction {
    assert!(game_id > 0, "game_id must be non-zero");

    let (game_pda, _) = pda::game_pda(game_id);

    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(game_pda, false),
            AccountMeta::new_readonly(player.pubkey(), true),
        ],
        data: CommitGuess { commitment }.data(),
    }
}

/// Build the `RevealGuess` instruction.
///
/// Reveals the preimage and (optionally) the matchup preimage. The
/// on-chain program verifies `SHA-256(preimage) == commitment` and
/// resolves the game if both players have revealed.
#[allow(clippy::too_many_arguments)]
pub fn build_reveal_guess(
    game_id: u64,
    tournament_id: u64,
    preimage: [u8; 32],
    r_matchup: Option<[u8; 32]>,
    player: &dyn Signer,
    player_one: Pubkey,
    player_two: Pubkey,
    global_config_pda: Pubkey,
    treasury: Pubkey,
) -> Instruction {
    assert!(game_id > 0, "game_id must be non-zero");
    assert!(tournament_id > 0, "tournament_id must be non-zero");

    let (game_pda, _) = pda::game_pda(game_id);
    let (tournament_pda, _) = pda::tournament_pda(tournament_id);
    let (p1_profile, _) = pda::player_profile_pda(tournament_id, &player_one);
    let (p2_profile, _) = pda::player_profile_pda(tournament_id, &player_two);

    Instruction {
        program_id: PROGRAM_ID,
        // Order matches RevealGuess<'info> in the program.
        accounts: vec![
            AccountMeta::new(game_pda, false),
            AccountMeta::new_readonly(player.pubkey(), true),
            AccountMeta::new(p1_profile, false),
            AccountMeta::new(p2_profile, false),
            AccountMeta::new(tournament_pda, false),
            AccountMeta::new_readonly(global_config_pda, false),
            AccountMeta::new(treasury, false),
            AccountMeta::new(player_one, false),
            AccountMeta::new(player_two, false),
        ],
        data: RevealGuess {
            r: preimage,
            r_matchup,
        }
        .data(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::signature::Keypair;

    #[test]
    fn build_deposit_stake_has_correct_program_id() {
        let kp = Keypair::new();
        let ix = build_deposit_stake(1, &kp);
        assert_eq!(ix.program_id, PROGRAM_ID);
        // Precondition: 4 accounts (escrow, tournament, payer, system).
        assert_eq!(ix.accounts.len(), 4, "deposit_stake must have 4 accounts");
    }

    #[test]
    fn build_deposit_stake_payer_is_signer() {
        let kp = Keypair::new();
        let ix = build_deposit_stake(1, &kp);
        let payer_meta = &ix.accounts[2];
        assert!(payer_meta.is_signer, "payer must be a signer");
        assert!(payer_meta.is_writable, "payer must be writable");
    }

    #[test]
    #[should_panic(expected = "tournament_id must be non-zero")]
    fn build_deposit_stake_rejects_zero_tournament() {
        let kp = Keypair::new();
        let _ = build_deposit_stake(0, &kp);
    }

    #[test]
    fn build_join_game_has_correct_accounts() {
        let kp = Keypair::new();
        let ix = build_join_game(1, 1, &kp);
        assert_eq!(ix.program_id, PROGRAM_ID);
        // 6 accounts: game, profile, escrow, tournament, player, system.
        assert_eq!(ix.accounts.len(), 6, "join_game must have 6 accounts");
    }

    #[test]
    #[should_panic(expected = "game_id must be non-zero")]
    fn build_join_game_rejects_zero_game_id() {
        let kp = Keypair::new();
        let _ = build_join_game(0, 1, &kp);
    }

    #[test]
    fn build_commit_guess_has_correct_accounts() {
        let kp = Keypair::new();
        let ix = build_commit_guess(1, [0u8; 32], &kp);
        assert_eq!(ix.program_id, PROGRAM_ID);
        // 2 accounts: game, player.
        assert_eq!(ix.accounts.len(), 2, "commit_guess must have 2 accounts");
    }

    #[test]
    fn build_reveal_guess_has_correct_accounts() {
        let kp = Keypair::new();
        let p1 = Pubkey::new_unique();
        let p2 = Pubkey::new_unique();
        let (gc, _) = pda::global_config_pda();
        let treasury = Pubkey::new_unique();
        let ix = build_reveal_guess(1, 1, [0u8; 32], None, &kp, p1, p2, gc, treasury);
        assert_eq!(ix.program_id, PROGRAM_ID);
        // 9 accounts: game, player, p1_profile, p2_profile, tournament,
        //             global_config, treasury, player_one, player_two.
        assert_eq!(ix.accounts.len(), 9, "reveal_guess must have 9 accounts");
    }

    #[test]
    #[should_panic(expected = "game_id must be non-zero")]
    fn build_commit_guess_rejects_zero_game_id() {
        let kp = Keypair::new();
        let _ = build_commit_guess(0, [0u8; 32], &kp);
    }
}
