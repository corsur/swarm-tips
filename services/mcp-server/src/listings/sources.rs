use crate::listings::models::{HealthCheck, RawListing};
use chrono::{DateTime, NaiveDateTime, Utc};
use std::time::Instant;

/// Upper bound on shillbot open tasks pulled per fetch. Set above the
/// orchestrator's own 100 max-per-request cap so the whole open queue surfaces,
/// while still bounding the loop against an unbounded upstream response.
const MAX_SHILLBOT_TASKS: usize = 200;

/// Result of fetching from one source: listings + health check data.
pub struct FetchResult {
    pub source: String,
    pub listings: Vec<RawListing>,
    pub health: HealthCheck,
}

/// Time one source's fetch and fold the outcome into a `FetchResult`.
///
/// This wrapper — the `Instant`, the Ok/Err match, and the `HealthCheck`
/// assembly — was duplicated verbatim in all five `fetch_*` functions, so a
/// change to how health is recorded had to be made five times. Each fetcher now
/// passes only its own request+parse future.
///
/// The per-source warn on failure moves from a static literal to a dynamic
/// field, which is structurally equivalent in Cloud Logging.
async fn timed_fetch<F>(source: &str, fut: F) -> FetchResult
where
    F: std::future::Future<Output = Result<(Vec<RawListing>, u16), reqwest::Error>>,
{
    let start = Instant::now();
    let result = fut.await;
    let elapsed_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok((listings, status)) => {
            let count = listings.len() as u32;
            FetchResult {
                source: source.to_string(),
                listings,
                health: HealthCheck {
                    timestamp: Utc::now(),
                    status_code: status,
                    response_ms: elapsed_ms,
                    listing_count: count,
                    error: None,
                },
            }
        }
        Err(e) => {
            tracing::warn!(source = %source, error = %e, "fetch failed");
            FetchResult {
                source: source.to_string(),
                listings: vec![],
                health: HealthCheck {
                    timestamp: Utc::now(),
                    status_code: 0,
                    response_ms: elapsed_ms,
                    listing_count: 0,
                    error: Some(e.to_string()),
                },
            }
        }
    }
}

/// Fetch open bounties from BotBounty (Base / ETH).
pub async fn fetch_botbounty(client: &reqwest::Client) -> FetchResult {
    timed_fetch("botbounty", async {
        let res = client
            .get("https://botbounty-production.up.railway.app/api/agent/bounties")
            .header("Content-Type", "application/json")
            .send()
            .await?;

        let status = res.status().as_u16();
        if !res.status().is_success() {
            tracing::warn!(source = "botbounty", status, "non-success response");
            return Ok::<(Vec<RawListing>, u16), reqwest::Error>((vec![], status));
        }

        let data: serde_json::Value = res.json().await?;
        let bounties = if data.is_array() {
            data.as_array().cloned().unwrap_or_default()
        } else {
            data.get("bounties")
                .and_then(|b| b.as_array())
                .cloned()
                .unwrap_or_default()
        };

        let listings: Vec<RawListing> = bounties
            .iter()
            .take(20)
            .filter_map(parse_botbounty)
            .collect();

        Ok((listings, status))
    })
    .await
}

/// Hardcoded ETH price fallback for USD estimation. Same staleness policy as
/// SOL_PRICE_USD below: the listings card rounds to whole dollars, so a stale
/// constant is tolerable until ETH moves >50% from this figure. It is currently
/// well BELOW market, which understates every ETH-denominated reward — refresh
/// it when the gap gets embarrassing.
const ETH_PRICE_USD: f64 = 2000.0;

/// Hardcoded SOL price fallback for USD estimation. Used by sources that
/// quote rewards in lamports. A live price feed would be nicer but the
/// listings card is rounded to whole dollars so a stale constant is fine
/// until SOL moves >50%.
const SOL_PRICE_USD: f64 = 150.0;

