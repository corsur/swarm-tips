//! Canonical certificate byte layout for cross-chain matches.
//!
//! THE cross-VM contract: the Anchor program (via the keccak syscall),
//! the EVM `CertLib.sol` (via `keccak256(abi.encode(...))`), and the
//! backend all hash exactly these payload bytes. Every field is encoded
//! as one 32-byte big-endian word — byte-identical to Solidity's
//! `abi.encode` for value types. The Solidity mirror is held equal by
//! the golden vectors in `tests/fixtures/cert-vectors.json`.
//!
//! Design and panel-mandated requirements: `multichain/decision.md` §4.1.
//! Not EIP-712 — both legs' chain tags + contracts live inside ONE signed
//! payload, and each verifier executes only its own leg's amounts, which
//! is what makes cross-chain replay/double-claim structurally impossible.

/// keccak256("SWARM_XCHAIN_MATCH_LIVE") — asserted in tests.
pub const MATCH_LIVE_MAGIC: [u8; 32] = [
    0x65, 0xcc, 0x8f, 0x7e, 0x0a, 0xbe, 0x2d, 0xde, 0x3a, 0xa2, 0x54, 0x8c, 0xf4, 0xb5, 0xe5, 0xba,
    0x0b, 0x4c, 0x85, 0x69, 0xbf, 0x82, 0x0a, 0x93, 0x42, 0xa4, 0xfa, 0x47, 0x54, 0xad, 0xcd, 0x15,
];

/// keccak256("SWARM_XCHAIN_CHECKPOINT") — asserted in tests.
pub const CHECKPOINT_MAGIC: [u8; 32] = [
    0xc2, 0x63, 0x54, 0x65, 0xd7, 0xfd, 0xde, 0x2b, 0x4f, 0xa6, 0xc4, 0xcd, 0x10, 0x9c, 0xfc, 0x35,
    0x62, 0xf6, 0x88, 0xfa, 0xb9, 0xf7, 0x2a, 0xf8, 0x18, 0xdb, 0x1e, 0xf1, 0xfc, 0xde, 0x34, 0x69,
];

/// keccak256("SWARM_XCHAIN_OUTCOME") — asserted in tests.
pub const OUTCOME_MAGIC: [u8; 32] = [
    0xd8, 0x85, 0xa3, 0x02, 0xd4, 0x46, 0xa4, 0x92, 0x95, 0x2c, 0xd7, 0xb6, 0x70, 0xa9, 0xb6, 0xe6,
    0x8c, 0x96, 0x27, 0xf8, 0x45, 0x78, 0x90, 0x24, 0x57, 0xb8, 0xcf, 0xe2, 0x27, 0x84, 0x3e, 0x3d,
];

pub const SCHEMA_VERSION: u16 = 1;

/// Encoded sizes: every field is one 32-byte word.
pub const MATCH_LIVE_WORDS: usize = 22;
pub const CHECKPOINT_WORDS: usize = 12;
pub const OUTCOME_WORDS: usize = 11;
pub use crate::words::WORD;

/// A terminal transcript has all four steps: both commits + both reveals.
pub const TERMINAL_STEP_COUNT: u8 = 4;

/// Sentinel for a guess that has not been revealed (mirrors the on-chain
/// `UNREVEALED` value and `CertLib.UNREVEALED`).
pub const UNREVEALED: u8 = 255;

/// One settlement leg of a match — the per-chain escrow terms. Each
/// chain's verifier checks ITS leg against locally recorded state and
/// executes only its own leg's amounts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertLeg {
    /// keccak256 of the CAIP-2 chain string (chain registry value).
    pub chain_tag: [u8; 32],
    /// Program ID (Solana, 32 bytes) or contract address (EVM, 20 bytes
    /// left-padded to 32).
    pub contract: [u8; 32],
    /// Player wallet: Pubkey (32) or EVM address left-padded.
    pub player: [u8; 32],
    /// Per-match secp256k1 session key, eth-address form (20 bytes).
    pub session_key: [u8; 20],
    /// Stake in the chain's native base unit (lamports / wei).
    pub stake: u128,
    /// Max cross-chain payout locked from this chain's float pool.
    pub tranche: u128,
}

