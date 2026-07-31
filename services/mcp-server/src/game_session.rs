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
use std::str::FromStr;
use std::sync::Arc;

use anyhow::{Context, Result};
use base64::Engine;
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

/// Headroom over the stake a wallet needs before we let it deposit: rent for the
/// game PDA plus transaction fees for the deposit/commit/reveal sequence. The
/// stake itself comes from `game_chain::FIXED_STAKE_LAMPORTS` — never restated.
const STAKE_FEE_BUFFER_LAMPORTS: u64 = 20_000_000;

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

const MCP_EVM_PREIMAGES_COLLECTION: &str = "mcp_evm_preimages";

/// Firestore document holding a same-chain EVM player's guess preimage between
/// commit and reveal — the EVM analog of `commit_preimage_hex` on the Solana
/// session. The EVM game keeps no in-memory session in mcp-server (its tools are
/// stateless proxies), so the preimage is stored keyed by wallet+game_id and
/// recovered at reveal time. Without it a reconnecting agent can't reveal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvmPreimageDoc {
    /// Document id: `{wallet}:{game_id}`.
    pub id: String,
    pub wallet: String,
    pub game_id: String,
    /// Hex-encoded `[u8; 32]` guess preimage.
    pub preimage_hex: String,
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
    /// Solana network for this session — `Some("devnet")` routes every
    /// game-api call through `?network=devnet` and every Solana RPC read
    /// to the devnet endpoint. `None` (the default) is mainnet. Set once
    /// when `build_find_match_tx` is called with a network, then reused
    /// for the rest of the session's lifecycle so post-deposit auth,
    /// queue join, cosign, and game-state notifications all hit the same
    /// cluster the deposit was broadcast on.
    pub network: Option<String>,
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
///
/// Holds RPC URLs for both networks (mainnet + devnet) so per-call tools
/// (`game_find_match`, `game_submit_tx`) can route to the cluster the
/// caller specifies. The cached `tx_builders` is keyed by wallet only and
/// uses the cluster-default URL — matched the original mainnet-only flow.
/// Anything that needs a non-default network builds an ephemeral
/// GameTxBuilder via `tx_builder_for_network`. See the 2026-05-09 devnet
/// gameplay E2E sweep: human-vs-agent's MCP-driven agent submitted its
/// deposit_stake to mainnet (the only RPC the cached builder knew about)
/// while the test was running on devnet → AccountNotInitialized for the
/// devnet-only Tournament PDA.
pub struct GameSessionManager {
    sessions: RwLock<HashMap<String, Arc<Mutex<GameSession>>>>,
    ws_sinks: RwLock<HashMap<String, Arc<Mutex<WsSink>>>>,
    tx_builders: RwLock<HashMap<String, GameTxBuilder>>,
    /// Cancellation tokens for background WS listener tasks.
    /// Cancelled in `cleanup()` to stop dangling reconnect loops.
    ws_cancel_tokens: RwLock<HashMap<String, CancellationToken>>,
    game_api_url: String,
    solana_rpc_url: String,
    solana_rpc_url_mainnet: String,
    solana_rpc_url_devnet: String,
    db: Arc<FirestoreDb>,
}

/// Inputs assembled from the in-memory session for `build_reveal_tx`.
/// Only constructed once all four fields are present — the loader returns
/// `None` while still waiting for the opponent's reveal_data.
struct RevealInputs {
    game_id: u64,
    tournament_id: u64,
    preimage: [u8; 32],
    r_matchup: [u8; 32],
}

fn decode_signed_tx(signed_tx_b64: &str) -> Result<Vec<u8>> {
    base64::engine::general_purpose::STANDARD
        .decode(signed_tx_b64)
        .context("invalid base64 signed transaction")
}

/// Whether `action` is a cross-chain (Solana↔EVM) Solana-leg tx submitted via
/// `game_submit_tx`. These broadcast like any Solana tx but, unlike same-chain
/// actions, drive no `GameSession` state machine — settlement happens off the
/// MCP session via checkpoint relay + settle.
fn is_xchain_solana_action(action: &str) -> bool {
    matches!(
        action,
        "create_xmatch"
            | "lock_xtranche"
            | "refund_xmatch_nocert"
            | "refund_xmatch_timeout"
            | "settle_xmatch"
    )
}

/// Emit the critical business event for a confirmed cross-chain Solana-leg tx.
/// Cross-chain matches have no on-chain `Game` and no MCP session state to
/// advance, so observability is the entire post-submit responsibility.
fn log_xchain_solana_action(wallet: &str, action: &str, sig: &str) {
    let event = match action {
        "create_xmatch" => "cross-chain Solana leg funded",
        "lock_xtranche" => "cross-chain Solana leg locked",
        "refund_xmatch_nocert" => "cross-chain Solana refund (no-cert) settled",
        "refund_xmatch_timeout" => "cross-chain Solana refund (timeout) settled",
        "settle_xmatch" => "cross-chain Solana leg settled",
        _ => "cross-chain Solana action confirmed",
    };
    tracing::info!(wallet = %wallet, action = %action, sig = %sig, "{event}");
}

/// Agent-facing result for a confirmed cross-chain Solana-leg tx, including the
/// next step the agent should take.
fn xchain_submit_result(action: &str, sig: &str) -> serde_json::Value {
    let next_step = match action {
        "create_xmatch" => {
            "Your Solana leg is funded. Once the EVM leg funds too, lock your leg \
             via xchain_build_lock_xmatch (permissionless — you pay only the fee); the EVM \
             player locks theirs via xchain_build_lock. Both legs must be Locked before settle."
        }
        "lock_xtranche" => {
            "Your Solana leg is locked. After both legs lock, play out the match \
             (co-signed checkpoints) and assemble the settle with your session key."
        }
        "settle_xmatch" => {
            "Settlement submitted. Read your PlayerProfile / on-chain balance \
             to confirm the payout."
        }
        _ => "Refund submitted; your stake has been returned to your wallet.",
    };
    serde_json::json!({
        "tx_signature": sig,
        "status": "submitted",
        "next_step": next_step,
    })
}

/// Pure function: route a per-call network argument to one of the three
/// configured RPC URLs. Extracted so `GameSessionManager::pick_rpc_url`
/// can be tested without standing up a real `FirestoreDb`.
fn pick_rpc_url_for_network<'a>(
    network: Option<&str>,
    default: &'a str,
    mainnet: &'a str,
    devnet: &'a str,
) -> &'a str {
    match network {
        Some("devnet") => devnet,
        Some("mainnet") => mainnet,
        _ => default,
    }
}

