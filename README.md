# Coordination Game — Smart Contracts

Solana program for the Coordination Game: an anonymous 1v1 social deduction game where players stake lamports and guess whether their opponent is human or AI.

Built with [Anchor](https://www.anchor-lang.com/) on Solana.

## Overview

Players are matched anonymously, chat, then each submits a guess — *human* or *AI* — via a commit-reveal scheme. Stakes are held in escrow and redistributed based on the payoff matrix when both guesses are revealed (or a timeout fires).

The full game design, payoff logic, and incentive rationale are documented in [CLAUDE.md](./CLAUDE.md).

## Prerequisites

- [Rust](https://rustup.rs/) (stable)
- [Solana CLI](https://docs.solanalabs.com/cli/install)
- [Anchor CLI](https://www.anchor-lang.com/docs/installation) v0.32.1
- Node.js 20+

## Local Development

```sh
# Build the program
make build

# Run the full test suite against a local validator
make test

# Clean build artifacts
make clean
```

`anchor test` starts a local validator, runs all end-to-end tests in `tests/coordination.ts`, then stops the validator.

## Program Instructions

| Instruction | Signer | Description |
|---|---|---|
| `initialize` | any | One-time setup; creates the global `GameCounter` PDA |
| `create_tournament` | authority | Create a time-bounded tournament |
| `create_game` | player 1 | Create a game and lock stake |
| `join_game` | player 2 | Join a game and lock stake |
| `commit_guess` | player | Submit a SHA-256 commitment to a guess |
| `reveal_guess` | player | Reveal the preimage; resolves game when both reveal |
| `resolve_timeout` | anyone | Slash the non-participating player after timeout |
| `finalize_tournament` | anyone | Snapshot scores after tournament end time |
| `claim_reward` | player | Claim proportional prize from the tournament pool |
| `close_game` | anyone | Reclaim rent from a resolved game account |

## Commit-Reveal Scheme

Guesses are submitted in two phases to prevent Player 2 from copying Player 1's on-chain guess:

**Commit:** Generate a random 32-byte preimage `R`. Encode the guess in the last bit: `R[31] = (R[31] & 0xFE) | guess`. Submit `commitment = SHA-256(R)`.

**Reveal:** Submit `R`. The program verifies `SHA-256(R) == commitment` and extracts `guess = R[31] & 1`.

## Deployment

The CI pipeline (`deploy` job in `.github/workflows/program.yml`) deploys to devnet on every merge to `main`, after all tests pass. Deployment requires `SOLANA_DEPLOY_KEYPAIR` and `ANCHOR_PROGRAM_KEYPAIR` GitHub secrets.

Program ID: `2qqVk7kUqffnahiJpcQJCsSd8ErbEUgKTgCn1zYsw64P`
