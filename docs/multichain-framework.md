# Multichain (SVM + EVM) framework — decision doc

> Status: **DRAFT / spike in progress.** Per-layer leans are preliminary and gated on the end-to-end PoC (S6). No production changes until the framework is decided and the (separately-approved) mainnet push begins.
>
> Plan: `~/.claude/plans/woolly-puzzling-ullman.md`. Spike tasks: #42–48.

## Objective
Make all swarm products — **Coordination Game, Shillbot, reputation/credit** — multichain across **SVM (Solana) + EVM**, with a framework that spans **frontend + backend + smart contracts** so **end users choose what they want to do** (chain, wallet, path). Explicitly **a best-of-breed framework per layer, composed**, not one monolith — decided **before** the Solana-mainnet push so that push is the first instance of this architecture.

## Decision criteria (weighted for a single founder, ~$50–200K, info-work preference)
1. **Dev effort & maintainability** — minimal new surface for one builder.
2. **Reuse of existing stack** — Rust backend, Anchor programs, off-chain logic (matchmaking, verification via Google Workflows, MCP, EigenTrust).
3. **EVM + SVM coverage** — both first-class, not a bolt-on.
4. **Audit / ops surface** — fewer audited contracts, fewer moving services.
5. **Wallet / gas UX** — users pick a chain without friction.
6. **Lock-in / cost** — prefer self-hostable / open over SaaS where it matters.
7. **Time-to-first-EVM-presence** — how fast a real EVM path ships.

## Portability inventory (per product)
| Component | What's on-chain | Coupling | Notes |
|---|---|---|---|
| **Reputation/trust spine** | nothing — `services/eigentrust` (`PeerId = String`, lib.rs:74), `mcp-server/web_position.rs`, `composite_trust.rs` are pure off-chain | **none** | Already chain-agnostic. The unifier. Only the *edge reader* (`mcp-server/solana_reads.rs`) + shillbot `AgentState` Phase-1 counters are Solana-bound. |
| **Coordination Game** | commit-reveal state, tournament escrow, payoff | **low-med** | `payoff.rs` pure + portable; Keccak is cross-chain. PDAs → contract storage. Simplest settlement → good first dual-chain product. |
| **extension-registry / -credit** | vouch bonds, advances; emits events | **medium** | The durable artifact is the *events* (the web-position indexer consumes events, not account state) → ports cleanly. |
| **Shillbot** | task lifecycle, SOL escrow, **Switchboard oracle** in `verify_task`, Phase-1 counters | **high** | Oracle + escrow + counters. Switchboard → Chainlink/equiv on EVM. Heaviest port. |

---

## S1 — Frontend layer — **DECIDED: Reown AppKit** (2026-06-09)
**Candidates considered:** Reown AppKit, Dynamic, Privy (Stripe), Para, Openfort.
**Decision: Reown AppKit** — open + self-hostable (lowest lock-in; the founder owns the stack), native EVM+Solana(+BTC), SIWX multichain auth, multi-wallet + chain-switch. Chosen over Dynamic (more SaaS) given the info-work / self-host preference.
**Next (implementation):** standardize the three frontends (coordination-game, shillbot, swarm-tips) on AppKit; surface chain-choice to the user via AppKit's network selector; map the connected EVM/Solana address to the agent's chain-prefixed identity (S4).

## S2 — Smart-contract layer — **DECIDED: per-chain native + messaging** (2026-06-09)
**Decision: native contracts per chain** — Anchor (Rust) on Solana, **Solidity on EVM** — where each contract only manages **its own chain's** escrow/settlement. Cross-chain matches (S5) are coordinated by the **backend-as-middleman via operator liquidity + the `ChainAdapter`**, NOT by a trustless bridge/messaging layer in the hot path. A **cross-chain messaging layer (Wormhole) is therefore optional** and used only for reputation attestation (publishing a score/standing to a chain whose contract must gate on it without a backend call). Chosen over unified-VM (Stylus/Eclipse) to stay fully native + idiomatic on each chain. The PoC's EVM contract is therefore **Solidity**, not Stylus.

