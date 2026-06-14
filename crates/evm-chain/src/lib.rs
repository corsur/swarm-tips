#![deny(warnings)]
#![deny(clippy::all)]
#![deny(clippy::arithmetic_side_effects)]

//! Unsigned EVM transaction builders for the CrossChainGame contract.
//!
//! Mirrors the non-custodial `game-chain` pattern: the backend builds an
//! unsigned EIP-1559 transaction, the agent/operator signs it client-side,
//! and the backend submits the raw bytes. This crate never holds a key.
//!
//! The certificate structs are bound via `sol!` so their ABI encoding is
//! the EVM-side counterpart of `chain_core::cert_schema`; the canonical
//! signing digest itself is produced by chain-core (shared across chains).
//!
//! Pure calldata builder: no RPC, no signer. The backend owns submission.

use alloy_primitives::{Address, Bytes, U256};
use alloy_sol_types::{sol, SolCall};

sol! {
    /// One settlement leg. Field order matches CertLib.sol / cert_schema.rs.
    #[derive(Debug)]
    struct Leg {
        bytes32 chainTag;
        bytes32 contractId;
        bytes32 player;
        address sessionKey;
        uint128 stake;
        uint128 tranche;
    }

    #[derive(Debug)]
    struct MatchLiveCert {
        bytes32 matchId;
        uint64 tournamentId;
        bytes32 matchupCommitment;
        Leg legA;
        Leg legB;
        uint64 quoteTimestamp;
        uint32 quoteMaxAgeSecs;
        uint64 matchDeadline;
        uint32 claimWindowSecs;
        uint8 aIsP1;
    }

    #[derive(Debug)]
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

    #[derive(Debug)]
    struct Checkpoint {
        bytes32 matchLiveDigest;
        uint8 stepCount;
        bytes32 p1Commit;
        bytes32 p2Commit;
        uint8 p1Guess;
        uint8 p2Guess;
        uint8 firstCommitter;
        uint8 matchupType;
        bytes32 transcriptHash;
    }

    /// The subset of CrossChainGame the backend builds transactions for.
    interface CrossChainGame {
        function createMatch(
            bytes32 matchId,
            address sessionKey,
            address counterSessionKey,
            bool playerIsP1,
            uint64 fundDeadline,
            uint64 matchDeadline
        ) external payable;

        function lockTranche(MatchLiveCert cert, bytes operatorSig) external;

        function settle(
            MatchLiveCert cert,
            OutcomeCert oc,
            bytes[3] liveSigs,
            bytes[3] ocSigs
        ) external;

        function openClaim(
            MatchLiveCert cert,
            Checkpoint cp,
            bytes[3] liveSigs,
            bytes[2] cpSigs
        ) external;

        function settleClaim(bytes32 matchId) external;

        function refundNoCert(bytes32 matchId) external;
        function refundTimeout(bytes32 matchId) external;

        function poolDeposit() external payable;
    }
}

/// An EVM call the caller must wrap in a transaction, sign, and submit.
/// `value` is the native ETH the call sends (stake / pool deposit), zero for
/// non-payable calls. The backend fills gas/nonce/chainId at submit time.
#[derive(Debug, Clone)]
pub struct UnsignedEvmCall {
    pub to: Address,
    pub data: Bytes,
    pub value: U256,
}

fn call(contract: Address, data: Vec<u8>, value: U256) -> UnsignedEvmCall {
    UnsignedEvmCall {
        to: contract,
        data: data.into(),
        value,
    }
}

impl UnsignedEvmCall {
    /// `(to, data, value)` as client-ready strings: `to` and `data` are
    /// `0x`-hex, `value` is decimal wei (lossless — wei exceeds JSON's safe
    /// integer range). Lets callers (e.g. the MCP server) relay an unsigned
    /// call without depending on alloy types.
    pub fn to_hex_parts(&self) -> (String, String, String) {
        (
            self.to.to_string(),
            self.data.to_string(),
            self.value.to_string(),
        )
    }
}

