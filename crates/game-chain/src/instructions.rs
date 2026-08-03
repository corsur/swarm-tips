//! Instruction builders for the coordination game program.
//!
//! Each function constructs a complete `solana_sdk::Instruction` with the
//! correct accounts, data, and program ID. Uses the `coordination` crate's
//! typed instruction data (Anchor `InstructionData` trait) to produce the
//! correct discriminator + serialized args.

use anchor_lang::InstructionData;
use coordination::{
    cert::{CheckpointArg, MatchLiveCertArg, MatchLiveCertNoA, OutcomeCertArg},
    instruction::{
        CloseXmatch, CommitGuess, CreateGame, CreateXmatch, DepositStake, InitializeXpool,
        JoinGame, LockXtranche, OpenXclaim, RefundPending, RefundXmatchNocert, RefundXmatchTimeout,
        RevealGuess, SettleXclaim, SettleXmatch, SubmitEquivocationProof, SupersedeXclaim,
        XpoolDeposit,
    },
    instructions::xchain::CreateXMatchArgs,
    ID as PROGRAM_ID,
};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey,
    pubkey::Pubkey,
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
    player: &Pubkey,
    matchmaker: &Pubkey,
) -> Instruction {
    assert!(tournament_id > 0, "tournament_id must be non-zero");
    // Mirrors build_create_xmatch's sibling assert. The on-chain handler
    // rejects any stake != GlobalConfig.stake_lamports anyway, so this only fails
    // earlier what would fail on-chain.
    assert!(stake_lamports > 0, "stake_lamports must be non-zero");
    assert!(
        matchup_commitment != [0u8; 32],
        "matchup_commitment must not be all zeros"
    );

    let (game_pda, _) = pda::game_pda(game_counter_value);
    let (game_counter_pda, _) = pda::game_counter_pda();
    let (tournament_pda, _) = pda::tournament_pda(tournament_id);
    let (profile_pda, _) = pda::player_profile_pda(tournament_id, player);
    let (escrow_pda, _) = pda::escrow_pda(tournament_id, player);
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
            AccountMeta::new(*player, true),              // signer + payer
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ],
        data: CreateGame {
            stake_lamports,
            matchup_commitment,
        }
        .data(),
    }
}

/// Build the `create_xmatch` instruction — the Solana leg of a cross-chain
/// match. Like `create_game` it is co-signed by the matchmaker and funded by
/// the player; `match_id` is the 32-byte cross-chain match id (seeds the
/// xmatch PDA). The stake is locked into the xmatch account, which doubles as
/// the escrow vault.
pub fn build_create_xmatch(
    match_id: [u8; 32],
    args: CreateXMatchArgs,
    player: &Pubkey,
    matchmaker: &Pubkey,
) -> Instruction {
    assert!(args.tournament_id > 0, "tournament_id must be non-zero");
    assert!(args.stake_lamports > 0, "stake_lamports must be non-zero");

    let (xmatch_pda, _) = pda::xmatch_pda(&match_id);
    let (global_config_pda, _) = pda::global_config_pda();

    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(xmatch_pda, false),
            AccountMeta::new_readonly(global_config_pda, false),
            AccountMeta::new_readonly(*matchmaker, true), // signer (cosigned)
            AccountMeta::new(*player, true),              // signer + payer
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ],
        data: CreateXmatch { match_id, args }.data(),
    }
}

/// Build the `initialize_xpool` instruction — one-time setup of the operator
/// float pool that backs every cross-chain match's tranche. Authority-signed
/// (must equal `global_config.authority`). `operator` is the Solana key allowed
/// to lock tranches; `operator_signer` is the 20-byte secp256k1 eth-address of
/// the off-chain cosigner. Account mut-flags mirror `InitializeXPool<'info>`.
pub fn build_initialize_xpool(
    operator: &Pubkey,
    operator_signer: [u8; 20],
    max_tranche_lamports: u64,
    max_claim_window_secs: u32,
    skew_margin_secs: u32,
    authority: &Pubkey,
) -> Instruction {
    assert!(skew_margin_secs > 0, "skew_margin_secs must be non-zero");
    assert!(
        operator_signer != [0u8; 20],
        "operator_signer must not be all zeros"
    );

    let (xpool_pda, _) = pda::xpool_pda();
    let (global_config_pda, _) = pda::global_config_pda();

    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(xpool_pda, false),
            AccountMeta::new_readonly(global_config_pda, false),
            AccountMeta::new(*authority, true), // signer + payer
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ],
        data: InitializeXpool {
            operator: *operator,
            operator_signer,
            max_tranche_lamports,
            max_claim_window_secs,
            skew_margin_secs,
        }
        .data(),
    }
}

