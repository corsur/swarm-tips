//! Host-specific MCP product surfaces.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Surface {
    Swarm,
    Shillbot,
    Game,
}

impl Surface {
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
}
