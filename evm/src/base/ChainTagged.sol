// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @title ChainTagged — CAIP-2 chain-tag domain binding
/// @notice Pins the deployment to one chain: `CHAIN_TAG` is the keccak256 of
///         the CAIP-2 string (e.g. keccak256("eip155:84532") for Base
///         Sepolia), set from the chain registry at deploy and immutable so a
///         certificate/attestation authored for another chain can never
///         execute here. Consumers: CrossChainGame (cert legs) and
///         ShillbotEscrow (attestation digests). CoordinationGame is
///         deliberately NOT a consumer — same-chain digests bind
///         `block.chainid` directly and carry no cross-VM chain tag.
abstract contract ChainTagged {
    bytes32 public immutable CHAIN_TAG;

    error BadChainTag();

    constructor(bytes32 chainTag) {
        if (chainTag == bytes32(0)) revert BadChainTag();
        CHAIN_TAG = chainTag;
    }

    /// @dev This deployment's 32-byte contract-id word (the EVM address
    ///      left-padded to a word) — how EVM contracts are identified inside
    ///      cross-VM signed payloads (cert legs, attestation certs).
    function _contractIdWord() internal view returns (bytes32) {
        return bytes32(uint256(uint160(address(this))));
    }
}
