//! Layer 2 classifier — calls Grok to tag MCP servers Layer 1 wasn't confident
//! about. Reads the system prompt from `prompts/discovery_classify.txt` and
//! POSTs one server at a time to `https://api.x.ai/v1/chat/completions`. The
//! result is parsed into a [`Layer2Classification`] and written back into the
//! same `mcp_servers/{slug}` Firestore doc as a sibling field of Layer 1.
//!
//! Why one-server-per-call instead of batched: a single JSON-array response
//! is harder to parse defensively (one bad entry corrupts the whole batch),
//! and Grok-3-mini's per-call latency at this prompt size is ~1-2s, so we
//! can do 50-100 servers per minute serially. The cost driver is input
//! tokens (the system prompt), not request count — batching would help cost
//! but not enough to justify the parsing fragility.
//!
//! Cost math: ~$0.0003 per call × 200 servers/cycle = ~$0.06/cycle.
//! Total Layer 2 sweep over the unconfident remainder (~1500 servers, capped
//! at 200/cycle) = 8 cycles × $0.06 = ~$0.50. Well under the plan's $5 ceiling.

use crate::discovery::models::{
    CashFlowDirection, Category, EnrichedServer, Layer2Classification, ValueToSwarm,
};
use anyhow::{Context, Result};
use chrono::Utc;
use serde::Deserialize;

const MODEL: &str = "grok-3-mini";
const API_URL: &str = "https://api.x.ai/v1/chat/completions";
const SYSTEM_PROMPT: &str = include_str!("prompts/discovery_classify.txt");

/// Per-cycle cap on the number of servers Layer 2 will classify in one pass.
/// Bounds budget exposure: at ~$0.0003/call this caps a single run at $0.06.
pub const MAX_SERVERS_PER_CYCLE: usize = 200;

/// Grok API client for Layer 2 classification. Hand-rolled reqwest instead of
/// `async-openai` to avoid pulling a heavy dep just for one endpoint.
pub struct LlmClassifier {
    client: reqwest::Client,
    api_key: String,
}

impl LlmClassifier {
    pub fn new(api_key: String, http: reqwest::Client) -> Self {
        assert!(!api_key.is_empty(), "xAI API key must not be empty");
        Self {
            client: http,
            api_key,
        }
    }

    /// Classify a single server. Returns a [`Layer2Classification`] ready to
    /// be written back to Firestore. The Grok call is fully synchronous from
    /// the caller's perspective — caller is responsible for backoff +
    /// concurrency.
    pub async fn classify_server(&self, server: &EnrichedServer) -> Result<Layer2Classification> {
        let user_prompt = build_user_prompt(server);
        let body = serde_json::json!({
            "model": MODEL,
            "messages": [
                { "role": "system", "content": SYSTEM_PROMPT },
                { "role": "user", "content": user_prompt }
            ],
            "max_tokens": 300,
            "temperature": 0.2
        });

        let resp = self
            .client
            .post(API_URL)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .context("Grok request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Grok returned {status}: {body}");
        }

        let parsed: GrokResponse = resp
            .json()
            .await
            .context("failed to deserialize Grok response envelope")?;

        let raw = parsed
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .context("Grok response had no choices")?;

        parse_layer2_response(&raw, MODEL)
    }
}

/// Build the user prompt body for one server. Pure function — used for tests.
pub fn build_user_prompt(server: &EnrichedServer) -> String {
    let title = server.title.as_deref().unwrap_or("(no title)");
    let desc = server.description.as_deref().unwrap_or("(no description)");
    let repo = server.github_repo.as_deref().unwrap_or("(no repo)");
    let npm = server.npm_package.as_deref().unwrap_or("(no npm package)");
    let endpoint = server.endpoint.as_deref().unwrap_or("(no endpoint)");
    let transport = server.transport.as_deref().unwrap_or("(unknown transport)");
    format!(
        "Server name: {}\n\
         Title: {title}\n\
         Description: {desc}\n\
         GitHub repo: {repo}\n\
         npm package: {npm}\n\
         Endpoint: {endpoint}\n\
         Transport: {transport}\n",
        server.name
    )
}

