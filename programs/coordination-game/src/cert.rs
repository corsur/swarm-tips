//! On-chain cross-chain certificate verification (the Solana mirror of
//! evm/src/CertLib.sol). The canonical payload bytes come from the SHARED
//! `chain_core::cert_schema` encoder — the exact same code the backend and
//! golden vectors use — so the digest hashed here is byte-identical to the
//! EVM `keccak256(abi.encode(...))`. Signatures are secp256k1 on both legs
//! (`ecrecover` there, the `secp256k1_recover` syscall here), so there is
//! one uniform signature scheme across chains.

use crate::errors::CoordinationError;
use anchor_lang::prelude::*;
use chain_core::cert_schema as cs;
use solana_secp256k1_recover::secp256k1_recover;

/// One settlement leg, as an instruction argument. Mirrors
/// `cs::CertLeg`; converted to it purely for canonical encoding.
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct CertLegArg {
    pub chain_tag: [u8; 32],
    pub contract: [u8; 32],
    pub player: [u8; 32],
    pub session_key: [u8; 20],
    pub stake: u128,
    pub tranche: u128,
}

impl CertLegArg {
    fn to_schema(&self) -> cs::CertLeg {
        cs::CertLeg {
            chain_tag: self.chain_tag,
            contract: self.contract,
            player: self.player,
            session_key: self.session_key,
            stake: self.stake,
            tranche: self.tranche,
        }
    }
}

/// Match-live certificate, as an instruction argument.
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct MatchLiveCertArg {
    pub match_id: [u8; 32],
    pub tournament_id: u64,
    pub matchup_commitment: [u8; 32],
    pub leg_a: CertLegArg,
    pub leg_b: CertLegArg,
    pub quote_timestamp: u64,
    pub quote_max_age_secs: u32,
    pub match_deadline: u64,
    pub claim_window_secs: u32,
    pub a_is_p1: u8,
}

impl MatchLiveCertArg {
    fn to_schema(&self) -> cs::MatchLiveCert {
        cs::MatchLiveCert {
            match_id: self.match_id,
            tournament_id: self.tournament_id,
            matchup_commitment: self.matchup_commitment,
            leg_a: self.leg_a.to_schema(),
            leg_b: self.leg_b.to_schema(),
            quote_timestamp: self.quote_timestamp,
            quote_max_age_secs: self.quote_max_age_secs,
            match_deadline: self.match_deadline,
            claim_window_secs: self.claim_window_secs,
            a_is_p1: self.a_is_p1,
        }
    }

    pub fn digest(&self) -> [u8; 32] {
        keccak256(&self.to_schema().encode())
    }
}

/// Co-signed transcript checkpoint, as an instruction argument.
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct CheckpointArg {
    pub match_live_digest: [u8; 32],
    pub step_count: u8,
    pub p1_commit: [u8; 32],
    pub p2_commit: [u8; 32],
    pub p1_guess: u8,
    pub p2_guess: u8,
    pub first_committer: u8,
    pub matchup_type: u8,
    pub transcript_hash: [u8; 32],
}

impl CheckpointArg {
    fn to_schema(&self) -> cs::Checkpoint {
        cs::Checkpoint {
            match_live_digest: self.match_live_digest,
            step_count: self.step_count,
            p1_commit: self.p1_commit,
            p2_commit: self.p2_commit,
            p1_guess: self.p1_guess,
            p2_guess: self.p2_guess,
            first_committer: self.first_committer,
            matchup_type: self.matchup_type,
            transcript_hash: self.transcript_hash,
        }
    }

    pub fn digest(&self) -> [u8; 32] {
        keccak256(&self.to_schema().encode())
    }

    /// The outcome a checkpoint entitles a claimant to under the timeout
    /// semantics (committer/revealer wins; neither → both forfeit). Mirrors
    /// CertLib.deriveClaimOutcome, including the inconsistent-transcript
    /// defensive guards.
    pub fn derive_claim_outcome(&self) -> u8 {
        use crate::payoff::*;
        if self.step_count == TERMINAL_STEP_COUNT {
            return self.derive_terminal_outcome();
        }
        if self.step_count == 1 {
            // One commit landed; the committer wins. Inconsistent committer
            // field → both forfeit.
            return match self.first_committer {
                1 => XKIND_TIMEOUT_P1_WINS,
                2 => XKIND_TIMEOUT_P2_WINS,
                _ => XKIND_TIMEOUT_BOTH_FORFEIT,
            };
        }
        if self.step_count == 3 {
            // Both committed, exactly one revealed: the revealer wins.
            // Both-set or both-unset is inconsistent → both forfeit.
            let p1_revealed = self.p1_guess != UNREVEALED;
            let p2_revealed = self.p2_guess != UNREVEALED;
            if p1_revealed == p2_revealed {
                return XKIND_TIMEOUT_BOTH_FORFEIT;
            }
            return if p1_revealed {
                XKIND_TIMEOUT_P1_WINS
            } else {
                XKIND_TIMEOUT_P2_WINS
            };
        }
        // step 0 (nobody committed) / step 2 (both committed, none revealed).
        XKIND_TIMEOUT_BOTH_FORFEIT
    }

