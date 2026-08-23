> Historical snapshot (2026-06-30, from the file's own date) — not maintained.

# Listings Source Health Diagnosis — 2026-06-30

**Trigger:** `GET https://mcp.swarm.tips/internal/listings` returns 0 entries (public bounty board + `list_earning_opportunities` empty).

**Root cause:** Not a dry market — the two sources that *have* inventory are both failing at the network layer, so every ingestion cycle ends `returned=0`. The other sources are genuinely empty or filtered.

## Per-source verdict

| Source | Status | Evidence |
|---|---|---|
| **shillbot** | **BROKEN (fixable, high value)** | `fetch_shillbot` hits hardcoded `https://api.shillbot.org/tasks?limit=200` (`sources.rs:321`) whose **TLS cert is expired** (`curl`/WebFetch: "certificate has expired"). Prod log every cycle: `fetch failed source=shillbot error="error sending request for url (https://api.shillbot.org/tasks?limit=200)"`. Its 15 cached listings then get swept by the 24h staleness rule. **The data exists** — the MCP tool `shillbot_list_available_tasks` returns 9+ open mainnet tasks via the *orchestrator* path (`ORCHESTRATOR_URL`, default `http://shillbot-api:8080`), which works. The pipeline just points at the wrong (public, cert-expired) host. |
| **moltlaunch** | **BROKEN (likely dead)** | Prod log every cycle: `scraper subprocess failed: ... dns error: failed to lookup address information: No address associated with hostname (api.moltlaunch.com)`. The API host no longer resolves; `moltlaunch.com` returns 403. 20 cached listings swept. |
| **botbounty** | **DRY** (healthy, 0) | Live: `{"count":0,"bounties":[]}`. |
| **bountycaster** | **DRY** (healthy, 0) | Live: `{"bounties":[]}`. |
| **defillama-ai** | **Healthy but contributes 0** | `api.llama.fi/protocols` up; ~33 AI-agent protocols fetched each cycle (`fetched from external sources total_fetched=33`) but all filtered by `apply_filters` (no reward → below the $1 floor). Never reaches the EARN board — discovery category, by design. |

Representative cycle (verbatim): `fetch failed source=shillbot ...` → `fetch failed source=moltlaunch ...dns error...` → `fetched from external sources total_fetched=33` → `dropped stale listings ... by_source={"shillbot":15,"moltlaunch":20}` → `listings ingestion complete returned=0`.

## Recommended fixes (separate, gated follow-ups)

1. **shillbot (do first — restores first-party listings):** repoint `fetch_shillbot` from the public cert-expired `https://api.shillbot.org/tasks` to the **in-cluster orchestrator** the MCP tool already uses (`ORCHESTRATOR_URL`, default `http://shillbot-api:8080`) → `{orchestrator}/tasks?limit=200`. Plumb `orchestrator_url` into the listings fetch (or read the same env var). Removes the public-TLS dependency entirely (no recurrence when the cert lapses) and unifies the two shillbot data paths. Code-only, in our control, no cloud change. Alternative/stopgap: renew the `api.shillbot.org` cert (infra; needs explicit go-ahead per the read-only-cloud rule, and the cert will lapse again).
2. **moltlaunch (disable):** it's been failing every cycle for days (API host dead). Remove/disable `fetch_moltlaunch` from the `get_listings` fan-out so it stops the 6h-backoff churn and stale noise; re-add behind a working URL if the platform returns. (It was a PASS in April with 172 tasks — likely shut down or moved.)
3. **defillama-ai (optional):** it fetches 33 protocols every cycle only to filter all of them. Consider dropping it from the EARN fan-out (it's discovery, not paid bounties) to cut noise — or leave as-is (harmless, just wasted work).

## Notes

- The 24h `STALENESS_THRESHOLD_SECS` + 3-fail/6h backoff are working as designed; they correctly swept the dead sources. The bug is purely that `fetch_shillbot` targets the wrong host.
- After the shillbot fix, the board should show the 9+ open first-party Shillbot tasks again within ~5 min (cache TTL).
- This is independent of the 0xWork question (that source is ready-but-unpushed; it would add its own claimable tasks once live, but is not related to the empty-board cause).