/// Build the `xpool_deposit` instruction — credit `amount` lamports from the
/// funder wallet into the operator float pool's free balance via a System CPI.
/// The funder signs and pays. Account mut-flags mirror `XPoolDeposit<'info>`.
pub fn build_xpool_deposit(amount: u64, funder: &Pubkey) -> Instruction {
    assert!(amount > 0, "amount must be non-zero");

    let (xpool_pda, _) = pda::xpool_pda();

    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(xpool_pda, false),
            AccountMeta::new(*funder, true), // signer + funder
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ],
        data: XpoolDeposit { amount }.data(),
    }
}

/// Build the permissionless `lock_xtranche` instruction — moves the cert's
/// leg-A tranche from the pool's free balance into the locked balance and binds
/// it to the funded match, transitioning it to `Locked` (the precondition for
/// settle). Authorization is the operator's match-live signature over the cert
/// (the same sig relayed at pairing — no new operator action); `cranker` is a
/// permissionless fee payer (gas external). Account mut-flags mirror
/// `LockXTranche<'info>` exactly.
pub fn build_lock_xtranche(
    cert: MatchLiveCertArg,
    operator_sig: [u8; 65],
    cranker: &Pubkey,
) -> Instruction {
    let (xmatch_pda, _) = pda::xmatch_pda(&cert.match_id);
    let (xpool_pda, _) = pda::xpool_pda();

    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(xmatch_pda, false),
            AccountMeta::new(xpool_pda, false),
            AccountMeta::new(*cranker, true), // permissionless fee payer
        ],
        data: LockXtranche { cert, operator_sig }.data(),
    }
}

/// Build the permissionless `settle_xmatch` instruction — the Solana leg of a
/// cross-chain settlement. The cranker pays the player-profile rent (if the
/// cross-chain player has none for this tournament) but anyone can crank.
/// `player` is the recorded Solana player (== `xmatch.player`); `treasury`
/// must equal `global_config.treasury`. Account mut-flags mirror
/// `SettleXMatch<'info>` exactly.
#[allow(clippy::too_many_arguments)]
pub fn build_settle_xmatch(
    cert_no_a: MatchLiveCertNoA,
    outcome: OutcomeCertArg,
    live_sigs: [[u8; 65]; 3],
    oc_sigs: [[u8; 65]; 3],
    player: &Pubkey,
    treasury: &Pubkey,
    cranker: &Pubkey,
) -> Instruction {
    let (xmatch_pda, _) = pda::xmatch_pda(&cert_no_a.match_id);
    let (xpool_pda, _) = pda::xpool_pda();
    let (tournament_pda, _) = pda::tournament_pda(cert_no_a.tournament_id);
    let (profile_pda, _) = pda::player_profile_pda(cert_no_a.tournament_id, player);
    let (global_config_pda, _) = pda::global_config_pda();

    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(xmatch_pda, false),
            AccountMeta::new(xpool_pda, false),
            AccountMeta::new(tournament_pda, false),
            AccountMeta::new(profile_pda, false),
            AccountMeta::new_readonly(global_config_pda, false),
            AccountMeta::new(*treasury, false),
            AccountMeta::new(*player, false),
            AccountMeta::new(*cranker, true), // signer + payer
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ],
        data: SettleXmatch {
            cert_no_a,
            outcome,
            live_sigs,
            oc_sigs,
        }
        .data(),
    }
}

/// Build the permissionless `open_xclaim` instruction — opens the optimistic
/// claim path with a co-signed checkpoint, starting the challenge window. Used
/// when a match stalls (timeout/forfeit): the outcome is derived ON-CHAIN from
/// the checkpoint via `derive_claim_outcome`. `live_sigs` are the three
/// match-live signatures (both session keys + operator); `cp_sigs` are the two
/// checkpoint co-signatures. No signer account — the fee payer submits. Account
/// mut-flags mirror `OpenXClaim<'info>`.
pub fn build_open_xclaim(
    cert: MatchLiveCertArg,
    cp: CheckpointArg,
    live_sigs: [[u8; 65]; 3],
    cp_sigs: [[u8; 65]; 2],
) -> Instruction {
    let (xmatch_pda, _) = pda::xmatch_pda(&cert.match_id);
    let (xpool_pda, _) = pda::xpool_pda();
    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(xmatch_pda, false),
            AccountMeta::new_readonly(xpool_pda, false),
        ],
        data: OpenXclaim {
            cert,
            cp,
            live_sigs,
            cp_sigs,
        }
        .data(),
    }
}

