# Coordination Game — On-Chain Program Spec

Solana program (Anchor) for the coordination game. For game design rationale, player psychology, and economic model, see `swarm/coordination-game/CLAUDE.md`. For shared code standards, see the root `CLAUDE.md`. This file covers implementation-specific details only.

---

## Overview

A commit-reveal 1v1 game where players stake SOL, chat anonymously, then guess whether their opponent is on the same team or a different team. Losing stakes flow to the DAO treasury and a tournament prize pool.

Uses `init_if_needed` for `PlayerProfile` only (player pays, idempotent). All other accounts use `init`.

---

## State Machine

```
         ──(create_game)──► Pending
Pending ──(join_game)──► Active
Active ──(commit_guess: first)──► Committing
Active ──(resolve_timeout: neither commits)──► Resolved (both forfeit)
Committing ──(commit_guess: second)──► Revealing
Committing ──(resolve_timeout)──► Resolved
Revealing ──(reveal_guess: both revealed)──► Resolved
Revealing ──(resolve_timeout)──► Resolved
Resolved ──(close_game)──► [account closed]
```

---

## Accounts

| Account | PDA Seeds | Purpose |
|---|---|---|
| `GameCounter` | `["game_counter"]` | Singleton: monotonic game ID counter |
| `GlobalConfig` | `["global_config"]` | Singleton: authority, matchmaker, treasury, treasury_split_bps |
| `Tournament` | `["tournament", tournament_id (8-byte LE)]` | Tournament data + prize pool lamport vault |
| `Game` | `["game", game_id (8-byte LE)]` | Game data + staked lamport vault |
| `PlayerProfile` | `["player", tournament_id (8-byte LE), wallet]` | Per-tournament player stats: wins, total_games, score |
| `Escrow` | `["escrow", tournament_id, player]` | Per-tournament player stake escrow |
| `SessionAuthority` | `["game_session", player, session_key]` | 24-hour ephemeral session key delegation |

See `state/*.rs` for full field layouts.

---

## Instructions

### Setup & Config
- `initialize()` — one-time GameCounter setup
- `initialize_config(treasury_split_bps)` — one-time GlobalConfig setup with authority, matchmaker, treasury
- `update_config(treasury_split_bps, treasury, matchmaker, new_authority)` — authority updates config

### Tournament
- `create_tournament(tournament_id, start_time, end_time)` — anyone can create
- `finalize_tournament(merkle_root)` — authority posts merkle root of (player, entitlement) pairs after end_time
- `claim_reward(amount, proof)` — player claims prize via merkle proof; requires finalized + minimum 5 games

### Stake Management
- `deposit_stake()` — player deposits SOL to tournament escrow PDA
- `withdraw_stake()` — player withdraws unused escrow balance

### Game Lifecycle
- `create_game(stake_lamports, matchup_commitment)` — player + matchmaker co-sign; matchmaker attests commitment is legitimate
- `join_game()` — second player joins; stakes transferred to Game PDA; game becomes Active
- `commit_guess(commitment)` — player commits SHA-256 hash of guess preimage
- `reveal_guess(r, r_matchup)` — player reveals guess; first revealer also reveals matchup type; if both revealed, game resolves
- `resolve_timeout()` — permissionless crank for timed-out games
- `close_game()` — permissionless; closes Resolved game account

### Session Key Instructions
- `create_player_session()` — creates 24-hour ephemeral session key
- `close_player_session()` — player closes session
- `close_session_by_delegate()` — delegate can close own session
- `deposit_stake_session()` — session-delegated deposit_stake
- `create_game_session(stake_lamports, matchup_commitment)` — session-delegated create_game
- `join_game_session()` — session-delegated join_game
- `commit_guess_session(commitment)` — session-delegated commit_guess
- `reveal_guess_session(r, r_matchup)` — session-delegated reveal_guess

---

## Payoff Logic

### Same Team (matchup_type = 0)
- Both correct → both keep stake (S, S). Tournament gains 0. Both earn a win.
- One correct, one wrong → correct gets 0.5S, wrong gets 0. Tournament gains 1.5S.
- Both wrong → both forfeit. Tournament gains 2S.

### Different Teams (matchup_type = 1)
- One correct → winner takes full pot (2S, 0). Tournament gains 0.
- Both correct → first committer takes full pot (2S, 0). Tournament gains 0.
- Both wrong → both forfeit. Tournament gains 2S.

**Tournament gain split:** treasury gets `gain * treasury_split_bps / 10_000`, prize pool gets remainder. `treasury_split_bps` bounded to [2000, 8000].

**Lamport conservation:** `p1_return + p2_return + tournament_gain == 2 * stake_lamports` asserted on every resolution.

---

## Timeouts

| Stage | Timeout |
|---|---|
| Active (neither commits) | 7,200 slots (~1 hour) |
| Committing (one committed) | 7,200 slots (~1 hour) |
| Revealing (one revealed) | 14,400 slots (~2 hours) |

**Timeout resolution:**
- Active: both forfeit, stakes go to pool/treasury split
- Committing: committer wins full pot (2S), non-committer slashed
- Revealing: revealer wins full pot (2S), non-revealer slashed

---

## Merkle Proof Verification

`claim_reward` verifies inclusion proofs against `tournament.merkle_root` using `keccak::hashv` with domain separation: 0x00 prefix for leaves, 0x01 for internal nodes, sorted children. Requires minimum 5 games played.

---

## Scoring

`PlayerProfile.score = wins * wins / total_games` (integer division, updated after each resolution).

**Win definition:** A player earns a win when they guess correctly. In same-team both-correct, BOTH players earn a win.
