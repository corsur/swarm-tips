/**
 * Program IDL in camelCase format in order to be used in JS/TS.
 *
 * Note that this is only a type helper and is not the actual IDL. The original
 * IDL can be found at `target/idl/coordination_game.json`.
 */
export type CoordinationGame = {
  address: "2qqVk7kUqffnahiJpcQJCsSd8ErbEUgKTgCn1zYsw64P";
  metadata: {
    name: "coordinationGame";
    version: "0.1.0";
    spec: "0.1.0";
    description: "Created with Anchor";
  };
  instructions: [
    {
      name: "claimReward";
      discriminator: [149, 95, 181, 242, 94, 90, 158, 162];
      accounts: [
        {
          name: "tournament";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [116, 111, 117, 114, 110, 97, 109, 101, 110, 116];
              },
              {
                kind: "account";
                path: "tournament.tournament_id";
                account: "tournament";
              }
            ];
          };
        },
        {
          name: "playerProfile";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [112, 108, 97, 121, 101, 114];
              },
              {
                kind: "account";
                path: "tournament.tournament_id";
                account: "tournament";
              },
              {
                kind: "account";
                path: "player";
              }
            ];
          };
        },
        {
          name: "player";
          writable: true;
          signer: true;
        }
      ];
      args: [
        {
          name: "amount";
          type: "u64";
        },
        {
          name: "proof";
          type: {
            vec: {
              array: ["u8", 32];
            };
          };
        }
      ];
    },
    {
      name: "closeGame";
      discriminator: [237, 236, 157, 201, 253, 20, 248, 67];
      accounts: [
        {
          name: "game";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [103, 97, 109, 101];
              },
              {
                kind: "account";
                path: "game.game_id";
                account: "game";
              }
            ];
          };
        },
        {
          name: "caller";
          writable: true;
          signer: true;
        }
      ];
      args: [];
    },
    {
      name: "closePlayerSession";
      discriminator: [71, 20, 190, 152, 125, 164, 158, 29];
      accounts: [
        {
          name: "sessionAuthority";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [
                  103,
                  97,
                  109,
                  101,
                  95,
                  115,
                  101,
                  115,
                  115,
                  105,
                  111,
                  110
                ];
              },
              {
                kind: "account";
                path: "player";
              },
              {
                kind: "account";
                path: "sessionKey";
              }
            ];
          };
        },
        {
          name: "player";
          writable: true;
          signer: true;
        },
        {
          name: "sessionKey";
          docs: ["`session_authority.session_key` constraint in the handler."];
        }
      ];
      args: [];
    },
    {
      name: "closeSessionByDelegate";
      discriminator: [84, 16, 164, 152, 197, 147, 185, 53];
      accounts: [
        {
          name: "sessionAuthority";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [
                  103,
                  97,
                  109,
                  101,
                  95,
                  115,
                  101,
                  115,
                  115,
                  105,
                  111,
                  110
                ];
              },
              {
                kind: "account";
                path: "session_authority.player";
                account: "sessionAuthority";
              },
              {
                kind: "account";
                path: "sessionSigner";
              }
            ];
          };
        },
        {
          name: "sessionSigner";
          writable: true;
          signer: true;
        }
      ];
      args: [];
    },
    {
      name: "closeXmatch";
      discriminator: [89, 82, 105, 191, 208, 158, 183, 160];
      accounts: [
        {
          name: "xmatch";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [120, 109, 97, 116, 99, 104];
              },
              {
                kind: "account";
                path: "xmatch.match_id";
                account: "xChainMatch";
              }
            ];
          };
        },
        {
          name: "player";
          writable: true;
        }
      ];
      args: [];
    },
    {
      name: "commitGuess";
      discriminator: [116, 86, 218, 54, 77, 153, 60, 230];
      accounts: [
        {
          name: "game";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [103, 97, 109, 101];
              },
              {
                kind: "account";
                path: "game.game_id";
                account: "game";
              }
            ];
          };
        },
        {
          name: "player";
          signer: true;
        }
      ];
      args: [
        {
          name: "commitment";
          type: {
            array: ["u8", 32];
          };
        }
      ];
    },
    {
      name: "commitGuessSession";
      discriminator: [250, 149, 250, 122, 14, 69, 157, 127];
      accounts: [
        {
          name: "game";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [103, 97, 109, 101];
              },
              {
                kind: "account";
                path: "game.game_id";
                account: "game";
              }
            ];
          };
        },
        {
          name: "player";
          docs: [
            "Verified against session_authority.player and game participants in the handler."
          ];
        },
        {
          name: "sessionAuthority";
          pda: {
            seeds: [
              {
                kind: "const";
                value: [
                  103,
                  97,
                  109,
                  101,
                  95,
                  115,
                  101,
                  115,
                  115,
                  105,
                  111,
                  110
                ];
              },
              {
                kind: "account";
                path: "player";
              },
              {
                kind: "account";
                path: "sessionSigner";
              }
            ];
          };
        },
        {
          name: "sessionSigner";
          signer: true;
        }
      ];
      args: [
        {
          name: "commitment";
          type: {
            array: ["u8", 32];
          };
        }
      ];
    },
    {
      name: "createGame";
      discriminator: [124, 69, 75, 66, 184, 220, 72, 206];
      accounts: [
        {
          name: "game";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [103, 97, 109, 101];
              },
              {
                kind: "account";
                path: "game_counter.count";
                account: "gameCounter";
              }
            ];
          };
        },
        {
          name: "gameCounter";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [
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
                ];
              }
            ];
          };
        },
        {
          name: "playerProfile";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [112, 108, 97, 121, 101, 114];
              },
              {
                kind: "account";
                path: "tournament.tournament_id";
                account: "tournament";
              },
              {
                kind: "account";
                path: "player";
              }
            ];
          };
        },
        {
          name: "escrow";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [101, 115, 99, 114, 111, 119];
              },
              {
                kind: "account";
                path: "tournament.tournament_id";
                account: "tournament";
              },
              {
                kind: "account";
                path: "player";
              }
            ];
          };
        },
        {
          name: "tournament";
          pda: {
            seeds: [
              {
                kind: "const";
                value: [116, 111, 117, 114, 110, 97, 109, 101, 110, 116];
              },
              {
                kind: "account";
                path: "tournament.tournament_id";
                account: "tournament";
              }
            ];
          };
        },
        {
          name: "globalConfig";
          pda: {
            seeds: [
              {
                kind: "const";
                value: [
                  103,
                  108,
                  111,
                  98,
                  97,
                  108,
                  95,
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ];
              }
            ];
          };
        },
        {
          name: "matchmaker";
          docs: [
            "Matchmaker co-signs to attest the commitment is legitimate.",
            "Verified against GlobalConfig.matchmaker. Does not pay gas."
          ];
          signer: true;
        },
        {
          name: "player";
          writable: true;
          signer: true;
        },
        {
          name: "systemProgram";
          address: "11111111111111111111111111111111";
        }
      ];
      args: [
        {
          name: "stakeLamports";
          type: "u64";
        },
        {
          name: "matchupCommitment";
          type: {
            array: ["u8", 32];
          };
        }
      ];
    },
    {
      name: "createGameSession";
      discriminator: [130, 34, 251, 80, 77, 159, 113, 224];
      accounts: [
        {
          name: "game";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [103, 97, 109, 101];
              },
              {
                kind: "account";
                path: "game_counter.count";
                account: "gameCounter";
              }
            ];
          };
        },
        {
          name: "gameCounter";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [
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
                ];
              }
            ];
          };
        },
        {
          name: "playerProfile";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [112, 108, 97, 121, 101, 114];
              },
              {
                kind: "account";
                path: "tournament.tournament_id";
                account: "tournament";
              },
              {
                kind: "account";
                path: "player";
              }
            ];
          };
        },
        {
          name: "escrow";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [101, 115, 99, 114, 111, 119];
              },
              {
                kind: "account";
                path: "tournament.tournament_id";
                account: "tournament";
              },
              {
                kind: "account";
                path: "player";
              }
            ];
          };
        },
        {
          name: "tournament";
          pda: {
            seeds: [
              {
                kind: "const";
                value: [116, 111, 117, 114, 110, 97, 109, 101, 110, 116];
              },
              {
                kind: "account";
                path: "tournament.tournament_id";
                account: "tournament";
              }
            ];
          };
        },
        {
          name: "globalConfig";
          pda: {
            seeds: [
              {
                kind: "const";
                value: [
                  103,
                  108,
                  111,
                  98,
                  97,
                  108,
                  95,
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ];
              }
            ];
          };
        },
        {
          name: "matchmaker";
          docs: [
            "Matchmaker co-signs to attest this commitment is legitimate. Verified",
            "against GlobalConfig.matchmaker. Does not pay gas."
          ];
          signer: true;
        },
        {
          name: "player";
          docs: ["Verified against session_authority.player in the handler."];
        },
        {
          name: "sessionAuthority";
          pda: {
            seeds: [
              {
                kind: "const";
                value: [
                  103,
                  97,
                  109,
                  101,
                  95,
                  115,
                  101,
                  115,
                  115,
                  105,
                  111,
                  110
                ];
              },
              {
                kind: "account";
                path: "player";
              },
              {
                kind: "account";
                path: "sessionSigner";
              }
            ];
          };
        },
        {
          name: "sessionSigner";
          writable: true;
          signer: true;
        },
        {
          name: "systemProgram";
          address: "11111111111111111111111111111111";
        }
      ];
      args: [
        {
          name: "stakeLamports";
          type: "u64";
        },
        {
          name: "matchupCommitment";
          type: {
            array: ["u8", 32];
          };
        }
      ];
    },
    {
      name: "createPlayerSession";
      discriminator: [246, 143, 125, 132, 223, 76, 77, 177];
      accounts: [
        {
          name: "sessionAuthority";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [
                  103,
                  97,
                  109,
                  101,
                  95,
                  115,
                  101,
                  115,
                  115,
                  105,
                  111,
                  110
                ];
              },
              {
                kind: "account";
                path: "player";
              },
              {
                kind: "account";
                path: "sessionKey";
              }
            ];
          };
        },
        {
          name: "player";
          writable: true;
          signer: true;
        },
        {
          name: "sessionKey";
          docs: [
            "The ephemeral session keypair's public key. Not required to sign here;",
            "the player is authorizing this key to act on their behalf.",
            "is read from this account; it is only used for its key in PDA derivation."
          ];
        },
        {
          name: "systemProgram";
          address: "11111111111111111111111111111111";
        }
      ];
      args: [];
    },
    {
      name: "createTournament";
      discriminator: [158, 137, 233, 231, 73, 132, 191, 68];
      accounts: [
        {
          name: "tournament";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [116, 111, 117, 114, 110, 97, 109, 101, 110, 116];
              },
              {
                kind: "arg";
                path: "tournamentId";
              }
            ];
          };
        },
        {
          name: "authority";
          writable: true;
          signer: true;
        },
        {
          name: "systemProgram";
          address: "11111111111111111111111111111111";
        }
      ];
      args: [
        {
          name: "tournamentId";
          type: "u64";
        },
        {
          name: "startTime";
          type: "i64";
        },
        {
          name: "endTime";
          type: "i64";
        }
      ];
    },
    {
      name: "createXmatch";
      discriminator: [123, 175, 124, 99, 101, 87, 143, 136];
      accounts: [
        {
          name: "xmatch";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [120, 109, 97, 116, 99, 104];
              },
              {
                kind: "arg";
                path: "matchId";
              }
            ];
          };
        },
        {
          name: "globalConfig";
          pda: {
            seeds: [
              {
                kind: "const";
                value: [
                  103,
                  108,
                  111,
                  98,
                  97,
                  108,
                  95,
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ];
              }
            ];
          };
        },
        {
          name: "matchmaker";
          docs: ["Matchmaker co-signs; does not pay gas."];
          signer: true;
        },
        {
          name: "player";
          writable: true;
          signer: true;
        },
        {
          name: "systemProgram";
          address: "11111111111111111111111111111111";
        }
      ];
      args: [
        {
          name: "matchId";
          type: {
            array: ["u8", 32];
          };
        },
        {
          name: "args";
          type: {
            defined: {
              name: "createXMatchArgs";
            };
          };
        }
      ];
    },
    {
      name: "depositStake";
      discriminator: [160, 167, 9, 220, 74, 243, 228, 43];
      accounts: [
        {
          name: "escrow";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [101, 115, 99, 114, 111, 119];
              },
              {
                kind: "account";
                path: "tournament.tournament_id";
                account: "tournament";
              },
              {
                kind: "account";
                path: "player";
              }
            ];
          };
        },
        {
          name: "tournament";
        },
        {
          name: "player";
          writable: true;
          signer: true;
        },
        {
          name: "systemProgram";
          address: "11111111111111111111111111111111";
        }
      ];
      args: [];
    },
    {
      name: "depositStakeSession";
      discriminator: [165, 195, 38, 185, 74, 161, 105, 28];
      accounts: [
        {
          name: "escrow";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [101, 115, 99, 114, 111, 119];
              },
              {
                kind: "account";
                path: "tournament.tournament_id";
                account: "tournament";
              },
              {
                kind: "account";
                path: "player";
              }
            ];
          };
        },
        {
          name: "tournament";
        },
        {
          name: "player";
          docs: ["Verified against session_authority.player in the handler."];
        },
        {
          name: "sessionAuthority";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [
                  103,
                  97,
                  109,
                  101,
                  95,
                  115,
                  101,
                  115,
                  115,
                  105,
                  111,
                  110
                ];
              },
              {
                kind: "account";
                path: "player";
              },
              {
                kind: "account";
                path: "sessionSigner";
              }
            ];
          };
        },
        {
          name: "sessionSigner";
          writable: true;
          signer: true;
        },
        {
          name: "systemProgram";
          address: "11111111111111111111111111111111";
        }
      ];
      args: [];
    },
    {
      name: "finalizeTournament";
      discriminator: [205, 30, 149, 11, 108, 122, 120, 11];
      accounts: [
        {
          name: "tournament";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [116, 111, 117, 114, 110, 97, 109, 101, 110, 116];
              },
              {
                kind: "account";
                path: "tournament.tournament_id";
                account: "tournament";
              }
            ];
          };
        },
        {
          name: "globalConfig";
          pda: {
            seeds: [
              {
                kind: "const";
                value: [
                  103,
                  108,
                  111,
                  98,
                  97,
                  108,
                  95,
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ];
              }
            ];
          };
        },
        {
          name: "authority";
          signer: true;
        }
      ];
      args: [
        {
          name: "merkleRoot";
          type: {
            array: ["u8", 32];
          };
        }
      ];
    },
    {
      name: "initialize";
      discriminator: [175, 175, 109, 31, 13, 152, 155, 237];
      accounts: [
        {
          name: "gameCounter";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [
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
                ];
              }
            ];
          };
        },
        {
          name: "authority";
          writable: true;
          signer: true;
        },
        {
          name: "systemProgram";
          address: "11111111111111111111111111111111";
        }
      ];
      args: [];
    },
    {
      name: "initializeConfig";
      discriminator: [208, 127, 21, 1, 194, 190, 196, 70];
      accounts: [
        {
          name: "globalConfig";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [
                  103,
                  108,
                  111,
                  98,
                  97,
                  108,
                  95,
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ];
              }
            ];
          };
        },
        {
          name: "authority";
          writable: true;
          signer: true;
        },
        {
          name: "matchmaker";
        },
        {
          name: "treasury";
        },
        {
          name: "systemProgram";
          address: "11111111111111111111111111111111";
        }
      ];
      args: [
        {
          name: "treasurySplitBps";
          type: "u16";
        }
      ];
    },
    {
      name: "initializeXpool";
      discriminator: [100, 223, 73, 78, 245, 69, 45, 66];
      accounts: [
        {
          name: "pool";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [120, 112, 111, 111, 108];
              }
            ];
          };
        },
        {
          name: "globalConfig";
          pda: {
            seeds: [
              {
                kind: "const";
                value: [
                  103,
                  108,
                  111,
                  98,
                  97,
                  108,
                  95,
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ];
              }
            ];
          };
        },
        {
          name: "authority";
          writable: true;
          signer: true;
        },
        {
          name: "systemProgram";
          address: "11111111111111111111111111111111";
        }
      ];
      args: [
        {
          name: "operator";
          type: "pubkey";
        },
        {
          name: "operatorSigner";
          type: {
            array: ["u8", 20];
          };
        },
        {
          name: "maxTrancheLamports";
          type: "u64";
        },
        {
          name: "maxClaimWindowSecs";
          type: "u32";
        },
        {
          name: "skewMarginSecs";
          type: "u32";
        }
      ];
    },
    {
      name: "joinGame";
      discriminator: [107, 112, 18, 38, 56, 173, 60, 128];
      accounts: [
        {
          name: "game";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [103, 97, 109, 101];
              },
              {
                kind: "account";
                path: "game.game_id";
                account: "game";
              }
            ];
          };
        },
        {
          name: "playerProfile";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [112, 108, 97, 121, 101, 114];
              },
              {
                kind: "account";
                path: "tournament.tournament_id";
                account: "tournament";
              },
              {
                kind: "account";
                path: "player";
              }
            ];
          };
        },
        {
          name: "escrow";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [101, 115, 99, 114, 111, 119];
              },
              {
                kind: "account";
                path: "tournament.tournament_id";
                account: "tournament";
              },
              {
                kind: "account";
                path: "player";
              }
            ];
          };
        },
        {
          name: "tournament";
          pda: {
            seeds: [
              {
                kind: "const";
                value: [116, 111, 117, 114, 110, 97, 109, 101, 110, 116];
              },
              {
                kind: "account";
                path: "game.tournament_id";
                account: "game";
              }
            ];
          };
        },
        {
          name: "globalConfig";
          pda: {
            seeds: [
              {
                kind: "const";
                value: [
                  103,
                  108,
                  111,
                  98,
                  97,
                  108,
                  95,
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ];
              }
            ];
          };
        },
        {
          name: "matchmaker";
          docs: [
            "Matchmaker co-signs to attest this is the paired opponent. Verified",
            "against GlobalConfig.matchmaker. Does not pay gas."
          ];
          signer: true;
        },
        {
          name: "player";
          writable: true;
          signer: true;
        },
        {
          name: "systemProgram";
          address: "11111111111111111111111111111111";
        }
      ];
      args: [];
    },
    {
      name: "joinGameSession";
      discriminator: [247, 94, 51, 88, 130, 132, 135, 152];
      accounts: [
        {
          name: "game";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [103, 97, 109, 101];
              },
              {
                kind: "account";
                path: "game.game_id";
                account: "game";
              }
            ];
          };
        },
        {
          name: "playerProfile";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [112, 108, 97, 121, 101, 114];
              },
              {
                kind: "account";
                path: "tournament.tournament_id";
                account: "tournament";
              },
              {
                kind: "account";
                path: "player";
              }
            ];
          };
        },
        {
          name: "escrow";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [101, 115, 99, 114, 111, 119];
              },
              {
                kind: "account";
                path: "tournament.tournament_id";
                account: "tournament";
              },
              {
                kind: "account";
                path: "player";
              }
            ];
          };
        },
        {
          name: "tournament";
          pda: {
            seeds: [
              {
                kind: "const";
                value: [116, 111, 117, 114, 110, 97, 109, 101, 110, 116];
              },
              {
                kind: "account";
                path: "game.tournament_id";
                account: "game";
              }
            ];
          };
        },
        {
          name: "globalConfig";
          pda: {
            seeds: [
              {
                kind: "const";
                value: [
                  103,
                  108,
                  111,
                  98,
                  97,
                  108,
                  95,
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ];
              }
            ];
          };
        },
        {
          name: "matchmaker";
          docs: [
            "Matchmaker co-signs to attest this is the paired opponent. Verified",
            "against GlobalConfig.matchmaker. Does not pay gas."
          ];
          signer: true;
        },
        {
          name: "player";
          docs: ["Verified against session_authority.player in the handler."];
        },
        {
          name: "sessionAuthority";
          pda: {
            seeds: [
              {
                kind: "const";
                value: [
                  103,
                  97,
                  109,
                  101,
                  95,
                  115,
                  101,
                  115,
                  115,
                  105,
                  111,
                  110
                ];
              },
              {
                kind: "account";
                path: "player";
              },
              {
                kind: "account";
                path: "sessionSigner";
              }
            ];
          };
        },
        {
          name: "sessionSigner";
          writable: true;
          signer: true;
        },
        {
          name: "systemProgram";
          address: "11111111111111111111111111111111";
        }
      ];
      args: [];
    },
    {
      name: "lockXtranche";
      discriminator: [251, 188, 147, 114, 170, 44, 196, 99];
      accounts: [
        {
          name: "xmatch";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [120, 109, 97, 116, 99, 104];
              },
              {
                kind: "arg";
                path: "cert.match_id";
              }
            ];
          };
        },
        {
          name: "pool";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [120, 112, 111, 111, 108];
              }
            ];
          };
        },
        {
          name: "cranker";
          docs: [
            "Permissionless fee payer — authorization is the operator signature on the",
            "cert, not this account. The caller pays the tx fee (gas external)."
          ];
          signer: true;
        }
      ];
      args: [
        {
          name: "cert";
          type: {
            defined: {
              name: "matchLiveCertArg";
            };
          };
        },
        {
          name: "operatorSig";
          type: {
            array: ["u8", 65];
          };
        }
      ];
    },
    {
      name: "migrateGlobalConfig";
      docs: [
        "One-shot realloc of the singleton GlobalConfig to carry `stake_lamports`.",
        "Run once per network; idempotent thereafter."
      ];
      discriminator: [207, 52, 247, 7, 1, 230, 228, 147];
      accounts: [
        {
          name: "globalConfig";
          docs: [
            "cannot be deserialized into the 115-byte struct. Owner, discriminator,",
            "PDA seeds and authority are all verified in the handler."
          ];
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [
                  103,
                  108,
                  111,
                  98,
                  97,
                  108,
                  95,
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ];
              }
            ];
          };
        },
        {
          name: "authority";
          docs: ["Must equal the authority recorded inside the account."];
          signer: true;
        }
      ];
      args: [];
    },
    {
      name: "openXclaim";
      discriminator: [15, 161, 204, 56, 9, 104, 194, 15];
      accounts: [
        {
          name: "xmatch";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [120, 109, 97, 116, 99, 104];
              },
              {
                kind: "arg";
                path: "cert.match_id";
              }
            ];
          };
        },
        {
          name: "pool";
          pda: {
            seeds: [
              {
                kind: "const";
                value: [120, 112, 111, 111, 108];
              }
            ];
          };
        }
      ];
      args: [
        {
          name: "cert";
          type: {
            defined: {
              name: "matchLiveCertArg";
            };
          };
        },
        {
          name: "cp";
          type: {
            defined: {
              name: "checkpointArg";
            };
          };
        },
        {
          name: "liveSigs";
          type: {
            array: [
              {
                array: ["u8", 65];
              },
              3
            ];
          };
        },
        {
          name: "cpSigs";
          type: {
            array: [
              {
                array: ["u8", 65];
              },
              2
            ];
          };
        }
      ];
    },
    {
      name: "refundPending";
      discriminator: [70, 207, 125, 172, 197, 218, 120, 112];
      accounts: [
        {
          name: "game";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [103, 97, 109, 101];
              },
              {
                kind: "account";
                path: "game.game_id";
                account: "game";
              }
            ];
          };
        },
        {
          name: "playerOneWallet";
          docs: [
            "the refunded stake regardless of who cranks the instruction."
          ];
          writable: true;
        },
        {
          name: "caller";
          docs: [
            "Permissionless caller: pays the tx fee, receives the reclaimed rent."
          ];
          writable: true;
          signer: true;
        }
      ];
      args: [];
    },
    {
      name: "refundXmatchNocert";
      discriminator: [151, 125, 113, 14, 42, 180, 182, 111];
      accounts: [
        {
          name: "xmatch";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [120, 109, 97, 116, 99, 104];
              },
              {
                kind: "account";
                path: "xmatch.match_id";
                account: "xChainMatch";
              }
            ];
          };
        },
        {
          name: "player";
          writable: true;
        }
      ];
      args: [];
    },
    {
      name: "refundXmatchTimeout";
      discriminator: [171, 45, 68, 234, 69, 88, 162, 242];
      accounts: [
        {
          name: "xmatch";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [120, 109, 97, 116, 99, 104];
              },
              {
                kind: "account";
                path: "xmatch.match_id";
                account: "xChainMatch";
              }
            ];
          };
        },
        {
          name: "pool";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [120, 112, 111, 111, 108];
              }
            ];
          };
        },
        {
          name: "player";
          writable: true;
        }
      ];
      args: [];
    },
    {
      name: "resolveTimeout";
      discriminator: [149, 55, 89, 144, 121, 143, 48, 210];
      accounts: [
        {
          name: "game";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [103, 97, 109, 101];
              },
              {
                kind: "account";
                path: "game.game_id";
                account: "game";
              }
            ];
          };
        },
        {
          name: "p1Profile";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [112, 108, 97, 121, 101, 114];
              },
              {
                kind: "account";
                path: "tournament.tournament_id";
                account: "tournament";
              },
              {
                kind: "account";
                path: "game.player_one";
                account: "game";
              }
            ];
          };
        },
        {
          name: "p2Profile";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [112, 108, 97, 121, 101, 114];
              },
              {
                kind: "account";
                path: "tournament.tournament_id";
                account: "tournament";
              },
              {
                kind: "account";
                path: "game.player_two";
                account: "game";
              }
            ];
          };
        },
        {
          name: "tournament";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [116, 111, 117, 114, 110, 97, 109, 101, 110, 116];
              },
              {
                kind: "account";
                path: "game.tournament_id";
                account: "game";
              }
            ];
          };
        },
        {
          name: "globalConfig";
          pda: {
            seeds: [
              {
                kind: "const";
                value: [
                  103,
                  108,
                  111,
                  98,
                  97,
                  108,
                  95,
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ];
              }
            ];
          };
        },
        {
          name: "treasury";
          writable: true;
        },
        {
          name: "playerOneWallet";
          writable: true;
        },
        {
          name: "playerTwoWallet";
          writable: true;
        },
        {
          name: "caller";
          docs: [
            "Caller receives no prize but pays the transaction fee; rent reclaim via close_game"
          ];
          signer: true;
        }
      ];
      args: [];
    },
    {
      name: "revealGuess";
      discriminator: [209, 228, 167, 227, 138, 208, 149, 57];
      accounts: [
        {
          name: "game";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [103, 97, 109, 101];
              },
              {
                kind: "account";
                path: "game.game_id";
                account: "game";
              }
            ];
          };
        },
        {
          name: "player";
          signer: true;
        },
        {
          name: "p1Profile";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [112, 108, 97, 121, 101, 114];
              },
              {
                kind: "account";
                path: "tournament.tournament_id";
                account: "tournament";
              },
              {
                kind: "account";
                path: "game.player_one";
                account: "game";
              }
            ];
          };
        },
        {
          name: "p2Profile";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [112, 108, 97, 121, 101, 114];
              },
              {
                kind: "account";
                path: "tournament.tournament_id";
                account: "tournament";
              },
              {
                kind: "account";
                path: "game.player_two";
                account: "game";
              }
            ];
          };
        },
        {
          name: "tournament";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [116, 111, 117, 114, 110, 97, 109, 101, 110, 116];
              },
              {
                kind: "account";
                path: "game.tournament_id";
                account: "game";
              }
            ];
          };
        },
        {
          name: "globalConfig";
          pda: {
            seeds: [
              {
                kind: "const";
                value: [
                  103,
                  108,
                  111,
                  98,
                  97,
                  108,
                  95,
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ];
              }
            ];
          };
        },
        {
          name: "treasury";
          writable: true;
        },
        {
          name: "playerOneWallet";
          writable: true;
        },
        {
          name: "playerTwoWallet";
          writable: true;
        }
      ];
      args: [
        {
          name: "r";
          type: {
            array: ["u8", 32];
          };
        },
        {
          name: "rMatchup";
          type: {
            option: {
              array: ["u8", 32];
            };
          };
        }
      ];
    },
    {
      name: "revealGuessSession";
      discriminator: [33, 255, 161, 50, 125, 126, 132, 197];
      accounts: [
        {
          name: "game";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [103, 97, 109, 101];
              },
              {
                kind: "account";
                path: "game.game_id";
                account: "game";
              }
            ];
          };
        },
        {
          name: "player";
          docs: [
            "Verified against session_authority.player and game participants in the handler."
          ];
        },
        {
          name: "sessionAuthority";
          pda: {
            seeds: [
              {
                kind: "const";
                value: [
                  103,
                  97,
                  109,
                  101,
                  95,
                  115,
                  101,
                  115,
                  115,
                  105,
                  111,
                  110
                ];
              },
              {
                kind: "account";
                path: "player";
              },
              {
                kind: "account";
                path: "sessionSigner";
              }
            ];
          };
        },
        {
          name: "sessionSigner";
          signer: true;
        },
        {
          name: "p1Profile";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [112, 108, 97, 121, 101, 114];
              },
              {
                kind: "account";
                path: "tournament.tournament_id";
                account: "tournament";
              },
              {
                kind: "account";
                path: "game.player_one";
                account: "game";
              }
            ];
          };
        },
        {
          name: "p2Profile";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [112, 108, 97, 121, 101, 114];
              },
              {
                kind: "account";
                path: "tournament.tournament_id";
                account: "tournament";
              },
              {
                kind: "account";
                path: "game.player_two";
                account: "game";
              }
            ];
          };
        },
        {
          name: "tournament";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [116, 111, 117, 114, 110, 97, 109, 101, 110, 116];
              },
              {
                kind: "account";
                path: "game.tournament_id";
                account: "game";
              }
            ];
          };
        },
        {
          name: "globalConfig";
          pda: {
            seeds: [
              {
                kind: "const";
                value: [
                  103,
                  108,
                  111,
                  98,
                  97,
                  108,
                  95,
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ];
              }
            ];
          };
        },
        {
          name: "treasury";
          writable: true;
        },
        {
          name: "playerOneWallet";
          writable: true;
        },
        {
          name: "playerTwoWallet";
          writable: true;
        }
      ];
      args: [
        {
          name: "r";
          type: {
            array: ["u8", 32];
          };
        },
        {
          name: "rMatchup";
          type: {
            option: {
              array: ["u8", 32];
            };
          };
        }
      ];
    },
    {
      name: "setStakeLamports";
      docs: [
        "Re-peg the per-game stake without a program upgrade — the Solana",
        "counterpart of the EVM contract's `setConfig`."
      ];
      discriminator: [76, 249, 165, 98, 24, 24, 181, 140];
      accounts: [
        {
          name: "globalConfig";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [
                  103,
                  108,
                  111,
                  98,
                  97,
                  108,
                  95,
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ];
              }
            ];
          };
        },
        {
          name: "authority";
          signer: true;
          relations: ["globalConfig"];
        }
      ];
      args: [
        {
          name: "newStake";
          type: "u64";
        }
      ];
    },
    {
      name: "settleXclaim";
      discriminator: [217, 87, 189, 58, 156, 153, 141, 144];
      accounts: [
        {
          name: "xmatch";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [120, 109, 97, 116, 99, 104];
              },
              {
                kind: "account";
                path: "xmatch.match_id";
                account: "xChainMatch";
              }
            ];
          };
        },
        {
          name: "pool";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [120, 112, 111, 111, 108];
              }
            ];
          };
        },
        {
          name: "tournament";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [116, 111, 117, 114, 110, 97, 109, 101, 110, 116];
              },
              {
                kind: "account";
                path: "xmatch.tournament_id";
                account: "xChainMatch";
              }
            ];
          };
        },
        {
          name: "playerProfile";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [112, 108, 97, 121, 101, 114];
              },
              {
                kind: "account";
                path: "xmatch.tournament_id";
                account: "xChainMatch";
              },
              {
                kind: "account";
                path: "xmatch.player";
                account: "xChainMatch";
              }
            ];
          };
        },
        {
          name: "globalConfig";
          pda: {
            seeds: [
              {
                kind: "const";
                value: [
                  103,
                  108,
                  111,
                  98,
                  97,
                  108,
                  95,
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ];
              }
            ];
          };
        },
        {
          name: "treasury";
          writable: true;
        },
        {
          name: "player";
          writable: true;
        },
        {
          name: "cranker";
          docs: [
            "Pays profile rent if the cross-chain player has none yet for this",
            "tournament. Settle stays permissionless — anyone can crank."
          ];
          writable: true;
          signer: true;
        },
        {
          name: "systemProgram";
          address: "11111111111111111111111111111111";
        }
      ];
      args: [];
    },
    {
      name: "settleXmatch";
      discriminator: [143, 202, 124, 191, 205, 192, 18, 207];
      accounts: [
        {
          name: "xmatch";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [120, 109, 97, 116, 99, 104];
              },
              {
                kind: "arg";
                path: "cert.match_id";
              }
            ];
          };
        },
        {
          name: "pool";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [120, 112, 111, 111, 108];
              }
            ];
          };
        },
        {
          name: "tournament";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [116, 111, 117, 114, 110, 97, 109, 101, 110, 116];
              },
              {
                kind: "account";
                path: "xmatch.tournament_id";
                account: "xChainMatch";
              }
            ];
          };
        },
        {
          name: "playerProfile";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [112, 108, 97, 121, 101, 114];
              },
              {
                kind: "account";
                path: "xmatch.tournament_id";
                account: "xChainMatch";
              },
              {
                kind: "account";
                path: "xmatch.player";
                account: "xChainMatch";
              }
            ];
          };
        },
        {
          name: "globalConfig";
          pda: {
            seeds: [
              {
                kind: "const";
                value: [
                  103,
                  108,
                  111,
                  98,
                  97,
                  108,
                  95,
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ];
              }
            ];
          };
        },
        {
          name: "treasury";
          writable: true;
        },
        {
          name: "player";
          writable: true;
        },
        {
          name: "cranker";
          docs: [
            "Pays profile rent if the cross-chain player has none yet for this",
            "tournament. Settle stays permissionless — anyone can crank."
          ];
          writable: true;
          signer: true;
        },
        {
          name: "systemProgram";
          address: "11111111111111111111111111111111";
        }
      ];
      args: [
        {
          name: "certNoA";
          type: {
            defined: {
              name: "matchLiveCertNoA";
            };
          };
        },
        {
          name: "outcome";
          type: {
            defined: {
              name: "outcomeCertArg";
            };
          };
        },
        {
          name: "liveSigs";
          type: {
            array: [
              {
                array: ["u8", 65];
              },
              3
            ];
          };
        },
        {
          name: "ocSigs";
          type: {
            array: [
              {
                array: ["u8", 65];
              },
              3
            ];
          };
        }
      ];
    },
    {
      name: "submitEquivocationProof";
      discriminator: [77, 177, 71, 33, 100, 111, 192, 103];
      accounts: [
        {
          name: "xmatch";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [120, 109, 97, 116, 99, 104];
              },
              {
                kind: "arg";
                path: "cert.match_id";
              }
            ];
          };
        },
        {
          name: "pool";
          pda: {
            seeds: [
              {
                kind: "const";
                value: [120, 112, 111, 111, 108];
              }
            ];
          };
        }
      ];
      args: [
        {
          name: "cert";
          type: {
            defined: {
              name: "matchLiveCertArg";
            };
          };
        },
        {
          name: "cpA";
          type: {
            defined: {
              name: "checkpointArg";
            };
          };
        },
        {
          name: "cpB";
          type: {
            defined: {
              name: "checkpointArg";
            };
          };
        },
        {
          name: "sigA";
          type: {
            array: ["u8", 65];
          };
        },
        {
          name: "sigB";
          type: {
            array: ["u8", 65];
          };
        }
      ];
    },
    {
      name: "supersedeXclaim";
      discriminator: [248, 228, 150, 207, 177, 83, 19, 0];
      accounts: [
        {
          name: "xmatch";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [120, 109, 97, 116, 99, 104];
              },
              {
                kind: "arg";
                path: "cert.match_id";
              }
            ];
          };
        }
      ];
      args: [
        {
          name: "cert";
          type: {
            defined: {
              name: "matchLiveCertArg";
            };
          };
        },
        {
          name: "cp";
          type: {
            defined: {
              name: "checkpointArg";
            };
          };
        },
        {
          name: "cpSigs";
          type: {
            array: [
              {
                array: ["u8", 65];
              },
              2
            ];
          };
        }
      ];
    },
    {
      name: "sweepUnclaimedToNext";
      discriminator: [181, 185, 22, 176, 116, 21, 237, 142];
      accounts: [
        {
          name: "srcTournament";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [116, 111, 117, 114, 110, 97, 109, 101, 110, 116];
              },
              {
                kind: "account";
                path: "src_tournament.tournament_id";
                account: "tournament";
              }
            ];
          };
        },
        {
          name: "destTournament";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [116, 111, 117, 114, 110, 97, 109, 101, 110, 116];
              },
              {
                kind: "account";
                path: "dest_tournament.tournament_id";
                account: "tournament";
              }
            ];
          };
        },
        {
          name: "globalConfig";
          pda: {
            seeds: [
              {
                kind: "const";
                value: [
                  103,
                  108,
                  111,
                  98,
                  97,
                  108,
                  95,
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ];
              }
            ];
          };
        },
        {
          name: "authority";
          signer: true;
        }
      ];
      args: [];
    },
    {
      name: "updateConfig";
      discriminator: [29, 158, 252, 191, 10, 83, 219, 99];
      accounts: [
        {
          name: "globalConfig";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [
                  103,
                  108,
                  111,
                  98,
                  97,
                  108,
                  95,
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ];
              }
            ];
          };
        },
        {
          name: "authority";
          signer: true;
        }
      ];
      args: [
        {
          name: "treasurySplitBps";
          type: "u16";
        },
        {
          name: "treasury";
          type: "pubkey";
        },
        {
          name: "matchmaker";
          type: "pubkey";
        },
        {
          name: "newAuthority";
          type: "pubkey";
        }
      ];
    },
    {
      name: "withdrawStake";
      discriminator: [153, 8, 22, 138, 105, 176, 87, 66];
      accounts: [
        {
          name: "escrow";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [101, 115, 99, 114, 111, 119];
              },
              {
                kind: "account";
                path: "escrow.tournament_id";
                account: "stakeEscrow";
              },
              {
                kind: "account";
                path: "player";
              }
            ];
          };
        },
        {
          name: "player";
          writable: true;
          signer: true;
          relations: ["escrow"];
        }
      ];
      args: [];
    },
    {
      name: "xpoolDeposit";
      discriminator: [110, 14, 7, 35, 110, 0, 226, 67];
      accounts: [
        {
          name: "pool";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [120, 112, 111, 111, 108];
              }
            ];
          };
        },
        {
          name: "funder";
          writable: true;
          signer: true;
        },
        {
          name: "systemProgram";
          address: "11111111111111111111111111111111";
        }
      ];
      args: [
        {
          name: "amount";
          type: "u64";
        }
      ];
    },
    {
      name: "xpoolWithdraw";
      discriminator: [225, 201, 161, 86, 205, 234, 61, 231];
      accounts: [
        {
          name: "pool";
          writable: true;
          pda: {
            seeds: [
              {
                kind: "const";
                value: [120, 112, 111, 111, 108];
              }
            ];
          };
        },
        {
          name: "operator";
          writable: true;
          signer: true;
        }
      ];
      args: [
        {
          name: "amount";
          type: "u64";
        }
      ];
    }
  ];
  accounts: [
    {
      name: "game";
      discriminator: [27, 90, 166, 125, 74, 100, 121, 18];
    },
    {
      name: "gameCounter";
      discriminator: [117, 67, 148, 185, 138, 194, 249, 87];
    },
    {
      name: "globalConfig";
      discriminator: [149, 8, 156, 202, 160, 252, 176, 217];
    },
    {
      name: "playerProfile";
      discriminator: [82, 226, 99, 87, 164, 130, 181, 80];
    },
    {
      name: "sessionAuthority";
      discriminator: [48, 9, 30, 120, 134, 35, 172, 170];
    },
    {
      name: "stakeEscrow";
      discriminator: [115, 173, 53, 77, 43, 219, 85, 124];
    },
    {
      name: "tournament";
      discriminator: [175, 139, 119, 242, 115, 194, 57, 92];
    },
    {
      name: "xChainMatch";
      discriminator: [54, 197, 96, 54, 83, 217, 157, 191];
    },
    {
      name: "xPayoutPool";
      discriminator: [78, 40, 163, 89, 47, 91, 90, 182];
    }
  ];
  events: [
    {
      name: "configUpdated";
      discriminator: [40, 241, 230, 122, 11, 19, 198, 194];
    },
    {
      name: "gameCancelled";
      discriminator: [113, 20, 200, 104, 76, 35, 9, 241];
    },
    {
      name: "gameCreated";
      discriminator: [218, 25, 150, 94, 177, 112, 96, 2];
    },
    {
      name: "gameResolved";
      discriminator: [145, 78, 127, 55, 138, 225, 142, 124];
    },
    {
      name: "gameStarted";
      discriminator: [222, 247, 78, 255, 61, 184, 156, 41];
    },
    {
      name: "guessCommitted";
      discriminator: [174, 25, 105, 114, 240, 123, 51, 187];
    },
    {
      name: "guessRevealed";
      discriminator: [128, 133, 199, 174, 53, 25, 192, 97];
    },
    {
      name: "rewardClaimed";
      discriminator: [49, 28, 87, 84, 158, 48, 229, 175];
    },
    {
      name: "sessionClosed";
      discriminator: [57, 237, 11, 243, 194, 34, 120, 27];
    },
    {
      name: "sessionCreated";
      discriminator: [107, 111, 254, 25, 21, 122, 220, 225];
    },
    {
      name: "stakeConfigured";
      discriminator: [33, 182, 14, 68, 141, 57, 14, 197];
    },
    {
      name: "stakeDeposited";
      discriminator: [69, 152, 144, 109, 232, 34, 225, 19];
    },
    {
      name: "stakeWithdrawn";
      discriminator: [33, 120, 159, 58, 140, 255, 174, 79];
    },
    {
      name: "timeoutSlash";
      discriminator: [92, 134, 243, 150, 210, 236, 191, 12];
    },
    {
      name: "tournamentCreated";
      discriminator: [102, 32, 240, 45, 52, 64, 97, 0];
    },
    {
      name: "tournamentFinalized";
      discriminator: [34, 61, 238, 26, 68, 54, 253, 144];
    },
    {
      name: "unclaimedSwept";
      discriminator: [20, 92, 19, 237, 135, 103, 255, 168];
    },
    {
      name: "xClaimOpened";
      discriminator: [131, 228, 172, 36, 173, 65, 72, 14];
    },
    {
      name: "xClaimSuperseded";
      discriminator: [53, 49, 110, 44, 213, 203, 99, 15];
    },
    {
      name: "xEquivocationProven";
      discriminator: [13, 130, 157, 206, 86, 165, 243, 226];
    },
    {
      name: "xMatchCreated";
      discriminator: [138, 194, 96, 86, 81, 146, 243, 65];
    },
    {
      name: "xMatchRefunded";
      discriminator: [146, 208, 173, 69, 81, 34, 154, 223];
    },
    {
      name: "xMatchSettled";
      discriminator: [222, 178, 153, 230, 99, 150, 152, 100];
    },
    {
      name: "xPoolDeposited";
      discriminator: [13, 187, 192, 177, 229, 177, 5, 207];
    },
    {
      name: "xPoolWithdrawn";
      discriminator: [208, 66, 138, 228, 180, 109, 42, 103];
    },
    {
      name: "xTrancheLocked";
      discriminator: [156, 55, 163, 65, 177, 70, 20, 43];
    }
  ];
  errors: [
    {
      code: 6000;
      name: "invalidGameState";
      msg: "Invalid game state for this instruction";
    },
    {
      code: 6001;
      name: "notAParticipant";
      msg: "Player is not a participant in this game";
    },
    {
      code: 6002;
      name: "alreadyCommitted";
      msg: "Player has already committed a guess";
    },
    {
      code: 6003;
      name: "alreadyRevealed";
      msg: "Player has already revealed a guess";
    },
    {
      code: 6004;
      name: "alreadyClaimed";
      msg: "Player has already claimed their reward";
    },
    {
      code: 6005;
      name: "cannotJoinOwnGame";
      msg: "Cannot join your own game";
    },
    {
      code: 6006;
      name: "stakeMismatch";
      msg: "Stake amount does not match the game's required stake";
    },
    {
      code: 6007;
      name: "commitmentMismatch";
      msg: "Commitment hash mismatch on reveal";
    },
    {
      code: 6008;
      name: "invalidGuessValue";
      msg: "Revealed guess is not a valid value (must be 0 or 1)";
    },
    {
      code: 6009;
      name: "timeoutNotElapsed";
      msg: "Timeout has not elapsed yet";
    },
    {
      code: 6010;
      name: "invalidTournamentTimes";
      msg: "Tournament end_time must be after start_time";
    },
    {
      code: 6011;
      name: "tournamentNotEnded";
      msg: "Tournament has not ended yet";
    },
    {
      code: 6012;
      name: "tournamentNotFinalized";
      msg: "Tournament must be finalized before rewards can be claimed";
    },
    {
      code: 6013;
      name: "emptyPrizePool";
      msg: "Tournament prize pool is empty";
    },
    {
      code: 6014;
      name: "outsideTournamentWindow";
      msg: "Game is outside the tournament window";
    },
    {
      code: 6015;
      name: "profileTournamentMismatch";
      msg: "Player profile does not belong to this tournament";
    },
    {
      code: 6016;
      name: "belowMinimumGames";
      msg: "Player has not played enough games to claim a reward (minimum 5)";
    },
    {
      code: 6017;
      name: "arithmeticOverflow";
      msg: "Arithmetic overflow";
    },
    {
      code: 6018;
      name: "tooManyAccounts";
      msg: "Too many accounts passed to finalize_tournament (maximum 30)";
    },
    {
      code: 6019;
      name: "escrowAlreadyConsumed";
      msg: "Escrow has already been consumed by a game";
    },
    {
      code: 6020;
      name: "escrowInvalid";
      msg: "Escrow is not valid for this game (wrong player, tournament, or amount)";
    },
    {
      code: 6021;
      name: "sessionExpired";
      msg: "Session has expired";
    },
    {
      code: 6022;
      name: "sessionPlayerMismatch";
      msg: "Session authority does not match the player";
    },
    {
      code: 6023;
      name: "sessionSignerMismatch";
      msg: "Session signer does not match the session key";
    },
    {
      code: 6024;
      name: "notAuthority";
      msg: "Caller is not the GlobalConfig authority";
    },
    {
      code: 6025;
      name: "notMatchmaker";
      msg: "Caller is not the authorized matchmaker";
    },
    {
      code: 6026;
      name: "invalidTreasurySplitBps";
      msg: "Treasury split basis points out of bounds [2000, 8000]";
    },
    {
      code: 6027;
      name: "invalidMerkleProof";
      msg: "Merkle proof verification failed";
    },
    {
      code: 6028;
      name: "merkleProofTooLong";
      msg: "Merkle proof exceeds maximum depth (20 levels)";
    },
    {
      code: 6029;
      name: "insufficientLamports";
      msg: "Source account has insufficient lamports for transfer";
    },
    {
      code: 6030;
      name: "unclaimedGracePeriodNotElapsed";
      msg: "Unclaimed grace period has not elapsed (T+90 days from end_time)";
    },
    {
      code: 6031;
      name: "destTournamentInvalid";
      msg: "Destination tournament is invalid (same as source, finalized, or outside its active window)";
    },
    {
      code: 6032;
      name: "rMatchupMismatch";
      msg: "r_matchup must not be passed once the matchup type is already revealed in the Game account";
    },
    {
      code: 6033;
      name: "xInvalidStatus";
      msg: "Cross-chain match is in the wrong status for this instruction";
    },
    {
      code: 6034;
      name: "xCertMismatch";
      msg: "Certificate terms do not match the recorded escrow state";
    },
    {
      code: 6035;
      name: "xBadSignature";
      msg: "Certificate signature did not recover the expected signer";
    },
    {
      code: 6036;
      name: "xStaleQuote";
      msg: "Rate quote is stale relative to the tranche lock";
    },
    {
      code: 6037;
      name: "xDeadlineNotReached";
      msg: "Deadline has not been reached yet";
    },
    {
      code: 6038;
      name: "xDeadlinePassed";
      msg: "Deadline has already passed";
    },
    {
      code: 6039;
      name: "xPoolInsufficient";
      msg: "Payout pool has insufficient free balance";
    },
    {
      code: 6040;
      name: "xTrancheTooLarge";
      msg: "Tranche exceeds the configured maximum";
    },
    {
      code: 6041;
      name: "xBadConfig";
      msg: "Cross-chain configuration is invalid";
    },
    {
      code: 6042;
      name: "xBadOutcome";
      msg: "Outcome kind is not valid for this settlement path";
    },
    {
      code: 6043;
      name: "invalidTreasury";
      msg: "Treasury must not be the zero pubkey";
    },
    {
      code: 6044;
      name: "tournamentEndsInPast";
      msg: "Tournament end_time must be in the future";
    }
  ];
  types: [
    {
      name: "certLegArg";
      docs: [
        "One settlement leg, as an instruction argument. Mirrors",
        "`cs::CertLeg`; converted to it purely for canonical encoding."
      ];
      type: {
        kind: "struct";
        fields: [
          {
            name: "chainTag";
            type: {
              array: ["u8", 32];
            };
          },
          {
            name: "contract";
            type: {
              array: ["u8", 32];
            };
          },
          {
            name: "player";
            type: {
              array: ["u8", 32];
            };
          },
          {
            name: "sessionKey";
            type: {
              array: ["u8", 20];
            };
          },
          {
            name: "stake";
            type: "u128";
          },
          {
            name: "tranche";
            type: "u128";
          }
        ];
      };
    },
    {
      name: "checkpointArg";
      docs: ["Co-signed transcript checkpoint, as an instruction argument."];
      type: {
        kind: "struct";
        fields: [
          {
            name: "matchLiveDigest";
            type: {
              array: ["u8", 32];
            };
          },
          {
            name: "stepCount";
            type: "u8";
          },
          {
            name: "p1Commit";
            type: {
              array: ["u8", 32];
            };
          },
          {
            name: "p2Commit";
            type: {
              array: ["u8", 32];
            };
          },
          {
            name: "p1Guess";
            type: "u8";
          },
          {
            name: "p2Guess";
            type: "u8";
          },
          {
            name: "firstCommitter";
            type: "u8";
          },
          {
            name: "matchupType";
            type: "u8";
          },
          {
            name: "transcriptHash";
            type: {
              array: ["u8", 32];
            };
          },
          {
            name: "rMatchup";
            docs: [
              "Matchup-type reveal preimage; bound to the cert's commitment on",
              "terminal checkpoints (see `verify_matchup_binding`). 0 when unused."
            ];
            type: {
              array: ["u8", 32];
            };
          }
        ];
      };
    },
    {
      name: "configUpdated";
      type: {
        kind: "struct";
        fields: [
          {
            name: "authority";
            type: "pubkey";
          },
          {
            name: "treasurySplitBps";
            type: "u16";
          }
        ];
      };
    },
    {
      name: "createXMatchArgs";
      docs: [
        "Args for `create_xmatch`, bundled so the instruction stays within the",
        "argument-count budget."
      ];
      type: {
        kind: "struct";
        fields: [
          {
            name: "tournamentId";
            type: "u64";
          },
          {
            name: "playerIsP1";
            type: "bool";
          },
          {
            name: "sessionKey";
            type: {
              array: ["u8", 20];
            };
          },
          {
            name: "counterSessionKey";
            type: {
              array: ["u8", 20];
            };
          },
          {
            name: "stakeLamports";
            type: "u64";
          },
          {
            name: "fundDeadline";
            type: "i64";
          },
          {
            name: "matchDeadline";
            type: "i64";
          }
        ];
      };
    },
    {
      name: "game";
      type: {
        kind: "struct";
        fields: [
          {
            name: "gameId";
            type: "u64";
          },
          {
            name: "tournamentId";
            type: "u64";
          },
          {
            name: "playerOne";
            type: "pubkey";
          },
          {
            name: "playerTwo";
            type: "pubkey";
          },
          {
            name: "state";
            type: {
              defined: {
                name: "gameState";
              };
            };
          },
          {
            name: "stakeLamports";
            type: "u64";
          },
          {
            name: "p1Commit";
            type: {
              array: ["u8", 32];
            };
          },
          {
            name: "p2Commit";
            type: {
              array: ["u8", 32];
            };
          },
          {
            name: "p1Guess";
            type: "u8";
          },
          {
            name: "p2Guess";
            type: "u8";
          },
          {
            name: "firstCommitter";
            type: "u8";
          },
          {
            name: "p1CommitSlot";
            type: "u64";
          },
          {
            name: "p2CommitSlot";
            type: "u64";
          },
          {
            name: "commitTimeoutSlots";
            type: "u64";
          },
          {
            name: "createdAt";
            type: "i64";
          },
          {
            name: "resolvedAt";
            type: "i64";
          },
          {
            name: "activatedAtSlot";
            docs: [
              "Solana slot at which the game entered Active state (set by join_game).",
              "Used for Active-state commit timeout (neither player commits)."
            ];
            type: "u64";
          },
          {
            name: "matchupCommitment";
            docs: [
              "SHA-256 commitment of the matchup type preimage (set at create_game by matchmaker co-sign).",
              "Verified during the first reveal to extract the actual matchup_type."
            ];
            type: {
              array: ["u8", 32];
            };
          },
          {
            name: "matchupType";
            docs: [
              "0 = same team (homogenous), 1 = different teams (heterogeneous), 255 = unset.",
              "Set to 255 at creation, resolved during first reveal via matchup_commitment verification."
            ];
            type: "u8";
          },
          {
            name: "bump";
            type: "u8";
          }
        ];
      };
    },
    {
      name: "gameCancelled";
      docs: [
        "Emitted by `refund_pending` when an un-joined Pending game is cancelled and",
        "P1's stake is refunded (mirrors the EVM `GameCancelled`). Powers a log-based",
        "metric / alarm on stuck-Pending recovery."
      ];
      type: {
        kind: "struct";
        fields: [
          {
            name: "gameId";
            type: "u64";
          },
          {
            name: "playerOne";
            type: "pubkey";
          },
          {
            name: "refundLamports";
            type: "u64";
          }
        ];
      };
    },
    {
      name: "gameCounter";
      type: {
        kind: "struct";
        fields: [
          {
            name: "count";
            type: "u64";
          },
          {
            name: "bump";
            type: "u8";
          }
        ];
      };
    },
    {
      name: "gameCreated";
      type: {
        kind: "struct";
        fields: [
          {
            name: "gameId";
            type: "u64";
          },
          {
            name: "tournamentId";
            type: "u64";
          },
          {
            name: "playerOne";
            type: "pubkey";
          },
          {
            name: "stakeLamports";
            type: "u64";
          }
        ];
      };
    },
    {
      name: "gameResolved";
      type: {
        kind: "struct";
        fields: [
          {
            name: "gameId";
            type: "u64";
          },
          {
            name: "p1Guess";
            type: "u8";
          },
          {
            name: "p2Guess";
            type: "u8";
          },
          {
            name: "p1Return";
            type: "u64";
          },
          {
            name: "p2Return";
            type: "u64";
          },
          {
            name: "tournamentGain";
            type: "u64";
          },
          {
            name: "treasuryGain";
            type: "u64";
          }
        ];
      };
    },
    {
      name: "gameStarted";
      type: {
        kind: "struct";
        fields: [
          {
            name: "gameId";
            type: "u64";
          },
          {
            name: "tournamentId";
            type: "u64";
          },
          {
            name: "playerOne";
            type: "pubkey";
          },
          {
            name: "playerTwo";
            type: "pubkey";
          }
        ];
      };
    },
    {
      name: "gameState";
      docs: [
        "Game lifecycle state machine.",
        "",
        "```text",
        "--(create_game)--> Pending",
        "Pending --(join_game)--> Active",
        "Pending --(refund_pending: P2 never joined, window elapsed)--> Cancelled (P1 refunded, account closed)",
        "Active  --(commit_guess: first)--> Committing",
        "Active  --(resolve_timeout: neither commits)--> Resolved",
        "Committing --(commit_guess: second)--> Revealing",
        "Committing --(resolve_timeout)--> Resolved",
        "Revealing  --(reveal_guess: both)--> Resolved",
        "Revealing  --(resolve_timeout)--> Resolved",
        "Resolved   --(close_game)--> [account closed]",
        "```"
      ];
      type: {
        kind: "enum";
        variants: [
          {
            name: "pending";
          },
          {
            name: "active";
          },
          {
            name: "committing";
          },
          {
            name: "revealing";
          },
          {
            name: "resolved";
          },
          {
            name: "cancelled";
          }
        ];
      };
    },
    {
      name: "globalConfig";
      docs: [
        "Singleton PDA storing protocol-level configuration.",
        'Seeds: `["global_config"]`'
      ];
      type: {
        kind: "struct";
        fields: [
          {
            name: "authority";
            docs: ["Governance authority (EOA for v1)."];
            type: "pubkey";
          },
          {
            name: "matchmaker";
            docs: ["Authorized matchmaker that gates `create_game`."];
            type: "pubkey";
          },
          {
            name: "treasury";
            docs: ["DAO treasury address for losing stake split."];
            type: "pubkey";
          },
          {
            name: "treasurySplitBps";
            docs: [
              "Portion of losing stakes sent to treasury (basis points).",
              "Default 5000 = 50%. Bounded to [2000, 8000]."
            ];
            type: "u16";
          },
          {
            name: "bump";
            type: "u8";
          },
          {
            name: "stakeLamports";
            docs: [
              "Per-game stake in lamports.",
              "",
              "APPENDED AFTER `bump` deliberately: the 107-byte v1 layout stays a",
              "byte-exact prefix, so `migrate_global_config` only has to grow the",
              "account and write this one field. An account survey on 2026-08-03 found",
              "exactly ONE GlobalConfig on mainnet and one on devnet, both at 107",
              "bytes, so the migration is a single call per network.",
              "",
              "This used to be the compile-time `FIXED_STAKE_LAMPORTS`, which meant",
              "re-pegging Solana required a PROGRAM UPGRADE while the EVM chains only",
              "needed `setConfig`. That asymmetry is why Solana sat at $3.64 against a",
              "$5 EVM anchor: the cheap change was made and the expensive one was not."
            ];
            type: "u64";
          }
        ];
      };
    },
    {
      name: "guessCommitted";
      type: {
        kind: "struct";
        fields: [
          {
            name: "gameId";
            type: "u64";
          },
          {
            name: "player";
            type: "pubkey";
          },
          {
            name: "commitSlot";
            type: "u64";
          }
        ];
      };
    },
    {
      name: "guessRevealed";
      type: {
        kind: "struct";
        fields: [
          {
            name: "gameId";
            type: "u64";
          },
          {
            name: "player";
            type: "pubkey";
          }
        ];
      };
    },
    {
      name: "matchLiveCertArg";
      docs: ["Match-live certificate, as an instruction argument."];
      type: {
        kind: "struct";
        fields: [
          {
            name: "matchId";
            type: {
              array: ["u8", 32];
            };
          },
          {
            name: "tournamentId";
            type: "u64";
          },
          {
            name: "matchupCommitment";
            type: {
              array: ["u8", 32];
            };
          },
          {
            name: "legA";
            type: {
              defined: {
                name: "certLegArg";
              };
            };
          },
          {
            name: "legB";
            type: {
              defined: {
                name: "certLegArg";
              };
            };
          },
          {
            name: "quoteTimestamp";
            type: "u64";
          },
          {
            name: "quoteMaxAgeSecs";
            type: "u32";
          },
          {
            name: "matchDeadline";
            type: "u64";
          },
          {
            name: "claimWindowSecs";
            type: "u32";
          },
          {
            name: "aIsP1";
            type: "u8";
          }
        ];
      };
    },
    {
      name: "matchLiveCertNoA";
      docs: [
        "Match-live certificate WITHOUT leg A — used by `settle_xmatch` to stay",
        "under Solana's 1232-byte transaction limit. Leg A is the Solana leg and",
        "is fully determined by on-chain match state, so the program reconstructs",
        "it rather than carrying its 148 bytes over the wire. This also removes",
        "the tamper surface: leg A is authoritative from state, never the caller."
      ];
      type: {
        kind: "struct";
        fields: [
          {
            name: "matchId";
            type: {
              array: ["u8", 32];
            };
          },
          {
            name: "tournamentId";
            type: "u64";
          },
          {
            name: "matchupCommitment";
            type: {
              array: ["u8", 32];
            };
          },
          {
            name: "legB";
            type: {
              defined: {
                name: "certLegArg";
              };
            };
          },
          {
            name: "quoteTimestamp";
            type: "u64";
          },
          {
            name: "quoteMaxAgeSecs";
            type: "u32";
          },
          {
            name: "matchDeadline";
            type: "u64";
          },
          {
            name: "claimWindowSecs";
            type: "u32";
          },
          {
            name: "aIsP1";
            type: "u8";
          }
        ];
      };
    },
    {
      name: "outcomeCertArg";
      docs: ["Outcome certificate, as an instruction argument."];
      type: {
        kind: "struct";
        fields: [
          {
            name: "matchId";
            type: {
              array: ["u8", 32];
            };
          },
          {
            name: "matchLiveDigest";
            type: {
              array: ["u8", 32];
            };
          },
          {
            name: "outcomeKind";
            type: "u8";
          },
          {
            name: "stepCount";
            type: "u8";
          },
          {
            name: "p1Guess";
            type: "u8";
          },
          {
            name: "p2Guess";
            type: "u8";
          },
          {
            name: "firstCommitter";
            type: "u8";
          },
          {
            name: "matchupType";
            type: "u8";
          },
          {
            name: "transcriptHash";
            type: {
              array: ["u8", 32];
            };
          }
        ];
      };
    },
    {
      name: "playerProfile";
      type: {
        kind: "struct";
        fields: [
          {
            name: "wallet";
            type: "pubkey";
          },
          {
            name: "tournamentId";
            type: "u64";
          },
          {
            name: "wins";
            type: "u64";
          },
          {
            name: "totalGames";
            type: "u64";
          },
          {
            name: "score";
            type: "u64";
          },
          {
            name: "claimed";
            type: "bool";
          },
          {
            name: "bump";
            type: "u8";
          }
        ];
      };
    },
    {
      name: "rewardClaimed";
      type: {
        kind: "struct";
        fields: [
          {
            name: "tournamentId";
            type: "u64";
          },
          {
            name: "player";
            type: "pubkey";
          },
          {
            name: "amount";
            type: "u64";
          }
        ];
      };
    },
    {
      name: "sessionAuthority";
      docs: [
        "Ephemeral session authority that lets a player delegate transaction signing",
        "to a temporary keypair. The player signs once to create the session; all",
        "subsequent game instructions can be signed by the session key instead.",
        "",
        'PDA seeds: `["game_session", player, session_key]`'
      ];
      type: {
        kind: "struct";
        fields: [
          {
            name: "player";
            docs: ["The real wallet that created this session."];
            type: "pubkey";
          },
          {
            name: "sessionKey";
            docs: ["The ephemeral keypair's public key."];
            type: "pubkey";
          },
          {
            name: "expiresAt";
            docs: ["Unix timestamp after which the session is invalid."];
            type: "i64";
          },
          {
            name: "bump";
            type: "u8";
          }
        ];
      };
    },
    {
      name: "sessionClosed";
      type: {
        kind: "struct";
        fields: [
          {
            name: "player";
            type: "pubkey";
          },
          {
            name: "sessionKey";
            type: "pubkey";
          }
        ];
      };
    },
    {
      name: "sessionCreated";
      type: {
        kind: "struct";
        fields: [
          {
            name: "player";
            type: "pubkey";
          },
          {
            name: "sessionKey";
            type: "pubkey";
          },
          {
            name: "expiresAt";
            type: "i64";
          }
        ];
      };
    },
    {
      name: "stakeConfigured";
      docs: [
        "Emitted when the authority re-pegs the per-game stake. Carries both values",
        "so an indexer can reconstruct the peg history without diffing account state."
      ];
      type: {
        kind: "struct";
        fields: [
          {
            name: "previousLamports";
            type: "u64";
          },
          {
            name: "newLamports";
            type: "u64";
          },
          {
            name: "authority";
            type: "pubkey";
          }
        ];
      };
    },
    {
      name: "stakeDeposited";
      type: {
        kind: "struct";
        fields: [
          {
            name: "player";
            type: "pubkey";
          },
          {
            name: "tournamentId";
            type: "u64";
          },
          {
            name: "amount";
            type: "u64";
          }
        ];
      };
    },
    {
      name: "stakeEscrow";
      docs: [
        "Per-player escrow that holds staked lamports while the player is in the",
        "matchmaking queue. Created by `deposit_stake`, consumed by `create_game`",
        "or `join_game`, refunded by `withdraw_stake`.",
        "",
        'PDA seeds: `["escrow", tournament_id, player]`'
      ];
      type: {
        kind: "struct";
        fields: [
          {
            name: "player";
            type: "pubkey";
          },
          {
            name: "tournamentId";
            type: "u64";
          },
          {
            name: "amount";
            type: "u64";
          },
          {
            name: "consumed";
            docs: [
              "True once the escrow has been consumed by a create_game or join_game",
              "instruction. Prevents double-spend if the same escrow PDA is reused."
            ];
            type: "bool";
          },
          {
            name: "bump";
            type: "u8";
          }
        ];
      };
    },
    {
      name: "stakeWithdrawn";
      type: {
        kind: "struct";
        fields: [
          {
            name: "wallet";
            type: "pubkey";
          },
          {
            name: "tournamentId";
            type: "u64";
          },
          {
            name: "amount";
            type: "u64";
          }
        ];
      };
    },
    {
      name: "timeoutSlash";
      type: {
        kind: "struct";
        fields: [
          {
            name: "gameId";
            type: "u64";
          },
          {
            name: "slashedPlayer";
            type: "pubkey";
          },
          {
            name: "slashAmount";
            type: "u64";
          }
        ];
      };
    },
    {
      name: "tournament";
      type: {
        kind: "struct";
        fields: [
          {
            name: "tournamentId";
            type: "u64";
          },
          {
            name: "authority";
            type: "pubkey";
          },
          {
            name: "startTime";
            type: "i64";
          },
          {
            name: "endTime";
            type: "i64";
          },
          {
            name: "prizeLamports";
            type: "u64";
          },
          {
            name: "gameCount";
            type: "u64";
          },
          {
            name: "finalized";
            type: "bool";
          },
          {
            name: "prizeSnapshot";
            type: "u64";
          },
          {
            name: "merkleRoot";
            type: {
              array: ["u8", 32];
            };
          },
          {
            name: "bump";
            type: "u8";
          }
        ];
      };
    },
    {
      name: "tournamentCreated";
      type: {
        kind: "struct";
        fields: [
          {
            name: "tournamentId";
            type: "u64";
          },
          {
            name: "startTime";
            type: "i64";
          },
          {
            name: "endTime";
            type: "i64";
          }
        ];
      };
    },
    {
      name: "tournamentFinalized";
      type: {
        kind: "struct";
        fields: [
          {
            name: "tournamentId";
            type: "u64";
          },
          {
            name: "prizeSnapshot";
            type: "u64";
          },
          {
            name: "merkleRoot";
            type: {
              array: ["u8", 32];
            };
          }
        ];
      };
    },
    {
      name: "unclaimedSwept";
      type: {
        kind: "struct";
        fields: [
          {
            name: "srcTournamentId";
            type: "u64";
          },
          {
            name: "destTournamentId";
            type: "u64";
          },
          {
            name: "amount";
            type: "u64";
          }
        ];
      };
    },
    {
      name: "xChainMatch";
      docs: [
        "Per-match cross-chain escrow + claim state. Doubles as the stake",
        "lamport vault. Cross-chain deadlines are unix seconds (NOT slots) so",
        "the certificate means the same thing on both legs.",
        "",
        'Seeds: `["xmatch", match_id]`'
      ];
      type: {
        kind: "struct";
        fields: [
          {
            name: "matchId";
            type: {
              array: ["u8", 32];
            };
          },
          {
            name: "tournamentId";
            type: "u64";
          },
          {
            name: "player";
            docs: ["Local player wallet (payout/refund recipient)."];
            type: "pubkey";
          },
          {
            name: "playerIsP1";
            docs: ["1 when this player is P1 in the payoff matrix, else 0."];
            type: "u8";
          },
          {
            name: "sessionKey";
            docs: [
              "This player's per-match secp256k1 session key (eth-address form)."
            ];
            type: {
              array: ["u8", 20];
            };
          },
          {
            name: "counterSessionKey";
            docs: ["Counterparty's session key (cert cross-check)."];
            type: {
              array: ["u8", 20];
            };
          },
          {
            name: "stakeLamports";
            type: "u64";
          },
          {
            name: "trancheLamports";
            docs: [
              "0 until locked. Max cross-chain payout from the float pool."
            ];
            type: "u64";
          },
          {
            name: "fundDeadline";
            type: "i64";
          },
          {
            name: "matchDeadline";
            type: "i64";
          },
          {
            name: "lockedAt";
            docs: ["Quote-freshness anchor; set at lock."];
            type: "i64";
          },
          {
            name: "claimWindowEnd";
            docs: [
              "Anchored claim window end: match_deadline + claim_window_secs."
            ];
            type: "i64";
          },
          {
            name: "claimWindowSecs";
            type: "u32";
          },
          {
            name: "matchLiveDigest";
            type: {
              array: ["u8", 32];
            };
          },
          {
            name: "bestStepCount";
            docs: ["255 once an equivocation verdict is final."];
            type: "u8";
          },
          {
            name: "bestOutcomeKind";
            type: "u8";
          },
          {
            name: "localEquivocated";
            docs: [
              "Equivocation flags — verdict is order-independent (mirrors the EVM",
              "fix: never inferred from best_outcome_kind)."
            ];
            type: "bool";
          },
          {
            name: "counterEquivocated";
            type: "bool";
          },
          {
            name: "status";
            type: {
              defined: {
                name: "xChainStatus";
              };
            };
          },
          {
            name: "bump";
            type: "u8";
          }
        ];
      };
    },
    {
      name: "xChainStatus";
      docs: [
        "Cross-chain match state machine (the Solana leg; mirrors the EVM",
        "CrossChainGame contract). Each chain holds one player's native-coin",
        "stake and settles only its own leg of a co-signed certificate.",
        "",
        "```text",
        "None --create_xmatch--> Funded --lock_xtranche--> Locked --settle_xmatch--> Settled",
        "(player+stake)    |        (operator)        |       (terminal 3+3-sig)",
        "+matchmaker       |                          |",
        "|                          +--open_xclaim--> Claiming",
        "|                          |   (2-sig checkpoint)  |",
        "|                          |   supersede / equivocation",
        "|                          |                       |",
        "|                          |   settle_xclaim (window closed)",
        "|                          |                       v",
        "|                          |                  ClaimSettled",
        "Funded --refund_xmatch_nocert--> RefundedNoCert      |",
        "(t > fund_deadline,                          +--refund_xmatch_timeout-->",
        "never locked)                                  RefundedTimeout",
        "(t > match_deadline +",
        "max_claim_window + 2*skew)",
        "```"
      ];
      type: {
        kind: "enum";
        variants: [
          {
            name: "none";
          },
          {
            name: "funded";
          },
          {
            name: "locked";
          },
          {
            name: "claiming";
          },
          {
            name: "settled";
          },
          {
            name: "claimSettled";
          },
          {
            name: "refundedNoCert";
          },
          {
            name: "refundedTimeout";
          }
        ];
      };
    },
    {
      name: "xClaimOpened";
      type: {
        kind: "struct";
        fields: [
          {
            name: "matchId";
            type: {
              array: ["u8", 32];
            };
          },
          {
            name: "claimedOutcome";
            type: "u8";
          },
          {
            name: "claimWindowEnd";
            type: "i64";
          }
        ];
      };
    },
    {
      name: "xClaimSuperseded";
      type: {
        kind: "struct";
        fields: [
          {
            name: "matchId";
            type: {
              array: ["u8", 32];
            };
          },
          {
            name: "stepCount";
            type: "u8";
          },
          {
            name: "claimedOutcome";
            type: "u8";
          }
        ];
      };
    },
    {
      name: "xEquivocationProven";
      type: {
        kind: "struct";
        fields: [
          {
            name: "matchId";
            type: {
              array: ["u8", 32];
            };
          },
          {
            name: "newBestOutcome";
            type: "u8";
          }
        ];
      };
    },
    {
      name: "xMatchCreated";
      type: {
        kind: "struct";
        fields: [
          {
            name: "matchId";
            type: {
              array: ["u8", 32];
            };
          },
          {
            name: "player";
            type: "pubkey";
          },
          {
            name: "stakeLamports";
            type: "u64";
          }
        ];
      };
    },
    {
      name: "xMatchRefunded";
      type: {
        kind: "struct";
        fields: [
          {
            name: "matchId";
            type: {
              array: ["u8", 32];
            };
          },
          {
            name: "toPlayer";
            type: "u64";
          }
        ];
      };
    },
    {
      name: "xMatchSettled";
      type: {
        kind: "struct";
        fields: [
          {
            name: "matchId";
            type: {
              array: ["u8", 32];
            };
          },
          {
            name: "outcomeKind";
            type: "u8";
          },
          {
            name: "toPlayer";
            type: "u64";
          },
          {
            name: "toTreasury";
            type: "u64";
          }
        ];
      };
    },
    {
      name: "xPayoutPool";
      docs: [
        "Singleton operator float pool — the on-chain collateral that pays",
        "cross-chain winners. Lamport vault tracked by free/locked split.",
        "",
        'Seeds: `["xpool"]`'
      ];
      type: {
        kind: "struct";
        fields: [
          {
            name: "operator";
            docs: ["Operator (float manager + tranche locker)."];
            type: "pubkey";
          },
          {
            name: "operatorSigner";
            docs: [
              "Dedicated secp256k1 certificate signer (eth-address form). NOT the",
              "operator key, NOT the program upgrade authority."
            ];
            type: {
              array: ["u8", 20];
            };
          },
          {
            name: "freeLamports";
            type: "u64";
          },
          {
            name: "lockedLamports";
            type: "u64";
          },
          {
            name: "maxTrancheLamports";
            type: "u64";
          },
          {
            name: "maxClaimWindowSecs";
            type: "u32";
          },
          {
            name: "skewMarginSecs";
            type: "u32";
          },
          {
            name: "bump";
            type: "u8";
          }
        ];
      };
    },
    {
      name: "xPoolDeposited";
      type: {
        kind: "struct";
        fields: [
          {
            name: "funder";
            type: "pubkey";
          },
          {
            name: "amount";
            type: "u64";
          }
        ];
      };
    },
    {
      name: "xPoolWithdrawn";
      type: {
        kind: "struct";
        fields: [
          {
            name: "amount";
            type: "u64";
          }
        ];
      };
    },
    {
      name: "xTrancheLocked";
      type: {
        kind: "struct";
        fields: [
          {
            name: "matchId";
            type: {
              array: ["u8", 32];
            };
          },
          {
            name: "trancheLamports";
            type: "u64";
          }
        ];
      };
    }
  ];
};