**Messaging-protocol sub-decision (lean — to confirm):** **Wormhole** — most battle-tested generic Solana↔EVM message passing (VAAs), broadest Solana support; fits "publish a trust-score / identity-link attestation cross-chain." Alternatives: **LayerZero v2** (if the OFT token standard / a canonical cross-chain token becomes central), **Hyperlane** (if permissionless self-deployed mailboxes / max sovereignty is preferred), **CCTP** (only for USDC value movement). _Confirm Wormhole vs LayerZero once the cross-chain payload (attestation vs token) is pinned in S4/S5._

**Per-program settlement mapping:** Coordination Game escrow + commit-reveal → Solidity equivalent (Keccak identical, PDAs → mappings); Shillbot escrow + `verify_task` oracle → Solidity + **Chainlink** (replaces Switchboard); extension-registry/credit (vouch bonds, advances) → Solidity, emitting the same events the off-chain web-position indexer already consumes.

## S3 — Backend layer — **DECIDED: in-house `ChainAdapter`, defer any service** (2026-06-09)

**Decision:** build the in-house `ChainAdapter` trait as the primary boundary; do **not** adopt a chain-abstraction/intent service yet — revisit only if cross-chain *value routing* becomes a real product feature (the S2 Wormhole layer covers narrow value/attestation needs in the meantime). Evaluation below.

**The question:** how do the Rust services (`shillbot-api`, `game-api`, `mcp-server`, `shillbot-verifier`) read/write/settle across Solana (Anchor) + EVM (Solidity)?

**Decisive insight — swarm's on-chain ops are bespoke app calls, not asset transfers.** The vast majority of swarm's on-chain interactions are *application-specific contract calls*: commit-reveal game moves, tournament escrow, shillbot task create/finalize + oracle `verify_task`, vouch-bond submit/withdraw, advance open/`route_and_recoup`. Chain-abstraction **services** and **intent/solver networks** (Socket, ERC-7683) are optimized for **value movement / bridging** ("get X tokens from chain A to B at best price"), *not* arbitrary per-app contract interactions. So a service would cover only the thin slice where *value* moves cross-chain — and the backend would **still** need a per-chain adapter for every bespoke call. A service does not replace the adapter; at best it complements it for value movement, which the chosen **messaging layer (S2)** already handles at the contract level.

### Scored comparison (1–5, higher = better for this project)
| Criterion | In-house `ChainAdapter` trait | Chain-abstraction service (Socket / intents) |
|---|---|---|
| Covers bespoke app calls (games, tasks, vouch, advance) | **5** — built for exactly these | 2 — services target transfers, not app calls |
| Reuse of existing Rust + `?network=` seam (`config.rs:36`) | **5** — generalize network→chain-family | 2 — new external integration model |
| Dev effort (upfront) | 3 — build Solana + EVM adapters (alloy) | 3 — integrate service, but still need per-app decode |
| Maintainability (single founder) | **4** — owned, deterministic, auditable | 2 — external API/uptime/economics to track |
| Audit / trust surface | **5** — no third-party in settlement path | 2 — adds a trust/centralization vector |
| Cross-chain VALUE routing | 3 — via S2 messaging layer | **5** — solver networks excel here |
| Fit with self-custody / permissionless ethos | **5** | 2 |
| **Weighted lean** | **Primary boundary** | Complement only, if/when value-routing is core |

### Recommendation
**In-house `ChainAdapter` trait** as the primary backend boundary: a Rust trait over `solana-sdk`/anchor (`SolanaAdapter`) + `alloy`/`ethers-rs` (`EvmAdapter`), selected per request by **chain-family**, generalizing the existing `?network=` seam (`shillbot-api/src/config.rs:36`). The business logic (matchmaking, verification via Google Workflows, reputation/EigenTrust) is already off-chain + chain-neutral; only the read / build-tx / submit / read-events calls need abstracting. **Defer/adopt a chain-abstraction service only if cross-chain *value routing* becomes a first-class product feature** — and even then it complements, not replaces, the adapter (and the S2 messaging layer may already suffice).

