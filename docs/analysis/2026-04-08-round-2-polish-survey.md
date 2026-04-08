# Round 2 Polish Pass — Fresh-Claude Feedback Response

**Date:** 2026-04-08
**Catalyst:** a second fresh-Claude review of `mcp.swarm.tips` after the morning's unified-list-tools shift shipped. Fresh-Claude #2 explicitly praised the post-shift state ("one of the better-designed MCP servers I've seen in terms of tool descriptions, non-custodial flow, and session ergonomics") and the cash-flow tag convention ("real ergonomic win"), but called out residual surface-area issues, UX traps, and one meaningful product gap.

## What fresh-Claude flagged (verbatim summary)

> **Tool surface is bloating.** 23 tools, with overlapping paths: game_join_queue and game_find_match + game_register_wallet for the same outcome, plus game_info duplicating what's already in the server instructions field. The description on game_join_queue even says "for a simpler flow, use … instead" — that's a deprecation note shipped as a live tool.
>
> **Two submit_tx tools that do the same thing.** game_submit_tx and shillbot_submit_tx both broadcast a signed Solana tx and dispatch on an action string. One submit_signed_tx with a wider action enum would shrink the surface and remove a class of "agent called the wrong submit_tx" bugs.
>
> **tournament_id is required but there's only ever one tournament.** Both game_find_match and game_get_leaderboard require it and the description says "Typically 1 — the only active tournament … there is no public discovery endpoint." If there's no discovery and only one value, make it optional with a default of 1. Right now it's a trap.
>
> **Two tools numbered 7 in the server instructions block** (game_reveal_guess and game_get_result). Tiny, but it's the kind of thing that makes me wonder if anyone proofread the prompt the LLM is actually going to read.
>
> **game_register_wallet quietly does double duty as the Shillbot wallet registration.** The description mentions it, but there's no shillbot_register_wallet alias, so an agent landing on Shillbot tools first has to read the game tool's docs to discover registration. A thin alias (or splitting registration into its own register_wallet tool with both products listed) would be friendlier.
>
> **is_ai boolean on game_join_queue — "required for data integrity."** This is unenforceable and slightly silly; any adversarial agent will lie. If you actually need the signal, infer it from behavior; if you don't, drop the field.
>
> **No resources or prompts capability advertised in the initialize response, only tools.** For a server this rich (especially the Coordination Game, which has a real protocol), exposing the rules / brand-voice / blocklist as MCP resources would be more idiomatic than packing them into a game_info tool call.
>
> **check_video_status is poll-only.** Fine for v1, but MCP supports server→client notifications; a long-running video job is exactly the case for it.
>
> **Risk surface worth flagging to users.** This is a server that will, on a single tool call, hand an agent an unsigned transaction that moves real SOL/USDC. The non-custodial design means swarm.tips can't steal funds, but a prompt-injected agent absolutely can be talked into signing one. There's no per-session spend cap, no confirmation step, no dry-run mode. **The product is basically "give an LLM a wallet and a marketplace" — the missing guardrail is the LLM-side equivalent of a spending limit.** This is the only thing fresh-Claude called "a real concern."

## What this PR ships

Five fixes, all on the polish/cleanup side:

### 1. Duplicate "7." typo in INSTRUCTIONS — fixed

The `INSTRUCTIONS` const string returned in the MCP `initialize` response had two list items numbered `7.`:
```
7. game_reveal_guess — poll until both committed, then reveals and resolves
7. game_get_result — see outcome
```
Fixed in this PR. The Coordination Game section now uses correct sequential numbering.

### 2. `tournament_id` is now optional with default 1

`GameFindMatchArgs.tournament_id` and `GameGetLeaderboardArgs.tournament_id` are now `Option<u64>`. Both handlers default to `1` via `args.tournament_id.unwrap_or(1)`. Both descriptions and arg doc comments updated to reflect the default. Fresh-Claude was right — making a field required when there's no discovery endpoint and only one valid value is a UX trap. Agents that don't read the description carefully no longer fail; they get the only working value automatically.

### 3. `game_join_queue` removed entirely

The tool's own description said "For a simpler flow, use game_register_wallet + game_find_match instead." It was a deprecation note shipped as a live tool. Removed in this PR along with `GameJoinQueueArgs` (which carried the unenforceable `is_ai: bool` field — that's also gone, fix #6 from the fresh-Claude list, moot once the tool is removed). The `auth_challenge` adapter on `GameApiProxy` is now dead at the tool layer; it's marked `#[allow(dead_code)]` (matching the existing `join_queue` adapter pattern) so the proxy adapter is preserved as a reusable swap-out point if a future flow needs the challenge/sign/JWT path.

### 4. `game_info` removed; content merged into INSTRUCTIONS

`game_info` returned a `GAME_INFO_JSON` constant containing rules, stake, how_to_play, and rules_for_agents. Most of this content was already in the `INSTRUCTIONS` field returned in the MCP `initialize` response — every agent reads it before any tool call. The duplication burned tokens for no incremental value.

