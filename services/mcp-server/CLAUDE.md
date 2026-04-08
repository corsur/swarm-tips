# MCP Server — Service Context

Unified MCP server for Swarm Tips (`mcp.swarm.tips`). 22 tools live: Coordination Game (12), Shillbot marketplace (6, mainnet, all `shillbot_*`-prefixed), video generation (2), and two universal opportunity-discovery tools (`list_earning_opportunities`, `list_spending_opportunities`). For the full swarm.tips spec, see `swarm/swarm-tips/CLAUDE.md`. For shared code standards, see the root `CLAUDE.md`.

---

## Registry Status

**Official MCP Registry:** Published as `io.github.corsur/swarm-tips` on `registry.modelcontextprotocol.io`.

| Field | Value |
|-------|-------|
| Name | `io.github.corsur/swarm-tips` |
| Published version | **0.1.0** (2026-04-04) |
| Local `server.json` version | **0.1.1** (pending re-publish) |
| Status | active |
| Transport | streamable-http at `https://mcp.swarm.tips/mcp` |

The 0.1.0 listing description still says "22 tools" but it's unrelated to the new 22-tool count from the 2026-04-08 strategic shift — it's stale and was meant to be updated to "28 tools". v0.1.2 ships with the new description ("22 tools across 3 mainnet protocols plus universal opportunity discovery. Non-custodial.") reflecting the post-shift state. To re-publish: run `mcp-publisher publish` from `services/mcp-server/` (the OAuth tokens were refreshed on 2026-04-07; if they expire again, run `mcp-publisher login github` first for the interactive browser flow).

**Auth tokens** are stored in `services/mcp-server/.mcpregistry_github_token` and `.mcpregistry_registry_token` (gitignored). Both expire periodically.

**Other directories:** Not yet submitted to mcp.so, PulseMCP, Glama, or ClawHub. SKILL.md (at repo root) is ready for ClawHub submission.

**Discovery sources (read-side):** `src/discovery/sources.rs` pulls from four upstream catalogs: the official MCP registry, `wong2/awesome-mcp-servers`, `appcypher/awesome-mcp-servers`, and `tolkonepiu/best-of-mcp-servers`. All four run in parallel inside `refresh_discovery` with per-source error degradation. PulseMCP is gated on credentials (email `api@pulsemcp.com`); Smithery requires API surface verification before integration. The first DefiLlama meta-discovery scan landed 2026-04-07 — see `docs/analysis/2026-04-07-defillama-discovery-survey.md` for findings and `src/listings/sources.rs::fetch_defillama_ai_agents` for the source.

**Tool descriptions** include cash-flow tags (`[READ]`, `[STAKE: ...]`, `[EARN: ...]`, `[SPEND: ...]`, `[STATE]`) so AI agents running a business can reason about inflows vs outflows from descriptions alone.

---

## Listing Policy — The Workprotocol Test

**Rule:** A bounty source becomes a `fetch_*` integration in `src/listings/sources.rs` only if we can demonstrate that users acting on its listings can reasonably expect to be paid. Discovery of a platform is necessary but **not sufficient**. Payment provability is the bar.

**Why:** the 2026-04-07 arbitrage survey originally surfaced `workprotocol.ai` as a "verified earning platform" because it had open jobs, structured listings, and real USDC amounts. It later turned out to be vaporware — no completed bounties, no payment evidence, no track record. Listing a vaporware source on swarm.tips would have wasted the time of every agent that tried to claim from it and degraded trust in the aggregator. The cost of one bad listing is much higher than the cost of skipping a marginal one.

**Verification procedure** — apply in order of cost. Stop as soon as you can assign a verdict.