/// Build the permissionless `supersede_xclaim` instruction — replaces the open
/// claim's checkpoint with a strictly-higher-`step_count` co-signed checkpoint
/// (reveals supersede commits) before the window closes. No signer account.
/// Account mut-flags mirror `SupersedeXClaim<'info>` (only `xmatch`, mut).
pub fn build_supersede_xclaim(
    cert: MatchLiveCertArg,
    cp: CheckpointArg,
    cp_sigs: [[u8; 65]; 2],
) -> Instruction {
    let (xmatch_pda, _) = pda::xmatch_pda(&cert.match_id);
    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![AccountMeta::new(xmatch_pda, false)],
        data: SupersedeXclaim { cert, cp, cp_sigs }.data(),
    }
}

/// Build the permissionless `submit_equivocation_proof` instruction — proves a
/// party signed two DISTINCT checkpoints at the SAME `step_count` (equivocation)
/// and applies the final equivocation verdict. `cp_a`/`cp_b` are the conflicting
/// checkpoints; `sig_a`/`sig_b` the offending party's two signatures. No signer
/// account. Account mut-flags mirror `SubmitEquivocation<'info>`.
pub fn build_submit_equivocation_proof(
    cert: MatchLiveCertArg,
    cp_a: CheckpointArg,
    cp_b: CheckpointArg,
    sig_a: [u8; 65],
    sig_b: [u8; 65],
) -> Instruction {
    let (xmatch_pda, _) = pda::xmatch_pda(&cert.match_id);
    let (xpool_pda, _) = pda::xpool_pda();
    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(xmatch_pda, false),
            AccountMeta::new_readonly(xpool_pda, false),
        ],
        data: SubmitEquivocationProof {
            cert,
            cp_a,
            cp_b,
            sig_a,
            sig_b,
        }
        .data(),
    }
}

/// Build the permissionless `settle_xclaim` instruction — finalizes the claim
/// after its window closes, paying out the `best_outcome_kind` recorded on the
/// match. No instruction args (everything is on-chain state), so `match_id`,
/// `tournament_id`, and `player` are passed for PDA derivation. The 9 accounts
/// mirror `SettleXClaim<'info>` exactly (identical layout to `settle_xmatch`).
pub fn build_settle_xclaim(
    match_id: [u8; 32],
    tournament_id: u64,
    player: &Pubkey,
    treasury: &Pubkey,
    cranker: &Pubkey,
) -> Instruction {
    let (xmatch_pda, _) = pda::xmatch_pda(&match_id);
    let (xpool_pda, _) = pda::xpool_pda();
    let (tournament_pda, _) = pda::tournament_pda(tournament_id);
    let (profile_pda, _) = pda::player_profile_pda(tournament_id, player);
    let (global_config_pda, _) = pda::global_config_pda();

    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(xmatch_pda, false),
            AccountMeta::new(xpool_pda, false),
            AccountMeta::new(tournament_pda, false),
            AccountMeta::new(profile_pda, false),
            AccountMeta::new_readonly(global_config_pda, false),
            AccountMeta::new(*treasury, false),
            AccountMeta::new(*player, false),
            AccountMeta::new(*cranker, true), // signer + payer
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ],
        data: SettleXclaim {}.data(),
    }
}

/// Build the permissionless `refund_xmatch_nocert` instruction — refunds the
/// Solana player when a funded cross-chain match never had a certificate
/// signed (the lock/cosign never happened). No signer account: the fee payer
/// (anyone) submits. `player` must equal the recorded `xmatch.player`.
pub fn build_refund_xmatch_nocert(match_id: [u8; 32], player: &Pubkey) -> Instruction {
    let (xmatch_pda, _) = pda::xmatch_pda(&match_id);
    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(xmatch_pda, false),
            AccountMeta::new(*player, false),
        ],
        data: RefundXmatchNocert {}.data(),
    }
}

/// Build the permissionless `refund_xmatch_timeout` instruction — refunds the
/// Solana player after the claim/timeout window closes, releasing any locked
/// tranche back to the float pool. No signer account; `player` must equal
/// `xmatch.player`.
pub fn build_refund_xmatch_timeout(match_id: [u8; 32], player: &Pubkey) -> Instruction {
    let (xmatch_pda, _) = pda::xmatch_pda(&match_id);
    let (xpool_pda, _) = pda::xpool_pda();
    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(xmatch_pda, false),
            AccountMeta::new(xpool_pda, false),
            AccountMeta::new(*player, false),
        ],
        data: RefundXmatchTimeout {}.data(),
    }
}

