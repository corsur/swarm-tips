//! Per-agent game session management for the MCP server.
//!
//! Manages WebSocket connections, chain clients, and chat buffers for each
//! agent playing the Coordination Game through MCP tools. Each registered
//! wallet gets a background WS listener that dispatches messages into the
//! session's buffers.
//!
//! ```text
//! MCP tool call
//!       │
//!       ▼
//! GameSessionManager
//! ├── sessions: HashMap<wallet, Arc<Mutex<GameSession>>>
//! ├── ws_sinks: HashMap<wallet, Arc<Mutex<WsSink>>>
//! └── chain_clients: HashMap<wallet, GameChainClient>
//!       │
//!       │  Background task per session:
//!       │  ws_listener reads WsStream → dispatches to session buffers
//!       │
//!       ▼
//! game-api (WS)  +  Solana RPC (on-chain)
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use futures_util::SinkExt;
use game_api_client::ws::{MatchFoundMsg, ServerMessage, WsConnection, WsSink};
use game_api_client::GameApiClient;
use game_chain::client::GameChainClient;
use serde::Serialize;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use tokio::sync::{Mutex, RwLock};

// ---------------------------------------------------------------------------
// Session state
// ---------------------------------------------------------------------------

/// Current phase of a game session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameSessionState {
    /// Authenticated, WS connected, ready to queue.
    Connected,
    /// In the matchmaking queue.
    Queued,
    /// Match found, game joined on-chain.
    Matched,
    /// Chat phase — game is active.
    InGame,
    /// Guess committed, waiting for opponent + reveal.
    Committed,
    /// Game resolved.
    Resolved,
}

/// Per-agent game session data, protected by `Mutex` for concurrent tool calls.
pub struct GameSession {
    pub wallet: String,
    pub jwt: String,
    pub state: GameSessionState,
    pub game_id: Option<u64>,
    pub tournament_id: Option<u64>,
    pub session_id: Option<String>,
    pub role: Option<u8>,
    pub chat_buffer: Vec<String>,
    pub match_found: Option<MatchFoundMsg>,
    pub game_ready: Option<u64>,
    pub reveal_data: Option<String>,
}

/// Status returned by `check_match`.
#[derive(Debug, Serialize)]
pub struct MatchStatus {
    pub status: String,
    pub game_id: Option<u64>,
    pub role: Option<u8>,
}

/// Outcome of `submit_guess`.
#[derive(Debug, Serialize)]
pub struct GuessOutcome {
    pub status: String,
    pub p1_guess: Option<u8>,
    pub p2_guess: Option<u8>,
}

/// Game result from on-chain state.
#[derive(Debug, Serialize)]
pub struct GameResult {
    pub status: String,
    pub game_id: u64,
    pub p1_guess: u8,
    pub p2_guess: u8,
    pub state: String,
}

// ---------------------------------------------------------------------------
// Manager
// ---------------------------------------------------------------------------

/// Manages all active game sessions. Thread-safe, shared across MCP tool calls.
pub struct GameSessionManager {
    sessions: RwLock<HashMap<String, Arc<Mutex<GameSession>>>>,
    ws_sinks: RwLock<HashMap<String, Arc<Mutex<WsSink>>>>,
    chain_clients: RwLock<HashMap<String, GameChainClient>>,
    game_api_url: String,
    solana_rpc_url: String,
}

impl GameSessionManager {
    pub fn new(game_api_url: String, solana_rpc_url: String) -> Self {
        assert!(!game_api_url.is_empty(), "game_api_url must not be empty");
        assert!(
            !solana_rpc_url.is_empty(),
            "solana_rpc_url must not be empty"
        );
        Self {
            sessions: RwLock::new(HashMap::new()),
            ws_sinks: RwLock::new(HashMap::new()),
            chain_clients: RwLock::new(HashMap::new()),
            game_api_url,
            solana_rpc_url,
        }
    }

    /// Check whether a session exists for the given wallet.
    pub async fn has_session(&self, wallet: &str) -> bool {
        self.sessions.read().await.contains_key(wallet)
    }

    /// Return the wallet pubkey of any active game session.
    ///
    /// Used to resolve the caller when MCP auth middleware is not yet wired
    /// up. Returns `None` if no sessions exist.
    pub async fn get_any_wallet(&self) -> Option<String> {
        self.sessions.read().await.keys().next().cloned()
    }

