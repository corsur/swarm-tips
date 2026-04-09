# PulseMCP exhaustive earning sweep + retroactive Workprotocol Tests

**Date:** 2026-04-08
**Scope:** Sweep PulseMCP for crypto-native agent earning opportunities. Apply the
Workprotocol Test to every visible candidate. Retroactively test the integrated
sources (Moltlaunch, BotBounty) that predate the test policy.

## Executive summary

- **PulseMCP exhaustive sweep produced zero new earning sources for integration.**
  53 keyword queries returned 466 unique servers; the strict crypto+earning filter
  surfaced 25 candidates; only 3 pass the Workprotocol Test as labor markets
  (Elisym, nullpath, Mood Booster), and none of those three are `fetch_*`-shaped
  for the existing listings pattern. The agent-native MCP ecosystem is dominated
  by infrastructure (selling shovels), not labor markets (selling gold).

- **Retroactive Workprotocol Test on Moltlaunch: PASS** (172 completed tasks, ~196
  ETH lifetime activity across 43 active earning agents). An initial false-fail
  verdict was retracted after deeper probing — the failure mode was reading the
  `/api/gigs` list endpoint and the upstream GitHub README without querying the
  agent detail endpoint where actual completion counts live. Documented as a
  cautionary methodology note below.

- **Retroactive Workprotocol Test on BotBounty: PASS** (42 completed bounties,
  $2,418 lifetime payouts via the `/api/stats` endpoint). The integration
  currently produces zero listings because there are zero open bounties at probe
  time — that's correct integration behavior, not a failure. Survey-doc claims
  of "NPC theater" partially contradicted by live stats.

- **Two historical bounty platforms confirmed dead:** OnlyDust has explicitly shut
  down (company-wide closure notice on `app.onlydust.com`); Gitcoin Bounties is
  discontinued (URL redirects to the Gitcoin homepage, current product is Grants
  only).

- **Three additional candidates resolved as fails:** Algora (Stripe Connect rail),
  ArcAgent (Stripe Escrow rail), DirectPing Escrow (Base Sepolia testnet,
  unreachable production deployment).

- **Discovery blind spot identified.** Moltlaunch is not on PulseMCP, the official
  MCP registry, or any of our four `awesome-mcp-*` lists. Our entire discovery
  pipeline is biased toward platforms that ship MCP servers; the most interesting
  agent labor markets are web apps + smart contracts that don't bother with MCP.
  Closing this gap requires non-MCP discovery channels (ecosystem-specific awesome
  lists, ERC-8004 on-chain monitoring, agent-token launchpad scrapers).

## Methodology

### Sweep procedure

PulseMCP `v0beta` API at `https://api.pulsemcp.com/v0beta/servers?count_per_page=20&query=...`,
unauthenticated. **Note:** the v0beta API is being sunset (1% random failures
already, scaling to 100% by September 2026). The replacement v0.1 API at `/api`
requires credentials we don't yet have. Migration is on the deferred work list.

53 keyword queries spanning: bounty, crypto, wallet, solana, usdc, x402, defi,
earn, payment, escrow, stripe, token, ethereum, base, polygon, arbitrum, near,
sui, aptos, sei, gig, task, marketplace, freelance, work, hire, reward, grant,
hackathon, contest, prize, tip, airdrop, faucet, claim, mint, stake, yield,
swap, lend, borrow, micropayment, l402, lightning, onchain, smart contract,
agent, agentic, autonomous, workforce, oracle, attestation, switchboard,
chainlink, verification, verify, fund, treasury, dao, reputation, erc-8004.

466 unique servers returned across all queries. Strict crypto-native + earning
filter (regex match against chain/token symbols, earning verbs, and agent context)
narrowed to 50 candidates; structural review further narrowed to 25 candidates
worth probing with the full Workprotocol Test.

### Workprotocol Test gates (recap)

1. **Structural** — does the platform expose listings with structured pricing
   and on-chain references?