/// `MATCHUP_TYPE_UNSET` from the on-chain `Game` struct. Mirrored here
/// so we don't take a build-time dep on the program crate just to read
/// one constant. The on-chain value is the source of truth — see
/// `programs/coordination-game/src/state/game.rs`.
const MATCHUP_TYPE_UNSET: u8 = 255;

/// Decide whether to pass `r_matchup` on the reveal_guess transaction
/// based on the current on-chain matchup_type.
///
/// `Some(r_matchup)` when we are the first revealer (`matchup_type ==
/// MATCHUP_TYPE_UNSET`); `None` when the opponent already revealed
/// (`matchup_type` is now 0 or 1) — the on-chain program rejects with
/// `RMatchupMismatch` (error 6032) if the second revealer passes
/// r_matchup. Pure function so the contract is tested without the
/// validator. Mirrors the same guard in the browser's
/// `frontend/coordination-game/src/hooks/reveal-tx.ts:submitRevealTx`.
fn pick_reveal_matchup_arg(matchup_type: u8, r_matchup: [u8; 32]) -> Option<[u8; 32]> {
    if matchup_type == MATCHUP_TYPE_UNSET {
        Some(r_matchup)
    } else {
        None
    }
}

/// Parse a hex-encoded `[u8; 32]` `r_matchup` reveal value. Used by
/// `build_reveal_tx` to convert the websocket-delivered hex string back into
/// the fixed-size byte array the on-chain instruction expects.
fn parse_r_matchup_hex(reveal_hex: &str) -> Result<[u8; 32]> {
    debug_assert!(!reveal_hex.is_empty(), "reveal_hex must be non-empty");
    let bytes = hex::decode(reveal_hex).context("invalid hex in r_matchup")?;
    let len = bytes.len();
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("r_matchup must be 32 bytes, got {}", len))?;
    // Postcondition: array length is exactly 32 bytes (enforced by type).
    debug_assert_eq!(arr.len(), 32, "r_matchup must be 32 bytes");
    Ok(arr)
}

/// Read the matchmaker pubkey from the GlobalConfig PDA on-chain.
///
/// `GlobalConfig` layout: 8-byte discriminator + 32-byte authority +
/// 32-byte matchmaker + ... — we slice `[40..72]` to extract the matchmaker.
async fn read_matchmaker_pubkey(tx_builder: &GameTxBuilder) -> Result<solana_sdk::pubkey::Pubkey> {
    let (global_config_pda, _) = game_chain::pda::global_config_pda();
    let config_data = tx_builder
        .rpc()
        .get_account_data(&global_config_pda)
        .await
        .context("failed to read global_config")?;
    // Precondition: account data must contain at least disc + authority + matchmaker.
    anyhow::ensure!(config_data.len() >= 72, "global_config data too short");
    let matchmaker =
        solana_sdk::pubkey::Pubkey::try_from(&config_data[40..72]).context("parse matchmaker")?;
    // Postcondition: a real on-chain matchmaker is non-default.
    debug_assert!(
        matchmaker != solana_sdk::pubkey::Pubkey::default(),
        "matchmaker must not be the default pubkey"
    );
    Ok(matchmaker)
}

/// Read the next-allocated game id from the game_counter PDA. Used by P1 to
/// know what `game_id` the create_game tx will assign.
async fn read_next_game_id(tx_builder: &GameTxBuilder) -> Result<u64> {
    let (counter_pda, _) = game_chain::pda::game_counter_pda();
    let counter_data = tx_builder
        .rpc()
        .get_account_data(&counter_pda)
        .await
        .context("failed to read game_counter")?;
    // Precondition: data must contain at least the 8-byte disc + u64 counter.
    anyhow::ensure!(counter_data.len() >= 16, "game_counter data too short");
    let game_id = u64::from_le_bytes(counter_data[8..16].try_into().context("parse count")?);
    Ok(game_id)
}

/// Pump the Queued → Matched state transition once `match_found` has arrived
/// over the WebSocket. Idempotent for any non-Queued state. Reads from
/// `match_found` and writes `session_id`, `role`, and `state` on the session.
fn advance_queued_to_matched(s: &mut GameSession) -> Result<()> {
    debug_assert!(
        s.match_found.is_some(),
        "advance requires match_found populated"
    );
    if s.state != GameSessionState::Queued {
        return Ok(());
    }
    let (sid, role) = {
        let mf = s.match_found.as_ref().context("match_found missing")?;
        (mf.session_id.clone(), mf.role)
    };
    s.session_id = Some(sid);
    s.role = Some(role);
    s.state = GameSessionState::Matched;
    // Postcondition: state advanced and identifying fields are populated.
    debug_assert_eq!(
        s.state,
        GameSessionState::Matched,
        "state must be Matched after advance"
    );
    Ok(())
}

/// Construct the "matched but no tx yet" `check_match` response — the agent
/// is matched on-chain but check_match doesn't need to return an unsigned tx
/// this poll (e.g. P2 still waiting for game_ready).
fn match_status_matched(game_id: Option<u64>, role: Option<u8>) -> MatchStatus {
    let status = MatchStatus {
        status: "matched".to_string(),
        game_id,
        role,
        unsigned_tx: None,
        action: None,
        matchmaker_signature: None,
        blockhash: None,
    };
    debug_assert!(status.unsigned_tx.is_none(), "matched must not carry a tx");
    status
}

/// Construct the "already in_game" `check_match` response (no transaction work needed).
fn match_status_in_game(game_id: Option<u64>, role: Option<u8>) -> MatchStatus {
    // Postcondition: an in_game status only exposes session metadata, never a tx.
    let status = MatchStatus {
        status: "in_game".to_string(),
        game_id,
        role,
        unsigned_tx: None,
        action: None,
        matchmaker_signature: None,
        blockhash: None,
    };
    debug_assert!(status.unsigned_tx.is_none(), "in_game must not carry a tx");
    status
}

/// Construct the "still queued" `check_match` response — no match yet.
fn match_status_queued() -> MatchStatus {
    let status = MatchStatus {
        status: "queued".to_string(),
        game_id: None,
        role: None,
        unsigned_tx: None,
        action: None,
        matchmaker_signature: None,
        blockhash: None,
    };
    debug_assert!(status.game_id.is_none(), "queued must have no game_id");
    debug_assert!(status.unsigned_tx.is_none(), "queued must have no tx");
    status
}

