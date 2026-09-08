/**
 * Minimal CrossChainGame ABI for client-side EVM-leg calls (viem
 * encodeFunctionData). Vendored from the Foundry artifact
 * (swarm-tips-repo/evm/out/CrossChainGame.sol/CrossChainGame.json) — only the
 * functions the cross-chain match UX needs: fund (createMatch), refund
 * (refundTimeout/refundNoCert), and the read views (stakeWei, CHAIN_TAG,
 * matches). Settle is permissionless and operator/harness-driven, so it is not
 * vendored here. Regenerate if the contract ABI changes.
 */
export const CROSS_CHAIN_GAME_ABI = [
  {
    "type": "function",
    "name": "CHAIN_TAG",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "bytes32",
        "internalType": "bytes32"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "createMatch",
    "inputs": [
      {
        "name": "matchId",
        "type": "bytes32",
        "internalType": "bytes32"
      },
      {
        "name": "sessionKey",
        "type": "address",
        "internalType": "address"
      },
      {
        "name": "counterSessionKey",
        "type": "address",
        "internalType": "address"
      },
      {
        "name": "playerIsP1",
        "type": "bool",
        "internalType": "bool"
      },
      {
        "name": "fundDeadline",
        "type": "uint64",
        "internalType": "uint64"
      },
      {
        "name": "matchDeadline",
        "type": "uint64",
        "internalType": "uint64"
      },
      {
        "name": "operatorSig",
        "type": "bytes",
        "internalType": "bytes"
      }
    ],
    "outputs": [],
    "stateMutability": "payable"
  },
  {
    "type": "function",
    "name": "matches",
    "inputs": [
      {
        "name": "",
        "type": "bytes32",
        "internalType": "bytes32"
      }
    ],
    "outputs": [
      {
        "name": "status",
        "type": "uint8",
        "internalType": "enum CrossChainGame.Status"
      },
      {
        "name": "player",
        "type": "address",
        "internalType": "address"
      },
      {
        "name": "sessionKey",
        "type": "address",
        "internalType": "address"
      },
      {
        "name": "counterSessionKey",
        "type": "address",
        "internalType": "address"
      },
      {
        "name": "playerIsP1",
        "type": "bool",
        "internalType": "bool"
      },
      {
        "name": "localEquivocated",
        "type": "bool",
        "internalType": "bool"
      },
      {
        "name": "counterEquivocated",
        "type": "bool",
        "internalType": "bool"
      },
      {
        "name": "stakeWei",
        "type": "uint128",
        "internalType": "uint128"
      },
      {
        "name": "trancheWei",
        "type": "uint128",
        "internalType": "uint128"
      },
      {
        "name": "fundDeadline",
        "type": "uint64",
        "internalType": "uint64"
      },
      {
        "name": "matchDeadline",
        "type": "uint64",
        "internalType": "uint64"
      },
      {
        "name": "lockedAt",
        "type": "uint64",
        "internalType": "uint64"
      },
      {
        "name": "claimWindowEnd",
        "type": "uint64",
        "internalType": "uint64"
      },
      {
        "name": "matchLiveDigest",
        "type": "bytes32",
        "internalType": "bytes32"
      },
      {
        "name": "bestStepCount",
        "type": "uint8",
        "internalType": "uint8"
      },
      {
        "name": "bestOutcomeKind",
        "type": "uint8",
        "internalType": "uint8"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "refundNoCert",
    "inputs": [
      {
        "name": "matchId",
        "type": "bytes32",
        "internalType": "bytes32"
      }
    ],
    "outputs": [],
    "stateMutability": "nonpayable"
  },
  {
    "type": "function",
    "name": "refundTimeout",
    "inputs": [
      {
        "name": "matchId",
        "type": "bytes32",
        "internalType": "bytes32"
      }
    ],
    "outputs": [],
    "stateMutability": "nonpayable"
  },
  {
    "type": "function",
    "name": "stakeWei",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint128",
        "internalType": "uint128"
      }
    ],
    "stateMutability": "view"
  }
] as const;

/**
 * Minimal CoordinationGame ABI for the same-chain EVM-vs-EVM /play flow. Vendored
 * from swarm-tips-repo/evm/out/CoordinationGame.sol/CoordinationGame.json — only
 * the gameplay functions the browser builds + sends itself (commitGuess,
 * revealGuess). createGame/joinGame come pre-encoded from game-api (as
 * create_call/join_call), so they aren't needed for client-side encoding, but
 * are vendored for completeness/verification. Regenerate if the ABI changes.
 */
