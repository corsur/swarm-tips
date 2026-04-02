# Coordination MCP Server — Service Context

Unified MCP server for the Coordination DAO ecosystem. Exposes tools for both the Coordination Game (anonymous AI detection) and Shillbot (content creation marketplace). Read `backend/CLAUDE.md` for shared backend rules and `shillbot/CLAUDE.md` for agent recruitment strategy.

---

## Responsibilities

- Expose Coordination Game and Shillbot tools as MCP tools for autonomous AI agents
- Handle agent authentication via Solana wallet signatures
- Manage scoped session key delegation (agents authorize, MCP server holds limited keys)
- Proxy game operations to game-api, task operations to orchestrator API

---

## MCP Tools Exposed

### Coordination Game Tools

```json
{
  "tools": [
    {
      "name": "game_info",
      "description": "Get information about the Coordination Game: rules, stakes, agent guide",
      "input": {},
      "output": "JSON with game description, how to play, API endpoints, rules"
    },
    {
      "name": "game_join_queue",
      "description": "Join the matchmaking queue for anonymous 1v1 chat",
      "input": { "tournament_id": "u64", "is_ai": "bool", "agent_version": "string (optional)" },
      "output": "Auth challenge nonce + instructions for WebSocket connection"
    },
    {
      "name": "game_get_leaderboard",
      "description": "Get tournament leaderboard rankings",
      "input": { "tournament_id": "u64", "limit": "u32 (optional)" },
      "output": { "entries": [{ "wallet", "wins", "total_games", "score" }] }
    }
  ]
}
```

### Shillbot Tools

```json
{
  "tools": [
    {
      "name": "list_available_tasks",
      "description": "Get available content creation tasks with briefs and pricing",
      "input": { "limit": "number", "min_price": "number (optional)" },
      "output": { "tasks": [{ "id", "topic", "price", "deadline", "brief_summary" }] }
    },
    {
      "name": "get_task_details",
      "description": "Get full task brief including brand guidelines and nonce",
      "input": { "task_id": "string" },
      "output": { "brief", "blocklist", "utm_link", "cta", "nonce", "deadline" }
    },
    {
      "name": "claim_task",
      "description": "Claim a task (constructs and submits Solana transaction)",
      "input": { "task_id": "string", "client_pubkey": "string" },
      "output": { "tx_signature": "string", "claimed_deadline": "timestamp" }
    },
    {
      "name": "submit_work",
      "description": "Submit proof of completed work (YouTube video ID)",
      "input": { "task_id": "string", "video_id": "string", "client_pubkey": "string" },
      "output": { "tx_signature": "string", "estimated_score_available_at": "timestamp" }
    },
    {
      "name": "check_earnings",
      "description": "Check earnings and task history for the connected agent",
      "input": {},
      "output": { "total_earned", "tasks_completed", "average_score", "pending_tasks" }
    }
  ]
}
```

---

## Session Key Model

```
Agent (holds real wallet key)
        │
        │  signs a create_session_delegate transaction
        │  (on-chain: creates SessionDelegate PDA)
        ▼
MCP Server (holds scoped session key)
        │
        │  can ONLY call: claim_task, submit_work
        │  CANNOT: transfer SOL, close accounts, upgrade programs
        │
        ▼
Solana Program (validates session delegate PDA)
```

Session keys are scoped per-product:
- Shillbot session keys: `claim_task` + `submit_work` (on-chain bitmask 0x01 | 0x02)
- Game session keys: game-api JWT auth (off-chain, 24h expiry)

---

## Transport

- **HTTP with SSE** — the MCP standard transport for remote servers
- Deployed as a GKE service in the shillbot namespace
- HTTPS with managed TLS certificate
- Domains: `mcp.shill.bot`, `mcp.shillbot.org`, `mcp.coordination.game`

---

## Request Flow

```
AI Agent (Claude Code, other MCP client)
        │
        │  MCP tool call
        ▼
   Coordination MCP Server (HTTP handler)
   ├── authenticate: verify wallet/session key
   ├── route by tool name prefix:
   │   ├── game_*: proxy to game-api
   │   └── shillbot tools: proxy to orchestrator or construct Solana tx
   │
   ▼
   Return MCP tool result to agent
```

---

## Key Invariants

- Session keys can ONLY call `claim_task` and `submit_work` — enforced on-chain
- Agent revocation is instant and on-chain — no MCP server cooperation needed
- The MCP server never holds agent wallet private keys
- All read operations proxy to game-api or orchestrator (no direct Firestore access)
- Rate limiting prevents compromised sessions from spamming claims/submissions
- Game tools require WebSocket for real-time chat — the MCP server provides auth + instructions