/// Build the permissionless `close_xmatch` instruction — reclaims the
/// `XChainMatch` account's rent (~0.00236 SOL) once the match is terminal
/// (`Settled`/`ClaimSettled`/`RefundedNoCert`/`RefundedTimeout`). Anchor's
/// `close = player` returns the rent to the recorded player. No signer account:
/// the fee payer (anyone) submits; `player` must equal `xmatch.player`. Cranked
/// after settle/refund so the per-match rent doesn't leak every game.
pub fn build_close_xmatch(match_id: [u8; 32], player: &Pubkey) -> Instruction {
    let (xmatch_pda, _) = pda::xmatch_pda(&match_id);
    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(xmatch_pda, false),
            AccountMeta::new(*player, false),
        ],
        data: CloseXmatch {}.data(),
    }
}

/// Build the permissionless `RefundPending` instruction.
///
/// Refunds P1's stake from an un-joined `Pending` game and closes the account
/// once the join window (`created_at + PENDING_JOIN_WINDOW_SECS`) has elapsed.
/// `player_one` must equal `game.player_one` (receives the refund); `caller`
/// (anyone) pays the fee and receives the reclaimed rent. Mirrors the EVM
/// `cancelPending`. Account order matches `RefundPending<'info>`.
pub fn build_refund_pending(game_id: u64, player_one: &Pubkey, caller: &Pubkey) -> Instruction {
    let (game_pda, _) = pda::game_pda(game_id);
    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(game_pda, false),
            AccountMeta::new(*player_one, false),
            AccountMeta::new(*caller, true),
        ],
        data: RefundPending {}.data(),
    }
}

/// Build the `DepositStake` instruction.
///
/// Deposits the fixed stake into the per-player escrow PDA for the
/// given tournament.
pub fn build_deposit_stake(tournament_id: u64, payer: &Pubkey) -> Instruction {
    assert!(tournament_id > 0, "tournament_id must be non-zero");

    let (tournament_pda, _) = pda::tournament_pda(tournament_id);
    let (escrow_pda, _) = pda::escrow_pda(tournament_id, payer);
    // GlobalConfig now carries the live stake, so deposit_stake reads it and the
    // account is REQUIRED. It is first in the handler's Accounts struct, so it
    // must be first here — Anchor matches account metas positionally, and a
    // wrong order fails with a confusing constraint error rather than
    // "you forgot an account".
    let (global_config_pda, _) = pda::global_config_pda();

    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new_readonly(global_config_pda, false),
            AccountMeta::new(escrow_pda, false),
            AccountMeta::new_readonly(tournament_pda, false),
            AccountMeta::new(*payer, true),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ],
        data: DepositStake {}.data(),
    }
}

/// Build the `JoinGame` instruction.
///
/// Joins an existing game as Player 2. The game must already have been
/// created by the matchmaker.
pub fn build_join_game(game_id: u64, tournament_id: u64, player: &Pubkey) -> Instruction {
    assert!(game_id > 0, "game_id must be non-zero");
    assert!(tournament_id > 0, "tournament_id must be non-zero");

    let (game_pda, _) = pda::game_pda(game_id);
    let (tournament_pda, _) = pda::tournament_pda(tournament_id);
    let (profile_pda, _) = pda::player_profile_pda(tournament_id, player);
    let (escrow_pda, _) = pda::escrow_pda(tournament_id, player);

    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(game_pda, false),
            AccountMeta::new(profile_pda, false),
            AccountMeta::new(escrow_pda, false),
            AccountMeta::new_readonly(tournament_pda, false),
            AccountMeta::new(*player, true),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ],
        data: JoinGame {}.data(),
    }
}

