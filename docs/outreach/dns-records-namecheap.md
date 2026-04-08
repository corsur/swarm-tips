# Crawler verification DNS records — manual Namecheap entries

**Status:** PREPARED — awaiting user approval to add in Namecheap dashboard.

**Correction (2026-04-08):** Brave Search has **no webmaster tool** and no DNS verification flow. The original version of this doc listed `brave-site-verification=…` records, but verified at probe time that no such tool exists. Brave Search builds its index via the [Web Discovery Project](https://brave.com/web-discovery-project/) — anonymous sampling of opted-in Brave users' browsing — not via crawler submission. The way to get on Brave is to get real users to visit on the Brave browser, which is downstream of the launch outreach in this playbook (Bountycaster, X, Farcaster). No Namecheap entry needed for Brave; just don't worry about it.

DNS for `swarm.tips`, `coordination.game`, `shillbot.org` lives at Namecheap (the registrar), not in Terraform. To verify these properties with **Google Search Console** and **Bing Webmaster Tools**, the following TXT records need to be added manually in the Namecheap "Advanced DNS" tab for each domain.

## Why this isn't automated

Per `coordination-app/infra/CLAUDE.md`: "DNS is managed externally (not Cloud DNS). Terraform outputs static external IPs; A records on the domain registrar point to them." There's no `dns.tf`, no DNS provider configured, and no programmatic write path. These get added by hand.

---

## Records to add (per domain)

For **each** of the three domains, add two TXT records at the apex (`@` host) — one for Google, one for Bing.

### swarm.tips

| Type | Host | Value | TTL |
|------|------|-------|-----|
| TXT | `@` | `google-site-verification=<TOKEN_FROM_GSC>` | Automatic |
| TXT | `@` | `MS=ms<DIGITS_FROM_BING>` _(or BingSiteAuth.xml file upload — pick whichever Bing offers)_ | Automatic |

### coordination.game

| Type | Host | Value | TTL |
|------|------|-------|-----|
| TXT | `@` | `google-site-verification=<TOKEN_FROM_GSC>` | Automatic |
| TXT | `@` | `MS=ms<DIGITS_FROM_BING>` | Automatic |

### shillbot.org

| Type | Host | Value | TTL |
|------|------|-------|-----|
| TXT | `@` | `google-site-verification=<TOKEN_FROM_GSC>` | Automatic |
| TXT | `@` | `MS=ms<DIGITS_FROM_BING>` | Automatic |

## How to get the tokens

Each token is issued by the search engine when you add the property to its webmaster tools. The flow:

1. **Google Search Console** — `https://search.google.com/search-console`
   - Add property → `swarm.tips` (URL prefix → `https://swarm.tips/`)
   - Choose "HTML tag" or "Domain (DNS TXT)" verification — DNS TXT is preferred
   - Google shows the exact `google-site-verification=…` string to paste
   - Repeat for `coordination.game` and `shillbot.org`

2. **Bing Webmaster Tools** — `https://www.bing.com/webmasters`
   - **Strong recommendation:** use the **"Import sites from Google Search Console"** button on the Bing dashboard. One click brings over all three properties + their sitemaps after GSC verification completes. Skips the per-site TXT-record dance.
   - If you don't import: Add site → enter URL → choose "Add a CNAME or TXT record to DNS" → Bing shows the exact `MS=…` string → paste into Namecheap. Repeat for the other two.
   - **Why Bing matters for swarm.tips specifically:** ChatGPT's web search uses Bing, Microsoft Copilot uses Bing, DuckDuckGo+Yahoo+Ecosia all use Bing as their backend. Verifying Bing reaches a much larger AI-agent surface than the raw Bing market share suggests.

3. **Brave Search** — no webmaster tool exists. **Skip this step entirely.**
   - Brave's index is built from the Web Discovery Project (anonymous sampling of opted-in Brave users' actual browsing), not from crawler submission.
   - The way to get on Brave is to get real users to visit your site on the Brave browser. That happens downstream of the Bountycaster + X + Farcaster outreach in the rest of this playbook.
   - No DNS record, no submission form, nothing to do here today.

## After the records propagate (5–60 min)

In each webmaster tool's UI, click "Verify". Once verified:
- **Google Search Console:** submit `https://swarm.tips/sitemap.xml`, request indexing on the homepage
- **Bing Webmaster Tools:** submit the sitemap (auto-imported if you used the GSC import button), then **explicitly enable IndexNow integration** if it isn't auto-on. This is the multiplier — every future content change pings Bing + Yandex + Seznam + Naver simultaneously via the `{key}.txt` files already deployed at site root.
- **Brave Search:** nothing to do. Indexing happens passively as real Brave users visit (downstream of the launch outreach).

## IndexNow keys (already deployed in `public/`)

| Property | Key file path |
|----------|---------------|
| swarm.tips | `coordination-app/frontend/swarm-tips/public/f903ad668dc1934ef7e072f8f3d742b9f94c795f24a5d751a936e7d203cf2007.txt` |
| coordination.game | `coordination-app/frontend/coordination-game/public/ed55cc8e8142a7de516e7d2cf4a37b6f4a1a336b77d8fb06670a47f5a9fd5c1d.txt` |
| shillbot.org | `coordination-app/frontend/shillbot/public/6796b5c9a03cca843c737afe7985fa13f19fc7e8d73573da7b6e6c1eea348b8e.txt` |

These ship to GCS on next deploy. After they're live, a single POST to `https://api.indexnow.org/indexnow` notifies Bing + Yandex + Seznam + Naver of new URLs simultaneously.

Test command (after deploy):
```sh
curl https://swarm.tips/f903ad668dc1934ef7e072f8f3d742b9f94c795f24a5d751a936e7d203cf2007.txt
# expect: f903ad668dc1934ef7e072f8f3d742b9f94c795f24a5d751a936e7d203cf2007
```