/// Reconstruct an in-memory `GameSession` from a persisted Firestore document.
/// Used by `try_restore_persisted_session` to keep the (large) struct
/// initializer out of the orchestration path.
fn build_restored_session(
    wallet: &str,
    persisted: &PersistedGameSession,
    state: GameSessionState,
) -> GameSession {
    // Precondition: the wallet identifier must already be canonicalized.
    debug_assert!(!wallet.is_empty(), "wallet must be non-empty");
    // Postcondition (asserted by usage): when state is past Connected, a
    // session_id should normally exist — but Resolved sessions are filtered
    // out by the caller, so we don't enforce that here.
    let preimage = persisted
        .commit_preimage_hex
        .as_ref()
        .and_then(|h| hex::decode(h).ok())
        .and_then(|bytes| <[u8; 32]>::try_from(bytes.as_slice()).ok());
    debug_assert!(
        state != GameSessionState::Resolved,
        "Resolved sessions must not be restored"
    );
    GameSession {
        wallet: wallet.to_string(),
        jwt: persisted.jwt.clone(),
        state,
        game_id: persisted.game_id,
        tournament_id: persisted.tournament_id,
        session_id: persisted.session_id.clone(),
        role: persisted.role,
        chat_buffer: Vec::new(),
        match_found: None,
        matchup_commitment: persisted.matchup_commitment.clone(),
        commit_preimage: preimage,
        game_ready: persisted.game_ready,
        reveal_data: persisted.reveal_data.clone(),
        // Restored sessions don't carry a network — the next per-call tool
        // (find_match) will set it. Mainnet-default is correct for
        // restored sessions in production where most traffic is mainnet.
        network: None,
    }
}

impl GameSessionManager {
    pub fn new(
        game_api_url: String,
        solana_rpc_url: String,
        solana_rpc_url_mainnet: String,
        solana_rpc_url_devnet: String,
        db: Arc<FirestoreDb>,
    ) -> Self {
        assert!(!game_api_url.is_empty(), "game_api_url must not be empty");
        assert!(
            !solana_rpc_url.is_empty(),
            "solana_rpc_url must not be empty"
        );
        assert!(
            !solana_rpc_url_mainnet.is_empty(),
            "solana_rpc_url_mainnet must not be empty"
        );
        assert!(
            !solana_rpc_url_devnet.is_empty(),
            "solana_rpc_url_devnet must not be empty"
        );
        Self {
            sessions: RwLock::new(HashMap::new()),
            ws_sinks: RwLock::new(HashMap::new()),
            tx_builders: RwLock::new(HashMap::new()),
            ws_cancel_tokens: RwLock::new(HashMap::new()),
            game_api_url,
            solana_rpc_url,
            solana_rpc_url_mainnet,
            solana_rpc_url_devnet,
            db,
        }
    }

    /// Resolve the RPC URL for a per-call network argument. `None` and
    /// `Some("mainnet")` both map to the configured mainnet URL; `Some("devnet")`
    /// to devnet. Anything else falls back to the cluster default —
    /// matches the existing `?network=` semantics on the orchestrator.
    fn pick_rpc_url(&self, network: Option<&str>) -> &str {
        pick_rpc_url_for_network(
            network,
            &self.solana_rpc_url,
            &self.solana_rpc_url_mainnet,
            &self.solana_rpc_url_devnet,
        )
    }

    /// Build an ephemeral GameTxBuilder for the requested network. The
    /// cached `tx_builders` map is kept for backward compat (cluster-default
    /// network only) and used by paths that don't yet take a network arg.
    /// Tools that DO take a network arg (`game_find_match`, `game_submit_tx`)
    /// call this so the broadcast lands on the right cluster.
    fn tx_builder_for_network(
        &self,
        wallet_pubkey: solana_sdk::pubkey::Pubkey,
        network: Option<&str>,
    ) -> GameTxBuilder {
        GameTxBuilder::new(self.pick_rpc_url(network), wallet_pubkey)
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

    /// Persist a same-chain EVM guess preimage (write-through) so the reveal step
    /// can recover it after a pod restart. Keyed by wallet+game_id. Fatal on
    /// failure — like the Solana commit preimage, losing it strands the reveal.
    pub async fn store_evm_preimage(
        &self,
        wallet: &str,
        game_id: &str,
        preimage: [u8; 32],
    ) -> Result<()> {
        let doc = EvmPreimageDoc {
            id: format!("{wallet}:{game_id}"),
            wallet: wallet.to_string(),
            game_id: game_id.to_string(),
            preimage_hex: hex::encode(preimage),
            updated_at: firestore::FirestoreTimestamp(chrono::Utc::now()),
        };
        self.db
            .fluent()
            .update()
            .in_col(MCP_EVM_PREIMAGES_COLLECTION)
            .document_id(&doc.id)
            .object(&doc)
            .execute::<EvmPreimageDoc>()
            .await
            .map_err(|e| {
                tracing::error!(wallet = %wallet, game_id, error = %e, "CRITICAL: failed to persist EVM commit preimage");
                anyhow::anyhow!("failed to persist EVM commit preimage — retry game_evm_commit_guess")
            })?;
        Ok(())
    }

    /// Load a same-chain EVM guess preimage for `wallet`+`game_id`, if stored.
    pub async fn load_evm_preimage(&self, wallet: &str, game_id: &str) -> Result<Option<[u8; 32]>> {
        let doc: Option<EvmPreimageDoc> = self
            .db
            .fluent()
            .select()
            .by_id_in(MCP_EVM_PREIMAGES_COLLECTION)
            .obj::<EvmPreimageDoc>()
            .one(format!("{wallet}:{game_id}"))
            .await
            .map_err(|e| anyhow::anyhow!("failed to load EVM preimage: {e}"))?;
        let Some(doc) = doc else { return Ok(None) };
        let bytes = hex::decode(&doc.preimage_hex)
            .map_err(|e| anyhow::anyhow!("stored EVM preimage is not hex: {e}"))?;
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("stored EVM preimage is not 32 bytes"))?;
        Ok(Some(arr))
    }

    /// Load a persisted session from Firestore, if one exists.
    ///
    /// A read failure PROPAGATES rather than degrading to `None`: registering
    /// fresh on a transient Firestore error means the next write-through
    /// overwrites the persisted doc, destroying the stored `commit_preimage`
    /// this layer exists to protect. `.one()` already distinguishes not-found
    /// (`Ok(None)`) from transport failure (`Err`), so fresh registration still
    /// happens for a genuinely absent doc.
    async fn load_persisted_session(&self, wallet: &str) -> Result<Option<PersistedGameSession>> {
        self.db
            .fluent()
            .select()
            .by_id_in(MCP_SESSIONS_COLLECTION)
            .obj::<PersistedGameSession>()
            .one(wallet)
            .await
            .map_err(|e| anyhow::anyhow!("failed to load persisted session for {wallet}: {e}"))
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
        // EVM (0x) wallets are routed to the cross-chain path in the
        // register_wallet handler before reaching here; this method only ever
        // sees Solana base58 pubkeys.
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
        if let Some(persisted) = self.load_persisted_session(&wallet).await? {
            if let Some(restored_balance) = self
                .try_restore_persisted_session(&wallet, persisted, tx_builder, balance)
                .await
            {
                return Ok((wallet, restored_balance));
            }
            // try_restore returned None → session was Resolved and the
            // tx_builder was consumed; recreate it for the fresh-register path.
        }

        let tx_builder = GameTxBuilder::new(&self.solana_rpc_url, pubkey);
        self.register_fresh_session(&wallet, tx_builder, balance)
            .await;
        Ok((wallet, balance))
    }

