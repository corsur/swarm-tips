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
//! └── tx_builders: HashMap<wallet, GameChainClient>
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
use firestore::FirestoreDb;
use futures_util::SinkExt;
use game_api_client::ws::{MatchFoundMsg, ServerMessage, WsConnection, WsSink};
use game_api_client::GameApiClient;
use game_chain::client::GameTxBuilder;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;

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

impl GameSessionState {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Connected => "connected",
            Self::Queued => "queued",
            Self::Matched => "matched",
            Self::InGame => "in_game",
            Self::Committed => "committed",
            Self::Resolved => "resolved",
        }
    }

    fn from_str(s: &str) -> Option<Self> {
        match s {
            "connected" => Some(Self::Connected),
            "queued" => Some(Self::Queued),
            "matched" => Some(Self::Matched),
            "in_game" => Some(Self::InGame),
            "committed" => Some(Self::Committed),
            "resolved" => Some(Self::Resolved),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Firestore persistence
// ---------------------------------------------------------------------------

const MCP_SESSIONS_COLLECTION: &str = "mcp_game_sessions";

/// Firestore document for persisted MCP game session state.
///
/// Stored on every state transition so that pod restarts do not lose
/// critical state (especially `commit_preimage_hex`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedGameSession {
    pub wallet: String,
    pub jwt: String,
    pub state: String,
    pub game_id: Option<u64>,
    pub tournament_id: Option<u64>,
    pub session_id: Option<String>,
    pub role: Option<u8>,
    pub matchup_commitment: Option<String>,
    /// Hex-encoded `[u8; 32]` — critical for the reveal step.
    pub commit_preimage_hex: Option<String>,
    pub game_ready: Option<u64>,
    pub reveal_data: Option<String>,
    pub updated_at: firestore::FirestoreTimestamp,
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
    /// Base64-encoded unsigned transaction message (when status = "needs_signature").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unsigned_tx: Option<String>,
    /// What action this transaction performs (e.g., "join_game", "create_game").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// Matchmaker's cosignature (base64, for create_game multi-sig assembly).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matchmaker_signature: Option<String>,
    /// Blockhash used for the unsigned transaction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blockhash: Option<String>,
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
    tx_builders: RwLock<HashMap<String, GameTxBuilder>>,
    /// Cancellation tokens for background WS listener tasks.
    /// Cancelled in `cleanup()` to stop dangling reconnect loops.
    ws_cancel_tokens: RwLock<HashMap<String, CancellationToken>>,
    game_api_url: String,
    solana_rpc_url: String,
    db: Arc<FirestoreDb>,
}

impl GameSessionManager {
    pub fn new(game_api_url: String, solana_rpc_url: String, db: Arc<FirestoreDb>) -> Self {
        assert!(!game_api_url.is_empty(), "game_api_url must not be empty");
        assert!(
            !solana_rpc_url.is_empty(),
            "solana_rpc_url must not be empty"
        );
        Self {
            sessions: RwLock::new(HashMap::new()),
            ws_sinks: RwLock::new(HashMap::new()),
            tx_builders: RwLock::new(HashMap::new()),
            ws_cancel_tokens: RwLock::new(HashMap::new()),
            game_api_url,
            solana_rpc_url,
            db,
        }
    }

    // -- Firestore persistence helpers --

    /// Persist the current session state to Firestore (write-through).
    ///
    /// Returns an error only when the session contains a commit preimage —
    /// losing the preimage means the agent can't reveal after a pod restart.
    /// For other states, Firestore failures are logged but non-fatal.
    async fn persist_session(&self, session: &GameSession) -> Result<()> {
        let has_preimage = session.commit_preimage.is_some();
        let doc = PersistedGameSession {
            wallet: session.wallet.clone(),
            jwt: session.jwt.clone(),
            state: session.state.as_str().to_string(),
            game_id: session.game_id,
            tournament_id: session.tournament_id,
            session_id: session.session_id.clone(),
            role: session.role,
            matchup_commitment: session.matchup_commitment.clone(),
            commit_preimage_hex: session.commit_preimage.map(hex::encode),
            game_ready: session.game_ready,
            reveal_data: session.reveal_data.clone(),
            updated_at: firestore::FirestoreTimestamp(chrono::Utc::now()),
        };
        if let Err(e) = self
            .db
            .fluent()
            .update()
            .in_col(MCP_SESSIONS_COLLECTION)
            .document_id(&session.wallet)
            .object(&doc)
            .execute::<PersistedGameSession>()
            .await
        {
            if has_preimage {
                // Preimage loss is critical — agent can't reveal after pod restart.
                tracing::error!(
                    wallet = %session.wallet,
                    error = %e,
                    "CRITICAL: failed to persist commit preimage to Firestore"
                );
                return Err(anyhow::anyhow!(
                    "failed to persist commit preimage — retry commit_guess"
                ));
            }
            tracing::warn!(
                wallet = %session.wallet,
                error = %e,
                "failed to persist session to Firestore (non-fatal)"
            );
        }
        Ok(())
    }

    /// Load a persisted session from Firestore, if one exists.
    async fn load_persisted_session(&self, wallet: &str) -> Option<PersistedGameSession> {
        match self
            .db
            .fluent()
            .select()
            .by_id_in(MCP_SESSIONS_COLLECTION)
            .obj::<PersistedGameSession>()
            .one(wallet)
            .await
        {
            Ok(doc) => doc,
            Err(e) => {
                tracing::warn!(wallet = %wallet, error = %e, "failed to load persisted session");
                None
            }
        }
    }

    /// Delete the persisted session document.
    async fn delete_persisted_session(&self, wallet: &str) {
        if let Err(e) = self
            .db
            .fluent()
            .delete()
            .from(MCP_SESSIONS_COLLECTION)
            .document_id(wallet)
            .execute()
            .await
        {
            tracing::warn!(wallet = %wallet, error = %e, "failed to delete persisted session");
        }
    }

    /// Return the wallet pubkey of any active game session.
    ///
    /// Used to resolve the caller when MCP auth middleware is not yet wired
    /// up. Returns `None` if no sessions exist.
    pub async fn get_any_wallet(&self) -> Option<String> {
        self.sessions.read().await.keys().next().cloned()
    }

    /// Whether the in-memory session map already holds a session for
    /// `wallet`. Used by `resolve_wallet` to skip the heavy
    /// `register_wallet` re-hydrate path on every tool call — only the
    /// first call after a pod restart actually pays the
    /// Firestore-load + balance-check cost.
    pub async fn is_registered(&self, wallet: &str) -> bool {
        self.sessions.read().await.contains_key(wallet)
    }

    /// Register an agent wallet by public key only. Non-custodial: no private
    /// key ever touches the MCP server.
    ///
    /// Creates a `GameTxBuilder` and checks the wallet balance. Auth and
    /// WebSocket connection happen later in `submit_signed_game_tx` when the
    /// agent submits their signed deposit_stake transaction (stake = auth).
    ///
    /// If a persisted session exists from a previous pod lifecycle (and is not
    /// resolved), it is restored — including the critical `commit_preimage`.
    ///
    /// Returns `(wallet_pubkey, balance_lamports)`.
    pub async fn register_wallet(&self, pubkey_b58: &str) -> Result<(String, u64)> {
        let pubkey: solana_sdk::pubkey::Pubkey = pubkey_b58
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid pubkey: {e}"))?;
        let wallet = pubkey.to_string();

        let tx_builder = GameTxBuilder::new(&self.solana_rpc_url, pubkey);

        let balance = tx_builder
            .rpc()
            .get_balance(&pubkey)
            .await
            .context("failed to check wallet balance")?;

        // Check Firestore for an active session from a previous pod lifecycle.
        if let Some(persisted) = self.load_persisted_session(&wallet).await {
            let state =
                GameSessionState::from_str(&persisted.state).unwrap_or(GameSessionState::Connected);

            if state != GameSessionState::Resolved {
                tracing::info!(
                    wallet = %wallet,
                    state = persisted.state,
                    game_id = ?persisted.game_id,
                    "restoring persisted session from Firestore"
                );

                let preimage = persisted
                    .commit_preimage_hex
                    .as_ref()
                    .and_then(|h| hex::decode(h).ok())
                    .and_then(|bytes| <[u8; 32]>::try_from(bytes.as_slice()).ok());

                let restored = Arc::new(Mutex::new(GameSession {
                    wallet: wallet.clone(),
                    jwt: persisted.jwt.clone(),
                    state,
                    game_id: persisted.game_id,
                    tournament_id: persisted.tournament_id,
                    session_id: persisted.session_id.clone(),
                    role: persisted.role,
                    chat_buffer: Vec::new(),
                    match_found: None,
                    matchup_commitment: persisted.matchup_commitment,
                    commit_preimage: preimage,
                    game_ready: persisted.game_ready,
                    reveal_data: persisted.reveal_data,
                }));

                self.sessions
                    .write()
                    .await
                    .insert(wallet.clone(), restored.clone());
                self.tx_builders
                    .write()
                    .await
                    .insert(wallet.clone(), tx_builder);

                // Re-establish WS if JWT is available (with timeout — JWT may be expired).
                if !persisted.jwt.is_empty() {
                    let ws_result = tokio::time::timeout(
                        tokio::time::Duration::from_secs(10),
                        WsConnection::connect(&self.game_api_url, &persisted.jwt),
                    )
                    .await;
                    match ws_result {
                        Err(_) => {
                            tracing::warn!(
                                wallet = %wallet,
                                "WS connect timed out for restored session (JWT likely expired)"
                            );
                        }
                        Ok(Err(e)) => {
                            tracing::warn!(
                                wallet = %wallet,
                                error = %e,
                                "failed to re-establish WS for restored session"
                            );
                        }
                        Ok(Ok(ws)) => {
                            let (sink, stream) = ws.into_split();
                            let ws_sink = Arc::new(Mutex::new(sink));
                            let session_clone = Arc::clone(&restored);
                            let sink_clone = Arc::clone(&ws_sink);
                            let api_url = self.game_api_url.clone();
                            let cancel_token = CancellationToken::new();
                            let token_clone = cancel_token.clone();
                            tokio::spawn(async move {
                                ws_listener_with_reconnect(
                                    session_clone,
                                    stream,
                                    sink_clone,
                                    api_url,
                                    token_clone,
                                )
                                .await;
                            });
                            self.ws_sinks.write().await.insert(wallet.clone(), ws_sink);
                            self.ws_cancel_tokens
                                .write()
                                .await
                                .insert(wallet.clone(), cancel_token);
                            tracing::info!(wallet = %wallet, "WS re-established for restored session");
                        }
                    }
                }

                return Ok((wallet, balance));
            }

            // Session was resolved — clean it up and proceed with fresh registration.
            self.delete_persisted_session(&wallet).await;
        }

        // Build session without JWT — auth happens after deposit_stake.
        let session = Arc::new(Mutex::new(GameSession {
            wallet: wallet.clone(),
            jwt: String::new(), // populated after stake-as-auth
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

        self.sessions.write().await.insert(wallet.clone(), session);
        self.tx_builders
            .write()
            .await
            .insert(wallet.clone(), tx_builder);

        tracing::info!(
            service = "coordination-mcp-server",
            wallet = %wallet,
            balance,
            "wallet registered (pubkey only, non-custodial)"
        );

        Ok((wallet, balance))
    }

    /// Build an unsigned deposit_stake transaction for the agent to sign.
    ///
    /// Returns the unsigned transaction. The agent signs locally and submits
    /// via `submit_signed_game_tx`, which handles queue join and auth.
    pub async fn build_find_match_tx(
        &self,
        wallet: &str,
        tournament_id: u64,
    ) -> Result<game_chain::client::UnsignedTx> {
        let builders = self.tx_builders.read().await;
        let tx_builder = builders.get(wallet).context("no session for wallet")?;

        // Check balance before building the stake tx.
        let balance = tx_builder
            .rpc()
            .get_balance(&tx_builder.pubkey())
            .await
            .context("failed to check balance")?;
        anyhow::ensure!(
            balance >= 70_000_000,
            "insufficient balance: need at least 0.07 SOL to play, have {} SOL",
            balance as f64 / 1_000_000_000.0
        );

        let unsigned = tx_builder.build_deposit_stake(tournament_id).await?;

        // Store tournament_id in session for later queue join.
        if let Some(session) = self.sessions.read().await.get(wallet) {
            session.lock().await.tournament_id = Some(tournament_id);
        }

        Ok(unsigned)
    }

    /// Submit a signed game transaction and handle post-submission logic.
    ///
    /// After deposit_stake: authenticates with game-api via the tx signature,
    /// connects WebSocket if needed, and joins the matchmaking queue.
    /// Submit a signed game transaction and handle action-specific post-submission logic.
    pub async fn submit_signed_game_tx(
        &self,
        wallet: &str,
        signed_tx_b64: &str,
        action: &str,
    ) -> Result<serde_json::Value> {
        let builders = self.tx_builders.read().await;
        let tx_builder = builders.get(wallet).context("no session for wallet")?;

        use base64::Engine;
        let signed_bytes = base64::engine::general_purpose::STANDARD
            .decode(signed_tx_b64)
            .context("invalid base64 signed transaction")?;

        tracing::info!(
            wallet = %wallet,
            action = %action,
            tx_len = signed_bytes.len(),
            "submitting signed transaction"
        );

        let sig = match tx_builder.submit_signed(&signed_bytes).await {
            Ok(s) => s,
            Err(e) => {
                tracing::error!(
                    wallet = %wallet,
                    action = %action,
                    error = %e,
                    error_debug = ?e,
                    "signed transaction failed"
                );
                return Err(e);
            }
        };
        let sig_str = sig.to_string();
        tracing::info!(
            wallet = %wallet,
            action = %action,
            sig = %sig_str,
            "signed transaction confirmed"
        );

        let session = self
            .sessions
            .read()
            .await
            .get(wallet)
            .context("no session for wallet")?
            .clone();

        // Action-specific post-submission logic.
        match action {
            "deposit_stake" => {
                // Always re-authenticate after a deposit_stake submission. The
                // JWT we have (if any) was issued for a *previous* stake; the
                // game-api ties session_id to a specific deposit, so reusing
                // an old JWT against a new stake hits an ExpiredSignature or
                // wrong-session 401 once the original token TTL elapses. The
                // fresh stake we just broadcast IS the auth credential.
                if !session.lock().await.jwt.is_empty() {
                    if let Some(token) = self.ws_cancel_tokens.write().await.remove(wallet) {
                        token.cancel();
                    }
                    self.ws_sinks.write().await.remove(wallet);
                    session.lock().await.jwt.clear();
                }
                {
                    let api_client = GameApiClient::new(&self.game_api_url)?;
                    let auth_resp = api_client.session_auth(wallet, &sig_str).await?;
                    let jwt = auth_resp.token.clone();

                    let ws = WsConnection::connect(&self.game_api_url, &jwt).await?;
                    let (sink, stream) = ws.into_split();
                    let ws_sink = Arc::new(Mutex::new(sink));

                    let session_clone = Arc::clone(&session);
                    let sink_clone = Arc::clone(&ws_sink);
                    let api_url = self.game_api_url.clone();
                    let cancel_token = CancellationToken::new();
                    let token_clone = cancel_token.clone();
                    tokio::spawn(async move {
                        ws_listener_with_reconnect(
                            session_clone,
                            stream,
                            sink_clone,
                            api_url,
                            token_clone,
                        )
                        .await;
                    });

                    self.ws_sinks
                        .write()
                        .await
                        .insert(wallet.to_string(), ws_sink);
                    self.ws_cancel_tokens
                        .write()
                        .await
                        .insert(wallet.to_string(), cancel_token);
                    session.lock().await.jwt = jwt;

                    // Persist after auth.
                    {
                        let s = session.lock().await;
                        self.persist_session(&s).await?;
                    }

                    tracing::info!(wallet = %wallet, "authenticated via deposit_stake tx");
                }

                // Join the queue.
                let (jwt, tournament_id) = {
                    let s = session.lock().await;
                    (s.jwt.clone(), s.tournament_id)
                };
                if let Some(tid) = tournament_id {
                    self.join_queue_after_stake(wallet, &jwt, tid).await?;
                }
            }
            "join_game" => {
                let (jwt, session_id) = {
                    let mut s = session.lock().await;
                    s.state = GameSessionState::InGame;
                    (s.jwt.clone(), s.session_id.clone().unwrap_or_default())
                };
                {
                    let s = session.lock().await;
                    self.persist_session(&s).await?;
                }
                let game_id = session.lock().await.game_id.unwrap_or(0);
                if !session_id.is_empty() {
                    let api_client = GameApiClient::new(&self.game_api_url)?;
                    api_client
                        .post_games_joined(&jwt, game_id, &session_id)
                        .await?;
                }
                tracing::info!(wallet = %wallet, game_id, "P2 joined game on-chain");
            }
            "commit_guess" => {
                let mut s = session.lock().await;
                s.state = GameSessionState::Committed;
                // CRITICAL: persist preimage so it survives pod restarts.
                self.persist_session(&s).await?;
                let jwt = s.jwt.clone();
                let session_id = s.session_id.clone().unwrap_or_default();
                drop(s);
                if !session_id.is_empty() {
                    let api_client = GameApiClient::new(&self.game_api_url)?;
                    if let Err(e) = api_client.post_games_committed(&jwt, &session_id).await {
                        tracing::warn!(wallet = %wallet, error = %e, "post_games_committed failed (non-fatal)");
                    }
                }
                tracing::info!(wallet = %wallet, "committed guess on-chain");
            }
            "reveal_guess" => {
                session.lock().await.state = GameSessionState::Resolved;
                tracing::info!(wallet = %wallet, "revealed guess on-chain");
                // Cleanup runs after the response is sent — see below.
            }
            "create_game" => {
                let (jwt, session_id, game_id) = {
                    let mut s = session.lock().await;
                    s.state = GameSessionState::InGame;
                    let gid = s.game_id.unwrap_or(0);
                    (s.jwt.clone(), s.session_id.clone().unwrap_or_default(), gid)
                };
                {
                    let s = session.lock().await;
                    self.persist_session(&s).await?;
                }
                if !session_id.is_empty() {
                    let api_client = GameApiClient::new(&self.game_api_url)?;
                    api_client
                        .post_games_started(&jwt, game_id, &session_id)
                        .await?;
                }
                tracing::info!(wallet = %wallet, game_id, "P1 created game on-chain");
            }
            _ => {
                tracing::warn!(wallet = %wallet, action, "unknown action for game_submit_tx");
            }
        }

        Ok(serde_json::json!({
            "tx_signature": sig_str,
            "status": "submitted",
        }))
    }

    /// Internal: join the matchmaking queue after stake deposit.
    async fn join_queue_after_stake(
        &self,
        wallet: &str,
        jwt: &str,
        tournament_id: u64,
    ) -> Result<()> {
        // Clear stale queue entry from previous crash.
        let api_client = GameApiClient::new(&self.game_api_url)?;
        if let Err(e) = api_client.leave_queue(jwt, tournament_id).await {
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
        api_client.join_queue(jwt, &request).await?;

        // Update session state.
        let session = self
            .sessions
            .read()
            .await
            .get(wallet)
            .context("no session for wallet")?
            .clone();
        let mut s = session.lock().await;
        s.state = GameSessionState::Queued;
        s.tournament_id = Some(tournament_id);
        self.persist_session(&s).await?;

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
                unsigned_tx: None,
                action: None,
                matchmaker_signature: None,
                blockhash: None,
            });
        }

        // Still waiting for match.
        if s.match_found.is_none() {
            return Ok(MatchStatus {
                status: "queued".to_string(),
                game_id: None,
                role: None,
                unsigned_tx: None,
                action: None,
                matchmaker_signature: None,
                blockhash: None,
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
        let _session_id = s.session_id.clone().context("session_id not set")?;
        let jwt = s.jwt.clone();

        if role == 0 {
            // Player 1: create the game on-chain.
            let commitment_hex = s
                .matchup_commitment
                .clone()
                .context("role=0 but no matchup_commitment received")?;

            // Drop lock before I/O.
            drop(s);

            let (unsigned, matchmaker_sig, game_id) = self
                .build_create_game_tx(wallet, tournament_id, &commitment_hex, &jwt)
                .await?;

            // Store game_id but don't transition to InGame — that happens after submit.
            let mut s = session.lock().await;
            s.game_id = Some(game_id);

            return Ok(MatchStatus {
                status: "needs_signature".to_string(),
                game_id: Some(game_id),
                role: Some(0),
                unsigned_tx: Some(unsigned.transaction_b64),
                action: Some("create_game".to_string()),
                matchmaker_signature: Some(matchmaker_sig),
                blockhash: Some(unsigned.blockhash),
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

                let chain = self.tx_builders.read().await;
                let tx_builder = chain.get(wallet).context("no tx builder for wallet")?;
                let unsigned = tx_builder.build_join_game(game_id, tournament_id).await?;

                // Store game_id but don't transition to InGame yet —
                // that happens after the agent submits the signed tx.
                let mut s = session.lock().await;
                s.game_id = Some(game_id);

                tracing::info!(wallet = %wallet, game_id, "P2 join_game tx built (needs signing)");

                // Return the unsigned tx in the match status
                return Ok(MatchStatus {
                    status: "needs_signature".to_string(),
                    game_id: Some(game_id),
                    role: Some(1),
                    unsigned_tx: Some(unsigned.transaction_b64),
                    action: Some("join_game".to_string()),
                    matchmaker_signature: None,
                    blockhash: Some(unsigned.blockhash),
                });
            }
        }

        Ok(MatchStatus {
            status: "matched".to_string(),
            game_id: s.game_id,
            role: s.role,
            unsigned_tx: None,
            action: None,
            matchmaker_signature: None,
            blockhash: None,
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
    /// Build an unsigned commit_guess transaction. Returns the unsigned tx
    /// and the preimage hex (needed for the reveal step).
    ///
    /// The preimage is also stored in the session for `build_reveal_tx`.
    /// The agent signs the tx locally and submits via `submit_signed_game_tx`.
    pub async fn build_commit_tx(
        &self,
        wallet: &str,
        guess: u8,
    ) -> Result<(game_chain::client::UnsignedTx, String)> {
        anyhow::ensure!(guess <= 1, "guess must be 0 (same) or 1 (different)");

        let session = self
            .sessions
            .read()
            .await
            .get(wallet)
            .context("no session for wallet")?
            .clone();

        let game_id = {
            let s = session.lock().await;
            s.game_id.context("no active game")?
        };

        // Generate commitment locally.
        let (preimage, commitment) =
            game_chain::commit::generate_commit_secret(guess).map_err(|e| anyhow::anyhow!(e))?;

        let chain = self.tx_builders.read().await;
        let tx_builder = chain.get(wallet).context("no tx builder for wallet")?;
        let unsigned = tx_builder.build_commit_guess(game_id, commitment).await?;

        // Store preimage for the reveal step and persist to Firestore.
        // This is the most critical persist point — without the preimage,
        // the agent cannot reveal even if they reconnect.
        {
            let mut s = session.lock().await;
            s.commit_preimage = Some(preimage);
            self.persist_session(&s).await?;
        }

        let preimage_hex = hex::encode(preimage);
        tracing::info!(wallet = %wallet, game_id, guess, "built unsigned commit_guess tx");
        Ok((unsigned, preimage_hex))
    }

    /// Try to reveal the guess on-chain.
    ///
    /// Checks if `reveal_data` has arrived via WebSocket. If not, returns
    /// Check if reveal_data has arrived and build an unsigned reveal transaction.
    ///
    /// Returns `None` if still waiting for the opponent to commit.
    /// Returns `Some(UnsignedTx)` when ready — agent signs and submits via `game_submit_tx`.
    pub async fn build_reveal_tx(
        &self,
        wallet: &str,
    ) -> Result<Option<game_chain::client::UnsignedTx>> {
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

        // Check if reveal_data has arrived via WebSocket.
        let reveal_hex = match &s.reveal_data {
            None => return Ok(None), // still waiting
            Some(hex) => hex.clone(),
        };
        drop(s);

        let r_matchup_bytes = hex::decode(&reveal_hex).context("invalid hex in r_matchup")?;
        let r_matchup: [u8; 32] = r_matchup_bytes
            .try_into()
            .map_err(|v: Vec<u8>| anyhow::anyhow!("r_matchup must be 32 bytes, got {}", v.len()))?;

        let chain = self.tx_builders.read().await;
        let tx_builder = chain.get(wallet).context("no tx builder for wallet")?;

        // Read the game to get player pubkeys and verify state.
        let game = tx_builder
            .read_game(game_id)
            .await?
            .context("game account not found")?;

        // If the game is no longer in Revealing state (opponent revealed first,
        // or timeout resolved it), skip building a stale transaction.
        if game.state != game_chain::GameState::Revealing {
            tracing::info!(
                wallet = %wallet,
                game_id,
                state = ?game.state,
                "game not in Revealing state, skipping reveal tx"
            );
            return Ok(None);
        }

        let unsigned = tx_builder
            .build_reveal_guess(
                game_id,
                tournament_id,
                preimage,
                Some(r_matchup),
                game.player_one,
                game.player_two,
            )
            .await?;

        tracing::info!(wallet = %wallet, game_id, "built unsigned reveal_guess tx");
        Ok(Some(unsigned))
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

        let chain = self.tx_builders.read().await;
        let tx_builder = chain.get(wallet).context("no tx builder for wallet")?;
        let game = tx_builder
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

    // -- Player 1 game creation -----------------------------------------------

    /// Build an unsigned create_game transaction as Player 1.
    ///
    /// Returns the unsigned tx, matchmaker cosignature, and expected game_id.
    /// The agent signs their part, assembles the multi-sig tx, and submits
    /// via `submit_signed_game_tx` with action="create_game".
    pub async fn build_create_game_tx(
        &self,
        wallet: &str,
        tournament_id: u64,
        commitment_hex: &str,
        jwt: &str,
    ) -> Result<(game_chain::client::UnsignedTx, String, u64)> {
        let commitment_bytes = hex::decode(commitment_hex).context("invalid hex commitment")?;
        let matchup_commitment: [u8; 32] = commitment_bytes.try_into().map_err(|v: Vec<u8>| {
            anyhow::anyhow!("commitment must be 32 bytes, got {}", v.len())
        })?;

        let chain = self.tx_builders.read().await;
        let tx_builder = chain.get(wallet).context("no tx builder for wallet")?;

        // Log balance before create_game for debugging.
        let balance = tx_builder
            .rpc()
            .get_balance(&tx_builder.pubkey())
            .await
            .unwrap_or(0);
        tracing::info!(wallet = %wallet, balance_lamports = balance, "creating game as P1");

        // Read the matchmaker pubkey from GlobalConfig on-chain.

        let (global_config_pda, _) = game_chain::pda::global_config_pda();
        let config_data = tx_builder
            .rpc()
            .get_account_data(&global_config_pda)
            .await
            .context("failed to read global_config")?;
        // GlobalConfig layout: 8-byte discriminator + 32-byte authority + 32-byte matchmaker + ...
        anyhow::ensure!(config_data.len() >= 72, "global_config data too short");
        let matchmaker = solana_sdk::pubkey::Pubkey::try_from(&config_data[40..72])
            .context("parse matchmaker")?;

        let stake: u64 = 50_000_000; // 0.05 SOL — matches FIXED_STAKE_LAMPORTS on-chain

        // Build unsigned create_game transaction.
        let unsigned = tx_builder
            .build_create_game(tournament_id, stake, matchup_commitment, &matchmaker)
            .await?;

        // DEBUG: print byte lengths to track down a 6-byte truncation in the
        // deployed binary's create_game tx output (vs locally-built ~546 bytes).
        tracing::info!(
            wallet = %wallet,
            transaction_b64_len = unsigned.transaction_b64.len(),
            message_len = unsigned.message.len(),
            num_signers = unsigned.num_signers,
            "DEBUG create_game tx sizes"
        );

        // Get matchmaker cosignature from game-api.
        use base64::Engine;
        let msg_b64 = base64::engine::general_purpose::STANDARD.encode(&unsigned.message);
        let api_client = GameApiClient::new(&self.game_api_url)?;
        let cosign_resp = api_client
            .request_cosign(jwt, &msg_b64)
            .await
            .map_err(|e| anyhow::anyhow!("cosign request failed: {e}"))?;

        // Read the game counter to get the expected game_id.
        let (counter_pda, _) = game_chain::pda::game_counter_pda();
        let counter_data = tx_builder
            .rpc()
            .get_account_data(&counter_pda)
            .await
            .context("failed to read game_counter")?;
        anyhow::ensure!(counter_data.len() >= 16, "game_counter data too short");
        let game_id = u64::from_le_bytes(counter_data[8..16].try_into().context("parse count")?);

        tracing::info!(
            wallet = %wallet,
            game_id,
            "P1 create_game tx built with matchmaker cosign"
        );

        Ok((unsigned, cosign_resp.signature, game_id))
    }

    // -- internal helpers --
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
/// the session is abandoned. Exits immediately when `cancel` is triggered
/// (game resolved or session cleaned up).
async fn ws_listener_with_reconnect(
    session: Arc<Mutex<GameSession>>,
    initial_stream: game_api_client::ws::WsStream,
    sink: Arc<Mutex<WsSink>>,
    game_api_url: String,
    cancel: CancellationToken,
) {
    let wallet = session.lock().await.wallet.clone();
    tracing::info!(wallet = %wallet, "ws_listener started");

    let mut stream = initial_stream;

    loop {
        if cancel.is_cancelled() {
            tracing::info!(wallet = %wallet, "ws_listener cancelled");
            return;
        }

        // Run the read loop until disconnect.
        run_ws_read_loop(&session, &mut stream, &sink, &wallet).await;

        if cancel.is_cancelled() {
            tracing::info!(wallet = %wallet, "ws_listener cancelled after disconnect");
            return;
        }

        // Attempt reconnect with exponential backoff.
        let jwt = session.lock().await.jwt.clone();
        let mut reconnected = false;

        for attempt in 0..WS_MAX_RECONNECT_ATTEMPTS {
            if cancel.is_cancelled() {
                tracing::info!(wallet = %wallet, "ws_listener cancelled during reconnect");
                return;
            }
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
    fn match_status_serialization() {
        let status = MatchStatus {
            status: "queued".to_string(),
            game_id: None,
            role: None,
            unsigned_tx: None,
            action: None,
            matchmaker_signature: None,
            blockhash: None,
        };
        let json = serde_json::to_string(&status).expect("serialize");
        assert!(json.contains("queued"));
    }

    // --- GameSessionState serialization ---

    #[test]
    fn game_session_state_roundtrip() {
        let variants = [
            GameSessionState::Connected,
            GameSessionState::Queued,
            GameSessionState::Matched,
            GameSessionState::InGame,
            GameSessionState::Committed,
            GameSessionState::Resolved,
        ];
        for state in &variants {
            let s = state.as_str();
            let restored = GameSessionState::from_str(s)
                .unwrap_or_else(|| panic!("failed to parse '{s}' back to GameSessionState"));
            assert_eq!(*state, restored, "roundtrip failed for '{s}'");
        }
    }

    #[test]
    fn game_session_state_from_unknown_returns_none() {
        assert!(GameSessionState::from_str("nonexistent").is_none());
    }

    // --- Preimage hex roundtrip ---

    #[test]
    fn preimage_hex_roundtrip() {
        let original: [u8; 32] = [
            0xde, 0xad, 0xbe, 0xef, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a,
            0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18,
            0x19, 0x1a, 0x1b, 0x1c,
        ];
        let hex_str = hex::encode(original);
        let decoded = hex::decode(&hex_str).expect("valid hex");
        let restored: [u8; 32] = decoded.try_into().expect("32 bytes");
        assert_eq!(original, restored);
    }

    // --- PersistedGameSession serialization ---

    #[test]
    fn persisted_session_json_roundtrip() {
        let doc = PersistedGameSession {
            wallet: "Abc123".to_string(),
            jwt: "jwt-token".to_string(),
            state: "committed".to_string(),
            game_id: Some(42),
            tournament_id: Some(1),
            session_id: Some("sess-1".to_string()),
            role: Some(0),
            matchup_commitment: Some("deadbeef".to_string()),
            commit_preimage_hex: Some(hex::encode([0xabu8; 32])),
            game_ready: Some(42),
            reveal_data: None,
            updated_at: firestore::FirestoreTimestamp(chrono::Utc::now()),
        };
        let json = serde_json::to_string(&doc).expect("serialize");
        let restored: PersistedGameSession = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.wallet, "Abc123");
        assert_eq!(restored.state, "committed");
        assert_eq!(restored.game_id, Some(42));
        assert_eq!(restored.commit_preimage_hex, doc.commit_preimage_hex);
    }

    #[test]
    fn restore_skips_resolved_sessions() {
        let state = GameSessionState::from_str("resolved");
        assert_eq!(state, Some(GameSessionState::Resolved));
        // In register_wallet, resolved sessions are cleaned up, not restored.
        // This test verifies the state parsing works correctly.
        assert_eq!(state.unwrap(), GameSessionState::Resolved);
    }

    #[test]
    fn restore_recovers_committed_session_with_preimage() {
        let preimage_bytes = [0x42u8; 32];
        let hex_str = hex::encode(preimage_bytes);

        // Simulate restoring from Firestore.
        let restored_preimage = hex::decode(&hex_str)
            .ok()
            .and_then(|bytes| <[u8; 32]>::try_from(bytes.as_slice()).ok());

        assert_eq!(restored_preimage, Some(preimage_bytes));
    }
}
