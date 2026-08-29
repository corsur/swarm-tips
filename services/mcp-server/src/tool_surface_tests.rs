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

use crate::server::{filter_visible_tools, SwarmTipsMcp, HIDDEN_UNTIL_MAINNET, INSTRUCTIONS};
use rmcp::model::Tool;
use std::collections::{BTreeMap, BTreeSet};

const SNAPSHOT_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tool-surface.snapshot.json");

/// Total budget for the default-visible surface: serialized tools + the
/// INSTRUCTIONS blob, estimated at chars/4. Ratchet — lower it, never raise
/// it. chars/4 overestimates vs a real tokenizer (~18.3k here ≈ 13.3k
/// tiktoken live), so the Phase-3 target of ≤8.8k real tokens lands around
/// 12_100 in this unit.
const TOTAL_TOKEN_RATCHET: usize = 18_400;

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

fn visible_tools() -> Vec<Tool> {
    filter_visible_tools(all_tools(), false)
}

fn tool_json(tool: &Tool) -> serde_json::Value {
    serde_json::to_value(tool).expect("Tool serializes")
}

fn surface_json() -> serde_json::Value {
    serde_json::Value::Array(all_tools().iter().map(tool_json).collect())
}

// -- snapshot ---------------------------------------------------------------

/// The full declared surface (all 66 tools — hidden ones are callable and
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
    let visible_owned = visible_tools();
    let visible: Vec<&str> = visible_owned.iter().map(|t| t.name.as_ref()).collect();
    let expected_visible = [
        "agent_ack_messages",
        "agent_get_messages",
        "agent_mute_thread",
        "agent_profile",
        "agent_reputation_leaderboard",
        "agent_send_message",
        "agent_trust_score",
        "agent_verify_wallet",
        "check_video_status",
        "delete_webhook",
        "discover_opportunities",
        "game_check_match",
        "game_commit_guess",
        "game_find_match",
        "game_get_leaderboard",
        "game_get_messages",
        "game_get_result",
        "game_reveal_guess",
        "game_send_message",
        "game_submit_tx",
        "generate_video",
        "get_webhook",
        "list_earning_opportunities",
        "list_extensions",
        "list_spending_opportunities",
        "query_agent_credit_web_score",
        "register_wallet",
        "register_webhook",
        "search_mcp_servers",
        "shillbot_approve_task",
        "shillbot_check_earnings",
        "shillbot_claim_task",
        "shillbot_complete_task",
        "shillbot_create_campaign",
        "shillbot_finalize_task",
        "shillbot_get_attestation",
        "shillbot_get_task_details",
        "shillbot_list_available_tasks",
        "shillbot_list_pending_approval",
        "shillbot_onboard",
        "shillbot_reject_task",
        "shillbot_submit_tx",
        "shillbot_submit_work",
        "shillbot_verify_task",
        "topic_publish",
        "topic_read",
        "topic_report",
    ];
    assert_eq!(visible, expected_visible, "default-visible tool names");

    // Declared = visible + the hidden testnet list, no overlap, nothing else.
    let declared: BTreeSet<String> = all_tools().iter().map(|t| t.name.to_string()).collect();
    let mut expected_declared: BTreeSet<String> =
        expected_visible.iter().map(|s| s.to_string()).collect();
    for hidden in HIDDEN_UNTIL_MAINNET {
        assert!(
            expected_declared.insert((*hidden).to_string()),
            "{hidden} is both visible and hidden"
        );
    }
    assert_eq!(declared, expected_declared, "declared tool names");
}

// -- token budget -----------------------------------------------------------

/// What one agent pays per connection: every visible tool's serialized JSON
/// plus INSTRUCTIONS, at the chars/4 estimate the live e2e uses.
#[test]
fn visible_surface_fits_the_token_ratchet() {
    let tools_chars: usize = visible_tools()
        .iter()
        .map(|t| {
            serde_json::to_string(&tool_json(t))
                .expect("serialize")
                .len()
        })
        .sum();
    let total = tools_chars.div_ceil(4) + approx_tokens(INSTRUCTIONS);
    assert!(
        total <= TOTAL_TOKEN_RATCHET,
        "visible surface is ~{total} tokens (ratchet {TOTAL_TOKEN_RATCHET}). \
         Trim before adding — this ceiling only goes down."
    );
}

/// Per-description cap. Known oversized descriptions are grandfathered at
/// their current size — they may shrink, never grow; new tools get 200.
#[test]
fn descriptions_fit_their_caps() {
    // name -> grandfathered cap (current measured size). Phase 3 empties this.
    let grandfathered: BTreeMap<&str, usize> = [
        ("agent_get_messages", 320),
        ("agent_send_message", 560),
        ("agent_verify_wallet", 400),
        ("game_evm_commit_guess", 230),
        ("game_evm_committed", 230),
        ("game_evm_reveal_guess", 260),
        ("game_find_evm_match", 230),
        ("generate_video", 350),
        ("list_earning_opportunities", 340),
        ("register_wallet", 460),
        ("register_webhook", 300),
        ("search_mcp_servers", 300),
        ("shillbot_complete_task", 300),
        ("shillbot_create_campaign", 280),
        ("shillbot_get_attestation", 400),
        ("shillbot_onboard", 300),
        ("shillbot_reject_task", 260),
        ("topic_publish", 300),
        ("xchain_build_create_match", 260),
        ("xchain_build_create_xmatch", 260),
        ("xchain_build_lock", 300),
        ("xchain_build_lock_xmatch", 260),
        ("xchain_build_refund", 230),
        ("xchain_build_settle", 340),
        ("xchain_find_match", 260),
        ("xchain_supported_chains", 230),
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
    let known: BTreeSet<&str> = [
        "ClaimTaskArgs",
        "EvmCommittedArgs",
        "NetworkOnlyArgs",
        "WebhookManageArgs",
        "XchainBuildCreateMatchArgs",
        "XchainBuildRefundArgs",
        "XchainGameplayArgs",
        // Six tools publish NO title at all (schemars emits none for their
        // arg shapes) — as misleading as a shared one. Phase 3 fixes.
        "",
    ]
    .into_iter()
    .collect();

    let mut by_title: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for tool in all_tools() {
        let title = tool_json(&tool)["inputSchema"]["title"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        by_title
            .entry(title)
            .or_default()
            .push(tool.name.to_string());
    }
    let new_collisions: Vec<_> = by_title
        .iter()
        .filter(|(title, tools)| tools.len() > 1 && !known.contains(title.as_str()))
        .collect();
    assert!(
        new_collisions.is_empty(),
        "new schema-title collisions (give each tool's arg struct its own title): {new_collisions:?}"
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

/// INSTRUCTIONS parity, both directions: every visible tool is mentioned by
/// name, and every count the prose states matches the computed surface. This
/// runs pre-deploy — the shell e2e re-checks the same contract live.
#[test]
fn instructions_name_every_visible_tool_and_counts_match() {
    let visible = visible_tools();
    let missing: Vec<_> = visible
        .iter()
        .map(|t| t.name.as_ref())
        .filter(|name: &&str| !INSTRUCTIONS.contains(*name))
        .collect();
    assert!(
        missing.is_empty(),
        "visible tools absent from INSTRUCTIONS: {missing:?}"
    );

    let count_phrases = [
        format!("exposes {} tools", visible.len()),
        format!("all {} tools", visible.len()),
    ];
    for phrase in &count_phrases {
        assert!(
            INSTRUCTIONS.contains(phrase.as_str()),
            "INSTRUCTIONS count drifted: expected the phrase {phrase:?} \
             (visible surface is {} tools)",
            visible.len()
        );
    }
}
