// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Script} from "forge-std/Script.sol";
import {console2} from "forge-std/console2.sol";
import {ShillbotEscrow} from "../src/ShillbotEscrow.sol";

/// @notice Deploys ShillbotEscrow to the configured network. Parameters come
///         from env vars, mirroring the chain registry
///         (crates/chain-registry, the single source of truth). The chain tag
///         is either given directly (SHILLBOT_ESCROW_CHAIN_TAG, 32 bytes) or
///         derived as keccak256 of the CAIP-2 string
///         (SHILLBOT_ESCROW_CAIP2, e.g. "eip155:84532").
///
/// Required env:
///   SHILLBOT_ESCROW_OWNER                      owner EOA (EVM deploy/upgrade authority)
///   SHILLBOT_ESCROW_ATTESTER_SIGNER            dedicated attester key (NOT owner/treasury)
///   SHILLBOT_ESCROW_TREASURY                   DAO treasury address
///   SHILLBOT_ESCROW_FEE_BPS                    protocol fee, 100..2500
///   SHILLBOT_ESCROW_QUALITY_THRESHOLD          kind-0 payout threshold, < 1_000_000
///   SHILLBOT_ESCROW_CHALLENGE_WINDOW_SECS      60..30d
///   SHILLBOT_ESCROW_DISPUTE_WINDOW_SECS        1h..30d
///   SHILLBOT_ESCROW_VERIFICATION_TIMEOUT_SECS  1h..90d
///   SHILLBOT_ESCROW_BOND_MULTIPLIER            challenge bond multiplier, 2..10
///   SHILLBOT_ESCROW_BOND_SLASH_TREASURY_BPS    treasury share of slashed bonds, 0..10000
///   SHILLBOT_ESCROW_MIN_ESCROW_WEI             > 0
///   SHILLBOT_ESCROW_MAX_ESCROW_WEI             >= min
///   SHILLBOT_ESCROW_CHAIN_TAG or SHILLBOT_ESCROW_CAIP2 (tag wins if both set)
///
/// Run:
///   forge script script/DeployShillbotEscrow.s.sol --rpc-url base_sepolia \
///     --private-key "$DEPLOYER_PK" --broadcast
contract DeployShillbotEscrowScript is Script {
    function run() external returns (ShillbotEscrow escrow) {
        bytes32 chainTag = vm.envOr("SHILLBOT_ESCROW_CHAIN_TAG", bytes32(0));
        if (chainTag == bytes32(0)) {
            chainTag = keccak256(bytes(vm.envString("SHILLBOT_ESCROW_CAIP2")));
        }
        address owner = vm.envAddress("SHILLBOT_ESCROW_OWNER");
        ShillbotEscrow.Config memory cfg = ShillbotEscrow.Config({
            attesterSigner: vm.envAddress("SHILLBOT_ESCROW_ATTESTER_SIGNER"),
            treasury: vm.envAddress("SHILLBOT_ESCROW_TREASURY"),
            protocolFeeBps: uint16(vm.envUint("SHILLBOT_ESCROW_FEE_BPS")),
            qualityThreshold: uint64(vm.envUint("SHILLBOT_ESCROW_QUALITY_THRESHOLD")),
            challengeWindowSecs: uint32(vm.envUint("SHILLBOT_ESCROW_CHALLENGE_WINDOW_SECS")),
            disputeWindowSecs: uint32(vm.envUint("SHILLBOT_ESCROW_DISPUTE_WINDOW_SECS")),
            verificationTimeoutSecs: uint32(vm.envUint("SHILLBOT_ESCROW_VERIFICATION_TIMEOUT_SECS")),
            challengeBondMultiplier: uint8(vm.envUint("SHILLBOT_ESCROW_BOND_MULTIPLIER")),
            bondSlashTreasuryBps: uint16(vm.envUint("SHILLBOT_ESCROW_BOND_SLASH_TREASURY_BPS")),
            minEscrowWei: uint128(vm.envUint("SHILLBOT_ESCROW_MIN_ESCROW_WEI")),
            maxEscrowWei: uint128(vm.envUint("SHILLBOT_ESCROW_MAX_ESCROW_WEI"))
        });

        vm.startBroadcast();
        escrow = new ShillbotEscrow(chainTag, owner, cfg);
        vm.stopBroadcast();

        console2.log("ShillbotEscrow deployed at:", address(escrow));
        console2.log("  owner:", owner);
        console2.log("  attesterSigner:", cfg.attesterSigner);
        console2.logBytes32(chainTag);
    }
}