/// The match-live certificate: both legs' terms + the co-signed rate
/// schedule. Signed by both player session keys AND the operator
/// (third signer with reference-rate bounds — panel requirement).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchLiveCert {
    pub match_id: [u8; 32],
    pub tournament_id: u64,
    /// SHA-256 of the matchup-type preimage (same scheme as same-chain).
    pub matchup_commitment: [u8; 32],
    /// Leg A is ALWAYS the Solana leg (canonical ordering so both legs
    /// hash identical bytes).
    pub leg_a: CertLeg,
    /// Leg B is ALWAYS the EVM leg.
    pub leg_b: CertLeg,
    /// Operator's reference-rate quote time; settle enforces
    /// `quote_timestamp + quote_max_age_secs >= locked_at`.
    pub quote_timestamp: u64,
    pub quote_max_age_secs: u32,
    /// Unix seconds. Cross-chain windows are wall-clock on BOTH legs
    /// (never slots) so the cert means the same thing everywhere.
    pub match_deadline: u64,
    pub claim_window_secs: u32,
    /// 1 when leg A's player is P1 in the payoff matrix, else 0.
    pub a_is_p1: u8,
}

/// A co-signed transcript checkpoint. Monotonic: higher `step_count`
/// supersedes lower (reveals supersede commits) — the basis of the
/// optimistic claim path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkpoint {
    pub match_live_digest: [u8; 32],
    /// 0..=4: p1_commit, p2_commit, first reveal, second reveal.
    pub step_count: u8,
    pub p1_commit: [u8; 32],
    pub p2_commit: [u8; 32],
    /// 255 = unrevealed (mirrors on-chain sentinel).
    pub p1_guess: u8,
    pub p2_guess: u8,
    /// 1 or 2; 255 before any commit (mirrors on-chain value space).
    pub first_committer: u8,
    /// 0 same-team, 1 diff-team, 255 unset.
    pub matchup_type: u8,
    pub transcript_hash: [u8; 32],
    /// The matchup-type reveal preimage. A terminal checkpoint binds it on
    /// every leg: `sha256(r_matchup) == match_live.matchup_commitment` and
    /// `matchup_type == r_matchup[31] & 1`. Without this on-chain check the
    /// contested-claim path (no operator) would let two colluding players
    /// settle a fabricated matchup. 0 on non-terminal checkpoints (unused).
    pub r_matchup: [u8; 32],
}

/// How a match resolved. The numeric values are part of the cross-chain
/// wire format — never reorder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OutcomeKind {
    HomogBothCorrect = 0,
    HomogP1Correct = 1,
    HomogP2Correct = 2,
    BothWrong = 3,
    HeteroP1Wins = 4,
    HeteroP2Wins = 5,
    TimeoutP1Wins = 6,
    TimeoutP2Wins = 7,
    TimeoutBothForfeit = 8,
}

impl OutcomeKind {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::HomogBothCorrect),
            1 => Some(Self::HomogP1Correct),
            2 => Some(Self::HomogP2Correct),
            3 => Some(Self::BothWrong),
            4 => Some(Self::HeteroP1Wins),
            5 => Some(Self::HeteroP2Wins),
            6 => Some(Self::TimeoutP1Wins),
            7 => Some(Self::TimeoutP2Wins),
            8 => Some(Self::TimeoutBothForfeit),
            _ => None,
        }
    }
}

/// The outcome certificate: the terminal checkpoint plus the explicit
/// result, bound to the exact match-live cert by digest (transitively
/// inheriting all domain separation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutcomeCert {
    pub match_id: [u8; 32],
    pub match_live_digest: [u8; 32],
    pub outcome_kind: OutcomeKind,
    pub step_count: u8,
    pub p1_guess: u8,
    pub p2_guess: u8,
    pub first_committer: u8,
    pub matchup_type: u8,
    pub transcript_hash: [u8; 32],
}

use crate::words::{push_addr20, push_u128, push_u16, push_u32, push_u64, push_u8, push_word};

fn push_leg(out: &mut Vec<u8>, leg: &CertLeg) {
    push_word(out, &leg.chain_tag);
    push_word(out, &leg.contract);
    push_word(out, &leg.player);
    push_addr20(out, &leg.session_key);
    push_u128(out, leg.stake);
    push_u128(out, leg.tranche);
}

impl MatchLiveCert {
    /// Canonical payload: 22 words, 704 bytes. Hash with keccak256 to
    /// get the signing digest.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(704);
        push_word(&mut out, &MATCH_LIVE_MAGIC);
        push_u16(&mut out, SCHEMA_VERSION);
        push_word(&mut out, &self.match_id);
        push_u64(&mut out, self.tournament_id);
        push_word(&mut out, &self.matchup_commitment);
        push_leg(&mut out, &self.leg_a);
        push_leg(&mut out, &self.leg_b);
        push_u64(&mut out, self.quote_timestamp);
        push_u32(&mut out, self.quote_max_age_secs);
        push_u64(&mut out, self.match_deadline);
        push_u32(&mut out, self.claim_window_secs);
        push_u8(&mut out, self.a_is_p1);
        debug_assert_eq!(out.len(), MATCH_LIVE_WORDS.saturating_mul(WORD));
        out
    }
}

