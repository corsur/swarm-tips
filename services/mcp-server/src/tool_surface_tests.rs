//! Guards over the generated tool surface — the JSON agents actually receive
//! from `tools/list` plus the `initialize` INSTRUCTIONS blob.
//!
//! The older inventory tests in `server.rs` assert cardinality only; a change
//! that rewrites every description and schema passes them untouched. These
//! tests close that gap:
//!
//!   snapshot   — full surface diffed against a committed JSON file
//!   manifest   — exact sorted name lists (readable diff on add/remove/rename)
//!   budget     — chars/4 token ratchet, total + per-description
//!   lints      — schema-title collisions, dangling tool references,
//!                [READ] ⇔ read_only_hint coherence, INSTRUCTIONS parity
//!
//! Ratchets hold the CURRENT surface as the ceiling; the Phase-3 trim tightens
//! them (total → 8_800, per-description → 200, allowlists → empty).

use crate::server::{filter_tools_for_surface, SwarmTipsMcp};
use crate::surfaces::Surface;
use rmcp::model::Tool;
use std::collections::{BTreeMap, BTreeSet};

const SNAPSHOT_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tool-surface.snapshot.json");

/// Total budget for the default-visible surface: serialized tools + the
/// INSTRUCTIONS blob, estimated at chars/4. Ratchet — lower it, never raise
/// it. chars/4 overestimates vs a real tokenizer by ~1.37x (18.3k here was
/// 13.3k tiktoken live), so this ceiling ≈ 11.4k real tokens. History:
/// 18_400 pre-trim → 15_650 after the Phase-3 trim (the keep-list — flows,
/// footguns, signing walkthroughs, the complete_task guide — is protected
/// content and priced in; the fusion PR trims the four inbox/registration
/// descriptions further). The live e2e gate measures the real tokenizer.
const TOTAL_TOKEN_RATCHET: usize = 12_000;

/// Per-description budget for NEW tools (the v4 audit cap that
/// scripts/e2e/mcp-initialize.sh also enforces live).
const DESCRIPTION_TOKEN_CAP: usize = 200;

fn approx_tokens(s: &str) -> usize {
    s.len().div_ceil(4)
}

fn all_tools() -> Vec<Tool> {
    let mut tools = SwarmTipsMcp::declared_tools();
    tools.sort_by(|a, b| a.name.cmp(&b.name));
    tools
}

fn visible_tools(surface: Surface) -> Vec<Tool> {
    filter_tools_for_surface(all_tools(), surface, false)
}

fn tool_json(tool: &Tool) -> serde_json::Value {
    serde_json::to_value(tool).expect("Tool serializes")
}

fn surface_json() -> serde_json::Value {
    serde_json::Value::Array(all_tools().iter().map(tool_json).collect())
}

// -- snapshot ---------------------------------------------------------------

/// The full declared surface — hidden ones are callable and
/// ship to clients with SHOW_TESTNET_TOOLS) against the committed snapshot.
/// Regenerate deliberately with:
///   UPDATE_TOOL_SNAPSHOT=1 cargo test -p mcp-server tool_surface
#[test]
fn tool_surface_matches_committed_snapshot() {
    let current = surface_json();
    if std::env::var("UPDATE_TOOL_SNAPSHOT").is_ok() {
        let pretty = serde_json::to_string_pretty(&current).expect("serialize snapshot");
        std::fs::write(SNAPSHOT_PATH, pretty + "\n").expect("write snapshot");
        if let Ok(path) = std::env::var("MCP_REFERENCE_EXPORT_PATH") {
            let reference = serde_json::json!({
                "fetchedAt": chrono::Utc::now().to_rfc3339(),
                "instructions": crate::instructions::for_surface(Surface::Swarm),
                "tools": visible_tools(Surface::Swarm).iter().map(tool_json).collect::<Vec<_>>()
            });
            std::fs::write(
                path,
                serde_json::to_string_pretty(&reference).unwrap() + "\n",
            )
            .expect("export staged reference");
        }
        return;
    }
    let committed: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(SNAPSHOT_PATH).expect(
            "tool-surface.snapshot.json missing — generate it with \
             UPDATE_TOOL_SNAPSHOT=1 cargo test -p mcp-server tool_surface",
        ))
        .expect("snapshot parses");

    // Diff per tool name so a failure names the tools, not a 90KB blob.
    let by_name = |v: &serde_json::Value| -> BTreeMap<String, serde_json::Value> {
        v.as_array()
            .expect("snapshot is an array")
            .iter()
            .map(|t| {
                (
                    t["name"].as_str().expect("tool has a name").to_string(),
                    t.clone(),
                )
            })
            .collect()
    };
    let old = by_name(&committed);
    let new = by_name(&current);

    let old_names: BTreeSet<_> = old.keys().collect();
    let new_names: BTreeSet<_> = new.keys().collect();
    let added: Vec<_> = new_names.difference(&old_names).collect();
    let removed: Vec<_> = old_names.difference(&new_names).collect();
    let changed: Vec<_> = old
        .iter()
        .filter(|(name, v)| new.get(*name).is_some_and(|n| n != *v))
        .map(|(name, _)| name.clone())
        .collect();

    assert!(
        added.is_empty() && removed.is_empty() && changed.is_empty(),
        "tool surface drifted from the committed snapshot.\n  added: {added:?}\n  \
         removed: {removed:?}\n  changed: {changed:?}\nIf intentional, regenerate with \
         UPDATE_TOOL_SNAPSHOT=1 cargo test -p mcp-server tool_surface (and update the \
         count-bearing prose the server.rs inventory test names)."
    );
}