2. **On-chain** — can we verify escrow contracts and historical payouts?
3. **Independent payment evidence** — has the platform paid external claimants?
4. **Negative signal** — searches for non-payment, scam, ghosting reports
5. **LLM synthesis** — only when ambiguous

For this sweep, an additional crypto-native filter was applied as a
zeroth-step gate: **the payment rail must be a crypto wallet, not a bank
account or fiat payment processor.** This filter eliminates Stripe-rail
"agent" platforms (Algora, ArcAgent, Replit Bounties) immediately, since
autonomous AI agents can't hold bank accounts or 1099 tax identities.

## Verdict table — 25 visible candidates

| Source | Category | Chain | Pay rail | Verdict | Notes |
|---|---|---|---|---|---|
| **Elisym Protocol** | Earning (substrate) | Solana | SOL | **PASS (substrate)** | Nostr NIP-90 DVM, real npm/cargo packages, provider workflow ships. Not fetch_*-shaped — would need a Nostr relay client integration |
| **nullpath Marketplace** | Earning | Base L2 | x402 USDC | **UNCERTAIN — defer 30 days** | Real platform with proper API + MCP server. Homepage shows literal `0 agents, 0 transactions, $0 volume`. Genuinely Early Access. Re-check 2026-05-08 |
| **Mood Booster** | Tip + reputation | 6 chains | USDC | **PASS (small)** | ERC-8004 reference implementation. Tipping + on-chain feedback. Tiny but the ERC-8004 reputation primitive is the strategically interesting part |
| **Lightning Faucet** | Earning + spending | BTC Lightning | L402 + x402 | **PASS** | 37 MCP tools, "100 free sats" on signup. Has operator role for earning. |
| **402 Index** | Meta source (spending) | multi | L402 + x402 + MPP | **PASS as meta source** | 17,312 endpoints indexed, 7,005 payment-verified, hourly health checks. Aggregator of paid APIs |
| **BlockRun** | Spending (LLM gateway) | Base | x402 USDC | **PASS** | 41 LLMs, 1.1M monthly calls, Circle endorsement |
| **Sats4AI** | Spending (multi-tool) | BTC Lightning | L402 | **PASS** | 40+ tools, no signup, "from 2 sats" |
| **BTCFi API** | Spending (data) | BTC | x402 | **PASS** | Free tier with no signup |
| **Fortytwo Prime** | Spending (premium inference) | Monad/Base | x402Escrow USDC | **PASS** | Collective inference, "Rewards Program" mention |
| **AiPayGen** | Spending (tool catalog) | Base | USDC | **PASS** | 155 tools, 1 free call + free tier |
| **BotIndex** | Spending (data) | Solana | x402 USDC | **PASS** | 50 free premium calls |
| **x402 DeFi Data API** | Spending (data) | Base | x402 USDC | **PASS** | 8 tools, pay-per-call DeFi data |
| **API for Chads** | Spending (data) | Solana | x402 | **PASS** | Crypto prices, prediction markets, web research |
| **Coin Railz** | Spending (data) | multi | x402 | **PASS** | "API key in 60 seconds" |
| **PYTHIA Oracle** | Spending (small) | Base | x402 | **PASS (small)** | Single-tool oracle |
| **ClawSwap** | Spending (swap) | Solana↔Base | x402 USDC | **PASS** | Cross-chain swaps |
| **1ly Store** | Earning + spending (substrate) | Solana/Base | x402 USDC | **PASS (substrate)** | Two-sided marketplace, publish API to get paid. Not fetch_*-shaped |
| **DRAIN** | Infra (protocol) | Polygon | USDC | **TRACK** | ERC-8190 streaming micropayments draft. Protocol primitive, not a service |
| **P402** | Infra (protocol) | Base | x402 USDC | **TRACK** | x402 implementation primitive |
| **AgentPay** | Infra (protocol) | CKB Fiber | BTC | **TRACK** | BTC-native protocol on niche chain |
| **Lightning Memory** | Spending (memory) | BTC Lightning | L402 | **PASS (niche)** | Decentralized agent memory |
| **Revettr** | Spending (risk scoring) | Base | x402 USDC | **PASS (niche)** | Counterparty risk |
| **Agent Church** | Spending (joke) | Base | x402 | **PASS (joke?)** | "Spiritual services for AI agents" |
| **BoltzPay** | Meta source (spending) | BTC + Base | L402 + x402 | **PASS (overlaps 402 Index)** | Aggregator |
| **SolX402** | Infra (Solana x402 primitives) | Solana | x402 | **TRACK** | Wallet/discovery primitives |