export const COORDINATION_GAME_ABI = [
  {
    "type": "function",
    "name": "createGame",
    "inputs": [
      { "name": "gameId", "type": "bytes32", "internalType": "bytes32" },
      { "name": "matchupCommitment", "type": "bytes32", "internalType": "bytes32" },
      { "name": "operatorSig", "type": "bytes", "internalType": "bytes" },
      { "name": "player", "type": "address", "internalType": "address" }
    ],
    "outputs": [],
    "stateMutability": "payable"
  },
  {
    "type": "function",
    "name": "joinGame",
    "inputs": [
      { "name": "gameId", "type": "bytes32", "internalType": "bytes32" },
      { "name": "player", "type": "address", "internalType": "address" }
    ],
    "outputs": [],
    "stateMutability": "payable"
  },
  {
    // Wallet-as-player: the wallet calls this ONCE to register its gas-only
    // session key AND fund it (msg.value = gas + stake buffer forwarded to the
    // sessionKey EOA) — the single popup that opens a session.
    "type": "function",
    "name": "openSession",
    "inputs": [
      { "name": "sessionKey", "type": "address", "internalType": "address" },
      { "name": "expiry", "type": "uint64", "internalType": "uint64" }
    ],
    "outputs": [],
    "stateMutability": "payable"
  },
  {
    // Opens the session AND escrows the stake in ONE transaction: `gasAmount`
    // is forwarded to the session EOA, the remainder is credited to
    // withdrawable[msg.sender] for createGame/joinGame to debit at value 0.
    // This is the entry point the browser uses; plain openSession above is kept
    // for grok-agent (own EOA, no session key) and any un-migrated client.
    "type": "function",
    "name": "openSessionAndDeposit",
    "inputs": [
      { "name": "sessionKey", "type": "address", "internalType": "address" },
      { "name": "expiry", "type": "uint64", "internalType": "uint64" },
      { "name": "gasAmount", "type": "uint256", "internalType": "uint256" }
    ],
    "outputs": [],
    "stateMutability": "payable"
  },
  {
    // Credit the caller's own escrow balance. Not used by the browser (which
    // uses openSessionAndDeposit), vendored so the ledger surface is complete.
    "type": "function",
    "name": "deposit",
    "inputs": [],
    "outputs": [],
    "stateMutability": "payable"
  },
  {
    // Permissionless push of `to`'s credited winnings to `to` — the session key
    // calls this post-resolve so the wallet is paid with no wallet popup.
    "type": "function",
    "name": "withdrawFor",
    "inputs": [{ "name": "to", "type": "address", "internalType": "address" }],
    "outputs": [],
    "stateMutability": "nonpayable"
  },
  {
    "type": "function",
    "name": "commitGuess",
    "inputs": [
      { "name": "gameId", "type": "bytes32", "internalType": "bytes32" },
      { "name": "commitment", "type": "bytes32", "internalType": "bytes32" }
    ],
    "outputs": [],
    "stateMutability": "nonpayable"
  },
  {
    "type": "function",
    "name": "revealGuess",
    "inputs": [
      { "name": "gameId", "type": "bytes32", "internalType": "bytes32" },
      { "name": "r", "type": "bytes32", "internalType": "bytes32" },
      { "name": "rMatchup", "type": "bytes32", "internalType": "bytes32" }
    ],
    "outputs": [],
    "stateMutability": "nonpayable"
  },
  {
    "type": "function",
    "name": "withdrawable",
    "inputs": [{ "name": "", "type": "address", "internalType": "address" }],
    "outputs": [{ "name": "", "type": "uint256", "internalType": "uint256" }],
    "stateMutability": "view"
  },
  {
    // The on-chain session registration for a wallet. `_actsFor` requires
    // `sessionKey == msg.sender && block.timestamp < expiry`, so this is the
    // only way a client can tell whether its session is still VALID as opposed
    // to merely funded. Its absence from this ABI is why setupEvmSessionIfNeeded
    // inferred registration from the session key's balance — and why an expired
    // session locked a wallet out with no way to recover.
    "type": "function",
    "name": "sessions",
    "inputs": [{ "name": "", "type": "address", "internalType": "address" }],
    "outputs": [
      { "name": "sessionKey", "type": "address", "internalType": "address" },
      { "name": "expiry", "type": "uint64", "internalType": "uint64" }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "withdraw",
    "inputs": [],
    "outputs": [],
    "stateMutability": "nonpayable"
  },
  {
    // Auto-getter for `mapping(bytes32 => Game) public games`. Read to learn
    // whether the matchup is already bound (matchupType != 255) so the reveal
    // knows if it is the FIRST revealer (bind with rMatchup) or the SECOND
    // (must pass the zero sentinel), and to compute the timeout deadline from
    // the stage anchor + window. Field order matches CoordinationGame.sol Game.
    "type": "function",
    "name": "games",
    "inputs": [{ "name": "", "type": "bytes32", "internalType": "bytes32" }],
    "outputs": [
      { "name": "status", "type": "uint8", "internalType": "uint8" },
      { "name": "player1", "type": "address", "internalType": "address" },
      { "name": "player2", "type": "address", "internalType": "address" },
      { "name": "p1Guess", "type": "uint8", "internalType": "uint8" },
      { "name": "p2Guess", "type": "uint8", "internalType": "uint8" },
      { "name": "firstCommitter", "type": "uint8", "internalType": "uint8" },
      { "name": "matchupType", "type": "uint8", "internalType": "uint8" },
      { "name": "stakeWei", "type": "uint128", "internalType": "uint128" },
      { "name": "matchupCommitment", "type": "bytes32", "internalType": "bytes32" },
      { "name": "p1Commit", "type": "bytes32", "internalType": "bytes32" },
      { "name": "p2Commit", "type": "bytes32", "internalType": "bytes32" },
      { "name": "activatedAt", "type": "uint64", "internalType": "uint64" },
      { "name": "firstCommitAt", "type": "uint64", "internalType": "uint64" },
      { "name": "bothCommitAt", "type": "uint64", "internalType": "uint64" },
      { "name": "createdAt", "type": "uint64", "internalType": "uint64" },
      { "name": "commitWindowSecs", "type": "uint32", "internalType": "uint32" },
      { "name": "revealWindowSecs", "type": "uint32", "internalType": "uint32" }
    ],
    "stateMutability": "view"
  },
  {
    // Permissionless timeout resolution: when the opponent stalls past the stage
    // window (Active/Committing → commitWindow, Revealing → revealWindow), anyone
    // can crank this to resolve the game so the honest player's stake isn't
    // stranded. Reverts (Elapsed check) if the window hasn't passed yet.
    "type": "function",
    "name": "resolveTimeout",
    "inputs": [{ "name": "gameId", "type": "bytes32", "internalType": "bytes32" }],
    "outputs": [],
    "stateMutability": "nonpayable"
  }
] as const;
