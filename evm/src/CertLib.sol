// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @title CertLib — canonical certificate encoding for cross-chain matches
/// @notice Solidity mirror of `crates/chain-core/src/cert_schema.rs`. Every
///         field is one 32-byte big-endian word; `keccak256(abi.encode(...))`
///         here is byte-identical to the Rust word-concat encoding. The two
///         implementations are held equal by the golden vectors in
///         `tests/fixtures/cert-vectors.json` (verified by both forge tests
///         and `cargo test`). Field order is the wire format — never reorder.
///
///         Not EIP-712 by design: both legs' chain tags + contracts live in
///         ONE signed payload and each verifier executes only its own leg,
///         which is what makes cross-chain replay/double-claim structurally
///         impossible. See multichain/decision.md §4.1.
library CertLib {
    /// keccak256("SWARM_XCHAIN_MATCH_LIVE")
    bytes32 internal constant MATCH_LIVE_MAGIC = 0x65cc8f7e0abe2dde3aa2548cf4b5e5ba0b4c8569bf820a9342a4fa4754adcd15;
    /// keccak256("SWARM_XCHAIN_CHECKPOINT")
    bytes32 internal constant CHECKPOINT_MAGIC = 0xc2635465d7fdde2b4fa6c4cd109cfc3562f688fab9f72af818db1ef1fcde3469;
    /// keccak256("SWARM_XCHAIN_OUTCOME")
    bytes32 internal constant OUTCOME_MAGIC = 0xd885a302d446a492952cd7b670a9b6e68c9627f84578902457b8cfe227843e3d;

    uint16 internal constant SCHEMA_VERSION = 1;

    /// A terminal transcript has all four steps: both commits + both reveals.
    uint8 internal constant TERMINAL_STEP_COUNT = 4;

    /// Sentinels mirroring the on-chain Solana value space.
    uint8 internal constant UNREVEALED = 255;

    /// Outcome kinds — numeric values are the cross-chain wire format.
    uint8 internal constant HOMOG_BOTH_CORRECT = 0;
    uint8 internal constant HOMOG_P1_CORRECT = 1;
    uint8 internal constant HOMOG_P2_CORRECT = 2;
    uint8 internal constant BOTH_WRONG = 3;
    uint8 internal constant HETERO_P1_WINS = 4;
    uint8 internal constant HETERO_P2_WINS = 5;
    uint8 internal constant TIMEOUT_P1_WINS = 6;
    uint8 internal constant TIMEOUT_P2_WINS = 7;
    uint8 internal constant TIMEOUT_BOTH_FORFEIT = 8;

    /// One settlement leg — the per-chain escrow terms.
    struct Leg {
        bytes32 chainTag; // keccak256 of the CAIP-2 string
        bytes32 contractId; // program ID (solana) or address left-padded (evm)
        bytes32 player; // wallet: Pubkey or address left-padded
        address sessionKey; // per-match secp256k1 session key
        uint128 stake; // native base units (lamports / wei)
        uint128 tranche; // max float-pool payout locked for this match
    }

    /// Match-live certificate. Leg A is ALWAYS Solana, leg B ALWAYS EVM
    /// (canonical ordering so both legs hash identical bytes).
    struct MatchLiveCert {
        bytes32 matchId;
        uint64 tournamentId;
        bytes32 matchupCommitment;
        Leg legA;
        Leg legB;
        uint64 quoteTimestamp;
        uint32 quoteMaxAgeSecs;
        uint64 matchDeadline; // unix seconds on BOTH legs (never slots)
        uint32 claimWindowSecs;
        uint8 aIsP1; // 1 when leg A's player is P1 in the payoff matrix
    }

    /// Co-signed transcript checkpoint. Monotonic: higher stepCount
    /// supersedes lower (reveals supersede commits).
    struct Checkpoint {
        bytes32 matchLiveDigest;
        uint8 stepCount; // 0..=4
        bytes32 p1Commit;
        bytes32 p2Commit;
        uint8 p1Guess; // 255 = unrevealed
        uint8 p2Guess;
        uint8 firstCommitter; // 1 | 2; 255 before any commit
        uint8 matchupType; // 0 same-team, 1 diff-team, 255 unset
        bytes32 transcriptHash;
    }

    /// Outcome certificate: terminal checkpoint + explicit result, bound
    /// to the exact match-live cert by digest.
    struct OutcomeCert {
        bytes32 matchId;
        bytes32 matchLiveDigest;
        uint8 outcomeKind;
        uint8 stepCount;
        uint8 p1Guess;
        uint8 p2Guess;
        uint8 firstCommitter;
        uint8 matchupType;
        bytes32 transcriptHash;
    }

    /// @notice 22 words, 704 bytes — identical to cert_schema.rs encode().
    function encodeMatchLive(MatchLiveCert memory cert) internal pure returns (bytes memory) {
        // abi.encode pads every value type to a 32-byte word, matching the
        // Rust side's word-concat exactly. Two encodes spliced to stay
        // under the stack limit.
        bytes memory head = abi.encode(
            MATCH_LIVE_MAGIC,
            uint256(SCHEMA_VERSION),
            cert.matchId,
            uint256(cert.tournamentId),
            cert.matchupCommitment,
            cert.legA.chainTag,
            cert.legA.contractId,
            cert.legA.player,
            cert.legA.sessionKey,
            uint256(cert.legA.stake),
            uint256(cert.legA.tranche)
        );
        bytes memory tail = abi.encode(
            cert.legB.chainTag,
            cert.legB.contractId,
            cert.legB.player,
            cert.legB.sessionKey,
            uint256(cert.legB.stake),
            uint256(cert.legB.tranche),
            uint256(cert.quoteTimestamp),
            uint256(cert.quoteMaxAgeSecs),
            uint256(cert.matchDeadline),
            uint256(cert.claimWindowSecs),
            uint256(cert.aIsP1)
        );
        return bytes.concat(head, tail);
    }

    function matchLiveDigest(MatchLiveCert memory cert) internal pure returns (bytes32) {
        return keccak256(encodeMatchLive(cert));
    }

    /// @notice 11 words, 352 bytes.
    function encodeCheckpoint(Checkpoint memory cp) internal pure returns (bytes memory) {
        return abi.encode(
            CHECKPOINT_MAGIC,
            uint256(SCHEMA_VERSION),
            cp.matchLiveDigest,
            uint256(cp.stepCount),
            cp.p1Commit,
            cp.p2Commit,
            uint256(cp.p1Guess),
            uint256(cp.p2Guess),
            uint256(cp.firstCommitter),
            uint256(cp.matchupType),
            cp.transcriptHash
        );
    }

    function checkpointDigest(Checkpoint memory cp) internal pure returns (bytes32) {
        return keccak256(encodeCheckpoint(cp));
    }

    /// @notice 11 words, 352 bytes.
    function encodeOutcome(OutcomeCert memory oc) internal pure returns (bytes memory) {
        return abi.encode(
            OUTCOME_MAGIC,
            uint256(SCHEMA_VERSION),
            oc.matchId,
            oc.matchLiveDigest,
            uint256(oc.outcomeKind),
            uint256(oc.stepCount),
            uint256(oc.p1Guess),
            uint256(oc.p2Guess),
            uint256(oc.firstCommitter),
            uint256(oc.matchupType),
            oc.transcriptHash
        );
    }

    function outcomeDigest(OutcomeCert memory oc) internal pure returns (bytes32) {
        return keccak256(encodeOutcome(oc));
    }

    function isTimeoutKind(uint8 kind) internal pure returns (bool) {
        return kind == TIMEOUT_P1_WINS || kind == TIMEOUT_P2_WINS || kind == TIMEOUT_BOTH_FORFEIT;
    }

    /// @notice The outcome a checkpoint entitles a claimant to under the
    ///         timeout semantics (committer/revealer wins; neither → both
    ///         forfeit). Mirrors the same-chain resolve_timeout rules.
    function deriveClaimOutcome(Checkpoint memory cp) internal pure returns (uint8) {
        if (cp.stepCount == TERMINAL_STEP_COUNT) {
            return deriveTerminalOutcome(cp);
        }
        if (cp.stepCount == 1) {
            // One commit landed, the other never did: committer wins.
            // Defensive: a step-1 checkpoint must name the committer (1|2);
            // any other value is an inconsistent transcript → both forfeit.
            if (cp.firstCommitter == 1) return TIMEOUT_P1_WINS;
            if (cp.firstCommitter == 2) return TIMEOUT_P2_WINS;
            return TIMEOUT_BOTH_FORFEIT;
        }
        if (cp.stepCount == 3) {
            // Both committed, exactly one revealed: the revealer wins.
            // Defensive: require exactly one guess set; both-set or
            // both-unset is an inconsistent transcript → both forfeit.
            bool p1Revealed = cp.p1Guess != UNREVEALED;
            bool p2Revealed = cp.p2Guess != UNREVEALED;
            if (p1Revealed == p2Revealed) return TIMEOUT_BOTH_FORFEIT;
            return p1Revealed ? TIMEOUT_P1_WINS : TIMEOUT_P2_WINS;
        }
        // step 0 (nobody committed) and step 2 (both committed, none
        // revealed): both forfeit — mirrors same-chain Active/Revealing
        // timeout handling.
        return TIMEOUT_BOTH_FORFEIT;
    }

    /// @notice Recompute the payoff-matrix outcome from a terminal
    ///         transcript. Mirrors programs/coordination-game payoff.rs.
    function deriveTerminalOutcome(Checkpoint memory cp) internal pure returns (uint8) {
        // In the coordination game a guess is "correct" when it matches
        // the matchup type (guess 0 = same-team, 1 = diff-team).
        bool p1Correct = cp.p1Guess == cp.matchupType;
        bool p2Correct = cp.p2Guess == cp.matchupType;
        if (cp.matchupType == 0) {
            // Homogeneous (same-team) matrix.
            if (p1Correct && p2Correct) return HOMOG_BOTH_CORRECT;
            if (p1Correct) return HOMOG_P1_CORRECT;
            if (p2Correct) return HOMOG_P2_CORRECT;
            return BOTH_WRONG;
        }
        // Heterogeneous (diff-team): winner takes both stakes; ties break
        // to the first committer.
        if (!p1Correct && !p2Correct) return BOTH_WRONG;
        if (p1Correct == p2Correct) {
            return cp.firstCommitter == 1 ? HETERO_P1_WINS : HETERO_P2_WINS;
        }
        return p1Correct ? HETERO_P1_WINS : HETERO_P2_WINS;
    }
}
