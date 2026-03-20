/**
 * Program IDL in camelCase format in order to be used in JS/TS.
 *
 * Note that this is only a type helper and is not the actual IDL. The original
 * IDL can be found at `target/idl/coordination.json`.
 */
export type Coordination = {
  "address": "2mqqXnhRtqEYUM9ycyL7mLjkCfjjutMXSfYWuXWxEJac",
  "metadata": {
    "name": "coordination",
    "version": "0.1.0",
    "spec": "0.1.0",
    "description": "Created with Anchor"
  },
  "instructions": [
    {
      "name": "claimReward",
      "discriminator": [
        149,
        95,
        181,
        242,
        94,
        90,
        158,
        162
      ],
      "accounts": [
        {
          "name": "tournament",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  116,
                  111,
                  117,
                  114,
                  110,
                  97,
                  109,
                  101,
                  110,
                  116
                ]
              },
              {
                "kind": "account",
                "path": "tournament.tournament_id",
                "account": "tournament"
              }
            ]
          }
        },
        {
          "name": "playerProfile",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  112,
                  108,
                  97,
                  121,
                  101,
                  114
                ]
              },
              {
                "kind": "account",
                "path": "tournament.tournament_id",
                "account": "tournament"
              },
              {
                "kind": "account",
                "path": "player"
              }
            ]
          }
        },
        {
          "name": "player",
          "writable": true,
          "signer": true
        }
      ],
      "args": []
    },
    {
      "name": "closeGame",
      "discriminator": [
        237,
        236,
        157,
        201,
        253,
        20,
        248,
        67
      ],
      "accounts": [
        {
          "name": "game",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  103,
                  97,
                  109,
                  101
                ]
              },
              {
                "kind": "account",
                "path": "game.game_id",
                "account": "game"
              }
            ]
          }
        },
        {
          "name": "caller",
          "writable": true,
          "signer": true
        }
      ],
      "args": []
    },
    {
      "name": "commitGuess",
      "discriminator": [
        116,
        86,
        218,
        54,
        77,
        153,
        60,
        230
      ],
      "accounts": [
        {
          "name": "game",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  103,
                  97,
                  109,
                  101
                ]
              },
              {
                "kind": "account",
                "path": "game.game_id",
                "account": "game"
              }
            ]
          }
        },
        {
          "name": "player",
          "signer": true
        }
      ],
      "args": [
        {
          "name": "commitment",
          "type": {
            "array": [
              "u8",
              32
            ]
          }
        },
        {
          "name": "proofA",
          "type": {
            "array": [
              "u8",
              64
            ]
          }
        },
        {
          "name": "proofB",
          "type": {
            "array": [
              "u8",
              128
            ]
          }
        },
        {
          "name": "proofC",
          "type": {
            "array": [
              "u8",
              64
            ]
          }
        }
      ]
    },
    {
      "name": "createGame",
      "discriminator": [
        124,
        69,
        75,
        66,
        184,
        220,
        72,
        206
      ],
      "accounts": [
        {
          "name": "game",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  103,
                  97,
                  109,
                  101
                ]
              },
              {
                "kind": "account",
                "path": "game_counter.count",
                "account": "gameCounter"
              }
            ]
          }
        },
        {
          "name": "gameCounter",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  103,
                  97,
                  109,
                  101,
                  95,
                  99,
                  111,
                  117,
                  110,
                  116,
                  101,
                  114
                ]
              }
            ]
          }
        },
        {
          "name": "playerProfile",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  112,
                  108,
                  97,
                  121,
                  101,
                  114
                ]
              },
              {
                "kind": "account",
                "path": "tournament.tournament_id",
                "account": "tournament"
              },
              {
                "kind": "account",
                "path": "player"
              }
            ]
          }
        },
        {
          "name": "tournament"
        },
        {
          "name": "player",
          "writable": true,
          "signer": true
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        }
      ],
      "args": [
        {
          "name": "stakeLamports",
          "type": "u64"
        }
      ]
    },
    {
      "name": "createTournament",
      "discriminator": [
        158,
        137,
        233,
        231,
        73,
        132,
        191,
        68
      ],
      "accounts": [
        {
          "name": "tournament",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  116,
                  111,
                  117,
                  114,
                  110,
                  97,
                  109,
                  101,
                  110,
                  116
                ]
              },
              {
                "kind": "arg",
                "path": "tournamentId"
              }
            ]
          }
        },
        {
          "name": "authority",
          "writable": true,
          "signer": true
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        }
      ],
      "args": [
        {
          "name": "tournamentId",
          "type": "u64"
        },
        {
          "name": "startTime",
          "type": "i64"
        },
        {
          "name": "endTime",
          "type": "i64"
        }
      ]
    },
    {
      "name": "finalizeTournament",
      "discriminator": [
        205,
        30,
        149,
        11,
        108,
        122,
        120,
        11
      ],
      "accounts": [
        {
          "name": "tournament",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  116,
                  111,
                  117,
                  114,
                  110,
                  97,
                  109,
                  101,
                  110,
                  116
                ]
              },
              {
                "kind": "account",
                "path": "tournament.tournament_id",
                "account": "tournament"
              }
            ]
          }
        },
        {
          "name": "caller",
          "signer": true
        }
      ],
      "args": []
    },
    {
      "name": "initialize",
      "discriminator": [
        175,
        175,
        109,
        31,
        13,
        152,
        155,
        237
      ],
      "accounts": [
        {
          "name": "gameCounter",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  103,
                  97,
                  109,
                  101,
                  95,
                  99,
                  111,
                  117,
                  110,
                  116,
                  101,
                  114
                ]
              }
            ]
          }
        },
        {
          "name": "authority",
          "writable": true,
          "signer": true
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        }
      ],
      "args": []
    },
    {
      "name": "joinGame",
      "discriminator": [
        107,
        112,
        18,
        38,
        56,
        173,
        60,
        128
      ],
      "accounts": [
        {
          "name": "game",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  103,
                  97,
                  109,
                  101
                ]
              },
              {
                "kind": "account",
                "path": "game.game_id",
                "account": "game"
              }
            ]
          }
        },
        {
          "name": "playerProfile",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  112,
                  108,
                  97,
                  121,
                  101,
                  114
                ]
              },
              {
                "kind": "account",
                "path": "tournament.tournament_id",
                "account": "tournament"
              },
              {
                "kind": "account",
                "path": "player"
              }
            ]
          }
        },
        {
          "name": "tournament"
        },
        {
          "name": "player",
          "writable": true,
          "signer": true
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        }
      ],
      "args": []
    },
    {
      "name": "resolveTimeout",
      "discriminator": [
        149,
        55,
        89,
        144,
        121,
        143,
        48,
        210
      ],
      "accounts": [
        {
          "name": "game",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  103,
                  97,
                  109,
                  101
                ]
              },
              {
                "kind": "account",
                "path": "game.game_id",
                "account": "game"
              }
            ]
          }
        },
        {
          "name": "p1Profile",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  112,
                  108,
                  97,
                  121,
                  101,
                  114
                ]
              },
              {
                "kind": "account",
                "path": "tournament.tournament_id",
                "account": "tournament"
              },
              {
                "kind": "account",
                "path": "game.player_one",
                "account": "game"
              }
            ]
          }
        },
        {
          "name": "p2Profile",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  112,
                  108,
                  97,
                  121,
                  101,
                  114
                ]
              },
              {
                "kind": "account",
                "path": "tournament.tournament_id",
                "account": "tournament"
              },
              {
                "kind": "account",
                "path": "game.player_two",
                "account": "game"
              }
            ]
          }
        },
        {
          "name": "tournament",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  116,
                  111,
                  117,
                  114,
                  110,
                  97,
                  109,
                  101,
                  110,
                  116
                ]
              },
              {
                "kind": "account",
                "path": "game.tournament_id",
                "account": "game"
              }
            ]
          }
        },
        {
          "name": "playerOneWallet",
          "writable": true
        },
        {
          "name": "playerTwoWallet",
          "writable": true
        },
        {
          "name": "caller",
          "docs": [
            "Caller receives no prize but pays the transaction fee; rent reclaim via close_game"
          ],
          "signer": true
        }
      ],
      "args": []
    },
    {
      "name": "revealGuess",
      "discriminator": [
        209,
        228,
        167,
        227,
        138,
        208,
        149,
        57
      ],
      "accounts": [
        {
          "name": "game",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  103,
                  97,
                  109,
                  101
                ]
              },
              {
                "kind": "account",
                "path": "game.game_id",
                "account": "game"
              }
            ]
          }
        },
        {
          "name": "player",
          "signer": true
        },
        {
          "name": "p1Profile",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  112,
                  108,
                  97,
                  121,
                  101,
                  114
                ]
              },
              {
                "kind": "account",
                "path": "tournament.tournament_id",
                "account": "tournament"
              },
              {
                "kind": "account",
                "path": "game.player_one",
                "account": "game"
              }
            ]
          }
        },
        {
          "name": "p2Profile",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  112,
                  108,
                  97,
                  121,
                  101,
                  114
                ]
              },
              {
                "kind": "account",
                "path": "tournament.tournament_id",
                "account": "tournament"
              },
              {
                "kind": "account",
                "path": "game.player_two",
                "account": "game"
              }
            ]
          }
        },
        {
          "name": "tournament",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  116,
                  111,
                  117,
                  114,
                  110,
                  97,
                  109,
                  101,
                  110,
                  116
                ]
              },
              {
                "kind": "account",
                "path": "game.tournament_id",
                "account": "game"
              }
            ]
          }
        },
        {
          "name": "playerOneWallet",
          "writable": true
        },
        {
          "name": "playerTwoWallet",
          "writable": true
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        }
      ],
      "args": [
        {
          "name": "guess",
          "type": "u8"
        },
        {
          "name": "salt",
          "type": {
            "array": [
              "u8",
              32
            ]
          }
        }
      ]
    }
  ],
  "accounts": [
    {
      "name": "game",
      "discriminator": [
        27,
        90,
        166,
        125,
        74,
        100,
        121,
        18
      ]
    },
    {
      "name": "gameCounter",
      "discriminator": [
        117,
        67,
        148,
        185,
        138,
        194,
        249,
        87
      ]
    },
    {
      "name": "playerProfile",
      "discriminator": [
        82,
        226,
        99,
        87,
        164,
        130,
        181,
        80
      ]
    },
    {
      "name": "tournament",
      "discriminator": [
        175,
        139,
        119,
        242,
        115,
        194,
        57,
        92
      ]
    }
  ],
  "events": [
    {
      "name": "gameCreated",
      "discriminator": [
        218,
        25,
        150,
        94,
        177,
        112,
        96,
        2
      ]
    },
    {
      "name": "gameResolved",
      "discriminator": [
        145,
        78,
        127,
        55,
        138,
        225,
        142,
        124
      ]
    },
    {
      "name": "gameStarted",
      "discriminator": [
        222,
        247,
        78,
        255,
        61,
        184,
        156,
        41
      ]
    },
    {
      "name": "guessCommitted",
      "discriminator": [
        174,
        25,
        105,
        114,
        240,
        123,
        51,
        187
      ]
    },
    {
      "name": "guessRevealed",
      "discriminator": [
        128,
        133,
        199,
        174,
        53,
        25,
        192,
        97
      ]
    },
    {
      "name": "rewardClaimed",
      "discriminator": [
        49,
        28,
        87,
        84,
        158,
        48,
        229,
        175
      ]
    },
    {
      "name": "timeoutSlash",
      "discriminator": [
        92,
        134,
        243,
        150,
        210,
        236,
        191,
        12
      ]
    },
    {
      "name": "tournamentCreated",
      "discriminator": [
        102,
        32,
        240,
        45,
        52,
        64,
        97,
        0
      ]
    },
    {
      "name": "tournamentFinalized",
      "discriminator": [
        34,
        61,
        238,
        26,
        68,
        54,
        253,
        144
      ]
    }
  ],
  "errors": [
    {
      "code": 6000,
      "name": "invalidGameState",
      "msg": "Invalid game state for this instruction"
    },
    {
      "code": 6001,
      "name": "invalidStateTransition",
      "msg": "Invalid state transition"
    },
    {
      "code": 6002,
      "name": "notAParticipant",
      "msg": "Player is not a participant in this game"
    },
    {
      "code": 6003,
      "name": "alreadyCommitted",
      "msg": "Player has already committed a guess"
    },
    {
      "code": 6004,
      "name": "alreadyRevealed",
      "msg": "Player has already revealed a guess"
    },
    {
      "code": 6005,
      "name": "alreadyClaimed",
      "msg": "Player has already claimed their reward"
    },
    {
      "code": 6006,
      "name": "cannotJoinOwnGame",
      "msg": "Cannot join your own game"
    },
    {
      "code": 6007,
      "name": "stakeMismatch",
      "msg": "Stake amount does not match the game's required stake"
    },
    {
      "code": 6008,
      "name": "commitmentMismatch",
      "msg": "Commitment hash mismatch on reveal"
    },
    {
      "code": 6009,
      "name": "invalidGuessValue",
      "msg": "Revealed guess is not a valid value (must be 0 or 1)"
    },
    {
      "code": 6010,
      "name": "invalidRangeProof",
      "msg": "ZK range proof verification failed"
    },
    {
      "code": 6011,
      "name": "timeoutNotElapsed",
      "msg": "Timeout has not elapsed yet"
    },
    {
      "code": 6012,
      "name": "tournamentNotEnded",
      "msg": "Tournament has not ended yet"
    },
    {
      "code": 6013,
      "name": "tournamentNotFinalized",
      "msg": "Tournament must be finalized before rewards can be claimed"
    },
    {
      "code": 6014,
      "name": "emptyPrizePool",
      "msg": "Tournament prize pool is empty"
    },
    {
      "code": 6015,
      "name": "outsideTournamentWindow",
      "msg": "Game is outside the tournament window"
    },
    {
      "code": 6016,
      "name": "profileTournamentMismatch",
      "msg": "Player profile does not belong to this tournament"
    },
    {
      "code": 6017,
      "name": "belowMinimumGames",
      "msg": "Player has not played enough games to claim a reward (minimum 5)"
    },
    {
      "code": 6018,
      "name": "arithmeticOverflow",
      "msg": "Arithmetic overflow"
    },
    {
      "code": 6019,
      "name": "tooManyAccounts",
      "msg": "Too many accounts passed to finalize_tournament (maximum 30)"
    }
  ],
  "types": [
    {
      "name": "game",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "gameId",
            "type": "u64"
          },
          {
            "name": "tournamentId",
            "type": "u64"
          },
          {
            "name": "playerOne",
            "type": "pubkey"
          },
          {
            "name": "playerTwo",
            "type": "pubkey"
          },
          {
            "name": "state",
            "type": {
              "defined": {
                "name": "gameState"
              }
            }
          },
          {
            "name": "stakeLamports",
            "type": "u64"
          },
          {
            "name": "p1Commit",
            "type": {
              "array": [
                "u8",
                32
              ]
            }
          },
          {
            "name": "p2Commit",
            "type": {
              "array": [
                "u8",
                32
              ]
            }
          },
          {
            "name": "p1Guess",
            "type": "u8"
          },
          {
            "name": "p2Guess",
            "type": "u8"
          },
          {
            "name": "firstCommitter",
            "type": "u8"
          },
          {
            "name": "p1CommitSlot",
            "type": "u64"
          },
          {
            "name": "p2CommitSlot",
            "type": "u64"
          },
          {
            "name": "commitTimeoutSlots",
            "type": "u64"
          },
          {
            "name": "createdAt",
            "type": "i64"
          },
          {
            "name": "resolvedAt",
            "type": "i64"
          },
          {
            "name": "bump",
            "type": "u8"
          }
        ]
      }
    },
    {
      "name": "gameCounter",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "count",
            "type": "u64"
          },
          {
            "name": "bump",
            "type": "u8"
          }
        ]
      }
    },
    {
      "name": "gameCreated",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "gameId",
            "type": "u64"
          },
          {
            "name": "tournamentId",
            "type": "u64"
          },
          {
            "name": "playerOne",
            "type": "pubkey"
          },
          {
            "name": "stakeLamports",
            "type": "u64"
          }
        ]
      }
    },
    {
      "name": "gameResolved",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "gameId",
            "type": "u64"
          },
          {
            "name": "p1Guess",
            "type": "u8"
          },
          {
            "name": "p2Guess",
            "type": "u8"
          },
          {
            "name": "p1Return",
            "type": "u64"
          },
          {
            "name": "p2Return",
            "type": "u64"
          },
          {
            "name": "tournamentGain",
            "type": "u64"
          }
        ]
      }
    },
    {
      "name": "gameStarted",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "gameId",
            "type": "u64"
          },
          {
            "name": "tournamentId",
            "type": "u64"
          },
          {
            "name": "playerOne",
            "type": "pubkey"
          },
          {
            "name": "playerTwo",
            "type": "pubkey"
          }
        ]
      }
    },
    {
      "name": "gameState",
      "type": {
        "kind": "enum",
        "variants": [
          {
            "name": "pending"
          },
          {
            "name": "active"
          },
          {
            "name": "committing"
          },
          {
            "name": "revealing"
          },
          {
            "name": "resolved"
          }
        ]
      }
    },
    {
      "name": "guessCommitted",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "gameId",
            "type": "u64"
          },
          {
            "name": "player",
            "type": "pubkey"
          },
          {
            "name": "commitSlot",
            "type": "u64"
          }
        ]
      }
    },
    {
      "name": "guessRevealed",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "gameId",
            "type": "u64"
          },
          {
            "name": "player",
            "type": "pubkey"
          }
        ]
      }
    },
    {
      "name": "playerProfile",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "wallet",
            "type": "pubkey"
          },
          {
            "name": "tournamentId",
            "type": "u64"
          },
          {
            "name": "wins",
            "type": "u64"
          },
          {
            "name": "totalGames",
            "type": "u64"
          },
          {
            "name": "score",
            "type": "u64"
          },
          {
            "name": "claimed",
            "type": "bool"
          },
          {
            "name": "bump",
            "type": "u8"
          }
        ]
      }
    },
    {
      "name": "rewardClaimed",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "tournamentId",
            "type": "u64"
          },
          {
            "name": "player",
            "type": "pubkey"
          },
          {
            "name": "amount",
            "type": "u64"
          }
        ]
      }
    },
    {
      "name": "timeoutSlash",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "gameId",
            "type": "u64"
          },
          {
            "name": "slashedPlayer",
            "type": "pubkey"
          },
          {
            "name": "slashAmount",
            "type": "u64"
          }
        ]
      }
    },
    {
      "name": "tournament",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "tournamentId",
            "type": "u64"
          },
          {
            "name": "authority",
            "type": "pubkey"
          },
          {
            "name": "startTime",
            "type": "i64"
          },
          {
            "name": "endTime",
            "type": "i64"
          },
          {
            "name": "prizeLamports",
            "type": "u64"
          },
          {
            "name": "gameCount",
            "type": "u64"
          },
          {
            "name": "finalized",
            "type": "bool"
          },
          {
            "name": "prizeSnapshot",
            "type": "u64"
          },
          {
            "name": "totalScoreSnapshot",
            "type": "u64"
          },
          {
            "name": "bump",
            "type": "u8"
          }
        ]
      }
    },
    {
      "name": "tournamentCreated",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "tournamentId",
            "type": "u64"
          },
          {
            "name": "startTime",
            "type": "i64"
          },
          {
            "name": "endTime",
            "type": "i64"
          }
        ]
      }
    },
    {
      "name": "tournamentFinalized",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "tournamentId",
            "type": "u64"
          },
          {
            "name": "prizeSnapshot",
            "type": "u64"
          },
          {
            "name": "totalScoreSnapshot",
            "type": "u64"
          }
        ]
      }
    }
  ]
};
