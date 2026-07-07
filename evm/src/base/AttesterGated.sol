// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";

/// @title AttesterGated — allow-listed off-chain-signer gate
/// @notice One dedicated secp256k1 key (the game matchmaker / cert operator,
///         or the Shillbot attester) authorizes privileged payloads by
///         signature over a domain-bound digest. Storage is private so each
///         consumer exposes its own domain name (`operatorSigner()` /
///         `attesterSigner()`); rotation goes through the consumer's
///         owner-only `setConfig`, whose single `_validateConfig` gate owns
///         the signer≠owner≠treasury key-separation checks.
abstract contract AttesterGated {
    address private _authorizedSigner;

    /// @dev Rotate the authorized signer. Consumers MUST call this only from
    ///      an owner-gated path after `_validateConfig` has vetted the key.
    function _setAuthorizedSigner(address signer) internal {
        _authorizedSigner = signer;
    }

    function _authorizedSignerAddr() internal view returns (address) {
        return _authorizedSigner;
    }

    /// @dev True iff `sig` (65-byte [r||s||v], v = 27|28 — the
    ///      chain-core `cosign::sign_digest_eth` form) over `digest` recovers
    ///      to the authorized signer. Malformed signatures revert inside
    ///      OZ ECDSA; a valid signature from the wrong key returns false.
    ///      Consumers revert their own `BadSignature()` on false (Solidity
    ///      cannot surface a base-declared error through the derived type,
    ///      and existing tests reference `<Consumer>.BadSignature.selector`).
    function _recoverAndCheck(bytes32 digest, bytes memory sig) internal view returns (bool) {
        return ECDSA.recover(digest, sig) == _authorizedSigner;
    }
}