    /// Register an agent wallet: authenticate, connect WS, prepare chain client.
    ///
    /// Returns `(wallet_pubkey, balance_lamports)`.
    pub async fn register_wallet(&self, keypair_b58: &str) -> Result<(String, u64)> {
        // Decode the base58 secret key into a Keypair.
        let secret_bytes = bs58::decode(keypair_b58)
            .into_vec()
            .context("invalid base58 keypair")?;
        let keypair = Keypair::try_from(secret_bytes.as_slice())
            .map_err(|e| anyhow::anyhow!("keypair must be 64 bytes: {e}"))?;
        let wallet = keypair.pubkey().to_string();

        // Authenticate with game-api: challenge → sign → verify → JWT.
        let api_client = GameApiClient::new(&self.game_api_url)?;
        let challenge = api_client.request_challenge(&wallet).await?;
        let sig = keypair.sign_message(challenge.nonce.as_bytes());
        let verify_resp = api_client
            .verify_challenge(&wallet, &challenge.nonce, &sig.to_string())
            .await?;
        let jwt = verify_resp.token.clone();

        // Connect WebSocket.
        let ws = WsConnection::connect(&self.game_api_url, &jwt).await?;
        let (sink, stream) = ws.into_split();

        // Create chain client.
        let chain_client = GameChainClient::new(&self.solana_rpc_url, Arc::new(keypair));

        // Check balance.
        let balance = chain_client
            .rpc()
            .get_balance(&chain_client.pubkey())
            .await
            .context("failed to check wallet balance")?;

        // Build session.
        let session = Arc::new(Mutex::new(GameSession {
            wallet: wallet.clone(),
            jwt,
            state: GameSessionState::Connected,
            game_id: None,
            tournament_id: None,
            session_id: None,
            role: None,
            chat_buffer: Vec::new(),
            match_found: None,
            game_ready: None,
            reveal_data: None,
        }));

        // Spawn background WS listener.
        let session_clone = Arc::clone(&session);
        tokio::spawn(async move {
            ws_listener(session_clone, stream).await;
        });

        // Store session, sink, and chain client.
        self.sessions.write().await.insert(wallet.clone(), session);
        self.ws_sinks
            .write()
            .await
            .insert(wallet.clone(), Arc::new(Mutex::new(sink)));
        self.chain_clients
            .write()
            .await
            .insert(wallet.clone(), chain_client);

        tracing::info!(
            service = "coordination-mcp-server",
            wallet = %wallet,
            balance,
            "game session registered"
        );

        Ok((wallet, balance))
    }

    /// Deposit stake, clear stale queue entry, and join the matchmaking queue.
    pub async fn find_match(&self, wallet: &str, tournament_id: u64) -> Result<()> {
        let chain = self.chain_clients.read().await;
        let chain_client = chain.get(wallet).context("no session for wallet")?;

        let session = self
            .sessions
            .read()
            .await
            .get(wallet)
            .context("no session for wallet")?
            .clone();

        let jwt = session.lock().await.jwt.clone();

        // Deposit stake (idempotent if already deposited).
        chain_client.deposit_stake(tournament_id).await?;

        // Clear stale queue entry from previous crash.
        let api_client = GameApiClient::new(&self.game_api_url)?;
        if let Err(e) = api_client.leave_queue(&jwt, tournament_id).await {
            tracing::warn!(
                service = "coordination-mcp-server",
                error = %e,
                "leave_queue failed (ignoring)"
            );
        }

        // Join queue.
        let request = game_api_client::QueueJoinRequest {
            tournament_id,
            is_ai: true,
            agent_version: "mcp-agent/v1",
            is_internal: false, // external agents are NOT internal
        };
        api_client.join_queue(&jwt, &request).await?;

        // Update session state.
        let mut s = session.lock().await;
        s.state = GameSessionState::Queued;
        s.tournament_id = Some(tournament_id);

        tracing::info!(
            service = "coordination-mcp-server",
            wallet = %wallet,
            tournament_id,
            "joined matchmaking queue"
        );

        Ok(())
    }