**Adapter surface (the trait methods):** `read_state<T>` (typed per product), `build_settlement_tx` (escrow / task lifecycle / game move / vouch / advance), `submit_tx`, `read_events` (feeds the reputation indexer + verification), `balance`/`identity`. Solana impl wraps current anchor calls; EVM impl wraps `alloy` ABI calls.

## S4 — Cross-cutting spine — **confirmed unifier + keying design** (2026-06-09)
The chain-agnostic reputation/identity layer ties all three products across both chains, with **no changes to the trust math** — only to how nodes are keyed and read.

**Keying — adopt CAIP-10 + a canonical agent ID.** Today the spine keys edges by raw Solana pubkey (`web_position.rs` `extender/recipient: String` → `TrustEdge{from,to}`; `eigentrust` `PeerId = String`). Generalize:
- Every address becomes a **CAIP-10** string: `solana:<genesis>:<pubkey>` and `eip155:<chainId>:<address>`. `PeerId = String` already accepts this — **zero change to the EigenTrust crate**.
- A **canonical agent ID** maps an agent's N chain addresses to one identity. Created by **proof-of-control**: the agent signs a link message with each address (Solana ed25519 + EVM secp256k1), the identity service records the verified link. EigenTrust is computed over **canonical IDs** (per-chain vouch edges resolve each endpoint address → its canonical ID), so an agent's EVM and Solana presence **share one reputation**.

**Multi-chain edge reader.** Generalize the Solana-only edge reader (`mcp-server/solana_reads.rs` `getProgramAccounts`) to read vouch/extension events from **both** chains (Anchor events on Solana, Solidity events on EVM) via the `ChainAdapter.read_events`, map endpoints → canonical IDs, feed the union to `compute_eigentrust` once.

**Exposure — API-first, attestation only where a contract must gate.** Trust is computed once and **exposed via the existing MCP/API path** (default — already built, chain-neutral; the gasless-standing gate can authorize off-chain by reading the spine). **Optional on-chain attestation** (publish a score/standing to a specific chain via the S2 **Wormhole** layer) only when a contract on that chain must gate on trust without a backend call. Start API-first; add attestation per-need.

**Net change is small:** CAIP-10 keying (string format), a canonical-ID link table + proof-of-control endpoint, and a two-chain edge reader. The trust computation (`eigentrust`, `composite_trust`, `web_position`) is reused as-is.

## S5 — Reference architecture + phased path (2026-06-09)

**Core principle — gameplay is backend-mediated; same-chain by default, cross-chain via backend-middleman.** The live game (matchmaking, commit-reveal, moves, chat) runs through the backend **off-chain**, so it stays **snappy regardless of which chains the players are on** — only **stake-in** (escrow) and **payout-out** touch chains, and each touches the player's **own** chain.
- **Default: same-chain matches** — both players on one chain; the pot settles on that chain; no operator liquidity needed. Simplest, zero counterparty risk.
- **Optional: cross-chain matches via backend-as-middleman** — Player A (e.g. Solana) and Player B (e.g. Base) each stake on their own chain; the backend pays the winner on the winner's chain from **operator liquidity** and reconciles its cross-chain exposure on its own balance sheet. **No trustless bridge in the hot path** — gameplay stays fast; cross-chain play is a backend liquidity/operational choice (the operator is the counterparty/house), not a heavy bridge integration. Trade-off: operator liquidity + counterparty risk per chain.

Independently of matches, the **reputation/identity spine always spans chains** — async, off-chain, zero-latency: an agent's Solana + Base activity aggregate into one canonical identity.

