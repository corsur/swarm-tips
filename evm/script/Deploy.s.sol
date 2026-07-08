// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Script} from "forge-std/Script.sol";
import {console2} from "forge-std/console2.sol";
import {CrossChainGame} from "../src/CrossChainGame.sol";

/// @notice Deploys CrossChainGame to the configured network. Parameters
///         come from env vars so the same script serves Base Sepolia
///         (testnet) and any future chain — values mirror the chain
///         registry (crates/chain-registry), the single source of truth.
///
/// Required env:
///   XCHAIN_CHAIN_TAG       keccak256 of the CAIP-2 string (0x… 32 bytes)
///   XCHAIN_OWNER           operator EOA (the EVM deploy/upgrade authority)
///   XCHAIN_OPERATOR_SIGNER dedicated secp256k1 cert signer (NOT the owner)
///   XCHAIN_TREASURY        DAO treasury address
///   XCHAIN_TREASURY_BPS    treasury split, 2000..8000
///   XCHAIN_STAKE_WEI       per-match stake
///   XCHAIN_MAX_TRANCHE_WEI per-match tranche clamp
///   XCHAIN_DAILY_TRANCHE_CAP_WEI  rolling-24h aggregate tranche-lock cap (M6)
///   XCHAIN_CLAIM_WINDOW    contested-claim window (secs)
///   XCHAIN_SKEW_MARGIN     cross-chain skew margin (secs)
///
/// Run:
///   forge script script/Deploy.s.sol --rpc-url base_sepolia \
///     --account xchain-testnet --broadcast
contract DeployScript is Script {
    function run() external returns (CrossChainGame game) {
        bytes32 chainTag = vm.envBytes32("XCHAIN_CHAIN_TAG");
        address owner = vm.envAddress("XCHAIN_OWNER");
        address operatorSigner = vm.envAddress("XCHAIN_OPERATOR_SIGNER");
        address treasury = vm.envAddress("XCHAIN_TREASURY");
        uint16 treasuryBps = uint16(vm.envUint("XCHAIN_TREASURY_BPS"));
        uint128 stakeWei = uint128(vm.envUint("XCHAIN_STAKE_WEI"));
        uint128 maxTrancheWei = uint128(vm.envUint("XCHAIN_MAX_TRANCHE_WEI"));
        uint128 dailyTrancheCapWei = uint128(vm.envUint("XCHAIN_DAILY_TRANCHE_CAP_WEI"));
        uint32 claimWindow = uint32(vm.envUint("XCHAIN_CLAIM_WINDOW"));
        uint32 skewMargin = uint32(vm.envUint("XCHAIN_SKEW_MARGIN"));

        vm.startBroadcast();
        game = new CrossChainGame(
            chainTag,
            owner,
            operatorSigner,
            treasury,
            treasuryBps,
            stakeWei,
            maxTrancheWei,
            dailyTrancheCapWei,
            claimWindow,
            skewMargin
        );
        vm.stopBroadcast();

        console2.log("CrossChainGame deployed at:", address(game));
        console2.log("  owner:", owner);
        console2.log("  operatorSigner:", operatorSigner);
    }
}