    /// Try to restore a persisted Firestore game session for `wallet`.
    /// Returns `Some(balance)` when the session was active and was
    /// re-hydrated into the in-memory map (with a best-effort WS reconnect).
    /// Returns `None` when the session was Resolved (already cleaned up by
    /// this function) and the caller should fall through to fresh registration.
    async fn try_restore_persisted_session(
        &self,
        wallet: &str,
        persisted: PersistedGameSession,
        tx_builder: GameTxBuilder,
        balance: u64,
    ) -> Option<u64> {
        // Precondition: the wallet string is the canonical pubkey form.
        debug_assert!(!wallet.is_empty(), "wallet must be non-empty");
        let state =
            GameSessionState::from_str(&persisted.state).unwrap_or(GameSessionState::Connected);

        if state == GameSessionState::Resolved {
            // Session was resolved — clean it up so the caller can register fresh.
            self.delete_persisted_session(wallet).await;
            return None;
        }

        tracing::info!(
            wallet = %wallet,
            state = persisted.state,
            game_id = ?persisted.game_id,
            "restoring persisted session from Firestore"
        );

        let restored = Arc::new(Mutex::new(build_restored_session(
            wallet, &persisted, state,
        )));
        self.sessions
            .write()
            .await
            .insert(wallet.to_string(), restored.clone());
        self.tx_builders
            .write()
            .await
            .insert(wallet.to_string(), tx_builder);

        // Re-establish WS if JWT is available (with timeout — JWT may be expired).
        if !persisted.jwt.is_empty() {
            self.spawn_ws_reconnect_for_restored(wallet, &persisted.jwt, &restored)
                .await;
        }
        // Postcondition: in-memory maps now have an entry for this wallet.
        debug_assert!(
            self.sessions.read().await.contains_key(wallet),
            "session must be registered after restore"
        );
        Some(balance)
    }

