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

    /// @notice Withdraw the caller's accrued balance. CEI + nonReentrant: the
    ///         balance is zeroed before the transfer, so a reverting recipient
    ///         only fails its OWN withdraw.
    function withdraw() external nonReentrant {
        uint256 amount = withdrawable[msg.sender];
        if (amount == 0) revert NothingToWithdraw();
        withdrawable[msg.sender] = 0;
        emit Withdrawn(msg.sender, amount);
        (bool ok,) = payable(msg.sender).call{value: amount}("");
        require(ok, "withdraw failed");
    }
}
