// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Script} from "forge-std/Script.sol";
import {console2} from "forge-std/console2.sol";
import {CrossChainGame} from "../src/CrossChainGame.sol";

/// @notice Re-peg a deployed CrossChainGame's `stakeWei` / `maxTrancheWei` /
///         `dailyTrancheCapWei` without redeploying.
///
///         The companion to SetStake.s.sol. `chain-registry`'s
///         `stake_base_units` governs BOTH contracts on a chain — the
///         same-chain CoordinationGame and the cross-chain CrossChainGame — and
///         re-pegging only one leaves the pair inconsistent. That is exactly
///         what happened on the first re-peg: CoordinationGame moved to
///         0.0027 ETH, CrossChainGame stayed at 0.0005, and evm-ci's
///         registry-deployed-parity job (which reads CrossChainGame) stayed red
///         while a parity script that only read CoordinationGame reported PASS.
///
///         A cross-chain match records exactly `stakeWei` in its certificate,
///         so a CrossChainGame disagreeing with the registry does not merely
///         mis-price — it makes the settle digest check fail on live matches.
///
///         `setConfig` rewrites the whole tuple, so every unrelated field is
///         read back off-chain and passed through unchanged.
///
/// Required env:
///   XCHAIN_GAME                deployed CrossChainGame address
///   NEW_STAKE_WEI              new per-match stake, in wei
///   NEW_MAX_TRANCHE_WEI        new per-match tranche clamp, in wei
///   NEW_DAILY_TRANCHE_CAP_WEI  new daily tranche cap, in wei
contract SetXChainStakeScript is Script {
    function run() external {
        address gameAddr = vm.envAddress("XCHAIN_GAME");
        uint256 stakeRaw = vm.envUint("NEW_STAKE_WEI");
        uint256 trancheRaw = vm.envUint("NEW_MAX_TRANCHE_WEI");
        uint256 dailyRaw = vm.envUint("NEW_DAILY_TRANCHE_CAP_WEI");
        // Check the narrowing rather than assuming it — a value above 2^128
        // would truncate to an unrelated stake, and this script exists to stop
        // wrong stakes reaching chain.
        require(stakeRaw <= type(uint128).max, "NEW_STAKE_WEI exceeds uint128");
        require(trancheRaw <= type(uint128).max, "NEW_MAX_TRANCHE_WEI exceeds uint128");
        require(dailyRaw <= type(uint128).max, "NEW_DAILY_TRANCHE_CAP_WEI exceeds uint128");
        // casting to 'uint128' is safe because of the requires above.
        // forge-lint: disable-next-line(unsafe-typecast)
        uint128 newStake = uint128(stakeRaw);
        // forge-lint: disable-next-line(unsafe-typecast)
        uint128 newTranche = uint128(trancheRaw);
        // forge-lint: disable-next-line(unsafe-typecast)
        uint128 newDaily = uint128(dailyRaw);

        CrossChainGame game = CrossChainGame(payable(gameAddr));

        // Read the live config so everything except the three stake-related
        // fields round-trips unchanged.
        address operatorSigner = game.operatorSigner();
        address treasury = game.treasury();
        uint16 treasurySplitBps = game.treasurySplitBps();
        uint32 maxClaimWindowSecs = game.maxClaimWindowSecs();
        uint32 skewMarginSecs = game.skewMarginSecs();
        uint128 oldStake = game.stakeWei();

        require(
            newStake != oldStake || newTranche != game.maxTrancheWei() || newDaily != game.dailyTrancheCapWei(),
            "already at target"
        );

        console2.log("CrossChainGame        ", gameAddr);
        console2.log("stakeWei  before      ", oldStake);
        console2.log("stakeWei  after       ", newStake);
        console2.log("maxTranche after      ", newTranche);
        console2.log("dailyCap  after       ", newDaily);

        vm.startBroadcast();
        game.setConfig(
            operatorSigner,
            treasury,
            treasurySplitBps,
            newStake,
            newTranche,
            newDaily,
            maxClaimWindowSecs,
            skewMarginSecs
        );
        vm.stopBroadcast();

        // A broadcast reporting success is not proof the value moved.
        require(game.stakeWei() == newStake, "stakeWei did not take effect");
        require(game.maxTrancheWei() == newTranche, "maxTrancheWei did not take effect");
        console2.log("confirmed stakeWei    ", game.stakeWei());
    }
}
