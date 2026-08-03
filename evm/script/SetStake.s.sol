// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Script} from "forge-std/Script.sol";
import {console2} from "forge-std/console2.sol";
import {CoordinationGame} from "../src/CoordinationGame.sol";

/// @notice Re-peg a deployed CoordinationGame's `stakeWei` without redeploying.
///
///         This exists because there was no path to change a stake except an
///         ad-hoc owner transaction, and that is exactly how three EVM anchors
///         ended up pegged at three different ETH prices ($1,562 / $3,000 /
///         $1,600) with the same product costing 5x more on one chain than
///         another. A re-peg needs to be reviewable, repeatable, and runnable
///         from CI like every other production change.
///
///         `setConfig` rewrites the FULL config tuple, so every other field is
///         read back off-chain and passed through unchanged. Only `stakeWei`
///         and `maxTranche`-adjacent values should differ, and the script prints
///         a before/after so the diff is auditable in the run log.
///
///         COHERENCE: the registry (crates/chain-registry) and the deploy
///         workflow must carry the SAME number. `createGame` reverts BadStake
///         unless `msg.value == stakeWei`, and the cross-chain client reads
///         `stakeWei()` and refuses to send when it disagrees with the relay's
///         quote — so a chain whose stake differs from the registry is not
///         merely inconsistent, it is unplayable. Run
///         `tests/e2e/scripts/check-stake-parity.mjs` after this to confirm.
///
/// Required env:
///   COORDINATION_GAME  deployed CoordinationGame address
///   NEW_STAKE_WEI      the new per-game stake, in wei
///
/// Run (via CI; owner key supplied by the workflow):
///   forge script script/SetStake.s.sol --rpc-url base \
///     --private-key "$OWNER_PK" --broadcast
contract SetStakeScript is Script {
    function run() external {
        address gameAddr = vm.envAddress("COORDINATION_GAME");
        uint256 newStakeRaw = vm.envUint("NEW_STAKE_WEI");
        // Check the narrowing rather than assuming it: a NEW_STAKE_WEI above
        // 2^128 would silently truncate to an unrelated (possibly tiny) stake,
        // and this script exists precisely to stop wrong stakes reaching chain.
        require(newStakeRaw <= type(uint128).max, "NEW_STAKE_WEI exceeds uint128");
        // casting to 'uint128' is safe because the require above rejects any
        // value above type(uint128).max, so no truncation is possible here.
        // forge-lint: disable-next-line(unsafe-typecast)
        uint128 newStake = uint128(newStakeRaw);
        CoordinationGame game = CoordinationGame(payable(gameAddr));

        // Read the live config so every field except the stake round-trips
        // unchanged. Re-deriving these from env would reintroduce exactly the
        // copy-drift this script exists to prevent.
        address operatorSigner = game.operatorSigner();
        address treasury = game.treasury();
        uint16 treasurySplitBps = game.treasurySplitBps();
        uint128 oldStake = game.stakeWei();
        uint32 commitTimeout = game.commitTimeoutSecs();
        uint32 revealTimeout = game.revealTimeoutSecs();

        require(newStake != oldStake, "stake already at target");

        console2.log("CoordinationGame      ", gameAddr);
        console2.log("stakeWei  before      ", oldStake);
        console2.log("stakeWei  after       ", newStake);
        console2.log("operatorSigner (kept) ", operatorSigner);
        console2.log("treasury       (kept) ", treasury);
        console2.log("treasurySplitBps(kept)", treasurySplitBps);

        vm.startBroadcast();
        game.setConfig(
            operatorSigner,
            treasury,
            treasurySplitBps,
            newStake,
            commitTimeout,
            revealTimeout
        );
        vm.stopBroadcast();

        // A broadcast that "succeeded" is not proof the value moved.
        uint128 confirmed = game.stakeWei();
        require(confirmed == newStake, "setConfig did not take effect");
        console2.log("confirmed stakeWei    ", confirmed);
    }
}