export const IDL = {
  address: "2qqVk7kUqffnahiJpcQJCsSd8ErbEUgKTgCn1zYsw64P",
  metadata: {
    name: "coordination_game",
    version: "0.1.0",
    spec: "0.1.0",
    description: "Created with Anchor",
  },
  instructions: [
    {
      name: "claim_reward",
      discriminator: [149, 95, 181, 242, 94, 90, 158, 162],
      accounts: [
        {
          name: "tournament",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [116, 111, 117, 114, 110, 97, 109, 101, 110, 116],
              },
              {
                kind: "account",
                path: "tournament.tournament_id",
                account: "Tournament",
              },
            ],
          },
        },
        {
          name: "player_profile",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [112, 108, 97, 121, 101, 114],
              },
              {
                kind: "account",
                path: "tournament.tournament_id",
                account: "Tournament",
              },
              {
                kind: "account",
                path: "player",
              },
            ],
          },
        },
        {
          name: "player",
          writable: true,
          signer: true,
        },
      ],
      args: [
        {
          name: "amount",
          type: "u64",
        },
        {
          name: "proof",
          type: {
            vec: {
              array: ["u8", 32],
            },
          },
        },
      ],
    },
    {
      name: "close_game",
      discriminator: [237, 236, 157, 201, 253, 20, 248, 67],
      accounts: [
        {
          name: "game",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [103, 97, 109, 101],
              },
              {
                kind: "account",
                path: "game.game_id",
                account: "Game",
              },
            ],
          },
        },
        {
          name: "caller",
          writable: true,
          signer: true,
        },
      ],
      args: [],
    },
    {
      name: "close_player_session",
      discriminator: [71, 20, 190, 152, 125, 164, 158, 29],
      accounts: [
        {
          name: "session_authority",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [
                  103, 97, 109, 101, 95, 115, 101, 115, 115, 105, 111, 110,
                ],
              },
              {
                kind: "account",
                path: "player",
              },
              {
                kind: "account",
                path: "session_key",
              },
            ],
          },
        },
        {
          name: "player",
          writable: true,
          signer: true,
        },
        {
          name: "session_key",
          docs: ["`session_authority.session_key` constraint in the handler."],
        },
      ],
      args: [],
    },
    {
      name: "close_session_by_delegate",
      discriminator: [84, 16, 164, 152, 197, 147, 185, 53],
      accounts: [
        {
          name: "session_authority",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [
                  103, 97, 109, 101, 95, 115, 101, 115, 115, 105, 111, 110,
                ],
              },
              {
                kind: "account",
                path: "session_authority.player",
                account: "SessionAuthority",
              },
              {
                kind: "account",
                path: "session_signer",
              },
            ],
          },
        },
        {
          name: "session_signer",
          writable: true,
          signer: true,
        },
      ],
      args: [],
    },
    {
      name: "close_xmatch",
      discriminator: [89, 82, 105, 191, 208, 158, 183, 160],
      accounts: [
        {
          name: "xmatch",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [120, 109, 97, 116, 99, 104],
              },
              {
                kind: "account",
                path: "xmatch.match_id",
                account: "XChainMatch",
              },
            ],
          },
        },
        {
          name: "player",
          writable: true,
        },
      ],
      args: [],
    },
    {
      name: "commit_guess",
      discriminator: [116, 86, 218, 54, 77, 153, 60, 230],
      accounts: [
        {
          name: "game",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [103, 97, 109, 101],
              },
              {
                kind: "account",
                path: "game.game_id",
                account: "Game",
              },
            ],
          },
        },
        {
          name: "player",
          signer: true,
        },
      ],
      args: [
        {
          name: "commitment",
          type: {
            array: ["u8", 32],
          },
        },
      ],
    },
    {
      name: "commit_guess_session",
      discriminator: [250, 149, 250, 122, 14, 69, 157, 127],
      accounts: [
        {
          name: "game",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [103, 97, 109, 101],
              },
              {
                kind: "account",
                path: "game.game_id",
                account: "Game",
              },
            ],
          },
        },
        {
          name: "player",
          docs: [
            "Verified against session_authority.player and game participants in the handler.",
          ],
        },
        {
          name: "session_authority",
          pda: {
            seeds: [
              {
                kind: "const",
                value: [
                  103, 97, 109, 101, 95, 115, 101, 115, 115, 105, 111, 110,
                ],
              },
              {
                kind: "account",
                path: "player",
              },
              {
                kind: "account",
                path: "session_signer",
              },
            ],
          },
        },
        {
          name: "session_signer",
          signer: true,
        },
      ],
      args: [
        {
          name: "commitment",
          type: {
            array: ["u8", 32],
          },
        },
      ],
    },
    {
      name: "create_game",
      discriminator: [124, 69, 75, 66, 184, 220, 72, 206],
      accounts: [
        {
          name: "game",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [103, 97, 109, 101],
              },
              {
                kind: "account",
                path: "game_counter.count",
                account: "GameCounter",
              },
            ],
          },
        },
        {
          name: "game_counter",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [
                  103, 97, 109, 101, 95, 99, 111, 117, 110, 116, 101, 114,
                ],
              },
            ],
          },
        },
        {
          name: "player_profile",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [112, 108, 97, 121, 101, 114],
              },
              {
                kind: "account",
                path: "tournament.tournament_id",
                account: "Tournament",
              },
              {
                kind: "account",
                path: "player",
              },
            ],
          },
        },
        {
          name: "escrow",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [101, 115, 99, 114, 111, 119],
              },
              {
                kind: "account",
                path: "tournament.tournament_id",
                account: "Tournament",
              },
              {
                kind: "account",
                path: "player",
              },
            ],
          },
        },
        {
          name: "tournament",
          pda: {
            seeds: [
              {
                kind: "const",
                value: [116, 111, 117, 114, 110, 97, 109, 101, 110, 116],
              },
              {
                kind: "account",
                path: "tournament.tournament_id",
                account: "Tournament",
              },
            ],
          },
        },
        {
          name: "global_config",
          pda: {
            seeds: [
              {
                kind: "const",
                value: [
                  103, 108, 111, 98, 97, 108, 95, 99, 111, 110, 102, 105, 103,
                ],
              },
            ],
          },
        },
        {
          name: "matchmaker",
          docs: [
            "Matchmaker co-signs to attest the commitment is legitimate.",
            "Verified against GlobalConfig.matchmaker. Does not pay gas.",
          ],
          signer: true,
        },
        {
          name: "player",
          writable: true,
          signer: true,
        },
        {
          name: "system_program",
          address: "11111111111111111111111111111111",
        },
      ],
      args: [
        {
          name: "stake_lamports",
          type: "u64",
        },
        {
          name: "matchup_commitment",
          type: {
            array: ["u8", 32],
          },
        },
      ],
    },
    {
      name: "create_game_session",
      discriminator: [130, 34, 251, 80, 77, 159, 113, 224],
      accounts: [
        {
          name: "game",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [103, 97, 109, 101],
              },
              {
                kind: "account",
                path: "game_counter.count",
                account: "GameCounter",
              },
            ],
          },
        },
        {
          name: "game_counter",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [
                  103, 97, 109, 101, 95, 99, 111, 117, 110, 116, 101, 114,
                ],
              },
            ],
          },
        },
        {
          name: "player_profile",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [112, 108, 97, 121, 101, 114],
              },
              {
                kind: "account",
                path: "tournament.tournament_id",
                account: "Tournament",
              },
              {
                kind: "account",
                path: "player",
              },
            ],
          },
        },
        {
          name: "escrow",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [101, 115, 99, 114, 111, 119],
              },
              {
                kind: "account",
                path: "tournament.tournament_id",
                account: "Tournament",
              },
              {
                kind: "account",
                path: "player",
              },
            ],
          },
        },
        {
          name: "tournament",
          pda: {
            seeds: [
              {
                kind: "const",
                value: [116, 111, 117, 114, 110, 97, 109, 101, 110, 116],
              },
              {
                kind: "account",
                path: "tournament.tournament_id",
                account: "Tournament",
              },
            ],
          },
        },
        {
          name: "global_config",
          pda: {
            seeds: [
              {
                kind: "const",
                value: [
                  103, 108, 111, 98, 97, 108, 95, 99, 111, 110, 102, 105, 103,
                ],
              },
            ],
          },
        },
        {
          name: "matchmaker",
          docs: [
            "Matchmaker co-signs to attest this commitment is legitimate. Verified",
            "against GlobalConfig.matchmaker. Does not pay gas.",
          ],
          signer: true,
        },
        {
          name: "player",
          docs: ["Verified against session_authority.player in the handler."],
        },
        {
          name: "session_authority",
          pda: {
            seeds: [
              {
                kind: "const",
                value: [
                  103, 97, 109, 101, 95, 115, 101, 115, 115, 105, 111, 110,
                ],
              },
              {
                kind: "account",
                path: "player",
              },
              {
                kind: "account",
                path: "session_signer",
              },
            ],
          },
        },
        {
          name: "session_signer",
          writable: true,
          signer: true,
        },
        {
          name: "system_program",
          address: "11111111111111111111111111111111",
        },
      ],
      args: [
        {
          name: "stake_lamports",
          type: "u64",
        },
        {
          name: "matchup_commitment",
          type: {
            array: ["u8", 32],
          },
        },
      ],
    },
    {
      name: "create_player_session",
      discriminator: [246, 143, 125, 132, 223, 76, 77, 177],
      accounts: [
        {
          name: "session_authority",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [
                  103, 97, 109, 101, 95, 115, 101, 115, 115, 105, 111, 110,
                ],
              },
              {
                kind: "account",
                path: "player",
              },
              {
                kind: "account",
                path: "session_key",
              },
            ],
          },
        },
        {
          name: "player",
          writable: true,
          signer: true,
        },
        {
          name: "session_key",
          docs: [
            "The ephemeral session keypair's public key. Not required to sign here;",
            "the player is authorizing this key to act on their behalf.",
            "is read from this account; it is only used for its key in PDA derivation.",
          ],
        },
        {
          name: "system_program",
          address: "11111111111111111111111111111111",
        },
      ],
      args: [],
    },
    {
      name: "create_tournament",
      discriminator: [158, 137, 233, 231, 73, 132, 191, 68],
      accounts: [
        {
          name: "tournament",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [116, 111, 117, 114, 110, 97, 109, 101, 110, 116],
              },
              {
                kind: "arg",
                path: "tournament_id",
              },
            ],
          },
        },
        {
          name: "authority",
          writable: true,
          signer: true,
        },
        {
          name: "system_program",
          address: "11111111111111111111111111111111",
        },
      ],
      args: [
        {
          name: "tournament_id",
          type: "u64",
        },
        {
          name: "start_time",
          type: "i64",
        },
        {
          name: "end_time",
          type: "i64",
        },
      ],
    },
    {
      name: "create_xmatch",
      discriminator: [123, 175, 124, 99, 101, 87, 143, 136],
      accounts: [
        {
          name: "xmatch",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [120, 109, 97, 116, 99, 104],
              },
              {
                kind: "arg",
                path: "match_id",
              },
            ],
          },
        },
        {
          name: "global_config",
          pda: {
            seeds: [
              {
                kind: "const",
                value: [
                  103, 108, 111, 98, 97, 108, 95, 99, 111, 110, 102, 105, 103,
                ],
              },
            ],
          },
        },
        {
          name: "matchmaker",
          docs: ["Matchmaker co-signs; does not pay gas."],
          signer: true,
        },
        {
          name: "player",
          writable: true,
          signer: true,
        },
        {
          name: "system_program",
          address: "11111111111111111111111111111111",
        },
      ],
      args: [
        {
          name: "match_id",
          type: {
            array: ["u8", 32],
          },
        },
        {
          name: "args",
          type: {
            defined: {
              name: "CreateXMatchArgs",
            },
          },
        },
      ],
    },
    {
      name: "deposit_stake",
      discriminator: [160, 167, 9, 220, 74, 243, 228, 43],
      accounts: [
        {
          name: "escrow",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [101, 115, 99, 114, 111, 119],
              },
              {
                kind: "account",
                path: "tournament.tournament_id",
                account: "Tournament",
              },
              {
                kind: "account",
                path: "player",
              },
            ],
          },
        },
        {
          name: "tournament",
        },
        {
          name: "player",
          writable: true,
          signer: true,
        },
        {
          name: "system_program",
          address: "11111111111111111111111111111111",
        },
      ],
      args: [],
    },
    {
      name: "deposit_stake_session",
      discriminator: [165, 195, 38, 185, 74, 161, 105, 28],
      accounts: [
        {
          name: "escrow",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [101, 115, 99, 114, 111, 119],
              },
              {
                kind: "account",
                path: "tournament.tournament_id",
                account: "Tournament",
              },
              {
                kind: "account",
                path: "player",
              },
            ],
          },
        },
        {
          name: "tournament",
        },
        {
          name: "player",
          docs: ["Verified against session_authority.player in the handler."],
        },
        {
          name: "session_authority",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [
                  103, 97, 109, 101, 95, 115, 101, 115, 115, 105, 111, 110,
                ],
              },
              {
                kind: "account",
                path: "player",
              },
              {
                kind: "account",
                path: "session_signer",
              },
            ],
          },
        },
        {
          name: "session_signer",
          writable: true,
          signer: true,
        },
        {
          name: "system_program",
          address: "11111111111111111111111111111111",
        },
      ],
      args: [],
    },
    {
      name: "finalize_tournament",
      discriminator: [205, 30, 149, 11, 108, 122, 120, 11],
      accounts: [
        {
          name: "tournament",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [116, 111, 117, 114, 110, 97, 109, 101, 110, 116],
              },
              {
                kind: "account",
                path: "tournament.tournament_id",
                account: "Tournament",
              },
            ],
          },
        },
        {
          name: "global_config",
          pda: {
            seeds: [
              {
                kind: "const",
                value: [
                  103, 108, 111, 98, 97, 108, 95, 99, 111, 110, 102, 105, 103,
                ],
              },
            ],
          },
        },
        {
          name: "authority",
          signer: true,
        },
      ],
      args: [
        {
          name: "merkle_root",
          type: {
            array: ["u8", 32],
          },
        },
      ],
    },
    {
      name: "initialize",
      discriminator: [175, 175, 109, 31, 13, 152, 155, 237],
      accounts: [
        {
          name: "game_counter",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [
                  103, 97, 109, 101, 95, 99, 111, 117, 110, 116, 101, 114,
                ],
              },
            ],
          },
        },
        {
          name: "authority",
          writable: true,
          signer: true,
        },
        {
          name: "system_program",
          address: "11111111111111111111111111111111",
        },
      ],
      args: [],
    },
    {
      name: "initialize_config",
      discriminator: [208, 127, 21, 1, 194, 190, 196, 70],
      accounts: [
        {
          name: "global_config",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [
                  103, 108, 111, 98, 97, 108, 95, 99, 111, 110, 102, 105, 103,
                ],
              },
            ],
          },
        },
        {
          name: "authority",
          writable: true,
          signer: true,
        },
        {
          name: "matchmaker",
        },
        {
          name: "treasury",
        },
        {
          name: "system_program",
          address: "11111111111111111111111111111111",
        },
      ],
      args: [
        {
          name: "treasury_split_bps",
          type: "u16",
        },
      ],
    },
    {
      name: "initialize_xpool",
      discriminator: [100, 223, 73, 78, 245, 69, 45, 66],
      accounts: [
        {
          name: "pool",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [120, 112, 111, 111, 108],
              },
            ],
          },
        },
        {
          name: "global_config",
          pda: {
            seeds: [
              {
                kind: "const",
                value: [
                  103, 108, 111, 98, 97, 108, 95, 99, 111, 110, 102, 105, 103,
                ],
              },
            ],
          },
        },
        {
          name: "authority",
          writable: true,
          signer: true,
        },
        {
          name: "system_program",
          address: "11111111111111111111111111111111",
        },
      ],
      args: [
        {
          name: "operator",
          type: "pubkey",
        },
        {
          name: "operator_signer",
          type: {
            array: ["u8", 20],
          },
        },
        {
          name: "max_tranche_lamports",
          type: "u64",
        },
        {
          name: "max_claim_window_secs",
          type: "u32",
        },
        {
          name: "skew_margin_secs",
          type: "u32",
        },
      ],
    },
    {
      name: "join_game",
      discriminator: [107, 112, 18, 38, 56, 173, 60, 128],
      accounts: [
        {
          name: "game",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [103, 97, 109, 101],
              },
              {
                kind: "account",
                path: "game.game_id",
                account: "Game",
              },
            ],
          },
        },
        {
          name: "player_profile",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [112, 108, 97, 121, 101, 114],
              },
              {
                kind: "account",
                path: "tournament.tournament_id",
                account: "Tournament",
              },
              {
                kind: "account",
                path: "player",
              },
            ],
          },
        },
        {
          name: "escrow",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [101, 115, 99, 114, 111, 119],
              },
              {
                kind: "account",
                path: "tournament.tournament_id",
                account: "Tournament",
              },
              {
                kind: "account",
                path: "player",
              },
            ],
          },
        },
        {
          name: "tournament",
          pda: {
            seeds: [
              {
                kind: "const",
                value: [116, 111, 117, 114, 110, 97, 109, 101, 110, 116],
              },
              {
                kind: "account",
                path: "game.tournament_id",
                account: "Game",
              },
            ],
          },
        },
        {
          name: "global_config",
          pda: {
            seeds: [
              {
                kind: "const",
                value: [
                  103, 108, 111, 98, 97, 108, 95, 99, 111, 110, 102, 105, 103,
                ],
              },
            ],
          },
        },
        {
          name: "matchmaker",
          docs: [
            "Matchmaker co-signs to attest this is the paired opponent. Verified",
            "against GlobalConfig.matchmaker. Does not pay gas.",
          ],
          signer: true,
        },
        {
          name: "player",
          writable: true,
          signer: true,
        },
        {
          name: "system_program",
          address: "11111111111111111111111111111111",
        },
      ],
      args: [],
    },
    {
      name: "join_game_session",
      discriminator: [247, 94, 51, 88, 130, 132, 135, 152],
      accounts: [
        {
          name: "game",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [103, 97, 109, 101],
              },
              {
                kind: "account",
                path: "game.game_id",
                account: "Game",
              },
            ],
          },
        },
        {
          name: "player_profile",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [112, 108, 97, 121, 101, 114],
              },
              {
                kind: "account",
                path: "tournament.tournament_id",
                account: "Tournament",
              },
              {
                kind: "account",
                path: "player",
              },
            ],
          },
        },
        {
          name: "escrow",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [101, 115, 99, 114, 111, 119],
              },
              {
                kind: "account",
                path: "tournament.tournament_id",
                account: "Tournament",
              },
              {
                kind: "account",
                path: "player",
              },
            ],
          },
        },
        {
          name: "tournament",
          pda: {
            seeds: [
              {
                kind: "const",
                value: [116, 111, 117, 114, 110, 97, 109, 101, 110, 116],
              },
              {
                kind: "account",
                path: "game.tournament_id",
                account: "Game",
              },
            ],
          },
        },
        {
          name: "global_config",
          pda: {
            seeds: [
              {
                kind: "const",
                value: [
                  103, 108, 111, 98, 97, 108, 95, 99, 111, 110, 102, 105, 103,
                ],
              },
            ],
          },
        },
        {
          name: "matchmaker",
          docs: [
            "Matchmaker co-signs to attest this is the paired opponent. Verified",
            "against GlobalConfig.matchmaker. Does not pay gas.",
          ],
          signer: true,
        },
        {
          name: "player",
          docs: ["Verified against session_authority.player in the handler."],
        },
        {
          name: "session_authority",
          pda: {
            seeds: [
              {
                kind: "const",
                value: [
                  103, 97, 109, 101, 95, 115, 101, 115, 115, 105, 111, 110,
                ],
              },
              {
                kind: "account",
                path: "player",
              },
              {
                kind: "account",
                path: "session_signer",
              },
            ],
          },
        },
        {
          name: "session_signer",
          writable: true,
          signer: true,
        },
        {
          name: "system_program",
          address: "11111111111111111111111111111111",
        },
      ],
      args: [],
    },
    {
      name: "lock_xtranche",
      discriminator: [251, 188, 147, 114, 170, 44, 196, 99],
      accounts: [
        {
          name: "xmatch",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [120, 109, 97, 116, 99, 104],
              },
              {
                kind: "arg",
                path: "cert.match_id",
              },
            ],
          },
        },
        {
          name: "pool",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [120, 112, 111, 111, 108],
              },
            ],
          },
        },
        {
          name: "cranker",
          docs: [
            "Permissionless fee payer \u2014 authorization is the operator signature on the",
            "cert, not this account. The caller pays the tx fee (gas external).",
          ],
          signer: true,
        },
      ],
      args: [
        {
          name: "cert",
          type: {
            defined: {
              name: "MatchLiveCertArg",
            },
          },
        },
        {
          name: "operator_sig",
          type: {
            array: ["u8", 65],
          },
        },
      ],
    },
    {
      name: "migrate_global_config",
      docs: [
        "One-shot realloc of the singleton GlobalConfig to carry `stake_lamports`.",
        "Run once per network; idempotent thereafter.",
      ],
      discriminator: [207, 52, 247, 7, 1, 230, 228, 147],
      accounts: [
        {
          name: "global_config",
          docs: [
            "cannot be deserialized into the 115-byte struct. Owner, discriminator,",
            "PDA seeds and authority are all verified in the handler.",
          ],
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [
                  103, 108, 111, 98, 97, 108, 95, 99, 111, 110, 102, 105, 103,
                ],
              },
            ],
          },
        },
        {
          name: "authority",
          docs: ["Must equal the authority recorded inside the account."],
          signer: true,
        },
      ],
      args: [],
    },
    {
      name: "open_xclaim",
      discriminator: [15, 161, 204, 56, 9, 104, 194, 15],
      accounts: [
        {
          name: "xmatch",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [120, 109, 97, 116, 99, 104],
              },
              {
                kind: "arg",
                path: "cert.match_id",
              },
            ],
          },
        },
        {
          name: "pool",
          pda: {
            seeds: [
              {
                kind: "const",
                value: [120, 112, 111, 111, 108],
              },
            ],
          },
        },
      ],
      args: [
        {
          name: "cert",
          type: {
            defined: {
              name: "MatchLiveCertArg",
            },
          },
        },
        {
          name: "cp",
          type: {
            defined: {
              name: "CheckpointArg",
            },
          },
        },
        {
          name: "live_sigs",
          type: {
            array: [
              {
                array: ["u8", 65],
              },
              3,
            ],
          },
        },
        {
          name: "cp_sigs",
          type: {
            array: [
              {
                array: ["u8", 65],
              },
              2,
            ],
          },
        },
      ],
    },
    {
      name: "refund_pending",
      discriminator: [70, 207, 125, 172, 197, 218, 120, 112],
      accounts: [
        {
          name: "game",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [103, 97, 109, 101],
              },
              {
                kind: "account",
                path: "game.game_id",
                account: "Game",
              },
            ],
          },
        },
        {
          name: "player_one_wallet",
          docs: [
            "the refunded stake regardless of who cranks the instruction.",
          ],
          writable: true,
        },
        {
          name: "caller",
          docs: [
            "Permissionless caller: pays the tx fee, receives the reclaimed rent.",
          ],
          writable: true,
          signer: true,
        },
      ],
      args: [],
    },
    {
      name: "refund_xmatch_nocert",
      discriminator: [151, 125, 113, 14, 42, 180, 182, 111],
      accounts: [
        {
          name: "xmatch",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [120, 109, 97, 116, 99, 104],
              },
              {
                kind: "account",
                path: "xmatch.match_id",
                account: "XChainMatch",
              },
            ],
          },
        },
        {
          name: "player",
          writable: true,
        },
      ],
      args: [],
    },
    {
      name: "refund_xmatch_timeout",
      discriminator: [171, 45, 68, 234, 69, 88, 162, 242],
      accounts: [
        {
          name: "xmatch",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [120, 109, 97, 116, 99, 104],
              },
              {
                kind: "account",
                path: "xmatch.match_id",
                account: "XChainMatch",
              },
            ],
          },
        },
        {
          name: "pool",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [120, 112, 111, 111, 108],
              },
            ],
          },
        },
        {
          name: "player",
          writable: true,
        },
      ],
      args: [],
    },
    {
      name: "resolve_timeout",
      discriminator: [149, 55, 89, 144, 121, 143, 48, 210],
      accounts: [
        {
          name: "game",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [103, 97, 109, 101],
              },
              {
                kind: "account",
                path: "game.game_id",
                account: "Game",
              },
            ],
          },
        },
        {
          name: "p1_profile",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [112, 108, 97, 121, 101, 114],
              },
              {
                kind: "account",
                path: "tournament.tournament_id",
                account: "Tournament",
              },
              {
                kind: "account",
                path: "game.player_one",
                account: "Game",
              },
            ],
          },
        },
        {
          name: "p2_profile",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [112, 108, 97, 121, 101, 114],
              },
              {
                kind: "account",
                path: "tournament.tournament_id",
                account: "Tournament",
              },
              {
                kind: "account",
                path: "game.player_two",
                account: "Game",
              },
            ],
          },
        },
        {
          name: "tournament",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [116, 111, 117, 114, 110, 97, 109, 101, 110, 116],
              },
              {
                kind: "account",
                path: "game.tournament_id",
                account: "Game",
              },
            ],
          },
        },
        {
          name: "global_config",
          pda: {
            seeds: [
              {
                kind: "const",
                value: [
                  103, 108, 111, 98, 97, 108, 95, 99, 111, 110, 102, 105, 103,
                ],
              },
            ],
          },
        },
        {
          name: "treasury",
          writable: true,
        },
        {
          name: "player_one_wallet",
          writable: true,
        },
        {
          name: "player_two_wallet",
          writable: true,
        },
        {
          name: "caller",
          docs: [
            "Caller receives no prize but pays the transaction fee; rent reclaim via close_game",
          ],
          signer: true,
        },
      ],
      args: [],
    },
    {
      name: "reveal_guess",
      discriminator: [209, 228, 167, 227, 138, 208, 149, 57],
      accounts: [
        {
          name: "game",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [103, 97, 109, 101],
              },
              {
                kind: "account",
                path: "game.game_id",
                account: "Game",
              },
            ],
          },
        },
        {
          name: "player",
          signer: true,
        },
        {
          name: "p1_profile",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [112, 108, 97, 121, 101, 114],
              },
              {
                kind: "account",
                path: "tournament.tournament_id",
                account: "Tournament",
              },
              {
                kind: "account",
                path: "game.player_one",
                account: "Game",
              },
            ],
          },
        },
        {
          name: "p2_profile",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [112, 108, 97, 121, 101, 114],
              },
              {
                kind: "account",
                path: "tournament.tournament_id",
                account: "Tournament",
              },
              {
                kind: "account",
                path: "game.player_two",
                account: "Game",
              },
            ],
          },
        },
        {
          name: "tournament",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [116, 111, 117, 114, 110, 97, 109, 101, 110, 116],
              },
              {
                kind: "account",
                path: "game.tournament_id",
                account: "Game",
              },
            ],
          },
        },
        {
          name: "global_config",
          pda: {
            seeds: [
              {
                kind: "const",
                value: [
                  103, 108, 111, 98, 97, 108, 95, 99, 111, 110, 102, 105, 103,
                ],
              },
            ],
          },
        },
        {
          name: "treasury",
          writable: true,
        },
        {
          name: "player_one_wallet",
          writable: true,
        },
        {
          name: "player_two_wallet",
          writable: true,
        },
      ],
      args: [
        {
          name: "r",
          type: {
            array: ["u8", 32],
          },
        },
        {
          name: "r_matchup",
          type: {
            option: {
              array: ["u8", 32],
            },
          },
        },
      ],
    },
    {
      name: "reveal_guess_session",
      discriminator: [33, 255, 161, 50, 125, 126, 132, 197],
      accounts: [
        {
          name: "game",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [103, 97, 109, 101],
              },
              {
                kind: "account",
                path: "game.game_id",
                account: "Game",
              },
            ],
          },
        },
        {
          name: "player",
          docs: [
            "Verified against session_authority.player and game participants in the handler.",
          ],
        },
        {
          name: "session_authority",
          pda: {
            seeds: [
              {
                kind: "const",
                value: [
                  103, 97, 109, 101, 95, 115, 101, 115, 115, 105, 111, 110,
                ],
              },
              {
                kind: "account",
                path: "player",
              },
              {
                kind: "account",
                path: "session_signer",
              },
            ],
          },
        },
        {
          name: "session_signer",
          signer: true,
        },
        {
          name: "p1_profile",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [112, 108, 97, 121, 101, 114],
              },
              {
                kind: "account",
                path: "tournament.tournament_id",
                account: "Tournament",
              },
              {
                kind: "account",
                path: "game.player_one",
                account: "Game",
              },
            ],
          },
        },
        {
          name: "p2_profile",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [112, 108, 97, 121, 101, 114],
              },
              {
                kind: "account",
                path: "tournament.tournament_id",
                account: "Tournament",
              },
              {
                kind: "account",
                path: "game.player_two",
                account: "Game",
              },
            ],
          },
        },
        {
          name: "tournament",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [116, 111, 117, 114, 110, 97, 109, 101, 110, 116],
              },
              {
                kind: "account",
                path: "game.tournament_id",
                account: "Game",
              },
            ],
          },
        },
        {
          name: "global_config",
          pda: {
            seeds: [
              {
                kind: "const",
                value: [
                  103, 108, 111, 98, 97, 108, 95, 99, 111, 110, 102, 105, 103,
                ],
              },
            ],
          },
        },
        {
          name: "treasury",
          writable: true,
        },
        {
          name: "player_one_wallet",
          writable: true,
        },
        {
          name: "player_two_wallet",
          writable: true,
        },
      ],
      args: [
        {
          name: "r",
          type: {
            array: ["u8", 32],
          },
        },
        {
          name: "r_matchup",
          type: {
            option: {
              array: ["u8", 32],
            },
          },
        },
      ],
    },
    {
      name: "set_stake_lamports",
      docs: [
        "Re-peg the per-game stake without a program upgrade \u2014 the Solana",
        "counterpart of the EVM contract's `setConfig`.",
      ],
      discriminator: [76, 249, 165, 98, 24, 24, 181, 140],
      accounts: [
        {
          name: "global_config",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [
                  103, 108, 111, 98, 97, 108, 95, 99, 111, 110, 102, 105, 103,
                ],
              },
            ],
          },
        },
        {
          name: "authority",
          signer: true,
          relations: ["global_config"],
        },
      ],
      args: [
        {
          name: "new_stake",
          type: "u64",
        },
      ],
    },
    {
      name: "settle_xclaim",
      discriminator: [217, 87, 189, 58, 156, 153, 141, 144],
      accounts: [
        {
          name: "xmatch",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [120, 109, 97, 116, 99, 104],
              },
              {
                kind: "account",
                path: "xmatch.match_id",
                account: "XChainMatch",
              },
            ],
          },
        },
        {
          name: "pool",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [120, 112, 111, 111, 108],
              },
            ],
          },
        },
        {
          name: "tournament",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [116, 111, 117, 114, 110, 97, 109, 101, 110, 116],
              },
              {
                kind: "account",
                path: "xmatch.tournament_id",
                account: "XChainMatch",
              },
            ],
          },
        },
        {
          name: "player_profile",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [112, 108, 97, 121, 101, 114],
              },
              {
                kind: "account",
                path: "xmatch.tournament_id",
                account: "XChainMatch",
              },
              {
                kind: "account",
                path: "xmatch.player",
                account: "XChainMatch",
              },
            ],
          },
        },
        {
          name: "global_config",
          pda: {
            seeds: [
              {
                kind: "const",
                value: [
                  103, 108, 111, 98, 97, 108, 95, 99, 111, 110, 102, 105, 103,
                ],
              },
            ],
          },
        },
        {
          name: "treasury",
          writable: true,
        },
        {
          name: "player",
          writable: true,
        },
        {
          name: "cranker",
          docs: [
            "Pays profile rent if the cross-chain player has none yet for this",
            "tournament. Settle stays permissionless \u2014 anyone can crank.",
          ],
          writable: true,
          signer: true,
        },
        {
          name: "system_program",
          address: "11111111111111111111111111111111",
        },
      ],
      args: [],
    },
    {
      name: "settle_xmatch",
      discriminator: [143, 202, 124, 191, 205, 192, 18, 207],
      accounts: [
        {
          name: "xmatch",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [120, 109, 97, 116, 99, 104],
              },
              {
                kind: "arg",
                path: "cert.match_id",
              },
            ],
          },
        },
        {
          name: "pool",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [120, 112, 111, 111, 108],
              },
            ],
          },
        },
        {
          name: "tournament",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [116, 111, 117, 114, 110, 97, 109, 101, 110, 116],
              },
              {
                kind: "account",
                path: "xmatch.tournament_id",
                account: "XChainMatch",
              },
            ],
          },
        },
        {
          name: "player_profile",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [112, 108, 97, 121, 101, 114],
              },
              {
                kind: "account",
                path: "xmatch.tournament_id",
                account: "XChainMatch",
              },
              {
                kind: "account",
                path: "xmatch.player",
                account: "XChainMatch",
              },
            ],
          },
        },
        {
          name: "global_config",
          pda: {
            seeds: [
              {
                kind: "const",
                value: [
                  103, 108, 111, 98, 97, 108, 95, 99, 111, 110, 102, 105, 103,
                ],
              },
            ],
          },
        },
        {
          name: "treasury",
          writable: true,
        },
        {
          name: "player",
          writable: true,
        },
        {
          name: "cranker",
          docs: [
            "Pays profile rent if the cross-chain player has none yet for this",
            "tournament. Settle stays permissionless \u2014 anyone can crank.",
          ],
          writable: true,
          signer: true,
        },
        {
          name: "system_program",
          address: "11111111111111111111111111111111",
        },
      ],
      args: [
        {
          name: "cert_no_a",
          type: {
            defined: {
              name: "MatchLiveCertNoA",
            },
          },
        },
        {
          name: "outcome",
          type: {
            defined: {
              name: "OutcomeCertArg",
            },
          },
        },
        {
          name: "live_sigs",
          type: {
            array: [
              {
                array: ["u8", 65],
              },
              3,
            ],
          },
        },
        {
          name: "oc_sigs",
          type: {
            array: [
              {
                array: ["u8", 65],
              },
              3,
            ],
          },
        },
      ],
    },
    {
      name: "submit_equivocation_proof",
      discriminator: [77, 177, 71, 33, 100, 111, 192, 103],
      accounts: [
        {
          name: "xmatch",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [120, 109, 97, 116, 99, 104],
              },
              {
                kind: "arg",
                path: "cert.match_id",
              },
            ],
          },
        },
        {
          name: "pool",
          pda: {
            seeds: [
              {
                kind: "const",
                value: [120, 112, 111, 111, 108],
              },
            ],
          },
        },
      ],
      args: [
        {
          name: "cert",
          type: {
            defined: {
              name: "MatchLiveCertArg",
            },
          },
        },
        {
          name: "cp_a",
          type: {
            defined: {
              name: "CheckpointArg",
            },
          },
        },
        {
          name: "cp_b",
          type: {
            defined: {
              name: "CheckpointArg",
            },
          },
        },
        {
          name: "sig_a",
          type: {
            array: ["u8", 65],
          },
        },
        {
          name: "sig_b",
          type: {
            array: ["u8", 65],
          },
        },
      ],
    },
    {
      name: "supersede_xclaim",
      discriminator: [248, 228, 150, 207, 177, 83, 19, 0],
      accounts: [
        {
          name: "xmatch",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [120, 109, 97, 116, 99, 104],
              },
              {
                kind: "arg",
                path: "cert.match_id",
              },
            ],
          },
        },
      ],
      args: [
        {
          name: "cert",
          type: {
            defined: {
              name: "MatchLiveCertArg",
            },
          },
        },
        {
          name: "cp",
          type: {
            defined: {
              name: "CheckpointArg",
            },
          },
        },
        {
          name: "cp_sigs",
          type: {
            array: [
              {
                array: ["u8", 65],
              },
              2,
            ],
          },
        },
      ],
    },
    {
      name: "sweep_unclaimed_to_next",
      discriminator: [181, 185, 22, 176, 116, 21, 237, 142],
      accounts: [
        {
          name: "src_tournament",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [116, 111, 117, 114, 110, 97, 109, 101, 110, 116],
              },
              {
                kind: "account",
                path: "src_tournament.tournament_id",
                account: "Tournament",
              },
            ],
          },
        },
        {
          name: "dest_tournament",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [116, 111, 117, 114, 110, 97, 109, 101, 110, 116],
              },
              {
                kind: "account",
                path: "dest_tournament.tournament_id",
                account: "Tournament",
              },
            ],
          },
        },
        {
          name: "global_config",
          pda: {
            seeds: [
              {
                kind: "const",
                value: [
                  103, 108, 111, 98, 97, 108, 95, 99, 111, 110, 102, 105, 103,
                ],
              },
            ],
          },
        },
        {
          name: "authority",
          signer: true,
        },
      ],
      args: [],
    },
    {
      name: "update_config",
      discriminator: [29, 158, 252, 191, 10, 83, 219, 99],
      accounts: [
        {
          name: "global_config",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [
                  103, 108, 111, 98, 97, 108, 95, 99, 111, 110, 102, 105, 103,
                ],
              },
            ],
          },
        },
        {
          name: "authority",
          signer: true,
        },
      ],
      args: [
        {
          name: "treasury_split_bps",
          type: "u16",
        },
        {
          name: "treasury",
          type: "pubkey",
        },
        {
          name: "matchmaker",
          type: "pubkey",
        },
        {
          name: "new_authority",
          type: "pubkey",
        },
      ],
    },
    {
      name: "withdraw_stake",
      discriminator: [153, 8, 22, 138, 105, 176, 87, 66],
      accounts: [
        {
          name: "escrow",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [101, 115, 99, 114, 111, 119],
              },
              {
                kind: "account",
                path: "escrow.tournament_id",
                account: "StakeEscrow",
              },
              {
                kind: "account",
                path: "player",
              },
            ],
          },
        },
        {
          name: "player",
          writable: true,
          signer: true,
          relations: ["escrow"],
        },
      ],
      args: [],
    },
    {
      name: "xpool_deposit",
      discriminator: [110, 14, 7, 35, 110, 0, 226, 67],
      accounts: [
        {
          name: "pool",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [120, 112, 111, 111, 108],
              },
            ],
          },
        },
        {
          name: "funder",
          writable: true,
          signer: true,
        },
        {
          name: "system_program",
          address: "11111111111111111111111111111111",
        },
      ],
      args: [
        {
          name: "amount",
          type: "u64",
        },
      ],
    },
    {
      name: "xpool_withdraw",
      discriminator: [225, 201, 161, 86, 205, 234, 61, 231],
      accounts: [
        {
          name: "pool",
          writable: true,
          pda: {
            seeds: [
              {
                kind: "const",
                value: [120, 112, 111, 111, 108],
              },
            ],
          },
        },
        {
          name: "operator",
          writable: true,
          signer: true,
        },
      ],
      args: [
        {
          name: "amount",
          type: "u64",
        },
      ],
    },
  ],
  accounts: [
    {
      name: "Game",
      discriminator: [27, 90, 166, 125, 74, 100, 121, 18],
    },
    {
      name: "GameCounter",
      discriminator: [117, 67, 148, 185, 138, 194, 249, 87],
    },
    {
      name: "GlobalConfig",
      discriminator: [149, 8, 156, 202, 160, 252, 176, 217],
    },
    {
      name: "PlayerProfile",
      discriminator: [82, 226, 99, 87, 164, 130, 181, 80],
    },
    {
      name: "SessionAuthority",
      discriminator: [48, 9, 30, 120, 134, 35, 172, 170],
    },
    {
      name: "StakeEscrow",
      discriminator: [115, 173, 53, 77, 43, 219, 85, 124],
    },
    {
      name: "Tournament",
      discriminator: [175, 139, 119, 242, 115, 194, 57, 92],
    },
    {
      name: "XChainMatch",
      discriminator: [54, 197, 96, 54, 83, 217, 157, 191],
    },
    {
      name: "XPayoutPool",
      discriminator: [78, 40, 163, 89, 47, 91, 90, 182],
    },
  ],
  events: [
    {
      name: "ConfigUpdated",
      discriminator: [40, 241, 230, 122, 11, 19, 198, 194],
    },
    {
      name: "GameCancelled",
      discriminator: [113, 20, 200, 104, 76, 35, 9, 241],
    },
    {
      name: "GameCreated",
      discriminator: [218, 25, 150, 94, 177, 112, 96, 2],
    },
    {
      name: "GameResolved",
      discriminator: [145, 78, 127, 55, 138, 225, 142, 124],
    },
    {
      name: "GameStarted",
      discriminator: [222, 247, 78, 255, 61, 184, 156, 41],
    },
    {
      name: "GuessCommitted",
      discriminator: [174, 25, 105, 114, 240, 123, 51, 187],
    },
    {
      name: "GuessRevealed",
      discriminator: [128, 133, 199, 174, 53, 25, 192, 97],
    },
    {
      name: "RewardClaimed",
      discriminator: [49, 28, 87, 84, 158, 48, 229, 175],
    },
    {
      name: "SessionClosed",
      discriminator: [57, 237, 11, 243, 194, 34, 120, 27],
    },
    {
      name: "SessionCreated",
      discriminator: [107, 111, 254, 25, 21, 122, 220, 225],
    },
    {
      name: "StakeConfigured",
      discriminator: [33, 182, 14, 68, 141, 57, 14, 197],
    },
    {
      name: "StakeDeposited",
      discriminator: [69, 152, 144, 109, 232, 34, 225, 19],
    },
    {
      name: "StakeWithdrawn",
      discriminator: [33, 120, 159, 58, 140, 255, 174, 79],
    },
    {
      name: "TimeoutSlash",
      discriminator: [92, 134, 243, 150, 210, 236, 191, 12],
    },
    {
      name: "TournamentCreated",
      discriminator: [102, 32, 240, 45, 52, 64, 97, 0],
    },
    {
      name: "TournamentFinalized",
      discriminator: [34, 61, 238, 26, 68, 54, 253, 144],
    },
    {
      name: "UnclaimedSwept",
      discriminator: [20, 92, 19, 237, 135, 103, 255, 168],
    },
    {
      name: "XClaimOpened",
      discriminator: [131, 228, 172, 36, 173, 65, 72, 14],
    },
    {
      name: "XClaimSuperseded",
      discriminator: [53, 49, 110, 44, 213, 203, 99, 15],
    },
    {
      name: "XEquivocationProven",
      discriminator: [13, 130, 157, 206, 86, 165, 243, 226],
    },
    {
      name: "XMatchCreated",
      discriminator: [138, 194, 96, 86, 81, 146, 243, 65],
    },
    {
      name: "XMatchRefunded",
      discriminator: [146, 208, 173, 69, 81, 34, 154, 223],
    },
    {
      name: "XMatchSettled",
      discriminator: [222, 178, 153, 230, 99, 150, 152, 100],
    },
    {
      name: "XPoolDeposited",
      discriminator: [13, 187, 192, 177, 229, 177, 5, 207],
    },
    {
      name: "XPoolWithdrawn",
      discriminator: [208, 66, 138, 228, 180, 109, 42, 103],
    },
    {
      name: "XTrancheLocked",
      discriminator: [156, 55, 163, 65, 177, 70, 20, 43],
    },
  ],
  errors: [
    {
      code: 6000,
      name: "InvalidGameState",
      msg: "Invalid game state for this instruction",
    },
    {
      code: 6001,
      name: "NotAParticipant",
      msg: "Player is not a participant in this game",
    },
    {
      code: 6002,
      name: "AlreadyCommitted",
      msg: "Player has already committed a guess",
    },
    {
      code: 6003,
      name: "AlreadyRevealed",
      msg: "Player has already revealed a guess",
    },
    {
      code: 6004,
      name: "AlreadyClaimed",
      msg: "Player has already claimed their reward",
    },
    {
      code: 6005,
      name: "CannotJoinOwnGame",
      msg: "Cannot join your own game",
    },
    {
      code: 6006,
      name: "StakeMismatch",
      msg: "Stake amount does not match the game's required stake",
    },
    {
      code: 6007,
      name: "CommitmentMismatch",
      msg: "Commitment hash mismatch on reveal",
    },
    {
      code: 6008,
      name: "InvalidGuessValue",
      msg: "Revealed guess is not a valid value (must be 0 or 1)",
    },
    {
      code: 6009,
      name: "TimeoutNotElapsed",
      msg: "Timeout has not elapsed yet",
    },
    {
      code: 6010,
      name: "InvalidTournamentTimes",
      msg: "Tournament end_time must be after start_time",
    },
    {
      code: 6011,
      name: "TournamentNotEnded",
      msg: "Tournament has not ended yet",
    },
    {
      code: 6012,
      name: "TournamentNotFinalized",
      msg: "Tournament must be finalized before rewards can be claimed",
    },
    {
      code: 6013,
      name: "EmptyPrizePool",
      msg: "Tournament prize pool is empty",
    },
    {
      code: 6014,
      name: "OutsideTournamentWindow",
      msg: "Game is outside the tournament window",
    },
    {
      code: 6015,
      name: "ProfileTournamentMismatch",
      msg: "Player profile does not belong to this tournament",
    },
    {
      code: 6016,
      name: "BelowMinimumGames",
      msg: "Player has not played enough games to claim a reward (minimum 5)",
    },
    {
      code: 6017,
      name: "ArithmeticOverflow",
      msg: "Arithmetic overflow",
    },
    {
      code: 6018,
      name: "TooManyAccounts",
      msg: "Too many accounts passed to finalize_tournament (maximum 30)",
    },
    {
      code: 6019,
      name: "EscrowAlreadyConsumed",
      msg: "Escrow has already been consumed by a game",
    },
    {
      code: 6020,
      name: "EscrowInvalid",
      msg: "Escrow is not valid for this game (wrong player, tournament, or amount)",
    },
    {
      code: 6021,
      name: "SessionExpired",
      msg: "Session has expired",
    },
    {
      code: 6022,
      name: "SessionPlayerMismatch",
      msg: "Session authority does not match the player",
    },
    {
      code: 6023,
      name: "SessionSignerMismatch",
      msg: "Session signer does not match the session key",
    },
    {
      code: 6024,
      name: "NotAuthority",
      msg: "Caller is not the GlobalConfig authority",
    },
    {
      code: 6025,
      name: "NotMatchmaker",
      msg: "Caller is not the authorized matchmaker",
    },
    {
      code: 6026,
      name: "InvalidTreasurySplitBps",
      msg: "Treasury split basis points out of bounds [2000, 8000]",
    },
    {
      code: 6027,
      name: "InvalidMerkleProof",
      msg: "Merkle proof verification failed",
    },
    {
      code: 6028,
      name: "MerkleProofTooLong",
      msg: "Merkle proof exceeds maximum depth (20 levels)",
    },
    {
      code: 6029,
      name: "InsufficientLamports",
      msg: "Source account has insufficient lamports for transfer",
    },
    {
      code: 6030,
      name: "UnclaimedGracePeriodNotElapsed",
      msg: "Unclaimed grace period has not elapsed (T+90 days from end_time)",
    },
    {
      code: 6031,
      name: "DestTournamentInvalid",
      msg: "Destination tournament is invalid (same as source, finalized, or outside its active window)",
    },
    {
      code: 6032,
      name: "RMatchupMismatch",
      msg: "r_matchup must not be passed once the matchup type is already revealed in the Game account",
    },
    {
      code: 6033,
      name: "XInvalidStatus",
      msg: "Cross-chain match is in the wrong status for this instruction",
    },
    {
      code: 6034,
      name: "XCertMismatch",
      msg: "Certificate terms do not match the recorded escrow state",
    },
    {
      code: 6035,
      name: "XBadSignature",
      msg: "Certificate signature did not recover the expected signer",
    },
    {
      code: 6036,
      name: "XStaleQuote",
      msg: "Rate quote is stale relative to the tranche lock",
    },
    {
      code: 6037,
      name: "XDeadlineNotReached",
      msg: "Deadline has not been reached yet",
    },
    {
      code: 6038,
      name: "XDeadlinePassed",
      msg: "Deadline has already passed",
    },
    {
      code: 6039,
      name: "XPoolInsufficient",
      msg: "Payout pool has insufficient free balance",
    },
    {
      code: 6040,
      name: "XTrancheTooLarge",
      msg: "Tranche exceeds the configured maximum",
    },
    {
      code: 6041,
      name: "XBadConfig",
      msg: "Cross-chain configuration is invalid",
    },
    {
      code: 6042,
      name: "XBadOutcome",
      msg: "Outcome kind is not valid for this settlement path",
    },
    {
      code: 6043,
      name: "InvalidTreasury",
      msg: "Treasury must not be the zero pubkey",
    },
    {
      code: 6044,
      name: "TournamentEndsInPast",
      msg: "Tournament end_time must be in the future",
    },
  ],
  types: [
    {
      name: "CertLegArg",
      docs: [
        "One settlement leg, as an instruction argument. Mirrors",
        "`cs::CertLeg`; converted to it purely for canonical encoding.",
      ],
      type: {
        kind: "struct",
        fields: [
          {
            name: "chain_tag",
            type: {
              array: ["u8", 32],
            },
          },
          {
            name: "contract",
            type: {
              array: ["u8", 32],
            },
          },
          {
            name: "player",
            type: {
              array: ["u8", 32],
            },
          },
          {
            name: "session_key",
            type: {
              array: ["u8", 20],
            },
          },
          {
            name: "stake",
            type: "u128",
          },
          {
            name: "tranche",
            type: "u128",
          },
        ],
      },
    },
    {
      name: "CheckpointArg",
      docs: ["Co-signed transcript checkpoint, as an instruction argument."],
      type: {
        kind: "struct",
        fields: [
          {
            name: "match_live_digest",
            type: {
              array: ["u8", 32],
            },
          },
          {
            name: "step_count",
            type: "u8",
          },
          {
            name: "p1_commit",
            type: {
              array: ["u8", 32],
            },
          },
          {
            name: "p2_commit",
            type: {
              array: ["u8", 32],
            },
          },
          {
            name: "p1_guess",
            type: "u8",
          },
          {
            name: "p2_guess",
            type: "u8",
          },
          {
            name: "first_committer",
            type: "u8",
          },
          {
            name: "matchup_type",
            type: "u8",
          },
          {
            name: "transcript_hash",
            type: {
              array: ["u8", 32],
            },
          },
          {
            name: "r_matchup",
            docs: [
              "Matchup-type reveal preimage; bound to the cert's commitment on",
              "terminal checkpoints (see `verify_matchup_binding`). 0 when unused.",
            ],
            type: {
              array: ["u8", 32],
            },
          },
        ],
      },
    },
    {
      name: "ConfigUpdated",
      type: {
        kind: "struct",
        fields: [
          {
            name: "authority",
            type: "pubkey",
          },
          {
            name: "treasury_split_bps",
            type: "u16",
          },
        ],
      },
    },
    {
      name: "CreateXMatchArgs",
      docs: [
        "Args for `create_xmatch`, bundled so the instruction stays within the",
        "argument-count budget.",
      ],
      type: {
        kind: "struct",
        fields: [
          {
            name: "tournament_id",
            type: "u64",
          },
          {
            name: "player_is_p1",
            type: "bool",
          },
          {
            name: "session_key",
            type: {
              array: ["u8", 20],
            },
          },
          {
            name: "counter_session_key",
            type: {
              array: ["u8", 20],
            },
          },
          {
            name: "stake_lamports",
            type: "u64",
          },
          {
            name: "fund_deadline",
            type: "i64",
          },
          {
            name: "match_deadline",
            type: "i64",
          },
        ],
      },
    },
    {
      name: "Game",
      type: {
        kind: "struct",
        fields: [
          {
            name: "game_id",
            type: "u64",
          },
          {
            name: "tournament_id",
            type: "u64",
          },
          {
            name: "player_one",
            type: "pubkey",
          },
          {
            name: "player_two",
            type: "pubkey",
          },
          {
            name: "state",
            type: {
              defined: {
                name: "GameState",
              },
            },
          },
          {
            name: "stake_lamports",
            type: "u64",
          },
          {
            name: "p1_commit",
            type: {
              array: ["u8", 32],
            },
          },
          {
            name: "p2_commit",
            type: {
              array: ["u8", 32],
            },
          },
          {
            name: "p1_guess",
            type: "u8",
          },
          {
            name: "p2_guess",
            type: "u8",
          },
          {
            name: "first_committer",
            type: "u8",
          },
          {
            name: "p1_commit_slot",
            type: "u64",
          },
          {
            name: "p2_commit_slot",
            type: "u64",
          },
          {
            name: "commit_timeout_slots",
            type: "u64",
          },
          {
            name: "created_at",
            type: "i64",
          },
          {
            name: "resolved_at",
            type: "i64",
          },
          {
            name: "activated_at_slot",
            docs: [
              "Solana slot at which the game entered Active state (set by join_game).",
              "Used for Active-state commit timeout (neither player commits).",
            ],
            type: "u64",
          },
          {
            name: "matchup_commitment",
            docs: [
              "SHA-256 commitment of the matchup type preimage (set at create_game by matchmaker co-sign).",
              "Verified during the first reveal to extract the actual matchup_type.",
            ],
            type: {
              array: ["u8", 32],
            },
          },
          {
            name: "matchup_type",
            docs: [
              "0 = same team (homogenous), 1 = different teams (heterogeneous), 255 = unset.",
              "Set to 255 at creation, resolved during first reveal via matchup_commitment verification.",
            ],
            type: "u8",
          },
          {
            name: "bump",
            type: "u8",
          },
        ],
      },
    },
    {
      name: "GameCancelled",
      docs: [
        "Emitted by `refund_pending` when an un-joined Pending game is cancelled and",
        "P1's stake is refunded (mirrors the EVM `GameCancelled`). Powers a log-based",
        "metric / alarm on stuck-Pending recovery.",
      ],
      type: {
        kind: "struct",
        fields: [
          {
            name: "game_id",
            type: "u64",
          },
          {
            name: "player_one",
            type: "pubkey",
          },
          {
            name: "refund_lamports",
            type: "u64",
          },
        ],
      },
    },
    {
      name: "GameCounter",
      type: {
        kind: "struct",
        fields: [
          {
            name: "count",
            type: "u64",
          },
          {
            name: "bump",
            type: "u8",
          },
        ],
      },
    },
    {
      name: "GameCreated",
      type: {
        kind: "struct",
        fields: [
          {
            name: "game_id",
            type: "u64",
          },
          {
            name: "tournament_id",
            type: "u64",
          },
          {
            name: "player_one",
            type: "pubkey",
          },
          {
            name: "stake_lamports",
            type: "u64",
          },
        ],
      },
    },
    {
      name: "GameResolved",
      type: {
        kind: "struct",
        fields: [
          {
            name: "game_id",
            type: "u64",
          },
          {
            name: "p1_guess",
            type: "u8",
          },
          {
            name: "p2_guess",
            type: "u8",
          },
          {
            name: "p1_return",
            type: "u64",
          },
          {
            name: "p2_return",
            type: "u64",
          },
          {
            name: "tournament_gain",
            type: "u64",
          },
          {
            name: "treasury_gain",
            type: "u64",
          },
        ],
      },
    },
    {
      name: "GameStarted",
      type: {
        kind: "struct",
        fields: [
          {
            name: "game_id",
            type: "u64",
          },
          {
            name: "tournament_id",
            type: "u64",
          },
          {
            name: "player_one",
            type: "pubkey",
          },
          {
            name: "player_two",
            type: "pubkey",
          },
        ],
      },
    },
    {
      name: "GameState",
      docs: [
        "Game lifecycle state machine.",
        "",
        "```text",
        "--(create_game)--> Pending",
        "Pending --(join_game)--> Active",
        "Pending --(refund_pending: P2 never joined, window elapsed)--> Cancelled (P1 refunded, account closed)",
        "Active  --(commit_guess: first)--> Committing",
        "Active  --(resolve_timeout: neither commits)--> Resolved",
        "Committing --(commit_guess: second)--> Revealing",
        "Committing --(resolve_timeout)--> Resolved",
        "Revealing  --(reveal_guess: both)--> Resolved",
        "Revealing  --(resolve_timeout)--> Resolved",
        "Resolved   --(close_game)--> [account closed]",
        "```",
      ],
      type: {
        kind: "enum",
        variants: [
          {
            name: "Pending",
          },
          {
            name: "Active",
          },
          {
            name: "Committing",
          },
          {
            name: "Revealing",
          },
          {
            name: "Resolved",
          },
          {
            name: "Cancelled",
          },
        ],
      },
    },
    {
      name: "GlobalConfig",
      docs: [
        "Singleton PDA storing protocol-level configuration.",
        'Seeds: `["global_config"]`',
      ],
      type: {
        kind: "struct",
        fields: [
          {
            name: "authority",
            docs: ["Governance authority (EOA for v1)."],
            type: "pubkey",
          },
          {
            name: "matchmaker",
            docs: ["Authorized matchmaker that gates `create_game`."],
            type: "pubkey",
          },
          {
            name: "treasury",
            docs: ["DAO treasury address for losing stake split."],
            type: "pubkey",
          },
          {
            name: "treasury_split_bps",
            docs: [
              "Portion of losing stakes sent to treasury (basis points).",
              "Default 5000 = 50%. Bounded to [2000, 8000].",
            ],
            type: "u16",
          },
          {
            name: "bump",
            type: "u8",
          },
          {
            name: "stake_lamports",
            docs: [
              "Per-game stake in lamports.",
              "",
              "APPENDED AFTER `bump` deliberately: the 107-byte v1 layout stays a",
              "byte-exact prefix, so `migrate_global_config` only has to grow the",
              "account and write this one field. An account survey on 2026-08-03 found",
              "exactly ONE GlobalConfig on mainnet and one on devnet, both at 107",
              "bytes, so the migration is a single call per network.",
              "",
              "This used to be the compile-time `FIXED_STAKE_LAMPORTS`, which meant",
              "re-pegging Solana required a PROGRAM UPGRADE while the EVM chains only",
              "needed `setConfig`. That asymmetry is why Solana sat at $3.64 against a",
              "$5 EVM anchor: the cheap change was made and the expensive one was not.",
            ],
            type: "u64",
          },
        ],
      },
    },
    {
      name: "GuessCommitted",
      type: {
        kind: "struct",
        fields: [
          {
            name: "game_id",
            type: "u64",
          },
          {
            name: "player",
            type: "pubkey",
          },
          {
            name: "commit_slot",
            type: "u64",
          },
        ],
      },
    },
    {
      name: "GuessRevealed",
      type: {
        kind: "struct",
        fields: [
          {
            name: "game_id",
            type: "u64",
          },
          {
            name: "player",
            type: "pubkey",
          },
        ],
      },
    },
    {
      name: "MatchLiveCertArg",
      docs: ["Match-live certificate, as an instruction argument."],
      type: {
        kind: "struct",
        fields: [
          {
            name: "match_id",
            type: {
              array: ["u8", 32],
            },
          },
          {
            name: "tournament_id",
            type: "u64",
          },
          {
            name: "matchup_commitment",
            type: {
              array: ["u8", 32],
            },
          },
          {
            name: "leg_a",
            type: {
              defined: {
                name: "CertLegArg",
              },
            },
          },
          {
            name: "leg_b",
            type: {
              defined: {
                name: "CertLegArg",
              },
            },
          },
          {
            name: "quote_timestamp",
            type: "u64",
          },
          {
            name: "quote_max_age_secs",
            type: "u32",
          },
          {
            name: "match_deadline",
            type: "u64",
          },
          {
            name: "claim_window_secs",
            type: "u32",
          },
          {
            name: "a_is_p1",
            type: "u8",
          },
        ],
      },
    },
    {
      name: "MatchLiveCertNoA",
      docs: [
        "Match-live certificate WITHOUT leg A \u2014 used by `settle_xmatch` to stay",
        "under Solana's 1232-byte transaction limit. Leg A is the Solana leg and",
        "is fully determined by on-chain match state, so the program reconstructs",
        "it rather than carrying its 148 bytes over the wire. This also removes",
        "the tamper surface: leg A is authoritative from state, never the caller.",
      ],
      type: {
        kind: "struct",
        fields: [
          {
            name: "match_id",
            type: {
              array: ["u8", 32],
            },
          },
          {
            name: "tournament_id",
            type: "u64",
          },
          {
            name: "matchup_commitment",
            type: {
              array: ["u8", 32],
            },
          },
          {
            name: "leg_b",
            type: {
              defined: {
                name: "CertLegArg",
              },
            },
          },
          {
            name: "quote_timestamp",
            type: "u64",
          },
          {
            name: "quote_max_age_secs",
            type: "u32",
          },
          {
            name: "match_deadline",
            type: "u64",
          },
          {
            name: "claim_window_secs",
            type: "u32",
          },
          {
            name: "a_is_p1",
            type: "u8",
          },
        ],
      },
    },
    {
      name: "OutcomeCertArg",
      docs: ["Outcome certificate, as an instruction argument."],
      type: {
        kind: "struct",
        fields: [
          {
            name: "match_id",
            type: {
              array: ["u8", 32],
            },
          },
          {
            name: "match_live_digest",
            type: {
              array: ["u8", 32],
            },
          },
          {
            name: "outcome_kind",
            type: "u8",
          },
          {
            name: "step_count",
            type: "u8",
          },
          {
            name: "p1_guess",
            type: "u8",
          },
          {
            name: "p2_guess",
            type: "u8",
          },
          {
            name: "first_committer",
            type: "u8",
          },
          {
            name: "matchup_type",
            type: "u8",
          },
          {
            name: "transcript_hash",
            type: {
              array: ["u8", 32],
            },
          },
        ],
      },
    },
    {
      name: "PlayerProfile",
      type: {
        kind: "struct",
        fields: [
          {
            name: "wallet",
            type: "pubkey",
          },
          {
            name: "tournament_id",
            type: "u64",
          },
          {
            name: "wins",
            type: "u64",
          },
          {
            name: "total_games",
            type: "u64",
          },
          {
            name: "score",
            type: "u64",
          },
          {
            name: "claimed",
            type: "bool",
          },
          {
            name: "bump",
            type: "u8",
          },
        ],
      },
    },
    {
      name: "RewardClaimed",
      type: {
        kind: "struct",
        fields: [
          {
            name: "tournament_id",
            type: "u64",
          },
          {
            name: "player",
            type: "pubkey",
          },
          {
            name: "amount",
            type: "u64",
          },
        ],
      },
    },
    {
      name: "SessionAuthority",
      docs: [
        "Ephemeral session authority that lets a player delegate transaction signing",
        "to a temporary keypair. The player signs once to create the session; all",
        "subsequent game instructions can be signed by the session key instead.",
        "",
        'PDA seeds: `["game_session", player, session_key]`',
      ],
      type: {
        kind: "struct",
        fields: [
          {
            name: "player",
            docs: ["The real wallet that created this session."],
            type: "pubkey",
          },
          {
            name: "session_key",
            docs: ["The ephemeral keypair's public key."],
            type: "pubkey",
          },
          {
            name: "expires_at",
            docs: ["Unix timestamp after which the session is invalid."],
            type: "i64",
          },
          {
            name: "bump",
            type: "u8",
          },
        ],
      },
    },
    {
      name: "SessionClosed",
      type: {
        kind: "struct",
        fields: [
          {
            name: "player",
            type: "pubkey",
          },
          {
            name: "session_key",
            type: "pubkey",
          },
        ],
      },
    },
    {
      name: "SessionCreated",
      type: {
        kind: "struct",
        fields: [
          {
            name: "player",
            type: "pubkey",
          },
          {
            name: "session_key",
            type: "pubkey",
          },
          {
            name: "expires_at",
            type: "i64",
          },
        ],
      },
    },
    {
      name: "StakeConfigured",
      docs: [
        "Emitted when the authority re-pegs the per-game stake. Carries both values",
        "so an indexer can reconstruct the peg history without diffing account state.",
      ],
      type: {
        kind: "struct",
        fields: [
          {
            name: "previous_lamports",
            type: "u64",
          },
          {
            name: "new_lamports",
            type: "u64",
          },
          {
            name: "authority",
            type: "pubkey",
          },
        ],
      },
    },
    {
      name: "StakeDeposited",
      type: {
        kind: "struct",
        fields: [
          {
            name: "player",
            type: "pubkey",
          },
          {
            name: "tournament_id",
            type: "u64",
          },
          {
            name: "amount",
            type: "u64",
          },
        ],
      },
    },
    {
      name: "StakeEscrow",
      docs: [
        "Per-player escrow that holds staked lamports while the player is in the",
        "matchmaking queue. Created by `deposit_stake`, consumed by `create_game`",
        "or `join_game`, refunded by `withdraw_stake`.",
        "",
        'PDA seeds: `["escrow", tournament_id, player]`',
      ],
      type: {
        kind: "struct",
        fields: [
          {
            name: "player",
            type: "pubkey",
          },
          {
            name: "tournament_id",
            type: "u64",
          },
          {
            name: "amount",
            type: "u64",
          },
          {
            name: "consumed",
            docs: [
              "True once the escrow has been consumed by a create_game or join_game",
              "instruction. Prevents double-spend if the same escrow PDA is reused.",
            ],
            type: "bool",
          },
          {
            name: "bump",
            type: "u8",
          },
        ],
      },
    },
    {
      name: "StakeWithdrawn",
      type: {
        kind: "struct",
        fields: [
          {
            name: "wallet",
            type: "pubkey",
          },
          {
            name: "tournament_id",
            type: "u64",
          },
          {
            name: "amount",
            type: "u64",
          },
        ],
      },
    },
    {
      name: "TimeoutSlash",
      type: {
        kind: "struct",
        fields: [
          {
            name: "game_id",
            type: "u64",
          },
          {
            name: "slashed_player",
            type: "pubkey",
          },
          {
            name: "slash_amount",
            type: "u64",
          },
        ],
      },
    },
    {
      name: "Tournament",
      type: {
        kind: "struct",
        fields: [
          {
            name: "tournament_id",
            type: "u64",
          },
          {
            name: "authority",
            type: "pubkey",
          },
          {
            name: "start_time",
            type: "i64",
          },
          {
            name: "end_time",
            type: "i64",
          },
          {
            name: "prize_lamports",
            type: "u64",
          },
          {
            name: "game_count",
            type: "u64",
          },
          {
            name: "finalized",
            type: "bool",
          },
          {
            name: "prize_snapshot",
            type: "u64",
          },
          {
            name: "merkle_root",
            type: {
              array: ["u8", 32],
            },
          },
          {
            name: "bump",
            type: "u8",
          },
        ],
      },
    },
    {
      name: "TournamentCreated",
      type: {
        kind: "struct",
        fields: [
          {
            name: "tournament_id",
            type: "u64",
          },
          {
            name: "start_time",
            type: "i64",
          },
          {
            name: "end_time",
            type: "i64",
          },
        ],
      },
    },
    {
      name: "TournamentFinalized",
      type: {
        kind: "struct",
        fields: [
          {
            name: "tournament_id",
            type: "u64",
          },
          {
            name: "prize_snapshot",
            type: "u64",
          },
          {
            name: "merkle_root",
            type: {
              array: ["u8", 32],
            },
          },
        ],
      },
    },
    {
      name: "UnclaimedSwept",
      type: {
        kind: "struct",
        fields: [
          {
            name: "src_tournament_id",
            type: "u64",
          },
          {
            name: "dest_tournament_id",
            type: "u64",
          },
          {
            name: "amount",
            type: "u64",
          },
        ],
      },
    },
    {
      name: "XChainMatch",
      docs: [
        "Per-match cross-chain escrow + claim state. Doubles as the stake",
        "lamport vault. Cross-chain deadlines are unix seconds (NOT slots) so",
        "the certificate means the same thing on both legs.",
        "",
        'Seeds: `["xmatch", match_id]`',
      ],
      type: {
        kind: "struct",
        fields: [
          {
            name: "match_id",
            type: {
              array: ["u8", 32],
            },
          },
          {
            name: "tournament_id",
            type: "u64",
          },
          {
            name: "player",
            docs: ["Local player wallet (payout/refund recipient)."],
            type: "pubkey",
          },
          {
            name: "player_is_p1",
            docs: ["1 when this player is P1 in the payoff matrix, else 0."],
            type: "u8",
          },
          {
            name: "session_key",
            docs: [
              "This player's per-match secp256k1 session key (eth-address form).",
            ],
            type: {
              array: ["u8", 20],
            },
          },
          {
            name: "counter_session_key",
            docs: ["Counterparty's session key (cert cross-check)."],
            type: {
              array: ["u8", 20],
            },
          },
          {
            name: "stake_lamports",
            type: "u64",
          },
          {
            name: "tranche_lamports",
            docs: [
              "0 until locked. Max cross-chain payout from the float pool.",
            ],
            type: "u64",
          },
          {
            name: "fund_deadline",
            type: "i64",
          },
          {
            name: "match_deadline",
            type: "i64",
          },
          {
            name: "locked_at",
            docs: ["Quote-freshness anchor; set at lock."],
            type: "i64",
          },
          {
            name: "claim_window_end",
            docs: [
              "Anchored claim window end: match_deadline + claim_window_secs.",
            ],
            type: "i64",
          },
          {
            name: "claim_window_secs",
            type: "u32",
          },
          {
            name: "match_live_digest",
            type: {
              array: ["u8", 32],
            },
          },
          {
            name: "best_step_count",
            docs: ["255 once an equivocation verdict is final."],
            type: "u8",
          },
          {
            name: "best_outcome_kind",
            type: "u8",
          },
          {
            name: "local_equivocated",
            docs: [
              "Equivocation flags \u2014 verdict is order-independent (mirrors the EVM",
              "fix: never inferred from best_outcome_kind).",
            ],
            type: "bool",
          },
          {
            name: "counter_equivocated",
            type: "bool",
          },
          {
            name: "status",
            type: {
              defined: {
                name: "XChainStatus",
              },
            },
          },
          {
            name: "bump",
            type: "u8",
          },
        ],
      },
    },
    {
      name: "XChainStatus",
      docs: [
        "Cross-chain match state machine (the Solana leg; mirrors the EVM",
        "CrossChainGame contract). Each chain holds one player's native-coin",
        "stake and settles only its own leg of a co-signed certificate.",
        "",
        "```text",
        "None --create_xmatch--> Funded --lock_xtranche--> Locked --settle_xmatch--> Settled",
        "(player+stake)    |        (operator)        |       (terminal 3+3-sig)",
        "+matchmaker       |                          |",
        "|                          +--open_xclaim--> Claiming",
        "|                          |   (2-sig checkpoint)  |",
        "|                          |   supersede / equivocation",
        "|                          |                       |",
        "|                          |   settle_xclaim (window closed)",
        "|                          |                       v",
        "|                          |                  ClaimSettled",
        "Funded --refund_xmatch_nocert--> RefundedNoCert      |",
        "(t > fund_deadline,                          +--refund_xmatch_timeout-->",
        "never locked)                                  RefundedTimeout",
        "(t > match_deadline +",
        "max_claim_window + 2*skew)",
        "```",
      ],
      type: {
        kind: "enum",
        variants: [
          {
            name: "None",
          },
          {
            name: "Funded",
          },
          {
            name: "Locked",
          },
          {
            name: "Claiming",
          },
          {
            name: "Settled",
          },
          {
            name: "ClaimSettled",
          },
          {
            name: "RefundedNoCert",
          },
          {
            name: "RefundedTimeout",
          },
        ],
      },
    },
    {
      name: "XClaimOpened",
      type: {
        kind: "struct",
        fields: [
          {
            name: "match_id",
            type: {
              array: ["u8", 32],
            },
          },
          {
            name: "claimed_outcome",
            type: "u8",
          },
          {
            name: "claim_window_end",
            type: "i64",
          },
        ],
      },
    },
    {
      name: "XClaimSuperseded",
      type: {
        kind: "struct",
        fields: [
          {
            name: "match_id",
            type: {
              array: ["u8", 32],
            },
          },
          {
            name: "step_count",
            type: "u8",
          },
          {
            name: "claimed_outcome",
            type: "u8",
          },
        ],
      },
    },
    {
      name: "XEquivocationProven",
      type: {
        kind: "struct",
        fields: [
          {
            name: "match_id",
            type: {
              array: ["u8", 32],
            },
          },
          {
            name: "new_best_outcome",
            type: "u8",
          },
        ],
      },
    },
    {
      name: "XMatchCreated",
      type: {
        kind: "struct",
        fields: [
          {
            name: "match_id",
            type: {
              array: ["u8", 32],
            },
          },
          {
            name: "player",
            type: "pubkey",
          },
          {
            name: "stake_lamports",
            type: "u64",
          },
        ],
      },
    },
    {
      name: "XMatchRefunded",
      type: {
        kind: "struct",
        fields: [
          {
            name: "match_id",
            type: {
              array: ["u8", 32],
            },
          },
          {
            name: "to_player",
            type: "u64",
          },
        ],
      },
    },
    {
      name: "XMatchSettled",
      type: {
        kind: "struct",
        fields: [
          {
            name: "match_id",
            type: {
              array: ["u8", 32],
            },
          },
          {
            name: "outcome_kind",
            type: "u8",
          },
          {
            name: "to_player",
            type: "u64",
          },
          {
            name: "to_treasury",
            type: "u64",
          },
        ],
      },
    },
    {
      name: "XPayoutPool",
      docs: [
        "Singleton operator float pool \u2014 the on-chain collateral that pays",
        "cross-chain winners. Lamport vault tracked by free/locked split.",
        "",
        'Seeds: `["xpool"]`',
      ],
      type: {
        kind: "struct",
        fields: [
          {
            name: "operator",
            docs: ["Operator (float manager + tranche locker)."],
            type: "pubkey",
          },
          {
            name: "operator_signer",
            docs: [
              "Dedicated secp256k1 certificate signer (eth-address form). NOT the",
              "operator key, NOT the program upgrade authority.",
            ],
            type: {
              array: ["u8", 20],
            },
          },
          {
            name: "free_lamports",
            type: "u64",
          },
          {
            name: "locked_lamports",
            type: "u64",
          },
          {
            name: "max_tranche_lamports",
            type: "u64",
          },
          {
            name: "max_claim_window_secs",
            type: "u32",
          },
          {
            name: "skew_margin_secs",
            type: "u32",
          },
          {
            name: "bump",
            type: "u8",
          },
        ],
      },
    },
    {
      name: "XPoolDeposited",
      type: {
        kind: "struct",
        fields: [
          {
            name: "funder",
            type: "pubkey",
          },
          {
            name: "amount",
            type: "u64",
          },
        ],
      },
    },
    {
      name: "XPoolWithdrawn",
      type: {
        kind: "struct",
        fields: [
          {
            name: "amount",
            type: "u64",
          },
        ],
      },
    },
    {
      name: "XTrancheLocked",
      type: {
        kind: "struct",
        fields: [
          {
            name: "match_id",
            type: {
              array: ["u8", 32],
            },
          },
          {
            name: "tranche_lamports",
            type: "u64",
          },
        ],
      },
    },
  ],
} as unknown as CoordinationGame;
