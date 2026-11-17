#![deny(warnings)]
#![deny(clippy::all)]

//! SSRF-hardened outbound URL validation, DNS-pinned resolution, and bounded HTTP fetching.
//!
//! Defends against:
//! - Non-HTTPS downgrade (`http://`, `file://`, `ftp://`, `gopher://`)
//! - Trailing-dot FQDN filter bypasses (`https://metadata.google.internal./`, `https://svc.run.app./`)
//! - Cloud metadata (`169.254.169.254`, `metadata.google.internal`), loopback, RFC1918, CGNAT,
//!   multicast, benchmarking, unique-local (`fc00::/7`), link-local (`fe80::/10`), NAT64 (`64:ff9b::/96`),
//!   6to4 (`2002::/16`), Teredo (`2001::/32`), and IPv4-mapped IPv6 (`::ffff:0:0/96`)
//! - DNS rebinding TOCTOU between resolution check and socket connect (via `PinnedPublicTarget`)
//! - Open redirect chains (`301`/`302`/`307`/`308` to internal hosts)
//! - Unbounded response-body memory exhaustion (streamed chunk ceiling)

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;
use url::{Host, Url};

pub const DEFAULT_MAX_URL_BYTES: usize = 2048;
pub const DEFAULT_MAX_REDIRECTS: usize = 3;
pub const DEFAULT_ALLOWED_PORTS: &[u16] = &[443];
pub const DEFAULT_FORBIDDEN_HOSTS: &[&str] = &[
    "localhost",
    "metadata.google.internal",
    "metadata",
    "instance-data",
];
pub const DEFAULT_FORBIDDEN_HOST_SUFFIXES: &[&str] = &[
    "-wjucctfqta-uc.a.run.app",
    ".internal",
    ".localhost",
    ".local",
    ".nip.io",
    ".sslip.io",
    ".xip.io",
];

#[derive(Debug, Clone)]
pub struct FetchPolicy {
    pub max_url_bytes: usize,
    pub max_redirects: usize,
    pub allow_public_ip_literals: bool,
    pub allowed_ports: &'static [u16],
    pub forbidden_hosts: &'static [&'static str],
    pub forbidden_host_suffixes: &'static [&'static str],
    pub connect_timeout: Duration,
    pub total_timeout: Duration,
    pub max_response_bytes: usize,
}

impl Default for FetchPolicy {
    fn default() -> Self {
        Self::strict_domain_only(1024 * 1024)
    }
}

impl FetchPolicy {
    /// Strict policy for untrusted external proof/context/listing URLs:
    /// HTTPS on port 443, registered domain names only (no raw IP literals),
    /// DNS-pinned public IPs, up to 3 validated HTTPS redirects, and bounded body reads.
    pub const fn strict_domain_only(max_response_bytes: usize) -> Self {
        Self {
            max_url_bytes: DEFAULT_MAX_URL_BYTES,
            max_redirects: DEFAULT_MAX_REDIRECTS,
            allow_public_ip_literals: false,
            allowed_ports: DEFAULT_ALLOWED_PORTS,
            forbidden_hosts: DEFAULT_FORBIDDEN_HOSTS,
            forbidden_host_suffixes: DEFAULT_FORBIDDEN_HOST_SUFFIXES,
            connect_timeout: Duration::from_secs(5),
            total_timeout: Duration::from_secs(15),
            max_response_bytes,
        }
    }

    /// Policy for agent webhook endpoints: HTTPS on port 443, public domain or
    /// public IP literal allowed, trailing-dot and internal suffixes blocked,
    /// DNS-pinned public IPs, up to 3 validated HTTPS redirects, and bounded handshake response reads.
    pub const fn webhook(max_response_bytes: usize, timeout_secs: u64) -> Self {
        Self {
            max_url_bytes: DEFAULT_MAX_URL_BYTES,
            max_redirects: DEFAULT_MAX_REDIRECTS,
            allow_public_ip_literals: true,
            allowed_ports: DEFAULT_ALLOWED_PORTS,
            forbidden_hosts: DEFAULT_FORBIDDEN_HOSTS,
            forbidden_host_suffixes: DEFAULT_FORBIDDEN_HOST_SUFFIXES,
            connect_timeout: Duration::from_secs(5),
            total_timeout: Duration::from_secs(timeout_secs),
            max_response_bytes,
        }
    }

