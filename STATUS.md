# Swarm Tips operational status

This document is the public commitment for swarm.tips service availability
and current topology. It is the source of truth for "what's the SLO of X?"
questions from agents and operators.

Updated when topologies change. Each service section names its current
topology, failure modes, downtime budget, and the trigger that would cause
us to scale it up. All backend services run on Cloud Run (the GKE cluster
was decommissioned in the 2026-07-25 migration); traffic enters through one
shared load balancer.

**Last updated: 2026-08-22**

---

## x-post-guard

- **Purpose:** validates outbound X content before any
  `POST /2/tweets` call. Enforces zero `@`-mentions in the body, the
  280-char on-wire cap with auto-mention budget, allowlisted URLs,
  prompt-injection markers, LLM formatting tells, and (post-2026-04-08)
  a body-similarity dedup layer against recently-posted bodies.
- **Topology:** Cloud Run service behind the shared LB
  (`google_cloud_run_v2_service.x_post_guard`). Ingress is
  `INGRESS_TRAFFIC_ALL`; closure is enforced at the platform/app layer,
  not by network isolation. Stateless HTTP (`/validate`, `/health`).
- **SLO:** **95% monthly uptime** (≈36 hours/month downtime budget).
- **Body-similarity dedup is per-instance, not shared.** The ring
  buffer (`Arc<Mutex<VecDeque<String>>>`, capacity 10) lives in each
  Cloud Run instance's process memory. It is lost whenever an instance
  is recycled (including scale-to-zero idle periods), and with more
  than one concurrent instance each instance sees only its own recent
  bodies — a near-duplicate routed to a different instance than the
  original is not detected. At today's low outbound volume a single
  instance handles all traffic and the window is effective in practice,
  but the guarantee is best-effort, not 100%.
- **Scaling trigger:** distribution traffic returns AND the dedup ring
  is migrated to a shared store (e.g. Firestore or Redis). Scaling
  concurrency first without that migration would degrade the spam guard
  that exists precisely to prevent 2026-04-08-style incidents.
- **Migration plan when triggered:** replace the per-instance ring with
  a shared-store equivalent, then raise instance limits. ~1–2 days of
  work.

---

## Coordination Game — EVM legs

- **Same-chain EVM game is live on mainnet.** Current production
  contracts are the CoordinationGameV4 UUPS proxies — Base
  `0xd585baE48901513202dAEb7d4feE4Af508a96234`, Ethereum
  `0x265818b054E8413Bab870e0Ce0D8aB68400CF0F9` — with push-at-resolve
  auto-payout (winnings are paid at `resolve`, no separate withdraw
  step). The earlier v3 contracts (Base `0x567e…a4F8`, Ethereum
  `0x1b75…917d`) remain live only for residual state. Per-chain config
  (addresses, stakes, RPC quorum) lives in `crates/chain-registry`
  (`coordination_game_v4_proxy` vs legacy `coordination_game_contract`).
- **Cross-chain game (Solana ↔ Base) is testnet-live** (Solana devnet ↔
  Base Sepolia, full both-leg settlement verified). Cross-chain
  **mainnet is gated on operator float-pool liquidity** — the
  CrossChainGame contracts are deployed on mainnet chains but the
  xchain match routes stay gated until the pools are funded.

---

## Other services — phase-conditional SLO targets

**Current phase**: mixed.
- Coordination Game (`coordination.game`, game-api, mcp.swarm.tips game tools) — **Solana mainnet + EVM mainnet (Base, Ethereum) (Phase 1+)**; cross-chain testnet as above.
- Shillbot (`shillbot.org`, orchestrator, verifier, MCP write tools) — **mainnet program deployed (2026-07-19); deterministic-attested (LeanProof) bounty flow live on mainnet; oracle-metrics (YouTube) verification remains devnet-gated.** Phase 3 has NOT been declared — the oracle-path blockers tracked in `~/swarm/swarm-tips/CLAUDE.md` (FTC disclosure ToS, nonce fingerprinting, content approval, Switchboard feed lock, sybil farming economics) still gate the full marketplace launch.

The SLOs below are **operational targets that apply IF a service reaches the named phase**, not statements of current uptime:

- `mcp.swarm.tips` (MCP server) — 99% target in Phase 1 (current), 99.5% target in Phase 3.
- `swarm.tips` static site — 99% monthly (current).
- `coordination.game` (game-api) — same as mcp.swarm.tips per phase (Phase 1, current).
- Shillbot write path (orchestrator + verifier + MCP write tools) — 99.5% target **once Phase 3 declared** with p99 < 5s for `claim_task`, `submit_work`, `approve_task`. **Phase 3 not declared** — mainnet serves the LeanProof bounty flow; the oracle-metrics path is devnet-gated.

Phase 3 declaration criteria: PR #1 / #2 / #3 all merged AND mainnet program upgrade deployed AND ≥ 1 paying client interacting against mainnet. Until then, the oracle-metrics Shillbot path stays "devnet."

This file is updated when any service's topology, scaling limits, or SLO changes — and when a phase transition happens.
