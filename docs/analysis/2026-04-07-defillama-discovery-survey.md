# Discovery Expansion — Meta-Discovery + MCP Catalog Survey

**Date:** 2026-04-07
**Tracks shipped today:**
- **Track A (MCP catalog expansion):** added 3 new Layer 1 sources to `src/discovery/sources.rs` — `wong2/awesome-mcp-servers`, `appcypher/awesome-mcp-servers`, `tolkonepiu/best-of-mcp-servers`. Estimated combined yield: ~1000 new entries before cross-source dedupe (vs ~2000 from the official registry alone). PulseMCP and Smithery deferred (auth + API verification respectively).
- **Track B (meta-discovery):** added DefiLlama "AI Agents" + "Decentralized AI" categories to `src/listings/sources.rs::fetch_defillama_ai_agents`. Surfaced 27 protocols. **Detail below.**

**Source for Track B detail:** `https://api.llama.fi/protocols`, filtered to `category in {"AI Agents", "Decentralized AI"}`
**Method:** one-shot pull executed during the implementation of `services/mcp-server/src/listings/sources.rs::fetch_defillama_ai_agents`. No auth required. Field set captured: `name`, `slug`, `category`, `url`, `chains[]`, `tvl`, `description`, `twitter`, `listedAt`. Per-protocol verdicts below are pattern-match triage — none of these have been probed for actual earning APIs yet.

## Why this survey exists

The 2026-04-07 official-MCP-registry survey (`2026-04-07-arbitrage-survey.md`) found exactly one verified earning platform (workprotocol.ai, since revealed as vaporware) and one verified-but-empty commerce platform (midas-protocol). The catalog was too small to surface signal.

This is the first meta-discovery scan: instead of asking *"which MCP servers expose earning tools?"*, it asks *"which crypto-native agent platforms have launched in the last 18 months?"* DefiLlama tracks 27 such protocols today across two adjacent categories. The expectation isn't that any of these are MCP-shaped — it's that some of them host the kind of priced-gig marketplace that Moltlaunch turned out to be, and those become candidates for direct integration in `src/listings/sources.rs` the way Moltlaunch was added by hand.

The companion code change (`fetch_defillama_ai_agents`) makes this scan repeatable on every listings refresh. New platforms entering the AI Agents category appear automatically; the existing reward filter drops them from the public listings response (they have no individual-job rewards) but persists them to Firestore for survey work like this.

## Headline numbers

- **27 protocols** total — 23 in `AI Agents`, 4 in `Decentralized AI`
- **5 with meaningful TVL (>$100K)**: Giza, MorpheusAI, Capx AI, Infinite Trading Protocol, AgentFi
- **9 with TVL = 0 but listed**: presence signal only
- **9 with TVL = null**: TVL untracked, often pre-launch or token-only
- **1 unexpected bonus finding**: DefiLlama itself runs an MCP server at `defillama.com/mcp` — surfaced incidentally during research, worth tracking as a peer/competitor in the discovery space.

## Tier A — meaningful TVL (>$100K)

| Name | TVL | Chains | Shape | Verdict |
|------|----:|--------|-------|---------|
| **Giza** | $16.7M | Base, Arbitrum, Hyperliquid L1, Plasma | Infrastructure: non-custodial algo agents executing DeFi strategies | **Skip.** Infrastructure, not a marketplace. Agents are first-party. No earning loop for swarm.tips operators. |
| **MorpheusAI** | $14.7M | Ethereum | Infrastructure: decentralized AI compute, yield-powered tokenomics | **Skip.** Infrastructure layer. Token incentives target compute providers, not job-completing agents. |
| **Capx AI** | $2.3M | Capx Chain (own L2) | Launchpad: AI Apps issue ERC20s | **Skip.** Token launchpad, not a job marketplace. Adjacent but not a fit. |
| **Infinite Trading Protocol** | $231K | Optimism, Base, Polygon, Arbitrum | **Marketplace of priced automated trading strategies** | **PROBE — top candidate.** Closest shape match to Moltlaunch in this set. "Marketplace and protocol that enables you to invest in fully automated trading strategies" — agents publish strategies, users hire/invest. Worth a manual API probe. URL: https://www.infinitetrading.io/ |
| **AgentFi** | $179K | Blast | Agent creation + sharing on Blast | **PROBE.** "Allowing anyone to create, customise and share onchain agents" — sharing implies a marketplace shape. Single-chain (Blast) limits reach. URL: https://agentfi.io/ |

## Tier B — small TVL ($1 – $100K)

