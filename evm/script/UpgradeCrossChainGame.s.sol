// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Script} from "forge-std/Script.sol";
import {console2} from "forge-std/console2.sol";
import {CrossChainGame} from "../src/CrossChainGame.sol";

/// @notice Deploy a fresh CrossChainGame implementation and point an
///         EXISTING proxy at it, preserving all state (poolBalance, matches, claims).
contract UpgradeCrossChainGameScript is Script {
    function run() external returns (address implementation) {
        address proxy = vm.envAddress("XCHAIN_PROXY");
        require(proxy.code.length > 0, "XCHAIN_PROXY has no code");

        vm.startBroadcast();
        CrossChainGame impl = new CrossChainGame();
        CrossChainGame(payable(proxy)).upgradeToAndCall(address(impl), "");
        vm.stopBroadcast();

        console2.log("proxy         ", proxy);
        console2.log("implementation", address(impl));
        return address(impl);
    }
}