// -- name manifest ----------------------------------------------------------

/// Exact sorted name lists. Unlike the integer counts in server.rs, a diff
/// here NAMES the tool that appeared/vanished/renamed.
#[test]
fn tool_name_manifest_is_exact() {
    const SWARM: &[&str] = &[
        "agent_ack_message_ids",
        "agent_ack_messages",
        "agent_get_messages",
        "agent_list_messages",
        "agent_mute_thread",
        "agent_open_messages",
        "agent_profile",
        "agent_reputation_leaderboard",
        "agent_send_message",
        "agent_trust_score",
        "agent_verify_wallet",
        "delete_webhook",
        "discover_opportunities",
        "get_webhook",
        "list_earning_opportunities",
        "list_extensions",
        "list_related_servers",
        "query_agent_credit_web_score",
        "register_wallet",
        "register_webhook",
        "search_mcp_servers",
        "shillbot_approve_task",
        "shillbot_check_earnings",
        "shillbot_claim_task",
        "shillbot_complete_task",
        "shillbot_confirm_tx",
        "shillbot_create_campaign",
        "shillbot_finalize_task",
        "shillbot_get_attestation",
        "shillbot_get_task_details",
        "shillbot_list_available_tasks",
        "shillbot_list_pending_approval",
        "shillbot_onboard",
        "shillbot_sponsor_tx",
        "shillbot_submit_tx",
        "shillbot_submit_work",
        "shillbot_verify_task",
        "topic_publish",
        "topic_read",
        "topic_report",
    ];
    const SHILLBOT: &[&str] = &[
        "agent_verify_wallet",
        "check_video_status",
        "generate_video",
        "list_related_servers",
        "register_wallet",
        "shillbot_approve_task",
        "shillbot_check_earnings",
        "shillbot_claim_task",
        "shillbot_complete_task",
        "shillbot_confirm_tx",
        "shillbot_create_campaign",
        "shillbot_finalize_task",
        "shillbot_get_attestation",
        "shillbot_get_task_details",
        "shillbot_list_available_tasks",
        "shillbot_list_pending_approval",
        "shillbot_onboard",
        "shillbot_sponsor_tx",
        "shillbot_submit_tx",
        "shillbot_submit_work",
        "shillbot_verify_task",
    ];
    const GAME: &[&str] = &[
        "game_check_match",
        "game_commit_guess",
        "game_evm_commit_guess",
        "game_evm_committed",
        "game_evm_match_status",
        "game_evm_reveal_guess",
        "game_find_evm_match",
        "game_find_match",
        "game_get_leaderboard",
        "game_get_messages",
        "game_get_result",
        "game_reveal_guess",
        "game_send_message",
        "game_submit_tx",
        "list_related_servers",
        "register_wallet",
        "xchain_build_create_match",
        "xchain_build_create_xmatch",
        "xchain_build_lock",
        "xchain_build_lock_xmatch",
        "xchain_build_refund",
        "xchain_build_refund_xmatch",
        "xchain_build_settle",
        "xchain_commit_guess",
        "xchain_find_match",
        "xchain_gameplay_status",
        "xchain_match_status",
        "xchain_reveal_guess",
        "xchain_sign_checkpoint",
        "xchain_supported_chains",
    ];

    for (surface, expected) in [
        (Surface::Swarm, SWARM),
        (Surface::Shillbot, SHILLBOT),
        (Surface::Game, GAME),
    ] {
        let listed = visible_tools(surface);
        let names: Vec<&str> = listed.iter().map(|t| t.name.as_ref()).collect();
        assert_eq!(names, expected, "{} tool names", surface.host());
    }

    let unregistered: Vec<_> = all_tools()
        .iter()
        .filter(|t| crate::capabilities::capability(t.name.as_ref()).is_none())
        .map(|t| t.name.to_string())
        .collect();
    assert!(
        unregistered.is_empty(),
        "tools missing registry metadata: {unregistered:?}"
    );
}

// -- token budget -----------------------------------------------------------

/// What one agent pays per connection: every visible tool's serialized JSON
/// plus INSTRUCTIONS, at the chars/4 estimate the live e2e uses.
#[test]
fn visible_surface_fits_the_token_ratchet() {
    for surface in [Surface::Swarm, Surface::Shillbot, Surface::Game] {
        let tools_chars: usize = visible_tools(surface)
            .iter()
            .map(|t| {
                serde_json::to_string(&tool_json(t))
                    .expect("serialize")
                    .len()
            })
            .sum();
        let instructions = crate::instructions::for_surface(surface);
        let total = tools_chars.div_ceil(4) + approx_tokens(&instructions);
        assert!(
            total <= TOTAL_TOKEN_RATCHET,
            "{} surface is ~{total} tokens (ratchet {TOTAL_TOKEN_RATCHET})",
            surface.host()
        );
    }
}