| Name | TVL | Shape | Verdict |
|------|----:|-------|---------|
| Mind Network | $67K | FHE infrastructure | Skip. Encryption infra, not earning. |
| Gud.Tech | $42K | "AI for the attention economy" on Zircuit | Skip. Vague positioning, single-chain, low signal. |
| Calculus | $3K | On-chain framework for autonomous trading agents on BNB | Skip. Framework, not a marketplace. |
| Glorb | $704 | Single autonomous AI agent on Base, runs onchain games | Skip. Single agent, not a marketplace. |

## Tier C — zero TVL, DefiLlama-listed (presence signal only)

| Name | Chains | Shape | Verdict |
|------|--------|-------|---------|
| Vader AI | Base | Single farming/staking agent | Skip. Single agent. |
| Botto | Ethereum, Base | Decentralized autonomous artist (DAO) | Skip. Single agent. |
| Yoko | Sonic | No-code platform for launching AI Agents | **PROBE-LATER.** Launchpad shape. Sonic-only limits reach. |
| Laika AI | Binance | Web3 AI layer: RAG, APIs, analytics | Skip. Tooling, not earning. |
| Arbius | Arbitrum | Decentralized AI inference network (miners run models) | **PROBE-LATER.** Mining/inference earning loop — different shape from gigs but agents could resell inference. |
| INFINIT | 10 chains | DeFi navigation via AI agents | Skip. Execution layer for end users. |
| OpenLedger | Ethereum, Binance, Open | On-chain AI training + attribution | Skip. Infrastructure. |
| FLock.io | Base | Private AI training platform | Skip. Training infra. |
| Chutes | Bittensor | Decentralized serverless AI compute | **PROBE-LATER.** Compute marketplace — agents could earn by serving requests. Bittensor-native. |

## Tier D — TVL null (TVL untracked)

| Name | Shape | Verdict |
|------|-------|---------|
| **Virtuals Protocol** | Society of AI Agents (Base) — launchpad + commerce | **PROBE — second-tier candidate.** Already cited as a deferred meta-discovery target in the 2026-04-07 arbitrage survey. DefiLlama catches it automatically here. URL: https://app.virtuals.io/ |
| **Creator Bid** | Tokenize AI agents, AI launchpad on Base | **PROBE-LATER.** Launchpad-adjacent to Virtuals. Worth checking whether agents have priced services. |
| **MyShell** | "Creators build, share, and monetize AI agents" | **PROBE — third-tier candidate.** Explicit marketplace language. URL: https://myshell.ai/ |
| **Xeleb Protocol** | "AI Agent Influencer Hub where AI delivers real utility" | **PROBE.** Possible Shillbot peer — if it's an influencer-marketing marketplace, that's directly competitive and instructive. URL: https://xeleb.io/ |
| Cookie DAO | Modular data layer for the AI-driven reality | Skip. Data infrastructure. |
| Finder Bot | Crypto-trading tool, expert picks + bot | Skip. End-user tool. |
| QuantixAI | AI-driven trading software | Skip. End-user tool. |
| Alaya AI | Web3 data sampling + auto-labelling | Skip. Data infrastructure. |
| CARV | AI chain ecosystem for data sovereignty | Skip. Infrastructure. |

## Candidates worth pursuing (ranked)

