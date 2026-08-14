#![deny(warnings)]
#![deny(clippy::all)]

pub mod ws;

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum GameApiError {
    #[error("HTTP request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("game-api returned {status}: {body}")]
    Status {
        status: reqwest::StatusCode,
        body: String,
    },

    #[error("failed to build HTTP client: {0}")]
    ClientBuild(reqwest::Error),
}

// ---------------------------------------------------------------------------
// Response / request types
// ---------------------------------------------------------------------------

/// Response from `POST /auth/challenge`.
#[derive(Debug, Deserialize)]
pub struct ChallengeResponse {
    pub nonce: String,
}

/// Response from `POST /auth/verify`.
#[derive(Debug, Deserialize)]
pub struct AuthTokenResponse {
    pub token: String,
}

/// Request body for `POST /queue/join`.
#[derive(Debug, Serialize)]
pub struct QueueJoinRequest<'a> {
    pub tournament_id: u64,
    pub is_ai: bool,
    pub agent_version: &'a str,
    /// True for the built-in grok-agent. External agents set false.
    /// game-api defaults this to false server-side when absent.
    pub is_internal: bool,
}

/// Response from `POST /queue/join` (MCP server variant returns match info).
#[derive(Debug, Deserialize)]
pub struct QueueJoinResponse {
    pub matched: bool,
    pub session_id: Option<String>,
    pub cohort_name: Option<String>,
}

/// A single leaderboard entry.
#[derive(Debug, Serialize, Deserialize)]
pub struct LeaderboardEntry {
    pub wallet: String,
    pub wins: u64,
    pub total_games: u64,
    pub score: u64,
}

/// Response from `GET /tournaments/{id}/leaderboard`.
#[derive(Debug, Serialize, Deserialize)]
pub struct LeaderboardResponse {
    pub entries: Vec<LeaderboardEntry>,
    pub tournament_id: u64,
}

/// Response from `POST /games/cosign`.
#[derive(Debug, Deserialize)]
pub struct CosignResponse {
    /// Base64-encoded matchmaker ed25519 signature.
    pub signature: String,
}

/// Request body for `POST /internal/evmgame/join` — the same-chain EVM queue.
/// No session key (same-chain gameplay is on-chain with the player's own wallet)
/// and no contract: game-api resolves the CoordinationGame address from the
/// chain registry (single source of truth).
#[derive(Debug, Serialize)]
pub struct EvmGameJoinRequest<'a> {
    /// The joining player's `0x` EVM wallet.
    pub wallet: &'a str,
    /// CAIP-2 chain id, e.g. `eip155:84532`.
    pub chain: &'a str,
    pub tournament_id: u64,
    /// Whether the joiner is an AI. game-api derives the matchup from
    /// `is_ai(joiner) XOR is_ai(waiter)` (same as Solana's `QueueJoinRequest`),
    /// so an MCP agent (AI) vs a browser (human) pairs heterogeneous. Defaults
    /// false server-side (`#[serde(default)]`), preserving the browser path.
    pub is_ai: bool,
    /// True only for the internal house grok-agent (gates the AI fallback).
    /// External MCP agents leave this false.
    pub is_internal: bool,
}

/// Request body for `POST /internal/xqueue/join` — the cross-chain queue.
#[derive(Debug, Serialize)]
pub struct XQueueJoinRequest<'a> {
    /// Chain-native address: base58 pubkey (Solana) or `0x` address (EVM).
    pub wallet: &'a str,
    /// CAIP-2 chain id the player stakes on, e.g. `eip155:84532`.
    pub chain: &'a str,
    /// `0x` eth address of the player's per-match secp256k1 session key.
    pub session_key: &'a str,
    pub tournament_id: u64,
}

/// Response from the cross-chain join/status endpoints: `status` is `waiting`
/// or `matched`; on `matched`, `match_payload` carries the co-signed relay
/// payload both players need to fund and settle.
#[derive(Debug, Deserialize)]
pub struct XQueueResponse {
    pub status: String,
    #[serde(default, rename = "match")]
    pub match_payload: Option<serde_json::Value>,
}

