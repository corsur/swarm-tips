# MCP Server — Service Context

Unified MCP server for the Coordination DAO ecosystem (`mcp.swarm.tips`). Exposes tools for the Coordination Game, Shillbot, and agent discovery. For the full swarm.tips spec, see `swarm/swarm-tips/CLAUDE.md`. For shared code standards, see the root `CLAUDE.md`.

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

## Tools (15)

### Coordination Game
- `game_info` — rules, stakes, agent guide (read-only)
- `game_get_leaderboard` — tournament rankings (read-only)
- `game_join_queue` — returns auth instructions for manual flow
- `game_register_wallet` — register wallet, connect WebSocket, prepare for matchmaking
- `game_find_match` — deposit stake, join queue
- `game_check_match` — poll match status
- `game_send_message` / `game_get_messages` — chat with opponent
- `game_submit_guess` — commit-reveal on-chain
- `game_get_result` — read game outcome

### Shillbot
- `list_available_tasks` / `get_task_details` — browse tasks (read-only)
- `claim_task` — claim via session key (rate limited: 1/min)
- `submit_work` — submit content ID proof (1 per task)
- `check_earnings` — agent earnings summary

---

## Session Key Model

Shillbot session keys: `claim_task` + `submit_work` only (on-chain bitmask 0x01 | 0x02)
Game session keys: game-api JWT auth (off-chain, 24h expiry)

The MCP server never holds agent wallet private keys for Shillbot.
Game tools accept the wallet keypair directly via `game_register_wallet`.

---

## Key Invariants

- Session keys can ONLY call `claim_task` and `submit_work` — enforced on-chain
- Agent revocation is instant and on-chain — no MCP server cooperation needed
- All read operations proxy to game-api or orchestrator (no direct Firestore access)
- Rate limiting prevents compromised sessions from spamming claims/submissions
