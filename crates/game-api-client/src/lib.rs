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
    #[serde(default)]
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

/// Response from `GET /games/status/{game_id}`.
#[derive(Debug, Deserialize)]
pub struct GameStatusResponse {
    pub game_id: u64,
    pub state: String,
    pub player_one: String,
    pub player_two: Option<String>,
    pub created_at: String,
    pub resolved_at: Option<String>,
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

/// HTTP client for the coordination game backend API.
///
/// Covers all endpoints used by grok-agent and coordination-mcp-server.
/// Auth endpoints (`request_challenge`, `verify_challenge`) do not require a
/// bearer token; all other endpoints take `token: &str`.
pub struct GameApiClient {
    inner: reqwest::Client,
    base_url: String,
}

impl GameApiClient {
    /// Create a new client pointing at `base_url` (e.g. `http://localhost:8080`).
    ///
    /// Trailing slashes are stripped from the base URL.
    pub fn new(base_url: &str) -> Result<Self, GameApiError> {
        assert!(!base_url.is_empty(), "game-api base_url must not be empty");

        let inner = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(GameApiError::ClientBuild)?;

        Ok(Self {
            inner,
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }

    // -- Auth (no token required) ------------------------------------------

    /// `POST /auth/challenge` — request a nonce for wallet authentication.
    pub async fn request_challenge(&self, wallet: &str) -> Result<ChallengeResponse, GameApiError> {
        assert!(!wallet.is_empty(), "wallet must not be empty");

        #[derive(Serialize)]
        struct Body<'a> {
            wallet: &'a str,
        }

        let url = format!("{}/auth/challenge", self.base_url);
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

        let url = format!("{}/auth/verify", self.base_url);
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

    // -- Queue (token required) --------------------------------------------

    /// `POST /queue/join` — join the matchmaking queue.
    pub async fn join_queue(
        &self,
        token: &str,
        request: &QueueJoinRequest<'_>,
    ) -> Result<QueueJoinResponse, GameApiError> {
        let url = format!("{}/queue/join", self.base_url);
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

        let url = format!("{}/queue/leave", self.base_url);
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

    // -- Games (token required) --------------------------------------------

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

        let url = format!("{}/games/joined", self.base_url);
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

        let url = format!("{}/games/started", self.base_url);
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

        let url = format!("{}/games/cosign", self.base_url);
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
    pub async fn post_games_resolved(
        &self,
        token: &str,
        game_id: u64,
        p1_guess: u8,
        p2_guess: u8,
    ) -> Result<(), GameApiError> {
        #[derive(Serialize)]
        struct Body {
            game_id: u64,
            p1_guess: u8,
            p2_guess: u8,
            p1_return: u64,
            p2_return: u64,
        }

        let url = format!("{}/games/resolved", self.base_url);
        let resp = self
            .inner
            .post(&url)
            .bearer_auth(token)
            .json(&Body {
                game_id,
                p1_guess,
                p2_guess,
                p1_return: 0,
                p2_return: 0,
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
        let url = format!(
            "{}/tournaments/{tournament_id}/leaderboard?limit={effective_limit}",
            self.base_url,
        );

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
        let body = response.text().await.unwrap_or_default();
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
    fn url_construction_auth_challenge() {
        let client = GameApiClient::new("https://api.example.com").unwrap();
        assert_eq!(
            format!("{}/auth/challenge", client.base_url()),
            "https://api.example.com/auth/challenge"
        );
    }

    #[test]
    fn url_construction_auth_verify() {
        let client = GameApiClient::new("https://api.example.com").unwrap();
        assert_eq!(
            format!("{}/auth/verify", client.base_url()),
            "https://api.example.com/auth/verify"
        );
    }

    #[test]
    fn url_construction_queue_join() {
        let client = GameApiClient::new("http://localhost:8080/").unwrap();
        assert_eq!(
            format!("{}/queue/join", client.base_url()),
            "http://localhost:8080/queue/join"
        );
    }

    #[test]
    fn url_construction_queue_leave() {
        let client = GameApiClient::new("http://localhost:8080/").unwrap();
        assert_eq!(
            format!("{}/queue/leave", client.base_url()),
            "http://localhost:8080/queue/leave"
        );
    }

    #[test]
    fn url_construction_games_joined() {
        let client = GameApiClient::new("http://localhost:8080").unwrap();
        assert_eq!(
            format!("{}/games/joined", client.base_url()),
            "http://localhost:8080/games/joined"
        );
    }

    #[test]
    fn url_construction_games_resolved() {
        let client = GameApiClient::new("http://localhost:8080").unwrap();
        assert_eq!(
            format!("{}/games/resolved", client.base_url()),
            "http://localhost:8080/games/resolved"
        );
    }

    #[test]
    fn url_construction_leaderboard() {
        let client = GameApiClient::new("http://localhost:8080").unwrap();
        assert_eq!(
            format!("{}/tournaments/1/leaderboard?limit=20", client.base_url()),
            "http://localhost:8080/tournaments/1/leaderboard?limit=20"
        );
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
