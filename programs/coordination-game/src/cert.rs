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

/// Match-live certificate WITHOUT leg A — used by `settle_xmatch` to stay
/// under Solana's 1232-byte transaction limit. Leg A is the Solana leg and
/// is fully determined by on-chain match state, so the program reconstructs
/// it rather than carrying its 148 bytes over the wire. This also removes
/// the tamper surface: leg A is authoritative from state, never the caller.
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct MatchLiveCertNoA {
    pub match_id: [u8; 32],
    pub tournament_id: u64,
    pub matchup_commitment: [u8; 32],
    pub leg_b: CertLegArg,
    pub quote_timestamp: u64,
    pub quote_max_age_secs: u32,
    pub match_deadline: u64,
    pub claim_window_secs: u32,
    pub a_is_p1: u8,
}

impl MatchLiveCertNoA {
    /// Reattach the reconstructed leg A to form the full certificate.
    pub fn with_leg_a(self, leg_a: CertLegArg) -> MatchLiveCertArg {
        MatchLiveCertArg {
            match_id: self.match_id,
            tournament_id: self.tournament_id,
            matchup_commitment: self.matchup_commitment,
            leg_a,
            leg_b: self.leg_b,
            quote_timestamp: self.quote_timestamp,
            quote_max_age_secs: self.quote_max_age_secs,
            match_deadline: self.match_deadline,
            claim_window_secs: self.claim_window_secs,
            a_is_p1: self.a_is_p1,
        }
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
    /// Matchup-type reveal preimage; bound to the cert's commitment on
    /// terminal checkpoints (see `verify_matchup_binding`). 0 when unused.
    pub r_matchup: [u8; 32],
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
            r_matchup: self.r_matchup,
        }
    }

    pub fn digest(&self) -> [u8; 32] {
        keccak256(&self.to_schema().encode())
    }

    /// On a TERMINAL checkpoint the payoff depends on `matchup_type`, which a
    /// colluding pair could otherwise fabricate. Bind it to the matchmaker's
    /// commitment exactly as same-chain `reveal_guess` does:
    /// `sha256(r_matchup) == matchup_commitment` and
    /// `matchup_type == r_matchup[31] & 1`. Non-terminal checkpoints derive
    /// timeout outcomes that don't read `matchup_type`, so no binding is needed.
    pub fn verify_matchup_binding(&self, matchup_commitment: &[u8; 32]) -> Result<()> {
        if self.step_count != TERMINAL_STEP_COUNT {
            return Ok(());
        }
        let computed: [u8; 32] = solana_sha256_hasher::hashv(&[self.r_matchup.as_ref()]).to_bytes();
        require!(
            computed == *matchup_commitment,
            CoordinationError::InvalidGameState
        );
        require!(
            self.matchup_type == (self.r_matchup[31] & 1),
            CoordinationError::InvalidGameState
        );
        Ok(())
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
            let p1_revealed = self.p1_guess != crate::state::GUESS_UNREVEALED;
            let p2_revealed = self.p2_guess != crate::state::GUESS_UNREVEALED;
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

pub const TERMINAL_STEP_COUNT: u8 = 4;

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

/// secp256k1 group order ÷ 2, big-endian. A signature whose s exceeds this is
/// malleable (its twin s' = n - s is equally valid). The EVM leg's OpenZeppelin
/// `ECDSA.recover` rejects high-s, so the Solana leg must too — otherwise a
/// malleated twin would verify on one leg but not the other.
const SECP256K1_HALF_ORDER: [u8; 32] = [
    0x7f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0x5d, 0x57, 0x6e, 0x73, 0x57, 0xa4, 0x50, 0x1d, 0xdf, 0xe9, 0x2f, 0x46, 0x68, 0x1b, 0x20, 0xa0,
];

/// Recover the 20-byte Ethereum address that produced a 65-byte
/// `[r || s || v]` secp256k1 signature over `digest`.
pub fn recover_eth_address(digest: &[u8; 32], sig65: &[u8; 65]) -> Result<[u8; 20]> {
    let recovery_id = match sig65[64] {
        27 => 0u8,
        28 => 1u8,
        v @ (0 | 1) => v,
        _ => return Err(error!(CoordinationError::InvalidGameState)),
    };
    // Reject high-s (malleable) signatures so this leg's acceptance set matches
    // the EVM leg's. Big-endian byte comparison == numeric comparison here.
    if sig65[32..64] > SECP256K1_HALF_ORDER[..] {
        return Err(error!(CoordinationError::InvalidGameState));
    }
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
            r_matchup: [0; 32],
        }
    }

    #[test]
    fn verify_matchup_binding_enforced_on_terminal_checkpoint() {
        let r = [0xE1u8; 32]; // LSB == 1
        let commitment: [u8; 32] = solana_sha256_hasher::hashv(&[r.as_ref()]).to_bytes();

        let mut good = cp(4, 1, 0, 1, 1); // terminal, matchup_type == 1
        good.r_matchup = r;
        assert!(good.verify_matchup_binding(&commitment).is_ok());

        // Forged preimage that doesn't open the commitment → reject.
        let mut bad_preimage = good.clone();
        bad_preimage.r_matchup = [0x07; 32];
        assert!(bad_preimage.verify_matchup_binding(&commitment).is_err());

        // matchup_type inconsistent with r_matchup LSB → reject.
        let mut bad_type = good.clone();
        bad_type.matchup_type = 0;
        assert!(bad_type.verify_matchup_binding(&commitment).is_err());

        // Non-terminal checkpoint: binding is not enforced (timeout payoff
        // doesn't read matchup_type), so any r_matchup passes.
        let mut nonterminal = cp(1, 255, 255, 1, 1);
        nonterminal.r_matchup = [0x07; 32];
        assert!(nonterminal.verify_matchup_binding(&commitment).is_ok());
    }

    /// The BPF derivation is pinned to the CANONICAL truth table exported by
    /// chain-core (`cert_schema::DERIVATION_TRUTH_TABLE`) — the SAME const the
    /// golden fixture (`outcome-derivation.json`, read by `CertLib.deriveClaimOutcome`)
    /// is generated from. Iterating it here (rather than a hand-copied duplicate)
    /// means any edit to the canonical table is enforced on all three VMs at once;
    /// there is no on-chain copy left to silently drift. Expected is the const's
    /// hand-authored `OutcomeKind as u8` column.
    #[test]
    fn derive_claim_outcome_matches_truth_table() {
        for &(step, p1, p2, fc, m, expected) in cs::DERIVATION_TRUTH_TABLE {
            assert_eq!(
                cp(step, p1, p2, fc, m).derive_claim_outcome(),
                expected,
                "row step={step} p1={p1} p2={p2} first_committer={fc} matchup={m}"
            );
        }
    }

    fn hex_literal(s: &str) -> [u8; 32] {
        let mut out = [0u8; 32];
        for (slot, pair) in out.iter_mut().zip(s.as_bytes().chunks_exact(2)) {
            *slot = u8::from_str_radix(core::str::from_utf8(pair).unwrap(), 16).unwrap();
        }
        out
    }
}
