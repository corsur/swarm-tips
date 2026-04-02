//! WebSocket client for the coordination game backend.
//!
//! Provides typed message parsing, ping/pong handling, and timeout-bounded
//! waiting for specific game events. Extracted from the grok-agent so that
//! both the grok-agent and the MCP server can share the same battle-tested
//! WebSocket integration.

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// Re-export `Message` so downstream crates don't need a direct `tokio-tungstenite` dep.
pub use tokio_tungstenite::tungstenite::Message as WsMessage;

/// Create a WebSocket text message. Convenience wrapper so callers don't
/// need to import `tungstenite::Message` directly.
pub fn text_message(text: String) -> Message {
    Message::Text(text)
}

/// Type alias for the write half of a WebSocket connection.
pub type WsSink = futures_util::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    Message,
>;

/// Type alias for the read half of a WebSocket connection.
pub type WsStream = futures_util::stream::SplitStream<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
>;

// ---------------------------------------------------------------------------
// Message types
// ---------------------------------------------------------------------------

pub struct MatchFoundMsg {
    pub session_id: String,
    pub role: u8,
}

pub struct RevealDataMsg {
    pub r_matchup: String,
}

/// Unified enum for all server-to-client WebSocket messages.
/// Used by the MCP server's background listener to dispatch messages
/// into per-session buffers without needing the typed `wait_for_*` methods.
#[derive(Debug, Clone)]
pub enum ServerMessage {
    MatchFound { session_id: String, role: u8 },
    GameReady { game_id: u64 },
    RevealData { r_matchup: String },
    Chat { text: String },
    Unknown,
}

// ---------------------------------------------------------------------------
// Connection
// ---------------------------------------------------------------------------

pub struct WsConnection {
    sink: WsSink,
    stream: WsStream,
}

/// Build a WebSocket URL from an HTTP base URL and JWT token.
///
/// Converts `http://` to `ws://` and `https://` to `wss://`, strips trailing
/// slashes, and appends `/ws?token={jwt}`.
fn build_ws_url(base_url: &str, jwt: &str) -> String {
    let ws_url = base_url
        .replacen("http://", "ws://", 1)
        .replacen("https://", "wss://", 1);
    format!("{}/ws?token={}", ws_url.trim_end_matches('/'), jwt)
}

/// Parse a raw WebSocket text frame into a `ServerMessage`.
pub fn parse_server_message(text: &str) -> ServerMessage {
    #[derive(Deserialize)]
    #[serde(tag = "type", rename_all = "snake_case")]
    enum RawMsg {
        MatchFound {
            session_id: String,
            role: u8,
        },
        GameReady {
            game_id: u64,
        },
        RevealData {
            r_matchup: String,
        },
        Chat {
            text: String,
        },
        #[serde(other)]
        Other,
    }

    match serde_json::from_str::<RawMsg>(text) {
        Ok(RawMsg::MatchFound { session_id, role }) => {
            ServerMessage::MatchFound { session_id, role }
        }
        Ok(RawMsg::GameReady { game_id }) => ServerMessage::GameReady { game_id },
        Ok(RawMsg::RevealData { r_matchup }) => ServerMessage::RevealData { r_matchup },
        Ok(RawMsg::Chat { text }) => ServerMessage::Chat { text },
        Ok(RawMsg::Other) | Err(_) => ServerMessage::Unknown,
    }
}

impl WsConnection {
    pub async fn connect(base_url: &str, jwt: &str) -> Result<Self> {
        let url = build_ws_url(base_url, jwt);
        let (ws_stream, _) = connect_async(&url)
            .await
            .context("WebSocket connect failed")?;
        let (sink, stream) = ws_stream.split();
        Ok(Self { sink, stream })
    }

    /// Split the connection into independent sink and stream halves.
    ///
    /// The sink can be wrapped in `Arc<Mutex<_>>` for shared chat sending,
    /// while the stream drives a background listener task. This is the
    /// preferred pattern for the MCP server's game session manager.
    pub fn into_split(self) -> (WsSink, WsStream) {
        (self.sink, self.stream)
    }

    /// Receive the next parsed server message, handling ping/pong internally.
    ///
    /// Returns `ServerMessage::Unknown` for unrecognized message types.
    /// Returns an error if the WebSocket stream closes or encounters an error.
    pub async fn recv_next(&mut self) -> Result<ServerMessage> {
        loop {
            let raw = self
                .stream
                .next()
                .await
                .context("WebSocket stream ended")?
                .context("WebSocket read error")?;
            match raw {
                Message::Text(text) => return Ok(parse_server_message(&text)),
                Message::Ping(data) => {
                    self.sink
                        .send(Message::Pong(data))
                        .await
                        .context("pong send failed")?;
                }
                Message::Close(_) => anyhow::bail!("WebSocket closed by server"),
                _ => {}
            }
        }
    }