/// Per-description cap. Known oversized descriptions are grandfathered at
/// their current size — they may shrink, never grow; new tools get 200.
#[test]
fn descriptions_fit_their_caps() {
    // name -> grandfathered cap (current measured size; may shrink, never
    // grow). Post-fusion residue: these three deliberately spend a little
    // over 200 on the register/verify one-tool loop + support-path carve-out
    // — the exact copy the usage data showed agents bouncing off when it was
    // missing.
    let grandfathered: BTreeMap<&str, usize> = [
        ("agent_send_message", 225),
        ("agent_verify_wallet", 205),
        ("register_wallet", 232),
    ]
    .into_iter()
    .collect();

    let mut violations = Vec::new();
    for tool in all_tools() {
        let name: &str = tool.name.as_ref();
        let desc = tool.description.as_deref().unwrap_or_default();
        let tokens = approx_tokens(desc);
        let cap = grandfathered
            .get(name)
            .copied()
            .unwrap_or(DESCRIPTION_TOKEN_CAP);
        if tokens > cap {
            violations.push(format!("{name}: ~{tokens} tokens (cap {cap})"));
        }
    }
    assert!(
        violations.is_empty(),
        "descriptions over budget (grandfathered caps shrink, never grow):\n{}",
        violations.join("\n")
    );
}

// -- lints ------------------------------------------------------------------

