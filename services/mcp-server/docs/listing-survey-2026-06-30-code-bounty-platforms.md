> Historical snapshot (2026-06-30, from the file's own date) — not maintained.

# Listing Survey — Code/Agent Bounty Platforms (2026-06-30)

Applied the Workprotocol Test (payment provability is the bar) **plus** two swarm.tips-specific gates that payment-provability alone doesn't cover:
- **Agent-claimable:** can an *autonomous AI agent* realistically claim and get paid (no KYC/fiat gate, no human-only work, no capital-at-risk stake it can't reason about)?
- **Add-value / non-duplication:** does swarm.tips add anything beyond a redirect, given existing aggregators (BountyHunte.rs, BountyHunt.xyz, bbradar) already index the security set?

**Headline:** the big-crypto-money platforms all **PASS payment but FAIL agent-claimability** (human-expert security work, subjective triage, KYC/stake gates) and are already aggregated elsewhere. Only **0xWork** is agent-native — and it's a **conditional** add (real but tiny, token-stake friction, subjective approval).

| Platform | Workprotocol | Agent-claimable | Add to swarm.tips | One-line reason |
|---|---|---|---|---|
| **0xWork** | **PASS** | Partial | **ADD (qualifies)** | Payment-provable ($8K / 510 tasks paid), agent-claimable; self-posted (Axobotl) demand is fine. Caveat: board often thin + current 2 live tasks are uncompletable junk (per-task quality, handled by redirect model) |
| **Pump.fun GO** | **FAIL** | No | **NO** | Custodial/discretionary payout (sole non-appealable arbiter); explicitly rejects AI; work is physical stunts |
| **Immunefi** | PASS | No | **NO** | KYC-gated, human-judged severity, novel human-level vuln discovery; already MCP/API-aggregated |
| **Cantina** | PASS | No | NO (human-researcher link only) | Competitive human-expert audit contests; no public API; BountyHunte.rs covers it |
| **Sherlock** | PASS | No | **NO** | Human-expert contests + $250 stake-to-submit + ≥20% valid-ratio gate = hostile to autonomous agents |

---

## 0xWork — PASS / CONDITIONAL

Site `0xwork.org` (Base mainnet, chainId 8453). "Your Agents. Your Income." Crypto-native: CLI/SDK/API + WebSocket/XMTP task push + x402 micropayments; "No API keys. No accounts. Just a wallet." No KYC/fiat gate.
- **On-chain payout proof:** escrow `TaskPoolV4 0xF404aFdbA46e05Af7B395FB45c43e66dB549C6D2` (source-verified, BaseScan) — 1,430 txns, latest 2026-06-18, USDC outflow to **15+ distinct external recipients** in bounty-sized amounts ($0.04–$245). USDC actually leaves to many wallets — not zero, not team-recycling. Clears the payment gate.
- **Live board is empty of real work (the disqualifier, checked 2026-06-30 via `api.0xwork.org/tasks`):** the only 2 open tasks are **#391** (50 USDC, "get @jessepollak to follow @Inner_Axiom on X") and **#390** (50 USDC, "get @jessepollak to RT/QT a post mentioning 0xWork", **22 failed attempts**). Both posted by **Axobotl — the platform's own bootstrap agent**, both stale since 2026-03-30, both essentially uncompletable (depend on a specific third party's behavior). No external client demand; nothing an agent can actually earn on today.
- **Other caveats:** (1) claiming requires staking **$AXOBOTL** (`0x810affc8…`, ~10% of bounty + 10,000 to register) — non-USDC onboarding friction; (2) verification is **human/requester approval** (subjective — not deterministic CI/oracle); (3) self-bootstrapped (built by its own "Axobotl" agent as first worker), $8,014 lifetime paid. Payment-provable but self-referential — talking to itself.
- **Negative signal:** none found — but also near-zero independent third-party testimonials (low footprint).
- **Non-duplication:** not currently indexed by swarm.tips or BountyHunte.rs.

## Pump.fun GO — FAIL

Live (~$370K paid, 320+ tasks) but: **custodial/discretionary** payout — "Pump.fun retains full authority to approve, reject, modify, or cancel… sole arbiter… final and not appealable" (documented case: ~40 SOL withheld over a typo in the prompt). **Explicitly rejects AI submissions**; paid work is physical/social stunts (tattoos, skydiving) an agent can't perform. "Agent-ready" wrappers are unofficial reverse-engineered endpoints. Only agent-doable slice (viral content) duplicates Shillbot.

## Immunefi — PASS payment / NO add

$110M+ paid, 3,000+ paid reports; named payouts (satya0x $10M Wormhole, pwning.eth $6M Aurora). But **KYC required for payout**, **human-judged severity** ("no fix, no pay", arbitration), novel human-level vuln discovery — not autonomously agent-claimable. Already served by purpose-built aggregators with APIs and an MCP server (BountyHunt.xyz, bbradar, BountyHunte.rs). swarm.tips adds only a redirect.

## Cantina — PASS payment / NO (human link only)

$16.1M paid to researchers, 5,000+ researchers; marquee pools (Uniswap v4 $2.35M, EF Pectra $2M). Competitive human-expert vuln hunts, subjective severity. No public REST API (would require scraping). Already in BountyHunte.rs (58 programs). No autonomous-agent earning evidence (EVMbench 72.2% is a *retrospective benchmark* on known bugs, not live competition).

## Sherlock — PASS payment / NO add

$16M top bounty (Usual); public contest archive + named Watson payouts (LZ $157,894 in 2025). But **$250 USDC stake-to-submit per report** (refunded only if valid) + payouts withheld until ≥2 valid lifetime issues AND ≥20% valid-issue ratio — actively hostile to a fresh/noisy autonomous agent (burns capital, gets frozen). No public API. Already in BountyHunte.rs.

---

## Recommendation

- **Add 0xWork** (`fetch_0xwork` → `list_earning_opportunities`, external `source_url`, no version bump). It passes the policy: payment-provable ($8K / 510 tasks, on-chain USDC to 15+ external wallets) and agent-claimable (crypto-native CLI/SDK, no KYC). **Self-posted (Axobotl) demand is not disqualifying** — a funded, completable, paying bounty is a real earning opportunity regardless of who posts it. Surface the AXOBOTL-stake caveat to agents. The listing is dynamic, so per-task quality (e.g. the current uncompletable "get @jessepollak to follow us" tasks) is the agent's call under the redirect model, same as any external source. Optional polish: filter the parser to completable task types. Residual caveat = thinness (board is often near-empty), which is low-value/low-harm, not a reason to exclude.
- **Decline** Pump.fun GO (fails payment provability), and **decline** Immunefi/Cantina/Sherlock as agent earning opportunities (pass payment, fail agent-claimability, already aggregated). Re-evaluate only if any ships an autonomous-agent earning path (deterministic verification + no KYC/stake gate).
- **Structural note:** every "pass payment / fail add" case here failed for the same reason — the work is human-expert and subjectively judged. This corroborates the verification-engine panel: the scarce, defensible thing is *agent-claimable, deterministically-verifiable* work, which almost none of the live crypto-money platforms offer.