    /// Recompute the payoff-matrix outcome from a terminal transcript.
    /// Mirrors CertLib.deriveTerminalOutcome / payoff.rs same-chain rules.
    fn derive_terminal_outcome(&self) -> u8 {
        use crate::payoff::*;
        let p1_correct = self.p1_guess == self.matchup_type;
        let p2_correct = self.p2_guess == self.matchup_type;
        if self.matchup_type == 0 {
            if p1_correct && p2_correct {
                return XKIND_HOMOG_BOTH_CORRECT;
            }
            if p1_correct {
                return XKIND_HOMOG_P1_CORRECT;
            }
            if p2_correct {
                return XKIND_HOMOG_P2_CORRECT;
            }
            return XKIND_BOTH_WRONG;
        }
        if !p1_correct && !p2_correct {
            return XKIND_BOTH_WRONG;
        }
        if p1_correct == p2_correct {
            return if self.first_committer == 1 {
                XKIND_HETERO_P1_WINS
            } else {
                XKIND_HETERO_P2_WINS
            };
        }
        if p1_correct {
            XKIND_HETERO_P1_WINS
        } else {
            XKIND_HETERO_P2_WINS
        }
    }
}

const TERMINAL_STEP_COUNT: u8 = 4;
const UNREVEALED: u8 = 255;

/// Outcome certificate, as an instruction argument.
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct OutcomeCertArg {
    pub match_id: [u8; 32],
    pub match_live_digest: [u8; 32],
    pub outcome_kind: u8,
    pub step_count: u8,
    pub p1_guess: u8,
    pub p2_guess: u8,
    pub first_committer: u8,
    pub matchup_type: u8,
    pub transcript_hash: [u8; 32],
}

impl OutcomeCertArg {
    fn to_schema(&self) -> cs::OutcomeCert {
        cs::OutcomeCert {
            match_id: self.match_id,
            match_live_digest: self.match_live_digest,
            // Validated before use in the handler; from_u8 keeps the encoder
            // total. An unknown kind hashes to a digest no signer co-signed.
            outcome_kind: cs::OutcomeKind::from_u8(self.outcome_kind)
                .unwrap_or(cs::OutcomeKind::TimeoutBothForfeit),
            step_count: self.step_count,
            p1_guess: self.p1_guess,
            p2_guess: self.p2_guess,
            first_committer: self.first_committer,
            matchup_type: self.matchup_type,
            transcript_hash: self.transcript_hash,
        }
    }

    pub fn digest(&self) -> [u8; 32] {
        keccak256(&self.to_schema().encode())
    }
}

pub fn keccak256(bytes: &[u8]) -> [u8; 32] {
    solana_keccak_hasher::hashv(&[bytes]).to_bytes()
}

/// Recover the 20-byte Ethereum address that produced a 65-byte
/// `[r || s || v]` secp256k1 signature over `digest`.
pub fn recover_eth_address(digest: &[u8; 32], sig65: &[u8; 65]) -> Result<[u8; 20]> {
    let recovery_id = match sig65[64] {
        27 => 0u8,
        28 => 1u8,
        v @ (0 | 1) => v,
        _ => return Err(error!(CoordinationError::InvalidGameState)),
    };
    let pubkey = secp256k1_recover(digest, recovery_id, &sig65[..64])
        .map_err(|_| error!(CoordinationError::InvalidGameState))?;
    // eth address = keccak256(uncompressed pubkey, no 0x04 prefix)[12..].
    let hash = keccak256(&pubkey.to_bytes());
    let mut addr = [0u8; 20];
    addr.copy_from_slice(&hash[12..]);
    Ok(addr)
}