    /// Connect a WS for a restored session and spawn the read loop. All errors
    /// are logged and swallowed — the caller already returned `Some(balance)`
    /// so the agent can retry by calling `register_wallet` again if needed.
    async fn spawn_ws_reconnect_for_restored(
        &self,
        wallet: &str,
        jwt: &str,
        restored: &Arc<Mutex<GameSession>>,
    ) {
        // Use the session's persisted network so reconnects bind to the
        // same per-network connection map the original session used.
        let network = restored.lock().await.network.clone();
        let ws_result = tokio::time::timeout(
            tokio::time::Duration::from_secs(10),
            WsConnection::connect_with_network(&self.game_api_url, jwt, network.as_deref()),
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
                let session_clone = Arc::clone(restored);
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
                tracing::info!(wallet = %wallet, "WS re-established for restored session");
            }
        }
    }

    /// Register a brand-new game session (no persisted state). JWT is empty —
    /// auth happens later when the agent submits a signed deposit_stake tx.
    async fn register_fresh_session(&self, wallet: &str, tx_builder: GameTxBuilder, balance: u64) {
        debug_assert!(!wallet.is_empty(), "wallet must be non-empty");
        let session = Arc::new(Mutex::new(GameSession {
            wallet: wallet.to_string(),
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
            network: None,
        }));

        self.sessions
            .write()
            .await
            .insert(wallet.to_string(), session);
        self.tx_builders
            .write()
            .await
            .insert(wallet.to_string(), tx_builder);

        tracing::info!(
            service = "coordination-mcp-server",
            wallet = %wallet,
            balance,
            "wallet registered (pubkey only, non-custodial)"
        );
        // Postcondition: the session is queryable via `is_registered`.
        debug_assert!(
            self.sessions.read().await.contains_key(wallet),
            "fresh session must be registered"
        );
    }

    /// Build an unsigned deposit_stake transaction for the agent to sign.
    ///
    /// `network` selects which RPC to read game_counter from and which
    /// blockhash to use. `None` falls back to the cluster default;
    /// `Some("devnet")` / `Some("mainnet")` route explicitly. The agent
    /// signs locally then submits via `submit_signed_game_tx` (which must
    /// be passed the same `network` so the broadcast lands on the same
    /// cluster).
    pub async fn build_find_match_tx(
        &self,
        wallet: &str,
        tournament_id: u64,
        network: Option<&str>,
    ) -> Result<game_chain::client::UnsignedTx> {
        // Existence in the cache is the proxy for "wallet is registered".
        anyhow::ensure!(
            self.tx_builders.read().await.contains_key(wallet),
            "no session for wallet"
        );
        let pubkey = solana_sdk::pubkey::Pubkey::from_str(wallet).context("invalid wallet")?;
        let tx_builder = self.tx_builder_for_network(pubkey, network);

        // Pin the network on the session so subsequent game-api calls
        // (auth, queue join, cosign, game-state notifications) all route
        // to the same cluster the deposit_stake message we're about to
        // build will land on. Without this, after_deposit_stake would
        // try to authenticate by tx-signature lookup on the cluster
        // default RPC — fine on mainnet, but a devnet broadcast can't
        // be found from a mainnet RPC.
        if let Some(session) = self.sessions.read().await.get(wallet) {
            session.lock().await.network = network.map(str::to_string);
        }

        // Check balance before building the stake tx.
        let balance = tx_builder
            .rpc()
            .get_balance(&tx_builder.pubkey())
            .await
            .context("failed to check balance")?;
        let min_balance = game_chain::FIXED_STAKE_LAMPORTS + STAKE_FEE_BUFFER_LAMPORTS;
        anyhow::ensure!(
            balance >= min_balance,
            "insufficient balance: need at least {} SOL to play, have {} SOL",
            min_balance as f64 / 1_000_000_000.0,
            balance as f64 / 1_000_000_000.0
        );

        let unsigned = tx_builder.build_deposit_stake(tournament_id).await?;

        // Store tournament_id in session for later queue join.
        if let Some(session) = self.sessions.read().await.get(wallet) {
            session.lock().await.tournament_id = Some(tournament_id);
        }

        Ok(unsigned)
    }

    /// Submit a signed game transaction and run action-specific
    /// post-submission logic. Each action's branch lives in its own helper
    /// (after_deposit_stake, after_join_game, etc.).
    ///
    /// `network` must match the network used to build the unsigned tx:
    /// the broadcast goes to that cluster's RPC. Passing the wrong network
    /// here will make the RPC reject the tx with `BlockhashNotFound`.
    pub async fn submit_signed_game_tx(
        &self,
        wallet: &str,
        signed_tx_b64: &str,
        action: &str,
        network: Option<&str>,
    ) -> Result<serde_json::Value> {
        let signed_bytes = decode_signed_tx(signed_tx_b64)?;
        let sig_str = self
            .broadcast_signed(wallet, action, &signed_bytes, network)
            .await?;

        let session = self
            .sessions
            .read()
            .await
            .get(wallet)
            .context("no session for wallet")?
            .clone();

        match action {
            "deposit_stake" => {
                self.after_deposit_stake(wallet, &sig_str, &session, network)
                    .await?
            }
            "join_game" => self.after_join_game(wallet, &session).await?,
            "commit_guess" => self.after_commit_guess(wallet, &session).await?,
            "reveal_guess" => self.after_reveal_guess(wallet, &session).await,
            "create_game" => self.after_create_game(wallet, &session).await?,
            _ if is_xchain_solana_action(action) => {
                // Cross-chain Solana-leg actions don't drive the same-chain
                // GameSession state machine (cross-chain resolution happens via
                // checkpoint relay + settle, not create→commit→reveal). The
                // broadcast above already landed the tx; here we just emit the
                // critical business event so it's queryable in Cloud Logging.
                log_xchain_solana_action(wallet, action, &sig_str);
                return Ok(xchain_submit_result(action, &sig_str));
            }
            other => {
                tracing::warn!(wallet = %wallet, action = other, "unknown action for game_submit_tx");
            }
        }

        Ok(serde_json::json!({
            "tx_signature": sig_str,
            "status": "submitted",
        }))
    }

    /// Submit the signed bytes through a `GameTxBuilder` for the requested
    /// network and return the resulting signature as a base58 string.
    async fn broadcast_signed(
        &self,
        wallet: &str,
        action: &str,
        signed_bytes: &[u8],
        network: Option<&str>,
    ) -> Result<String> {
        anyhow::ensure!(
            self.tx_builders.read().await.contains_key(wallet),
            "no session for wallet"
        );
        let pubkey = solana_sdk::pubkey::Pubkey::from_str(wallet).context("invalid wallet")?;
        let tx_builder = self.tx_builder_for_network(pubkey, network);
        tracing::info!(
            wallet = %wallet,
            action = %action,
            tx_len = signed_bytes.len(),
            network = ?network,
            "submitting signed transaction"
        );
        let sig = match tx_builder.submit_signed(signed_bytes).await {
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
        Ok(sig_str)
    }

    /// After deposit_stake: always re-authenticate. Reusing a previous
    /// stake's JWT against a fresh stake hits an ExpiredSignature /
    /// wrong-session 401 once the prior token TTL elapses — the fresh
    /// stake we just broadcast IS the auth credential.
    ///
    /// `network` must match the network the deposit_stake was broadcast on
    /// — the game-api fetches the tx by signature from the matching RPC
    /// to verify wallet ownership; mismatched network = "transaction not
    /// found".
    async fn after_deposit_stake(
        &self,
        wallet: &str,
        sig_str: &str,
        session: &Arc<Mutex<GameSession>>,
        network: Option<&str>,
    ) -> Result<()> {
        if !session.lock().await.jwt.is_empty() {
            if let Some(token) = self.ws_cancel_tokens.write().await.remove(wallet) {
                token.cancel();
            }
            self.ws_sinks.write().await.remove(wallet);
            session.lock().await.jwt.clear();
        }

        let api_client =
            GameApiClient::new(&self.game_api_url)?.with_network(network.map(str::to_string));
        let auth_resp = api_client.session_auth(wallet, sig_str).await?;
        let jwt = auth_resp.token.clone();
        self.spawn_ws_listener(wallet, &jwt, session, network)
            .await?;
        session.lock().await.jwt = jwt;
        {
            let s = session.lock().await;
            self.persist_session(&s).await?;
        }
        tracing::info!(wallet = %wallet, "authenticated via deposit_stake tx");

        let (jwt, tournament_id) = {
            let s = session.lock().await;
            (s.jwt.clone(), s.tournament_id)
        };
        if let Some(tid) = tournament_id {
            self.join_queue_after_stake(wallet, &jwt, tid).await?;
        }
        Ok(())
    }

    async fn spawn_ws_listener(
        &self,
        wallet: &str,
        jwt: &str,
        session: &Arc<Mutex<GameSession>>,
        network: Option<&str>,
    ) -> Result<()> {
        // Carry `network` into the WS connect URL so game-api binds the
        // ws connection to the right per-network connection map. Without
        // this, an MCP agent's HTTP queue join lands in `network=devnet`
        // but its WebSocket registers as `network=mainnet` (the default),
        // so `filter_stale_entries` treats the queue entry as orphaned
        // and drops it from candidate pairings — the matchmaker then
        // pairs the human-browser with a stale grok-agent instead of
        // the MCP-driven agent. Surfaced 2026-05-09 by full-game devnet
        // e2e (game-api logs showed `WebSocket upgrade ... network=mainnet`
        // for an mcp-agent that joined queue as devnet).
        let ws = WsConnection::connect_with_network(&self.game_api_url, jwt, network).await?;
        let (sink, stream) = ws.into_split();
        let ws_sink = Arc::new(Mutex::new(sink));

        let session_clone = Arc::clone(session);
        let sink_clone = Arc::clone(&ws_sink);
        let api_url = self.game_api_url.clone();
        let cancel_token = CancellationToken::new();
        let token_clone = cancel_token.clone();
        tokio::spawn(async move {
            ws_listener_with_reconnect(session_clone, stream, sink_clone, api_url, token_clone)
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
        Ok(())
    }

    async fn after_join_game(&self, wallet: &str, session: &Arc<Mutex<GameSession>>) -> Result<()> {
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
        let network = session.lock().await.network.clone();
        if !session_id.is_empty() {
            let api_client = GameApiClient::new(&self.game_api_url)?.with_network(network);
            api_client
                .post_games_joined(&jwt, game_id, &session_id)
                .await?;
        }
        tracing::info!(wallet = %wallet, game_id, "P2 joined game on-chain");
        Ok(())
    }

    async fn after_commit_guess(
        &self,
        wallet: &str,
        session: &Arc<Mutex<GameSession>>,
    ) -> Result<()> {
        // Must persist preimage before we drop the lock — it has to survive a
        // pod restart between commit and reveal.
        let (jwt, session_id) = {
            let mut s = session.lock().await;
            s.state = GameSessionState::Committed;
            self.persist_session(&s).await?;
            (s.jwt.clone(), s.session_id.clone().unwrap_or_default())
        };
        let network = session.lock().await.network.clone();
        if !session_id.is_empty() {
            let api_client = GameApiClient::new(&self.game_api_url)?.with_network(network);
            if let Err(e) = api_client.post_games_committed(&jwt, &session_id).await {
                tracing::warn!(wallet = %wallet, error = %e, "post_games_committed failed (non-fatal)");
            }
        }
        tracing::info!(wallet = %wallet, "committed guess on-chain");
        Ok(())
    }

    async fn after_reveal_guess(&self, wallet: &str, session: &Arc<Mutex<GameSession>>) {
        // Cleanup of WS sink + cancel token runs after the response is sent
        // by the caller (see callers of submit_signed_game_tx).
        session.lock().await.state = GameSessionState::Resolved;
        // Write the terminal state through, like every other transition. Without
        // this the doc stays Committed, so a pod restart resurrects a stale
        // Committed session for an already-resolved game — and the restore path's
        // resolved-cleanup branch never fires.
        {
            let s = session.lock().await;
            if let Err(e) = self.persist_session(&s).await {
                tracing::warn!(wallet = %wallet, error = %e, "failed to persist resolved session");
            }
        }
        tracing::info!(wallet = %wallet, "revealed guess on-chain");

        // Notify game-api so the backend broadcasts the WS
        // `game_resolved` message to the OPPONENT's browser/agent. Pre-fix
        // (commit pre-2026-05-09) this notification was missing, so a
        // human-vs-mcp-agent flow where the agent revealed second left
        // the human's browser stranded in the "Revealing" phase until
        // useGamePoll's 60s tick fired. The human-vs-agent e2e test then
        // timed out at the 120s outcome assertion. Best-effort — any
        // failure here is logged but doesn't override the on-chain
        // result.
        if let Err(e) = self.notify_game_resolved(wallet, session).await {
            tracing::warn!(
                wallet = %wallet,
                error = %e,
                "after_reveal_guess: failed to notify backend (non-fatal)"
            );
        }
    }

    /// Read the on-chain Game state and, if Resolved, POST /games/resolved
    /// so the backend broadcasts to all session WS connections.
    async fn notify_game_resolved(
        &self,
        wallet: &str,
        session: &Arc<Mutex<GameSession>>,
    ) -> Result<()> {
        use std::str::FromStr;
        let (game_id, session_id, jwt, network) = {
            let s = session.lock().await;
            (
                s.game_id.unwrap_or(0),
                s.session_id.clone().unwrap_or_default(),
                s.jwt.clone(),
                s.network.clone(),
            )
        };
        if game_id == 0 || session_id.is_empty() || jwt.is_empty() {
            return Ok(());
        }
        let pubkey = solana_sdk::pubkey::Pubkey::from_str(wallet)
            .context("after_reveal_guess: invalid wallet")?;
        let tx_builder = self.tx_builder_for_network(pubkey, network.as_deref());
        let game = tx_builder
            .read_game(game_id)
            .await?
            .context("after_reveal_guess: game account not found")?;
        if game.state != game_chain::GameState::Resolved {
            // Opponent hasn't revealed yet — nothing to broadcast.
            return Ok(());
        }
        let api_client = GameApiClient::new(&self.game_api_url)?.with_network(network);
        api_client
            .post_games_resolved(
                &jwt,
                game_id,
                game.p1_guess,
                game.p2_guess,
                0,
                0,
                game.matchup_type,
                game.first_committer,
                None,
                None,
            )
            .await
            .context("after_reveal_guess: post_games_resolved failed")?;
        tracing::info!(
            wallet = %wallet,
            game_id,
            "notified backend of game resolution"
        );
        Ok(())
    }

    async fn after_create_game(
        &self,
        wallet: &str,
        session: &Arc<Mutex<GameSession>>,
    ) -> Result<()> {
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
        let network = session.lock().await.network.clone();
        if !session_id.is_empty() {
            let api_client = GameApiClient::new(&self.game_api_url)?.with_network(network);
            api_client
                .post_games_started(&jwt, game_id, &session_id)
                .await?;
        }
        tracing::info!(wallet = %wallet, game_id, "P1 created game on-chain");
        Ok(())
    }

    /// Internal: join the matchmaking queue after stake deposit.
    async fn join_queue_after_stake(
        &self,
        wallet: &str,
        jwt: &str,
        tournament_id: u64,
    ) -> Result<()> {
        // Read the session's pinned network so this queue join routes to
        // the same cluster the deposit_stake landed on.
        let network = self
            .sessions
            .read()
            .await
            .get(wallet)
            .ok_or_else(|| anyhow::anyhow!("no session for wallet"))?
            .lock()
            .await
            .network
            .clone();

        // Clear stale queue entry from previous crash.
        let api_client = GameApiClient::new(&self.game_api_url)?.with_network(network);
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
            return Ok(match_status_in_game(s.game_id, s.role));
        }

        // Still waiting for match.
        if s.match_found.is_none() {
            return Ok(match_status_queued());
        }

        // Match found but not yet processed — advance Queued → Matched.
        advance_queued_to_matched(&mut s)?;

        let role = s.role.unwrap_or(1);
        let tournament_id = s.tournament_id.context("tournament_id not set")?;
        let _session_id = s.session_id.clone().context("session_id not set")?;
        let jwt = s.jwt.clone();

        if role == 0 {
            let commitment_hex = s
                .matchup_commitment
                .clone()
                .context("role=0 but no matchup_commitment received")?;
            drop(s);
            return self
                .check_match_player_one(wallet, tournament_id, &commitment_hex, &jwt, &session)
                .await;
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
                return self
                    .check_match_player_two(wallet, tournament_id, game_id, &session)
                    .await;
            }
        }

        Ok(match_status_matched(s.game_id, s.role))
    }

    /// Player 1 branch of `check_match`: build the create_game tx (with
    /// matchmaker cosignature), record the resulting game_id on the session,
    /// and return the `needs_signature` MatchStatus.
    async fn check_match_player_one(
        &self,
        wallet: &str,
        tournament_id: u64,
        commitment_hex: &str,
        jwt: &str,
        session: &Arc<Mutex<GameSession>>,
    ) -> Result<MatchStatus> {
        debug_assert!(!wallet.is_empty(), "wallet must be non-empty");
        debug_assert!(
            !commitment_hex.is_empty(),
            "commitment_hex must be non-empty for P1"
        );
        let (unsigned, matchmaker_sig, game_id) = self
            .build_create_game_tx(wallet, tournament_id, commitment_hex, jwt)
            .await?;

        // Store game_id but don't transition to InGame — that happens after submit.
        let mut s = session.lock().await;
        s.game_id = Some(game_id);

        Ok(MatchStatus {
            status: "needs_signature".to_string(),
            game_id: Some(game_id),
            role: Some(0),
            unsigned_tx: Some(unsigned.transaction_b64),
            action: Some("create_game".to_string()),
            matchmaker_signature: Some(matchmaker_sig),
            blockhash: Some(unsigned.blockhash),
        })
    }

    /// Player 2 branch of `check_match`: build the join_game tx, store the
    /// game_id on the session, and return the `needs_signature` MatchStatus.
    async fn check_match_player_two(
        &self,
        wallet: &str,
        tournament_id: u64,
        game_id: u64,
        session: &Arc<Mutex<GameSession>>,
    ) -> Result<MatchStatus> {
        debug_assert!(!wallet.is_empty(), "wallet must be non-empty");
        debug_assert!(game_id > 0, "game_id must be a real on-chain id");
        // Same network-routing fix as build_create_game_tx: build the
        // join_game tx with the session's network so its blockhash matches
        // the cluster the agent will broadcast to.
        let network = session.lock().await.network.clone();
        let pubkey = solana_sdk::pubkey::Pubkey::from_str(wallet).context("invalid wallet")?;
        let tx_builder = self.tx_builder_for_network(pubkey, network.as_deref());
        let unsigned = tx_builder.build_join_game(game_id, tournament_id).await?;

        // Store game_id but don't transition to InGame yet —
        // that happens after the agent submits the signed tx.
        let mut s = session.lock().await;
        s.game_id = Some(game_id);

        tracing::info!(wallet = %wallet, game_id, "P2 join_game tx built (needs signing)");

        Ok(MatchStatus {
            status: "needs_signature".to_string(),
            game_id: Some(game_id),
            role: Some(1),
            unsigned_tx: Some(unsigned.transaction_b64),
            action: Some("join_game".to_string()),
            matchmaker_signature: None,
            blockhash: Some(unsigned.blockhash),
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

        // Use the session's pinned network (set in build_find_match_tx)
        // so the blockhash on the unsigned commit_guess tx is fresh on
        // the same cluster the agent will broadcast to.
        let network = session.lock().await.network.clone();
        let pubkey = solana_sdk::pubkey::Pubkey::from_str(wallet).context("invalid wallet")?;
        let tx_builder = self.tx_builder_for_network(pubkey, network.as_deref());
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

        let reveal_inputs = match self.load_reveal_inputs(&session).await? {
            Some(inputs) => inputs,
            None => return Ok(None), // still waiting for opponent
        };
        let RevealInputs {
            game_id,
            tournament_id,
            preimage,
            r_matchup,
        } = reveal_inputs;

        // Same network-routing fix as build_commit_guess_tx — read the
        // session's pinned network so the on-chain Game read + reveal
        // blockhash both come from the right cluster.
        let network = session.lock().await.network.clone();
        let pubkey = solana_sdk::pubkey::Pubkey::from_str(wallet).context("invalid wallet")?;
        let tx_builder = self.tx_builder_for_network(pubkey, network.as_deref());

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

        // The on-chain reveal_guess program requires that ONLY the first
        // revealer pass r_matchup; the second revealer must pass None or
        // the program rejects with `RMatchupMismatch` (error 6032).
        // Without this branch the human-vs-mcp-agent e2e race
        // deterministically fails when the browser-human reveals first
        // and the MCP agent's reveal then trips RMatchupMismatch —
        // leaving the game stuck in Revealing forever. Mirrors the same
        // guard in the browser's
        // `frontend/coordination-game/src/hooks/reveal-tx.ts:submitRevealTx`.
        let r_matchup_arg = pick_reveal_matchup_arg(game.matchup_type, r_matchup);
        if r_matchup_arg.is_none() {
            tracing::info!(
                wallet = %wallet,
                game_id,
                matchup_type = game.matchup_type,
                "matchup already revealed by opponent, omitting r_matchup"
            );
        }
        let unsigned = tx_builder
            .build_reveal_guess(
                game_id,
                tournament_id,
                preimage,
                r_matchup_arg,
                game.player_one,
                game.player_two,
            )
            .await?;

        tracing::info!(wallet = %wallet, game_id, "built unsigned reveal_guess tx");
        Ok(Some(unsigned))
    }

    /// Drain the inputs needed to build the reveal_guess transaction from
    /// the in-memory session. Returns `Ok(None)` when the opponent's
    /// reveal_data hasn't arrived yet (the agent should poll again).
    /// Errors only when the session is missing data the prior tool calls
    /// were responsible for populating (game_id, preimage, etc).
    async fn load_reveal_inputs(
        &self,
        session: &Arc<Mutex<GameSession>>,
    ) -> Result<Option<RevealInputs>> {
        let s = session.lock().await;
        let game_id = s.game_id.context("no active game")?;
        let tournament_id = s.tournament_id.context("tournament_id not set")?;
        let preimage = s
            .commit_preimage
            .context("no commit preimage — call game_commit_guess first")?;
        let reveal_hex = match &s.reveal_data {
            None => return Ok(None),
            Some(hex) => hex.clone(),
        };
        drop(s);
        // Postcondition: when we return Ok(Some), every input is present.
        let r_matchup = parse_r_matchup_hex(&reveal_hex)?;
        debug_assert_eq!(r_matchup.len(), 32, "r_matchup must be 32 bytes");
        Ok(Some(RevealInputs {
            game_id,
            tournament_id,
            preimage,
            r_matchup,
        }))
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

        let (game_id, network) = {
            let s = session.lock().await;
            (s.game_id.context("no active game")?, s.network.clone())
        };

        let pubkey = solana_sdk::pubkey::Pubkey::from_str(wallet).context("invalid wallet")?;
        let tx_builder = self.tx_builder_for_network(pubkey, network.as_deref());
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

        // Use the session's pinned network (set in `build_find_match_tx`)
        // so the blockhash + game_counter read come from the same RPC the
        // agent's deposit_stake landed on. The cached tx_builder is
        // cluster-default — using it here would put a mainnet blockhash
        // into a tx that the agent then submits to devnet → "Blockhash
        // not found". Surfaced 2026-05-09 by the human-vs-agent E2E.
        let network = self
            .sessions
            .read()
            .await
            .get(wallet)
            .ok_or_else(|| anyhow::anyhow!("no session for wallet"))?
            .lock()
            .await
            .network
            .clone();
        let pubkey = solana_sdk::pubkey::Pubkey::from_str(wallet).context("invalid wallet")?;
        let tx_builder = self.tx_builder_for_network(pubkey, network.as_deref());

        // Log balance before create_game for debugging.
        let balance = tx_builder
            .rpc()
            .get_balance(&tx_builder.pubkey())
            .await
            .unwrap_or(0);
        tracing::info!(wallet = %wallet, balance_lamports = balance, network = ?network, "creating game as P1");

        let matchmaker = read_matchmaker_pubkey(&tx_builder).await?;

        let stake: u64 = game_chain::FIXED_STAKE_LAMPORTS;

        // Build unsigned create_game transaction.
        let unsigned = tx_builder
            .build_create_game(tournament_id, stake, matchup_commitment, &matchmaker)
            .await?;

        // Get matchmaker cosignature from game-api. Same network as the
        // tx so the cosign endpoint reads the right tournament PDA.
        use base64::Engine;
        let msg_b64 = base64::engine::general_purpose::STANDARD.encode(&unsigned.message);
        let api_client = GameApiClient::new(&self.game_api_url)?.with_network(network.clone());
        let cosign_resp = api_client
            .request_cosign(jwt, &msg_b64)
            .await
            .map_err(|e| anyhow::anyhow!("cosign request failed: {e}"))?;

        let game_id = read_next_game_id(&tx_builder).await?;

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
        let (jwt, network) = {
            let s = session.lock().await;
            (s.jwt.clone(), s.network.clone())
        };
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

            match WsConnection::connect_with_network(&game_api_url, &jwt, network.as_deref()).await
            {
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

    /// Regression: 2026-05-09 devnet gameplay E2E sweep. The MCP server's
    /// game tools used to broadcast every signed tx through the cached
    /// per-wallet GameTxBuilder, which was constructed from the cluster
    /// default RPC URL only. The human-vs-agent test ran against devnet
    /// (T1001), but the agent's deposit_stake landed on mainnet RPC where
    /// T1001 doesn't exist → AccountNotInitialized. The router now picks
    /// per-call so `Some("devnet")` ≠ `Some("mainnet")` ≠ default.
    #[test]
    fn pick_rpc_url_for_network_routes_per_arg() {
        let default = "https://default.example";
        let mainnet = "https://mainnet.example";
        let devnet = "https://devnet.example";
        assert_eq!(
            pick_rpc_url_for_network(Some("devnet"), default, mainnet, devnet),
            devnet
        );
        assert_eq!(
            pick_rpc_url_for_network(Some("mainnet"), default, mainnet, devnet),
            mainnet
        );
        assert_eq!(
            pick_rpc_url_for_network(None, default, mainnet, devnet),
            default
        );
        // Unknown network falls through to default — never silently picks
        // mainnet or devnet from an arbitrary string.
        assert_eq!(
            pick_rpc_url_for_network(Some("stagenet"), default, mainnet, devnet),
            default
        );
    }

    /// Regression: 2026-05-09 devnet human-vs-mcp-agent E2E. When the
    /// browser-human reveals first, the on-chain matchup_type flips
    /// from MATCHUP_TYPE_UNSET (255) to 0 or 1. If the MCP agent's
    /// reveal then unconditionally passes r_matchup, the program
    /// rejects with `RMatchupMismatch` (error 6032) and the game stays
    /// in Revealing forever — browser stuck waiting for outcome,
    /// 120s assertion times out. The browser's `submitRevealTx` has
    /// the same race-aware logic; the MCP server must match.
    #[test]
    fn pick_reveal_matchup_arg_passes_when_unset() {
        let r_matchup = [42u8; 32];
        assert_eq!(
            pick_reveal_matchup_arg(MATCHUP_TYPE_UNSET, r_matchup),
            Some(r_matchup),
        );
    }

    #[test]
    fn pick_reveal_matchup_arg_omits_when_already_revealed_same_team() {
        let r_matchup = [42u8; 32];
        // matchup_type=0 means same team — opponent already revealed.
        assert_eq!(pick_reveal_matchup_arg(0, r_matchup), None);
    }

    #[test]
    fn pick_reveal_matchup_arg_omits_when_already_revealed_diff_team() {
        let r_matchup = [42u8; 32];
        // matchup_type=1 means different teams — opponent already revealed.
        assert_eq!(pick_reveal_matchup_arg(1, r_matchup), None);
    }

    #[test]
    fn pick_reveal_matchup_arg_unset_sentinel_is_255() {
        // Pin the on-chain sentinel so a future refactor can't silently
        // drift from `programs/coordination-game/src/state/game.rs`.
        assert_eq!(MATCHUP_TYPE_UNSET, 255);
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

    // --- Cross-chain submit wiring (5d-B) ---

    #[test]
    fn xchain_solana_actions_are_recognized() {
        for a in [
            "create_xmatch",
            "lock_xtranche",
            "refund_xmatch_nocert",
            "refund_xmatch_timeout",
            "settle_xmatch",
        ] {
            assert!(is_xchain_solana_action(a), "{a} must be recognized");
        }
        // Same-chain actions and the EVM-leg (submitted off-server) are NOT.
        for a in [
            "create_game",
            "join_game",
            "commit_guess",
            "reveal_guess",
            "create_match",
        ] {
            assert!(!is_xchain_solana_action(a), "{a} must not be xchain");
        }
    }

    #[test]
    fn xchain_submit_result_routes_next_step_by_action() {
        let fund = xchain_submit_result("create_xmatch", "sig1");
        assert_eq!(fund["tx_signature"], "sig1");
        assert_eq!(fund["status"], "submitted");
        // After funding, the player locks their own leg (permissionless) — the
        // operator no longer locks.
        assert!(fund["next_step"]
            .as_str()
            .expect("next_step")
            .contains("xchain_build_lock_xmatch"));

        let lock = xchain_submit_result("lock_xtranche", "siglock");
        assert!(lock["next_step"]
            .as_str()
            .expect("next_step")
            .contains("locked"));

        let settle = xchain_submit_result("settle_xmatch", "sig2");
        assert!(settle["next_step"]
            .as_str()
            .expect("next_step")
            .contains("PlayerProfile"));

        // Both refund kinds fall through to the refund guidance.
        for kind in ["refund_xmatch_nocert", "refund_xmatch_timeout"] {
            let r = xchain_submit_result(kind, "sig3");
            assert!(r["next_step"]
                .as_str()
                .expect("next_step")
                .contains("Refund submitted"));
        }
    }
}