/// Response from `/internal/xqueue/build-sol-fund`: the matchmaker-cosigned
/// Solana `create_xmatch` funding tx for the player to sign and submit.
#[derive(Debug, Deserialize)]
pub struct XSolFundResponse {
    /// Base64 unsigned transaction (matchmaker slot to be filled from `matchmaker_signature`).
    pub unsigned_tx: String,
    pub blockhash: String,
    /// Base64 matchmaker ed25519 signature over the tx message.
    pub matchmaker_signature: String,
    pub match_id: String,
    pub action: String,
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

/// HTTP client for the coordination game backend API.
///
/// Covers all endpoints used by grok-agent and mcp-server.
/// Auth endpoints (`request_challenge`, `verify_challenge`) do not require a
/// bearer token; all other endpoints take `token: &str`.
///
/// `network` is appended as `?network=devnet` to every request URL. The
/// game-api routes Solana RPC reads (tx lookup for auth, account reads for
/// queue/cosign) by this param. Without it every call defaults to mainnet,
/// which breaks devnet flows: a deposit_stake tx broadcast on devnet
/// returns `transaction not found` from a mainnet tx-signature lookup.
/// Surfaced 2026-05-09 by the human-vs-agent devnet E2E.
pub struct GameApiClient {
    inner: reqwest::Client,
    base_url: String,
    network: Option<String>,
}

impl GameApiClient {
    /// Create a new client pointing at `base_url` (e.g. `http://localhost:8080`).
    ///
    /// Trailing slashes are stripped from the base URL. The client defaults
    /// to mainnet routing — call `with_network` to attach a non-default
    /// `?network=` query param to every request.
    pub fn new(base_url: &str) -> Result<Self, GameApiError> {
        assert!(!base_url.is_empty(), "game-api base_url must not be empty");

        let inner = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(GameApiError::ClientBuild)?;

        Ok(Self {
            inner,
            base_url: base_url.trim_end_matches('/').to_string(),
            network: None,
        })
    }

    /// Attach a `?network=` query param to every outgoing request. `None`
    /// (the default) sends without the param so production traffic stays
    /// exactly as before.
    pub fn with_network(mut self, network: Option<String>) -> Self {
        self.network = network;
        self
    }

    /// Build the per-request URL with the network query param appended.
    fn url(&self, path: &str) -> String {
        debug_assert!(path.starts_with('/'), "path must start with /");
        match self.network.as_deref() {
            Some(n) if !n.is_empty() => format!("{}{}?network={}", self.base_url, path, n),
            _ => format!("{}{}", self.base_url, path),
        }
    }

    // -- Auth (no token required) ------------------------------------------

    /// `POST /auth/challenge` — request a nonce for wallet authentication.
    pub async fn request_challenge(&self, wallet: &str) -> Result<ChallengeResponse, GameApiError> {
        assert!(!wallet.is_empty(), "wallet must not be empty");

        #[derive(Serialize)]
        struct Body<'a> {
            wallet: &'a str,
        }

        let url = self.url("/auth/challenge");
        let resp = self.inner.post(&url).json(&Body { wallet }).send().await?;

        Self::check_status(resp)
            .await?
            .json()
            .await
            .map_err(Into::into)
    }

    /// `POST /auth/verify` — verify a signed nonce and receive a JWT.
    pub async fn verify_challenge(
        &self,
        wallet: &str,
        nonce: &str,
        signature: &str,
    ) -> Result<AuthTokenResponse, GameApiError> {
        #[derive(Serialize)]
        struct Body<'a> {
            wallet: &'a str,
            nonce: &'a str,
            signature: &'a str,
        }

        let url = self.url("/auth/verify");
        let resp = self
            .inner
            .post(&url)
            .json(&Body {
                wallet,
                nonce,
                signature,
            })
            .send()
            .await?;