/// Verify a signature over `digest` recovers to `expected`.
pub fn require_signer(digest: &[u8; 32], sig65: &[u8; 65], expected: &[u8; 20]) -> Result<()> {
    let recovered = recover_eth_address(digest, sig65)?;
    require!(recovered == *expected, CoordinationError::InvalidGameState);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The on-chain digest helpers must match the golden vectors authored by
    /// chain-core (and verified by the EVM CertLib). This proves the Solana
    /// leg, the EVM leg, and the backend all hash the identical bytes.
    #[test]
    fn match_live_digest_matches_golden_vector() {
        // Sample mirrors crates/chain-core/tests/golden_vectors.rs.
        let leg = |seed: u8| CertLegArg {
            chain_tag: [seed; 32],
            contract: [seed.wrapping_add(1); 32],
            player: [seed.wrapping_add(2); 32],
            session_key: [seed.wrapping_add(3); 20],
            stake: (seed as u128).wrapping_mul(1_000_000),
            tranche: (seed as u128).wrapping_mul(2_000_000),
        };
        let cert = MatchLiveCertArg {
            match_id: [0xAA; 32],
            tournament_id: 7,
            matchup_commitment: [0xBB; 32],
            leg_a: leg(0x10),
            leg_b: leg(0x20),
            quote_timestamp: 1_765_000_000,
            quote_max_age_secs: 300,
            match_deadline: 1_765_000_900,
            claim_window_secs: 3600,
            a_is_p1: 1,
        };
        // From cert-vectors.json match_live.digest.
        let expected =
            hex_literal("29fad2095388ceace2cabf6a25f558be830e9ac5d7f5bac82d91a653693208df");
        assert_eq!(cert.digest(), expected);
    }

    fn cp(step: u8, p1: u8, p2: u8, first: u8, matchup: u8) -> CheckpointArg {
        CheckpointArg {
            match_live_digest: [0; 32],
            step_count: step,
            p1_commit: [0; 32],
            p2_commit: [0; 32],
            p1_guess: p1,
            p2_guess: p2,
            first_committer: first,
            matchup_type: matchup,
            transcript_hash: [0; 32],
        }
    }

    #[test]
    fn derive_claim_outcome_matches_evm_certlib() {
        use crate::payoff::*;
        // Terminal heterogeneous: correct guess wins.
        assert_eq!(
            cp(4, 1, 0, 1, 1).derive_claim_outcome(),
            XKIND_HETERO_P1_WINS
        );
        assert_eq!(
            cp(4, 0, 1, 1, 1).derive_claim_outcome(),
            XKIND_HETERO_P2_WINS
        );
        // Both correct → first committer.
        assert_eq!(
            cp(4, 1, 1, 2, 1).derive_claim_outcome(),
            XKIND_HETERO_P2_WINS
        );
        assert_eq!(cp(4, 0, 0, 1, 1).derive_claim_outcome(), XKIND_BOTH_WRONG);
        // Terminal homogeneous.
        assert_eq!(
            cp(4, 0, 0, 1, 0).derive_claim_outcome(),
            XKIND_HOMOG_BOTH_CORRECT
        );
        assert_eq!(
            cp(4, 0, 1, 1, 0).derive_claim_outcome(),
            XKIND_HOMOG_P1_CORRECT
        );
        // Timeout step 1: committer wins; bad committer → both forfeit.
        assert_eq!(
            cp(1, 255, 255, 1, 1).derive_claim_outcome(),
            XKIND_TIMEOUT_P1_WINS
        );
        assert_eq!(
            cp(1, 255, 255, 0, 1).derive_claim_outcome(),
            XKIND_TIMEOUT_BOTH_FORFEIT
        );
        // Timeout step 3: sole revealer wins; both-set → both forfeit (guard).
        assert_eq!(
            cp(3, 1, 255, 1, 1).derive_claim_outcome(),
            XKIND_TIMEOUT_P1_WINS
        );
        assert_eq!(
            cp(3, 255, 1, 1, 1).derive_claim_outcome(),
            XKIND_TIMEOUT_P2_WINS
        );
        assert_eq!(
            cp(3, 1, 1, 1, 1).derive_claim_outcome(),
            XKIND_TIMEOUT_BOTH_FORFEIT
        );
        // Step 0 / 2: both forfeit.
        assert_eq!(
            cp(0, 255, 255, 255, 1).derive_claim_outcome(),
            XKIND_TIMEOUT_BOTH_FORFEIT
        );
        assert_eq!(
            cp(2, 255, 255, 1, 1).derive_claim_outcome(),
            XKIND_TIMEOUT_BOTH_FORFEIT
        );
    }

    fn hex_literal(s: &str) -> [u8; 32] {
        let mut out = [0u8; 32];
        for (slot, pair) in out.iter_mut().zip(s.as_bytes().chunks_exact(2)) {
            *slot = u8::from_str_radix(core::str::from_utf8(pair).unwrap(), 16).unwrap();
        }
        out
    }
}
