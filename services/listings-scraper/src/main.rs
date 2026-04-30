//! Tiny TLS-impersonating fetch helper. Invoked as a subprocess by
//! `mcp-server`'s listings pipeline for upstreams that fingerprint-block
//! plain reqwest (currently `api.moltlaunch.com` behind Cloudflare). Emits a
//! real Chrome 131 TLS/JA3 + HTTP/2 handshake via `rquest` + BoringSSL.
//!
//! Lives as its own binary specifically to keep BoringSSL out of mcp-server's
//! link graph — see `Cargo.toml` for the full rationale.
//!
//! ## Contract
//!
//! ```text
//! listings-scraper --url <https-url>
//! ```
//!
//! On stdout: a JSON object `{ "status_code": <u16>, "body": <string> }`.
//! `body` is the upstream response body verbatim (UTF-8 text; for JSON
//! upstreams it is JSON-as-text — the caller decides how to parse it). Even
//! non-2xx responses produce stdout output: callers branch on `status_code`
//! and may want to log the body. The process exits 0.
//!
//! Exit code != 0 means the request never reached HTTP — TLS handshake
//! failure, DNS failure, timeout, malformed args. The reason is on stderr.

use anyhow::{anyhow, Context, Result};
use rquest::{header, Client, Impersonate};
use std::time::Duration;

#[derive(serde::Serialize)]
struct ScraperOutput {
    status_code: u16,
    body: String,
}

fn parse_url_arg() -> Result<String> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("--url") => args.next().ok_or_else(|| anyhow!("--url requires a value")),
        Some(other) => Err(anyhow!("unrecognized arg: {other}")),
        None => Err(anyhow!("missing required --url <https-url>")),
    }
}

fn build_client() -> Result<Client> {
    // Header bundle matches what a real Chrome-on-macOS sends, layered on
    // top of the JA3/HTTP2 fingerprint the impersonate() call provides.
    let mut headers = header::HeaderMap::new();
    let h = |v: &'static str| header::HeaderValue::from_static(v);
    headers.insert(
        header::ACCEPT,
        h("text/html,application/xhtml+xml,application/xml;q=0.9,application/json;q=0.9,*/*;q=0.8"),
    );
    headers.insert(header::ACCEPT_LANGUAGE, h("en-US,en;q=0.9"));
    headers.insert("DNT", h("1"));
    headers.insert("Sec-Fetch-Dest", h("document"));
    headers.insert("Sec-Fetch-Mode", h("navigate"));
    headers.insert("Sec-Fetch-Site", h("none"));
    headers.insert("Sec-Fetch-User", h("?1"));
    headers.insert("Upgrade-Insecure-Requests", h("1"));
    headers.insert("Sec-Ch-Ua-Mobile", h("?0"));
    headers.insert("Sec-Ch-Ua-Platform", h("\"macOS\""));
    headers.insert(
        "Sec-Ch-Ua",
        h("\"Google Chrome\";v=\"131\", \"Chromium\";v=\"131\", \"Not_A Brand\";v=\"24\""),
    );

    Client::builder()
        .impersonate(Impersonate::Chrome131)
        .user_agent(
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
             (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
        )
        .default_headers(headers)
        .timeout(Duration::from_secs(15))
        .connect_timeout(Duration::from_secs(5))
        .build()
        .context("rquest client build")
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let url = parse_url_arg()?;
    let client = build_client()?;
    let res = client.get(&url).send().await.context("send request")?;
    let status_code = res.status().as_u16();
    let body = res.text().await.context("read response body")?;
    let out = ScraperOutput { status_code, body };
    println!(
        "{}",
        serde_json::to_string(&out).context("serialize output")?
    );
    Ok(())
}