/// Every input schema publishes a `title` (the Rust struct name by default).
/// Shared arg structs give six different tools the same title — actively
/// misleading for `shillbot_finalize_task` wearing "ClaimTaskArgs". Known
/// collisions are frozen here; Phase 3 splits the structs and empties this.
#[test]
fn schema_title_collisions_do_not_grow() {
    let mut by_title: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for tool in all_tools() {
        let json = tool_json(&tool);
        // Argless tools share an untitled empty schema — nothing misleading
        // to collide on. The lint targets tools that actually take input.
        let has_properties = json["inputSchema"]["properties"]
            .as_object()
            .is_some_and(|p| !p.is_empty());
        if !has_properties {
            continue;
        }
        let title = json["inputSchema"]["title"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        by_title
            .entry(title)
            .or_default()
            .push(tool.name.to_string());
    }
    let collisions: Vec<_> = by_title
        .iter()
        .filter(|(_, tools)| tools.len() > 1)
        .collect();
    assert!(
        collisions.is_empty(),
        "schema-title collisions (give each tool's arg struct its own title): {collisions:?}"
    );
}

/// Every plain `network` selector tells the SAME story. The bespoke
/// exceptions are tools where the field genuinely does more, and only those.
#[test]
fn network_arg_docs_are_canonical() {
    let bespoke: BTreeSet<&str> = [
        "game_find_match",          // names the PDAs the read targets
        "game_submit_tx",           // BlockhashNotFound broadcast semantics
        "shillbot_submit_tx",       // broadcast + confirm routing semantics
        "shillbot_get_attestation", // 409 on mismatched network
    ]
    .into_iter()
    .collect();

    let mut off_script = Vec::new();
    for tool in all_tools() {
        if bespoke.contains(tool.name.as_ref()) {
            continue;
        }
        let json = tool_json(&tool);
        let Some(desc) = json["inputSchema"]["properties"]["network"]["description"].as_str()
        else {
            continue;
        };
        if desc != crate::server::NETWORK_ARG_DOC {
            off_script.push(format!("{}: {desc:?}", tool.name));
        }
    }
    assert!(
        off_script.is_empty(),
        "network arg docs diverged from NETWORK_ARG_DOC (one concept, one \
         explanation — add a bespoke entry only if the field does more):\n{}",
        off_script.join("\n")
    );
}

/// A description that names another tool must name a tool that exists —
/// pointer rot ("use list_available_tasks") sends agents to a 404.
#[test]
fn tool_references_in_descriptions_resolve() {
    let names: BTreeSet<String> = all_tools().iter().map(|t| t.name.to_string()).collect();
    let prefixes = [
        "game_",
        "shillbot_",
        "xchain_",
        "agent_",
        "topic_",
        "register_",
        "generate_",
        "check_",
        "query_",
        "list_",
        "discover_",
        "search_",
        "get_",
        "delete_",
    ];
    // Identifier-shaped words that legitimately are not tool names.
    let allow: BTreeSet<&str> = [
        "register_wallet_evm", // log event named in prose
        "get_messages",        // shorthand for the agent_/game_ pair
        "game_id",             // field name, not a tool
        "game_invite",         // topic-post intent value
        "xchain_build_",       // deliberate family-prefix mention (xchain_build_*)
    ]
    .into_iter()
    .collect();

    let mut dangling = Vec::new();
    for tool in all_tools() {
        let desc = tool.description.as_deref().unwrap_or_default();
        for word in desc
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .filter(|w| w.contains('_') && prefixes.iter().any(|p| w.starts_with(p)))
        {
            if !names.contains(word) && !allow.contains(word) {
                dangling.push(format!("{}: `{word}`", tool.name));
            }
        }
    }
    assert!(
        dangling.is_empty(),
        "descriptions reference undeclared tools:\n{}",
        dangling.join("\n")
    );
}

/// The cash-flow tag and the MCP annotation must agree: `[READ]` tools carry
/// read_only_hint=true and vice versa — agents trust both signals.
#[test]
fn read_tag_and_read_only_hint_agree() {
    let mut mismatches = Vec::new();
    for tool in all_tools() {
        let desc = tool.description.as_deref().unwrap_or_default();
        let tagged_read = desc.starts_with("[READ]");
        let hinted_read = tool
            .annotations
            .as_ref()
            .and_then(|a| a.read_only_hint)
            .unwrap_or(false);
        if tagged_read != hinted_read {
            mismatches.push(format!(
                "{}: [READ] tag={tagged_read}, read_only_hint={hinted_read}",
                tool.name
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "cash-flow tag / read_only_hint disagree:\n{}",
        mismatches.join("\n")
    );
}

/// MCP's four safety hints have deliberately pessimistic defaults when
/// omitted. Publish every one explicitly so clients receive our audited
/// classification rather than having to infer behavior from prose.
#[test]
fn every_tool_has_complete_standard_annotations() {
    let mut incomplete = Vec::new();
    for tool in all_tools() {
        let Some(a) = tool.annotations.as_ref() else {
            incomplete.push(format!("{}: annotations missing", tool.name));
            continue;
        };
        let fields = [
            ("readOnlyHint", a.read_only_hint),
            ("destructiveHint", a.destructive_hint),
            ("idempotentHint", a.idempotent_hint),
            ("openWorldHint", a.open_world_hint),
        ];
        for (name, value) in fields {
            if value.is_none() {
                incomplete.push(format!("{}: {name} missing", tool.name));
            }
        }
        if a.read_only_hint == Some(true) && a.destructive_hint != Some(false) {
            incomplete.push(format!(
                "{}: read-only tool must set destructiveHint=false",
                tool.name
            ));
        }
    }
    assert!(
        incomplete.is_empty(),
        "incomplete MCP safety annotations:\n{}",
        incomplete.join("\n")
    );
}

/// INSTRUCTIONS parity: every visible tool is mentioned by name, and the
/// prose carries NO literal tool counts — seven different stale counts were
/// on disk across the doc surfaces before this rule; pointing at tools/list
/// is the only claim that can't rot. Runs pre-deploy.
#[test]
fn instructions_explain_workflows_without_broadcasting_related_hosts() {
    for surface in Surface::ALL {
        let instructions = crate::instructions::for_surface(surface);
        assert!(instructions.contains("tools/list"));
        assert!(instructions.contains("authoritative"));
        assert!(instructions.contains("list_related_servers"));
        for related in surface.related() {
            assert!(!instructions.contains(related.mcp_url()));
        }
        assert!(visible_tools(surface)
            .iter()
            .any(|t| t.name == "list_related_servers"));
        let directory = crate::surfaces::related_directory(surface);
        assert_eq!(directory["server"], surface.registry_name());
        assert_eq!(directory["related_servers"].as_array().unwrap().len(), 2);
        for (entry, related) in directory["related_servers"]
            .as_array()
            .unwrap()
            .iter()
            .zip(surface.related())
        {
            assert_eq!(entry["remotes"][0]["url"], related.mcp_url());
            assert!(entry["session_setup"]
                .as_str()
                .unwrap()
                .contains("Independent"));
        }
    }
    let instructions = crate::instructions::for_surface(Surface::Swarm);
    for name in [
        "agent_list_messages",
        "agent_open_messages",
        "agent_ack_message_ids",
    ] {
        assert!(instructions.contains(name));
    }
    assert!(instructions.contains("Opening does not acknowledge"));
    assert!(instructions.contains("previews are off"));
}

// -- 1:1 REST / Stateful Client Parity Manifest -----------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolParity {
    /// Stateless HTTP REST equivalent exposed on one of the first-party APIs.
    HttpEndpoint {
        host: &'static str,
        method: &'static str,
        path: &'static str,
        operation_id: &'static str,
    },
    /// Inherently stateful WebSocket / commit-reveal preimage tool served by
    /// `@swarm-tips/client/coordination-game` (`mcp.coordination.game` fallback).
    ClientStateful {
        surface: &'static str,
        reason: &'static str,
    },
}

const STATEFUL_GAME_SURFACE: &str = "@swarm-tips/client/coordination-game";
const STATEFUL_GAME_REASON: &str =
    "Stateful WebSocket matchmaking, bounded chat stream, and commit-reveal preimage lifecycle";

const REST_PARITY_MANIFEST: &[(&str, ToolParity)] = &[
    // Discovery (4)
    (
        "discover_opportunities",
        ToolParity::HttpEndpoint {
            host: "mcp.swarm.tips",
            method: "GET",
            path: "/internal/mcp/earning-candidates",
            operation_id: "list_mcp_earning_candidates",
        },
    ),
    (
        "list_earning_opportunities",
        ToolParity::HttpEndpoint {
            host: "mcp.swarm.tips",
            method: "GET",
            path: "/internal/listings",
            operation_id: "list_earning_opportunities",
        },
    ),
    (
        "list_related_servers",
        ToolParity::HttpEndpoint {
            host: "mcp.swarm.tips",
            method: "GET",
            path: "/related-servers",
            operation_id: "list_related_servers",
        },
    ),
    (
        "search_mcp_servers",
        ToolParity::HttpEndpoint {
            host: "mcp.swarm.tips",
            method: "GET",
            path: "/internal/mcp/search",
            operation_id: "search_mcp_servers",
        },
    ),
    // Identity & Reputation (6)
    (
        "agent_profile",
        ToolParity::HttpEndpoint {
            host: "mcp.swarm.tips",
            method: "GET",
            path: "/internal/agent-reputation",
            operation_id: "get_agent_reputation",
        },
    ),
    (
        "agent_reputation_leaderboard",
        ToolParity::HttpEndpoint {
            host: "mcp.swarm.tips",
            method: "GET",
            path: "/internal/reputation/leaderboard",
            operation_id: "get_reputation_leaderboard",
        },
    ),
    (
        "agent_trust_score",
        ToolParity::HttpEndpoint {
            host: "mcp.swarm.tips",
            method: "GET",
            path: "/internal/agent-reputation",
            operation_id: "get_agent_reputation",
        },
    ),
    (
        "list_extensions",
        ToolParity::HttpEndpoint {
            host: "mcp.swarm.tips",
            method: "GET",
            path: "/internal/mcp/primitives",
            operation_id: "list_mcp_primitives",
        },
    ),
    (
        "query_agent_credit_web_score",
        ToolParity::HttpEndpoint {
            host: "mcp.swarm.tips",
            method: "GET",
            path: "/internal/agent-reputation",
            operation_id: "get_agent_reputation",
        },
    ),
    (
        "register_wallet",
        ToolParity::HttpEndpoint {
            host: "api.coordination.game",
            method: "POST",
            path: "/auth/challenge",
            operation_id: "game_auth_challenge",
        },
    ),
    // Messaging, Webhooks & Topic Boards (14)
    (
        "agent_ack_message_ids",
        ToolParity::HttpEndpoint {
            host: "mcp.swarm.tips",
            method: "POST",
            path: "/internal/inbox/ack-ids",
            operation_id: "inbox_ack_message_ids",
        },
    ),
    (
        "agent_ack_messages",
        ToolParity::HttpEndpoint {
            host: "mcp.swarm.tips",
            method: "POST",
            path: "/internal/inbox/ack",
            operation_id: "inbox_ack_messages",
        },
    ),
    (
        "agent_get_messages",
        ToolParity::HttpEndpoint {
            host: "mcp.swarm.tips",
            method: "GET",
            path: "/internal/inbox/messages",
            operation_id: "inbox_get_messages",
        },
    ),
    (
        "agent_list_messages",
        ToolParity::HttpEndpoint {
            host: "mcp.swarm.tips",
            method: "GET",
            path: "/internal/inbox/list",
            operation_id: "inbox_list_messages",
        },
    ),
    (
        "agent_mute_thread",
        ToolParity::HttpEndpoint {
            host: "mcp.swarm.tips",
            method: "POST",
            path: "/internal/inbox/mute",
            operation_id: "inbox_mute_thread",
        },
    ),
    (
        "agent_open_messages",
        ToolParity::HttpEndpoint {
            host: "mcp.swarm.tips",
            method: "POST",
            path: "/internal/inbox/open",
            operation_id: "inbox_open_messages",
        },
    ),
    (
        "agent_send_message",
        ToolParity::HttpEndpoint {
            host: "mcp.swarm.tips",
            method: "POST",
            path: "/internal/inbox/send",
            operation_id: "inbox_send_message",
        },
    ),
    (
        "agent_verify_wallet",
        ToolParity::HttpEndpoint {
            host: "mcp.swarm.tips",
            method: "POST",
            path: "/internal/inbox/session",
            operation_id: "create_inbox_session",
        },
    ),
    (
        "delete_webhook",
        ToolParity::HttpEndpoint {
            host: "mcp.swarm.tips",
            method: "DELETE",
            path: "/internal/inbox/webhook",
            operation_id: "inbox_delete_webhook",
        },
    ),
    (
        "get_webhook",
        ToolParity::HttpEndpoint {
            host: "mcp.swarm.tips",
            method: "GET",
            path: "/internal/inbox/webhook",
            operation_id: "inbox_get_webhook",
        },
    ),
    (
        "register_webhook",
        ToolParity::HttpEndpoint {
            host: "mcp.swarm.tips",
            method: "POST",
            path: "/internal/inbox/webhook",
            operation_id: "inbox_register_webhook",
        },
    ),
    (
        "topic_publish",
        ToolParity::HttpEndpoint {
            host: "mcp.swarm.tips",
            method: "POST",
            path: "/internal/topics/publish",
            operation_id: "topics_publish",
        },
    ),
    (
        "topic_read",
        ToolParity::HttpEndpoint {
            host: "mcp.swarm.tips",
            method: "GET",
            path: "/internal/topics/read",
            operation_id: "topics_read",
        },
    ),
    (
        "topic_report",
        ToolParity::HttpEndpoint {
            host: "mcp.swarm.tips",
            method: "POST",
            path: "/internal/topics/report",
            operation_id: "topics_report",
        },
    ),
    // Shillbot Earn & Client (16)
    (
        "shillbot_approve_task",
        ToolParity::HttpEndpoint {
            host: "api.shillbot.org",
            method: "POST",
            path: "/tasks/{task_id}/approve",
            operation_id: "shillbot_approve_task",
        },
    ),
    (
        "shillbot_check_earnings",
        ToolParity::HttpEndpoint {
            host: "api.shillbot.org",
            method: "GET",
            path: "/agent/earnings",
            operation_id: "shillbot_get_agent_earnings",
        },
    ),
    (
        "shillbot_claim_task",
        ToolParity::HttpEndpoint {
            host: "api.shillbot.org",
            method: "POST",
            path: "/tasks/{task_id}/claim",
            operation_id: "shillbot_claim_task",
        },
    ),
    (
        "shillbot_complete_task",
        ToolParity::HttpEndpoint {
            host: "api.shillbot.org",
            method: "GET",
            path: "/tasks/{task_id}",
            operation_id: "shillbot_get_task",
        },
    ),
    (
        "shillbot_confirm_tx",
        ToolParity::HttpEndpoint {
            host: "api.shillbot.org",
            method: "POST",
            path: "/tasks/{task_id}/confirm",
            operation_id: "shillbot_confirm_task",
        },
    ),
    (
        "shillbot_create_campaign",
        ToolParity::HttpEndpoint {
            host: "api.shillbot.org",
            method: "POST",
            path: "/campaigns",
            operation_id: "shillbot_create_campaign",
        },
    ),
    (
        "shillbot_finalize_task",
        ToolParity::HttpEndpoint {
            host: "api.shillbot.org",
            method: "POST",
            path: "/tasks/{task_id}/build-finalize",
            operation_id: "shillbot_build_finalize",
        },
    ),
    (
        "shillbot_get_attestation",
        ToolParity::HttpEndpoint {
            host: "api.shillbot.org",
            method: "GET",
            path: "/tasks/{task_id}/attestation",
            operation_id: "shillbot_get_attestation",
        },
    ),
    (
        "shillbot_get_task_details",
        ToolParity::HttpEndpoint {
            host: "api.shillbot.org",
            method: "GET",
            path: "/tasks/{task_id}",
            operation_id: "shillbot_get_task",
        },
    ),
    (
        "shillbot_list_available_tasks",
        ToolParity::HttpEndpoint {
            host: "api.shillbot.org",
            method: "GET",
            path: "/tasks",
            operation_id: "shillbot_list_tasks",
        },
    ),
    (
        "shillbot_list_pending_approval",
        ToolParity::HttpEndpoint {
            host: "api.shillbot.org",
            method: "GET",
            path: "/client/pending-approval",
            operation_id: "shillbot_list_pending_approval",
        },
    ),
    (
        "shillbot_onboard",
        ToolParity::HttpEndpoint {
            host: "api.shillbot.org",
            method: "POST",
            path: "/agent/onboard",
            operation_id: "shillbot_agent_onboard",
        },
    ),
    (
        "shillbot_sponsor_tx",
        ToolParity::HttpEndpoint {
            host: "api.shillbot.org",
            method: "POST",
            path: "/tasks/{task_id}/sponsor",
            operation_id: "shillbot_sponsor_transaction",
        },
    ),
    (
        "shillbot_submit_tx",
        ToolParity::HttpEndpoint {
            host: "api.shillbot.org",
            method: "POST",
            path: "/tasks/{task_id}/confirm",
            operation_id: "shillbot_confirm_task",
        },
    ),
    (
        "shillbot_submit_work",
        ToolParity::HttpEndpoint {
            host: "api.shillbot.org",
            method: "POST",
            path: "/tasks/{task_id}/submit",
            operation_id: "shillbot_submit_task",
        },
    ),
    (
        "shillbot_verify_task",
        ToolParity::HttpEndpoint {
            host: "mcp.swarm.tips",
            method: "POST",
            path: "/internal/build-verify-tx",
            operation_id: "build_verify_tx",
        },
    ),
    // Video (2) — publicly served on api.shillbot.org via LB /shorts/* path split
    (
        "check_video_status",
        ToolParity::HttpEndpoint {
            host: "api.shillbot.org",
            method: "GET",
            path: "/shorts/{id}",
            operation_id: "shillbot_check_video_status",
        },
    ),
    (
        "generate_video",
        ToolParity::HttpEndpoint {
            host: "api.shillbot.org",
            method: "POST",
            path: "/shorts/create-crypto",
            operation_id: "shillbot_generate_video",
        },
    ),
    // Stateful Coordination Game (28)
    (
        "game_check_match",
        ToolParity::ClientStateful {
            surface: STATEFUL_GAME_SURFACE,
            reason: STATEFUL_GAME_REASON,
        },
    ),
    (
        "game_commit_guess",
        ToolParity::ClientStateful {
            surface: STATEFUL_GAME_SURFACE,
            reason: STATEFUL_GAME_REASON,
        },
    ),
    (
        "game_evm_commit_guess",
        ToolParity::ClientStateful {
            surface: STATEFUL_GAME_SURFACE,
            reason: STATEFUL_GAME_REASON,
        },
    ),
    (
        "game_evm_committed",
        ToolParity::ClientStateful {
            surface: STATEFUL_GAME_SURFACE,
            reason: STATEFUL_GAME_REASON,
        },
    ),
    (
        "game_evm_match_status",
        ToolParity::ClientStateful {
            surface: STATEFUL_GAME_SURFACE,
            reason: STATEFUL_GAME_REASON,
        },
    ),
    (
        "game_evm_reveal_guess",
        ToolParity::ClientStateful {
            surface: STATEFUL_GAME_SURFACE,
            reason: STATEFUL_GAME_REASON,
        },
    ),
    (
        "game_find_evm_match",
        ToolParity::ClientStateful {
            surface: STATEFUL_GAME_SURFACE,
            reason: STATEFUL_GAME_REASON,
        },
    ),
    (
        "game_find_match",
        ToolParity::ClientStateful {
            surface: STATEFUL_GAME_SURFACE,
            reason: STATEFUL_GAME_REASON,
        },
    ),
    (
        "game_get_leaderboard",
        ToolParity::ClientStateful {
            surface: STATEFUL_GAME_SURFACE,
            reason: STATEFUL_GAME_REASON,
        },
    ),
    (
        "game_get_messages",
        ToolParity::ClientStateful {
            surface: STATEFUL_GAME_SURFACE,
            reason: STATEFUL_GAME_REASON,
        },
    ),
    (
        "game_get_result",
        ToolParity::ClientStateful {
            surface: STATEFUL_GAME_SURFACE,
            reason: STATEFUL_GAME_REASON,
        },
    ),
    (
        "game_reveal_guess",
        ToolParity::ClientStateful {
            surface: STATEFUL_GAME_SURFACE,
            reason: STATEFUL_GAME_REASON,
        },
    ),
    (
        "game_send_message",
        ToolParity::ClientStateful {
            surface: STATEFUL_GAME_SURFACE,
            reason: STATEFUL_GAME_REASON,
        },
    ),
    (
        "game_submit_tx",
        ToolParity::ClientStateful {
            surface: STATEFUL_GAME_SURFACE,
            reason: STATEFUL_GAME_REASON,
        },
    ),
    (
        "xchain_build_create_match",
        ToolParity::ClientStateful {
            surface: STATEFUL_GAME_SURFACE,
            reason: STATEFUL_GAME_REASON,
        },
    ),
    (
        "xchain_build_create_xmatch",
        ToolParity::ClientStateful {
            surface: STATEFUL_GAME_SURFACE,
            reason: STATEFUL_GAME_REASON,
        },
    ),
    (
        "xchain_build_lock",
        ToolParity::ClientStateful {
            surface: STATEFUL_GAME_SURFACE,
            reason: STATEFUL_GAME_REASON,
        },
    ),
    (
        "xchain_build_lock_xmatch",
        ToolParity::ClientStateful {
            surface: STATEFUL_GAME_SURFACE,
            reason: STATEFUL_GAME_REASON,
        },
    ),
    (
        "xchain_build_refund",
        ToolParity::ClientStateful {
            surface: STATEFUL_GAME_SURFACE,
            reason: STATEFUL_GAME_REASON,
        },
    ),
    (
        "xchain_build_refund_xmatch",
        ToolParity::ClientStateful {
            surface: STATEFUL_GAME_SURFACE,
            reason: STATEFUL_GAME_REASON,
        },
    ),
    (
        "xchain_build_settle",
        ToolParity::ClientStateful {
            surface: STATEFUL_GAME_SURFACE,
            reason: STATEFUL_GAME_REASON,
        },
    ),
    (
        "xchain_commit_guess",
        ToolParity::ClientStateful {
            surface: STATEFUL_GAME_SURFACE,
            reason: STATEFUL_GAME_REASON,
        },
    ),
    (
        "xchain_find_match",
        ToolParity::ClientStateful {
            surface: STATEFUL_GAME_SURFACE,
            reason: STATEFUL_GAME_REASON,
        },
    ),
    (
        "xchain_gameplay_status",
        ToolParity::ClientStateful {
            surface: STATEFUL_GAME_SURFACE,
            reason: STATEFUL_GAME_REASON,
        },
    ),
    (
        "xchain_match_status",
        ToolParity::ClientStateful {
            surface: STATEFUL_GAME_SURFACE,
            reason: STATEFUL_GAME_REASON,
        },
    ),
    (
        "xchain_reveal_guess",
        ToolParity::ClientStateful {
            surface: STATEFUL_GAME_SURFACE,
            reason: STATEFUL_GAME_REASON,
        },
    ),
    (
        "xchain_sign_checkpoint",
        ToolParity::ClientStateful {
            surface: STATEFUL_GAME_SURFACE,
            reason: STATEFUL_GAME_REASON,
        },
    ),
    (
        "xchain_supported_chains",
        ToolParity::ClientStateful {
            surface: STATEFUL_GAME_SURFACE,
            reason: STATEFUL_GAME_REASON,
        },
    ),
];

#[test]
fn rest_parity_manifest_covers_every_declared_tool_one_to_one() {
    let declared: BTreeSet<String> = all_tools()
        .into_iter()
        .map(|t| t.name.to_string())
        .collect();

    let mut seen = BTreeSet::new();
    let mut http_count = 0usize;
    let mut stateful_count = 0usize;

    let merged_openapi: serde_json::Value =
        serde_json::from_str(&crate::openapi::openapi_json_string())
            .expect("valid merged OpenAPI 3.1 JSON");
    let openapi_paths = merged_openapi["paths"].as_object().expect("paths object");

    // Also ensure the bundled merged spec never drifts behind mcp-server's own utoipa spec.
    let local_spec: serde_json::Value = serde_json::from_str(
        &crate::openapi::openapi_spec()
            .to_pretty_json()
            .expect("local openapi JSON"),
    )
    .expect("valid local openapi JSON");
    for (mcp_path, mcp_item) in local_spec["paths"].as_object().expect("local paths") {
        let merged_item = openapi_paths.get(mcp_path).unwrap_or_else(|| {
            panic!("openapi.merged.json is missing mcp-server path {mcp_path}; re-run merge-openapi.mjs")
        });
        for method in ["get", "post", "put", "patch", "delete"] {
            if let Some(local_op) = mcp_item.get(method) {
                assert_eq!(
                    merged_item[method]["operationId"], local_op["operationId"],
                    "openapi.merged.json drifted on {method} {mcp_path}"
                );
            }
        }
    }

    for &(tool_name, parity) in REST_PARITY_MANIFEST {
        assert!(
            seen.insert(tool_name.to_string()),
            "duplicate entry in REST_PARITY_MANIFEST: {tool_name}"
        );
        let cap = crate::capabilities::capability(tool_name)
            .unwrap_or_else(|| panic!("tool {tool_name} missing capability classification"));
        match parity {
            ToolParity::HttpEndpoint {
                host,
                method,
                path,
                operation_id,
            } => {
                http_count = http_count.saturating_add(1);
                assert_ne!(
                    cap.category,
                    crate::capabilities::Category::Game,
                    "Game tool {tool_name} must be classified as ClientStateful"
                );
                assert!(!host.is_empty() && !method.is_empty() && path.starts_with('/'));
                assert!(!operation_id.is_empty());

                let path_obj = openapi_paths.get(path).unwrap_or_else(|| {
                    panic!("tool {tool_name} maps to undocumented OpenAPI path {path} on {host}")
                });
                let method_key = method.to_ascii_lowercase();
                let op = path_obj.get(&method_key).unwrap_or_else(|| {
                    panic!("tool {tool_name} maps to undocumented method {method} on path {path}")
                });
                assert_eq!(
                    op["operationId"].as_str(),
                    Some(operation_id),
                    "operationId mismatch for {tool_name} ({method} {path})"
                );
                let expected_server = format!("https://{host}");
                assert_eq!(
                    op["servers"][0]["url"].as_str(),
                    Some(expected_server.as_str()),
                    "server host mismatch for {tool_name} ({method} {path})"
                );
            }
            ToolParity::ClientStateful { surface, reason } => {
                stateful_count = stateful_count.saturating_add(1);
                assert_eq!(
                    cap.category,
                    crate::capabilities::Category::Game,
                    "non-Game tool {tool_name} must map to an HttpEndpoint"
                );
                assert!(!surface.is_empty() && !reason.is_empty());
            }
        }
    }

    let missing: Vec<_> = declared.difference(&seen).collect();
    let extra: Vec<_> = seen.difference(&declared).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "REST_PARITY_MANIFEST drift — missing: {missing:?}, extra: {extra:?}"
    );
    assert_eq!(http_count, 42, "expected 42 stateless HTTP tools");
    assert_eq!(stateful_count, 28, "expected 28 stateful Game tools");
}

#[test]
fn skill_md_copies_are_byte_identical() {
    let canonical = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../skill/SKILL.md"));
    let indexed = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../skills/earning/swarm-tips/SKILL.md"
    ));

    assert_eq!(
        canonical, indexed,
        "skill/SKILL.md and skills/earning/swarm-tips/SKILL.md drifted; keep both byte-identical"
    );
    assert!(
        canonical.contains("\nversion: 2.0.0\n"),
        "SKILL.md must declare version: 2.0.0"
    );
    assert!(
        canonical.contains("@swarm-tips/client")
            && canonical.contains("https://mcp.swarm.tips/.well-known/openapi.json")
            && canonical.contains("https://mcp.swarm.tips/mcp"),
        "SKILL.md must document both Mode A (@swarm-tips/client + OpenAPI) and Mode B (MCP)"
    );
    assert!(
        !canonical.contains("../../../"),
        "SKILL.md must use full HTTPS links instead of depth-dependent ../../../ paths"
    );
    for tool in all_tools() {
        assert!(
            canonical.contains(tool.name.as_ref()),
            "SKILL.md is missing declared MCP tool `{}`",
            tool.name
        );
    }
}
