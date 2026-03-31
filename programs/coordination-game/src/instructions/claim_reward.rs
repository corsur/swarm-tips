use crate::errors::CoordinationError;
use crate::events::RewardClaimed;
use crate::instructions::utils::transfer_lamports;
use crate::state::{PlayerProfile, Tournament, MIN_GAMES_FOR_PAYOUT};
use anchor_lang::prelude::*;
use solana_keccak_hasher as keccak;

/// Maximum merkle proof depth. Supports up to ~1M leaves (2^20).
const MAX_PROOF_LEN: usize = 20;

/// Claims a player's tournament reward using a merkle proof.
///
/// The player submits their entitlement amount and a proof (sibling hashes)
/// that their `(wallet, amount)` pair is included in the finalized merkle tree.
///
/// Leaf: `keccak256(0x00 || player_wallet || amount_le_bytes)`
/// Internal: `keccak256(0x01 || min(left, right) || max(left, right))`
///
/// Domain separation (0x00 for leaves, 0x01 for internal nodes) prevents
/// second-preimage attacks. Sorted children make proofs order-independent.
pub fn claim_reward(ctx: Context<ClaimReward>, amount: u64, proof: Vec<[u8; 32]>) -> Result<()> {
    // Checks
    require!(
        proof.len() <= MAX_PROOF_LEN,
        CoordinationError::MerkleProofTooLong,
    );

    let tournament = &ctx.accounts.tournament;
    require!(
        tournament.finalized,
        CoordinationError::TournamentNotFinalized,
    );

    let profile = &ctx.accounts.player_profile;
    require!(!profile.claimed, CoordinationError::AlreadyClaimed);
    require!(
        profile.total_games >= MIN_GAMES_FOR_PAYOUT,
        CoordinationError::BelowMinimumGames,
    );

    // Verify merkle proof
    let player_wallet = ctx.accounts.player.key();
    let leaf = compute_leaf(&player_wallet, amount)?;
    require!(
        verify_proof(&leaf, &proof, &tournament.merkle_root)?,
        CoordinationError::InvalidMerkleProof,
    );
    require!(amount > 0, CoordinationError::EmptyPrizePool);

    // Effects: mark claimed before transfer (CEI ordering)
    ctx.accounts.player_profile.claimed = true;

    // Postcondition: claimed flag must be set to prevent double-claim
    require!(
        ctx.accounts.player_profile.claimed,
        CoordinationError::InvalidGameState,
    );

    // Interactions: transfer entitlement from tournament PDA to player wallet
    transfer_lamports(
        &ctx.accounts.tournament.to_account_info(),
        &ctx.accounts.player.to_account_info(),
        amount,
    )?;

    emit!(RewardClaimed {
        tournament_id: tournament.tournament_id,
        player: player_wallet,
        amount,
    });
    Ok(())
}

/// Compute a merkle leaf: `keccak256(0x00 || player_wallet || amount_le_bytes)`
fn compute_leaf(player_wallet: &Pubkey, amount: u64) -> Result<[u8; 32]> {
    let wallet_bytes = player_wallet.to_bytes();
    let amount_bytes = amount.to_le_bytes();
    let result = keccak::hashv(&[&[0x00], wallet_bytes.as_ref(), amount_bytes.as_ref()]).0;
    // Postcondition: a valid keccak256 hash should never be all zeros
    require!(result != [0u8; 32], CoordinationError::InvalidMerkleProof);
    Ok(result)
}

/// Walk the merkle proof from leaf to root, using sorted children and
/// domain-separated hashing for internal nodes.
///
/// Returns true if the computed root matches the expected root.
fn verify_proof(leaf: &[u8; 32], proof: &[[u8; 32]], root: &[u8; 32]) -> Result<bool> {
    // Precondition: proof depth must not exceed maximum
    require!(
        proof.len() <= MAX_PROOF_LEN,
        CoordinationError::MerkleProofTooLong
    );
    let mut current = *leaf;
    for sibling in proof.iter() {
        current = hash_internal_node(&current, sibling);
    }
    Ok(current == *root)
}

/// Hash an internal node: `keccak256(0x01 || min(left, right) || max(left, right))`
///
/// Children are sorted lexicographically so the proof is order-independent.
fn hash_internal_node(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    let (left, right) = if a <= b { (a, b) } else { (b, a) };
    keccak::hashv(&[&[0x01], left.as_ref(), right.as_ref()]).0
}