impl Checkpoint {
    /// Canonical payload: 12 words, 384 bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(384);
        push_word(&mut out, &CHECKPOINT_MAGIC);
        push_u16(&mut out, SCHEMA_VERSION);
        push_word(&mut out, &self.match_live_digest);
        push_u8(&mut out, self.step_count);
        push_word(&mut out, &self.p1_commit);
        push_word(&mut out, &self.p2_commit);
        push_u8(&mut out, self.p1_guess);
        push_u8(&mut out, self.p2_guess);
        push_u8(&mut out, self.first_committer);
        push_u8(&mut out, self.matchup_type);
        push_word(&mut out, &self.transcript_hash);
        push_word(&mut out, &self.r_matchup);
        debug_assert_eq!(out.len(), CHECKPOINT_WORDS.saturating_mul(WORD));
        out
    }

    /// The outcome this checkpoint entitles a claimant to under the timeout
    /// semantics (committer/revealer wins; neither → both forfeit). The exact
    /// canonical mapping shared by `CertLib.deriveClaimOutcome` (Solidity) and
    /// `programs/coordination-game/src/cert.rs::derive_claim_outcome` (BPF) —
    /// this is the backend/operator/AI-player copy used to build the settle
    /// `OutcomeCert`. The 4-way agreement is pinned by the golden vectors in
    /// `tests/fixtures/cert-vectors.json`.
    pub fn derive_outcome_kind(&self) -> OutcomeKind {
        if self.step_count == TERMINAL_STEP_COUNT {
            return self.derive_terminal_outcome();
        }
        if self.step_count == 1 {
            // One commit landed; the committer wins. Inconsistent committer
            // field → both forfeit.
            return match self.first_committer {
                1 => OutcomeKind::TimeoutP1Wins,
                2 => OutcomeKind::TimeoutP2Wins,
                _ => OutcomeKind::TimeoutBothForfeit,
            };
        }
        if self.step_count == 3 {
            // Both committed, exactly one revealed: the revealer wins.
            // Both-set or both-unset is inconsistent → both forfeit.
            let p1_revealed = self.p1_guess != UNREVEALED;
            let p2_revealed = self.p2_guess != UNREVEALED;
            if p1_revealed == p2_revealed {
                return OutcomeKind::TimeoutBothForfeit;
            }
            return if p1_revealed {
                OutcomeKind::TimeoutP1Wins
            } else {
                OutcomeKind::TimeoutP2Wins
            };
        }
        // step 0 (nobody committed) / step 2 (both committed, none revealed).
        OutcomeKind::TimeoutBothForfeit
    }

    /// Recompute the payoff-matrix outcome from a terminal transcript.
    /// Mirrors `CertLib.deriveTerminalOutcome` / `payoff.rs` same-chain rules.
    fn derive_terminal_outcome(&self) -> OutcomeKind {
        let p1_correct = self.p1_guess == self.matchup_type;
        let p2_correct = self.p2_guess == self.matchup_type;
        if self.matchup_type == 0 {
            if p1_correct && p2_correct {
                return OutcomeKind::HomogBothCorrect;
            }
            if p1_correct {
                return OutcomeKind::HomogP1Correct;
            }
            if p2_correct {
                return OutcomeKind::HomogP2Correct;
            }
            return OutcomeKind::BothWrong;
        }
        if !p1_correct && !p2_correct {
            return OutcomeKind::BothWrong;
        }
        if p1_correct == p2_correct {
            return if self.first_committer == 1 {
                OutcomeKind::HeteroP1Wins
            } else {
                OutcomeKind::HeteroP2Wins
            };
        }
        if p1_correct {
            OutcomeKind::HeteroP1Wins
        } else {
            OutcomeKind::HeteroP2Wins
        }
    }

    /// Build the `OutcomeCert` this checkpoint resolves to, binding it to the
    /// given `match_id`. The outcome kind is derived canonically; every other
    /// field is carried through so the resulting cert's digest matches what the
    /// players co-signed in the transcript.
    pub fn to_outcome_cert(&self, match_id: [u8; 32]) -> OutcomeCert {
        OutcomeCert {
            match_id,
            match_live_digest: self.match_live_digest,
            outcome_kind: self.derive_outcome_kind(),
            step_count: self.step_count,
            p1_guess: self.p1_guess,
            p2_guess: self.p2_guess,
            first_committer: self.first_committer,
            matchup_type: self.matchup_type,
            transcript_hash: self.transcript_hash,
        }
    }
}