In this PR: deleted the `game_info` tool, deleted the `GAME_INFO_JSON` constant, expanded the INSTRUCTIONS Coordination Game section to include the structured content (stake, rules for agents, how-to-play sequence) so nothing is lost. Net: -1 tool, +0 information, simpler surface.

Fresh-Claude's bigger suggestion was to expose this content as MCP **resources** rather than baking it into INSTRUCTIONS. That's the right architectural move but requires advertising the `resources` capability and implementing the resources protocol. Deferred to a separate plan — for this PR INSTRUCTIONS is the stopgap.

### 5. `game_register_wallet` renamed to `register_wallet`

The tool name was misleading: it implied game-only registration, but the same call is used by every Shillbot tool too. Fresh-Claude called it a UX trap for agents that land on Shillbot tools first.

In this PR: renamed `game_register_wallet` → `register_wallet` (tool name + Rust fn + description). The description was rewritten to drop the "to play the Coordination Game" framing and explicitly state that one registration covers every product (Coordination Game + Shillbot + video). All cross-references in other tool descriptions (Shillbot tools' "call game_register_wallet first" prompts, the unified `list_earning_opportunities` tool's description, the auth-required error messages) were updated.

This is a hard rename without an alias — pre-launch is the right time for one-way renames.

## Tool count

| Vertical | Before this PR | After this PR |
|---|---:|---:|
| Coordination Game (post-2026-04-08-morning shift) | 12 | **9** (drop game_info, drop game_join_queue, register_wallet moved to its own group) |
| Wallet registration (cross-product) | 0 | **1** (`register_wallet`) |
| Shillbot | 6 | 6 |
| Video | 2 | 2 |
| Universal opportunity discovery | 2 | 2 |
| **Total** | **22** | **20** |

Net change: -3 tools (dropped game_info + game_join_queue, renamed game_register_wallet to register_wallet — the rename keeps the count the same but conceptually moves it from "Coordination Game" to "wallet registration" since both products use it).

Note on counting: fresh-Claude said "23 tools" but the live `tools/list` returned exactly 22 before this PR. Their count is off by one — possibly counted `shillbot_submit_tx` separately or miscounted. Doesn't change the substance of the feedback; flagging just for the record.

## What this PR explicitly defers

### Spend guardrails — the only "real concern" fresh-Claude flagged

> *"There's no per-session spend cap, no confirmation step, no dry-run mode. If I were running this against an autonomous agent I'd want a max_spend_usd ceiling enforced server-side per session, or at minimum a dry_run: true parameter on every [SPEND]/[STATE] tool. The product is basically 'give an LLM a wallet and a marketplace' — the missing guardrail is the LLM-side equivalent of a spending limit."*

Deferred to a focused follow-up plan. Bundling spend guardrails with cleanup risks shipping a half-baked guardrail. The proper plan needs decisions on:

- **Per-session vs per-wallet tracking.** Per-wallet is more durable (resists session reconnects); per-session is simpler. Recommend per-wallet, persisted to Firestore on every increment using the existing game-session Firestore client.
- **Default cap value.** Probably $50 USD/wallet, configurable via env (`MAX_SPEND_USD_PER_WALLET=50`).
- **`dry_run: bool` parameter** on every SPEND tool (`game_find_match`, `generate_video`). When true, build the unsigned tx as normal, return it tagged `dry_run: true`, don't increment the spend counter.
- **`set_spend_limit(max_usd)` tool** — agents can lower their cap below the default but never raise above the env-configured max.
- **`check_spend_status()` tool** — returns `{total_spent_usd, remaining_usd, max_usd}` so agents can reason about their headroom.
- **Counter semantics**: increment at tool-call time (when the unsigned tx is built), not at submit time. This means agents that build but don't sign burn quota — but the alternative (tracking on-chain confirmations) is much more complex and the server can't observe what the agent does after the tool call.
- **USD computation**: `game_find_match` is 0.05 SOL × current SOL/USD price. Reuse `SOL_PRICE_USD = 150.0` constant from `services/mcp-server/src/listings/sources.rs:207` for v1; switch to a real Pyth/CoinGecko feed in v2. `generate_video` is fixed at $5.

This is a meaningful new feature with real design surface. Worth its own plan.

### `submit_tx` consolidation — recommended NOT to do

Fresh-Claude suggested collapsing `game_submit_tx` and `shillbot_submit_tx` into a single `submit_signed_tx` with a wider action enum. After reading the code:

- `game_submit_tx` calls `game_sessions.submit_signed_game_tx(&wallet, signed_tx, action)` — game-api state machine. Actions: `deposit_stake`, `join_game`, `commit_guess`, `reveal_guess`, `create_game`. No extra args needed.
- `shillbot_submit_tx` does its own `solana_tx::broadcast_signed_b64` + `wait_for_signature_confirmed` + `orchestrator.confirm_task(task_id, wallet, tx_signature, action)` flow. Actions: `claim`, `submit`. **Requires `task_id`** as an additional arg.

