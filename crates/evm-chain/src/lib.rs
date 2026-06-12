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

        function lockTranche(bytes32 matchId, uint128 trancheWei) external;

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

/// Build an unsigned `lockTranche` call (operator-signed).
pub fn build_lock_tranche(
    contract: Address,
    match_id: [u8; 32],
    tranche_wei: u128,
) -> UnsignedEvmCall {
    let data = CrossChainGame::lockTrancheCall {
        matchId: match_id.into(),
        trancheWei: tranche_wei,
    }
    .abi_encode();
    call(contract, data, U256::ZERO)
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

    #[test]
    fn lock_tranche_is_non_payable() {
        let tx = build_lock_tranche(C, [0xBB; 32], 5_000_000_000_000_000);
        assert_eq!(tx.value, U256::ZERO);
        assert_eq!(
            &tx.data[..4],
            &CrossChainGame::lockTrancheCall::SELECTOR[..]
        );
    }

    #[test]
    fn settle_encodes_certs_and_six_sigs() {
        let cert = MatchLiveCert {
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
        };
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
