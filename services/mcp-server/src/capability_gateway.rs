//! Compact discovery and same-router dispatch for capabilities hidden from the
//! default Swarm surface.

use crate::capabilities::{self, Category};
use rmcp::model::{CallToolRequestParams, JsonObject, Tool};
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct DiscoverCapabilityArgs {
    /// Category slug to list (for example `game`, `video`, or `shillbot_client`).
    pub category: Option<String>,
    /// Exact tool name to describe, including its input schema and destination.
    pub tool: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UseCapabilityArgs {
    /// Exact capability tool to invoke. Recursive gateway calls are rejected.
    pub tool: String,
    /// Arguments for the target tool, unchanged.
    #[serde(default)]
    pub arguments: JsonObject,
    /// Must be true for spend-capable calls. This is acknowledgement, not payment.
    pub acknowledge_spend: Option<bool>,
}

fn endpoint(category: Category) -> &'static str {
    match category {
        Category::Game => "https://mcp.coordination.game/mcp",
        Category::ShillbotEarn | Category::ShillbotClient | Category::Video => {
            "https://mcp.shillbot.org/mcp"
        }
        _ => "https://mcp.swarm.tips/mcp",
    }
}

fn summary(tool: &Tool) -> serde_json::Value {
    let cap = capabilities::capability(tool.name.as_ref()).expect("router tool is registered");
    serde_json::json!({
        "tool": tool.name,
        "category": cap.category.slug(),
        "economics": cap.economics.slug(),
        "mcp_url": endpoint(cap.category),
    })
}

pub fn describe(
    tools: &[Tool],
    args: &DiscoverCapabilityArgs,
) -> Result<serde_json::Value, rmcp::ErrorData> {
    if args.category.is_some() && args.tool.is_some() {
        return Err(rmcp::ErrorData::invalid_params(
            "pass either category or tool, not both",
            None,
        ));
    }
    if let Some(name) = args.tool.as_deref() {
        let tool = tools
            .iter()
            .find(|t| t.name.as_ref() == name)
            .ok_or_else(|| {
                rmcp::ErrorData::invalid_params(format!("unknown capability tool: {name}"), None)
            })?;
        if capabilities::GATEWAY_TOOLS.contains(&name) {
            return Err(rmcp::ErrorData::invalid_params(
                "gateway tools cannot describe or invoke themselves",
                None,
            ));
        }
        let mut value = summary(tool);
        value["description"] = serde_json::json!(tool.description);
        value["input_schema"] = serde_json::json!(tool.input_schema);
        return Ok(value);
    }

    let requested = if let Some(slug) = args.category.as_deref() {
        capabilities::categories()
            .find(|c| c.slug() == slug)
            .ok_or_else(|| {
                rmcp::ErrorData::invalid_params(
                    format!("unknown capability category: {slug}"),
                    None,
                )
            })?
    } else {
        // The zero-argument call is intentionally an index, not a dump of all
        // hidden schemas. This keeps the Swarm front door small while still
        // telling callers what focused surfaces exist and how broad they are.
        let categories: Vec<_> = capabilities::categories()
            .map(|category| {
                let entries: Vec<_> = tools
                    .iter()
                    .filter(|tool| !capabilities::GATEWAY_TOOLS.contains(&tool.name.as_ref()))
                    .filter_map(|tool| {
                        let cap = capabilities::capability(tool.name.as_ref())?;
                        (cap.category == category).then_some(cap)
                    })
                    .collect();
                let free = entries
                    .iter()
                    .filter(|cap| cap.economics == capabilities::Economics::Free)
                    .count();
                let earn = entries
                    .iter()
                    .filter(|cap| cap.economics == capabilities::Economics::Earn)
                    .count();
                let spend = entries
                    .iter()
                    .filter(|cap| cap.economics == capabilities::Economics::Spend)
                    .count();
                let mixed = entries
                    .iter()
                    .filter(|cap| cap.economics == capabilities::Economics::Mixed)
                    .count();
                serde_json::json!({
                    "category": category.slug(),
                    "tool_count": entries.len(),
                    "economics": {"free": free, "earn": earn, "spend": spend, "mixed": mixed},
                    "mcp_url": endpoint(category),
                })
            })
            .collect();
        return Ok(serde_json::json!({
            "categories": categories,
            "usage": "Pass one category to list its tools, or one exact tool name for its schema. Use the focused mcp_url directly for repeated work; swarm_use_capability is a one-call bridge and requires acknowledge_spend:true only for spend-capable calls.",
        }));
    };

    let mut grouped = serde_json::Map::new();
    for category in capabilities::categories().filter(|c| requested == *c) {
        let mut entries: Vec<_> = tools
            .iter()
            .filter(|t| !capabilities::GATEWAY_TOOLS.contains(&t.name.as_ref()))
            .filter(|t| {
                capabilities::capability(t.name.as_ref())
                    .is_some_and(|cap| cap.category == category)
            })
            .map(summary)
            .collect();
        entries.sort_by(|a, b| a["tool"].as_str().cmp(&b["tool"].as_str()));
        grouped.insert(
            category.slug().to_string(),
            serde_json::Value::Array(entries),
        );
    }
    Ok(serde_json::json!({
        "categories": grouped,
        "usage": "Call swarm_use_capability with {tool, arguments}; add acknowledge_spend:true when economics is spend or the target action spends funds. Direct focused hosts are preferred for repeated use.",
    }))
}