fn parse_botbounty(b: &serde_json::Value) -> Option<RawListing> {
    let id = b.get("id")?.to_string().trim_matches('"').to_string();
    let amount_str = b
        .get("amount")
        .map(|v| v.to_string().trim_matches('"').to_string())
        .unwrap_or_else(|| "0".to_string());
    let amount: f64 = amount_str.parse().unwrap_or(0.0);

    Some(RawListing {
        source: "botbounty".to_string(),
        source_id: id.clone(),
        source_url: format!("https://www.botbounty.ai/bounty/{id}"),
        title: str_field(b, "title").unwrap_or_else(|| "Untitled".to_string()),
        description: str_field(b, "description")
            .unwrap_or_default()
            .chars()
            .take(500)
            .collect(),
        category: str_field(b, "category").unwrap_or_else(|| "code".to_string()),
        tags: b
            .get("tags")
            .and_then(|t| t.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        reward_amount: amount_str,
        reward_token: "ETH".to_string(),
        reward_chain: "base".to_string(),
        reward_usd_estimate: Some(amount * ETH_PRICE_USD),
        payment_model: "fixed".to_string(),
        escrow: true,
        posted_at: parse_datetime(b.get("created_at")).unwrap_or_else(Utc::now),
        deadline: None,
    })
}

// moltlaunch source removed 2026-06-30: api.moltlaunch.com was decommissioned
// (DNS gone; it breaks moltlaunch's own explore page too) with no replacement
// endpoint — the platform pivoted off the gig marketplace. Dropped per the
// listing policy; re-add a fetch_* if/when a working gigs API returns.

/// Fetch open content tasks from the Shillbot orchestrator (Solana / SOL).
///
/// Shillbot is one of swarm.tips' own verticals — the AI-agent task
/// marketplace where clients pay agents in escrowed SOL to create short-form
/// content. Surfacing live Shillbot tasks under EARN closes the loop: the
/// landing page promises agent earning opportunities, and the DAO's own
/// marketplace is the most agent-native one we have. Without this source the
/// frontend never auto-picked up new Shillbot campaigns.
pub async fn fetch_shillbot(client: &reqwest::Client) -> FetchResult {
    timed_fetch("shillbot", async {
        // Orchestrator's /tasks defaults to ~10 results without ?limit.
        // First-party Shillbot is our highest-trust source — pull everything
        // it's offering. 200 is well above realistic queue depth; the
        // orchestrator clamps to its own 100 max-per-request cap, so this asks
        // for more than it can return and takes whatever comes back. Surfaced
        // 2026-05-11 when
        // 9 of 15 mainnet tasks were silently truncated from swarm.tips.
        let res = client
            .get("https://api.shillbot.org/tasks?limit=200")
            .send()
            .await?;

        let status = res.status().as_u16();
        if !res.status().is_success() {
            tracing::warn!(source = "shillbot", status, "non-success response");
            return Ok::<(Vec<RawListing>, u16), reqwest::Error>((vec![], status));
        }

        let data: serde_json::Value = res.json().await?;
        let tasks = data
            .get("tasks")
            .and_then(|t| t.as_array())
            .cloned()
            .unwrap_or_default();

        let listings: Vec<RawListing> = tasks
            .iter()
            .filter(|t| {
                t.get("state")
                    .and_then(|v| v.as_str())
                    .map(|s| s == "open")
                    .unwrap_or(false)
            })
            .take(MAX_SHILLBOT_TASKS)
            .filter_map(parse_shillbot_task)
            .collect();

        Ok((listings, status))
    })
    .await
}

/// Map a Shillbot platform enum integer to a human-readable label.
/// Must match the discriminants actually used in production by the
/// orchestrator/verifier (which diverged from the original PlatformType
/// *names* in the shared crate — only the discriminant numbers are the
/// contract). Keep this table in sync with
/// `coordination-app/backend/shillbot-api/src/services/campaign_service.rs::default_cohort_for_platform`.
fn shillbot_platform_label(platform: i64) -> &'static str {
    match platform {
        0 => "youtube",
        3 => "twitter",
        4 => "referral",
        5 => "game-play",
        9 => "website",
        _ => "other",
    }
}

/// Verb that matches the work shape of each platform. Used in the short
/// description string surfaced to agents in `list_earning_opportunities`.
fn shillbot_platform_action(platform: i64) -> &'static str {
    match platform {
        0 => "Create a youtube short.",
        3 => "Post an X thread.",
        4 => "Create a shillbot campaign.",
        5 => "Play a round of coordination.game.",
        9 => "Place a swarm.tips footer backlink on a site you control.",
        _ => "Complete a shillbot task.",
    }
}

