// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @title ChainTaggedUpgradeable — proxy-safe CAIP-2 chain-tag domain binding
/// @notice The upgradeable twin of {ChainTagged}. Same purpose — pin a
///         deployment to one chain so a certificate authored for another chain
///         can never execute here — but the tag lives in STORAGE, set once from
///         the consumer's initializer, instead of an `immutable`.
///
///         WHY A SEPARATE BASE: an `immutable` is baked into the IMPLEMENTATION
///         bytecode, so every proxy sharing one implementation would share one
///         tag — wrong for a per-chain UUPS deployment (the Base and Ethereum
///         proxies point at the same logic but must carry different tags). The
///         constructor-based {ChainTagged} stays for the immutable ShillbotEscrow
///         (and the pre-UUPS CrossChainGame); only the UUPS CrossChainGame uses
///         this variant.
abstract contract ChainTaggedUpgradeable {
    /// This deployment's CAIP-2 chain tag (keccak256 of the CAIP-2 string).
    /// Storage, not immutable — see the contract notice.
    bytes32 public chainTag;

    error BadChainTag();

    /// @dev Set the tag once, from the consumer's `initializer`. Mirrors the
    ///      immutable base's zero-check.
    function _setChainTag(bytes32 chainTag_) internal {
        if (chainTag_ == bytes32(0)) revert BadChainTag();
        chainTag = chainTag_;
    }

    /// @dev This deployment's 32-byte contract-id word (the EVM address
    ///      left-padded to a word) — how EVM contracts are identified inside
    ///      cross-VM signed payloads (cert legs, attestation certs).
    function _contractIdWord() internal view returns (bytes32) {
        return bytes32(uint256(uint160(address(this))));
    }
}