#[derive(Accounts)]
pub struct ClaimReward<'info> {
    #[account(
        mut,
        seeds = [b"tournament", tournament.tournament_id.to_le_bytes().as_ref()],
        bump = tournament.bump,
    )]
    pub tournament: Account<'info, Tournament>,
    #[account(
        mut,
        seeds = [
            b"player",
            tournament.tournament_id.to_le_bytes().as_ref(),
            player.key().as_ref(),
        ],
        bump = player_profile.bump,
        constraint = player_profile.wallet == player.key(),
    )]
    pub player_profile: Account<'info, PlayerProfile>,
    #[account(mut)]
    pub player: Signer<'info>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_leaf_deterministic() {
        let wallet = Pubkey::new_unique();
        let amount: u64 = 1_000_000;
        let leaf1 = compute_leaf(&wallet, amount).unwrap();
        let leaf2 = compute_leaf(&wallet, amount).unwrap();
        assert_eq!(leaf1, leaf2, "same inputs must produce same leaf");
        // Different amount must produce different leaf
        let leaf3 = compute_leaf(&wallet, 999_999).unwrap();
        assert_ne!(
            leaf1, leaf3,
            "different amounts must produce different leaves"
        );
    }

    #[test]
    fn compute_leaf_different_wallets() {
        let wallet_a = Pubkey::new_unique();
        let wallet_b = Pubkey::new_unique();
        let amount: u64 = 500_000;
        let leaf_a = compute_leaf(&wallet_a, amount).unwrap();
        let leaf_b = compute_leaf(&wallet_b, amount).unwrap();
        assert_ne!(
            leaf_a, leaf_b,
            "different wallets must produce different leaves"
        );
    }

    #[test]
    fn verify_proof_single_leaf_tree() {
        // A tree with one leaf: the root IS the leaf
        let wallet = Pubkey::new_unique();
        let amount: u64 = 1_000_000;
        let leaf = compute_leaf(&wallet, amount).unwrap();
        // Empty proof: leaf should equal root
        assert!(verify_proof(&leaf, &[], &leaf).unwrap());
    }

    #[test]
    fn verify_proof_two_leaf_tree() {
        let wallet_a = Pubkey::new_unique();
        let wallet_b = Pubkey::new_unique();
        let amount_a: u64 = 1_000_000;
        let amount_b: u64 = 2_000_000;

        let leaf_a = compute_leaf(&wallet_a, amount_a).unwrap();
        let leaf_b = compute_leaf(&wallet_b, amount_b).unwrap();

        // Root = hash_internal_node(leaf_a, leaf_b)
        let root = hash_internal_node(&leaf_a, &leaf_b);

        // Player A proves with sibling = leaf_b
        assert!(verify_proof(&leaf_a, &[leaf_b], &root).unwrap());
        // Player B proves with sibling = leaf_a
        assert!(verify_proof(&leaf_b, &[leaf_a], &root).unwrap());
    }

    #[test]
    fn verify_proof_order_independent() {
        // hash_internal_node sorts, so order of children shouldn't matter
        let a = [0u8; 32];
        let mut b = [0u8; 32];
        b[0] = 1;
        assert_eq!(
            hash_internal_node(&a, &b),
            hash_internal_node(&b, &a),
            "internal node hash must be order-independent"
        );
    }

    #[test]
    fn verify_proof_rejects_wrong_root() {
        let wallet = Pubkey::new_unique();
        let amount: u64 = 1_000_000;
        let leaf = compute_leaf(&wallet, amount).unwrap();
        let wrong_root = [0xFFu8; 32];
        assert!(
            !verify_proof(&leaf, &[], &wrong_root).unwrap(),
            "must reject proof against wrong root"
        );
    }

    #[test]
    fn verify_proof_rejects_wrong_amount() {
        let wallet_a = Pubkey::new_unique();
        let wallet_b = Pubkey::new_unique();
        let amount_a: u64 = 1_000_000;
        let amount_b: u64 = 2_000_000;

        let leaf_a = compute_leaf(&wallet_a, amount_a).unwrap();
        let leaf_b = compute_leaf(&wallet_b, amount_b).unwrap();
        let root = hash_internal_node(&leaf_a, &leaf_b);

        // Player A tries to claim with wrong amount
        let fake_leaf = compute_leaf(&wallet_a, 9_999_999).unwrap();
        assert!(
            !verify_proof(&fake_leaf, &[leaf_b], &root).unwrap(),
            "must reject proof with wrong entitlement amount"
        );
    }

    #[test]
    fn verify_proof_four_leaf_tree() {
        // Build a 4-leaf balanced tree and verify each player's proof
        let wallets: Vec<Pubkey> = (0..4).map(|_| Pubkey::new_unique()).collect();
        let amounts: [u64; 4] = [100, 200, 300, 400];

        let leaves: Vec<[u8; 32]> = wallets
            .iter()
            .zip(amounts.iter())
            .map(|(w, a)| compute_leaf(w, *a).unwrap())
            .collect();

        // Level 1: pair leaves
        let node_01 = hash_internal_node(&leaves[0], &leaves[1]);
        let node_23 = hash_internal_node(&leaves[2], &leaves[3]);
        // Root
        let root = hash_internal_node(&node_01, &node_23);

        // Verify leaf 0: proof = [leaf_1, node_23]
        assert!(verify_proof(&leaves[0], &[leaves[1], node_23], &root).unwrap());
        // Verify leaf 1: proof = [leaf_0, node_23]
        assert!(verify_proof(&leaves[1], &[leaves[0], node_23], &root).unwrap());
        // Verify leaf 2: proof = [leaf_3, node_01]
        assert!(verify_proof(&leaves[2], &[leaves[3], node_01], &root).unwrap());
        // Verify leaf 3: proof = [leaf_2, node_01]
        assert!(verify_proof(&leaves[3], &[leaves[2], node_01], &root).unwrap());
    }

    #[test]
    fn domain_separation_prevents_leaf_as_internal() {
        // A leaf hash (0x00 prefix) must not equal an internal node hash (0x01 prefix)
        // even if the payload bytes happen to match
        let payload = [0x42u8; 32];
        let leaf_hash = keccak::hashv(&[&[0x00], payload.as_ref(), 0u64.to_le_bytes().as_ref()]).0;
        let internal_hash =
            keccak::hashv(&[&[0x01], payload.as_ref(), 0u64.to_le_bytes().as_ref()]).0;
        assert_ne!(
            leaf_hash, internal_hash,
            "domain separation must differentiate leaves from internal nodes"
        );
    }
}
