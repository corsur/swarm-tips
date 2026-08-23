> Historical snapshot (2026-07-07, from the file's own date) — not maintained.

# Server→Contract Coverage Spike — 2026-07-07

**Question (from the mapping-panel decision, `agent-discovery/mapping-decision.md`):** is the
server→contract "on-chain usage tiebreaker" worth building? Gate: run the free
coverage count before writing any code.

**Method:** queried the live discovery pipeline's deep-analysis output
(`GET https://mcp.swarm.tips/internal/mcp/earning-candidates` — every earning
candidate carries `layer3_analysis`, which regex-extracts Solana/EVM addresses
from each server's README; `models.rs::ExtractedAddress`).

**Result:**

| Metric | Count |
|---|---|
| Earning candidates (crypto-native subset of the catalog) | 19 |
| … with Layer-3 deep analysis | 19 (100%) |
| … with ≥1 extracted on-chain address | **0** |
| Total extracted addresses | **0** |

**Caveat:** deep analysis runs only on the top earning candidates, not the full
~2k catalog. But earning candidates are precisely the crypto-native servers
where a fronted contract would surface — if even they expose zero addresses in
their READMEs, whole-catalog confirmable coverage is effectively nil, far below
the single-digit-percent floor the panel estimated.

**Disposition: DO NOT BUILD the contract-usage tiebreaker.** The mapping
panel's ranked pipeline (crawl→LLM→controlling-authority-proof) stays on the
shelf; the panel's long-term direction stands — server trust should come from
**trusted-agent usage** (routed attribution) when that instrumentation exists,
and until then server ranking rests on the automated relevance + quality
signals shipped in `discovery::search` (BM25 + corroboration/stars/downloads).

Re-evaluate only if a future ingest source starts carrying declared contract
bindings (e.g. `/.well-known` manifests or ERC-8004 registrations appearing in
the catalog).