impl OutcomeCert {
    /// Canonical payload: 11 words, 352 bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(352);
        push_word(&mut out, &OUTCOME_MAGIC);
        push_u16(&mut out, SCHEMA_VERSION);
        push_word(&mut out, &self.match_id);
        push_word(&mut out, &self.match_live_digest);
        push_u8(&mut out, self.outcome_kind as u8);
        push_u8(&mut out, self.step_count);
        push_u8(&mut out, self.p1_guess);
        push_u8(&mut out, self.p2_guess);
        push_u8(&mut out, self.first_committer);
        push_u8(&mut out, self.matchup_type);
        push_word(&mut out, &self.transcript_hash);
        debug_assert_eq!(out.len(), OUTCOME_WORDS.saturating_mul(WORD));
        out
    }
}

/// keccak256 for off-chain consumers (backend, tests, vector
/// generation). On-chain consumers hash the payload with their own
/// primitive (Solana keccak syscall / Solidity keccak256).
#[cfg(feature = "keccak")]
pub fn keccak256(payload: &[u8]) -> [u8; 32] {
    use tiny_keccak::{Hasher, Keccak};
    let mut hasher = Keccak::v256();
    hasher.update(payload);
    let mut out = [0u8; 32];
    hasher.finalize(&mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_leg(seed: u8) -> CertLeg {
        CertLeg {
            chain_tag: [seed; 32],
            contract: [seed.wrapping_add(1); 32],
            player: [seed.wrapping_add(2); 32],
            session_key: [seed.wrapping_add(3); 20],
            stake: u128::from(seed).wrapping_mul(1_000_000),
            tranche: u128::from(seed).wrapping_mul(2_000_000),
        }
    }

    fn sample_match_live() -> MatchLiveCert {
        MatchLiveCert {
            match_id: [0xAA; 32],
            tournament_id: 7,
            matchup_commitment: [0xBB; 32],
            leg_a: sample_leg(0x10),
            leg_b: sample_leg(0x20),
            quote_timestamp: 1_765_000_000,
            quote_max_age_secs: 300,
            match_deadline: 1_765_000_900,
            claim_window_secs: 3600,
            a_is_p1: 1,
        }
    }

    #[test]
    fn encoded_sizes_are_fixed() {
        assert_eq!(sample_match_live().encode().len(), 704);
        let checkpoint = Checkpoint {
            match_live_digest: [1; 32],
            step_count: 2,
            p1_commit: [2; 32],
            p2_commit: [3; 32],
            p1_guess: 255,
            p2_guess: 255,
            first_committer: 1,
            matchup_type: 255,
            transcript_hash: [4; 32],
            r_matchup: [0; 32],
        };
        assert_eq!(checkpoint.encode().len(), 384);
        let outcome = OutcomeCert {
            match_id: [5; 32],
            match_live_digest: [6; 32],
            outcome_kind: OutcomeKind::HeteroP1Wins,
            step_count: TERMINAL_STEP_COUNT,
            p1_guess: 0,
            p2_guess: 1,
            first_committer: 1,
            matchup_type: 1,
            transcript_hash: [7; 32],
        };
        assert_eq!(outcome.encode().len(), 352);
    }

    #[test]
    fn encoding_is_field_order_sensitive() {
        let base = sample_match_live();
        let mut swapped = base.clone();
        core::mem::swap(&mut swapped.leg_a, &mut swapped.leg_b);
        assert_ne!(base.encode(), swapped.encode());
    }

    #[test]
    fn small_ints_are_left_padded_be_words() {
        let cert = sample_match_live();
        let bytes = cert.encode();
        // Word 1 is schema_version: 30 zero bytes then 0x00 0x01.
        let version_word = &bytes[32..64];
        assert!(version_word[..30].iter().all(|b| *b == 0));
        assert_eq!(&version_word[30..], &[0x00, 0x01]);
        // Last word is a_is_p1 = 1.
        let last = &bytes[bytes.len() - 32..];
        assert!(last[..31].iter().all(|b| *b == 0));
        assert_eq!(last[31], 1);
    }

    #[test]
    fn outcome_kind_round_trips_and_rejects_unknown() {
        for value in 0..=8u8 {
            let kind = OutcomeKind::from_u8(value).expect("known outcome kind");
            assert_eq!(kind as u8, value);
        }
        assert_eq!(OutcomeKind::from_u8(9), None);
        assert_eq!(OutcomeKind::from_u8(255), None);
    }

    #[cfg(feature = "keccak")]
    #[test]
    fn magic_constants_match_their_preimages() {
        assert_eq!(keccak256(b"SWARM_XCHAIN_MATCH_LIVE"), MATCH_LIVE_MAGIC);
        assert_eq!(keccak256(b"SWARM_XCHAIN_CHECKPOINT"), CHECKPOINT_MAGIC);
        assert_eq!(keccak256(b"SWARM_XCHAIN_OUTCOME"), OUTCOME_MAGIC);
    }

    fn cp(step: u8, p1: u8, p2: u8, first: u8, matchup: u8) -> Checkpoint {
        Checkpoint {
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

    /// The exhaustive (step, p1, p2, first_committer, matchup, expected_kind)
    /// truth table — the same rows `tests/golden_vectors.rs` writes to
    /// `outcome-derivation.json` (which `CertLib.deriveClaimOutcome` reads) and
    /// `programs/.../cert.rs` mirrors. Hand-authored expected values here are the
    /// independent audit of this reference impl. 255 = UNREVEALED. Covers every
    /// branch of `derive_outcome_kind` + `derive_terminal_outcome`.
    const DERIVATION_TRUTH_TABLE: &[(u8, u8, u8, u8, u8, u8)] = &[
        (4, 0, 0, 1, 0, 0), // homog both correct (fc irrelevant)
        (4, 0, 0, 2, 0, 0),
        (4, 0, 1, 1, 0, 1), // homog p1 correct
        (4, 1, 0, 1, 0, 2), // homog p2 correct
        (4, 1, 1, 1, 0, 3), // homog both wrong
        (4, 1, 0, 1, 1, 4), // hetero p1 correct
        (4, 0, 1, 1, 1, 5), // hetero p2 correct
        (4, 1, 1, 1, 1, 4), // hetero both correct, tie → first_committer 1
        (4, 1, 1, 2, 1, 5), // hetero both correct, tie → first_committer 2
        (4, 0, 0, 1, 1, 3), // hetero both wrong
        (4, 0, 0, 2, 1, 3),
        (0, 255, 255, 255, 1, 8), // timeout step 0
        (0, 255, 255, 0, 0, 8),
        (1, 255, 255, 1, 1, 6), // timeout step 1: committer wins
        (1, 255, 255, 2, 1, 7),
        (1, 255, 255, 0, 1, 8), // inconsistent committer → forfeit
        (1, 255, 255, 3, 1, 8),
        (2, 255, 255, 1, 1, 8), // timeout step 2
        (2, 255, 255, 2, 0, 8),
        (3, 1, 255, 1, 1, 6), // timeout step 3: sole revealer wins
        (3, 255, 1, 1, 1, 7),
        (3, 255, 255, 1, 1, 8), // neither revealed
        (3, 1, 0, 1, 1, 8),     // both revealed (guard) → forfeit
        (3, 0, 1, 2, 0, 8),
    ];

    #[test]
    fn derive_outcome_kind_matches_truth_table() {
        for &(step, p1, p2, fc, m, expected) in DERIVATION_TRUTH_TABLE {
            assert_eq!(
                cp(step, p1, p2, fc, m).derive_outcome_kind() as u8,
                expected,
                "row step={step} p1={p1} p2={p2} first_committer={fc} matchup={m}"
            );
        }
    }

    #[test]
    fn to_outcome_cert_carries_transcript_and_derives_kind() {
        let mut checkpoint = cp(4, 1, 0, 1, 1);
        checkpoint.match_live_digest = [0xAB; 32];
        checkpoint.transcript_hash = [0xCD; 32];
        let match_id = [0xEF; 32];

        let oc = checkpoint.to_outcome_cert(match_id);
        assert_eq!(oc.match_id, match_id);
        assert_eq!(oc.match_live_digest, [0xAB; 32]);
        assert_eq!(oc.transcript_hash, [0xCD; 32]);
        assert_eq!(oc.outcome_kind, OutcomeKind::HeteroP1Wins);
        assert_eq!(oc.step_count, 4);
        assert_eq!(oc.p1_guess, 1);
        assert_eq!(oc.p2_guess, 0);
        assert_eq!(oc.first_committer, 1);
        assert_eq!(oc.matchup_type, 1);

        // A non-terminal checkpoint yields a timeout kind whose digest is a
        // valid, encodable outcome cert (settle accepts timeout kinds at <4).
        let timeout_oc = cp(1, 255, 255, 2, 1).to_outcome_cert(match_id);
        assert_eq!(timeout_oc.outcome_kind, OutcomeKind::TimeoutP2Wins);
        assert_eq!(timeout_oc.encode().len(), 352);
    }
}
