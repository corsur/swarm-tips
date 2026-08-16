// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {ReentrancyGuard} from "@openzeppelin/contracts/utils/ReentrancyGuard.sol";

/// @title PullPayment — credit-then-withdraw native-ETH payout ledger (M1)
/// @notice Shared by CoordinationGame, CrossChainGame, and ShillbotEscrow so
///         the pre-mainnet audit reviews ONE payout foundation instead of
///         three hand-rolled copies. Resolution paths CREDIT recipients here
///         instead of pushing ETH, so a recipient that reverts on receive can
///         never brick a state transition or strand escrowed funds; each
///         recipient later pulls via {withdraw}.
/// @dev Inherits ReentrancyGuard so every consumer shares one reentrancy
///      surface; consumers get the `nonReentrant` modifier through this base.
abstract contract PullPayment is ReentrancyGuard {
    /// Pull-payment ledger: accrued, not-yet-pulled payouts per recipient.
    mapping(address => uint256) public withdrawable;

    event Withdrawn(address indexed to, uint256 amount);

    error NothingToWithdraw();

    /// @dev Credit `amount` to `to`'s withdrawable balance. Storage-only —
    ///      never reverts on a hostile recipient, so settlement/refund/payout
    ///      flows always complete.
    function _credit(address to, uint256 amount) internal {
        withdrawable[to] += amount;
    }

    /// @dev Gas forwarded to a push attempt. Comfortably covers a native-ETH
    ///      transfer to an EOA, INCLUDING the cold-account (2600) + new-account
    ///      (25000) worst case, while bounding what a hostile contract recipient
    ///      can burn on the resolving caller before the push fails and falls
    ///      back to the ledger.
    uint256 private constant _PUSH_GAS = 50_000;

    /// @dev Push-preferred payout: deliver `amount` to `to` IMMEDIATELY when the
    ///      push succeeds, falling back to the {withdrawable} ledger only if it
    ///      fails. The common case — an EOA wallet-as-player — is paid inside the
    ///      resolving transaction with no second step, matching Solana's
    ///      push-at-resolve; a recipient that reverts, or a gas-heavy contract
    ///      wallet, simply falls back to a pull, preserving the M1 guarantee that
    ///      a hostile recipient can NEVER block the calling state transition.
    ///
    ///      SAFETY: does an external call, so it MUST be invoked only in the
    ///      Interactions phase (after all Effects) of a `nonReentrant` external
    ///      entry point. The `_PUSH_GAS` cap is far below what any reentrant call
    ///      back into this contract would need, and `nonReentrant` would revert
    ///      such a call anyway — a reentry attempt just makes the push fail and
    ///      credits the ledger. Emits {Withdrawn} on a successful push so an
    ///      auto-payout is observable exactly like a manual {withdraw}.
    function _settle(address to, uint256 amount) internal {
        if (amount == 0) return;
        (bool ok,) = payable(to).call{value: amount, gas: _PUSH_GAS}("");
        if (ok) {
            emit Withdrawn(to, amount);
        } else {
            _credit(to, amount);
        }
    }

    /// @notice Withdraw the caller's accrued balance. CEI + nonReentrant: the
    ///         balance is zeroed before the transfer, so a reverting recipient
    ///         only fails its OWN withdraw.
    function withdraw() external nonReentrant {
        _withdrawTo(msg.sender);
    }

    /// @notice Push `to`'s accrued balance to `to`. Permissionless — anyone (e.g.
    ///         a player's gas-only session key) may trigger the payout, but funds
    ///         only ever go to `to`, never the caller. Lets a wallet-as-player win
    ///         be realized with no wallet popup. Same CEI + nonReentrant safety as
    ///         {withdraw}: a reverting `to` only fails its own payout, funds stay
    ///         credited.
    function withdrawFor(address to) external nonReentrant {
        _withdrawTo(to);
    }

    /// @dev Zero-before-transfer payout of `to`'s credited balance to `to`.
    function _withdrawTo(address to) private {
        uint256 amount = withdrawable[to];
        if (amount == 0) revert NothingToWithdraw();
        withdrawable[to] = 0;
        emit Withdrawn(to, amount);
        (bool ok,) = payable(to).call{value: amount}("");
        require(ok, "withdraw failed");
    }
}
