# Known limitations

Tracked tech debt and intentional accepts. Each entry has a clear
resolution path queued; if any becomes blocking enough to act on,
move it to an active task.

---

## Frontend HTTP-status semantics

### KL-001 — Dynamic SPA routes return HTTP 404 (with the SPA body)

**Where:**
- `shillbot.org/generate/:id`, `/client/campaigns/:id`, `/shorts/:id`
- `coordination.game/game/:gameId`

**Symptom:** GET request returns HTTP 404. Body is the SPA bundle
(via the bucket's `not_found_page = "index.html"`), so the React
Router resolves the path client-side and the right page renders.
Browsers display the page normally.

**Why:** Cloud Storage's `notFoundPage` website-config option always
returns HTTP 404 status; the status code can't be overridden.
Cloud Load Balancer's URL Map could rewrite unmatched paths to
`/index.html` (returning 200), but the `pathTemplateRewrite` field
that does full-path rewrite is not exposed in Terraform's stable
`hashicorp/google` provider as of v5.45.2. `pathPrefixRewrite`
only replaces the matched prefix, leaving the suffix
(`/anything` → `/index.html/anything`).

**Downstream impact:**
- **Search engines** correctly skip indexing per-id pages — fine.
- **Link unfurlers** (Slack, Discord, Twitter) may skip generating
  OG previews when the status is 404. Real share-UX cost.
- **Uptime monitors** targeting deep-linked dynamic routes will
  false-positive as outages. Operators monitoring those URLs need
  to either monitor a static route instead or filter for body
  content vs. status code.
- **Curl / HTTP-tooling-based health checks** see 404. Same as
  above.

**Status code semantics aside, the app works correctly** — body is
right, React Router resolves, the user sees the right page.

**Resolution path:** when Terraform's google provider exposes
`pathTemplateRewrite` (or once we move to the `google-beta`
provider for an experiment), add a Cloud LB URL Map rule that
rewrites unmatched SPA paths to `/index.html` so GCS serves 200.
Until then, document and accept.

### KL-002 — Astro pretty URLs return HTTP 301→200 chain

**Where:**
- `swarm.tips/discover`, `/dashboard`, `/developers`,
  `/coordination-game`, `/start`

**Symptom:** First request to `/discover` returns 301 with
`Location: /discover/index.html`. Browsers follow to the explicit
URL and get 200.

**Why:** Astro `build.format: "directory"` emits
`<route>/index.html` rather than `<route>.html`. GCS sees `/discover`
as a directory prefix, applies `MainPageSuffix = "index.html"`, and
301-redirects to the explicit URL. This is intentional pretty-URL
behavior across the web (most static-site hosts do it).

**Downstream impact:**
- **First-visit latency penalty:** ~50–100ms extra round-trip on
  the first request to each pretty URL. Browsers cache the 301 for
  subsequent visits.
- **Search engines** handle 301s correctly.

**Resolution path:** if data shows real impact, switch Astro to
`build.format: "file"` (URLs become `/discover.html`) or change
GCS hosting to a setup that doesn't 301 on directory-style URLs.
For now, accept.

---

## Format

Each entry: KL-N, where, symptom, why, downstream impact,
resolution path. Update the entry's status if remediation
constraints change (e.g., Terraform provider gains the field that
unblocks KL-001).