/// Parse Grok's raw response text into a [`Layer2Classification`]. Tolerates
/// the model wrapping its JSON in code fences or surrounding prose by
/// extracting the first balanced `{...}` substring.
pub fn parse_layer2_response(raw: &str, model: &str) -> Result<Layer2Classification> {
    let json_str = extract_json_object(raw).context("no JSON object found in Grok response")?;
    let parsed: RawVerdict =
        serde_json::from_str(&json_str).context("failed to deserialize Grok JSON")?;

    let confidence = parsed.confidence.clamp(0.0, 1.0);

    Ok(Layer2Classification {
        category: parse_category(&parsed.category),
        cash_flow_direction: parse_cash_flow(&parsed.cash_flow_direction),
        currencies: parsed.currencies.unwrap_or_default(),
        value_to_swarm: parse_value(&parsed.value_to_swarm),
        confidence,
        reasoning: parsed.reasoning.unwrap_or_default(),
        integration_effort: parsed
            .integration_effort
            .unwrap_or_else(|| "unknown".into()),
        classified_at: Utc::now(),
        model: model.to_string(),
    })
}

/// Walk a string and extract the first brace-balanced JSON object. Copied
/// verbatim from `backend/x-bridge/src/grok_replier.rs::extract_json_object` —
/// tracks brace depth, ignores braces inside string literals, handles escapes.
/// We duplicate rather than depend because the two crates are in different
/// repos and the function is small + stable.
fn extract_json_object(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut start: Option<usize> = None;
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut escape = false;

    for (i, &b) in bytes.iter().enumerate() {
        if escape {
            escape = false;
            continue;
        }
        if b == b'\\' && in_string {
            escape = true;
            continue;
        }
        if b == b'"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        if b == b'{' {
            if depth == 0 {
                start = Some(i);
            }
            depth = depth.saturating_add(1);
        } else if b == b'}' {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                if let Some(s_idx) = start {
                    return Some(s[s_idx..=i].to_string());
                }
            }
        }
    }
    None
}

fn parse_category(s: &str) -> Option<Category> {
    match s.to_lowercase().as_str() {
        "bounty" => Some(Category::Bounty),
        "content" => Some(Category::Content),
        "payment" => Some(Category::Payment),
        "infrastructure" => Some(Category::Infrastructure),
        "game" => Some(Category::Game),
        "social" => Some(Category::Social),
        "devtools" => Some(Category::Devtools),
        "data" => Some(Category::Data),
        "other" => Some(Category::Other),
        _ => None,
    }
}

fn parse_cash_flow(s: &str) -> Option<CashFlowDirection> {
    match s.to_lowercase().as_str() {
        "earns_for_agent" => Some(CashFlowDirection::EarnsForAgent),
        "costs_agent" => Some(CashFlowDirection::CostsAgent),
        "neutral" => Some(CashFlowDirection::Neutral),
        _ => None,
    }
}

fn parse_value(s: &str) -> Option<ValueToSwarm> {
    match s.to_lowercase().as_str() {
        "aggregate_listing" => Some(ValueToSwarm::AggregateListing),
        "surface_as_spend" => Some(ValueToSwarm::SurfaceAsSpend),
        "competitor" => Some(ValueToSwarm::Competitor),
        "complement" => Some(ValueToSwarm::Complement),
        "dependency" => Some(ValueToSwarm::Dependency),
        "inspiration" => Some(ValueToSwarm::Inspiration),
        "none" => Some(ValueToSwarm::None),
        _ => None,
    }
}

// -- Wire types for Grok response parsing --

#[derive(Debug, Deserialize)]
struct GrokResponse {
    choices: Vec<GrokChoice>,
}

#[derive(Debug, Deserialize)]
struct GrokChoice {
    message: GrokMessage,
}

#[derive(Debug, Deserialize)]
struct GrokMessage {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawVerdict {
    #[serde(default)]
    category: String,
    #[serde(default)]
    cash_flow_direction: String,
    #[serde(default)]
    currencies: Option<Vec<String>>,
    #[serde(default)]
    value_to_swarm: String,
    #[serde(default)]
    integration_effort: Option<String>,
    #[serde(default)]
    confidence: f32,
    #[serde(default)]
    reasoning: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::models::Layer1Classification;

    fn fake_server(name: &str, desc: &str) -> EnrichedServer {
        EnrichedServer {
            name: name.to_string(),
            title: None,
            description: Some(desc.to_string()),
            endpoint: None,
            transport: None,
            npm_package: None,
            github_repo: None,
            sources: vec!["test".into()],
            source_count: 1,
            upstream_quality_score: None,
            upstream_visitors_estimate: None,
            classification: Layer1Classification {
                category: None,
                cash_flow_direction: None,
                currencies: vec![],
                value_to_swarm: None,
                confident: false,
                matched_signals: vec![],
            },
            layer2_classification: None,
            first_seen_at: Utc::now(),
            last_seen_at: Utc::now(),
        }
    }