fn parse_shillbot_task(t: &serde_json::Value) -> Option<RawListing> {
    let task_id = str_field(t, "task_id")?;
    let topic = str_field(t, "campaign_topic").unwrap_or_else(|| "Shillbot task".to_string());

    // Drop tasks without an estimated payment — they're not actionable as
    // earning opportunities.
    let lamports = t
        .get("estimated_payment_lamports")
        .and_then(|v| v.as_u64())?;
    if lamports == 0 {
        return None;
    }
    let sol_amount = (lamports as f64) / 1e9;

    let platform_int = t.get("platform").and_then(|v| v.as_i64()).unwrap_or(-1);
    let platform_label = shillbot_platform_label(platform_int);

    let brief = t.get("brief");
    let cta = brief
        .and_then(|b| b.get("cta"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let voice = brief
        .and_then(|b| b.get("brand_voice"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Description: combine voice + cta + platform so the swarm.tips card has
    // enough context for an agent to decide whether to pursue. The gasless note
    // sits right after the action verb (never past the 500-char truncation
    // below) so a $0 wallet knows it can take this task — onboard vouches it and
    // fronts rent, then claim/submit are sponsor-paid.
    let action = shillbot_platform_action(platform_int);
    let description = format!(
        "{action} No SOL needed — start gaslessly via shillbot_onboard. {voice} CTA: {cta}"
    )
    .chars()
    .take(500)
    .collect::<String>();

    // The shillbot.org frontend only routes /tasks (the TaskBoard); there's
    // no per-task detail page. Linking to /tasks/{id} 404s. Send agents to
    // the public board where they can browse + claim. Surfaced 2026-05-11.
    Some(RawListing {
        source: "shillbot".to_string(),
        source_id: task_id.clone(),
        source_url: "https://shillbot.org/tasks".to_string(),
        title: topic,
        description,
        category: "content".to_string(),
        tags: vec!["solana".to_string(), platform_label.to_string()],
        reward_amount: format!("{sol_amount:.4}"),
        reward_token: "SOL".to_string(),
        reward_chain: "solana".to_string(),
        reward_usd_estimate: Some(sol_amount * SOL_PRICE_USD),
        payment_model: "fixed".to_string(),
        escrow: true,
        posted_at: parse_datetime(t.get("created_at")).unwrap_or_else(Utc::now),
        deadline: None,
    })
}

/// Fetch AI-agent platforms from DefiLlama's "AI Agents" + "Decentralized AI"
/// categories (https://defillama.com/protocols/ai-agents).
///
/// These are *platforms*, not individual jobs — they get persisted as
/// `category = "platform-candidate"` so the existing reward filter drops
/// them from the public listings response while still landing them in
/// Firestore for the survey doc and future job-probe pipelines. The point
/// is meta-discovery: when a new crypto-native agent platform launches, it
/// shows up in DefiLlama within days, and we want it queryable here so we
/// can decide whether to integrate it as a real first-party listings source.
pub async fn fetch_defillama_ai_agents(client: &reqwest::Client) -> FetchResult {
    let result = timed_fetch("defillama-ai", async {
        let res = client
            .get("https://api.llama.fi/protocols")
            .header(
                reqwest::header::USER_AGENT,
                "SwarmTipsDiscovery/0.1 (+https://swarm.tips)",
            )
            .send()
            .await?;

        let status = res.status().as_u16();
        if !res.status().is_success() {
            tracing::warn!(source = "defillama-ai", status, "non-success response");
            return Ok::<(Vec<RawListing>, u16), reqwest::Error>((vec![], status));
        }

        let data: serde_json::Value = res.json().await?;
        let protocols = data.as_array().cloned().unwrap_or_default();

        // Bounded loop: DefiLlama returns ~7K protocols, we filter on category.
        // Cap input iteration at MAX_PROTOCOLS as a safety measure even though
        // we expect well under 100 matching entries today.
        const MAX_PROTOCOLS: usize = 20_000;
        let listings: Vec<RawListing> = protocols
            .iter()
            .take(MAX_PROTOCOLS)
            .filter(|p| {
                p.get("category")
                    .and_then(|c| c.as_str())
                    .map(|c| c == "AI Agents" || c == "Decentralized AI")
                    .unwrap_or(false)
            })
            .filter_map(parse_defillama_protocol)
            .collect();

        Ok((listings, status))
    })
    .await;

    // This source's own count log, kept after the timed_fetch extraction —
    // DefiLlama entries are platform CANDIDATES rather than bounties, so they
    // are dropped from the public response by the reward filter and this line
    // is the only visibility into how many were actually ingested.
    tracing::info!(
        source = "defillama-ai",
        count = result.listings.len(),
        "fetched DefiLlama AI agent platforms"
    );
    result
}

fn parse_defillama_protocol(p: &serde_json::Value) -> Option<RawListing> {
    let slug = str_field(p, "slug")?;
    let name = str_field(p, "name").unwrap_or_else(|| slug.clone());
    let category = str_field(p, "category").unwrap_or_default();

    // listedAt is Unix epoch seconds (e.g., 1668170565). Some entries omit it.
    let listed_secs = p.get("listedAt").and_then(|v| v.as_i64()).unwrap_or(0);
    let posted_at = if listed_secs > 0 {
        chrono::DateTime::<Utc>::from_timestamp(listed_secs, 0).unwrap_or_else(Utc::now)
    } else {
        Utc::now()
    };

    let raw_url = str_field(p, "url").unwrap_or_default();
    let project_url = if raw_url.is_empty() {
        format!("https://defillama.com/protocol/{slug}")
    } else {
        raw_url
    };

    let primary_chain = str_field(p, "chain").unwrap_or_else(|| "multi".to_string());
    let chains: Vec<String> = p
        .get("chains")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let twitter = str_field(p, "twitter").unwrap_or_default();
    let raw_description = str_field(p, "description").unwrap_or_default();

    // Combine description + twitter + DefiLlama URL so the survey doc has
    // enough context to triage the platform without re-querying.
    let mut description = raw_description.chars().take(380).collect::<String>();
    if description.is_empty() {
        description = format!("{name} (no description)");
    }
    if !twitter.is_empty() {
        description.push_str(&format!(" • twitter: @{twitter}"));
    }
    description.push_str(&format!(
        " • defillama: https://defillama.com/protocol/{slug}"
    ));

    let mut tags = vec![
        "meta-discovery".to_string(),
        "defillama".to_string(),
        category.to_lowercase().replace(' ', "-"),
    ];
    for c in chains.iter().take(8) {
        tags.push(c.to_lowercase());
    }

    Some(RawListing {
        source: "defillama-ai".to_string(),
        source_id: slug.clone(),
        source_url: project_url,
        title: name,
        description,
        // "platform-candidate" causes the reward filter to drop these from
        // the public listings response while still persisting to Firestore.
        // Future work: separate /internal/listings/platforms endpoint.
        category: "platform-candidate".to_string(),
        tags,
        reward_amount: "0".to_string(),
        reward_token: "N/A".to_string(),
        reward_chain: primary_chain.to_lowercase(),
        // None deliberately — the reward filter drops these as expected.
        reward_usd_estimate: None,
        payment_model: "discovery".to_string(),
        escrow: false,
        posted_at,
        deadline: None,
    })
}

/// Fetch open bounties from Bountycaster (Base / USDC, Farcaster-native).
pub async fn fetch_bountycaster(client: &reqwest::Client) -> FetchResult {
    timed_fetch("bountycaster", async {
        let res = client
            .get("https://www.bountycaster.xyz/api/v1/bounties/open")
            .send()
            .await?;

        let status = res.status().as_u16();
        if !res.status().is_success() {
            tracing::warn!(source = "bountycaster", status, "non-success response");
            return Ok::<(Vec<RawListing>, u16), reqwest::Error>((vec![], status));
        }

        let data: serde_json::Value = res.json().await?;
        let bounties = if data.is_array() {
            data.as_array().cloned().unwrap_or_default()
        } else {
            data.get("bounties")
                .and_then(|b| b.as_array())
                .cloned()
                .unwrap_or_default()
        };

        let listings: Vec<RawListing> = bounties
            .iter()
            .take(20)
            .filter_map(parse_bountycaster)
            .collect();

        Ok((listings, status))
    })
    .await
}

fn parse_bountycaster(b: &serde_json::Value) -> Option<RawListing> {
    let uid = str_field(b, "uid")?;
    let reward = b.get("reward_summary")?;
    if reward.is_null() {
        return None; // No reward = not a real bounty
    }

    let unit_amount = str_field(reward, "unit_amount").unwrap_or_else(|| "0".to_string());
    let token_symbol = reward
        .get("token")
        .and_then(|t| str_field(t, "symbol"))
        .unwrap_or_else(|| "USDC".to_string());
    let usd_value = reward
        .get("usd_value")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok())
        .or_else(|| reward.get("usd_value").and_then(|v| v.as_f64()));

    let hash = b
        .get("platform")
        .and_then(|p| str_field(p, "hash"))
        .unwrap_or_else(|| uid.clone());

    Some(RawListing {
        source: "bountycaster".to_string(),
        source_id: uid,
        source_url: format!("https://www.bountycaster.xyz/bounty/{hash}"),
        title: str_field(b, "title").unwrap_or_else(|| "Untitled".to_string()),
        description: str_field(b, "summary_text")
            .unwrap_or_default()
            .chars()
            .take(500)
            .collect(),
        category: "social".to_string(),
        tags: b
            .get("tag_slugs")
            .and_then(|t| t.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        reward_amount: unit_amount,
        reward_token: token_symbol,
        reward_chain: "base".to_string(),
        reward_usd_estimate: usd_value,
        payment_model: "fixed".to_string(),
        escrow: false,
        posted_at: parse_datetime(b.get("created_at")).unwrap_or_else(Utc::now),
        deadline: parse_datetime(b.get("expiration_date")),
    })
}

/// Fetch open tasks from 0xWork (Base mainnet, on-chain USDC escrow).
///
/// 0xWork is agent-native (wallet auth, no KYC) and payment-provable (on-chain
/// USDC payouts via TaskPoolV4). Claiming requires staking ~10% of the bounty
/// in $AXOBOTL — surfaced in each listing's description so agents can price
/// that friction before navigating out. External source: agents claim
/// off-platform via `source_url` (no in-MCP deep integration).
pub async fn fetch_0xwork(client: &reqwest::Client) -> FetchResult {
    timed_fetch("0xwork", async {
        let res = client.get("https://api.0xwork.org/tasks").send().await?;

        let status = res.status().as_u16();
        if !res.status().is_success() {
            tracing::warn!(source = "0xwork", status, "non-success response");
            return Ok::<(Vec<RawListing>, u16), reqwest::Error>((vec![], status));
        }

        let data: serde_json::Value = res.json().await?;
        let tasks = if data.is_array() {
            data.as_array().cloned().unwrap_or_default()
        } else {
            data.get("tasks")
                .and_then(|t| t.as_array())
                .cloned()
                .unwrap_or_default()
        };

        let listings: Vec<RawListing> = tasks.iter().take(20).filter_map(parse_0xwork).collect();

        Ok((listings, status))
    })
    .await
}

/// Parse one 0xWork task object into a `RawListing`. Only `Open` tasks are
/// kept (the API also returns claimed/completed ones). The $AXOBOTL claim-stake
/// requirement is appended to the description so agents see the cost up front.
fn parse_0xwork(t: &serde_json::Value) -> Option<RawListing> {
    let id = t.get("id").and_then(|v| v.as_u64())?;

    let status = str_field(t, "status").unwrap_or_default();
    if !status.eq_ignore_ascii_case("open") {
        return None; // only surface claimable (open) tasks
    }

    let bounty = str_field(t, "bounty_amount").unwrap_or_else(|| "0".to_string());
    let usd = bounty.parse::<f64>().ok();

    let raw_desc: String = str_field(t, "description")
        .unwrap_or_default()
        .chars()
        .take(400)
        .collect();
    // Synthesize a title from the first sentence/line of the description.
    let title: String = raw_desc
        .split(['.', '\n'])
        .next()
        .unwrap_or("")
        .trim()
        .chars()
        .take(80)
        .collect();
    let title = if title.is_empty() {
        format!("0xWork task #{id}")
    } else {
        title
    };

    let category = str_field(t, "category")
        .map(|c| c.to_lowercase())
        .unwrap_or_else(|| "general".to_string());

    let description = format!(
        "{raw_desc} [0xWork: on-chain USDC escrow on Base; claiming requires staking ~10% of the bounty in $AXOBOTL.]"
    );

    Some(RawListing {
        source: "0xwork".to_string(),
        source_id: id.to_string(),
        // SPA route on the 0xWork board; if it 404s, fall back to the board root.
        source_url: format!("https://www.0xwork.org/tasks/{id}"),
        title,
        description,
        category,
        tags: vec!["0xwork".to_string()],
        reward_amount: bounty,
        reward_token: "USDC".to_string(),
        reward_chain: "base".to_string(),
        reward_usd_estimate: usd,
        payment_model: "fixed".to_string(),
        escrow: true,
        posted_at: parse_naive_datetime(t.get("created_at")).unwrap_or_else(Utc::now),
        deadline: None,
    })
}

// -- Helpers --

fn str_field(v: &serde_json::Value, key: &str) -> Option<String> {
    v.get(key).and_then(|val| {
        if val.is_string() {
            val.as_str().map(String::from)
        } else if !val.is_null() {
            Some(val.to_string().trim_matches('"').to_string())
        } else {
            None
        }
    })
}

fn parse_datetime(val: Option<&serde_json::Value>) -> Option<DateTime<Utc>> {
    val.and_then(|v| v.as_str())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
}

/// Parse 0xWork's zone-less `"%Y-%m-%d %H:%M:%S"` timestamps as UTC.
fn parse_naive_datetime(val: Option<&serde_json::Value>) -> Option<DateTime<Utc>> {
    val.and_then(|v| v.as_str())
        .and_then(|s| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").ok())
        .map(|ndt| ndt.and_utc())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_0xwork_open_task() {
        let json = serde_json::json!({
            "id": 391,
            "description": "Get @jessepollak to follow @Inner_Axiom on X.",
            "category": "Social",
            "bounty_amount": "50",
            "status": "Open",
            "created_at": "2026-03-30 23:02:24",
            "poster_address": "0xabc"
        });
        let listing = parse_0xwork(&json).expect("open task should parse");
        assert_eq!(listing.source, "0xwork");
        assert_eq!(listing.source_id, "391");
        assert_eq!(listing.source_url, "https://www.0xwork.org/tasks/391");
        assert_eq!(
            listing.title,
            "Get @jessepollak to follow @Inner_Axiom on X"
        );
        assert_eq!(listing.reward_amount, "50");
        assert_eq!(listing.reward_token, "USDC");
        assert_eq!(listing.reward_chain, "base");
        assert_eq!(listing.reward_usd_estimate, Some(50.0));
        assert!(listing.escrow);
        assert!(listing.description.contains("AXOBOTL"));
        assert_eq!(listing.posted_at.to_rfc3339(), "2026-03-30T23:02:24+00:00");
    }

    #[test]
    fn parse_0xwork_skips_non_open_tasks() {
        let json = serde_json::json!({
            "id": 12,
            "description": "already done",
            "bounty_amount": "10",
            "status": "Completed",
            "created_at": "2026-03-30 23:02:24"
        });
        assert!(parse_0xwork(&json).is_none());
    }

    #[test]
    fn parse_bountycaster_with_reward() {
        let json = serde_json::json!({
            "uid": "abc123",
            "title": "Test bounty",
            "summary_text": "Do something useful for the community",
            "created_at": "2026-04-01T00:00:00Z",
            "expiration_date": "2026-04-15T00:00:00Z",
            "platform": { "hash": "0xabc" },
            "reward_summary": {
                "unit_amount": "5",
                "token": { "symbol": "USDC" },
                "usd_value": "5.00"
            },
            "tag_slugs": ["dev"]
        });

        let listing = parse_bountycaster(&json).expect("should parse");
        assert_eq!(listing.source, "bountycaster");
        assert_eq!(listing.source_id, "abc123");
        assert_eq!(listing.reward_amount, "5");
        assert_eq!(listing.reward_token, "USDC");
        assert!((listing.reward_usd_estimate.unwrap() - 5.0).abs() < f64::EPSILON);
        assert_eq!(listing.title, "Test bounty");
    }

    #[test]
    fn parse_bountycaster_without_reward_returns_none() {
        let json = serde_json::json!({
            "uid": "abc123",
            "title": "No reward post",
            "summary_text": "Just chatting",
            "created_at": "2026-04-01T00:00:00Z",
            "reward_summary": null
        });

        assert!(parse_bountycaster(&json).is_none());
    }

    #[test]
    fn parse_shillbot_task_happy_path() {
        let json = serde_json::json!({
            "task_id": "campaign-uuid:task-uuid",
            "campaign_id": "campaign-uuid",
            "campaign_topic": "Play a round of coordination.game",
            "state": "open",
            "platform": 5,
            "created_at": "2026-04-07T08:20:57.959927902Z",
            "estimated_payment_lamports": 20_000_000u64,
            "brief": {
                "topic": "Play a round of coordination.game",
                "brand_voice": "Direct incentive.",
                "cta": "Play one round at coordination.game",
                "utm_link": "https://coordination.game",
                "blocklist": [],
                "examples": []
            }
        });

        let listing = parse_shillbot_task(&json).expect("should parse");
        assert_eq!(listing.source, "shillbot");
        assert_eq!(listing.source_id, "campaign-uuid:task-uuid");
        assert_eq!(listing.reward_token, "SOL");
        assert_eq!(listing.reward_chain, "solana");
        assert_eq!(listing.reward_amount, "0.0200");
        // Game-play (platform=5) description must surface the game-play action,
        // not the old "youtube short" wording.
        assert!(listing.description.contains("coordination.game"));
        assert!(listing.tags.contains(&"game-play".to_string()));
        assert_eq!(listing.source_url, "https://shillbot.org/tasks");
        assert!(listing.escrow);
    }

    #[test]
    fn parse_shillbot_task_drops_unpriced() {
        let json = serde_json::json!({
            "task_id": "c:t",
            "campaign_topic": "topic",
            "state": "open",
            "platform": 3,
            "created_at": "2026-04-07T08:20:57Z",
            "estimated_payment_lamports": 0u64,
            "brief": {}
        });
        assert!(parse_shillbot_task(&json).is_none());
    }

    #[test]
    fn parse_shillbot_task_drops_missing_payment() {
        let json = serde_json::json!({
            "task_id": "c:t",
            "campaign_topic": "topic",
            "state": "open",
            "platform": 3,
            "created_at": "2026-04-07T08:20:57Z",
            "brief": {}
        });
        assert!(parse_shillbot_task(&json).is_none());
    }

    #[test]
    fn shillbot_platform_label_known_and_unknown() {
        // Matches production discriminants in the orchestrator.
        assert_eq!(shillbot_platform_label(0), "youtube");
        assert_eq!(shillbot_platform_label(3), "twitter");
        assert_eq!(shillbot_platform_label(4), "referral");
        assert_eq!(shillbot_platform_label(5), "game-play");
        assert_eq!(shillbot_platform_label(9), "website");
        assert_eq!(shillbot_platform_label(99), "other");
    }

    #[test]
    fn shillbot_platform_action_per_platform() {
        assert!(shillbot_platform_action(0).contains("youtube"));
        assert!(shillbot_platform_action(3).contains("X"));
        assert!(shillbot_platform_action(4).contains("campaign"));
        assert!(shillbot_platform_action(5).contains("coordination.game"));
        assert!(shillbot_platform_action(9).contains("swarm.tips"));
        assert_eq!(shillbot_platform_action(99), "Complete a shillbot task.");
    }

    #[test]
    fn parse_botbounty_basic() {
        let json = serde_json::json!({
            "id": "xyz",
            "title": "Fix a bug",
            "description": "There is a bug in the smart contract that needs fixing",
            "amount": "0.5",
            "created_at": "2026-04-01T00:00:00Z"
        });

        let listing = parse_botbounty(&json).expect("should parse");
        assert_eq!(listing.reward_token, "ETH");
        assert!((listing.reward_usd_estimate.unwrap() - 1000.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_defillama_protocol_basic() {
        let json = serde_json::json!({
            "id": "1234",
            "name": "Giza",
            "slug": "giza",
            "category": "AI Agents",
            "url": "https://www.gizatech.xyz/",
            "chain": "Multi-Chain",
            "chains": ["Base", "Arbitrum"],
            "tvl": 16795479.88,
            "description": "Giza is the infrastructure powering autonomous financial markets...",
            "twitter": "gizatechxyz",
            "listedAt": 1700000000i64
        });

        let listing = parse_defillama_protocol(&json).expect("should parse");
        assert_eq!(listing.source, "defillama-ai");
        assert_eq!(listing.source_id, "giza");
        assert_eq!(listing.title, "Giza");
        assert_eq!(listing.source_url, "https://www.gizatech.xyz/");
        assert_eq!(listing.category, "platform-candidate");
        assert_eq!(listing.reward_token, "N/A");
        assert!(listing.reward_usd_estimate.is_none());
        assert!(listing.tags.contains(&"meta-discovery".to_string()));
        assert!(listing.tags.contains(&"defillama".to_string()));
        assert!(listing.tags.contains(&"ai-agents".to_string()));
        assert!(listing.tags.contains(&"base".to_string()));
        assert!(listing.description.contains("@gizatechxyz"));
        assert!(listing.description.contains("defillama.com/protocol/giza"));
    }

    #[test]
    fn parse_defillama_protocol_decentralized_ai_category() {
        let json = serde_json::json!({
            "name": "FLock.io",
            "slug": "flock.io",
            "category": "Decentralized AI",
            "url": "https://www.flock.io/",
            "chain": "Base",
            "chains": ["Base"],
            "description": "FLock.io is a private AI training platform.",
            "twitter": "flock_io"
        });

        let listing = parse_defillama_protocol(&json).expect("should parse");
        assert_eq!(listing.source_id, "flock.io");
        assert!(listing.tags.contains(&"decentralized-ai".to_string()));
    }

    #[test]
    fn parse_defillama_protocol_falls_back_when_url_empty() {
        let json = serde_json::json!({
            "name": "Yoko",
            "slug": "yoko",
            "category": "AI Agents",
            "url": "",
            "chain": "Sonic",
            "chains": ["Sonic"],
            "description": "Yoko is a no-code platform for launching AI Agents"
        });

        let listing = parse_defillama_protocol(&json).expect("should parse");
        assert_eq!(listing.source_url, "https://defillama.com/protocol/yoko");
    }

    #[test]
    fn parse_defillama_protocol_handles_null_chain_and_missing_listedat() {
        let json = serde_json::json!({
            "name": "Virtuals Protocol",
            "slug": "virtuals-protocol",
            "category": "AI Agents",
            "url": "https://app.virtuals.io/",
            "chain": null,
            "chains": [],
            "description": "Society of AI Agents base"
        });

        let listing = parse_defillama_protocol(&json).expect("should parse");
        assert_eq!(listing.reward_chain, "multi");
        // Defaults posted_at to now when listedAt missing — just check it parses.
        assert_eq!(listing.title, "Virtuals Protocol");
    }

    #[test]
    fn parse_defillama_protocol_drops_missing_slug() {
        let json = serde_json::json!({
            "name": "no-slug-protocol",
            "category": "AI Agents",
            "description": "Should be dropped"
        });
        assert!(parse_defillama_protocol(&json).is_none());
    }

    #[test]
    fn parse_defillama_protocol_uses_placeholder_when_description_empty() {
        let json = serde_json::json!({
            "name": "Quietproto",
            "slug": "quietproto",
            "category": "AI Agents",
            "description": ""
        });
        let listing = parse_defillama_protocol(&json).expect("should parse");
        assert!(listing.description.contains("Quietproto"));
        assert!(listing.description.contains("(no description)"));
    }
}
