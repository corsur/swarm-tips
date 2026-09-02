//! The single source of truth for tool economics, product ownership and gates.

use crate::surfaces::Surface;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Discovery,
    Identity,
    Messaging,
    ShillbotEarn,
    ShillbotClient,
    Video,
    Game,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Economics {
    Free,
    Earn,
    Spend,
    Mixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    Public,
    Testnet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capability {
    pub category: Category,
    pub economics: Economics,
    pub gate: Gate,
}

pub const TESTNET_GAME_TOOLS: &[&str] = &[
    "xchain_supported_chains",
    "xchain_find_match",
    "xchain_match_status",
    "xchain_build_create_match",
    "xchain_build_create_xmatch",
    "xchain_build_lock",
    "xchain_build_lock_xmatch",
    "xchain_build_refund",
    "xchain_build_refund_xmatch",
    "xchain_build_settle",
    "xchain_commit_guess",
    "xchain_sign_checkpoint",
    "xchain_reveal_guess",
    "xchain_gameplay_status",
    "game_find_evm_match",
    "game_evm_match_status",
    "game_evm_committed",
    "game_evm_commit_guess",
    "game_evm_reveal_guess",
];

const SHILLBOT_CLIENT_TOOLS: &[&str] = &[
    "shillbot_create_campaign",
    "shillbot_list_pending_approval",
    "shillbot_approve_task",
    "shillbot_reject_task",
];

const DISCOVERY_TOOLS: &[&str] = &[
    "list_earning_opportunities",
    "list_spending_opportunities",
    "discover_opportunities",
    "search_mcp_servers",
];

const IDENTITY_TOOLS: &[&str] = &[
    "register_wallet",
    "agent_profile",
    "agent_trust_score",
    "agent_reputation_leaderboard",
    "query_agent_credit_web_score",
    "list_extensions",
];

/// Game reads are free capabilities even though the surrounding gameplay
/// vertical can stake funds. Economics describe the individual tool call, not
/// the most expensive workflow in the same category.
const FREE_GAME_TOOLS: &[&str] = &[
    "game_get_leaderboard",
    "game_check_match",
    "game_get_messages",
    "game_get_result",
    "game_evm_match_status",
    "xchain_supported_chains",
    "xchain_match_status",
    "xchain_gameplay_status",
];

pub fn capability(name: &str) -> Option<Capability> {
    let (category, economics) = if DISCOVERY_TOOLS.contains(&name) {
        (Category::Discovery, Economics::Free)
    } else if IDENTITY_TOOLS.contains(&name) {
        (Category::Identity, Economics::Free)
    } else if name.starts_with("agent_") || name.starts_with("topic_") || name.ends_with("webhook")
    {
        (Category::Messaging, Economics::Free)
    } else if name == "generate_video" || name == "check_video_status" {
        (
            Category::Video,
            if name == "generate_video" {
                Economics::Spend
            } else {
                Economics::Free
            },
        )
    } else if name.starts_with("game_") || name.starts_with("xchain_") {
        (
            Category::Game,
            if FREE_GAME_TOOLS.contains(&name) {
                Economics::Free
            } else {
                Economics::Spend
            },
        )
    } else if name.starts_with("shillbot_") {
        if SHILLBOT_CLIENT_TOOLS.contains(&name) {
            (
                Category::ShillbotClient,
                if name == "shillbot_create_campaign" || name == "shillbot_approve_task" {
                    Economics::Spend
                } else {
                    Economics::Free
                },
            )
        } else {
            (
                Category::ShillbotEarn,
                if name == "shillbot_submit_tx" {
                    Economics::Mixed
                } else {
                    Economics::Earn
                },
            )
        }
    } else {
        return None;
    };
    Some(Capability {
        category,
        economics,
        gate: if TESTNET_GAME_TOOLS.contains(&name) {
            Gate::Testnet
        } else {
            Gate::Public
        },
    })
}

pub fn listed_on(name: &str, surface: Surface, show_testnet: bool) -> bool {
    let Some(cap) = capability(name) else {
        return false;
    };
    if cap.gate == Gate::Testnet && !show_testnet {
        return false;
    }
    match surface {
        Surface::Swarm => !matches!(
            cap.category,
            Category::Game | Category::Video | Category::ShillbotClient
        ),
        Surface::Shillbot => {
            matches!(
                cap.category,
                Category::ShillbotEarn | Category::ShillbotClient | Category::Video
            ) || name == "register_wallet"
        }
        Surface::Game => cap.category == Category::Game || name == "register_wallet",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn testnet_gate_is_confined_to_the_game_surface() {
        for name in TESTNET_GAME_TOOLS {
            let cap = capability(name).expect("gated tool registered");
            assert_eq!(cap.category, Category::Game);
            assert!(!listed_on(name, Surface::Game, false));
            assert!(listed_on(name, Surface::Game, true));
            assert!(!listed_on(name, Surface::Swarm, true));
        }
    }

    #[test]
    fn hidden_capabilities_remain_classified_for_exact_name_calls() {
        assert_eq!(
            capability("generate_video").map(|cap| (cap.category, cap.economics)),
            Some((Category::Video, Economics::Spend))
        );
        assert_eq!(
            capability("game_get_leaderboard").map(|cap| (cap.category, cap.economics)),
            Some((Category::Game, Economics::Free))
        );
    }
}
