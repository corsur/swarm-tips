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

impl Category {
    pub const ALL: [Self; 7] = [
        Self::Discovery,
        Self::Identity,
        Self::Messaging,
        Self::ShillbotEarn,
        Self::ShillbotClient,
        Self::Video,
        Self::Game,
    ];

    pub const fn slug(self) -> &'static str {
        match self {
            Self::Discovery => "discovery",
            Self::Identity => "identity",
            Self::Messaging => "messaging",
            Self::ShillbotEarn => "shillbot_earn",
            Self::ShillbotClient => "shillbot_client",
            Self::Video => "video",
            Self::Game => "game",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Economics {
    Free,
    Earn,
    Spend,
    Mixed,
}

impl Economics {
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Free => "free",
            Self::Earn => "earn",
            Self::Spend => "spend",
            Self::Mixed => "mixed",
        }
    }
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

pub const GATEWAY_TOOLS: &[&str] = &["swarm_capabilities", "swarm_use_capability"];

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
    let (category, economics) = if GATEWAY_TOOLS.contains(&name) {
        (
            Category::Discovery,
            if name == "swarm_capabilities" {
                Economics::Free
            } else {
                Economics::Mixed
            },
        )
    } else if DISCOVERY_TOOLS.contains(&name) {
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

pub fn requires_spend_ack(
    name: &str,
    arguments: Option<&serde_json::Map<String, serde_json::Value>>,
) -> bool {
    let Some(cap) = capability(name) else {
        return false;
    };
    match cap.economics {
        Economics::Spend => true,
        Economics::Mixed if name == "shillbot_submit_tx" => arguments
            .and_then(|a| a.get("action"))
            .and_then(serde_json::Value::as_str)
            .is_some_and(|a| matches!(a, "create" | "approve")),
        _ => false,
    }
}

pub fn categories() -> impl Iterator<Item = Category> {
    Category::ALL.into_iter()
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
    fn spend_ack_handles_mixed_submit_actions() {
        let args = |action| {
            serde_json::json!({"action": action})
                .as_object()
                .cloned()
                .unwrap()
        };
        assert!(requires_spend_ack("generate_video", None));
        assert!(!requires_spend_ack("game_get_leaderboard", None));
        assert!(!requires_spend_ack("xchain_supported_chains", None));
        assert!(requires_spend_ack("game_find_match", None));
        assert!(requires_spend_ack(
            "shillbot_submit_tx",
            Some(&args("create"))
        ));
        assert!(!requires_spend_ack(
            "shillbot_submit_tx",
            Some(&args("claim"))
        ));
        assert!(!requires_spend_ack("shillbot_claim_task", None));
    }
}