### Failed before reaching the table (Stripe / fiat rail)

| Source | Reason |
|---|---|
| **Algora** | Direct: "STRIPE_SECRET_KEY for Escrow charges and Connect payouts" in their docs. Confirmed structural fail. |
| **ArcAgent** | README explicitly says "**Stripe Escrow** — one-way state machine". Confirmed structural fail despite the otherwise impressive 8-gate Firecracker microVM verification pipeline. |

### Failed (testnet / dormant)

| Source | Reason |
|---|---|
| **DirectPing Escrow** | README says "Network: Base Sepolia (testnet)". `0xdirectping.com` unreachable across two probe sessions. GitHub repo last pushed 2026-02-24, 2 stars. Production deployment is offline. |

## Free incentives discovered

Concrete claimable items, ranked by ease:

1. **Lightning Faucet — 100 free sats on signup** (~$0.10). `npm install -g lightning-wallet-mcp` + self-registration. Also gives you a working L402+x402 wallet for 17K endpoints in 402 Index.
2. **BTCFi API — fully free tier, no signup** for basic Bitcoin tools.
3. **Sats4AI — no signup ever**, 40+ AI tools accessible the moment you have a Lightning wallet.
4. **BotIndex — 50 free premium calls** (signup required).
5. **AiPayGen — 1 free call + free tier** (signup required).
6. **Mood Booster ERC-8004 airdrop position** — for ~$0.01 (one 0.001 USDC tip per chain), get a wallet recorded in the on-chain ERC-8004 Identity Registry. README explicitly says "your wallet eligible for future ecosystem airdrops."
7. **nullpath early-builder registration** — zero current activity = founder position on a real product with no competition.
8. **Fortytwo Prime "Rewards Program"** — mentioned in nav, contents unverified.

Total cost to capture all of the above: ~$0.01 in gas + ~30 minutes of setup.

## Retroactive Workprotocol Tests on integrated sources

### Moltlaunch — PASS (corrected from initial false-fail)

**Final verdict:** PASS. Keep the integration.

**Evidence (verified by querying `/api/agents/{id}` for all 309 unique agents):**

- 172 completed tasks across all agents
- 196.03 ETH in total earnings (~$392K at $2K/ETH)
- 72 reputation count
- **43 agents with completedTasks > 0**
- **60 agents with non-zero earnings**

Top earners:

| Agent | Done | Earned (ETH) | Rep |
|---|---|---|---|
| Moltlaunch | 10 | 70.59 | 5 |
| ODEI AI | 6 | 67.30 | 0 |
| Osobot | 4 | 49.07 | 4 |
| AJ Jr | 0 | 3.38 | 0 |
| DOLT | 5 | 1.02 | 5 |
| ChaosTheory | 1 | 0.96 | 1 |
| 0xLaVaN | 5 | 0.95 | 3 |

**Caveat:** the `totalEarningsETH` field appears to aggregate labor payments +
agent token swap fees (Moltlaunch agents have Flaunch tokens that earn trading
fees). The labor-specific income is closer to "172 completed tasks" than the
196 ETH headline number — but both are real, and the labor activity alone is
well above any reasonable Workprotocol Test threshold.

**How the false-fail happened (cautionary methodology note):**

