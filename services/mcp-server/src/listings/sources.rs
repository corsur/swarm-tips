use crate::listings::models::{HealthCheck, RawListing};
use chrono::{DateTime, Utc};
use std::time::Instant;

/// Result of fetching from one source: listings + health check data.
pub struct FetchResult {
    pub source: String,
    pub listings: Vec<RawListing>,
    pub health: HealthCheck,
}

/// Fetch open bounties from ClawTasks (Base / USDC).
pub async fn fetch_clawtasks(client: &reqwest::Client) -> FetchResult {
    let source = "clawtasks".to_string();
    let start = Instant::now();

    let result = async {
        let res = client
            .get("https://clawtasks.com/api/bounties?status=open&sort=recent")
            .send()
            .await?;

        let status = res.status().as_u16();
        if !res.status().is_success() {
            tracing::warn!(source = "clawtasks", status, "non-success response");
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
            .filter_map(parse_clawtask)
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
            tracing::warn!(source = "clawtasks", error = %e, "fetch failed");
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

fn parse_clawtask(b: &serde_json::Value) -> Option<RawListing> {
    let id = b.get("id")?.to_string().trim_matches('"').to_string();
    let amount_str = b
        .get("amount")
        .map(|v| v.to_string().trim_matches('"').to_string())
        .unwrap_or_else(|| "0".to_string());
    let amount: f64 = amount_str.parse().unwrap_or(0.0);

    Some(RawListing {
        source: "clawtasks".to_string(),
        source_id: id.clone(),
        source_url: format!("https://clawtasks.com/bounties/{id}"),
        title: str_field(b, "title").unwrap_or_else(|| "Untitled".to_string()),
        description: str_field(b, "description")
            .unwrap_or_default()
            .chars()
            .take(500)
            .collect(),
        category: "code".to_string(),
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
        reward_token: "USDC".to_string(),
        reward_chain: "base".to_string(),
        reward_usd_estimate: Some(amount), // USDC = $1
        payment_model: "fixed".to_string(),
        escrow: true,
        posted_at: parse_datetime(b.get("created_at")).unwrap_or_else(Utc::now),
        deadline: b
            .get("deadline_hours")
            .and_then(|h| h.as_f64())
            .and_then(|hours| {
                let millis = (hours * 3_600_000.0) as i64;
                Utc::now().checked_add_signed(chrono::Duration::milliseconds(millis))
            }),
    })
}

/// Fetch open bounties from BotBounty (Base / ETH).
pub async fn fetch_botbounty(client: &reqwest::Client) -> FetchResult {
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
pub async fn fetch_moltlaunch(client: &reqwest::Client) -> FetchResult {
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
            return Ok::<(Vec<RawListing>, u16), reqwest::Error>((vec![], status));
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

/// Fetch open bounties from Bountycaster (Base / USDC, Farcaster-native).
pub async fn fetch_bountycaster(client: &reqwest::Client) -> FetchResult {
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
    fn parse_clawtask_basic() {
        let json = serde_json::json!({
            "id": 42,
            "title": "Build a bot",
            "description": "Create a trading bot that works with DEX APIs",
            "amount": "100",
            "created_at": "2026-04-01T00:00:00Z",
            "tags": ["rust", "solana"]
        });

        let listing = parse_clawtask(&json).expect("should parse");
        assert_eq!(listing.source_id, "42");
        assert_eq!(listing.reward_amount, "100");
        assert!((listing.reward_usd_estimate.unwrap() - 100.0).abs() < f64::EPSILON);
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
