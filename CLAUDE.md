# Smart Contracts — Implementation Context

Solana program built with Anchor. Read the root `CLAUDE.md` before this file for project context and code standards.

### Code Standard Clarifications (Solana)

**Rule 3 — unbounded resource consumption:** On-chain, collection sizes must be statically bounded. Never use `Vec::with_capacity(n)` where `n` comes from instruction input — every caller can force arbitrary allocation and exhaust compute budget.

---

## File and Module Structure

```
smartcontracts/
├── Anchor.toml
├── Makefile                            # make build / test / clean
├── Cargo.toml                          # workspace root
├── programs/
│   └── coordination/
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs                  # program entrypoint, declare_id!, module re-exports
│           ├── state/
│           │   ├── mod.rs
│           │   ├── game.rs             # Game account, GameState enum
│           │   ├── player.rs           # PlayerProfile account
│           │   └── tournament.rs       # Tournament account
│           ├── instructions/
│           │   ├── mod.rs
│           │   ├── initialize.rs       # one-time global setup (GameCounter)
│           │   ├── create_tournament.rs
│           │   ├── create_game.rs      # player 1 creates and stakes
│           │   ├── join_game.rs        # player 2 joins and stakes
│           │   ├── commit_guess.rs
│           │   ├── reveal_guess.rs
│           │   ├── resolve_timeout.rs
│           │   ├── close_game.rs       # permissionless rent reclaim on resolved game
│           │   ├── finalize_tournament.rs
│           │   └── claim_reward.rs
│           ├── errors.rs
│           └── events.rs
└── tests/
    └── coordination.ts                 # end-to-end tests
```

Instruction handlers must be thin — validate, delegate to a pure function, emit event. Business logic lives in pure functions, not handlers. Each instruction is its own file.

---

## Dependencies

```toml
[dependencies]
anchor-lang = { version = "0.32.1", features = ["init-if-needed"] }
solana-sha256-hasher = "2"      # sol_sha256 syscall binding for commit verification
```

For hashing: use Solana's native `sol_sha256` syscall via `solana-sha256-hasher` rather than the `sha2` crate — lower compute unit cost.

---

## Account Structures

### `GameCounter`

Singleton PDA. Seeds: `["game_counter"]`

```
discriminator:    8 bytes
count:            u64      incremented on each create_game; used as game_id
bump:             u8
```

### `Tournament`

PDA seeds: `["tournament", tournament_id: u64 as 8-byte LE]`

```
tournament_id:            u64
authority:                Pubkey    informational only — no admin power post-creation
start_time:               i64       Unix timestamp
end_time:                 i64       Unix timestamp
prize_lamports:           u64       accumulates losing stakes during the tournament
game_count:               u64       total resolved games
finalized:                bool      set by finalize_tournament after end_time
prize_snapshot:           u64       prize_lamports frozen at finalization
total_score_snapshot:     u64       sum of all player scores frozen at finalization
bump:                     u8
```

The Tournament PDA is both data store and lamport vault for the prize pool.

### `PlayerProfile`

PDA seeds: `["player", tournament_id: u64 as 8-byte LE, wallet: Pubkey]`

One profile per (wallet, tournament). Created at `create_game` or `join_game` via `init_if_needed`; player pays for creation.

```
wallet:           Pubkey
tournament_id:    u64
wins:             u64      games where player received the +0.9 return
total_games:      u64      resolved games in this tournament
score:            u64      wins * wins / total_games (updated after each resolution)
claimed:          bool     set to true after claim_reward; prevents double-claim
bump:             u8
```

### `Game`

PDA seeds: `["game", game_id: u64 as 8-byte LE]`

```
game_id:                  u64
tournament_id:            u64
player_one:               Pubkey
player_two:               Pubkey    zero-key until second player joins
state:                    u8        GameState enum
stake_lamports:           u64       per-player stake; set at creation, must match exactly at join
p1_commit:                [u8; 32]  SHA-256 commitment, zeroed until set
p2_commit:                [u8; 32]
p1_guess:                 u8        0 = same team, 1 = different team, 255 = unrevealed
p2_guess:                 u8        0 = same team, 1 = different team, 255 = unrevealed
first_committer:          u8        0 = neither, 1 = p1, 2 = p2
p1_commit_slot:           u64       Solana slot at commit time
p2_commit_slot:           u64       Solana slot at commit time
commit_timeout_slots:     u64       set at creation (see Timeouts)
created_at:               i64       Unix timestamp
resolved_at:              i64       0 until resolved
matchup_type:             u8        0 = same team (homogenous), 1 = different teams (heterogeneous); randomly assigned by matchmaker
bump:                     u8
```

The Game PDA holds both players' staked lamports in its own balance. No separate vault account.

---

## State Machine