1. Read the `/api/gigs` list endpoint and noted that 2,234 returned gigs all
   had `escrowAddress: null`, `contractAddress: null`, `txHash: null`,
   `completedAt: null`.
2. Misread these absent fields as evidence that the on-chain escrow claim was
   false.
3. Read the upstream GitHub README at `nikshepsvn/moltlaunch` and saw it
   describing a token launch CLI, not a gig marketplace. Interpreted this as
   "the actual product is a token launcher, the gigs are bolt-on hollow data."
4. Found the `eltociear/awesome-molt-ecosystem` repo with a "MoltGigs - Status:
   SCAM" entry. Conflated "MoltGigs" (a 4-gig surface) with "Moltlaunch" (a
   2,234-gig marketplace). Different products with similar names.
5. Drafted a removal recommendation.

**What the actual workprotocol test required and I skipped:**

Querying any single `/api/agents/{id}` endpoint returns the real `completedTasks`
and `totalEarningsETH` values. NI-KA returned `completedTasks: 0` on the first
probe, which I treated as confirmation of failure. The mistake was not
sampling more agents — the very next agents in the ID space had non-zero values.
**A 30-second sample of 5 agents would have flipped the verdict immediately.**

**The corrected schema understanding:**

The `/api/gigs` endpoint returns **gig OFFERS**, not active escrows. There's no
`escrowAddress` because there's no escrow at offer time — the escrow comes into
existence at the moment a client commissions the agent for a specific gig. This
is a sensible schema, not a hollow one. Per-agent activity lives at
`/api/agents/{id}` in the `completedTasks` and `totalEarningsETH` fields.

The actual Moltlaunch product at `moltlaunch.com` is a full marketplace with
tabs for Marketplace, Agents, Gigs, Bounties, Rankings, Starlight (an AI
matchmaker), Token, Dashboard. Footer says "Base Mainnet | ERC-8004". The
upstream GitHub README describes only the CLI primitive — the live product is
substantially more than what the OSS readme covers.

**Lesson encoded for future probes:** for any platform that matters
strategically, **probe at least two layers deep AND query for actual payment
counts before recommending removal or non-integration.** The cheap probes
(homepage scraping, README reading, list endpoint sampling) are useful for
triage and discovery, but never for destructive verdicts.

### BotBounty — PASS (currently quiet, real lifetime activity)

**Final verdict:** PASS. Keep the integration as-is. The current empty state
is data, not a bug.

**Evidence from `/api/stats`:**

```json
{
  "open_bounties": 0,
  "total_value_available": 0,
  "completed_bounties": 42,
  "total_paid_out": 2418,
  "active_solvers": 42,
  "categories": { ... all 0 ... },
  "message": "Real opportunities. Real money. Start earning now."
}
```

**Interpretation:**

- 42 completed bounties = real labor activity
- $2,418 lifetime payouts = real money has flowed
- 42 active solvers = real participants
- 0 open bounties at probe time = current quiet period, not platform death

The integration's current empty output is the correct behavior. When new
bounties get posted, `fetch_botbounty` will surface them automatically.

**Survey doc claim partially contradicted:** the `eltociear/awesome-molt-ecosystem`
repo claims "BotBounty - 102 bounties completed - NPCs claimed everything. $0
for us." The live stats show 42 bounties (not 102), and there have been real
payouts ($2,418). The survey author may have been one researcher who couldn't
claim, but the platform processed real money to other entities.

**Recommended improvement (deferred to a follow-up PR):** add a `/api/stats`
query to `fetch_botbounty`'s health check, store the resulting numbers in the
existing `SourceHealthDoc` Firestore record. Continuous workprotocol-test
signal at zero cost. If `total_paid_out` grows over time, the platform is
healthy. If it stagnates indefinitely, that's evidence to re-evaluate. This
pattern probably applies to Moltlaunch and Bountycaster too.

## Conclusive verdicts on previously inconclusive sources

### OnlyDust — DEAD (confirmed shutdown)

`app.onlydust.com` returns:

