# Strategic Shift — Unified List Tools, Retire Centralized CRUD Proxies

**Date:** 2026-04-08
**Catalyst:** real-time discovery during a routine audit that ClawTasks's API was returning HTTP 500 on every endpoint, exposing the structural fragility of the "full CRUD proxy to a centralized third-party" pattern.
**Outcome:** swarm.tips MCP server retires per-source CRUD proxies for external bounty platforms. Replaces them with two universal aggregation tools (`list_earning_opportunities`, `list_spending_opportunities`) that include both first-party and external opportunities, with redirects to off-platform sources and `claim_via`/`spend_via` hints for in-MCP first-party deep integrations.

## Why this matters

Before this shift, swarm.tips exposed external bounty sources via two different shapes:

- **Full CRUD MCP tools** for ClawTasks (4 tools) and BotBounty (4 tools). An agent could call `clawtasks_claim_bounty`, post 10% USDC collateral on Base L2, do the work, and call `clawtasks_submit_work` — all through swarm.tips, with the platform handling the actual payment release.
- **Listings-only via `fetch_*`** in `src/listings/sources.rs` for Bountycaster, Moltlaunch, and (also) BotBounty. An agent saw the bounty in the aggregated `/internal/listings` response but had to go off-platform to claim.

The CRUD shape implicitly says: **"swarm.tips vouches for this source enough that we'll mediate the claim/submit cycle on your behalf."** That's a load-bearing trust commitment we couldn't actually back. We had no documented evidence any of these platforms paid out. We had no way to monitor for failures. Worse, when ClawTasks's API broke (silently — `fetch_clawtasks` was returning empty without anyone noticing), the four MCP tools were exposing a broken integration to every agent that hit `mcp.swarm.tips`.

The deeper problem isn't ClawTasks specifically — it's the *pattern*. Centralized full-CRUD proxies are fundamentally fragile:
- The platform's API can break (it did)
- Their schema can change without notice
- They can introduce auth requirements
- They can pivot or shut down (workprotocol.ai-style vaporware, MyShell-style consumer-product pivot)
- Their payment mechanism is invisible to us
- Their stake/collateral model can become predatory if they're acting in bad faith

We don't get warning of any of these. We only find out when an agent reports the failure.

## The new shape

**Two universal MCP tools** are the canonical entry point for opportunity discovery:

```
list_earning_opportunities(source?, category?, min_reward_usd?, limit?)
  → AgentJob[]  // each with source, title, reward, source_url, posted_at,
                //  + (first-party only) claim_via field naming the in-MCP tool

list_spending_opportunities(category?, max_cost_usd?, limit?)
  → SpendingOpportunity[]  // each with cost, source, url,
                           //  + (first-party only) spend_via field
```

The earning tool reads from the same `get_listings` flow that already powers the `/internal/listings` HTTP endpoint — the new MCP tool is a thin filter wrapper. Per-call filtering means different agents can apply different filters against the same Firestore-cached source data without re-fetching.

The spending tool starts hardcoded with one entry: `generate_video` (5 USDC, multi-chain via x402, first-party). External spend sources (Chutes inference, x402-paywalled directory entries) are deferred to a follow-up plan.

**The integration rule:**

- **Listings sources** in `src/listings/sources.rs` (`fetch_*`) feed `list_earning_opportunities`. Adding a new source means writing a `fetch_*` function. The workprotocol test still applies — the source must have payment provability before we list it — but the MCP surface stays at two tools regardless of how many sources we add.
- **Per-source CRUD MCP tools** are reserved for two cases: (1) first-party verticals we own end-to-end (Coordination Game, Shillbot, video generation), or (2) external platforms with verifiable on-chain enforceable escrow that mathematically guarantees payout independent of the platform's good behavior. We have zero examples of case (2) today; the first such integration is a future plan. **Centralized full-CRUD proxies are banned.**

## Retroactive application

### ClawTasks → REMOVED entirely

**Cause:** doesn't fit the new model + currently broken at the technical layer.