```
         ──(create_game)──► Pending
Pending ──(join_game)──► Active
Active ──(commit_guess: first player)──► Committing
Committing ──(commit_guess: second player)──► Revealing
Committing ──(resolve_timeout)──► Resolved
Revealing ──(reveal_guess: both revealed)──► Resolved
Revealing ──(resolve_timeout)──► Resolved
Resolved ──(close_game)──► [account closed]
```

Any instruction that receives a game in an invalid state for that instruction returns `InvalidGameState`. All transitions not listed above are rejected with `InvalidStateTransition`.

---

## Instructions

### `initialize`
Signers: `authority` (any wallet)
- One-time program setup; creates the global `GameCounter` PDA
- Must be called once before any games can be created
- No event emitted

### `create_tournament`
Signers: `authority` (any wallet)
- Validate `end_time > start_time` and `end_time > Clock::now()` (tournament must not have already ended)
- `start_time` may be in the past to create an already-started tournament
- Initialize Tournament; `prize_lamports = 0`, `finalized = false`
- Emit `TournamentCreated`

### `create_game`
Signers: `player` (becomes player one)
- Assert `Clock::now()` is within tournament window
- Assign `game_id = GameCounter.count`, then increment counter
- Init Game PDA; set `player_one`, `stake_lamports`, `state = Pending`
- Init `PlayerProfile` for player 1 via `init_if_needed` (player pays)
- Transfer `stake_lamports` from player to Game PDA
- Emit `GameCreated`

### `join_game`
Signers: `player` (becomes player two)
- Assert `state == Pending`
- Assert `player != player_one`
- Assert `Clock::now()` is within tournament window
- Set `player_two`, `state = Active`
- Init `PlayerProfile` for player 2 via `init_if_needed` (player pays)
- Transfer `stake_lamports` from player 2 to Game PDA
- Emit `GameStarted`

### `commit_guess`
Signers: `player`
- Assert `state == Active || state == Committing`
- Assert caller is a participant and has not already committed
- Store commitment; record commit slot
- Set `first_committer` if not yet set
- Transition: first commit → `Committing`; second commit → `Revealing`
- Emit `GuessCommitted`

Input: `commitment: [u8; 32]` — the SHA-256 hash of the player's random preimage `R`.

### `reveal_guess`
Signers: `player`
- Assert `state == Revealing`
- Assert caller has not already revealed
- Recompute `sol_sha256(R)` and assert it matches the stored commitment
- Extract guess: `R[31] & 1` (last bit of preimage encodes the guess)
- Store guess
- If both players have revealed: call `resolve_game` (pure function)
- Emit `GuessRevealed`

Input: `r: [u8; 32]` — the 32-byte random preimage. Client sets `r[31] = (r[31] & 0xFE) | guess` before hashing, so the last bit always encodes the guess.

### `resolve_timeout`
Permissionless — any wallet can call.
- Assert `state == Committing || state == Revealing`
- Assert timeout has elapsed (see Timeouts)
- If one player failed to participate: slash their stake to tournament; return other player's stake; credit the active player a win
- If both players failed to reveal (`Revealing` with neither revealed): both stakes go to tournament; neither gets a win
- Update both PlayerProfiles
- Set `state = Resolved`
- Emit `TimeoutSlash`

### `close_game`
Permissionless — any wallet can call.
- Assert `state == Resolved`
- Anchor's `close = caller` constraint transfers rent-exempt lamports to caller and zeros the discriminator
- Incentivizes callers to clean up resolved game accounts

### `finalize_tournament`
Permissionless — any wallet can call.
- Assert `Clock::now() > tournament.end_time`
- Assert `tournament.finalized == false`
- Snapshot `prize_snapshot = prize_lamports` and `total_score_snapshot = sum of all player scores`
- Set `finalized = true`
- Emit `TournamentFinalized`

Note: computing `total_score_snapshot` requires all PlayerProfile accounts to be passed in as remaining accounts. The instruction iterates them, verifies each is a valid PDA for this tournament, and sums scores. Caller constructs the account list off-chain.

### `claim_reward`
Signers: `player`
- Assert `tournament.finalized == true`
- Assert `player_profile.claimed == false`
- Assert `player_profile.total_games >= 5` (minimum games floor)
- Compute entitlement: `(player_score / total_score_snapshot) × prize_snapshot`
- Transfer entitlement from Tournament PDA to player wallet
- Set `player_profile.claimed = true`
- Emit `RewardClaimed`

---

## Payoff Logic (`resolve_game` pure function)

Called from `reveal_guess` when both guesses are in. Routes to `resolve_homogenous` or `resolve_heterogeneous` based on `game.matchup_type`.

Let `S = stake_lamports`. A correct guess means guessing the actual `matchup_type` value (0 for same team, 1 for different team).

**Same team (matchup_type = 0) — cooperative:**

