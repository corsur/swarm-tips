# Secondary MCP directory submissions — checklist

**Status:** DRAFT — execute when MCP server is stable.

The official MCP registry (`registry.modelcontextprotocol.io`) is the canonical source. Most secondary directories auto-pull from it within 24h, but submitting via their public form removes the timing dependency and gets us in the queue immediately.

For each directory below: zero email outreach needed unless explicitly noted. Just use the form / PR.

---

## 1. mcp.so

- **URL:** `https://mcp.so/submit` (or PR against their listing repo on GitHub)
- **Method:** Public web form. Fields: name, description, GitHub repo, install command, tags.
- **What we need:** registry name (`io.github.corsur/swarm-tips`), the new "28 tools…non-custodial" description, GitHub link, install command.
- **Editorial:** none. Auto-listed.

## 2. Glama (glama.ai)

- **URL:** `https://glama.ai/mcp/servers` — they auto-pull from the official MCP registry. No manual submission strictly needed.
- **Method:** Just confirm we appear within 48h after re-publishing v0.1.2. If we don't, look for their "Submit a server" page or contact via their site footer.
- **What we need:** to be on `registry.modelcontextprotocol.io` with current metadata.

## 3. Smithery (smithery.ai) — VERIFY BEFORE LAUNCH

- **Status:** submission path UNCLEAR. As of 2026-04-07 verification, Smithery's public docs surface a Connect/use API and a `POST /tokens` endpoint but **no documented `/servers` listing API**. Their CLI (`smithery mcp list`) lists *user* connections, not the global catalog. Their site references "API documentation" but the public docs don't show a self-serve submission flow.
- **Hypothesis:** like PulseMCP, Smithery's listing path is partnership-gated. The public site directs to "contact us at [email protected] to discuss how we can tailor our MCP server registry to your specific needs."
- **Action before launch:** spend 15 minutes reading [smithery-ai/cli](https://github.com/smithery-ai/cli) source to confirm whether there's a public submission endpoint we're missing. If yes, use it. If no, treat as PulseMCP-shaped: send an email asking for partnership access, separate from the launch-day execution list.
- **If submission path exists:** YAML/JSON entry with name, description, transport, install command. Mirror what's in our `server.json`.
- **Disposition for launch day:** **REMOVE FROM CRITICAL PATH.** Push to follow-up. Don't block the launch on a directory whose submission flow we don't understand.

## 4. ClawHub

- **URL:** their submission interface (link in `swarm-tips-repo/SKILL.md` references).
- **Method:** Submit `swarm-tips-repo/SKILL.md` via their interface. SKILL.md is already at the repo root and is current (28 tools, four verticals, install command).
- **What we need:** SKILL.md is ready as-is. Just push it through the form.
- **Reach:** ~2.87M Clawbot agents per the swarm-tips spec. Highest-leverage of the four.

## 5. Official MCP registry — already done (workstream 1, gated)

Re-publish v0.1.2 via `mcp-publisher publish` from `services/mcp-server/`. This is the prerequisite for everything above.

---

## Order of operations on launch day

1. `mcp-publisher publish` v0.1.2 → official registry.
2. Wait 1h, curl the registry to confirm v0.1.2 + new description are live.
3. Submit `mcp.so` form.
4. Push `SKILL.md` to ClawHub.
5. Wait 24h, confirm we appear on Glama (auto-pull). If not, manually submit.
6. Send PulseMCP email (`api@pulsemcp.com`, CC `hello@pulsemcp.com`) and submit their `/submit` form.
7. Bountycaster cast.
8. Operator DM batch.

**Smithery is NOT on the critical path** — submission flow is unverified. Push to follow-up after launch. See § 3 above.

Do not collapse steps 1–2. The registry pull cache on the secondary directories is a one-shot snapshot — if we submit before v0.1.2 is live, we ship the stale "22 tools" description to all of them.