    /// Check if match has been found. If matched, join the game on-chain.
    pub async fn check_match(&self, wallet: &str) -> Result<MatchStatus> {
        let session = self
            .sessions
            .read()
            .await
            .get(wallet)
            .context("no session for wallet")?
            .clone();

        let mut s = session.lock().await;

        // Still waiting for match.
        if s.match_found.is_none() {
            return Ok(MatchStatus {
                status: "queued".to_string(),
                game_id: None,
                role: None,
            });
        }

        // Match found but not yet joined.
        if s.state == GameSessionState::Queued {
            let (sid, role) = {
                let mf = s.match_found.as_ref().context("match_found missing")?;
                (mf.session_id.clone(), mf.role)
            };
            s.session_id = Some(sid);
            s.role = Some(role);
            s.state = GameSessionState::Matched;
        }

        // Wait for game_ready if we haven't received it yet.
        if s.game_ready.is_none() {
            // Drop the lock briefly, then re-acquire after a short wait.
            drop(s);
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            s = session.lock().await;
        }

        // If game_ready arrived, join the game on-chain.
        if let Some(game_id) = s.game_ready {
            if s.state == GameSessionState::Matched {
                let tournament_id = s.tournament_id.context("tournament_id not set")?;
                let session_id = s.session_id.clone().context("session_id not set")?;
                let jwt = s.jwt.clone();

                // Drop lock before doing I/O.
                drop(s);

                let chain = self.chain_clients.read().await;
                let chain_client = chain.get(wallet).context("no chain client")?;
                chain_client.join_game(game_id, tournament_id).await?;

                // Notify backend that we joined.
                let api_client = GameApiClient::new(&self.game_api_url)?;
                api_client
                    .post_games_joined(&jwt, game_id, &session_id)
                    .await?;

                // Update state.
                let mut s = session.lock().await;
                s.game_id = Some(game_id);
                s.state = GameSessionState::InGame;

                tracing::info!(
                    service = "coordination-mcp-server",
                    wallet = %wallet,
                    game_id,
                    "joined on-chain game"
                );

                return Ok(MatchStatus {
                    status: "in_game".to_string(),
                    game_id: Some(game_id),
                    role: s.role,
                });
            }
        }

        Ok(MatchStatus {
            status: match s.state {
                GameSessionState::InGame => "in_game".to_string(),
                GameSessionState::Matched => "matched".to_string(),
                _ => "queued".to_string(),
            },
            game_id: s.game_id,
            role: s.role,
        })
    }

    /// Send a chat message to the opponent via WebSocket.
    pub async fn send_message(&self, wallet: &str, text: &str) -> Result<()> {
        let sinks = self.ws_sinks.read().await;
        let sink = sinks.get(wallet).context("no WS sink for wallet")?;

        #[derive(Serialize)]
        struct ChatMsg<'a> {
            #[serde(rename = "type")]
            kind: &'static str,
            text: &'a str,
        }
        let json =
            serde_json::to_string(&ChatMsg { kind: "chat", text }).context("serialize chat")?;
        sink.lock()
            .await
            .send(game_api_client::ws::text_message(json))
            .await
            .context("WebSocket send failed")?;
        Ok(())
    }

    /// Drain and return all chat messages received since the last call.
    pub async fn get_messages(&self, wallet: &str) -> Result<Vec<String>> {
        let sessions = self.sessions.read().await;
        let session = sessions.get(wallet).context("no session for wallet")?;
        let mut s = session.lock().await;
        let msgs = std::mem::take(&mut s.chat_buffer);
        Ok(msgs)
    }

    /// Commit guess on-chain, wait for reveal data, reveal on-chain.
    ///
    /// Returns the guess outcome once the game resolves (or after reveal).
    pub async fn submit_guess(&self, wallet: &str, guess: u8) -> Result<GuessOutcome> {
        assert!(guess <= 1, "guess must be 0 (same) or 1 (different)");

        let session = self
            .sessions
            .read()
            .await
            .get(wallet)
            .context("no session for wallet")?
            .clone();

        let (game_id, tournament_id) = {
            let mut s = session.lock().await;
            let gid = s.game_id.context("no active game")?;
            let tid = s.tournament_id.context("tournament_id not set")?;
            s.state = GameSessionState::Committed;
            (gid, tid)
        };

        // Commit on-chain.
        let chain = self.chain_clients.read().await;
        let chain_client = chain.get(wallet).context("no chain client")?;
        let preimage = chain_client.submit_commit(game_id, guess).await?;

        tracing::info!(
            service = "coordination-mcp-server",
            wallet = %wallet,
            game_id,
            guess,
            "committed guess on-chain"
        );

        // Wait for reveal_data from the WS listener (up to 5 minutes).
        let r_matchup_hex = self.poll_reveal_data(wallet, 300).await?;
        let r_matchup_bytes = hex::decode(&r_matchup_hex).context("invalid hex in r_matchup")?;
        let r_matchup: [u8; 32] = r_matchup_bytes
            .try_into()
            .map_err(|v: Vec<u8>| anyhow::anyhow!("r_matchup must be 32 bytes, got {}", v.len()))?;

        // Reveal on-chain and wait for resolution.
        let (p1_guess, p2_guess) = chain_client
            .wait_and_reveal(game_id, tournament_id, preimage, Some(r_matchup))
            .await?;

        // Update session state.
        {
            let mut s = session.lock().await;
            s.state = GameSessionState::Resolved;
        }

        tracing::info!(
            service = "coordination-mcp-server",
            wallet = %wallet,
            game_id,
            p1_guess,
            p2_guess,
            "game resolved"
        );

        Ok(GuessOutcome {
            status: "resolved".to_string(),
            p1_guess: Some(p1_guess),
            p2_guess: Some(p2_guess),
        })
    }

    /// Read on-chain game state.
    pub async fn get_result(&self, wallet: &str) -> Result<GameResult> {
        let session = self
            .sessions
            .read()
            .await
            .get(wallet)
            .context("no session for wallet")?
            .clone();

        let s = session.lock().await;
        let game_id = s.game_id.context("no active game")?;
        drop(s);

        let chain = self.chain_clients.read().await;
        let chain_client = chain.get(wallet).context("no chain client")?;
        let game = chain_client
            .read_game(game_id)
            .await?
            .context("game account not found")?;

        Ok(GameResult {
            status: "ok".to_string(),
            game_id,
            p1_guess: game.p1_guess,
            p2_guess: game.p2_guess,
            state: format!("{:?}", game.state),
        })
    }

    /// Remove a session and clean up resources.
    pub async fn cleanup(&self, wallet: &str) {
        self.sessions.write().await.remove(wallet);
        self.ws_sinks.write().await.remove(wallet);
        self.chain_clients.write().await.remove(wallet);
        tracing::info!(
            service = "coordination-mcp-server",
            wallet = %wallet,
            "game session cleaned up"
        );
    }

    // -- internal helpers --

    /// Poll the session's `reveal_data` buffer until it's populated.
    async fn poll_reveal_data(&self, wallet: &str, timeout_secs: u64) -> Result<String> {
        let session = self
            .sessions
            .read()
            .await
            .get(wallet)
            .context("no session for wallet")?
            .clone();

        let deadline = tokio::time::Instant::now()
            .checked_add(tokio::time::Duration::from_secs(timeout_secs))
            .context("deadline overflow")?;

        loop {
            {
                let s = session.lock().await;
                if let Some(ref rd) = s.reveal_data {
                    return Ok(rd.clone());
                }
            }
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            anyhow::ensure!(
                !remaining.is_zero(),
                "timed out waiting for reveal_data after {timeout_secs}s"
            );
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
    }
}

