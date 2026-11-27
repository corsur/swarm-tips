// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Script} from "forge-std/Script.sol";
import {console2} from "forge-std/console2.sol";
import {SafeCast} from "@openzeppelin/contracts/utils/math/SafeCast.sol";
import {CoordinationGameV4} from "../src/CoordinationGameV4.sol";
import {ERC1967Proxy} from "../lib/openzeppelin-contracts/contracts/proxy/ERC1967/ERC1967Proxy.sol";

/// @notice Deploys v4 as implementation + ERC1967 proxy, initialized ATOMICALLY.
///
/// @dev The initializer runs in the proxy's constructor call, in the SAME
///      transaction. That matters: between deployment and initialization the
///      proxy has no owner, and a gap there is a front-running window on
///      `initialize`. Passing the init calldata to the proxy constructor closes
///      it entirely.
///
///      The implementation's own constructor calls `_disableInitializers()`, so
///      the implementation can never be initialized directly either.
contract DeployCoordinationGameV4Script is Script {
    using SafeCast for uint256;

    function run() external returns (address proxy, address implementation) {
        address owner = vm.envAddress("V4_OWNER");
        address operatorSigner = vm.envAddress("XCHAIN_OPERATOR_SIGNER");
        address treasury = vm.envAddress("XCHAIN_TREASURY");
        uint16 treasuryBps = vm.envUint("XCHAIN_TREASURY_BPS").toUint16();
        uint128 stakeWei = vm.envUint("XCHAIN_STAKE_WEI").toUint128();
        uint32 commitTimeout = vm.envUint("COORD_COMMIT_TIMEOUT").toUint32();
        uint32 revealTimeout = vm.envUint("COORD_REVEAL_TIMEOUT").toUint32();
        uint256 seasonId = vm.envUint("V4_SEASON_ID");
        // Short on testnet so expire -> finalize -> claim can actually be
        // exercised on a live chain; a year in production.
        uint64 seasonSecs = vm.envUint("V4_SEASON_SECS").toUint64();

        bytes memory initCall = abi.encodeCall(
            CoordinationGameV4.initialize,
            (owner, operatorSigner, treasury, treasuryBps, stakeWei, commitTimeout, revealTimeout, seasonId, seasonSecs)
        );

        vm.startBroadcast();
        CoordinationGameV4 impl = new CoordinationGameV4();
        ERC1967Proxy p = new ERC1967Proxy(address(impl), initCall);
        vm.stopBroadcast();

        proxy = address(p);
        implementation = address(impl);

        console2.log("CoordinationGameV4 PROXY (use this address):", proxy);
        console2.log("  implementation:", implementation);
        console2.log("  owner:", owner);
        console2.log("  seasonId:", seasonId);
        console2.log("  seasonSecs:", seasonSecs);
    }
}
