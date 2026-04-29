use crate::listings::models::{HealthCheck, RawListing};
use chrono::{DateTime, Utc};
use std::time::Instant;

/// Result of fetching from one source: listings + health check data.
pub struct FetchResult {
    pub source: String,
    pub listings: Vec<RawListing>,
    pub health: HealthCheck,
}

/// Fetch open bounties from BotBounty (Base / ETH).
pub async fn fetch_botbounty(client: &rquest::Client) -> FetchResult {
    let source = "botbounty".to_string();
    let start = Instant::now();

    let result = async {
        let res = client
            .get("https://botbounty-production.up.railway.app/api/agent/bounties")
            .header("Content-Type", "application/json")
            .send()
            .await?;

        let status = res.status().as_u16();
        if !res.status().is_success() {
            tracing::warn!(source = "botbounty", status, "non-success response");
            return Ok::<(Vec<RawListing>, u16), rquest::Error>((vec![], status));
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
    }
    .await;

    let elapsed_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok((listings, status)) => {
            let count = listings.len() as u32;
            FetchResult {
                source,
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
            tracing::warn!(source = "botbounty", error = %e, "fetch failed");
            FetchResult {
                source,
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

/// Hardcoded ETH price fallback for USD estimation.
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

/// Fetch open agent gigs from Moltlaunch (Base / ETH).
///
/// Moltlaunch is an agent marketplace on Base where AI agents publish "gigs"
/// (priced services they offer to perform). Clients hire them, ETH is escrowed
/// and released on delivery. From a Swarm Tips agent operator's perspective,
/// each gig is an existing agent doing earnable work that can be replicated.
/// We surface the most recent priced gigs as listings under EARN.
pub async fn fetch_moltlaunch(client: &rquest::Client) -> FetchResult {
    let source = "moltlaunch".to_string();
    let start = Instant::now();

    let result = async {
        let res = client
            .get("https://api.moltlaunch.com/api/gigs")
            .send()
            .await?;

        let status = res.status().as_u16();
        if !res.status().is_success() {
            tracing::warn!(source = "moltlaunch", status, "non-success response");
            return Ok::<(Vec<RawListing>, u16), rquest::Error>((vec![], status));
        }

        let data: serde_json::Value = res.json().await?;
        let gigs = data
            .get("gigs")
            .and_then(|g| g.as_array())
            .cloned()
            .unwrap_or_default();

        let listings: Vec<RawListing> = gigs
            .iter()
            .filter(|g| g.get("active").and_then(|v| v.as_bool()).unwrap_or(false))
            .take(20)
            .filter_map(parse_moltlaunch_gig)
            .collect();

        Ok((listings, status))
    }
    .await;

    let elapsed_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok((listings, status)) => {
            let count = listings.len() as u32;
            FetchResult {
                source,
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
            tracing::warn!(source = "moltlaunch", error = %e, "fetch failed");
            FetchResult {
                source,
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

/// Convert a Moltlaunch wei priceWei string to an ETH float.
/// Returns 0.0 on parse failure (caller decides whether to drop the listing).
fn moltlaunch_wei_to_eth(price_wei_str: &str) -> f64 {
    // priceWei comes back as a decimal string like "600000000000000".
    // 1 ETH = 1e18 wei. Use f64 division — for the price ranges we see
    // (< 1 ETH) precision loss is well below display significance.
    price_wei_str.parse::<f64>().unwrap_or(0.0) / 1e18
}

fn parse_moltlaunch_gig(g: &serde_json::Value) -> Option<RawListing> {
    let id = str_field(g, "id")?;
    let title = str_field(g, "title")?;
    let description = str_field(g, "description").unwrap_or_default();
    let category = str_field(g, "category").unwrap_or_else(|| "agent-services".to_string());
    let price_wei_str = str_field(g, "priceWei").unwrap_or_else(|| "0".to_string());
    let eth_amount = moltlaunch_wei_to_eth(&price_wei_str);

    // Drop unpriced gigs — they aren't real earning opportunities.
    if eth_amount <= 0.0 {
        return None;
    }

    // createdAt is Unix epoch milliseconds (e.g., 1775547934609).
    let created_ms = g.get("createdAt").and_then(|v| v.as_i64()).unwrap_or(0);
    let posted_at =
        chrono::DateTime::<Utc>::from_timestamp_millis(created_ms).unwrap_or_else(Utc::now);

    let agent_name = g
        .get("agent")
        .and_then(|a| a.get("name"))
        .and_then(|n| n.as_str())
        .map(String::from)
        .unwrap_or_else(|| "unknown agent".to_string());

    let delivery = str_field(g, "deliveryTime").unwrap_or_else(|| "TBD".to_string());

    // Reformat the description to include the offering agent + delivery window
    // so the surface card on swarm.tips conveys "this is an existing earning loop".
    let description = format!(
        "{} (offered by {} • delivery in {})",
        description.chars().take(400).collect::<String>(),
        agent_name,
        delivery
    );

    Some(RawListing {
        source: "moltlaunch".to_string(),
        source_id: id.clone(),
        source_url: format!("https://moltlaunch.com/agents/{id}"),
        title,
        description,
        category,
        tags: vec!["base".to_string(), "agent-marketplace".to_string()],
        reward_amount: format!("{eth_amount:.4}"),
        reward_token: "ETH".to_string(),
        reward_chain: "base".to_string(),
        reward_usd_estimate: Some(eth_amount * ETH_PRICE_USD),
        payment_model: "fixed".to_string(),
        escrow: true,
        posted_at,
        deadline: None,
    })
}

/// Fetch open content tasks from the Shillbot orchestrator (Solana / SOL).
///
/// Shillbot is one of swarm.tips' own verticals — the AI-agent task
/// marketplace where clients pay agents in escrowed SOL to create short-form
/// content. Surfacing live Shillbot tasks under EARN closes the loop: the
/// landing page promises agent earning opportunities, and the DAO's own
/// marketplace is the most agent-native one we have. Without this source the
/// frontend never auto-picked up new Shillbot campaigns.
pub async fn fetch_shillbot(client: &rquest::Client) -> FetchResult {
    let source = "shillbot".to_string();
    let start = Instant::now();

    let result = async {
        let res = client.get("https://api.shillbot.org/tasks").send().await?;

        let status = res.status().as_u16();
        if !res.status().is_success() {
            tracing::warn!(source = "shillbot", status, "non-success response");
            return Ok::<(Vec<RawListing>, u16), rquest::Error>((vec![], status));
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
            .take(20)
            .filter_map(parse_shillbot_task)
            .collect();

        Ok((listings, status))
    }
    .await;

    let elapsed_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok((listings, status)) => {
            let count = listings.len() as u32;
            FetchResult {
                source,
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
            tracing::warn!(source = "shillbot", error = %e, "fetch failed");
            FetchResult {
                source,
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

/// Map a Shillbot platform enum integer to a human-readable label.
/// Must match the discriminants actually used in production by the
/// orchestrator/verifier (which diverged from the original PlatformType
/// *names* in the shared crate — only the discriminant numbers are the
/// contract). Keep this table in sync with
/// `coordination-app/backend/shillbot-orchestrator/src/services/campaign_service.rs::default_cohort_for_platform`.
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
    // enough context for an agent to decide whether to pursue.
    let action = shillbot_platform_action(platform_int);
    let description = format!("{action} {voice} CTA: {cta}")
        .chars()
        .take(500)
        .collect::<String>();

    // task_id is already namespaced as "{campaign_id}:{task_uuid}"; strip the
    // campaign half for a cleaner public URL.
    let url_id = task_id.split(':').next_back().unwrap_or(&task_id);

    Some(RawListing {
        source: "shillbot".to_string(),
        source_id: task_id.clone(),
        source_url: format!("https://shillbot.org/tasks/{url_id}"),
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
/// can decide whether to integrate it as a real listings source the way
/// Moltlaunch was added by hand.
pub async fn fetch_defillama_ai_agents(client: &rquest::Client) -> FetchResult {
    let source = "defillama-ai".to_string();
    let start = Instant::now();

    let result = async {
        let res = client
            .get("https://api.llama.fi/protocols")
            .header(
                rquest::header::USER_AGENT,
                "SwarmTipsDiscovery/0.1 (+https://swarm.tips)",
            )
            .send()
            .await?;

        let status = res.status().as_u16();
        if !res.status().is_success() {
            tracing::warn!(source = "defillama-ai", status, "non-success response");
            return Ok::<(Vec<RawListing>, u16), rquest::Error>((vec![], status));
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
    }
    .await;

    let elapsed_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok((listings, status)) => {
            let count = listings.len() as u32;
            tracing::info!(
                source = "defillama-ai",
                count,
                "fetched DefiLlama AI agent platforms"
            );
            FetchResult {
                source,
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
            tracing::warn!(source = "defillama-ai", error = %e, "fetch failed");
            FetchResult {
                source,
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
pub async fn fetch_bountycaster(client: &rquest::Client) -> FetchResult {
    let source = "bountycaster".to_string();
    let start = Instant::now();

    let result = async {
        let res = client
            .get("https://www.bountycaster.xyz/api/v1/bounties/open")
            .send()
            .await?;

        let status = res.status().as_u16();
        if !res.status().is_success() {
            tracing::warn!(source = "bountycaster", status, "non-success response");
            return Ok::<(Vec<RawListing>, u16), rquest::Error>((vec![], status));
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
    }
    .await;

    let elapsed_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok((listings, status)) => {
            let count = listings.len() as u32;
            FetchResult {
                source,
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
            tracing::warn!(source = "bountycaster", error = %e, "fetch failed");
            FetchResult {
                source,
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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(listing.source_url.ends_with("/task-uuid"));
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
    fn moltlaunch_wei_to_eth_handles_typical_prices() {
        // 0.0006 ETH (median gig price)
        assert!((moltlaunch_wei_to_eth("600000000000000") - 0.0006).abs() < 1e-9);
        // 0.001 ETH
        assert!((moltlaunch_wei_to_eth("1000000000000000") - 0.001).abs() < 1e-9);
        // 1 ETH (high end)
        assert!((moltlaunch_wei_to_eth("1000000000000000000") - 1.0).abs() < 1e-9);
    }

    #[test]
    fn moltlaunch_wei_to_eth_handles_garbage() {
        assert_eq!(moltlaunch_wei_to_eth(""), 0.0);
        assert_eq!(moltlaunch_wei_to_eth("not-a-number"), 0.0);
        assert_eq!(moltlaunch_wei_to_eth("0"), 0.0);
    }

    #[test]
    fn parse_moltlaunch_gig_basic() {
        let json = serde_json::json!({
            "id": "11ae39c6-3b40-483f-a985-3c51d5db961c",
            "agentId": "38849",
            "title": "Quick React bug rescue",
            "description": "Fast bug isolation and fix for React/Next.js production issues.",
            "priceWei": "600000000000000",
            "deliveryTime": "12h",
            "category": "bug-fix",
            "active": true,
            "createdAt": 1775547934609i64,
            "agent": {
                "id": "0x97c1",
                "name": "NI-KA"
            }
        });

        let listing = parse_moltlaunch_gig(&json).expect("should parse");
        assert_eq!(listing.source, "moltlaunch");
        assert_eq!(listing.source_id, "11ae39c6-3b40-483f-a985-3c51d5db961c");
        assert_eq!(listing.title, "Quick React bug rescue");
        assert_eq!(listing.reward_token, "ETH");
        assert_eq!(listing.reward_chain, "base");
        assert_eq!(listing.reward_amount, "0.0006");
        assert_eq!(listing.category, "bug-fix");
        assert!(listing.description.contains("NI-KA"));
        assert!(listing.description.contains("12h"));
        assert!(listing.escrow);
        assert!(listing
            .source_url
            .starts_with("https://moltlaunch.com/agents/"));
    }

    #[test]
    fn parse_moltlaunch_gig_drops_unpriced() {
        let json = serde_json::json!({
            "id": "free-gig",
            "title": "Free thing",
            "priceWei": "0",
            "active": true,
            "createdAt": 1775547934609i64,
            "agent": { "name": "ghost" }
        });
        assert!(parse_moltlaunch_gig(&json).is_none());
    }

    #[test]
    fn parse_moltlaunch_gig_drops_missing_title() {
        let json = serde_json::json!({
            "id": "no-title",
            "priceWei": "1000000000000000",
            "active": true,
            "createdAt": 1775547934609i64,
            "agent": { "name": "ghost" }
        });
        assert!(parse_moltlaunch_gig(&json).is_none());
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

    #[test]
    fn parse_moltlaunch_gig_handles_missing_agent_name() {
        let json = serde_json::json!({
            "id": "agentless",
            "title": "Some service",
            "priceWei": "1000000000000000",
            "active": true,
            "createdAt": 1775547934609i64,
            "agent": {}
        });
        let listing = parse_moltlaunch_gig(&json).expect("should parse without agent name");
        assert!(listing.description.contains("unknown agent"));
    }
}
