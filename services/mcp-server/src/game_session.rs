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
    pub matchup_commitment: Option<String>,
    pub commit_preimage: Option<[u8; 32]>,
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
            matchup_commitment: None,
            commit_preimage: None,
            game_ready: None,
            reveal_data: None,
        }));

        let ws_sink = Arc::new(Mutex::new(sink));

        // Spawn background WS listener with reconnect capability.
        let session_clone = Arc::clone(&session);
        let sink_clone = Arc::clone(&ws_sink);
        let api_url = self.game_api_url.clone();
        tokio::spawn(async move {
            ws_listener_with_reconnect(session_clone, stream, sink_clone, api_url).await;
        });

        // Store session, sink, and chain client.
        self.sessions.write().await.insert(wallet.clone(), session);
        self.ws_sinks.write().await.insert(wallet.clone(), ws_sink);
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

        // Check balance before staking real SOL.
        let balance = chain_client
            .rpc()
            .get_balance(&chain_client.pubkey())
            .await
            .context("failed to check balance")?;
        anyhow::ensure!(
            balance >= 70_000_000,
            "insufficient balance: need at least 0.07 SOL to play, have {} SOL",
            balance as f64 / 1_000_000_000.0
        );

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

    /// Check if match has been found. Handle game creation or joining based on role.
    ///
    /// - **role=0 (Player 1):** Create the game on-chain via cosign, notify game-api.
    /// - **role=1 (Player 2):** Wait for `game_ready`, then join the game on-chain.
    pub async fn check_match(&self, wallet: &str) -> Result<MatchStatus> {
        let session = self
            .sessions
            .read()
            .await
            .get(wallet)
            .context("no session for wallet")?
            .clone();

        let mut s = session.lock().await;

        // Already in game — return current state.
        if s.state == GameSessionState::InGame {
            return Ok(MatchStatus {
                status: "in_game".to_string(),
                game_id: s.game_id,
                role: s.role,
            });
        }

        // Still waiting for match.
        if s.match_found.is_none() {
            return Ok(MatchStatus {
                status: "queued".to_string(),
                game_id: None,
                role: None,
            });
        }

        // Match found but not yet processed.
        if s.state == GameSessionState::Queued {
            let (sid, role) = {
                let mf = s.match_found.as_ref().context("match_found missing")?;
                (mf.session_id.clone(), mf.role)
            };
            s.session_id = Some(sid);
            s.role = Some(role);
            s.state = GameSessionState::Matched;
        }

        let role = s.role.unwrap_or(1);
        let tournament_id = s.tournament_id.context("tournament_id not set")?;
        let session_id = s.session_id.clone().context("session_id not set")?;
        let jwt = s.jwt.clone();

        if role == 0 {
            // Player 1: create the game on-chain.
            let commitment_hex = s
                .matchup_commitment
                .clone()
                .context("role=0 but no matchup_commitment received")?;

            // Drop lock before I/O.
            drop(s);

            let game_id = self
                .create_game_as_p1(wallet, tournament_id, &commitment_hex, &jwt, &session_id)
                .await?;

            let mut s = session.lock().await;
            s.game_id = Some(game_id);
            s.state = GameSessionState::InGame;

            return Ok(MatchStatus {
                status: "in_game".to_string(),
                game_id: Some(game_id),
                role: Some(0),
            });
        }

        // Player 2: wait for game_ready, then join.
        if s.game_ready.is_none() {
            drop(s);
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            s = session.lock().await;
        }

        if let Some(game_id) = s.game_ready {
            if s.state == GameSessionState::Matched {
                drop(s);

                let chain = self.chain_clients.read().await;
                let chain_client = chain.get(wallet).context("no chain client")?;
                chain_client.join_game(game_id, tournament_id).await?;

                let api_client = GameApiClient::new(&self.game_api_url)?;
                api_client
                    .post_games_joined(&jwt, game_id, &session_id)
                    .await?;

                let mut s = session.lock().await;
                s.game_id = Some(game_id);
                s.state = GameSessionState::InGame;

                tracing::info!(wallet = %wallet, game_id, "joined on-chain game as P2");

                return Ok(MatchStatus {
                    status: "in_game".to_string(),
                    game_id: Some(game_id),
                    role: Some(1),
                });
            }
        }

        Ok(MatchStatus {
            status: "matched".to_string(),
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

    /// Commit guess on-chain. Returns immediately after commit.
    ///
    /// Stores the preimage in the session for later use by `try_reveal`.
    pub async fn commit_guess(&self, wallet: &str, guess: u8) -> Result<u64> {
        anyhow::ensure!(guess <= 1, "guess must be 0 (same) or 1 (different)");

        let session = self
            .sessions
            .read()
            .await
            .get(wallet)
            .context("no session for wallet")?
            .clone();

        let (game_id, _tournament_id) = {
            let mut s = session.lock().await;
            let gid = s.game_id.context("no active game")?;
            let tid = s.tournament_id.context("tournament_id not set")?;
            s.state = GameSessionState::Committed;
            (gid, tid)
        };

        let chain = self.chain_clients.read().await;
        let chain_client = chain.get(wallet).context("no chain client")?;
        let preimage = chain_client.submit_commit(game_id, guess).await?;

        // Store preimage for reveal step.
        {
            let mut s = session.lock().await;
            s.commit_preimage = Some(preimage);
        }

        tracing::info!(wallet = %wallet, game_id, guess, "committed guess on-chain");
        Ok(game_id)
    }

    /// Try to reveal the guess on-chain.
    ///
    /// Checks if `reveal_data` has arrived via WebSocket. If not, returns
    /// `status: "waiting"`. If arrived, reveals on-chain and returns the outcome.
    pub async fn try_reveal(&self, wallet: &str) -> Result<GuessOutcome> {
        let session = self
            .sessions
            .read()
            .await
            .get(wallet)
            .context("no session for wallet")?
            .clone();

        let s = session.lock().await;
        let game_id = s.game_id.context("no active game")?;
        let tournament_id = s.tournament_id.context("tournament_id not set")?;
        let preimage = s
            .commit_preimage
            .context("no commit preimage — call game_commit_guess first")?;

        // Check if reveal_data has arrived.
        let reveal_hex = match &s.reveal_data {
            None => {
                return Ok(GuessOutcome {
                    status: "waiting".to_string(),
                    p1_guess: None,
                    p2_guess: None,
                });
            }
            Some(hex) => hex.clone(),
        };
        drop(s);

        let r_matchup_bytes = hex::decode(&reveal_hex).context("invalid hex in r_matchup")?;
        let r_matchup: [u8; 32] = r_matchup_bytes
            .try_into()
            .map_err(|v: Vec<u8>| anyhow::anyhow!("r_matchup must be 32 bytes, got {}", v.len()))?;

        let chain = self.chain_clients.read().await;
        let chain_client = chain.get(wallet).context("no chain client")?;

        let (p1_guess, p2_guess) = chain_client
            .wait_and_reveal(game_id, tournament_id, preimage, Some(r_matchup))
            .await?;

        {
            let mut s = session.lock().await;
            s.state = GameSessionState::Resolved;
        }

        tracing::info!(wallet = %wallet, game_id, p1_guess, p2_guess, "game resolved");

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

    // -- Player 1 game creation -----------------------------------------------

    /// Create a game on-chain as Player 1, using the matchmaker cosign endpoint.
    async fn create_game_as_p1(
        &self,
        wallet: &str,
        tournament_id: u64,
        commitment_hex: &str,
        jwt: &str,
        session_id: &str,
    ) -> Result<u64> {
        use solana_sdk::signature::Signature;

        let commitment_bytes = hex::decode(commitment_hex).context("invalid hex commitment")?;
        let matchup_commitment: [u8; 32] = commitment_bytes.try_into().map_err(|v: Vec<u8>| {
            anyhow::anyhow!("commitment must be 32 bytes, got {}", v.len())
        })?;

        // Read the matchmaker pubkey from GlobalConfig on-chain.
        let chain = self.chain_clients.read().await;
        let chain_client = chain.get(wallet).context("no chain client")?;

        let (global_config_pda, _) = game_chain::pda::global_config_pda();
        let config_data = chain_client
            .rpc()
            .get_account_data(&global_config_pda)
            .await
            .context("failed to read global_config")?;
        // GlobalConfig layout: 8-byte discriminator + 32-byte authority + 32-byte matchmaker + ...
        anyhow::ensure!(config_data.len() >= 72, "global_config data too short");
        let matchmaker = solana_sdk::pubkey::Pubkey::try_from(&config_data[40..72])
            .context("parse matchmaker")?;

        let stake: u64 = 50_000_000; // 0.05 SOL — matches FIXED_STAKE_LAMPORTS on-chain
        let api_url = self.game_api_url.clone();
        let jwt_owned = jwt.to_string();

        let (game_id, _sig) = chain_client
            .create_game(
                tournament_id,
                stake,
                matchup_commitment,
                &matchmaker,
                |message_bytes| {
                    let api_url = api_url.clone();
                    let jwt = jwt_owned.clone();
                    async move {
                        use base64::Engine;
                        let b64 = base64::engine::general_purpose::STANDARD.encode(&message_bytes);
                        let api_client = GameApiClient::new(&api_url)?;
                        let resp = api_client
                            .request_cosign(&jwt, &b64)
                            .await
                            .map_err(|e| anyhow::anyhow!("cosign request failed: {e}"))?;
                        let sig_bytes = base64::engine::general_purpose::STANDARD
                            .decode(&resp.signature)
                            .map_err(|e| anyhow::anyhow!("invalid base64 signature: {e}"))?;
                        let sig = Signature::try_from(sig_bytes.as_slice())
                            .map_err(|e| anyhow::anyhow!("invalid signature bytes: {e}"))?;
                        Ok(sig)
                    }
                },
            )
            .await?;

        // Notify game-api that the game was created.
        let api_client = GameApiClient::new(&self.game_api_url)?;
        api_client
            .post_games_started(jwt, game_id, session_id)
            .await
            .map_err(|e| anyhow::anyhow!("post_games_started failed: {e}"))?;

        tracing::info!(wallet = %wallet, game_id, "created on-chain game as P1");

        Ok(game_id)
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

/// Maximum reconnect attempts before giving up.
const WS_MAX_RECONNECT_ATTEMPTS: u32 = 3;

/// Initial backoff delay for reconnect (doubles each attempt).
const WS_RECONNECT_BASE_DELAY_SECS: u64 = 2;

/// Runs the WS read loop with automatic reconnect on disconnect.
///
/// On disconnect, attempts up to 3 reconnects with exponential backoff
/// (2s, 4s, 8s). Must reconnect within game-api's 60s grace window or
/// the session is abandoned.
async fn ws_listener_with_reconnect(
    session: Arc<Mutex<GameSession>>,
    initial_stream: game_api_client::ws::WsStream,
    sink: Arc<Mutex<WsSink>>,
    game_api_url: String,
) {
    let wallet = session.lock().await.wallet.clone();
    tracing::info!(wallet = %wallet, "ws_listener started");

    let mut stream = initial_stream;

    loop {
        // Run the read loop until disconnect.
        run_ws_read_loop(&session, &mut stream, &sink, &wallet).await;

        // Attempt reconnect with exponential backoff.
        let jwt = session.lock().await.jwt.clone();
        let mut reconnected = false;

        for attempt in 0..WS_MAX_RECONNECT_ATTEMPTS {
            let delay = WS_RECONNECT_BASE_DELAY_SECS
                .checked_shl(attempt)
                .unwrap_or(WS_RECONNECT_BASE_DELAY_SECS);
            tracing::info!(wallet = %wallet, attempt, delay_secs = delay, "ws reconnecting");
            tokio::time::sleep(tokio::time::Duration::from_secs(delay)).await;

            match WsConnection::connect(&game_api_url, &jwt).await {
                Ok(ws) => {
                    let (new_sink, new_stream) = ws.into_split();
                    // Swap the sink so send_message uses the new connection.
                    *sink.lock().await = new_sink;
                    stream = new_stream;
                    reconnected = true;
                    tracing::info!(wallet = %wallet, attempt, "ws reconnected");
                    break;
                }
                Err(e) => {
                    tracing::warn!(wallet = %wallet, attempt, error = %e, "ws reconnect failed");
                }
            }
        }

        if !reconnected {
            tracing::error!(
                wallet = %wallet,
                max_attempts = WS_MAX_RECONNECT_ATTEMPTS,
                "ws reconnect exhausted, session lost"
            );
            break;
        }
    }

    tracing::info!(wallet = %wallet, "ws_listener ended");
}

/// Inner read loop — processes frames until the stream errors or closes.
async fn run_ws_read_loop(
    session: &Arc<Mutex<GameSession>>,
    stream: &mut game_api_client::ws::WsStream,
    sink: &Arc<Mutex<WsSink>>,
    wallet: &str,
) {
    use futures_util::StreamExt;
    use game_api_client::ws::WsMessage;

    loop {
        let frame = match stream.next().await {
            Some(Ok(frame)) => frame,
            Some(Err(e)) => {
                tracing::warn!(wallet = %wallet, error = %e, "game WS read error");
                return;
            }
            None => {
                tracing::info!(wallet = %wallet, "game WS stream ended");
                return;
            }
        };

        match frame {
            WsMessage::Text(text) => {
                let msg = game_api_client::ws::parse_server_message(&text);
                let mut s = session.lock().await;
                match msg {
                    ServerMessage::MatchFound {
                        session_id,
                        role,
                        matchup_commitment,
                    } => {
                        tracing::info!(wallet = %wallet, %session_id, role, has_commitment = matchup_commitment.is_some(), "ws: match_found");
                        s.matchup_commitment = matchup_commitment.clone();
                        s.match_found = Some(MatchFoundMsg {
                            session_id,
                            role,
                            matchup_commitment,
                        });
                    }
                    ServerMessage::GameReady { game_id } => {
                        tracing::info!(wallet = %wallet, game_id, "ws: game_ready");
                        s.game_ready = Some(game_id);
                    }
                    ServerMessage::RevealData { r_matchup } => {
                        tracing::info!(wallet = %wallet, "ws: reveal_data");
                        s.reveal_data = Some(r_matchup);
                    }
                    ServerMessage::Chat { text } => {
                        s.chat_buffer.push(text);
                    }
                    ServerMessage::Unknown => {
                        tracing::debug!(wallet = %wallet, "ws: unknown message type");
                    }
                }
            }
            WsMessage::Ping(data) => {
                if let Err(e) = sink.lock().await.send(WsMessage::Pong(data)).await {
                    tracing::warn!(wallet = %wallet, error = %e, "pong send failed");
                    return;
                }
            }
            WsMessage::Pong(_) | WsMessage::Binary(_) | WsMessage::Frame(_) => {}
            WsMessage::Close(_) => {
                tracing::info!(wallet = %wallet, "game WS closed by server");
                return;
            }
        }
    }
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