> **OnlyDust Has Closed.** Thank you for the journey. We're proud of what we
> built together. After an incredible journey empowering open source
> contributors, we've decided to close this chapter. Thank you to our amazing
> community for making this adventure possible.
> With gratitude • The OnlyDust Team

The company has shut down. The marketing site at `onlydust.com` ("We back
engineers working on problems too complex for AI to solve") is a separate
surviving entity — possibly a successor brand, possibly leftover, unclear.
The actual contributor product is gone.

### Gitcoin Bounties — DEAD (product discontinued)

`bounties.gitcoin.co` redirects to `gitcoin.co` (the company homepage). The
bounties product is discontinued. Gitcoin's current product is **Gitcoin
Grants 24** (quadratic funding rounds for grants applications), which is
fundamentally different from agent-claimable bounties (application-gated,
human review, episodic).

Stop scouting Gitcoin for bounty integrations. If we ever want grant-shaped
earning sources, that's a different listing category.

### Layer3 — PIVOTED (no public quest API)

`app.layer3.xyz/quests` exists (returns HTTP 403, not 404 — the path is real).
But `api.layer3.xyz` has no DNS entry — there's no public API. The quest
product still exists internally but it's now per-user authenticated, not
browseable as a flat list. Pivoted to a wallet-connected experience with no
machine-readable surface.

Confirmed not integratable.

### nullpath — UNCERTAIN, defer 30 days

Real platform with proper architecture:

- Real REST API at `nullpath.com/api/v1/agents` (returns structured help text,
  not 404)
- Real MCP server at `nullpath.com/mcp` with proper `Mcp-Session-Id` header auth
- Documented x402 USDC payment flow on Base L2

But homepage stats are literal zeros (`0 agents, 0 transactions, $0 volume`),
and there's no flat agent-list endpoint to verify activity from outside (the
API requires per-agent ID lookups). Genuinely Early Access stage.

**Action: re-check 2026-05-08.** If non-zero activity by then, run the full
Workprotocol Test.

## Strategic findings

### The infrastructure-vs-labor pattern

The sharpest finding from the entire sweep: **the crypto-native AI agent
ecosystem on PulseMCP is overwhelmingly infrastructure, not labor markets.**

Sort the 25 candidates by what they actually are:

| Layer | Count | Examples |
|---|---|---|
| Payment protocols / rails | ~6 | x402, L402, P402, DRAIN (ERC-8190), AgentPay (CKB Fiber), BoltzPay |
| Escrow / trust primitives | ~4 | DirectPing, Revettr, Aegis, Agora402 |
| Inference & API gateways | ~5 | BlockRun, Fortytwo Prime, AiPayGen, Sats4AI, ClawRouter |
| Data / utility services | ~6 | BTCFi, Coin Railz, BotIndex, x402 DeFi Data, API for Chads, PYTHIA |
| Wallets / identity | ~4 | Lightning Faucet, SolX402, ASG Card, Lightning Memory |
| Directories / meta | ~2 | 402 Index, BoltzPay |
| Marketplaces (substrate, not tasks) | ~3 | nullpath, 1ly Store, Elisym |
| Reputation primitives | ~1 | Mood Booster (ERC-8004 demo) |
| **Actual streams of paid agent tasks** | **~0** | — |

Even the marketplaces (nullpath, 1ly Store, Elisym) are substrate — places
where someone could post a job and someone could claim it, but they aren't
pre-populated with work. nullpath has literal zeros. Elisym is a Nostr relay
subscription model where activity depends entirely on whether anyone is posting
NIP-90 jobs (and very few are). 1ly Store is a "publish your API to get paid"
shape (one-time setup, not bounty stream).

**Why this happens structurally:**

1. **Selling infrastructure is easier than running a labor market.** An API
   gateway can ship in a weekend. A bounty marketplace needs creators (the
   hard part), workers, verification, dispute resolution, escrow, and trust.
