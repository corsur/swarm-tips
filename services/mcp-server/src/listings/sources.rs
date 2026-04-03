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
            .filter_map(|b| parse_clawtask(b))
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
            .filter_map(|b| parse_botbounty(b))
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
            .filter_map(|b| parse_bountycaster(b))
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
}
