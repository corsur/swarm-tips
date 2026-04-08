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

## 3. Smithery (smithery.ai)

- **URL:** `https://smithery.ai/` — GitHub-based registry.
- **Method:** Open a PR adding our entry to their listing repo (`smithery-ai/registry` or similar — confirm the exact repo when submitting).
- **What we need:** YAML/JSON entry with name, description, transport, install command. Mirror what's in our `server.json`.

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
4. Submit Smithery PR.
5. Push `SKILL.md` to ClawHub.
6. Wait 24h, confirm we appear on Glama (auto-pull). If not, manually submit.
7. Send PulseMCP email (`hello@pulsemcp.com`) and submit their `/submit` form.
8. Bountycaster cast.
9. Operator DM batch.

Do not collapse steps 1–2. The registry pull cache on the secondary directories is a one-shot snapshot — if we submit before v0.1.2 is live, we ship the stale "22 tools" description to all of them.