pub fn rewrite(
    mut request: CallToolRequestParams,
) -> Result<CallToolRequestParams, rmcp::ErrorData> {
    let raw = request.arguments.take().unwrap_or_default();
    let args: UseCapabilityArgs =
        serde_json::from_value(serde_json::Value::Object(raw)).map_err(|e| {
            rmcp::ErrorData::invalid_params(
                format!("invalid swarm_use_capability arguments: {e}"),
                None,
            )
        })?;
    if capabilities::GATEWAY_TOOLS.contains(&args.tool.as_str()) {
        return Err(rmcp::ErrorData::invalid_params(
            "recursive capability gateway calls are not allowed",
            None,
        ));
    }
    if capabilities::capability(&args.tool).is_none() {
        return Err(rmcp::ErrorData::invalid_params(
            format!("unknown capability tool: {}", args.tool),
            None,
        ));
    }
    if capabilities::requires_spend_ack(&args.tool, Some(&args.arguments))
        && args.acknowledge_spend != Some(true)
    {
        return Err(rmcp::ErrorData::invalid_params(
            format!("{} is spend-capable; retry with acknowledge_spend:true after reviewing its cost and transaction", args.tool),
            Some(serde_json::json!({"reason":"spend_acknowledgement_required","tool":args.tool})),
        ));
    }
    request.name = args.tool.into();
    request.arguments = Some(args.arguments);
    Ok(request)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declared_tools() -> Vec<Tool> {
        crate::server::SwarmTipsMcp::declared_tools()
    }

    fn gateway_request(
        tool: &str,
        arguments: serde_json::Value,
        ack: Option<bool>,
    ) -> CallToolRequestParams {
        CallToolRequestParams::new("swarm_use_capability").with_arguments(
            serde_json::json!({"tool":tool,"arguments":arguments,"acknowledge_spend":ack})
                .as_object()
                .cloned()
                .unwrap(),
        )
    }

    #[test]
    fn read_calls_rewrite_without_ack_and_preserve_arguments() {
        let request = rewrite(gateway_request(
            "check_video_status",
            serde_json::json!({"session_id":"s"}),
            None,
        ))
        .unwrap();
        assert_eq!(request.name, "check_video_status");
        assert_eq!(request.arguments.unwrap()["session_id"], "s");

        let leaderboard = rewrite(gateway_request(
            "game_get_leaderboard",
            serde_json::json!({"limit": 5}),
            None,
        ))
        .unwrap();
        assert_eq!(leaderboard.name, "game_get_leaderboard");
    }

    #[test]
    fn spend_calls_require_ack_and_then_rewrite() {
        let err = rewrite(gateway_request(
            "generate_video",
            serde_json::json!({"prompt":"p"}),
            None,
        ))
        .unwrap_err();
        assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);
        let request = rewrite(gateway_request(
            "generate_video",
            serde_json::json!({"prompt":"p"}),
            Some(true),
        ))
        .unwrap();
        assert_eq!(request.name, "generate_video");
    }

    #[test]
    fn recursion_and_unknown_tools_are_rejected() {
        assert!(rewrite(gateway_request(
            "swarm_use_capability",
            serde_json::json!({}),
            Some(true)
        ))
        .is_err());
        assert!(rewrite(gateway_request(
            "not_a_tool",
            serde_json::json!({}),
            Some(true)
        ))
        .is_err());
    }

    #[test]
    fn discovery_routes_hidden_categories_and_returns_the_router_schema() {
        let tools = declared_tools();
        let index = describe(&tools, &DiscoverCapabilityArgs::default()).unwrap();
        let categories = index["categories"].as_array().unwrap();
        assert_eq!(categories.len(), Category::ALL.len());
        assert!(categories
            .iter()
            .all(|entry| entry.get("tool_count").is_some()));
        assert!(
            categories.iter().all(|entry| entry.get("tools").is_none()),
            "zero-argument discovery must remain a compact index"
        );

        let game = describe(
            &tools,
            &DiscoverCapabilityArgs {
                category: Some("game".into()),
                tool: None,
            },
        )
        .unwrap();
        let entries = game["categories"]["game"].as_array().unwrap();
        assert!(entries.iter().any(|entry| {
            entry["tool"] == "game_find_match"
                && entry["mcp_url"] == "https://mcp.coordination.game/mcp"
                && entry["economics"] == "spend"
        }));
        assert!(entries.iter().any(|entry| {
            entry["tool"] == "game_get_leaderboard" && entry["economics"] == "free"
        }));

        let described = describe(
            &tools,
            &DiscoverCapabilityArgs {
                category: None,
                tool: Some("generate_video".into()),
            },
        )
        .unwrap();
        let direct = tools
            .iter()
            .find(|tool| tool.name.as_ref() == "generate_video")
            .unwrap();
        assert_eq!(described["mcp_url"], "https://mcp.shillbot.org/mcp");
        assert_eq!(
            described["input_schema"],
            serde_json::json!(direct.input_schema)
        );
    }
}