/// Byte-array variant of [`build_create_match`] for callers that hold raw
/// 20/32-byte addresses (e.g. parsed from a relay payload) and don't want an
/// alloy dependency. Addresses are taken as their raw 20 bytes.
#[allow(clippy::too_many_arguments)]
pub fn build_create_match_parts(
    contract: [u8; 20],
    match_id: [u8; 32],
    session_key: [u8; 20],
    counter_session_key: [u8; 20],
    player_is_p1: bool,
    fund_deadline: u64,
    match_deadline: u64,
    stake_wei: u128,
) -> UnsignedEvmCall {
    build_create_match(
        Address::from(contract),
        match_id,
        Address::from(session_key),
        Address::from(counter_session_key),
        player_is_p1,
        fund_deadline,
        match_deadline,
        stake_wei,
    )
}

/// Build an unsigned `createMatch` call. The player signs and submits it;
/// `stake_wei` is sent as native ETH.
#[allow(clippy::too_many_arguments)]
pub fn build_create_match(
    contract: Address,
    match_id: [u8; 32],
    session_key: Address,
    counter_session_key: Address,
    player_is_p1: bool,
    fund_deadline: u64,
    match_deadline: u64,
    stake_wei: u128,
) -> UnsignedEvmCall {
    let data = CrossChainGame::createMatchCall {
        matchId: match_id.into(),
        sessionKey: session_key,
        counterSessionKey: counter_session_key,
        playerIsP1: player_is_p1,
        fundDeadline: fund_deadline,
        matchDeadline: match_deadline,
    }
    .abi_encode();
    call(contract, data, U256::from(stake_wei))
}

/// Build an unsigned permissionless `lockTranche` call. The caller submits and
/// pays gas; authorization is `operator_sig` — the operator's match-live
/// signature over the cert (relayed at pairing, no new operator action). The
/// locked amount is `cert.legB.tranche`, so the operator commits the exact
/// tranche by signing the cert.
pub fn build_lock_tranche(
    contract: Address,
    cert: MatchLiveCert,
    operator_sig: [u8; 65],
) -> UnsignedEvmCall {
    let data = CrossChainGame::lockTrancheCall {
        cert,
        operatorSig: operator_sig.to_vec().into(),
    }
    .abi_encode();
    call(contract, data, U256::ZERO)
}

/// Raw (alloy-free) cert leg for callers — e.g. the MCP server — that hold bytes
/// parsed from a relay payload and don't depend on alloy types. `contract`/
/// `player` are 32-byte cert words (EVM addresses left-padded); `session_key` is
/// the 20-byte eth-address form.
#[derive(Debug, Clone)]
pub struct LegParts {
    pub chain_tag: [u8; 32],
    pub contract: [u8; 32],
    pub player: [u8; 32],
    pub session_key: [u8; 20],
    pub stake: u128,
    pub tranche: u128,
}

/// Raw (alloy-free) match-live cert — the `lockTranche` calldata input for
/// callers holding payload bytes. Leg A is Solana, leg B is EVM (canonical order).
#[derive(Debug, Clone)]
pub struct CertParts {
    pub match_id: [u8; 32],
    pub tournament_id: u64,
    pub matchup_commitment: [u8; 32],
    pub leg_a: LegParts,
    pub leg_b: LegParts,
    pub quote_timestamp: u64,
    pub quote_max_age_secs: u32,
    pub match_deadline: u64,
    pub claim_window_secs: u32,
    pub a_is_p1: u8,
}

impl LegParts {
    fn to_sol(&self) -> Leg {
        Leg {
            chainTag: self.chain_tag.into(),
            contractId: self.contract.into(),
            player: self.player.into(),
            sessionKey: Address::from(self.session_key),
            stake: self.stake,
            tranche: self.tranche,
        }
    }
}

impl CertParts {
    fn to_sol(&self) -> MatchLiveCert {
        MatchLiveCert {
            matchId: self.match_id.into(),
            tournamentId: self.tournament_id,
            matchupCommitment: self.matchup_commitment.into(),
            legA: self.leg_a.to_sol(),
            legB: self.leg_b.to_sol(),
            quoteTimestamp: self.quote_timestamp,
            quoteMaxAgeSecs: self.quote_max_age_secs,
            matchDeadline: self.match_deadline,
            claimWindowSecs: self.claim_window_secs,
            aIsP1: self.a_is_p1,
        }
    }
}

/// Byte-array variant of [`build_lock_tranche`] for callers that hold raw cert
/// bytes (e.g. parsed from a relay payload) and don't want an alloy dependency.
/// Permissionless: the caller submits + pays gas; `operator_sig` authorizes.
pub fn build_lock_tranche_parts(
    contract: [u8; 20],
    cert: CertParts,
    operator_sig: [u8; 65],
) -> UnsignedEvmCall {
    build_lock_tranche(Address::from(contract), cert.to_sol(), operator_sig)
}