/// Build the `CommitGuess` instruction.
///
/// Submits a SHA-256 commitment of the player's guess. The commitment
/// must later be revealed via `build_reveal_guess`.
pub fn build_commit_guess(game_id: u64, commitment: [u8; 32], player: &Pubkey) -> Instruction {
    assert!(game_id > 0, "game_id must be non-zero");

    let (game_pda, _) = pda::game_pda(game_id);

    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(game_pda, false),
            AccountMeta::new_readonly(*player, true),
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
    player: &Pubkey,
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
            AccountMeta::new_readonly(*player, true),
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
    use solana_sdk::signer::Signer;

    #[test]
    fn build_deposit_stake_has_correct_program_id() {
        let kp = Keypair::new();
        let ix = build_deposit_stake(1, &kp.pubkey());
        assert_eq!(ix.program_id, PROGRAM_ID);
        // Precondition: 5 accounts — global_config FIRST (it carries the live
        // stake), then escrow, tournament, payer, system.
        assert_eq!(ix.accounts.len(), 5, "deposit_stake must have 5 accounts");
        assert_eq!(
            ix.accounts[0].pubkey,
            pda::global_config_pda().0,
            "global_config must be first — Anchor matches metas positionally, so a \
             wrong order surfaces as a confusing constraint error"
        );
        assert!(
            !ix.accounts[0].is_writable,
            "global_config is read-only here"
        );
    }

    #[test]
    fn build_deposit_stake_payer_is_signer() {
        let kp = Keypair::new();
        let ix = build_deposit_stake(1, &kp.pubkey());
        // Index 3 now: global_config was prepended.
        let payer_meta = &ix.accounts[3];
        assert!(payer_meta.is_signer, "payer must be a signer");
        assert!(payer_meta.is_writable, "payer must be writable");
    }

    #[test]
    #[should_panic(expected = "tournament_id must be non-zero")]
    fn build_deposit_stake_rejects_zero_tournament() {
        let kp = Keypair::new();
        let _ = build_deposit_stake(0, &kp.pubkey());
    }

    #[test]
    fn build_join_game_has_correct_accounts() {
        let kp = Keypair::new();
        let ix = build_join_game(1, 1, &kp.pubkey());
        assert_eq!(ix.program_id, PROGRAM_ID);
        // 6 accounts: game, profile, escrow, tournament, player, system.
        assert_eq!(ix.accounts.len(), 6, "join_game must have 6 accounts");
    }

    #[test]
    #[should_panic(expected = "game_id must be non-zero")]
    fn build_join_game_rejects_zero_game_id() {
        let kp = Keypair::new();
        let _ = build_join_game(0, 1, &kp.pubkey());
    }

    #[test]
    fn build_commit_guess_has_correct_accounts() {
        let kp = Keypair::new();
        let ix = build_commit_guess(1, [0u8; 32], &kp.pubkey());
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
        let ix = build_reveal_guess(1, 1, [0u8; 32], None, &kp.pubkey(), p1, p2, gc, treasury);
        assert_eq!(ix.program_id, PROGRAM_ID);
        // 9 accounts: game, player, p1_profile, p2_profile, tournament,
        //             global_config, treasury, player_one, player_two.
        assert_eq!(ix.accounts.len(), 9, "reveal_guess must have 9 accounts");
    }

    #[test]
    #[should_panic(expected = "game_id must be non-zero")]
    fn build_commit_guess_rejects_zero_game_id() {
        let kp = Keypair::new();
        let _ = build_commit_guess(0, [0u8; 32], &kp.pubkey());
    }

    fn xmatch_args() -> CreateXMatchArgs {
        CreateXMatchArgs {
            tournament_id: 1,
            player_is_p1: true,
            session_key: [0x13; 20],
            counter_session_key: [0x23; 20],
            stake_lamports: 50_000_000,
            fund_deadline: 1_765_000_000,
            match_deadline: 1_765_007_200,
        }
    }

    #[test]
    fn build_create_xmatch_has_correct_accounts_and_signers() {
        let player = Keypair::new();
        let matchmaker = Keypair::new();
        let match_id = [0xAB; 32];
        let ix = build_create_xmatch(
            match_id,
            xmatch_args(),
            &player.pubkey(),
            &matchmaker.pubkey(),
        );

        assert_eq!(ix.program_id, PROGRAM_ID);
        // 5 accounts: xmatch, global_config, matchmaker, player, system.
        assert_eq!(ix.accounts.len(), 5, "create_xmatch must have 5 accounts");

        // xmatch account is the PDA seeded by match_id, writable, not a signer.
        let (xmatch_pda, _) = pda::xmatch_pda(&match_id);
        assert_eq!(ix.accounts[0].pubkey, xmatch_pda);
        assert!(ix.accounts[0].is_writable && !ix.accounts[0].is_signer);

        // matchmaker signs (readonly), player signs + pays (writable).
        assert!(ix.accounts[2].is_signer && !ix.accounts[2].is_writable);
        assert_eq!(ix.accounts[2].pubkey, matchmaker.pubkey());
        assert!(ix.accounts[3].is_signer && ix.accounts[3].is_writable);
        assert_eq!(ix.accounts[3].pubkey, player.pubkey());
    }

    #[test]
    #[should_panic(expected = "stake_lamports must be non-zero")]
    fn build_create_xmatch_rejects_zero_stake() {
        let mut args = xmatch_args();
        args.stake_lamports = 0;
        let _ = build_create_xmatch(
            [1u8; 32],
            args,
            &Pubkey::new_unique(),
            &Pubkey::new_unique(),
        );
    }

    fn settle_certs() -> (MatchLiveCertNoA, OutcomeCertArg) {
        let leg_b = coordination::cert::CertLegArg {
            chain_tag: [0; 32],
            contract: [0; 32],
            player: [0; 32],
            session_key: [0; 20],
            stake: 0,
            tranche: 0,
        };
        let cert_no_a = MatchLiveCertNoA {
            match_id: [0xAB; 32],
            tournament_id: 1,
            matchup_commitment: [0; 32],
            leg_b,
            quote_timestamp: 0,
            quote_max_age_secs: 0,
            match_deadline: 0,
            claim_window_secs: 0,
            a_is_p1: 1,
        };
        let outcome = OutcomeCertArg {
            match_id: [0xAB; 32],
            match_live_digest: [0; 32],
            outcome_kind: 0,
            step_count: 4,
            p1_guess: 0,
            p2_guess: 0,
            first_committer: 1,
            matchup_type: 0,
            transcript_hash: [0; 32],
        };
        (cert_no_a, outcome)
    }

    #[test]
    fn build_settle_xmatch_has_correct_accounts_and_mut_flags() {
        let player = Pubkey::new_unique();
        let treasury = Pubkey::new_unique();
        let cranker = Keypair::new();
        let (cert, outcome) = settle_certs();
        let ix = build_settle_xmatch(
            cert,
            outcome,
            [[0u8; 65]; 3],
            [[0u8; 65]; 3],
            &player,
            &treasury,
            &cranker.pubkey(),
        );

        assert_eq!(ix.program_id, PROGRAM_ID);
        assert_eq!(ix.accounts.len(), 9, "settle_xmatch must have 9 accounts");
        // Mut-flags mirror SettleXMatch<'info> exactly.
        assert!(ix.accounts[0].is_writable, "xmatch writable");
        assert!(ix.accounts[1].is_writable, "pool writable");
        assert!(ix.accounts[2].is_writable, "tournament writable");
        assert!(ix.accounts[3].is_writable, "player_profile writable");
        assert!(!ix.accounts[4].is_writable, "global_config readonly");
        assert!(ix.accounts[5].is_writable, "treasury writable");
        assert_eq!(ix.accounts[5].pubkey, treasury);
        assert_eq!(ix.accounts[6].pubkey, player);
        assert!(ix.accounts[6].is_writable, "player writable");
        assert!(
            ix.accounts[7].is_signer && ix.accounts[7].is_writable,
            "cranker is signer + writable"
        );
        assert_eq!(ix.accounts[7].pubkey, cranker.pubkey());
        assert!(!ix.accounts[8].is_writable, "system_program readonly");
    }

    #[test]
    fn build_refund_xmatch_nocert_has_correct_accounts() {
        let player = Pubkey::new_unique();
        let ix = build_refund_xmatch_nocert([0x07; 32], &player);
        assert_eq!(ix.program_id, PROGRAM_ID);
        // 2 accounts: xmatch (PDA), player. Both writable, neither a signer
        // (refund is permissionless — the fee payer submits).
        assert_eq!(ix.accounts.len(), 2);
        let (xmatch_pda, _) = pda::xmatch_pda(&[0x07; 32]);
        assert_eq!(ix.accounts[0].pubkey, xmatch_pda);
        assert!(ix.accounts[0].is_writable && !ix.accounts[0].is_signer);
        assert_eq!(ix.accounts[1].pubkey, player);
        assert!(ix.accounts[1].is_writable && !ix.accounts[1].is_signer);
    }

    #[test]
    fn build_close_xmatch_has_correct_accounts_and_discriminator() {
        let player = Pubkey::new_unique();
        let ix = build_close_xmatch([0x0c; 32], &player);
        assert_eq!(ix.program_id, PROGRAM_ID);
        // 2 accounts: xmatch (PDA, mut — Anchor closes it), player (mut, rent
        // recipient). Permissionless: neither is a signer.
        assert_eq!(ix.accounts.len(), 2, "close_xmatch must have 2 accounts");
        let (xmatch_pda, _) = pda::xmatch_pda(&[0x0c; 32]);
        assert_eq!(ix.accounts[0].pubkey, xmatch_pda);
        assert!(ix.accounts[0].is_writable && !ix.accounts[0].is_signer);
        assert_eq!(ix.accounts[1].pubkey, player);
        assert!(ix.accounts[1].is_writable && !ix.accounts[1].is_signer);
        // The 8-byte Anchor discriminator must be close_xmatch's own, not a
        // refund's — same account layout, different instruction.
        assert_eq!(ix.data.len(), 8, "close_xmatch takes no args");
        assert_ne!(
            ix.data[..8],
            build_refund_xmatch_nocert([0x0c; 32], &player).data[..8],
            "close must not alias refund_nocert's discriminator"
        );
    }

    #[test]
    fn build_refund_xmatch_timeout_has_correct_accounts() {
        let player = Pubkey::new_unique();
        let ix = build_refund_xmatch_timeout([0x09; 32], &player);
        assert_eq!(ix.program_id, PROGRAM_ID);
        // 3 accounts: xmatch, pool, player — all writable, none signers.
        assert_eq!(ix.accounts.len(), 3);
        let (xpool_pda, _) = pda::xpool_pda();
        assert_eq!(ix.accounts[1].pubkey, xpool_pda);
        assert!(ix.accounts.iter().all(|a| a.is_writable && !a.is_signer));
        assert_eq!(ix.accounts[2].pubkey, player);
    }

    #[test]
    fn build_initialize_xpool_has_correct_accounts_and_mut_flags() {
        let operator = Pubkey::new_unique();
        let authority = Keypair::new();
        let ix = build_initialize_xpool(
            &operator,
            [0x42; 20],
            1_000_000_000,
            86_400,
            30,
            &authority.pubkey(),
        );

        assert_eq!(ix.program_id, PROGRAM_ID);
        // 4 accounts: pool (init), global_config, authority (signer+payer), system.
        assert_eq!(
            ix.accounts.len(),
            4,
            "initialize_xpool must have 4 accounts"
        );
        let (xpool_pda, _) = pda::xpool_pda();
        assert_eq!(ix.accounts[0].pubkey, xpool_pda);
        assert!(ix.accounts[0].is_writable && !ix.accounts[0].is_signer);
        assert!(!ix.accounts[1].is_writable, "global_config readonly");
        assert!(
            ix.accounts[2].is_signer && ix.accounts[2].is_writable,
            "authority is signer + payer"
        );
        assert_eq!(ix.accounts[2].pubkey, authority.pubkey());
        assert!(!ix.accounts[3].is_writable, "system_program readonly");
    }

    #[test]
    #[should_panic(expected = "operator_signer must not be all zeros")]
    fn build_initialize_xpool_rejects_zero_signer() {
        let _ = build_initialize_xpool(
            &Pubkey::new_unique(),
            [0u8; 20],
            1,
            1,
            30,
            &Pubkey::new_unique(),
        );
    }

    #[test]
    #[should_panic(expected = "skew_margin_secs must be non-zero")]
    fn build_initialize_xpool_rejects_zero_skew() {
        let _ = build_initialize_xpool(
            &Pubkey::new_unique(),
            [0x42; 20],
            1,
            1,
            0,
            &Pubkey::new_unique(),
        );
    }

    #[test]
    fn build_xpool_deposit_has_correct_accounts_and_mut_flags() {
        let funder = Keypair::new();
        let ix = build_xpool_deposit(500_000_000, &funder.pubkey());

        assert_eq!(ix.program_id, PROGRAM_ID);
        // 3 accounts: pool, funder (signer), system.
        assert_eq!(ix.accounts.len(), 3, "xpool_deposit must have 3 accounts");
        let (xpool_pda, _) = pda::xpool_pda();
        assert_eq!(ix.accounts[0].pubkey, xpool_pda);
        assert!(ix.accounts[0].is_writable && !ix.accounts[0].is_signer);
        assert!(
            ix.accounts[1].is_signer && ix.accounts[1].is_writable,
            "funder is signer + writable"
        );
        assert_eq!(ix.accounts[1].pubkey, funder.pubkey());
        assert!(!ix.accounts[2].is_writable, "system_program readonly");
    }

    #[test]
    #[should_panic(expected = "amount must be non-zero")]
    fn build_xpool_deposit_rejects_zero_amount() {
        let _ = build_xpool_deposit(0, &Pubkey::new_unique());
    }

    #[test]
    fn build_lock_xtranche_has_correct_accounts_and_mut_flags() {
        let cranker = Keypair::new();
        let cert = full_cert();
        let match_id = cert.match_id;
        let ix = build_lock_xtranche(cert, [0u8; 65], &cranker.pubkey());

        assert_eq!(ix.program_id, PROGRAM_ID);
        // 3 accounts: xmatch, pool, cranker (permissionless fee payer).
        assert_eq!(ix.accounts.len(), 3, "lock_xtranche must have 3 accounts");
        let (xmatch_pda, _) = pda::xmatch_pda(&match_id);
        let (xpool_pda, _) = pda::xpool_pda();
        assert_eq!(ix.accounts[0].pubkey, xmatch_pda);
        assert!(ix.accounts[0].is_writable && !ix.accounts[0].is_signer);
        assert_eq!(ix.accounts[1].pubkey, xpool_pda);
        assert!(ix.accounts[1].is_writable && !ix.accounts[1].is_signer);
        // Cranker is a permissionless writable signer (fee payer), NOT the operator.
        assert!(
            ix.accounts[2].is_signer && ix.accounts[2].is_writable,
            "cranker is a writable signer (fee payer)"
        );
        assert_eq!(ix.accounts[2].pubkey, cranker.pubkey());
    }

    fn full_cert() -> MatchLiveCertArg {
        let leg = |s: u8| coordination::cert::CertLegArg {
            chain_tag: [s; 32],
            contract: [s; 32],
            player: [s; 32],
            session_key: [s; 20],
            stake: 0,
            tranche: 0,
        };
        MatchLiveCertArg {
            match_id: [0xC1; 32],
            tournament_id: 1,
            matchup_commitment: [0; 32],
            leg_a: leg(0x10),
            leg_b: leg(0x20),
            quote_timestamp: 0,
            quote_max_age_secs: 0,
            match_deadline: 0,
            claim_window_secs: 0,
            a_is_p1: 1,
        }
    }

    fn checkpoint_arg(step: u8) -> CheckpointArg {
        CheckpointArg {
            match_live_digest: [0; 32],
            step_count: step,
            p1_commit: [0; 32],
            p2_commit: [0; 32],
            p1_guess: 1,
            p2_guess: 0,
            first_committer: 1,
            matchup_type: 1,
            transcript_hash: [0; 32],
            r_matchup: [0; 32],
        }
    }

    #[test]
    fn build_open_xclaim_has_correct_accounts() {
        let ix = build_open_xclaim(
            full_cert(),
            checkpoint_arg(4),
            [[0u8; 65]; 3],
            [[0u8; 65]; 2],
        );
        assert_eq!(ix.program_id, PROGRAM_ID);
        // 2 accounts: xmatch (mut, PDA), pool (readonly). Permissionless — no signer.
        assert_eq!(ix.accounts.len(), 2, "open_xclaim must have 2 accounts");
        let (xmatch_pda, _) = pda::xmatch_pda(&[0xC1; 32]);
        assert_eq!(ix.accounts[0].pubkey, xmatch_pda);
        assert!(ix.accounts[0].is_writable && !ix.accounts[0].is_signer);
        assert!(!ix.accounts[1].is_writable && !ix.accounts[1].is_signer);
    }

    #[test]
    fn build_supersede_xclaim_has_single_mut_xmatch() {
        let ix = build_supersede_xclaim(full_cert(), checkpoint_arg(3), [[0u8; 65]; 2]);
        assert_eq!(ix.program_id, PROGRAM_ID);
        assert_eq!(ix.accounts.len(), 1, "supersede_xclaim must have 1 account");
        assert!(ix.accounts[0].is_writable && !ix.accounts[0].is_signer);
    }

    #[test]
    fn build_submit_equivocation_proof_has_correct_accounts() {
        let ix = build_submit_equivocation_proof(
            full_cert(),
            checkpoint_arg(2),
            checkpoint_arg(2),
            [0u8; 65],
            [1u8; 65],
        );
        assert_eq!(ix.program_id, PROGRAM_ID);
        // 2 accounts: xmatch (mut), pool (readonly).
        assert_eq!(ix.accounts.len(), 2);
        assert!(ix.accounts[0].is_writable && !ix.accounts[0].is_signer);
        assert!(!ix.accounts[1].is_writable && !ix.accounts[1].is_signer);
    }

    #[test]
    fn build_settle_xclaim_mirrors_settle_xmatch_layout() {
        let player = Pubkey::new_unique();
        let treasury = Pubkey::new_unique();
        let cranker = Keypair::new();
        let ix = build_settle_xclaim([0xC1; 32], 1, &player, &treasury, &cranker.pubkey());
        assert_eq!(ix.program_id, PROGRAM_ID);
        // 9 accounts, identical layout to settle_xmatch.
        assert_eq!(ix.accounts.len(), 9, "settle_xclaim must have 9 accounts");
        assert!(ix.accounts[0].is_writable, "xmatch writable");
        assert!(ix.accounts[1].is_writable, "pool writable");
        assert!(ix.accounts[2].is_writable, "tournament writable");
        assert!(ix.accounts[3].is_writable, "player_profile writable");
        assert!(!ix.accounts[4].is_writable, "global_config readonly");
        assert!(ix.accounts[5].is_writable, "treasury writable");
        assert_eq!(ix.accounts[5].pubkey, treasury);
        assert_eq!(ix.accounts[6].pubkey, player);
        assert!(ix.accounts[6].is_writable, "player writable");
        assert!(
            ix.accounts[7].is_signer && ix.accounts[7].is_writable,
            "cranker is signer + payer"
        );
        assert_eq!(ix.accounts[7].pubkey, cranker.pubkey());
        assert!(!ix.accounts[8].is_writable, "system_program readonly");
    }
}