    /// Override the maximum number of validated, DNS-pinned HTTPS redirect hops (`0` disables redirects).
    pub const fn with_max_redirects(mut self, max_redirects: usize) -> Self {
        self.max_redirects = max_redirects;
        self
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SafeFetchError {
    #[error("url is required")]
    EmptyUrl,
    #[error("url exceeds {max} bytes")]
    UrlTooLong { max: usize },
    #[error("invalid url: {0}")]
    InvalidUrl(String),
    #[error("url scheme must be https")]
    NonHttpsScheme,
    #[error("url must not embed credentials")]
    EmbeddedCredentials,
    #[error("url must have a host")]
    MissingHost,
    #[error("port {0} is not allowed")]
    DisallowedPort(u16),
    #[error("ip literal hosts are not allowed")]
    IpLiteralDisallowed,
    #[error("host {0:?} is not a valid public target")]
    ForbiddenHost(String),
    #[error("address {0} is not a valid public target")]
    ForbiddenAddress(IpAddr),
    #[error("dns resolution failed: {0}")]
    DnsResolutionFailed(String),
    #[error("host resolved to no addresses")]
    EmptyDnsResolution,
    #[error("redirect limit exceeded (max {max})")]
    TooManyRedirects { max: usize },
    #[error("invalid redirect location: {0}")]
    InvalidRedirectLocation(String),
    #[error("http client error: {0}")]
    HttpClient(String),
    #[error("response body exceeds {max} bytes")]
    ResponseTooLarge { max: usize },
}

#[derive(Debug, Clone)]
pub struct ValidatedUrl {
    pub url: Url,
    pub host: String,
    pub port: u16,
    pub is_domain: bool,
}

#[derive(Debug, Clone)]
pub struct PinnedPublicTarget {
    pub url: Url,
    pub host: String,
    pub port: u16,
    pub addrs: Vec<SocketAddr>,
    pub needs_dns_pin: bool,
}

/// Returns `true` for any IPv4 address that is not globally routable on the public internet.
pub fn ipv4_is_non_global(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local() // 169.254.0.0/16 (includes GCP/AWS metadata 169.254.169.254)
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast() // 224.0.0.0/4
        || o[0] == 0 // 0.0.0.0/8 ("this network")
        || (o[0] == 100 && (o[1] & 0xC0) == 64) // 100.64.0.0/10 (CGNAT RFC 6598)
        || (o[0] == 192 && o[1] == 0 && o[2] == 0) // 192.0.0.0/24 (IETF protocol assignments)
        || (o[0] == 192 && o[1] == 0 && o[2] == 2) // 192.0.2.0/24 (TEST-NET-1)
        || (o[0] == 192 && o[1] == 88 && o[2] == 99) // 192.88.99.0/24 (deprecated 6to4 relay)
        || (o[0] == 198 && (o[1] & 0xFE) == 18) // 198.18.0.0/15 (benchmarking RFC 2544)
        || (o[0] == 198 && o[1] == 51 && o[2] == 100) // 198.51.100.0/24 (TEST-NET-2)
        || (o[0] == 203 && o[1] == 0 && o[2] == 113) // 203.0.113.0/24 (TEST-NET-3)
        || o[0] >= 240 // 240.0.0.0/4 (reserved / future use)
}

/// Returns `true` for any IPv6 address that is not globally routable on the public internet.
pub fn ipv6_is_non_global(ip: Ipv6Addr) -> bool {
    if let Some(mapped) = ip.to_ipv4_mapped() {
        return ipv4_is_non_global(mapped);
    }
    let s = ip.segments();
    // IPv4-compatible `::a.b.c.d`
    if s[0] == 0 && s[1] == 0 && s[2] == 0 && s[3] == 0 && s[4] == 0 && s[5] == 0 {
        return true;
    }
    // NAT64 well-known prefix `64:ff9b::/96` and local-use `64:ff9b:1::/48`
    if s[0] == 0x0064 && s[1] == 0xff9b {
        let embedded = Ipv4Addr::new(
            (s[6] >> 8) as u8,
            (s[6] & 0xff) as u8,
            (s[7] >> 8) as u8,
            (s[7] & 0xff) as u8,
        );
        if s[2] != 0 || s[3] != 0 || s[4] != 0 || s[5] != 0 || ipv4_is_non_global(embedded) {
            return true;
        }
    }
    // 6to4 `2002::/16` embeds IPv4 in `s[1..=2]`
    if s[0] == 0x2002 {
        let embedded = Ipv4Addr::new(
            (s[1] >> 8) as u8,
            (s[1] & 0xff) as u8,
            (s[2] >> 8) as u8,
            (s[2] & 0xff) as u8,
        );
        if ipv4_is_non_global(embedded) {
            return true;
        }
    }
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (s[0] & 0xfe00) == 0xfc00 // fc00::/7 unique-local
        || (s[0] & 0xffc0) == 0xfe80 // fe80::/10 link-local
        || (s[0] == 0x0100 && s[1] == 0 && s[2] == 0 && s[3] == 0) // 100::/64 discard
        || (s[0] == 0x2001 && s[1] == 0x0000) // 2001::/32 Teredo
        || (s[0] == 0x2001 && s[1] == 0x0002 && s[2] == 0) // 2001:2::/48 benchmarking
        || (s[0] == 0x2001 && s[1] == 0x0db8) // 2001:db8::/32 documentation
        || (s[0] == 0x2001 && (s[1] & 0xfff0) == 0x0010) // 2001:10::/28 ORCHID
        || (s[0] == 0x2001 && (s[1] & 0xfff0) == 0x0020) // 2001:20::/28 ORCHIDv2
}

/// Returns `true` when `ip` is loopback, private, link-local/metadata, CGNAT, ULA,
/// documentation, multicast, or otherwise non-global.
pub fn ip_is_non_global(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => ipv4_is_non_global(v4),
        IpAddr::V6(v6) => ipv6_is_non_global(v6),
    }
}

/// Pure, synchronous URL boundary validation (no DNS I/O).
pub fn validate_public_url(
    raw: &str,
    policy: &FetchPolicy,
) -> Result<ValidatedUrl, SafeFetchError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(SafeFetchError::EmptyUrl);
    }
    if raw.len() > policy.max_url_bytes {
        return Err(SafeFetchError::UrlTooLong {
            max: policy.max_url_bytes,
        });
    }
    let parsed = Url::parse(raw).map_err(|e| SafeFetchError::InvalidUrl(e.to_string()))?;
    if parsed.scheme() != "https" {
        return Err(SafeFetchError::NonHttpsScheme);
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(SafeFetchError::EmbeddedCredentials);
    }
    let port = parsed.port_or_known_default().unwrap_or(443);
    if !policy.allowed_ports.is_empty() && !policy.allowed_ports.contains(&port) {
        return Err(SafeFetchError::DisallowedPort(port));
    }

    match parsed.host() {
        None => Err(SafeFetchError::MissingHost),
        Some(Host::Domain(domain)) => {
            let lower = domain.to_ascii_lowercase();
            // Reject trailing-dot FQDNs (`metadata.google.internal.`, `svc.run.app.`, `.`)
            // AND evaluate forbidden host/suffix rules against the dot-stripped label.
            let stripped = lower.trim_end_matches('.');
            if stripped.is_empty() || lower.ends_with('.') {
                return Err(SafeFetchError::ForbiddenHost(lower));
            }
            let forbidden = policy.forbidden_hosts.contains(&stripped)
                || policy
                    .forbidden_host_suffixes
                    .iter()
                    .any(|suffix| stripped.ends_with(suffix));
            if forbidden {
                return Err(SafeFetchError::ForbiddenHost(lower));
            }
            // Catch decimal/hex/octal IPv4 encodings that `url` parsed or left as Domain.
            if let Ok(ip) = stripped.parse::<IpAddr>() {
                if !policy.allow_public_ip_literals {
                    return Err(SafeFetchError::IpLiteralDisallowed);
                }
                if ip_is_non_global(ip) {
                    return Err(SafeFetchError::ForbiddenAddress(ip));
                }
            }
            Ok(ValidatedUrl {
                url: parsed,
                host: stripped.to_string(),
                port,
                is_domain: true,
            })
        }
        Some(Host::Ipv4(v4)) => {
            if !policy.allow_public_ip_literals {
                return Err(SafeFetchError::IpLiteralDisallowed);
            }
            if ipv4_is_non_global(v4) {
                return Err(SafeFetchError::ForbiddenAddress(IpAddr::V4(v4)));
            }
            Ok(ValidatedUrl {
                url: parsed,
                host: v4.to_string(),
                port,
                is_domain: false,
            })
        }
        Some(Host::Ipv6(v6)) => {
            if !policy.allow_public_ip_literals {
                return Err(SafeFetchError::IpLiteralDisallowed);
            }
            if ipv6_is_non_global(v6) {
                return Err(SafeFetchError::ForbiddenAddress(IpAddr::V6(v6)));
            }
            Ok(ValidatedUrl {
                url: parsed,
                host: format!("[{v6}]"),
                port,
                is_domain: false,
            })
        }
    }
}

