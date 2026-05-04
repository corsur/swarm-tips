# Tool description token-cap audit (2026-05-04)

**Roadmap task #32.** Measure each MCP tool's `description` length, flag any
exceeding the 200-token soft cap, propose tightening.

## Why a cap

Every `description` field is shipped to the agent's LLM context inside the MCP
`initialize` response (`tools/list`). 31 tools × an average of ~110 tokens
already costs ~3.4K tokens before the agent has done a single thing. A 200-token
soft cap keeps each individual description from dominating, encourages
description authors to point at the source-of-truth (the spec, the on-chain
account, the linked tool) rather than re-stating it, and leaves headroom for
when the tool surface grows past 31.

The cap is *soft*: structural complexity sometimes legitimately requires a
longer description (e.g., `shillbot_get_attestation` documents a wire format
that callers must understand to consume the response). Tools at 150-200 tokens
are fine. The audit's job is to flag tools that crossed into "the description
is doing the work the spec should be doing" territory.

## Token estimate methodology

Token counts are estimated as `chars / 4` (the GPT-style heuristic). For real
descriptions in this codebase (English prose with technical identifiers and
dashes), this estimate runs ~5-10% above an actual `tiktoken cl100k_base` count.
That's the conservative direction — if we're under cap on this estimator, we're
under cap on the real tokenizer too.

Counted: the `description = "..."` string only, after Rust escape unescaping.
Not counted: tool name, parameter schema, `annotations(read_only_hint = ...)`,
or the `INSTRUCTIONS` block.

## Findings

**31 tools audited.** Pre-revision:

| Tokens | Chars | Tool | Status |
|---|---|---|---|
| **235** | 942 | `agent_trust_score` | **OVER** |
| **227** | 910 | `agent_profile` | **OVER** |
| 190 | 762 | `list_earning_opportunities` | borderline |
| 178 | 712 | `shillbot_get_attestation` | high but justified |
| 176 | 706 | `search_mcp_servers` | high but justified |
| 174 | 697 | `shillbot_complete_task` | high but justified |
| 163 | 653 | `shillbot_approve_task` | mid |
| 158 | 635 | `list_spending_opportunities` | mid |
| 158 | 634 | (post-revision target) | n/a |
| 156 | 625 | `shillbot_reject_task` | mid |
| 149 | 599 | `generate_video` | mid |
| 142 | 571 | `register_wallet` | mid |
| (16 others) | | | under 130 |

**Two tools over the 200-token cap, both new this sprint:**

1. `agent_profile` (#29) — 227 tokens, 910 chars. The over-budget content was
   the verbose enumeration of derived-metric formulas (`average_score =
   total_score_sum / total_completed`, etc.) and the rhetorical "NO orchestrator
   hop, NO Firestore cache". Both are recoverable from elsewhere — the
   formulas are documented in `services/mcp-server/src/server.rs:990-1014` and
   the trustlessness claim is implied by the "directly from Solana via
   getAccountInfo" phrase.

2. `agent_trust_score` (#31) — 235 tokens, 942 chars. The over-budget content
   was the parenthetical re-explanation of each input signal ("oracle-attested
   completion + score", "win rate ≥ 5 games"), the EigenTrust forward-reference
   paragraph, and the use-case framing. The signal names are
   self-documenting; the EigenTrust note can be one sentence; the use-case
   framing belongs in the spec, not the tool description.

## Tightening applied

`services/mcp-server/src/server.rs` revised in this commit. Both tools now
under the cap:

| Tool | Before | After | Δ |
|---|---|---|---|
| `agent_profile` | 227 tokens | 135 tokens | −92 |
| `agent_trust_score` | 235 tokens | 158 tokens | −77 |

What was kept:
- Cash-flow tag (`[READ]`).
- Trustlessness framing for `agent_profile` ("Trustless on-chain reputation
  lookup ... directly from Solana via getAccountInfo — no orchestrator hop,
  no cache").
- Both PDA names + their fields (`AgentState`, `PlayerProfile`).
- The partial-data + `null` semantics.
- The `tournament_id` default.
- Composite signal list for `agent_trust_score`.
- Confidence semantics (`0..=4`).
- `breakdown` field semantics.
- EigenTrust forward reference (one sentence).

What was dropped:
- Per-metric formulas (`average_score = total_score_sum / total_completed`, etc.)
  — recoverable from the response shape and the source (see `server.rs:993-1004`).
- The "NO orchestrator hop, NO Firestore cache — the orchestrator could lie,
  but this tool reads the source of truth" rhetorical aside — the trustless
  claim is preserved without it.
- Per-signal explanatory parentheticals in `agent_trust_score`
  ("oracle-attested completion + score", "Layer 3 curator tier ascription").
- Use-case framing ("Use this when you need a single number to decide whether
  to trust an agent for a task / hire / payout") — belongs in the spec.

## Borderline tools (worth watching, no change applied)

- `list_earning_opportunities` (190 tokens): documents the source list +
  `claim_via` vs `source_url` distinction + universal-entry-point framing. The
  `claim_via` semantics are load-bearing for callers — agents that call
  `list_earning_opportunities` need to know which entries route through MCP
  vs an external URL. If a future revision adds another source, prefer
  shortening to a list of source names with a one-line semantic tag, rather
  than continuing to list each source's framing in prose.
- `shillbot_get_attestation` (178 tokens): documents the AAS v0 wire format.
  Justifiable — callers consume this output and need to know its shape.
  When AAS v1 ships and the wire format is documented in `docs/specs/aas-v1.md`,
  the description can shrink to a one-line pointer at the spec.
- `search_mcp_servers` (176 tokens): documents three vetting tiers + the
  `discover_opportunities` cross-reference. Each line is load-bearing.
- `shillbot_complete_task` (174 tokens): documents the next-action dispatcher
  semantics + the wait-action subtype. Each line is load-bearing.

## Process recommendation

Add a CI check that runs the same regex extraction this audit uses and fails
the build if any tool description exceeds 200 tokens. Suggested location:
`services/mcp-server/build.rs` or a dedicated `cargo test` in
`services/mcp-server/tests/`. The test would parse `src/server.rs`, count
tokens per `description`, and assert all are ≤ 200. Cost: ~30 lines of Rust.
Benefit: regression-proof against future descriptions creeping back over the
cap. Not implemented in this commit (out of scope for the audit task) — added
to the follow-up queue.

Cap value (200 tokens) is a starting point, not load-bearing. If future tools
genuinely require more, raise the cap deliberately rather than waiving it
case-by-case.

## Verification

`cargo build && cargo test && cargo clippy` green on `services/mcp-server`
post-revision. No tests reference description text directly — descriptions are
LLM-facing prose, not API contract. Live tools/list response unchanged in
shape.
