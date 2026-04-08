# Crawler verification DNS records — manual Namecheap entries

**Status:** PREPARED — awaiting user approval to add in Namecheap dashboard.

DNS for `swarm.tips`, `coordination.game`, `shillbot.org` lives at Namecheap (the registrar), not in Terraform. To verify these properties with search engines, the following TXT records need to be added manually in the Namecheap "Advanced DNS" tab for each domain.

## Why this isn't automated

Per `coordination-app/infra/CLAUDE.md`: "DNS is managed externally (not Cloud DNS). Terraform outputs static external IPs; A records on the domain registrar point to them." There's no `dns.tf`, no DNS provider configured, and no programmatic write path. These get added by hand.

---

## Records to add (per domain)

For **each** of the three domains, add three TXT records at the apex (`@` host).

### swarm.tips

| Type | Host | Value | TTL |
|------|------|-------|-----|
| TXT | `@` | `google-site-verification=<TOKEN_FROM_GSC>` | Automatic |
| TXT | `@` | `MS=ms<DIGITS_FROM_BING>` _(or BingSiteAuth.xml file upload — pick whichever Bing offers)_ | Automatic |
| TXT | `@` | `brave-site-verification=<TOKEN_FROM_BRAVE>` | Automatic |

### coordination.game

| Type | Host | Value | TTL |
|------|------|-------|-----|
| TXT | `@` | `google-site-verification=<TOKEN_FROM_GSC>` | Automatic |
| TXT | `@` | `MS=ms<DIGITS_FROM_BING>` | Automatic |
| TXT | `@` | `brave-site-verification=<TOKEN_FROM_BRAVE>` | Automatic |

### shillbot.org

| Type | Host | Value | TTL |
|------|------|-------|-----|
| TXT | `@` | `google-site-verification=<TOKEN_FROM_GSC>` | Automatic |
| TXT | `@` | `MS=ms<DIGITS_FROM_BING>` | Automatic |
| TXT | `@` | `brave-site-verification=<TOKEN_FROM_BRAVE>` | Automatic |

## How to get the tokens

Each token is issued by the search engine when you add the property to its webmaster tools. The flow:

1. **Google Search Console** — `https://search.google.com/search-console`
   - Add property → `swarm.tips` (URL prefix → `https://swarm.tips/`)
   - Choose "HTML tag" or "Domain (DNS TXT)" verification — DNS TXT is preferred
   - Google shows the exact `google-site-verification=…` string to paste
   - Repeat for `coordination.game` and `shillbot.org`

2. **Bing Webmaster Tools** — `https://www.bing.com/webmasters`
   - Add site → enter URL
   - Choose "Add a CNAME or TXT record to DNS" — Bing shows the exact `MS=…` string
   - Repeat for the other two domains

3. **Brave Search Webmaster Tools** — `https://search.brave.com/help/webmaster-tools`
   - Add site → DNS TXT verification
   - Brave shows the exact `brave-site-verification=…` string
   - Repeat for the other two domains

## After the records propagate (5–60 min)

In each webmaster tool's UI, click "Verify". Once verified:
- **Google Search Console:** submit `https://swarm.tips/sitemap.xml`, request indexing on the homepage
- **Bing Webmaster Tools:** submit the sitemap, enable IndexNow integration (it'll auto-detect the `{key}.txt` file we already deployed at the site root)
- **Brave Search Webmaster Tools:** submit the sitemap

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
