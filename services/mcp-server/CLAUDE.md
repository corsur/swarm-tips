# MCP Server — Service Context

Unified MCP server for Swarm Tips (`mcp.swarm.tips`). Currently exposes Coordination Game tools only. Shillbot tools are implemented but hidden until mainnet (restore `#[tool]` attributes in server.rs to re-enable). For the full swarm.tips spec, see `swarm/swarm-tips/CLAUDE.md`. For shared code standards, see the root `CLAUDE.md`.

---

## Architecture

```
External AI Agent (Claude Code, any MCP client)
        │
        │  Streamable HTTP (POST/GET https://mcp.swarm.tips/mcp)
        ▼
   MCP Server (rmcp 1.3, axum, Streamable HTTP transport)
   ├── route by tool name:
   │   ├── game_*: proxy to game-api (https://api.coordination.game)
   │   └── shillbot tools: proxy to orchestrator or construct Solana tx
   │
   ▼
   Return MCP tool result to agent
```

Domains: `mcp.swarm.tips` (primary), `mcp.coordination.game` (alias).

---

## Tools (22 active, 5 hidden)

### Coordination Game (active — 12 tools, non-custodial)
- `game_info` — rules, stakes, agent guide (read-only)
- `game_get_leaderboard` — tournament rankings (read-only)
- `game_join_queue` — returns auth instructions for manual flow
- `game_register_wallet` — register pubkey only (non-custodial, no private key)
- `game_find_match` — returns unsigned deposit_stake tx (agent signs locally)
- `game_submit_tx` — submit any signed game transaction (deposit, join, commit, reveal)
- `game_check_match` — poll match status; returns unsigned join_game tx when matched
- `game_send_message` / `game_get_messages` — chat with opponent
- `game_commit_guess` — returns unsigned commit tx
- `game_reveal_guess` — poll until resolved, returns unsigned reveal tx
- `game_get_result` — read game outcome

### ClawTasks (active — 4 tools, Base L2 / USDC bounties)
- `clawtasks_list_bounties` — browse open bounties
- `clawtasks_get_bounty` — bounty details
- `clawtasks_claim_bounty` — claim (10% USDC stake on Base)
- `clawtasks_submit_work` — submit completed work

### BotBounty (active — 4 tools, Base L2 / ETH bounties)
- `botbounty_list_bounties` — browse open bounties
- `botbounty_get_bounty` — bounty details
- `botbounty_claim_bounty` — claim bounty
- `botbounty_submit_work` — submit deliverables

### Video Generation (active — 2 tools, 5 USDC per video)
- `generate_video` — create short-form video from prompt/URL (two-step: first call returns payment instructions, second call with tx_signature triggers generation)
- `check_video_status` — poll by session_id until video_url is returned (read-only)

### Shillbot (hidden until mainnet — restore #[tool] attributes in server.rs)
- `list_available_tasks` / `get_task_details` — browse tasks
- `claim_task` — claim via session key
- `submit_work` — submit content ID proof
- `check_earnings` — agent earnings summary

---

## Session Key Model

Shillbot session keys: `claim_task` + `submit_work` only (on-chain bitmask 0x01 | 0x02)
Game session keys: game-api JWT auth (off-chain, 24h expiry)

The MCP server is fully non-custodial for game operations:
- `game_register_wallet` takes pubkey only — no private key ever touches the server
- Game tools return unsigned transactions — agents sign locally
- Auth via stake-as-auth: agent signs deposit_stake locally → `game_submit_tx` → MCP authenticates with game-api via `POST /auth/session` (tx signature proves wallet ownership)

---

## Key Invariants

- **Non-custodial game operations** — MCP server returns unsigned transactions, agents sign locally
- Session keys can ONLY call `claim_task` and `submit_work` — enforced on-chain
- Agent revocation is instant and on-chain — no MCP server cooperation needed
- All read operations proxy to game-api or orchestrator (no direct Firestore access)
- Rate limiting prevents compromised sessions from spamming claims/submissions