A unified tool would need a discriminated-union schema (different required args per action). The JSON Schema for that is messier to construct than the agents reading it would expect — they'd need nested object construction with action-specific arg shapes. The savings are 1 tool (22 → 21 had this also been in scope), the cost is a schema that's harder to use correctly.

**Disposition**: NOT consolidating. Document the per-action arg differences in this survey doc as the rationale. If we ever do consolidate, it should be a deliberate API design pass (probably alongside spend guardrails), not a casual rename.

### MCP `resources` capability

Fresh-Claude correctly noted that the Coordination Game's rules + brand voice + blocklist are protocol content that would be more idiomatic as MCP resources than baked into a tool. Deferred to a separate plan because:

- Requires advertising the `resources` capability in the initialize response
- Requires implementing `resources/list` and `resources/read` handlers in `rmcp`
- Resource URI scheme to design (`game://info`, `game://rules`, `shillbot://brand-voice/{campaign_id}`, etc.)
- Real architectural change, not polish

For this PR, content was merged into INSTRUCTIONS as a stopgap.

### Server→client notifications for `check_video_status`

Polling burns tokens. MCP supports `notifications/progress` for long-running operations. The video generation pipeline takes minutes; an agent polling every 2-3 seconds racks up ~20-50 tool calls per video. Server-push would compress that to ~2 calls (initial + completion notification).

Real win, but requires:
- Async notification dispatch from the orchestrator → MCP server → client
- State tracking on the MCP server to know which session expects which video's notifications
- Plumbing through `rmcp` for the notification protocol

Deferred. The polling pattern works correctly for v1 even if it's wasteful.

## Out of scope (explicit non-goals)

- **`mcp-publisher publish v0.1.3`** — still gated on the user's manual action. Server.json version was NOT bumped in this PR per the new feedback memory: bump only at publish time, not on every change. The local file is still 0.1.3 from the morning shift; whenever the next publish happens, it can use that or whatever's > 0.1.0 (the currently-published version).
- **`game_list_tournaments` as a real tool** — still requires upstream API work in `crates/game-api-client`. The new optional + default-1 pattern covers the actual use case for now.
- **Per-tool count miscounting from fresh-Claude** — they said 23, the live count was 22. Not load-bearing.

## Verification

End-to-end checks after deploy:

1. **Tool count is 20** in the live `tools/list` (was 22).
2. **No `game_info`, `game_join_queue`, or `game_register_wallet`** in the response.
3. **`register_wallet` present** as a top-level tool.
4. **`game_find_match` schema**: `tournament_id` is optional (not required).
5. **`game_get_leaderboard` schema**: same.
6. **The `INSTRUCTIONS` field** returned in `initialize` no longer has the duplicate "7." numbering, has the merged Coordination Game content (rules for agents, how to play, stake), and references `register_wallet` instead of `game_register_wallet`.
7. **All other tool descriptions** that previously referenced `game_register_wallet` now reference `register_wallet` (Shillbot tools, list_earning_opportunities, error messages).

## Reference applications updated

The Listing Policy in `services/mcp-server/CLAUDE.md` § *Listing Policy — The Workprotocol Test* tracks reference applications chronologically. This PR doesn't add new applications (it's polish, not a Workprotocol-Test verdict on a new source) but the cumulative reference list now reads:

- 2026-04-07: workprotocol.ai → Fail
- 2026-04-07: Chutes → Uncertain
- 2026-04-08 (morning): ClawTasks → Removed (broken API + pattern mismatch)
- 2026-04-08 (morning): BotBounty MCP tools → Removed, listing source kept

This PR is polish, not verdicts. Same survey doc structure used for documentation continuity.

## Open follow-ups

- **★ Spend guardrails** — the highest-priority follow-up. Per fresh-Claude this is the only "real concern". Plan it next as a focused feature: per-wallet running USD tracker, dry_run parameter, set_spend_limit / check_spend_status tools, env-configured default cap.
- **MCP `resources` capability** — move game info content out of INSTRUCTIONS into proper MCP resources. Architectural improvement, not polish.
- **Server-push notifications for `check_video_status`** — token-saving improvement for the video generation flow.
- **`mcp-publisher publish` of the post-cleanup state** — the registry currently shows 0.1.0 with "22 tools". We're at v0.1.3 in local server.json with 20 tools. Whenever the user is ready, run `mcp-publisher publish` from `services/mcp-server/`.
- **Manual end-to-end smoke**: have a fresh-Claude (or this Claude) actually claim a Shillbot task, generate a video, play a game, end-to-end. We've verified the tool surface but not the full claim → submit → pay-out cycle on Shillbot since the rename.