1. **Infinite Trading Protocol** — strongest Moltlaunch-shape match. Real TVL ($231K), multi-chain, explicit marketplace language. Next step: probe `infinitetrading.io/api/*` for a strategy-listing endpoint analogous to `api.moltlaunch.com/api/gigs`.
2. **Virtuals Protocol** — was already on the deferred list from the 2026-04-07 survey; DefiLlama's catalog confirms it's catchable automatically. Society-of-agents framing implies internal commerce. Next step: check the [docs.virtuals.io](https://docs.virtuals.io) API surface.
3. **MyShell** — explicit "creators monetize AI agents" framing. Next step: probe `myshell.ai/api/*` and check whether the marketplace exposes agent listings.
4. **AgentFi** — sharing-implies-marketplace + non-zero TVL. Smaller scope (Blast-only) but real users. Next step: probe `agentfi.io/api/*`.
5. **Xeleb Protocol** — most interesting *competitor* finding. If it's an AI-influencer-marketing marketplace, it's directly in Shillbot's lane and worth understanding before it grows. Next step: visit the site, check whether they have a public agent-onboarding flow.

## Non-candidates that are still useful intel

- **Giza** and **MorpheusAI** are large enough ($30M+ combined TVL) that they're the de-facto leaders in autonomous-agent infrastructure on EVM. We don't earn from them, but their public docs are the canonical reference for what "production agent infra" looks like in 2026.
- **Arbius** and **Chutes** represent a different earning shape: decentralized compute. Not a fit for swarm.tips today (we sell jobs, not GPU time), but if Shillbot ever needs cheap inference for video generation, these are candidates.
- **DefiLlama itself runs an MCP server** at `defillama.com/mcp`. Surfaced incidentally during this research. It's not in our catalog as a peer. Worth adding it to the official MCP registry scan to see how it classifies and what tools it exposes.

## What didn't work

- **First WebFetch returned a sampled subset** of the DefiLlama `/protocols` endpoint and reported "no AI Agents category exists". The category does exist with 23 entries — direct `curl + python` introspection confirmed. Lesson: when a fetch tool reports "no matches", verify with raw HTTP before deciding.
- **The category split is inconsistent.** "AI Agents" and "Decentralized AI" overlap heavily (FLock.io is in Decentralized AI, Capx AI is in AI Agents — both involve training and agent issuance). The Layer 1 fetcher includes both to avoid missing entries; future work could collapse them or run pattern-matching across all categories instead of relying on DefiLlama's own taxonomy.
- **TVL is a noisy signal.** 18 of 27 protocols have TVL of 0 or null, including Virtuals Protocol — one of the most prominent agent platforms in the space. TVL zero does not mean the platform is dead; it often means DefiLlama doesn't have an adapter for the chain or the protocol's value isn't TVL-shaped. Triage by description first, TVL second.

## Track A — MCP catalog scrapers (shipped, not yet surveyed)

The three new Layer 1 sources land in this PR but the per-entry classification survey is deferred to a follow-up doc once a production refresh has run and the merged + Layer-2-classified output is in Firestore. Quick parser estimates against the live READMEs (April 2026 snapshots):

| Source | Repo | Estimated entries | Format |
|--------|------|------------------:|--------|
| `awesome-wong2` | `wong2/awesome-mcp-servers` | ~479 | `- **[Name](url)** - desc` markdown bullets, last pushed 2026-04-06 |
| `awesome-appcypher` | `appcypher/awesome-mcp-servers` | ~180 | `- <img...> [Name](url) - desc` markdown bullets, last pushed 2025-09-04 (stale but largest by stars) |
| `best-of-mcp` | `tolkonepiu/best-of-mcp-servers` | ~406 | `<details><summary>` HTML blocks with rank notation, updated weekly |

**Original plan named `punkpeye/awesome-mcp-servers`** — that repo was deleted between plan-time and implementation-time (404 from GitHub). Swapped in `wong2/awesome-mcp-servers` (3880 stars, actively maintained) as the replacement. Lesson for the plan playbook: verify upstream URLs exist immediately before coding, not at plan-write-time.

**Cross-source dedupe limitation noted:** the merge layer dedupes by lowercased `name`, so `Apify` from wong2 won't dedupe with `io.github.apify/actors-mcp-server` from the official registry. A `github_repo`-keyed dedupe would catch these but is left as a follow-up — for v1 the duplication is visible but harmless (the LLM classifier sees both rows and produces consistent verdicts).

**Follow-up survey doc:** once the next production discovery refresh runs (manual trigger or daily Workflow), generate a per-entry verdict doc the same way this one was written for DefiLlama. The interesting query is: "which entries surfaced from awesome-list or best-of-mcp DON'T appear in the official registry?" — those are the new-to-us catalog growth.

## Open follow-ups

- **Manually probe the 5 ranked candidates** above for public APIs. Time-box at 30 minutes per candidate. Anything that returns a structured listing payload becomes a `fetch_*` function in `src/listings/sources.rs` the same way Moltlaunch was added.
- **Add `defillama.com/mcp` to the official MCP registry scan results doc** — it should appear automatically next time `pull_official_registry` runs, but the 2026-04-07 arbitrage survey doc didn't mention it, so confirm the listing exists and capture the tool inventory.
- **Schedule a recurring DefiLlama scan diff.** This survey is a snapshot. The point of automating the source is that next month's scan will surface whatever's *new* — that diff is the actionable signal, not the absolute list. Once Google Workflows runs the listings refresh on schedule, write a small diff query: protocols in this scan that weren't in the previous scan = "new agent platforms launched this period".
- **Decide what "platform-candidate" listings should do in the public response.** Currently filtered out by the reward filter. If we want a public `/internal/listings/platforms` endpoint for the swarm.tips frontend to surface "agent platforms worth knowing about" (separate from "jobs you can earn from right now"), that's a follow-up.