/// Resolve a redirect `Location` header relative to `base_url` without validating policy yet,
/// returning `InvalidRedirectLocation` if `location` is empty or unparseable.
pub fn join_redirect_location(base_url: &Url, location: &str) -> Result<Url, SafeFetchError> {
    let trimmed = location.trim();
    if trimmed.is_empty() {
        return Err(SafeFetchError::InvalidRedirectLocation(
            "empty Location header".to_string(),
        ));
    }
    base_url
        .join(trimmed)
        .map_err(|e| SafeFetchError::InvalidRedirectLocation(e.to_string()))
}

fn pin_resolved_addrs(
    validated: ValidatedUrl,
    resolved: impl IntoIterator<Item = SocketAddr>,
) -> Result<PinnedPublicTarget, SafeFetchError> {
    let mut addrs = Vec::with_capacity(16);
    for addr in resolved.into_iter().take(16) {
        if ip_is_non_global(addr.ip()) {
            return Err(SafeFetchError::ForbiddenAddress(addr.ip()));
        }
        addrs.push(addr);
    }
    if addrs.is_empty() {
        return Err(SafeFetchError::EmptyDnsResolution);
    }
    Ok(PinnedPublicTarget {
        url: validated.url,
        host: validated.host,
        port: validated.port,
        addrs,
        needs_dns_pin: true,
    })
}

