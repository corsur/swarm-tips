// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Script} from "forge-std/Script.sol";
import {console2} from "forge-std/console2.sol";
import {CoordinationGame} from "../src/CoordinationGame.sol";

/// @notice Deploys CoordinationGame (the same-chain EVM-vs-EVM game) to the
///         configured network. Parameters come from env vars, mirroring the
///         chain registry (crates/chain-registry, the single source of truth).
///         Shares the common testnet env with the CrossChainGame deploy
///         (owner / operatorSigner / treasury / bps / stake) and adds the two
///         same-chain timeouts. CoordinationGame takes NO chainTag / tranche /
///         skew (no cross-chain certificate or float pool).
///
/// Required env:
///   XCHAIN_OWNER           owner EOA (EVM deploy/upgrade authority)
///   XCHAIN_OPERATOR_SIGNER dedicated secp256k1 matchup attestor (NOT owner/treasury)
///   XCHAIN_TREASURY        DAO treasury address
///   XCHAIN_TREASURY_BPS    treasury split, 2000..8000
///   XCHAIN_STAKE_WEI       per-game stake
///   COORD_COMMIT_TIMEOUT   Active + Committing timeout (secs)
///   COORD_REVEAL_TIMEOUT   Revealing timeout (secs)
///
/// Run:
///   forge script script/DeployCoordinationGame.s.sol --rpc-url base_sepolia \
///     --private-key "$DEPLOYER_PK" --broadcast
contract DeployCoordinationGameScript is Script {
    function run() external returns (CoordinationGame game) {
        address owner = vm.envAddress("XCHAIN_OWNER");
        address operatorSigner = vm.envAddress("XCHAIN_OPERATOR_SIGNER");
        address treasury = vm.envAddress("XCHAIN_TREASURY");
        uint16 treasuryBps = uint16(vm.envUint("XCHAIN_TREASURY_BPS"));
        uint128 stakeWei = uint128(vm.envUint("XCHAIN_STAKE_WEI"));
        uint32 commitTimeout = uint32(vm.envUint("COORD_COMMIT_TIMEOUT"));
        uint32 revealTimeout = uint32(vm.envUint("COORD_REVEAL_TIMEOUT"));

        vm.startBroadcast();
        game =
            new CoordinationGame(owner, operatorSigner, treasury, treasuryBps, stakeWei, commitTimeout, revealTimeout);
        vm.stopBroadcast();

        console2.log("CoordinationGame deployed at:", address(game));
        console2.log("  owner:", owner);
        console2.log("  operatorSigner:", operatorSigner);
    }
}