1. **Cheap structural checks.** Pull the platform's bounty/job listing endpoint. Look for: a *completed bounties* archive (not just open ones), a *payment history* endpoint, public *revenue/payments* aggregates, an explicit *escrow contract address*. The presence of any one is positive evidence; the absence of all of them is yellow.
2. **On-chain verification when applicable.** If the platform exposes a contract address (Layer 3 already extracts these), query the chain for transaction history. Number and total volume of payouts is a strong binary signal: zero historical payouts to external claimants = fail; many = pass.
3. **Independent payment evidence.** Public daily revenue (Chutes' `/daily_revenue_summary`), Bittensor subnet emissions, on-chain Stripe-equivalent attestations. The platform paying *something* to *someone* is necessary but not sufficient — we specifically need evidence the bounty mechanism itself pays out.
4. **Negative social signal.** Search GitHub issues + Twitter/X + Reddit for `"{platform} not paying"`, `"{platform} scam"`, `"{platform} ghosted"`. Even one credible negative report should flip to fail.
5. **LLM synthesis when ambiguous.** Feed the evidence above to the Layer 2 Grok classifier with a payout-verification prompt. Ask for a verdict + reasoning + which evidence was most load-bearing.

**Verdicts:**

- **Pass** — there is concrete, verifiable evidence of bounties being claimed and paid out by external (non-team) participants. Build the `fetch_*` integration.
- **Fail** — verified scam, abandoned platform, or "active listings but zero payment history". Skip the integration. Document the disposition in a survey doc so we don't re-evaluate the same source under a different name.
- **Uncertain** — the platform looks real (real revenue, real users) but the *bounty mechanism specifically* lacks verified payouts. Don't list yet. Re-evaluate after specific follow-ups (decode the bounty unit, find a historical paid-out example, read their docs end-to-end). Document in the survey doc as "discovered, did not pass workprotocol test on first probe — needs X".

**When to apply:** before writing any new `fetch_*` source. Also retroactively: if a source we already integrated stops passing the test (parser success rate drops, listings disappear without ever being claimed, social signal turns negative), document the disposition and remove the source.

**Where the verification lives:** for now, in survey docs at `swarm-tips-repo/docs/analysis/`. After 3-5 manual applications the patterns will be clear enough to formalize as a `verify_payout` function in `src/discovery/deep_analysis.rs`. Don't build that abstraction before the patterns exist.

**Reference applications** (chronological):
- 2026-04-07: workprotocol.ai → **Fail**. See `docs/analysis/2026-04-07-arbitrage-survey.md`.
- 2026-04-07: Chutes → **Uncertain**. See `docs/analysis/2026-04-07-defillama-discovery-survey.md`.
- 2026-04-08: ClawTasks → **Removed** (broken API + pattern mismatch). See `docs/analysis/2026-04-08-unified-list-tools-strategic-shift.md`.
- 2026-04-08: BotBounty MCP tools → **Removed**, `fetch_botbounty` listing source kept. Same doc.

---

## Listing Policy — Unified List Tools

**The structural rule (2026-04-08 strategic shift).** Layered on top of (not replacing) the workprotocol test. Routes integration decisions deterministically instead of forcing per-source forensic investigation.

**Two universal MCP tools** are the canonical entry point for opportunity discovery:

- `list_earning_opportunities` — aggregates earning entries across all known sources via the existing `fetch_*` infrastructure in `src/listings/sources.rs`. Each entry has a `source` field (`shillbot`, `bountycaster`, `moltlaunch`, `botbounty`, ...). First-party entries (currently only `source = "shillbot"`) get a `claim_via` field naming the in-MCP tool to call (`shillbot_claim_task`). External entries have a direct `source_url` redirect — agents claim through the source platform itself, swarm.tips does not mediate.
- `list_spending_opportunities` — aggregates paid services. v1 hardcoded with `generate_video` (first-party). External spend sources (Chutes inference, x402-paywalled APIs) are deferred to follow-up integrations. First-party entries get a `spend_via` field; external entries get a `url`.

**The integration rule:**

- **Listings sources** in `src/listings/sources.rs` (`fetch_*`) feed `list_earning_opportunities`. Adding a new source means writing a `fetch_*` function. The workprotocol test still applies — the source must have payment provability before we list it — but the MCP surface stays at two tools regardless.
- **Per-source CRUD MCP tools** are reserved for two cases: (1) first-party verticals we own end-to-end (Coordination Game, Shillbot, video generation), or (2) external platforms with verifiable on-chain enforceable escrow that mathematically guarantees payout independent of the platform's good behavior. We have zero examples of case (2) today; the first such integration is a future plan. **Centralized full-CRUD proxies are banned** — they're fundamentally fragile (the platform can break, change schemas, pivot, or shut down) and we can't independently verify pay-out.

**Why this matters:** before 2026-04-08 we proxied ClawTasks and BotBounty as full CRUD MCP tools. ClawTasks's API broke (returned HTTP 500 on every endpoint) and we caught it in real time during the audit, exposing the structural fragility. The unified-tools-with-redirect pattern eliminates that failure mode for the discovery surface and reserves the deeper engineering effort for cases where it actually pays off.

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

## Tools (22 active)

### Universal opportunity discovery (2 tools)
- `list_earning_opportunities` — aggregated earning entries across `fetch_*` sources (Shillbot, Bountycaster, Moltlaunch, BotBounty). First-party entries (`source = "shillbot"`) include a `claim_via` field naming the in-MCP tool to call. External entries have a direct `source_url` redirect — agents claim off-platform.
- `list_spending_opportunities` — aggregated paid services. v1 hardcoded with `generate_video` (first-party, 5 USDC). External sources (Chutes inference, x402-paywalled APIs) are deferred to follow-up integrations.

### Coordination Game (12 tools, non-custodial)
- `game_info` — rules, stakes, agent guide (read-only)
- `game_get_leaderboard` — tournament rankings (read-only)
- `game_join_queue` — returns auth instructions for manual flow
- `game_register_wallet` — register pubkey only (non-custodial, no private key)
- `game_find_match` — returns unsigned deposit_stake tx (agent signs locally)
- `game_submit_tx` — submit any signed game transaction (deposit, join, commit, reveal)
- `game_check_match` — poll match status; returns unsigned join_game tx when matched
- `game_send_message` / `game_get_messages` — chat with opponent (implicitly scoped to current MCP session)
- `game_commit_guess` — returns unsigned commit tx
- `game_reveal_guess` — poll until resolved, returns unsigned reveal tx
- `game_get_result` — read game outcome

### Shillbot (6 tools, Solana mainnet, on-chain escrow)
- `shillbot_list_available_tasks` — browse tasks (Shillbot-specific deep query; for cross-source aggregation use `list_earning_opportunities`)
- `shillbot_get_task_details` — full task brief, blocklist, brand voice
- `shillbot_claim_task` — claim via session key (returns unsigned tx)
- `shillbot_submit_work` — submit content ID proof (returns unsigned tx)
- `shillbot_submit_tx` — submit any signed Shillbot tx (claim, submit) — non-custodial path
- `shillbot_check_earnings` — agent earnings summary

### Video Generation (2 tools, 5 USDC per video)
- `generate_video` — create short-form video from prompt/URL (two-step: first call returns payment instructions including `payment_details: {chain, address, amount, memo}`, second call with `tx_signature` triggers generation)
- `check_video_status` — poll by session_id until video_url is returned (read-only)

### Removed 2026-04-08
- `clawtasks_*` (4 tools): API was returning HTTP 500, didn't fit the unified-tools strategic shift. See `docs/analysis/2026-04-08-unified-list-tools-strategic-shift.md`.
- `botbounty_*` (4 tools): MCP CRUD proxy retired, `fetch_botbounty` listing source kept (entries still appear in `list_earning_opportunities`).

---

## Session Key Model

Shillbot session keys: `claim_task` + `submit_work` only (on-chain bitmask 0x01 | 0x02)
Game session keys: game-api JWT auth (off-chain, 24h expiry)

The MCP server is fully non-custodial for game operations:
- `game_register_wallet` takes pubkey only — no private key ever touches the server
- Game tools return unsigned transactions — agents sign locally
- Auth via stake-as-auth: agent signs deposit_stake locally → `game_submit_tx` → MCP authenticates with game-api via `POST /auth/session` (tx signature proves wallet ownership)

---

## Game Session Persistence

Game sessions are persisted to Firestore (`mcp_game_sessions/{wallet}`) on every state transition. This ensures pod restarts don't lose critical state — especially the `commit_preimage` needed for the reveal step.

**Stored fields:** wallet, jwt, state, game_id, tournament_id, session_id, role, matchup_commitment, commit_preimage_hex, game_ready, reveal_data.

**Restore flow:** On `game_register_wallet`, if a persisted session exists with an active state (not Resolved), it is restored — including preimage and WS reconnection (with 10s timeout for stale JWTs). Resolved sessions are cleaned up on the next register call.

**WS reconnect:** Background WS listener uses exponential backoff (2s, 4s, 8s, max 3 attempts) with a `CancellationToken` for clean shutdown.

---

## Workflow Orchestration (Google Workflows)

**Cross-repo standard.** Multi-step / deferred work in mcp-server uses Google Workflows, never `tokio::spawn` timers, in-memory job queues, or polling loops. This is the same rule that applies to every backend service in every repo — see `swarm/CLAUDE.md` "Workflow Orchestration (Google Workflows) — cross-repo standard" for the canonical statement.

mcp-server-specific notes:

- **Layer 2 LLM classifier (`/internal/mcp/llm-classify`)** is currently invoked synchronously by HTTP. When the cap-bounded run grows past comfortable HTTP timeouts, migrate to a Google Workflow that calls `/internal/mcp/llm-classify` once per batch with a `sys.sleep` between batches. Workflow YAML belongs in `infra/workflows/` once it exists; today the directory only holds the daily discovery refresh skeleton.
- **Layer 3 deep analysis (`/internal/mcp/deep-analyze`)** has the same property — the current ~15s sync run is fine, but a future "full deep-analyze across the whole index" pass should be a Workflow.
- **Discovery refresh (`/internal/mcp/refresh`)** is meant to run daily. The Cloud Workflow + scheduled trigger that calls it lives in the `infra/workflows/` directory once added — this is the correct pattern for any periodic recompute.
- **What you must NOT add to mcp-server:** any background `tokio::spawn` task that runs forever, any Mutex<HashMap<job_id, ...>> queue, any "remind me later" mechanism that lives in a single pod's memory. Mcp-server is KEDA-scaled and stateless — anything in-process is lost on scale-down.

---

## Secret Management

**Cross-repo standard.** Sensitive runtime values (`xai-api-key`, future secrets) come from GCP Secret Manager DIRECTLY via `gcloud-sdk` at startup. Never via K8s Secrets, env-var mounts, or config files. See `swarm/CLAUDE.md` "Direct Secret Manager reads only for runtime secrets" for the canonical statement and `coordination-app/backend/CLAUDE.md` "Three secret categories, three homes" for the three-way split (runtime → GCP SM, CI → GitHub Secrets, K8s Secrets banned).

mcp-server-specific notes:

- **`src/config.rs::load_optional_secret`** is the reusable helper (copied verbatim from `backend/x-bridge/src/config.rs`). For secrets whose absence should crash-loop the pod, add a sibling `load_secret` that panics on failure — match `backend/chatwoot-responder/src/config.rs::load_secret`.
- **`xai-api-key`** is loaded via `load_optional_secret` at mcp-server startup. If Secret Manager access fails or the secret doesn't exist, mcp-server logs a `warn!` and boots with Layer 2 disabled. Layer 1 + Layer 3 continue to work. `POST /internal/mcp/llm-classify` returns 503.
- **Legacy gap:** the deployment manifest still has `envFrom.secretRef.name: solana-rpc-secret` (optional). That's a pre-existing K8s Secret bridge that should be migrated to direct Secret Manager reads using the same pattern. Don't add new secretRefs of that shape — migrate when touching game-related code.
- **What you must NOT add:** any env-var-based API key read (`std::env::var("FOO_API_KEY")` for sensitive values), any new `secretRef` in the deployment manifest, any hardcoded secret in Rust source.

---

## Key Invariants

- **Non-custodial game operations** — MCP server returns unsigned transactions, agents sign locally
- **Session persistence** — commit_preimage survives pod restarts via Firestore write-through
- Session keys can ONLY call `claim_task` and `submit_work` — enforced on-chain
- Agent revocation is instant and on-chain — no MCP server cooperation needed
- Game session reads from on-chain state (GameTxBuilder.read_game) for reveal state checks
- Rate limiting prevents compromised sessions from spamming claims/submissions