| Outcome | P1 return | P2 return | Tournament receives |
|---|---|---|---|
| Both guess correctly (both guess 0) | `S * 9 / 10` | `S * 9 / 10` | `S * 2 / 10` |
| At least one wrong | `0` | `0` | `2 * S` |

**Different teams (matchup_type = 1) — adversarial:**

Winner rule: if exactly one player is wrong, the wrong player loses. If both correct or both wrong, the first committer wins.

| Outcome | Winner return | Loser return | Tournament receives |
|---|---|---|---|
| One correct, one wrong | correct player: `S * 19 / 10` | `0` | `2*S - S*19/10` |
| Both correct or both wrong | first committer: `S * 19 / 10` | `0` | `2*S - S*19/10` |

All arithmetic uses `checked_mul` / `checked_div`. Lamport conservation is asserted as an invariant after each resolution.

After resolving:
- Transfer return amounts to player wallets
- Transfer tournament portion to Tournament PDA `prize_lamports`
- Check `tournament.end_time`: if `Clock::now() > end_time`, return stakes in full and contribute nothing to the prize pool (late resolution)
- Update `PlayerProfile.wins`, `total_games`, and `score = wins * wins / total_games` for both players
- Set `state = Resolved`, close Game account

---

## Scoring

```
score = wins * wins / total_games
```

- Minimum 5 `total_games` required to be eligible for `claim_reward`
- Tie-breaking: higher `total_games` wins; if still tied, first to claim wins (no special handling needed)
- ELO is a future off-chain display metric — not used for on-chain payout

---

## Timeouts

Measured in Solana slots (~400ms each).

| Stage | Timeout |
|---|---|
| Committing (waiting for second commit) | 7,200 slots (~1 hour) |
| Revealing (waiting for reveal after both committed) | 14,400 slots (~2 hours) |

`commit_timeout_slots` is stored on the Game account and set at creation using these constants. Defined as program constants:

```rust
pub const COMMIT_TIMEOUT_SLOTS: u64 = 7_200;
pub const REVEAL_TIMEOUT_SLOTS: u64 = 14_400;
```

---

## Lamport Flow

```
Player 1 ──join_game──► Game PDA (holds 2 × stake_lamports)
Player 2 ──join_game──┘

Game PDA ──resolve──► Player 1 wallet     (return per payoff matrix)
                  ──► Player 2 wallet     (return per payoff matrix)
                  ──► Tournament PDA      (losing portion)

Tournament PDA ──claim_reward──► Player wallet  (proportional entitlement)
```

---

## Error Types

```rust
InvalidGameState            // instruction not valid for current state
InvalidStateTransition      // transition not in the state machine
NotAParticipant             // caller is not player_one or player_two
AlreadyCommitted            // player has already committed
AlreadyRevealed             // player has already revealed
AlreadyClaimed              // player has already claimed reward
CannotJoinOwnGame           // player_two == player_one
StakeMismatch               // lamports sent != stake_lamports
CommitmentMismatch          // SHA-256(R) != stored commitment
TimeoutNotElapsed           // resolve_timeout called too early
InvalidTournamentTimes      // end_time <= start_time in create_tournament
TournamentNotEnded          // finalize or claim called before end_time
TournamentNotFinalized      // claim called before finalize_tournament
EmptyPrizePool              // nothing to claim
OutsideTournamentWindow     // create_game or join_game called outside tournament start/end
ProfileTournamentMismatch   // player profile belongs to a different tournament
BelowMinimumGames           // player has fewer than 5 games
ArithmeticOverflow          // checked arithmetic failure
TooManyAccounts             // finalize_tournament passed more than 30 remaining accounts
```

---

## Events

```rust
TournamentCreated   { tournament_id: u64, start_time: i64, end_time: i64 }
GameCreated         { game_id: u64, tournament_id: u64, player_one: Pubkey, stake_lamports: u64 }
GameStarted         { game_id: u64, tournament_id: u64, player_one: Pubkey, player_two: Pubkey }
GuessCommitted      { game_id: u64, player: Pubkey, commit_slot: u64 }
GuessRevealed       { game_id: u64, player: Pubkey }
GameResolved        { game_id: u64, p1_guess: u8, p2_guess: u8, p1_return: u64, p2_return: u64, tournament_gain: u64 }
TimeoutSlash        { game_id: u64, slashed_player: Pubkey, slash_amount: u64 }
TournamentFinalized { tournament_id: u64, prize_snapshot: u64, total_score_snapshot: u64 }
RewardClaimed       { tournament_id: u64, player: Pubkey, amount: u64 }
```

---

## Open Questions

- **Heterogeneous matchup payoffs** — the payoff matrix for human vs. AI matches is defined in root `CLAUDE.md` but not implemented in v1. When AI agents are added, `matchup_type` on the Game account drives the resolution logic.
- **`finalize_tournament` scaling** — passing all PlayerProfile accounts as remaining accounts works for small tournaments but hits transaction size limits at scale. Redesign needed before production.
