// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Script} from "forge-std/Script.sol";
import {console2} from "forge-std/console2.sol";
import {CoordinationGameV4} from "../src/CoordinationGameV4.sol";

/// @notice Deploy a fresh CoordinationGameV4 implementation and point an
///         EXISTING proxy at it, preserving all state.
///
/// @dev This is deliberately NOT DeployCoordinationGameV4.s.sol. That script
///      deploys a new proxy and initializes it, which ORPHANS every season,
///      player record and unclaimed balance held by the current proxy — on a
///      chain where a finalized season still owes money, that is unrecoverable.
///      An upgrade must touch the implementation pointer and nothing else.
///
///      `upgradeToAndCall(impl, "")` passes EMPTY calldata on purpose: there is
///      no re-initializer to run. Passing an `initialize` call here would revert
///      (the proxy is already initialized) and passing any other call would
///      execute it in the proxy's storage context — the single most dangerous
///      thing an upgrade can do accidentally.
///
///      Storage layout is pinned by CoordinationGameV4Layout.t.sol. The bases
///      below SeasonPot (PullPayment, AttesterGated) carry NO storage gap, so a
///      field added to any of them shifts every slot beneath it and this upgrade
///      would silently reinterpret live balances. That test is the gate; run it
///      before deploying an implementation built from changed sources.
contract UpgradeCoordinationGameV4Script is Script {
    function run() external returns (address implementation) {
        address proxy = vm.envAddress("V4_PROXY");
        require(proxy.code.length > 0, "V4_PROXY has no code");

        vm.startBroadcast();
        CoordinationGameV4 impl = new CoordinationGameV4();
        CoordinationGameV4(payable(proxy)).upgradeToAndCall(address(impl), "");
        vm.stopBroadcast();

        console2.log("proxy         ", proxy);
        console2.log("implementation", address(impl));
        return address(impl);
    }
}