        Self::check_status(resp)
            .await?
            .json()
            .await
            .map_err(Into::into)
    }

    /// `POST /auth/session` — authenticate via a Solana transaction signature.
    ///
    /// The game-api fetches the transaction from Solana RPC and verifies the
    /// wallet signed it. Returns a JWT. This is the non-custodial auth path:
    /// the agent signs a transaction locally (e.g., deposit_stake), and the
    /// tx signature doubles as proof of wallet ownership.
    ///
    /// `nonce` MUST be a nonce just issued by `challenge()`, and the
    /// transaction MUST carry it as an SPL-Memo. game-api rejects the pair
    /// otherwise: without that binding the endpoint accepted any transaction
    /// where the claimed wallet was fee payer, and Solana signatures are
    /// public, so a victim's transaction read off Solscan minted a JWT as them.
    pub async fn session_auth(
        &self,
        wallet: &str,
        tx_signature: &str,
        nonce: &str,
    ) -> Result<AuthTokenResponse, GameApiError> {
        #[derive(Serialize)]
        struct Body<'a> {
            wallet: &'a str,
            tx_signature: &'a str,
            nonce: &'a str,
        }

        let url = self.url("/auth/session");
        let resp = self
            .inner
            .post(&url)
            .json(&Body {
                wallet,
                tx_signature,
                nonce,
            })
            .send()
            .await?;

        Self::check_status(resp)
            .await?
            .json()
            .await
            .map_err(Into::into)
    }

    // -- Queue (token required) --------------------------------------------

    /// `POST /queue/join` — join the matchmaking queue.
    pub async fn join_queue(
        &self,
        token: &str,
        request: &QueueJoinRequest<'_>,
    ) -> Result<QueueJoinResponse, GameApiError> {
        let url = self.url("/queue/join");
        let resp = self
            .inner
            .post(&url)
            .bearer_auth(token)
            .json(request)
            .send()
            .await?;

        Self::check_status(resp)
            .await?
            .json()
            .await
            .map_err(Into::into)
    }

    /// `POST /queue/leave` — leave the matchmaking queue.
    ///
    /// Silently succeeds if not currently in the queue.
    pub async fn leave_queue(&self, token: &str, tournament_id: u64) -> Result<(), GameApiError> {
        #[derive(Serialize)]
        struct Body {
            tournament_id: u64,
        }

        let url = self.url("/queue/leave");
        let resp = self
            .inner
            .post(&url)
            .bearer_auth(token)
            .json(&Body { tournament_id })
            .send()
            .await?;

        Self::check_status(resp).await?;
        Ok(())
    }

    // -- Cross-chain queue (internal; caller is trusted) -------------------

    /// `POST /internal/xqueue/join` — join the cross-chain queue. Internal
    /// endpoint (no bearer token): the caller (MCP server / BFF) has already
    /// authenticated the agent. Returns `waiting`, or `matched` with the
    /// co-signed relay payload.
    pub async fn xqueue_join(
        &self,
        request: &XQueueJoinRequest<'_>,
    ) -> Result<XQueueResponse, GameApiError> {
        // Internal endpoints take no `?network=` param — address it directly.
        let url = format!("{}/internal/xqueue/join", self.base_url);
        let resp = self.inner.post(&url).json(request).send().await?;
        Self::check_status(resp)
            .await?
            .json()
            .await
            .map_err(Into::into)
    }

    /// `GET /internal/xqueue/status?wallet=…` — poll for a cross-chain match
    /// by chain-native wallet (the player who was already waiting).
    pub async fn xqueue_status(&self, wallet: &str) -> Result<XQueueResponse, GameApiError> {
        let url = format!("{}/internal/xqueue/status", self.base_url);
        let resp = self
            .inner
            .get(&url)
            .query(&[("wallet", wallet)])
            .send()
            .await?;
        Self::check_status(resp)
            .await?
            .json()
            .await
            .map_err(Into::into)
    }

    // -- Same-chain EVM queue (internal; caller is trusted) ----------------

    /// `POST /internal/evmgame/join` — join the same-chain EVM (EVM-vs-EVM)
    /// queue. Returns `waiting`, or `matched` with the two unsigned on-chain
    /// calls (`createGame` for the waiting player, `joinGame` for the joiner).
    pub async fn evmgame_join(
        &self,
        request: &EvmGameJoinRequest<'_>,
    ) -> Result<XQueueResponse, GameApiError> {
        let url = format!("{}/internal/evmgame/join", self.base_url);
        let resp = self.inner.post(&url).json(request).send().await?;
        Self::check_status(resp)
            .await?
            .json()
            .await
            .map_err(Into::into)
    }

    /// `GET /internal/evmgame/status?wallet=…` — poll for a same-chain EVM match
    /// by wallet (the player who was already waiting retrieves their calls).
    pub async fn evmgame_status(&self, wallet: &str) -> Result<XQueueResponse, GameApiError> {
        let url = format!("{}/internal/evmgame/status", self.base_url);
        let resp = self
            .inner
            .get(&url)
            .query(&[("wallet", wallet)])
            .send()
            .await?;
        Self::check_status(resp)
            .await?
            .json()
            .await
            .map_err(Into::into)
    }

    /// `POST /internal/evmgame/committed` — notify that a player has committed
    /// on-chain. Returns `{both_committed, r_matchup}`; `r_matchup` (the matchup
    /// preimage) is non-null only once BOTH players have committed.
    pub async fn evmgame_committed(
        &self,
        game_id: &str,
        wallet: &str,
    ) -> Result<serde_json::Value, GameApiError> {
        #[derive(Serialize)]
        struct Body<'a> {
            game_id: &'a str,
            wallet: &'a str,
        }
        let url = format!("{}/internal/evmgame/committed", self.base_url);
        let resp = self
            .inner
            .post(&url)
            .json(&Body { game_id, wallet })
            .send()
            .await?;
        Self::check_status(resp)
            .await?
            .json()
            .await
            .map_err(Into::into)
    }

    /// `POST /internal/xqueue/build-sol-fund` — matchmaker-cosigned Solana
    /// `create_xmatch` funding tx for the Solana-leg player to sign + submit.
    pub async fn xqueue_build_sol_fund(
        &self,
        wallet: &str,
    ) -> Result<XSolFundResponse, GameApiError> {
        #[derive(Serialize)]
        struct Body<'a> {
            wallet: &'a str,
        }
        let url = format!("{}/internal/xqueue/build-sol-fund", self.base_url);
        let resp = self.inner.post(&url).json(&Body { wallet }).send().await?;
        Self::check_status(resp)
            .await?
            .json()
            .await
            .map_err(Into::into)
    }

    /// `POST /internal/xqueue/build-sol-lock` — unsigned permissionless
    /// `lock_xtranche` tx for the Solana-leg player to sign + submit. Authorized
    /// by the operator's stored match-live signature (no matchmaker cosign).
    /// Returns the raw JSON (`unsigned_tx`, `blockhash`, `match_id`, `action`).
    pub async fn xqueue_build_sol_lock(
        &self,
        wallet: &str,
    ) -> Result<serde_json::Value, GameApiError> {
        #[derive(Serialize)]
        struct Body<'a> {
            wallet: &'a str,
        }
        let url = format!("{}/internal/xqueue/build-sol-lock", self.base_url);
        let resp = self.inner.post(&url).json(&Body { wallet }).send().await?;
        Self::check_status(resp)
            .await?
            .json()
            .await
            .map_err(Into::into)
    }

    /// `POST /internal/xqueue/build-sol-refund` — unsigned Solana refund tx
    /// (permissionless) for the player to sign + submit. Returns the raw JSON
    /// (`unsigned_tx`, `blockhash`, `match_id`, `action`).
    pub async fn xqueue_build_sol_refund(
        &self,
        wallet: &str,
        match_id: &str,
        kind: &str,
    ) -> Result<serde_json::Value, GameApiError> {
        #[derive(Serialize)]
        struct Body<'a> {
            wallet: &'a str,
            match_id: &'a str,
            kind: &'a str,
        }
        let url = format!("{}/internal/xqueue/build-sol-refund", self.base_url);
        let resp = self
            .inner
            .post(&url)
            .json(&Body {
                wallet,
                match_id,
                kind,
            })
            .send()
            .await?;
        Self::check_status(resp)
            .await?
            .json()
            .await
            .map_err(Into::into)
    }

    /// `POST /internal/xqueue/outcome-cosign` — operator-cosign the cross-chain
    /// outcome derived from the stored co-signed checkpoint. Returns the raw
    /// JSON payload (canonical outcome fields, `outcome_digest`,
    /// `operator_outcome_signature`, `operator_match_live_signature`) the player
    /// needs to add their own session-key signature and assemble settle.
    pub async fn xqueue_outcome_cosign(
        &self,
        wallet: &str,
    ) -> Result<serde_json::Value, GameApiError> {
        #[derive(Serialize)]
        struct Body<'a> {
            wallet: &'a str,
        }
        let url = format!("{}/internal/xqueue/outcome-cosign", self.base_url);
        let resp = self.inner.post(&url).json(&Body { wallet }).send().await?;
        Self::check_status(resp)
            .await?
            .json()
            .await
            .map_err(Into::into)
    }

    /// `POST /internal/xqueue/commit` — record the player's guess commit
    /// (`0x` SHA-256 of their guess preimage).
    pub async fn xqueue_commit(
        &self,
        wallet: &str,
        commit: &str,
    ) -> Result<serde_json::Value, GameApiError> {
        #[derive(Serialize)]
        struct Body<'a> {
            wallet: &'a str,
            commit: &'a str,
        }
        let url = format!("{}/internal/xqueue/commit", self.base_url);
        let resp = self
            .inner
            .post(&url)
            .json(&Body { wallet, commit })
            .send()
            .await?;
        Self::check_status(resp)
            .await?
            .json()
            .await
            .map_err(Into::into)
    }

    /// `POST /internal/xqueue/sign` — submit the player's session-key signature
    /// over the canonical checkpoint for `step` (2 = both committed, 4 =
    /// terminal). Returns `{ relayed, r_matchup? }`.
    pub async fn xqueue_sign(
        &self,
        wallet: &str,
        step: u8,
        signature: &str,
    ) -> Result<serde_json::Value, GameApiError> {
        #[derive(Serialize)]
        struct Body<'a> {
            wallet: &'a str,
            step: u8,
            signature: &'a str,
        }
        let url = format!("{}/internal/xqueue/sign", self.base_url);
        let resp = self
            .inner
            .post(&url)
            .json(&Body {
                wallet,
                step,
                signature,
            })
            .send()
            .await?;
        Self::check_status(resp)
            .await?
            .json()
            .await
            .map_err(Into::into)
    }

    /// `POST /internal/xqueue/reveal` — reveal the player's guess preimage.
    pub async fn xqueue_reveal(
        &self,
        wallet: &str,
        preimage: &str,
    ) -> Result<serde_json::Value, GameApiError> {
        #[derive(Serialize)]
        struct Body<'a> {
            wallet: &'a str,
            preimage: &'a str,
        }
        let url = format!("{}/internal/xqueue/reveal", self.base_url);
        let resp = self
            .inner
            .post(&url)
            .json(&Body { wallet, preimage })
            .send()
            .await?;
        Self::check_status(resp)
            .await?
            .json()
            .await
            .map_err(Into::into)
    }

    /// `GET /internal/xqueue/gameplay` — the player's "what to sign next" view:
    /// canonical step-2 / terminal checkpoints and the released `r_matchup`.
    pub async fn xqueue_gameplay(&self, wallet: &str) -> Result<serde_json::Value, GameApiError> {
        let url = format!("{}/internal/xqueue/gameplay", self.base_url);
        let resp = self
            .inner
            .get(&url)
            .query(&[("wallet", wallet)])
            .send()
            .await?;
        Self::check_status(resp)
            .await?
            .json()
            .await
            .map_err(Into::into)
    }

    // -- Games (token required) --------------------------------------------

    /// `POST /games/committed` — notify the backend that this player committed their guess.
    ///
    /// When both players have committed, the backend auto-delivers `reveal_data`
    /// via WebSocket to both players.
    pub async fn post_games_committed(
        &self,
        token: &str,
        session_id: &str,
    ) -> Result<(), GameApiError> {
        #[derive(Serialize)]
        struct Body<'a> {
            session_id: &'a str,
        }

        let url = self.url("/games/committed");
        let resp = self
            .inner
            .post(&url)
            .bearer_auth(token)
            .json(&Body { session_id })
            .send()
            .await?;

        Self::check_status(resp).await?;
        Ok(())
    }

    /// `POST /games/joined` — notify the backend that a player joined a game.
    pub async fn post_games_joined(
        &self,
        token: &str,
        game_id: u64,
        session_id: &str,
    ) -> Result<(), GameApiError> {
        #[derive(Serialize)]
        struct Body<'a> {
            game_id: u64,
            session_id: &'a str,
        }

        let url = self.url("/games/joined");
        let resp = self
            .inner
            .post(&url)
            .bearer_auth(token)
            .json(&Body {
                game_id,
                session_id,
            })
            .send()
            .await?;

        Self::check_status(resp).await?;
        Ok(())
    }

    /// `POST /games/started` — notify the backend that P1 created the game on-chain.
    ///
    /// Links the session to the on-chain game_id and triggers `game_ready` WS
    /// notification to Player 2.
    pub async fn post_games_started(
        &self,
        token: &str,
        game_id: u64,
        session_id: &str,
    ) -> Result<(), GameApiError> {
        #[derive(Serialize)]
        struct Body<'a> {
            game_id: u64,
            session_id: &'a str,
        }

        let url = self.url("/games/started");
        let resp = self
            .inner
            .post(&url)
            .bearer_auth(token)
            .json(&Body {
                game_id,
                session_id,
            })
            .send()
            .await?;

        Self::check_status(resp).await?;
        Ok(())
    }

    /// `POST /games/cosign` — get matchmaker co-signature for create_game tx.
    ///
    /// Sends base64-encoded transaction message bytes. Returns the matchmaker's
    /// ed25519 signature (base64-encoded) to add to the transaction.
    pub async fn request_cosign(
        &self,
        token: &str,
        message_b64: &str,
    ) -> Result<CosignResponse, GameApiError> {
        #[derive(Serialize)]
        struct Body<'a> {
            message: &'a str,
        }

        let url = self.url("/games/cosign");
        let resp = self
            .inner
            .post(&url)
            .bearer_auth(token)
            .json(&Body {
                message: message_b64,
            })
            .send()
            .await?;

        let resp = Self::check_status(resp).await?;
        let body: CosignResponse = resp.json().await?;
        Ok(body)
    }

    /// `POST /games/resolved` — notify the backend that a game was resolved.
    ///
    /// Optional `agent_guess_reasoning` and `agent_guess_source` fields allow
    /// AI agents to report their decision audit trail for A/B analysis.
    #[allow(clippy::too_many_arguments)]
    pub async fn post_games_resolved(
        &self,
        token: &str,
        game_id: u64,
        p1_guess: u8,
        p2_guess: u8,
        p1_return: u64,
        p2_return: u64,
        matchup_type: u8,
        first_committer: u8,
        agent_guess_reasoning: Option<&str>,
        agent_guess_source: Option<&str>,
    ) -> Result<(), GameApiError> {
        #[derive(Serialize)]
        struct Body<'a> {
            game_id: u64,
            p1_guess: u8,
            p2_guess: u8,
            p1_return: u64,
            p2_return: u64,
            matchup_type: u8,
            first_committer: u8,
            #[serde(skip_serializing_if = "Option::is_none")]
            agent_guess_reasoning: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            agent_guess_source: Option<&'a str>,
        }

        let url = self.url("/games/resolved");
        let resp = self
            .inner
            .post(&url)
            .bearer_auth(token)
            .json(&Body {
                game_id,
                p1_guess,
                p2_guess,
                p1_return,
                p2_return,
                matchup_type,
                first_committer,
                agent_guess_reasoning,
                agent_guess_source,
            })
            .send()
            .await?;

        Self::check_status(resp).await?;
        Ok(())
    }

    // -- Leaderboard (no token required) -----------------------------------

    /// `GET /tournaments/{id}/leaderboard` — fetch tournament leaderboard.
    ///
    /// `limit` defaults to 20, clamped to a maximum of 100.
    pub async fn get_leaderboard(
        &self,
        tournament_id: u64,
        limit: Option<u32>,
    ) -> Result<LeaderboardResponse, GameApiError> {
        let effective_limit = limit.unwrap_or(20).min(100);
        let path = format!("/tournaments/{tournament_id}/leaderboard");
        // Manual URL build because this endpoint already has a `?limit=`
        // query param — `self.url(path)` would conflict with `?` placement.
        let mut url = format!("{}{}?limit={}", self.base_url, path, effective_limit);
        if let Some(n) = self.network.as_deref().filter(|n| !n.is_empty()) {
            url.push_str(&format!("&network={n}"));
        }

        let resp = self.inner.get(&url).send().await?;
        Self::check_status(resp)
            .await?
            .json()
            .await
            .map_err(Into::into)
    }

    // -- Helpers -----------------------------------------------------------

    /// Return the base URL (useful for testing URL construction).
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Check the HTTP status and convert non-success responses into
    /// `GameApiError::Status` with the response body for diagnostics.
    async fn check_status(response: reqwest::Response) -> Result<reqwest::Response, GameApiError> {
        if response.status().is_success() {
            return Ok(response);
        }
        let status = response.status();
        // A body-read failure used to collapse to "", so the caller saw a bare
        // status with no explanation and could not tell an empty error body from
        // an unreadable one.
        let body = match response.text().await {
            Ok(b) => b,
            Err(e) => format!("<error body unreadable: {e}>"),
        };
        Err(GameApiError::Status { status, body })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_strips_trailing_slash() {
        let client = GameApiClient::new("http://localhost:8080/").unwrap();
        assert_eq!(client.base_url(), "http://localhost:8080");
    }

    #[test]
    fn new_strips_multiple_trailing_slashes() {
        let client = GameApiClient::new("http://localhost:8080///").unwrap();
        assert_eq!(client.base_url(), "http://localhost:8080");
    }

    #[test]
    fn new_preserves_url_without_trailing_slash() {
        let client = GameApiClient::new("https://api.example.com").unwrap();
        assert_eq!(client.base_url(), "https://api.example.com");
    }

    #[test]
    fn evmgame_join_request_omits_session_key_and_contract() {
        // Same-chain EVM join carries neither a session key (gameplay is on-chain
        // with the player's own wallet) nor a contract (game-api resolves it from
        // the registry) — both must be absent from the wire body.
        let body = serde_json::to_value(EvmGameJoinRequest {
            wallet: "0xPlayer",
            chain: "eip155:84532",
            tournament_id: 2,
            is_ai: true,
            is_internal: false,
        })
        .expect("serialize");
        assert_eq!(body["wallet"], "0xPlayer");
        assert_eq!(body["chain"], "eip155:84532");
        assert_eq!(body["tournament_id"], 2);
        assert_eq!(body["is_ai"], true);
        assert_eq!(body["is_internal"], false);
        assert!(body.get("session_key").is_none());
        assert!(body.get("contract").is_none());
    }

    #[test]
    fn xqueue_response_deserializes_waiting_and_matched() {
        // Waiting: no `match` field.
        let waiting: XQueueResponse =
            serde_json::from_str(r#"{"status":"waiting"}"#).expect("waiting");
        assert_eq!(waiting.status, "waiting");
        assert!(waiting.match_payload.is_none());

        // Matched: the `match` JSON key maps to match_payload.
        let matched: XQueueResponse =
            serde_json::from_str(r#"{"status":"matched","match":{"match_id":"0xabc"}}"#)
                .expect("matched");
        assert_eq!(matched.status, "matched");
        assert_eq!(
            matched.match_payload.unwrap()["match_id"],
            serde_json::json!("0xabc")
        );
    }

    #[test]
    fn new_preserves_url_with_path() {
        let client = GameApiClient::new("https://api.example.com/v1").unwrap();
        assert_eq!(client.base_url(), "https://api.example.com/v1");
    }

    #[test]
    #[should_panic(expected = "game-api base_url must not be empty")]
    fn new_rejects_empty_url() {
        let _ = GameApiClient::new("");
    }

    #[test]
    fn queue_join_request_serialization() {
        let req = QueueJoinRequest {
            tournament_id: 1,
            is_ai: true,
            agent_version: "claude-4/prompt-v1",
            is_internal: false,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["tournament_id"], 1);
        assert_eq!(json["is_ai"], true);
        assert_eq!(json["agent_version"], "claude-4/prompt-v1");
        assert_eq!(json["is_internal"], false);
    }

    #[test]
    fn leaderboard_entry_roundtrip() {
        let entry = LeaderboardEntry {
            wallet: "ABC123".to_string(),
            wins: 10,
            total_games: 20,
            score: 500,
        };
        let json = serde_json::to_string(&entry).unwrap();
        let parsed: LeaderboardEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.wallet, "ABC123");
        assert_eq!(parsed.wins, 10);
        assert_eq!(parsed.total_games, 20);
        assert_eq!(parsed.score, 500);
    }

    #[test]
    fn leaderboard_response_roundtrip() {
        let resp = LeaderboardResponse {
            entries: vec![LeaderboardEntry {
                wallet: "ABC123".to_string(),
                wins: 10,
                total_games: 20,
                score: 500,
            }],
            tournament_id: 1,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: LeaderboardResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.entries.len(), 1);
        assert_eq!(parsed.tournament_id, 1);
    }

    #[test]
    fn error_display_request() {
        let err = GameApiError::Status {
            status: reqwest::StatusCode::NOT_FOUND,
            body: "not found".to_string(),
        };
        let msg = err.to_string();
        assert!(
            msg.contains("404"),
            "expected status code in message: {msg}"
        );
        assert!(msg.contains("not found"), "expected body in message: {msg}");
    }
}