    /// Block until a `match_found` server message arrives, or timeout.
    ///
    /// Times out after 10 minutes — far longer than any realistic queue wait,
    /// but provides a definite upper bound so slots cannot hang forever.
    pub async fn wait_for_match_found(&mut self) -> Result<MatchFoundMsg> {
        let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(600);

        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            anyhow::ensure!(
                !remaining.is_zero(),
                "timed out waiting for match_found after 10m"
            );
            let msg = tokio::time::timeout(remaining, self.recv_next())
                .await
                .context("timeout waiting for match_found")??;
            if let ServerMessage::MatchFound { session_id, role } = msg {
                anyhow::ensure!(!session_id.is_empty(), "match_found session_id is empty");
                return Ok(MatchFoundMsg { session_id, role });
            }
        }
    }

    /// Block until a `game_ready` server message arrives, or timeout.
    ///
    /// Times out after 5 minutes — enough time for P1 to submit create_game on-chain.
    pub async fn wait_for_game_ready(&mut self) -> Result<u64> {
        let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(300);

        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            anyhow::ensure!(
                !remaining.is_zero(),
                "timed out waiting for game_ready after 5m"
            );
            let msg = tokio::time::timeout(remaining, self.recv_next())
                .await
                .context("timeout waiting for game_ready")??;
            if let ServerMessage::GameReady { game_id } = msg {
                anyhow::ensure!(game_id > 0, "game_ready game_id must be non-zero");
                return Ok(game_id);
            }
        }
    }

    /// Block until a `reveal_data` server message arrives, or timeout.
    ///
    /// The backend sends this after both players have committed their guesses.
    /// It contains the hex-encoded `r_matchup` preimage needed for the reveal
    /// instruction. Times out after 5 minutes.
    pub async fn wait_for_reveal_data(&mut self) -> Result<RevealDataMsg> {
        let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(300);

        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            anyhow::ensure!(
                !remaining.is_zero(),
                "timed out waiting for reveal_data after 5m"
            );
            let msg = tokio::time::timeout(remaining, self.recv_next())
                .await
                .context("timeout waiting for reveal_data")??;
            if let ServerMessage::RevealData { r_matchup } = msg {
                anyhow::ensure!(!r_matchup.is_empty(), "reveal_data r_matchup is empty");
                return Ok(RevealDataMsg { r_matchup });
            }
        }
    }

    /// Block until the next incoming chat message.
    pub async fn wait_for_chat(&mut self) -> Result<String> {
        loop {
            let msg = self.recv_next().await?;
            if let ServerMessage::Chat { text } = msg {
                return Ok(text);
            }
        }
    }

    /// Send a chat message to the server.
    pub async fn send_chat(&mut self, text: &str) -> Result<()> {
        #[derive(Serialize)]
        struct ChatMsg<'a> {
            #[serde(rename = "type")]
            kind: &'static str,
            text: &'a str,
        }
        let json =
            serde_json::to_string(&ChatMsg { kind: "chat", text }).context("serialize chat")?;
        self.sink
            .send(Message::Text(json))
            .await
            .context("WebSocket send failed")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // build_ws_url tests
    // -----------------------------------------------------------------------

    #[test]
    fn build_ws_url_converts_http_to_ws() {
        let url = build_ws_url("http://localhost:8080", "my-jwt");
        assert_eq!(url, "ws://localhost:8080/ws?token=my-jwt");
    }

    #[test]
    fn build_ws_url_converts_https_to_wss() {
        let url = build_ws_url("https://api.example.com", "tok123");
        assert_eq!(url, "wss://api.example.com/ws?token=tok123");
    }

    #[test]
    fn build_ws_url_strips_trailing_slash() {
        let url = build_ws_url("http://localhost:8080/", "jwt");
        assert_eq!(url, "ws://localhost:8080/ws?token=jwt");
    }

    #[test]
    fn build_ws_url_strips_multiple_trailing_slashes() {
        let url = build_ws_url("https://api.example.com///", "jwt");
        assert_eq!(url, "wss://api.example.com/ws?token=jwt");
    }

    #[test]
    fn build_ws_url_preserves_path() {
        let url = build_ws_url("http://localhost:8080/api/v1", "jwt");
        assert_eq!(url, "ws://localhost:8080/api/v1/ws?token=jwt");
    }

    #[test]
    fn build_ws_url_only_replaces_first_http_occurrence() {
        let url = build_ws_url("http://host/http://other", "jwt");
        assert_eq!(url, "ws://host/http://other/ws?token=jwt");
    }

    #[test]
    fn build_ws_url_empty_jwt() {
        let url = build_ws_url("http://localhost:8080", "");
        assert_eq!(url, "ws://localhost:8080/ws?token=");
    }

    // -----------------------------------------------------------------------
    // parse_server_message tests
    // -----------------------------------------------------------------------

    #[test]
    fn parse_match_found() {
        let msg = parse_server_message(r#"{"type":"match_found","session_id":"abc123","role":1}"#);
        match msg {
            ServerMessage::MatchFound { session_id, role } => {
                assert_eq!(session_id, "abc123");
                assert_eq!(role, 1);
            }
            other => panic!("expected MatchFound, got {other:?}"),
        }
    }

    #[test]
    fn parse_game_ready() {
        let msg = parse_server_message(r#"{"type":"game_ready","game_id":42}"#);
        match msg {
            ServerMessage::GameReady { game_id } => assert_eq!(game_id, 42),
            other => panic!("expected GameReady, got {other:?}"),
        }
    }

    #[test]
    fn parse_reveal_data() {
        let msg = parse_server_message(r#"{"type":"reveal_data","r_matchup":"deadbeef"}"#);
        match msg {
            ServerMessage::RevealData { r_matchup } => assert_eq!(r_matchup, "deadbeef"),
            other => panic!("expected RevealData, got {other:?}"),
        }
    }

    #[test]
    fn parse_chat() {
        let msg = parse_server_message(r#"{"type":"chat","text":"hello","from":"opponent"}"#);
        match msg {
            ServerMessage::Chat { text } => assert_eq!(text, "hello"),
            other => panic!("expected Chat, got {other:?}"),
        }
    }

    #[test]
    fn parse_unknown_type() {
        let msg = parse_server_message(r#"{"type":"ping"}"#);
        assert!(matches!(msg, ServerMessage::Unknown));
    }

    #[test]
    fn parse_invalid_json() {
        let msg = parse_server_message("not json");
        assert!(matches!(msg, ServerMessage::Unknown));
    }
}