/// Build an unsigned `settle` call (permissionless). Signatures are the
/// 65-byte `[r||s||v]` form produced by the session/operator keys.
pub fn build_settle(
    contract: Address,
    cert: MatchLiveCert,
    oc: OutcomeCert,
    live_sigs: [Vec<u8>; 3],
    oc_sigs: [Vec<u8>; 3],
) -> UnsignedEvmCall {
    let data = CrossChainGame::settleCall {
        cert,
        oc,
        liveSigs: live_sigs.map(Bytes::from),
        ocSigs: oc_sigs.map(Bytes::from),
    }
    .abi_encode();
    call(contract, data, U256::ZERO)
}

/// Build an unsigned `refundNoCert` call (permissionless).
pub fn build_refund_no_cert(contract: Address, match_id: [u8; 32]) -> UnsignedEvmCall {
    let data = CrossChainGame::refundNoCertCall {
        matchId: match_id.into(),
    }
    .abi_encode();
    call(contract, data, U256::ZERO)
}

/// Build an unsigned `refundTimeout` call (permissionless) — refunds the EVM
/// player after the claim/timeout window and releases the locked tranche.
pub fn build_refund_timeout(contract: Address, match_id: [u8; 32]) -> UnsignedEvmCall {
    let data = CrossChainGame::refundTimeoutCall {
        matchId: match_id.into(),
    }
    .abi_encode();
    call(contract, data, U256::ZERO)
}

/// Byte-array variants of the refund builders for callers that hold a raw
/// 20-byte contract address and don't want an alloy dependency.
pub fn build_refund_no_cert_parts(contract: [u8; 20], match_id: [u8; 32]) -> UnsignedEvmCall {
    build_refund_no_cert(Address::from(contract), match_id)
}

pub fn build_refund_timeout_parts(contract: [u8; 20], match_id: [u8; 32]) -> UnsignedEvmCall {
    build_refund_timeout(Address::from(contract), match_id)
}