    #[test]
    fn build_user_prompt_includes_server_metadata() {
        let s = fake_server("io.example/foo", "An example MCP server");
        let p = build_user_prompt(&s);
        assert!(p.contains("io.example/foo"));
        assert!(p.contains("An example MCP server"));
        assert!(p.contains("(no repo)"));
    }

    #[test]
    fn parse_layer2_response_happy_path() {
        let raw = r#"{
            "category": "bounty",
            "cash_flow_direction": "earns_for_agent",
            "currencies": ["USDC"],
            "value_to_swarm": "aggregate_listing",
            "integration_effort": "trivial",
            "confidence": 0.92,
            "reasoning": "Description explicitly says agents claim bounties paid in USDC."
        }"#;
        let parsed = parse_layer2_response(raw, "grok-3-mini").expect("must parse");
        assert!(matches!(parsed.category, Some(Category::Bounty)));
        assert!(matches!(
            parsed.cash_flow_direction,
            Some(CashFlowDirection::EarnsForAgent)
        ));
        assert_eq!(parsed.currencies, vec!["USDC"]);
        assert!(matches!(
            parsed.value_to_swarm,
            Some(ValueToSwarm::AggregateListing)
        ));
        assert!((parsed.confidence - 0.92).abs() < 0.001);
        assert_eq!(parsed.integration_effort, "trivial");
        assert_eq!(parsed.model, "grok-3-mini");
        assert!(parsed.reasoning.contains("USDC"));
    }

    #[test]
    fn parse_layer2_response_strips_code_fences() {
        let raw = "```json\n{\"category\":\"infrastructure\",\"cash_flow_direction\":\"neutral\",\"currencies\":[\"SOL\"],\"value_to_swarm\":\"dependency\",\"integration_effort\":\"trivial\",\"confidence\":0.9,\"reasoning\":\"RPC primitive\"}\n```";
        let parsed = parse_layer2_response(raw, "grok-3-mini").expect("must parse around fences");
        assert!(matches!(parsed.category, Some(Category::Infrastructure)));
        assert!(matches!(
            parsed.value_to_swarm,
            Some(ValueToSwarm::Dependency)
        ));
    }

    #[test]
    fn parse_layer2_response_clamps_out_of_range_confidence() {
        let raw = r#"{"category":"other","cash_flow_direction":"neutral","currencies":["none"],"value_to_swarm":"none","integration_effort":"high","confidence":1.5,"reasoning":"clamp test"}"#;
        let parsed = parse_layer2_response(raw, "grok-3-mini").expect("must parse");
        assert!((parsed.confidence - 1.0).abs() < 0.001);
    }

    #[test]
    fn parse_layer2_response_defaults_missing_fields() {
        // Bare minimum response — no currencies, no integration_effort, no
        // reasoning. Parser should fill in defaults rather than blow up.
        let raw = r#"{"category":"data","cash_flow_direction":"neutral","value_to_swarm":"none","confidence":0.5}"#;
        let parsed = parse_layer2_response(raw, "grok-3-mini").expect("must parse");
        assert!(matches!(parsed.category, Some(Category::Data)));
        assert!(parsed.currencies.is_empty());
        assert_eq!(parsed.integration_effort, "unknown");
        assert!(parsed.reasoning.is_empty());
    }

    #[test]
    fn parse_layer2_response_unknown_enum_values_become_none() {
        let raw = r#"{"category":"unknown_category","cash_flow_direction":"???","currencies":[],"value_to_swarm":"???","integration_effort":"trivial","confidence":0.5,"reasoning":""}"#;
        let parsed = parse_layer2_response(raw, "grok-3-mini").expect("must parse");
        assert!(parsed.category.is_none());
        assert!(parsed.cash_flow_direction.is_none());
        assert!(parsed.value_to_swarm.is_none());
    }

    #[test]
    fn parse_layer2_response_rejects_garbage() {
        assert!(parse_layer2_response("not json at all", "grok-3-mini").is_err());
    }

    #[test]
    fn extract_json_object_handles_string_with_braces() {
        let raw = r#"prefix {"a": "has {brace} inside", "b": 1} suffix"#;
        let extracted = extract_json_object(raw).expect("should ignore braces in strings");
        assert_eq!(extracted, r#"{"a": "has {brace} inside", "b": 1}"#);
    }

    #[test]
    fn extract_json_object_handles_nested_objects() {
        let raw = r#"{"outer": {"inner": {"deep": 1}}}"#;
        let extracted = extract_json_object(raw).expect("should match outer object");
        assert_eq!(extracted, raw);
    }
}