2. **Verification is the actual bottleneck.** Trustlessly verifying arbitrary
   work is unsolved. Every "agent bounty marketplace" hits this wall and either
   sidesteps with fiat + identity (ArcAgent → Stripe), sidesteps with naive
   trust-the-creator (DirectPing), or has no customers yet (nullpath).
3. **Network effects flow to first-movers in marketplaces.** Until one or two
   actually have liquidity, they all look identical and none of them attract
   supply or demand.

### Bittensor / Olas / Ritual / Fetch.ai are infrastructure, not gigs

Initial framing of these as "gig marketplaces" was wrong. Each subnet on
Bittensor is "miners run a model, validator scores it, emissions flow." The
validator's queries aren't real customer demand — they're synthetic scoring.
The miner is selling capacity to the protocol, which then redistributes
protocol-printed tokens. Same for Olas services and Ritual: agents publish
services that get called by clients, but in practice most calls are subsidized
by protocol emissions or treasury, not customer pull.

Income from protocol emissions = infrastructure-supply business (like crypto
mining). Income from clients with specific tasks paying for those tasks = labor
market. The two are different even when the surface descriptions sound similar.

### The actual global supply of crypto-native agent labor markets

After two sessions of probing (PulseMCP exhaustive sweep + non-agent-native
Tier 1 scout), the integratable supply is approximately:

| Source | Status |
|---|---|
| Shillbot | First-party (us) — oracle-attested verification, on-chain escrow, real paying clients |
| Moltlaunch | Integrated, 172 completed tasks + token swap activity, growing |
| Bountycaster | Integrated, Farcaster-native, low volume |
| BotBounty | Integrated, $2,418 lifetime, currently quiet |

**Four total. Three integrated, one of them us.** Two of the most-cited
historical bounty platforms (Gitcoin Bounties, OnlyDust) are confirmed dead.
The successor candidates from the agent-native MCP ecosystem (DirectPing,
ArcAgent, nullpath) are fiat-gated, dormant, or genuinely Early Access.

**This is the actual structure of the crypto-native agent labor market in 2026:
genuinely small, with us as one of the main suppliers.**

## Discovery blind spot (root cause)

Moltlaunch is **not on PulseMCP**, **not in the official MCP registry**, and
**not in any of our four `awesome-mcp-*` lists.** It was added to our codebase
manually, before the Workprotocol Test was formalized.

Reason: **Moltlaunch doesn't ship an MCP server.** It's a CLI tool
(`npx moltlaunch`) plus a web marketplace at `moltlaunch.com`. Our entire
discovery pipeline is biased toward platforms that publish MCP servers.

### Channels we currently use

| Channel | What it finds | What it misses |
|---|---|---|
| Official MCP registry | Things that registered as MCP servers | Anything without an MCP server |
| PulseMCP | Things on PulseMCP's catalog | Same |
| 4 awesome-mcp lists | MCP servers people thought to add to a list | Non-MCP agent infrastructure |
| DefiLlama AI Agents | DeFi-classified agent protocols | Agent marketplaces that aren't DeFi |
| `fetch_*` listings sources | Sources we already integrated | Anything we haven't found yet |

### Channels we should add (ranked by leverage)

1. **Ecosystem-specific awesome lists.** The `eltociear/awesome-molt-ecosystem`
   repo lists Moltlaunch directly alongside ~180 other Molt-ecosystem platforms
   with per-entry verdicts. Same scraper pattern as our existing
   `awesome-mcp-*` integrations. Build effort: ~2 hours.
   Other candidates: `awesome-base`, `awesome-virtuals`, `awesome-solana-agent-ecosystem`.
2. **ERC-8004 on-chain Identity Registry monitoring.** Contract
   `0x8004A169FB4a3325136EB29fA0ceB6D2e539a432` is deployed on BSC, Base,
   Ethereum, Arbitrum, Optimism, Polygon. Every Moltlaunch agent IS an
   ERC-8004 token. Monitoring new mintings + resolving `tokenURI` →
   marketplace URL automatically discovers every ERC-8004-compliant
   marketplace. Build effort: ~0.5-1 day. Single most structurally interesting
   move — catalog grows automatically as new marketplaces ship.
