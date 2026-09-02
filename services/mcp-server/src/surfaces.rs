//! Host-specific MCP product surfaces.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Surface {
    Swarm,
    Shillbot,
    Game,
}

impl Surface {
    pub const ALL: [Self; 3] = [Self::Swarm, Self::Shillbot, Self::Game];

    pub fn from_host(host: &str) -> Self {
        let forwarded = host.split(',').next().unwrap_or(host).trim();
        let host = forwarded
            .split(':')
            .next()
            .unwrap_or(forwarded)
            .to_ascii_lowercase();
        match host.as_str() {
            "mcp.shillbot.org" => Self::Shillbot,
            "mcp.coordination.game" => Self::Game,
            _ => Self::Swarm,
        }
    }

    pub const fn host(self) -> &'static str {
        match self {
            Self::Swarm => "mcp.swarm.tips",
            Self::Shillbot => "mcp.shillbot.org",
            Self::Game => "mcp.coordination.game",
        }
    }

    pub const fn server_name(self) -> &'static str {
        match self {
            Self::Swarm => "swarm-tips",
            Self::Shillbot => "shillbot",
            Self::Game => "coordination-game",
        }
    }

    pub const fn registry_name(self) -> &'static str {
        match self {
            Self::Swarm => "io.github.corsur/swarm-tips",
            Self::Shillbot => "io.github.corsur/shillbot",
            Self::Game => "io.github.corsur/coordination-game",
        }
    }

    pub const fn title(self) -> &'static str {
        match self {
            Self::Swarm => "Swarm Tips MCP",
            Self::Shillbot => "Shillbot MCP",
            Self::Game => "Coordination Game MCP",
        }
    }

    pub const fn mcp_url(self) -> &'static str {
        match self {
            Self::Swarm => "https://mcp.swarm.tips/mcp",
            Self::Shillbot => "https://mcp.shillbot.org/mcp",
            Self::Game => "https://mcp.coordination.game/mcp",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::Swarm => "Free tools and earning capabilities for agents.",
            Self::Shillbot => "Shillbot earning, client, and paid-video capabilities.",
            Self::Game => "Coordination Game capabilities.",
        }
    }

    pub fn related(self) -> impl Iterator<Item = Self> {
        Self::ALL
            .into_iter()
            .filter(move |candidate| *candidate != self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_hosts_select_their_surface_and_unknown_hosts_fail_safe_to_swarm() {
        assert_eq!(Surface::from_host("mcp.swarm.tips"), Surface::Swarm);
        assert_eq!(
            Surface::from_host("mcp.shillbot.org:443"),
            Surface::Shillbot
        );
        assert_eq!(Surface::from_host("MCP.COORDINATION.GAME"), Surface::Game);
        assert_eq!(Surface::from_host("localhost:8080"), Surface::Swarm);
    }

    #[test]
    fn every_surface_advertises_the_other_two() {
        for surface in Surface::ALL {
            let related: Vec<_> = surface.related().collect();
            assert_eq!(related.len(), 2);
            assert!(!related.contains(&surface));
            assert!(related.iter().all(|item| item.mcp_url().ends_with("/mcp")));
        }
    }
}