/// Validate `raw_url`, resolve DNS once under `policy.connect_timeout`, verify every
/// returned address is globally routable, and return socket addresses ready for pinning.
pub async fn resolve_and_pin_public_url(
    raw_url: &str,
    policy: &FetchPolicy,
) -> Result<PinnedPublicTarget, SafeFetchError> {
    let validated = validate_public_url(raw_url, policy)?;
    if !validated.is_domain {
        let ip: IpAddr = match validated.url.host() {
            Some(Host::Ipv4(v4)) => IpAddr::V4(v4),
            Some(Host::Ipv6(v6)) => IpAddr::V6(v6),
            _ => return Err(SafeFetchError::MissingHost),
        };
        return Ok(PinnedPublicTarget {
            url: validated.url,
            host: validated.host,
            port: validated.port,
            addrs: vec![SocketAddr::new(ip, validated.port)],
            needs_dns_pin: false,
        });
    }

    let lookup_fut = tokio::net::lookup_host((validated.host.as_str(), validated.port));
    let resolved: Vec<SocketAddr> = tokio::time::timeout(policy.connect_timeout, lookup_fut)
        .await
        .map_err(|_| SafeFetchError::DnsResolutionFailed("dns lookup timed out".to_string()))?
        .map_err(|e| SafeFetchError::DnsResolutionFailed(e.to_string()))?
        .take(16)
        .collect();

    pin_resolved_addrs(validated, resolved)
}

#[cfg(feature = "reqwest")]
pub mod reqwest_client {
    use super::{
        join_redirect_location, resolve_and_pin_public_url, FetchPolicy, PinnedPublicTarget,
        SafeFetchError,
    };
    use std::future::Future;

    #[derive(Debug)]
    pub struct SafeResponse {
        pub status: u16,
        pub headers: reqwest::header::HeaderMap,
        pub body: Vec<u8>,
    }