**The decided stack (per layer):**
- **Frontend:** Reown AppKit — user connects an EVM or Solana wallet + selects a chain; the connected address maps to the agent's canonical identity (S4).
- **Backend:** in-house `ChainAdapter` trait (`SolanaAdapter` over anchor, `EvmAdapter` over `alloy`), selected per-request by **chain-family** (generalizing `?network=`). Business logic (matchmaking, verification, reputation) stays off-chain + chain-neutral; only read/build-tx/submit/read-events is abstracted.
- **Contracts:** native per chain — Anchor (Solana) + Solidity (EVM) — with **Wormhole** for the narrow cross-chain message path (reputation attestation; any future value movement).
- **Spine:** CAIP-10 keying + canonical agent ID; EigenTrust computed once over canonical IDs; exposed via API, attested on-chain only where needed.

**End-to-end worked example — Coordination Game (simplest settlement):**
1. User opens the game, **picks a chain** in AppKit, connects that chain's wallet.
2. Frontend calls the backend with `chain_family` ∈ {svm, evm}.
3. Backend's matchmaker pairs players **within the chosen chain-family** (a player only matches others on the same chain — escrow + game state are single-chain, keeping it snappy); the `ChainAdapter` for that family builds the escrow/commit/reveal txns (anchor PDA on SVM, Solidity mapping on EVM — Keccak commit identical).
4. Players sign in their wallet; `ChainAdapter.submit_tx` lands settlement on the chosen chain.
5. On resolution, the contract emits the result event; the **multi-chain edge/event reader** ingests it → the spine updates that agent's canonical reputation **once**, readable from either chain.

**Phased adoption (validated order):**
1. **Reputation/identity spine** — CAIP-10 keying + canonical-ID link table + proof-of-control endpoint + two-chain edge reader. Highest leverage, lowest risk (mostly off-chain, already chain-agnostic), unifies everything.
2. **Frontend multichain SDK** — standardize the three frontends on Reown AppKit + chain-select.
3. **First dual-chain product — Coordination Game** — port escrow + commit-reveal to Solidity; wire the `EvmAdapter`. Simplest settlement, no oracle.
4. **Shillbot + credit** — Solidity port of task/escrow (Switchboard → Chainlink), vouch bonds, advances; heaviest, last.

## S6 — De-risking PoC (building on Base Sepolia)
Proves the actually-novel/risky thing: **chain-local EVM settlement → cross-chain reputation aggregation under ONE canonical identity** (not cross-chain gameplay — that's explicitly out per the chain-local principle). Thin path: an agent settles a chain-local action on a **minimal Solidity contract on Base Sepolia** (via the backend `EvmAdapter`/`alloy`) → the contract emits a result event → the **reputation spine ingests it** and updates that agent's canonical (**CAIP-10** `eip155:84532:<addr>`) reputation, which **also** carries the agent's Solana (`solana:...`) activity → demonstrating one identity spans both chains with **zero change to the EigenTrust math**. The AppKit wallet step is the low-risk part (mature SDK); the PoC focuses on the EVM `ChainAdapter` + spine-ingest composition. Validated locally on anvil first (no funding), then deployed to Base Sepolia.

## Mainnet-readiness delta — how this reshapes the deferred Solana-mainnet push (#34/#35/#41)
Build the mainnet push as the **first instance** of this architecture, not Solana-only:
- **Identity keying:** deploy with **CAIP-10 / canonical-ID** identity from the start (the reputation spine + onboarding key agents by canonical ID, not raw Solana pubkey) — so EVM addresses slot in later without a reputation migration.
- **Backend seam:** land the mainnet services behind the `ChainAdapter` boundary (even if only `SolanaAdapter` exists at first) so adding `EvmAdapter` is additive, not a refactor.
- **Settlement interface:** define the settlement calls (escrow, task lifecycle, vouch, advance) against the adapter trait, so the Solidity port in phases 3–4 implements a known interface.
- **Unchanged:** the actual mainnet deploy/migrate/seed steps (extension-registry + extension-credit + shillbot C2, `migrate_task`, Firestore per-wallet advance cap, lift `routes/agent.rs:49` gate) are the same — they just run against canonical identities + the adapter seam.