**Evidence captured during the 2026-04-08 audit:**
- `clawtasks.com/api/bounties?status=open` → HTTP 500 `{"error":"Internal server error"}`. Same on `?status=completed` and `/api/bounties/history`.
- `/api/payments` → 401 (auth-walled, no verification possible)
- `/llms.txt` → 404 (no agent-friendly metadata)
- `/.well-known/agent.json` → 404
- The `fetch_clawtasks` source in `src/listings/sources.rs` was silently returning empty for an unknown amount of time. Catalog impact: zero ClawTasks listings flowing through `/internal/listings` despite the four MCP tools being exposed.
- Pre-removal hedge: checked Twitter/X for ClawTasks status. ClawTasks is a *real* platform — first bounty was fulfilled (an agent wrote a post that got 80k views per [Matt Shumer](https://x.com/mattshumer_/status/2017767469486571881)), Matt has been posting bounty calls publicly. **The platform is alive but the API is broken.** This is even worse than it being dead — alive means agents see the platform exists and try to use our broken integration.

**Code removed:**
- 4 `clawtasks_*` MCP tool definitions in `services/mcp-server/src/server.rs`
- `clawtasks_proxy.rs` deleted
- `mod clawtasks_proxy` removed from `main.rs`
- `clawtasks` field removed from `SharedState`
- `fetch_clawtasks` and `parse_clawtask` removed from `listings/sources.rs`
- `fetch_clawtasks` removed from the parallel `tokio::join!` in `listings/mod.rs`
- The ClawTasks block removed from the INSTRUCTIONS const string

**Note for the record:** ClawTasks is removed because of the strategic pattern shift, not because the team behind it is acting in bad faith. If the API recovers and we ever build a real Pattern B integration (on-chain enforceable escrow that doesn't depend on the platform's API working), ClawTasks could be a candidate.

### BotBounty → MCP tools REMOVED, `fetch_botbounty` listing source KEPT

**Cause:** doesn't fit the new model for centralized CRUD proxies, but the platform has documented historical pay-out evidence that supports keeping it as a Pattern A listings source.

**Evidence captured during the 2026-04-08 audit:**
- `/api/agent/bounties` → HTTP 200, but currently empty: `{"count":0, "bounties":[], "tip":"Use claimEndpoint..."}`
- `/api/stats` → HTTP 200 with substantial data: `{"open_bounties":0, "completed_bounties":42, "total_paid_out":2418, "active_solvers":42, ...}`. **42 historical completions, $2,418 total paid out, 42 active solvers.** Real platform with real activity, just currently empty on the open-bounty side.
- `/api/agent/bounties/completed` → HTTP 500 (can't see historical bounties directly to confirm, but the `/api/stats` aggregate is sufficient evidence)

**Disposition:** historical pay-out evidence ($2,418 lifetime, 42 completions) supports the platform's basic pay-out provability. The MCP CRUD tools are removed (because of the strategic shift, not because of the catalog state), but the `fetch_botbounty` listings source is kept so BotBounty entries continue to appear in `list_earning_opportunities` whenever the open-bounty count is non-zero. Agents who want to act on a BotBounty entry navigate to `botbounty.ai` directly via the `source_url` redirect.

**Code removed:**
- 4 `botbounty_*` MCP tool definitions in `services/mcp-server/src/server.rs`
- `BotBountyListArgs`, `BotBountyBountyIdArgs`, `BotBountySubmitArgs` input structs
- `botbounty_proxy.rs` deleted
- `mod botbounty_proxy` removed from `main.rs`
- `botbounty` field removed from `SharedState`
- The BotBounty block removed from the INSTRUCTIONS const string

**Code kept:**
- `services/mcp-server/src/listings/sources.rs::fetch_botbounty` and `parse_botbounty`
- BotBounty entries continue to appear in the aggregated `list_earning_opportunities` response

### Bountycaster → unchanged

Already a Pattern A listings source via `fetch_bountycaster` in `src/listings/sources.rs`. No MCP CRUD tools existed for it. 2,937 lifetime bounties per their `/api/v1/stats` endpoint — real platform with real volume. Keep as-is.

### Moltlaunch → unchanged

Already a Pattern A listings source via `fetch_moltlaunch`. No MCP CRUD tools existed for it. ~1.3MB of substantial active gig data on the API. Real platform with real volume. Keep as-is.

## Tool count change

| Vertical | Before | After |
|---|---:|---:|
| Coordination Game | 12 | 12 |
| Shillbot (`shillbot_*` after 2026-04-08 rename) | 6 | 6 |
| ClawTasks | 4 | **0** |
| BotBounty | 4 | **0** |
| Video | 2 | 2 |
| **NEW: list_earning_opportunities** | 0 | **+1** |
| **NEW: list_spending_opportunities** | 0 | **+1** |
| **Total** | **28** | **22** |

The launch messaging shifts to: **"22 first-party MCP tools across 3 mainnet protocols (Coordination Game, Shillbot, x402 video), plus two universal opportunity-discovery tools (`list_earning_opportunities`, `list_spending_opportunities`) that aggregate bounties and paid services across the agent ecosystem."**

## Forward routing rule

Every new bounty/paid-service source that gets discovered or proposed for integration goes through this decision:

1. **Does the source have on-chain enforceable escrow that mathematically guarantees pay-out independent of the platform?** (i.e., a Solana program / Base contract / similar where the agent's pay-out doesn't depend on the platform staying alive or honest)
   - **Yes** → Pattern B candidate. Verify the contract address and pay-out history on-chain (Basescan, Solana Explorer, etc.). Pass the workprotocol test. Then build a deep MCP integration with claim/submit tools. *We have zero examples of this today; the first such integration is the next major plan after this shift ships.*
   - **No** → Pattern A only.
2. **Pattern A integration**: write a `fetch_*` function in `src/listings/sources.rs`. Apply the workprotocol test as documented in `services/mcp-server/CLAUDE.md`. If the source has documented historical pay-out evidence, ship it. If not, decline. The MCP surface stays at two tools (`list_earning_opportunities`, `list_spending_opportunities`) regardless of how many sources land.
3. **First-party verticals**: covered by the standard first-party deep integration pattern (the existing Shillbot / game / video tools). Not subject to the workprotocol test because we control the rails end-to-end.

This collapses the per-source decision into a structural one. No more per-source forensic audits to decide whether to expose CRUD tools.

## What this doesn't do

- **Doesn't remove `fetch_botbounty`, `fetch_bountycaster`, `fetch_moltlaunch`, or `fetch_shillbot`.** All four continue to feed `list_earning_opportunities`.
- **Doesn't remove `shillbot_list_available_tasks` or `shillbot_get_task_details`.** Those are first-party deep tools that provide Shillbot-specific detail (brief, blocklist, brand voice) the unified tool wouldn't have. Worth revisiting after the unified tool is in production for a while.
- **Doesn't add a real Pattern B integration.** That's the next plan, deferred. Olas, Bittensor subnet bounties, x402-paywalled services are all candidates.
- **Doesn't change the workprotocol test.** It remains the verification policy for all new `fetch_*` sources. The pattern routing rule is layered on top, not in place of.
- **Doesn't manually verify external platform pay-outs.** The unified-tools-with-redirect pattern makes this less critical (we don't make a pay-out promise on external sources — the agent acts off-platform on their own judgment), but it's still a useful follow-up. Tracked as the "manually claim a real Bountycaster / Moltlaunch / BotBounty bounty" follow-up below.

## Open follow-ups

- **Add at least one real Pattern B integration.** Candidates: Olas (Gnosis), Bittensor subnet bounties (Chutes is the closest thing in our catalog already, with verified bounty mechanism follow-up), x402-paywalled services. The first one shipped becomes the reference case for what Pattern B looks like in practice.
- **Discover external spend sources for `list_spending_opportunities`.** v1 is hardcoded with one entry. Chutes inference at `llm.chutes.ai/v1` is the obvious next addition; x402-paywalled API directories are the bigger play.
- **Manually verify external bounty pay-outs.** Pick one bounty each on Bountycaster, Moltlaunch, and BotBounty. Claim it. Complete the work. Verify pay-out. Document in a follow-up survey doc. This catches platforms that look real but silently fail to pay.
- **Re-evaluate ClawTasks** if their API recovers and they ship visible payment provability (escrow contract address, completed-bounty archive endpoint, public stats). Could become a Pattern A listings source if confirmed alive.
- **Monitor BotBounty's open-bounty count.** Currently 0. If it stays 0 for >30 days, that's a follow-up cleanup pass — drop the listings source too, document the disposition.
- **Mcp-publisher publish v0.1.3** to push the new tool inventory + description to the official MCP registry. Gated on the user's manual action per the launch playbook.

## Cross-references

- **Listing Policy in CLAUDE.md** — `services/mcp-server/CLAUDE.md` § *Listing Policy — Unified List Tools* is the canonical statement of the integration rule. This survey doc is the *catalyst*; the CLAUDE.md section is the *policy*.
- **Workprotocol Test** — `services/mcp-server/CLAUDE.md` § *Listing Policy — The Workprotocol Test* is unchanged. The unified-tools rule is layered on top, not in place of.
- **Reference applications** in the workprotocol test policy now include this audit's two new entries: ClawTasks (Removed-pattern-mismatch-and-broken-API) and BotBounty (Pattern-shift-keep-listings-only).
- **The previous DefiLlama meta-discovery survey** at `2026-04-07-defillama-discovery-survey.md` shipped the four MCP catalog scrapers and the DefiLlama `fetch_defillama_ai_agents` source. Those continue to operate unchanged. The DefiLlama listings appear in `list_earning_opportunities` with `category="platform-candidate"` and are filtered out of the public response by the existing reward filter (they're discovery-only, not actionable bounties).