3. **Coinbase Bazaar / Base ecosystem catalog at `base.org`.** Coinbase
   maintains a curated apps directory. Likely has agent marketplaces tagged.
   Single page scrape.
4. **Agent-token launchpad monitoring (Flaunch, Virtuals).** Moltlaunch agents
   launch tokens via Flaunch. Watching the launchpads for agent-tagged launches
   surfaces marketplaces via the token primitive.
5. **DefiLlama broader category sweep.** Currently only the AI Agents category.
   Moltlaunch may be tagged differently — Marketplace, Launchpad, Other.
6. **GitHub topic search.** Tags: `ai-agents`, `agent-marketplace`,
   `bounty-marketplace`, `agent-economy`. OSS-only but cheap.
7. **npm / crates.io search.** Catches CLI-distributed marketplaces (Moltlaunch
   ships as `npx moltlaunch`).
8. **Hackathon project showcases (ETHGlobal, Solana, Base).** Many agent
   platforms first ship as hackathon entries. Episodic.
9. **Social listening on X / Farcaster** following specific curators
   (eltociear, Base ecosystem accounts). Hard to automate, high precision per
   signal.

## Recommended follow-ups (deferred work)

1. **Build `fetch_awesome_molt_ecosystem`** as the first non-MCP discovery
   source. Validates the pattern. Surfaces ~180 platforms in one PR. Highest
   ROI per hour from this entire session's findings.
2. **Plan ERC-8004 Identity Registry monitor** as a focused planning round.
   Catalog growth that scales automatically with the ecosystem.
3. **Migrate to PulseMCP `v0.1` API** — credentials needed before September
   2026, when v0beta sunsets fully. Email already drafted in earlier session.
4. **Add `/api/stats` to `fetch_botbounty` health check** as continuous
   workprotocol-test signal. Generalize the pattern to Moltlaunch
   (`/api/agents/{id}` aggregation) and Bountycaster.
5. **Re-check nullpath 2026-05-08.** If non-zero activity, run the workprotocol
   test and consider integration.
6. **Add the verdict-table candidates (Mood Booster, ERC-8004 reference) as a
   reputation primitive note in CLAUDE.md.** ERC-8004 is the closest existing
   standard to the agent-reputation work the future swarm.tips meta-governance
   layer will need.
7. **Curate ~5 marquee spending entries** (BlockRun, Sats4AI, Fortytwo,
   AiPayGen, BTCFi) into `first_party_opportunities()` in `spending.rs`.
   Hand-curated, no aggregation, no quality risk. Deferred from this session
   pending an explicit decision on the spending-side filter design.

## Lessons encoded for future investigations

1. **Probe at the right layer.** Reading a list endpoint or a homepage is
   triage, not verdict. Per-entity detail endpoints (where actual completion
   counts live) are the workprotocol-test gate.
2. **Sample more than one entity.** If the first agent has zero completed
   tasks, that's one data point, not a verdict. Sample 5-10 before concluding.
3. **Don't conflate platforms with similar names.** "MoltGigs" is not
   "Moltlaunch." Similar-named projects in the same ecosystem can have
   completely different reputations.
4. **The OSS GitHub README is not the live product.** Live homepages can be
   substantially more than what the upstream readme describes — especially for
   SaaS or marketplace products built on top of an OSS primitive.
5. **For destructive recommendations, require multiple converging signals AND
   activity-layer verification.** Single survey-doc claims are not enough.
   API stats endpoints are the gold standard.
6. **Discovery is biased by the channels you use.** If your sweeps all return
   "infrastructure not labor," the answer might not be "labor doesn't exist"
   — it might be "labor doesn't ship MCP servers." Diversify discovery
   channels before concluding scarcity.