    /// Build a redirect-disabled, timeout-bounded, DNS-pinned `reqwest::Client`.
    /// Fails closed on builder error (never falls back to an unpinned client).
    pub fn build_pinned_client(
        target: &PinnedPublicTarget,
        policy: &FetchPolicy,
        user_agent: Option<&str>,
    ) -> Result<reqwest::Client, SafeFetchError> {
        let mut builder = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(policy.connect_timeout)
            .timeout(policy.total_timeout);
        if let Some(ua) = user_agent {
            builder = builder.user_agent(ua);
        }
        if target.needs_dns_pin {
            builder = builder.resolve_to_addrs(&target.host, &target.addrs);
        }
        builder
            .build()
            .map_err(|e| SafeFetchError::HttpClient(e.to_string()))
    }

    /// Stream response chunks with a hard ceiling of `max_bytes` (rejecting oversize
    /// `Content-Length` headers and chunked streams before allocating unbounded buffers).
    pub async fn read_response_bounded(
        mut resp: reqwest::Response,
        max_bytes: usize,
    ) -> Result<SafeResponse, SafeFetchError> {
        if let Some(len) = resp.content_length() {
            if len > max_bytes as u64 {
                return Err(SafeFetchError::ResponseTooLarge { max: max_bytes });
            }
        }
        let status = resp.status().as_u16();
        let headers = resp.headers().clone();
        let initial_cap = resp
            .content_length()
            .and_then(|n| usize::try_from(n).ok())
            .unwrap_or(4096)
            .min(max_bytes.min(64 * 1024));
        let mut body = Vec::with_capacity(initial_cap);
        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|e| SafeFetchError::HttpClient(e.to_string()))?
        {
            let next_len = body
                .len()
                .checked_add(chunk.len())
                .ok_or(SafeFetchError::ResponseTooLarge { max: max_bytes })?;
            if next_len > max_bytes {
                return Err(SafeFetchError::ResponseTooLarge { max: max_bytes });
            }
            body.extend_from_slice(&chunk);
        }
        Ok(SafeResponse {
            status,
            headers,
            body,
        })
    }

    fn is_redirect_status(status: u16) -> bool {
        matches!(status, 301 | 302 | 303 | 307 | 308)
    }

    pub(crate) async fn execute_hop_loop<R, RFut, S, SFut>(
        raw_url: &str,
        policy: &FetchPolicy,
        mut resolve_fn: R,
        mut send_fn: S,
    ) -> Result<SafeResponse, SafeFetchError>
    where
        R: FnMut(String) -> RFut,
        RFut: Future<Output = Result<PinnedPublicTarget, SafeFetchError>>,
        S: FnMut(PinnedPublicTarget) -> SFut,
        SFut: Future<Output = Result<SafeResponse, SafeFetchError>>,
    {
        let mut current_url = raw_url.to_string();
        for hop in 0..=policy.max_redirects {
            let target = resolve_fn(current_url).await?;
            let target_url = target.url.clone();
            let resp = send_fn(target).await?;
            if is_redirect_status(resp.status) {
                if hop == policy.max_redirects {
                    return Err(SafeFetchError::TooManyRedirects {
                        max: policy.max_redirects,
                    });
                }
                let loc_val = resp.headers.get(reqwest::header::LOCATION).ok_or_else(|| {
                    SafeFetchError::InvalidRedirectLocation("missing Location header".to_string())
                })?;
                let loc_str = loc_val.to_str().map_err(|_| {
                    SafeFetchError::InvalidRedirectLocation("non-ASCII Location header".to_string())
                })?;
                let next = join_redirect_location(&target_url, loc_str)?;
                current_url = next.to_string();
                continue;
            }
            return Ok(resp);
        }
        Err(SafeFetchError::TooManyRedirects {
            max: policy.max_redirects,
        })
    }

    /// Perform an SSRF-screened, DNS-pinned `GET` with bounded body read, following
    /// up to `policy.max_redirects` validated HTTPS redirect hops (with reqwest's own
    /// automatic redirect following strictly disabled so every hop is re-validated
    /// and DNS-pinned before socket connect).
    pub async fn safe_get_bytes(
        raw_url: &str,
        policy: &FetchPolicy,
        user_agent: Option<&str>,
    ) -> Result<SafeResponse, SafeFetchError> {
        execute_hop_loop(
            raw_url,
            policy,
            |url| async move { resolve_and_pin_public_url(&url, policy).await },
            |target| async move {
                let client = build_pinned_client(&target, policy, user_agent)?;
                let resp = client
                    .get(target.url.as_str())
                    .send()
                    .await
                    .map_err(|e| SafeFetchError::HttpClient(e.to_string()))?;
                if is_redirect_status(resp.status().as_u16()) {
                    Ok(SafeResponse {
                        status: resp.status().as_u16(),
                        headers: resp.headers().clone(),
                        body: Vec::new(),
                    })
                } else {
                    read_response_bounded(resp, policy.max_response_bytes).await
                }
            },
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trailing_dots_and_internal_suffixes_are_rejected() {
        let webhook_policy = FetchPolicy::webhook(16 * 1024, 10);
        let rejected = [
            "http://example.com/hook",
            "https://example.com./hook",
            "https://metadata.google.internal./computeMetadata/v1/",
            "https://mcp-server-xyz.a.run.app./hook",
            "https://shillbot-api-wjucctfqta-uc.a.run.app/tasks",
            "https://shillbot-api-wjucctfqta-uc.a.run.app./tasks",
            "https://localhost./hook",
            "https://foo.local./hook",
            "https://127.0.0.1.nip.io/hook",
            "https://169.254.169.254.sslip.io/hook",
            "https://example.com:8443/hook",
            "https://user:pass@example.com/hook",
            "https://169.254.169.254/latest",
            "https://100.64.0.1/hook",
            "https://198.18.0.1/hook",
            "https://192.0.2.1/hook",
            "https://[::ffff:169.254.169.254]/hook",
            "https://[64:ff9b::a9fe:a9fe]/hook",
            "https://[2002:a9fe:a9fe::1]/hook",
            "https://[fc00::1]/hook",
            "https://[fe80::1]/hook",
            "https://[2001:db8::1]/hook",
        ];
        for raw in rejected {
            assert!(
                validate_public_url(raw, &webhook_policy).is_err(),
                "{raw} must be rejected"
            );
        }

        let strict_policy = FetchPolicy::strict_domain_only(1024 * 1024);
        assert_eq!(
            validate_public_url("https://8.8.8.8/data.json", &strict_policy).unwrap_err(),
            SafeFetchError::IpLiteralDisallowed
        );
        assert!(validate_public_url("https://8.8.8.8/hook", &webhook_policy).is_ok());
        assert!(validate_public_url("https://agent.example.com/hook", &strict_policy).is_ok());
    }

    #[test]
    fn external_cloud_run_allowed_while_own_project_run_app_suffix_is_blocked() {
        let webhook_policy = FetchPolicy::webhook(16 * 1024, 10);
        let strict_policy = FetchPolicy::strict_domain_only(1024 * 1024);

        for allowed in [
            "https://my-agent-abc-uc.a.run.app/webhook",
            "https://agent-hook-99999-ew.a.run.app/v1/inbox",
            "https://external-service.run.app/proof.lean",
        ] {
            assert!(
                validate_public_url(allowed, &webhook_policy).is_ok(),
                "{allowed} must be allowed under webhook policy"
            );
            assert!(
                validate_public_url(allowed, &strict_policy).is_ok(),
                "{allowed} must be allowed under strict policy"
            );
        }

        for blocked in [
            "https://shillbot-api-wjucctfqta-uc.a.run.app/internal/tasks",
            "https://mcp-server-wjucctfqta-uc.a.run.app/mcp",
            "https://game-api-wjucctfqta-uc.a.run.app./health",
            "https://SHILLBOT-API-WJUCCtfQTA-UC.A.RUN.APP/hook",
        ] {
            assert!(
                matches!(
                    validate_public_url(blocked, &webhook_policy),
                    Err(SafeFetchError::ForbiddenHost(_))
                ),
                "{blocked} must fail with ForbiddenHost"
            );
            assert!(
                matches!(
                    validate_public_url(blocked, &strict_policy),
                    Err(SafeFetchError::ForbiddenHost(_))
                ),
                "{blocked} must fail with ForbiddenHost under strict policy"
            );
        }
    }

    #[cfg(feature = "reqwest")]
    #[tokio::test]
    async fn bounded_reader_enforces_hard_byte_ceiling() {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![b'x'; 2048]))
            .mount(&server)
            .await;

        let resp = reqwest::get(server.uri()).await.expect("get");
        let err = reqwest_client::read_response_bounded(resp, 1024)
            .await
            .expect_err("must reject > 1024 bytes");
        assert_eq!(err, SafeFetchError::ResponseTooLarge { max: 1024 });
    }

    #[cfg(feature = "reqwest")]
    #[tokio::test]
    async fn hop_by_hop_redirects_pin_each_hop_and_block_ssrf_downgrades() {
        use reqwest::header::{HeaderMap, HeaderValue, LOCATION};
        use std::net::Ipv4Addr;

        let public_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(93, 184, 216, 34)), 443);
        let strict = FetchPolicy::strict_domain_only(64 * 1024);
        assert_eq!(strict.max_redirects, 3);

        let resolve_public = |url: String| {
            let policy = strict.clone();
            async move {
                let validated = validate_public_url(&url, &policy)?;
                pin_resolved_addrs(validated, [public_addr])
            }
        };

        // 1. Valid 3-hop chain (gist -> relative -> external Cloud Run -> final 200) succeeds.
        let mut seen_urls = Vec::new();
        let ok_resp = reqwest_client::execute_hop_loop(
            "https://gist.github.com/user/abc/raw",
            &strict,
            resolve_public,
            |target| {
                seen_urls.push(target.url.to_string());
                async move {
                    let mut headers = HeaderMap::new();
                    match target.url.as_str() {
                        "https://gist.github.com/user/abc/raw" => {
                            headers.insert(LOCATION, HeaderValue::from_static("/user/abc/raw/v2"));
                            Ok(reqwest_client::SafeResponse {
                                status: 302,
                                headers,
                                body: Vec::new(),
                            })
                        }
                        "https://gist.github.com/user/abc/raw/v2" => {
                            headers.insert(
                                LOCATION,
                                HeaderValue::from_static(
                                    "https://my-agent-abc-uc.a.run.app/proof.lean",
                                ),
                            );
                            Ok(reqwest_client::SafeResponse {
                                status: 301,
                                headers,
                                body: Vec::new(),
                            })
                        }
                        "https://my-agent-abc-uc.a.run.app/proof.lean" => {
                            headers.insert(
                                LOCATION,
                                HeaderValue::from_static(
                                    "https://raw.githubusercontent.com/user/abc/Proof.lean",
                                ),
                            );
                            Ok(reqwest_client::SafeResponse {
                                status: 307,
                                headers,
                                body: Vec::new(),
                            })
                        }
                        "https://raw.githubusercontent.com/user/abc/Proof.lean" => {
                            Ok(reqwest_client::SafeResponse {
                                status: 200,
                                headers,
                                body: b"theorem proof : True := trivial".to_vec(),
                            })
                        }
                        other => panic!("unexpected hop url: {other}"),
                    }
                }
            },
        )
        .await
        .expect("3 safe HTTPS redirect hops must succeed");
        assert_eq!(ok_resp.status, 200);
        assert_eq!(ok_resp.body, b"theorem proof : True := trivial");
        assert_eq!(seen_urls.len(), 4);

        // 2. Redirect to http:// fails closed with NonHttpsScheme.
        let http_downgrade = reqwest_client::execute_hop_loop(
            "https://example.com/start",
            &strict,
            resolve_public,
            |_| async move {
                let mut headers = HeaderMap::new();
                headers.insert(
                    LOCATION,
                    HeaderValue::from_static("http://169.254.169.254/latest/meta-data/"),
                );
                Ok(reqwest_client::SafeResponse {
                    status: 302,
                    headers,
                    body: Vec::new(),
                })
            },
        )
        .await
        .expect_err("http:// redirect must fail");
        assert_eq!(http_downgrade, SafeFetchError::NonHttpsScheme);

        // 3. Redirect to https://127.0.0.1 or https://169.254.169.254 fails closed under both strict & webhook policies.
        let webhook = FetchPolicy::webhook(16 * 1024, 10);
        for bad_loc in ["https://127.0.0.1/admin", "https://169.254.169.254/latest"] {
            let strict_err = reqwest_client::execute_hop_loop(
                "https://example.com/start",
                &strict,
                resolve_public,
                |_| async move {
                    let mut headers = HeaderMap::new();
                    headers.insert(LOCATION, HeaderValue::from_str(bad_loc).unwrap());
                    Ok(reqwest_client::SafeResponse {
                        status: 302,
                        headers,
                        body: Vec::new(),
                    })
                },
            )
            .await
            .expect_err("strict policy must reject IP redirect");
            assert_eq!(strict_err, SafeFetchError::IpLiteralDisallowed);

            let webhook_err = reqwest_client::execute_hop_loop(
                "https://example.com/start",
                &webhook,
                |url| {
                    let policy = webhook.clone();
                    async move {
                        let validated = validate_public_url(&url, &policy)?;
                        if !validated.is_domain {
                            let ip: IpAddr = validated.host.parse().unwrap();
                            return Ok(PinnedPublicTarget {
                                url: validated.url,
                                host: validated.host,
                                port: validated.port,
                                addrs: vec![SocketAddr::new(ip, validated.port)],
                                needs_dns_pin: false,
                            });
                        }
                        pin_resolved_addrs(validated, [public_addr])
                    }
                },
                |_| async move {
                    let mut headers = HeaderMap::new();
                    headers.insert(LOCATION, HeaderValue::from_str(bad_loc).unwrap());
                    Ok(reqwest_client::SafeResponse {
                        status: 302,
                        headers,
                        body: Vec::new(),
                    })
                },
            )
            .await
            .expect_err("webhook policy must reject loopback/metadata IP redirect");
            assert!(matches!(webhook_err, SafeFetchError::ForbiddenAddress(_)));
        }

        // 4. Redirect to domain whose DNS resolves to non-global IP (DNS rebinding hop) fails with ForbiddenAddress.
        let dns_rebind_err = reqwest_client::execute_hop_loop(
            "https://example.com/start",
            &strict,
            |url| {
                let policy = strict.clone();
                async move {
                    let validated = validate_public_url(&url, &policy)?;
                    let ip = if validated.host == "rebind.example.org" {
                        Ipv4Addr::new(169, 254, 169, 254)
                    } else {
                        Ipv4Addr::new(93, 184, 216, 34)
                    };
                    pin_resolved_addrs(validated, [SocketAddr::new(IpAddr::V4(ip), 443)])
                }
            },
            |_| async move {
                let mut headers = HeaderMap::new();
                headers.insert(
                    LOCATION,
                    HeaderValue::from_static("https://rebind.example.org/secret"),
                );
                Ok(reqwest_client::SafeResponse {
                    status: 302,
                    headers,
                    body: Vec::new(),
                })
            },
        )
        .await
        .expect_err("redirect target resolving to metadata IP must fail");
        assert_eq!(
            dns_rebind_err,
            SafeFetchError::ForbiddenAddress(IpAddr::V4(Ipv4Addr::new(169, 254, 169, 254)))
        );

        // 5. Exceeding max_redirects fails with TooManyRedirects.
        let capped = strict.clone().with_max_redirects(2);
        let too_many = reqwest_client::execute_hop_loop(
            "https://example.com/hop0",
            &capped,
            |url| {
                let policy = capped.clone();
                async move {
                    let validated = validate_public_url(&url, &policy)?;
                    pin_resolved_addrs(validated, [public_addr])
                }
            },
            |target| async move {
                let mut headers = HeaderMap::new();
                let next_path = format!("{}x", target.url.path());
                headers.insert(LOCATION, HeaderValue::from_str(&next_path).unwrap());
                Ok(reqwest_client::SafeResponse {
                    status: 302,
                    headers,
                    body: Vec::new(),
                })
            },
        )
        .await
        .expect_err("3rd redirect under max_redirects=2 must fail");
        assert_eq!(too_many, SafeFetchError::TooManyRedirects { max: 2 });

        // 6. Missing Location header on redirect fails with InvalidRedirectLocation.
        let missing_loc = reqwest_client::execute_hop_loop(
            "https://example.com/start",
            &strict,
            resolve_public,
            |_| async move {
                Ok(reqwest_client::SafeResponse {
                    status: 302,
                    headers: HeaderMap::new(),
                    body: Vec::new(),
                })
            },
        )
        .await
        .expect_err("302 without Location must fail");
        assert!(matches!(
            missing_loc,
            SafeFetchError::InvalidRedirectLocation(_)
        ));
    }
}
