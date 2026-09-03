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
        "agent_ack_messages",
        "agent_get_messages",
        "agent_mute_thread",
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
        "check_video_status",
        "generate_video",
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
        "game_find_match",
        "game_get_leaderboard",
        "game_get_messages",
        "game_get_result",
        "game_reveal_guess",
        "game_send_message",
        "game_submit_tx",
        "register_wallet",
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
fn instructions_name_every_visible_tool_and_state_no_counts() {
    for surface in [Surface::Swarm, Surface::Shillbot, Surface::Game] {
        let instructions = crate::instructions::for_surface(surface);
        let visible = visible_tools(surface);
        let missing: Vec<_> = visible
            .iter()
            .map(|t| t.name.as_ref())
            .filter(|name: &&str| !instructions.contains(*name))
            .collect();
        assert!(
            missing.is_empty(),
            "{} tools absent from instructions: {missing:?}",
            surface.host()
        );
        assert!(instructions.contains("authoritative inventory is this server's own tools/list"));
        assert!(instructions.contains("Unified server: https://mcp.swarm.tips/mcp"));
        assert!(instructions
            .contains("every capability advertised by mcp.shillbot.org and mcp.coordination.game"));
        assert!(instructions.contains("callable there by exact tool name"));
        assert!(instructions.contains("Prefer the unified server"));
        assert!(instructions.contains("Related servers"));
        for related in surface.related() {
            assert!(instructions.contains(related.mcp_url()));
        }
        let mut digits_then_tools = instructions
            .split_whitespace()
            .zip(instructions.split_whitespace().skip(1))
            .filter(|(a, b)| a.chars().all(|c| c.is_ascii_digit()) && b.starts_with("tool"));
        assert!(
            digits_then_tools.next().is_none(),
            "{} instructions state a literal tool count",
            surface.host()
        );
    }
}