/// Build an unsigned `poolDeposit` call (operator-signed); funds the float.
pub fn build_pool_deposit(contract: Address, amount_wei: u128) -> UnsignedEvmCall {
    let data = CrossChainGame::poolDepositCall {}.abi_encode();
    call(contract, data, U256::from(amount_wei))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::address;

    const C: Address = address!("996213ed4099707059b8b5d7489ffF23dAC9770d");

    #[test]
    fn create_match_encodes_selector_and_value() {
        let tx = build_create_match(
            C,
            [0xAA; 32],
            Address::repeat_byte(1),
            Address::repeat_byte(2),
            true,
            100,
            200,
            2_500_000_000_000_000,
        );
        assert_eq!(tx.to, C);
        assert_eq!(tx.value, U256::from(2_500_000_000_000_000u128));
        // selector(4) + 6 32-byte words.
        assert_eq!(tx.data.len(), 4 + 6 * 32);
        // First 4 bytes are the createMatch selector.
        assert_eq!(
            &tx.data[..4],
            &CrossChainGame::createMatchCall::SELECTOR[..]
        );
    }

    #[test]
    fn refund_builders_encode_zero_value_and_correct_selectors() {
        let nocert = build_refund_no_cert_parts([0x11; 20], [0xAA; 32]);
        assert_eq!(nocert.value, U256::ZERO);
        assert_eq!(
            &nocert.data[..4],
            &CrossChainGame::refundNoCertCall::SELECTOR[..]
        );
        let timeout = build_refund_timeout_parts([0x11; 20], [0xAA; 32]);
        assert_eq!(timeout.value, U256::ZERO);
        assert_eq!(
            &timeout.data[..4],
            &CrossChainGame::refundTimeoutCall::SELECTOR[..]
        );
        // Different instructions → different selectors.
        assert_ne!(&nocert.data[..4], &timeout.data[..4]);
    }

    #[test]
    fn create_match_parts_matches_typed_builder_and_serializes_to_hex() {
        let contract = [0x11u8; 20];
        let parts = build_create_match_parts(
            contract,
            [0xAA; 32],
            [0x01; 20],
            [0x02; 20],
            true,
            100,
            200,
            2_500_000_000_000_000,
        );
        // Identical calldata + value to the typed builder.
        let typed = build_create_match(
            Address::from(contract),
            [0xAA; 32],
            Address::repeat_byte(1),
            Address::repeat_byte(2),
            true,
            100,
            200,
            2_500_000_000_000_000,
        );
        assert_eq!(parts.data, typed.data);
        assert_eq!(parts.value, typed.value);

        // Hex parts: to/data are 0x-hex, value is decimal wei.
        let (to, data, value) = parts.to_hex_parts();
        assert_eq!(
            to.to_lowercase(),
            "0x1111111111111111111111111111111111111111"
        );
        assert!(data.starts_with("0x"));
        assert_eq!(value, "2500000000000000");
    }

    fn sample_cert() -> MatchLiveCert {
        MatchLiveCert {
            matchId: [0xAA; 32].into(),
            tournamentId: 7,
            matchupCommitment: [0xBB; 32].into(),
            legA: Leg {
                chainTag: [0x10; 32].into(),
                contractId: [0x11; 32].into(),
                player: [0x12; 32].into(),
                sessionKey: Address::repeat_byte(0x13),
                stake: 1,
                tranche: 2,
            },
            legB: Leg {
                chainTag: [0x20; 32].into(),
                contractId: [0x21; 32].into(),
                player: [0x22; 32].into(),
                sessionKey: Address::repeat_byte(0x23),
                stake: 1,
                tranche: 2,
            },
            quoteTimestamp: 1,
            quoteMaxAgeSecs: 2,
            matchDeadline: 3,
            claimWindowSecs: 4,
            aIsP1: 1,
        }
    }

    #[test]
    fn lock_tranche_is_permissionless_and_non_payable() {
        // New signature: full cert + operator sig (permissionless), not
        // (matchId, trancheWei) onlyOwner.
        let tx = build_lock_tranche(C, sample_cert(), [0x7u8; 65]);
        assert_eq!(tx.value, U256::ZERO);
        assert_eq!(
            &tx.data[..4],
            &CrossChainGame::lockTrancheCall::SELECTOR[..]
        );
    }

    #[test]
    fn lock_tranche_parts_matches_typed_builder() {
        // The alloy-free parts builder must produce byte-identical calldata to
        // the typed builder (so the MCP server can stay alloy-free).
        let leg = |b: u8| LegParts {
            chain_tag: [b; 32],
            contract: [b + 1; 32],
            player: [b + 2; 32],
            session_key: [0x13; 20],
            stake: 1,
            tranche: 2,
        };
        // Mirror sample_cert() exactly.
        let parts = CertParts {
            match_id: [0xAA; 32],
            tournament_id: 7,
            matchup_commitment: [0xBB; 32],
            leg_a: LegParts {
                session_key: [0x13; 20],
                ..leg(0x10)
            },
            leg_b: LegParts {
                session_key: [0x23; 20],
                ..leg(0x20)
            },
            quote_timestamp: 1,
            quote_max_age_secs: 2,
            match_deadline: 3,
            claim_window_secs: 4,
            a_is_p1: 1,
        };
        let typed = build_lock_tranche(Address::from([0x11u8; 20]), sample_cert(), [0x7; 65]);
        let via_parts = build_lock_tranche_parts([0x11u8; 20], parts, [0x7; 65]);
        assert_eq!(
            via_parts.data, typed.data,
            "parts calldata must match typed"
        );
        assert_eq!(via_parts.value, typed.value);
        assert_eq!(via_parts.to, typed.to);
    }

    #[test]
    fn settle_encodes_certs_and_six_sigs() {
        let cert = sample_cert();
        let oc = OutcomeCert {
            matchId: [0xAA; 32].into(),
            matchLiveDigest: [0xCC; 32].into(),
            outcomeKind: 4,
            stepCount: 4,
            p1Guess: 1,
            p2Guess: 0,
            firstCommitter: 1,
            matchupType: 1,
            transcriptHash: [0xDD; 32].into(),
        };
        let sigs = || [vec![1u8; 65], vec![2u8; 65], vec![3u8; 65]];
        let tx = build_settle(C, cert, oc, sigs(), sigs());
        assert_eq!(tx.value, U256::ZERO);
        assert_eq!(&tx.data[..4], &CrossChainGame::settleCall::SELECTOR[..]);
    }
}