// ---------------------------------------------------------------------------
// Background WS listener
// ---------------------------------------------------------------------------

/// Consumes the WS read stream, dispatching messages to session buffers.
async fn ws_listener(session: Arc<Mutex<GameSession>>, mut stream: game_api_client::ws::WsStream) {
    use futures_util::StreamExt;
    use game_api_client::ws::WsMessage;

    loop {
        let frame = match stream.next().await {
            Some(Ok(frame)) => frame,
            Some(Err(e)) => {
                tracing::warn!(
                    service = "coordination-mcp-server",
                    error = %e,
                    "game WS read error, closing listener"
                );
                break;
            }
            None => break,
        };

        match frame {
            WsMessage::Text(text) => {
                let msg = game_api_client::ws::parse_server_message(&text);
                let mut s = session.lock().await;
                match msg {
                    ServerMessage::MatchFound { session_id, role } => {
                        s.match_found = Some(MatchFoundMsg { session_id, role });
                    }
                    ServerMessage::GameReady { game_id } => {
                        s.game_ready = Some(game_id);
                    }
                    ServerMessage::RevealData { r_matchup } => {
                        s.reveal_data = Some(r_matchup);
                    }
                    ServerMessage::Chat { text } => {
                        s.chat_buffer.push(text);
                    }
                    ServerMessage::Unknown => {}
                }
            }
            WsMessage::Ping(_)
            | WsMessage::Pong(_)
            | WsMessage::Binary(_)
            | WsMessage::Frame(_) => {}
            WsMessage::Close(_) => break,
        }
    }

    tracing::info!(
        service = "coordination-mcp-server",
        "game WS listener ended"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_state_transitions() {
        // Verify the state enum values are distinct.
        assert_ne!(GameSessionState::Connected, GameSessionState::Queued);
        assert_ne!(GameSessionState::Queued, GameSessionState::Matched);
        assert_ne!(GameSessionState::InGame, GameSessionState::Resolved);
    }

    #[test]
    fn manager_new_rejects_empty_urls() {
        let result = std::panic::catch_unwind(|| {
            GameSessionManager::new(String::new(), "https://rpc".to_string())
        });
        assert!(result.is_err());
    }

    #[test]
    fn match_status_serialization() {
        let status = MatchStatus {
            status: "queued".to_string(),
            game_id: None,
            role: None,
        };
        let json = serde_json::to_string(&status).expect("serialize");
        assert!(json.contains("queued"));
    }

    #[test]
    fn guess_outcome_serialization() {
        let outcome = GuessOutcome {
            status: "resolved".to_string(),
            p1_guess: Some(0),
            p2_guess: Some(1),
        };
        let json = serde_json::to_string(&outcome).expect("serialize");
        assert!(json.contains("resolved"));
        assert!(json.contains("p1_guess"));
    }
}
