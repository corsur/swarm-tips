//! Trustless on-chain reads for the `agent_profile` MCP tool (#29).
//!
//! Reads `AgentState` (Shillbot reputation) and `PlayerProfile`
//! (Coordination Game) PDAs directly from Solana via raw JSON-RPC
//! `getAccountInfo` calls — no orchestrator hop, no Firestore cache,
//! no signing. Field offsets mirror the on-chain account layouts in
//! `programs/shillbot/src/state/agent.rs::AgentState` (post-#12) and
//! `programs/coordination-game/src/state/player.rs::PlayerProfile`.
//!
//! Anchor account discriminators are verified before parsing as
//! defense-in-depth: if the off-chain `derive_pda` ever produced a
//! wrong address that happened to host a different account type,
//! length matching alone could let parsing proceed against garbage.
//! The discriminator check (`sha256("account:<Name>")[0..8]`,
//! mirroring `services/aas-verifier-ts/src/discriminator.ts`)
//! fails fast with a clear error instead. See
//! `fetch_account_data`.

use base64::Engine;

use crate::errors::McpServiceError;

/// Shillbot program ID (mainnet + devnet — same address).
const SHILLBOT_PROGRAM_ID_BASE58: &str = "2tR37nqMpwdV4DVUHjzUmL1rH2DtkA8zrRA4EAhT7KMi";

/// Coordination Game program ID.
const COORDINATION_GAME_PROGRAM_ID_BASE58: &str = "2qqVk7kUqffnahiJpcQJCsSd8ErbEUgKTgCn1zYsw64P";

/// Anchor account discriminator: `sha256("account:" + name)[0..8]`.
/// Used by `fetch_account_data` to defense-in-depth-verify the read
/// landed on the right account type before parsing field offsets.
/// Computed at module-load time (constant input → constant output).
fn anchor_discriminator(account_kind: &str) -> [u8; 8] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(format!("account:{account_kind}").as_bytes());
    let h: [u8; 32] = hasher.finalize().into();
    let mut out = [0u8; 8];
    out.copy_from_slice(&h[0..8]);
    out
}

/// Field-offset map for the post-#12 `AgentState` body
/// (`programs/shillbot/src/state/agent.rs`):
///   off  size  field
///     0    32  agent
///    32     1  claimed_count
///    33     8  total_completed
///    41     8  total_earned
///    49     8  total_score_sum
///    57     8  total_tasks_claimed
///    65     8  total_challenges_lost
///    73     8  _reserved
///    81     1  bump
/// Total body = 82 bytes; with 8-byte Anchor discriminator = 90 bytes.
const AGENT_STATE_MIN_BODY_LEN: usize = 82;

/// Field-offset map for the `PlayerProfile` body
/// (`programs/coordination-game/src/state/player.rs`):
///   off  size  field
///     0    32  wallet
///    32     8  tournament_id
///    40     8  wins
///    48     8  total_games
///    56     8  score
///    64     1  claimed
///    65     1  bump
/// Total body = 66 bytes; with discriminator = 74 bytes.
const PLAYER_PROFILE_MIN_BODY_LEN: usize = 66;

/// Subset of `AgentState` fields the `agent_profile` tool surfaces.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AgentStateData {
    pub agent: String,
    pub claimed_count: u8,
    pub total_completed: u64,
    pub total_earned: u64,
    pub total_score_sum: u64,
    pub total_tasks_claimed: u64,
    pub total_challenges_lost: u64,
}

/// Subset of `PlayerProfile` fields the `agent_profile` tool surfaces.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PlayerProfileData {
    pub wallet: String,
    pub tournament_id: u64,
    pub wins: u64,
    pub total_games: u64,
    pub score: u64,
}

/// Derive the AgentState PDA for a given agent wallet.
/// Seeds: `["agent_state", agent_pubkey]`.
pub fn agent_state_pda(agent_pubkey_b58: &str) -> Result<String, McpServiceError> {
    let agent_bytes = bs58::decode(agent_pubkey_b58)
        .into_vec()
        .map_err(|e| McpServiceError::InvalidInput(format!("invalid agent pubkey base58: {e}")))?;
    if agent_bytes.len() != 32 {
        return Err(McpServiceError::InvalidInput(format!(
            "agent pubkey must be 32 bytes, got {}",
            agent_bytes.len()
        )));
    }
    let program_id = bs58::decode(SHILLBOT_PROGRAM_ID_BASE58)
        .into_vec()
        .map_err(|e| McpServiceError::Internal(format!("invalid program id (constant): {e}")))?;
    let pda = derive_pda(&[b"agent_state", &agent_bytes], &program_id);
    Ok(bs58::encode(pda).into_string())
}

/// Derive the PlayerProfile PDA for `(wallet, tournament_id)`.
/// Seeds: `["player", tournament_id (8-byte LE), wallet]`.
pub fn player_profile_pda(wallet_b58: &str, tournament_id: u64) -> Result<String, McpServiceError> {
    let wallet_bytes = bs58::decode(wallet_b58)
        .into_vec()
        .map_err(|e| McpServiceError::InvalidInput(format!("invalid wallet base58: {e}")))?;
    if wallet_bytes.len() != 32 {
        return Err(McpServiceError::InvalidInput(format!(
            "wallet must be 32 bytes, got {}",
            wallet_bytes.len()
        )));
    }
    let program_id = bs58::decode(COORDINATION_GAME_PROGRAM_ID_BASE58)
        .into_vec()
        .map_err(|e| {
            McpServiceError::Internal(format!("invalid coordination-game program id: {e}"))
        })?;
    let tid_le = tournament_id.to_le_bytes();
    let pda = derive_pda(&[b"player", &tid_le, &wallet_bytes], &program_id);
    Ok(bs58::encode(pda).into_string())
}

/// Read the AgentState account for a given agent wallet, parse its
/// counters, and return them. Returns `Ok(None)` when the account
/// doesn't exist (agent has never claimed a task).
pub async fn read_agent_state(
    client: &reqwest::Client,
    rpc_url: &str,
    agent_pubkey_b58: &str,
) -> Result<Option<AgentStateData>, McpServiceError> {
    let pda = agent_state_pda(agent_pubkey_b58)?;
    let expected_disc = anchor_discriminator("AgentState");
    let body = match fetch_account_data(client, rpc_url, &pda, Some(&expected_disc)).await? {
        None => return Ok(None),
        Some(b) => b,
    };
    if body.len() < AGENT_STATE_MIN_BODY_LEN {
        return Err(McpServiceError::OrchestratorError(format!(
            "AgentState body too short: {} bytes (need >= {AGENT_STATE_MIN_BODY_LEN})",
            body.len()
        )));
    }
    let agent_bytes: [u8; 32] = body[0..32]
        .try_into()
        .map_err(|_| McpServiceError::OrchestratorError("AgentState agent slice".to_string()))?;
    Ok(Some(AgentStateData {
        agent: bs58::encode(agent_bytes).into_string(),
        claimed_count: body[32],
        total_completed: read_u64_le(&body[33..41])?,
        total_earned: read_u64_le(&body[41..49])?,
        total_score_sum: read_u64_le(&body[49..57])?,
        total_tasks_claimed: read_u64_le(&body[57..65])?,
        total_challenges_lost: read_u64_le(&body[65..73])?,
    }))
}

/// Read the PlayerProfile account for `(wallet, tournament_id)`. Returns
/// `Ok(None)` when the account doesn't exist (player has never joined
/// this tournament).
pub async fn read_player_profile(
    client: &reqwest::Client,
    rpc_url: &str,
    wallet_b58: &str,
    tournament_id: u64,
) -> Result<Option<PlayerProfileData>, McpServiceError> {
    let pda = player_profile_pda(wallet_b58, tournament_id)?;
    let expected_disc = anchor_discriminator("PlayerProfile");
    let body = match fetch_account_data(client, rpc_url, &pda, Some(&expected_disc)).await? {
        None => return Ok(None),
        Some(b) => b,
    };
    if body.len() < PLAYER_PROFILE_MIN_BODY_LEN {
        return Err(McpServiceError::OrchestratorError(format!(
            "PlayerProfile body too short: {} bytes (need >= {PLAYER_PROFILE_MIN_BODY_LEN})",
            body.len()
        )));
    }
    let wallet_bytes: [u8; 32] = body[0..32].try_into().map_err(|_| {
        McpServiceError::OrchestratorError("PlayerProfile wallet slice".to_string())
    })?;
    Ok(Some(PlayerProfileData {
        wallet: bs58::encode(wallet_bytes).into_string(),
        tournament_id: read_u64_le(&body[32..40])?,
        wins: read_u64_le(&body[40..48])?,
        total_games: read_u64_le(&body[48..56])?,
        score: read_u64_le(&body[56..64])?,
    }))
}

/// Read all PlayerProfile accounts for a given tournament_id via
/// `getProgramAccounts` with a discriminator + tournament_id memcmp
/// filter. Decodes each into `PlayerProfileData` and returns sorted by
/// `score` descending (the leaderboard order).
///
/// Used by the `game_get_leaderboard` MCP tool. Replaces the dead
/// `game_api_client::get_leaderboard` HTTP path that pointed at a
/// game-api endpoint that never existed (0b-FINDING-1, 2026-05-07
/// audit). Reading on-chain directly is the right architectural
/// answer because PlayerProfile data lives entirely on-chain — the
/// game-api hop was an unnecessary indirection.
pub async fn read_all_player_profiles_for_tournament(
    client: &reqwest::Client,
    rpc_url: &str,
    tournament_id: u64,
) -> Result<Vec<PlayerProfileData>, McpServiceError> {
    let expected_disc = anchor_discriminator("PlayerProfile");
    let disc_b58 = bs58::encode(expected_disc).into_string();
    let tid_b58 = bs58::encode(tournament_id.to_le_bytes()).into_string();

    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getProgramAccounts",
        "params": [
            COORDINATION_GAME_PROGRAM_ID_BASE58,
            {
                "encoding": "base64",
                "commitment": "confirmed",
                "filters": [
                    { "memcmp": { "offset": 0, "bytes": disc_b58 } },
                    // tournament_id is at offset 8 (disc) + 32 (wallet) = 40.
                    { "memcmp": { "offset": 40, "bytes": tid_b58 } },
                ],
            },
        ],
    });

    let resp = client
        .post(rpc_url)
        .json(&request)
        .send()
        .await
        .map_err(|e| {
            McpServiceError::OrchestratorError(format!("getProgramAccounts request failed: {e}"))
        })?;

    if !resp.status().is_success() {
        return Err(McpServiceError::OrchestratorError(format!(
            "getProgramAccounts RPC status {}",
            resp.status()
        )));
    }

    let body: serde_json::Value = resp.json().await.map_err(|e| {
        McpServiceError::OrchestratorError(format!("getProgramAccounts JSON parse failed: {e}"))
    })?;

    let result_arr = body
        .get("result")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            McpServiceError::OrchestratorError(format!(
                "getProgramAccounts missing result array: {body}"
            ))
        })?;

    let mut out = Vec::with_capacity(result_arr.len());
    for entry in result_arr {
        let data_arr = entry
            .get("account")
            .and_then(|a| a.get("data"))
            .and_then(|d| d.as_array())
            .ok_or_else(|| {
                McpServiceError::OrchestratorError(
                    "getProgramAccounts entry missing account.data".to_string(),
                )
            })?;
        let b64 = data_arr.first().and_then(|v| v.as_str()).ok_or_else(|| {
            McpServiceError::OrchestratorError(
                "getProgramAccounts entry data[0] not a string".to_string(),
            )
        })?;

        use base64::{engine::general_purpose::STANDARD, Engine as _};
        let raw = STANDARD.decode(b64).map_err(|e| {
            McpServiceError::OrchestratorError(format!("base64 decode failed: {e}"))
        })?;
        if raw.len() < 8 || raw[..8] != expected_disc {
            // Filter should have prevented this, but defense-in-depth.
            continue;
        }
        let body_bytes = &raw[8..];
        if body_bytes.len() < PLAYER_PROFILE_MIN_BODY_LEN {
            continue; // skip malformed entries
        }
        let wallet_bytes: [u8; 32] = body_bytes[0..32].try_into().map_err(|_| {
            McpServiceError::OrchestratorError("PlayerProfile wallet slice".to_string())
        })?;
        out.push(PlayerProfileData {
            wallet: bs58::encode(wallet_bytes).into_string(),
            tournament_id: read_u64_le(&body_bytes[32..40])?,
            wins: read_u64_le(&body_bytes[40..48])?,
            total_games: read_u64_le(&body_bytes[48..56])?,
            score: read_u64_le(&body_bytes[56..64])?,
        });
    }

    // Leaderboard order: highest score first. Stable secondary sort by
    // wallet to keep ordering deterministic across calls when scores tie.
    out.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.wallet.cmp(&b.wallet)));
    Ok(out)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// JSON-RPC `getAccountInfo` with base64 encoding. Returns the data
/// bytes AFTER the 8-byte Anchor discriminator (caller works with
/// the body), or `None` if the account doesn't exist.
///
/// `expected_discriminator` adds defense-in-depth: if Some, the
/// returned account's first 8 bytes MUST match (the canonical Anchor
/// `sha256("account:<Name>")[0..8]`). If None, the discriminator is
/// stripped without verification (legacy callers; new code passes
/// the expected bytes). A discriminator mismatch returns
/// `OrchestratorError` rather than silently parsing garbage as the
/// caller's expected struct — covers the "what if our locally-
/// derived PDA points at a different account type" failure mode
/// even when on-chain account ownership says it shouldn't.
async fn fetch_account_data(
    client: &reqwest::Client,
    rpc_url: &str,
    pubkey_b58: &str,
    expected_discriminator: Option<&[u8; 8]>,
) -> Result<Option<Vec<u8>>, McpServiceError> {
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getAccountInfo",
        "params": [
            pubkey_b58,
            { "encoding": "base64", "commitment": "confirmed" }
        ],
    });

    let resp = client
        .post(rpc_url)
        .json(&request)
        .send()
        .await
        .map_err(|e| McpServiceError::OrchestratorError(format!("RPC request failed: {e}")))?;

    if !resp.status().is_success() {
        return Err(McpServiceError::OrchestratorError(format!(
            "RPC status {}",
            resp.status()
        )));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| McpServiceError::OrchestratorError(format!("RPC parse failed: {e}")))?;

    let value = body.get("result").and_then(|r| r.get("value"));
    let value = match value {
        None | Some(serde_json::Value::Null) => return Ok(None),
        Some(v) => v,
    };

    let data_arr = value
        .get("data")
        .and_then(|d| d.as_array())
        .ok_or_else(|| {
            McpServiceError::OrchestratorError("RPC response missing data array".to_string())
        })?;
    let b64 = data_arr.first().and_then(|s| s.as_str()).ok_or_else(|| {
        McpServiceError::OrchestratorError("RPC response data[0] not a string".to_string())
    })?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| McpServiceError::OrchestratorError(format!("base64 decode: {e}")))?;

    // Verify + strip the 8-byte Anchor discriminator. Verifying covers
    // the "our locally-derived PDA points at a different-type account"
    // failure mode — Solana's runtime prevents writes to PDAs from
    // arbitrary programs, but if the off-chain `derive_pda` produced
    // a wrong address that happens to host a different account type,
    // length matching alone could let parsing proceed against garbage.
    // The discriminator check fails fast with a clear error.
    if bytes.len() < 8 {
        return Err(McpServiceError::OrchestratorError(format!(
            "account too short for discriminator: {} bytes",
            bytes.len()
        )));
    }
    if let Some(expected) = expected_discriminator {
        if &bytes[0..8] != expected.as_slice() {
            return Err(McpServiceError::OrchestratorError(format!(
                "account discriminator mismatch at {pubkey_b58}: expected {:02x?}, got {:02x?}",
                expected,
                &bytes[0..8]
            )));
        }
    }
    Ok(Some(bytes[8..].to_vec()))
}

fn read_u64_le(slice: &[u8]) -> Result<u64, McpServiceError> {
    let arr: [u8; 8] = slice.try_into().map_err(|_| {
        McpServiceError::OrchestratorError(format!("u64 slice length wrong: {}", slice.len()))
    })?;
    Ok(u64::from_le_bytes(arr))
}

/// Minimal off-chain `find_program_address` analogue. Solana uses
/// SHA-256 with a "ProgramDerivedAddress" sentinel and bumps from 255
/// down to 0; we replicate the algorithm so the MCP server doesn't
/// need to depend on the heavy `solana-sdk`. This function PANICS if
/// no PDA can be derived (vanishingly unlikely in practice).
fn derive_pda(seeds: &[&[u8]], program_id: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    const PDA_SENTINEL: &[u8] = b"ProgramDerivedAddress";

    for bump in (0u8..=255).rev() {
        let mut hasher = Sha256::new();
        for s in seeds {
            hasher.update(s);
        }
        hasher.update([bump]);
        hasher.update(program_id);
        hasher.update(PDA_SENTINEL);
        let candidate: [u8; 32] = hasher.finalize().into();
        // Curve25519 off-curve check: a real PDA must NOT lie on the
        // ed25519 curve. We use the curve25519-dalek check via the
        // ed25519-dalek crate.
        if !is_on_curve_compressed(&candidate) {
            return candidate;
        }
    }
    // 256 bumps without a valid PDA is a 1-in-2^256 event; treat as
    // unreachable.
    panic!("PDA derivation failed: no off-curve point in 256 bumps")
}

/// Check whether a 32-byte compressed Edwards point lies on the
/// ed25519 curve. Returns `true` if on-curve (rejected as a PDA),
/// `false` if off-curve (accepted).
///
/// We use `ed25519-dalek`'s `VerifyingKey::from_bytes` as a proxy: a
/// compressed point that fails to decompress is not on the curve.
fn is_on_curve_compressed(bytes: &[u8; 32]) -> bool {
    ed25519_dalek::VerifyingKey::from_bytes(bytes).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_state_pda_is_deterministic_and_matches_program_seeds() {
        // Constant test vector: a known wallet derives a stable PDA
        // under the seeds `["agent_state", wallet]`. We don't have a
        // canonical "expected" PDA value to compare against, so this
        // test just asserts determinism + base58 shape.
        let wallet = "11111111111111111111111111111112";
        let pda1 = agent_state_pda(wallet).unwrap();
        let pda2 = agent_state_pda(wallet).unwrap();
        assert_eq!(pda1, pda2);
        assert!(pda1.len() >= 32 && pda1.len() <= 44); // base58 of 32 bytes is 32-44 chars
    }

    #[test]
    fn player_profile_pda_is_deterministic_per_tournament() {
        let wallet = "11111111111111111111111111111112";
        let p_t1 = player_profile_pda(wallet, 1).unwrap();
        let p_t2 = player_profile_pda(wallet, 2).unwrap();
        let p_t1_again = player_profile_pda(wallet, 1).unwrap();
        // Same (wallet, tournament_id) → same PDA.
        assert_eq!(p_t1, p_t1_again);
        // Different tournament_id → different PDA.
        assert_ne!(p_t1, p_t2);
    }

    #[test]
    fn agent_state_pda_rejects_invalid_pubkey() {
        let res = agent_state_pda("not-a-base58-pubkey!!");
        assert!(res.is_err());
    }

    #[test]
    fn read_u64_le_round_trips() {
        let n: u64 = 0x0123_4567_89AB_CDEF;
        let bytes = n.to_le_bytes();
        let parsed = read_u64_le(&bytes).unwrap();
        assert_eq!(parsed, n);
    }

    #[test]
    fn read_u64_le_rejects_wrong_length() {
        assert!(read_u64_le(&[0u8; 7]).is_err());
        assert!(read_u64_le(&[0u8; 9]).is_err());
    }

    /// Canonical cross-check: the locally-implemented `derive_pda`
    /// MUST produce the same address as Solana's
    /// `Pubkey::find_program_address` for any (seeds, program_id).
    /// Without this test, a subtle algorithm error in `derive_pda`
    /// (hash field ordering, sentinel typo, off-curve check
    /// disagreement, bump direction) would silently make every PDA
    /// derivation wrong and every `agent_profile` read return None.
    #[test]
    fn derive_pda_matches_solana_sdk_find_program_address() {
        use solana_sdk::pubkey::Pubkey;
        use std::str::FromStr;

        let shillbot_program = Pubkey::from_str(SHILLBOT_PROGRAM_ID_BASE58).unwrap();
        let coordination_program = Pubkey::from_str(COORDINATION_GAME_PROGRAM_ID_BASE58).unwrap();
        let shillbot_program_bytes = shillbot_program.to_bytes();
        let coordination_program_bytes = coordination_program.to_bytes();

        // Test wallets — three distinct ones so multiple bumps get
        // exercised.
        let wallets_b58 = [
            "11111111111111111111111111111112",
            "11111111111111111111111111111113",
            "9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM",
        ];

        for wallet_b58 in wallets_b58 {
            let wallet_pk = Pubkey::from_str(wallet_b58).unwrap();
            let wallet_bytes = wallet_pk.to_bytes();

            // ----- AgentState seeds -----
            let (sdk_pda, _sdk_bump) = Pubkey::find_program_address(
                &[b"agent_state", wallet_bytes.as_ref()],
                &shillbot_program,
            );
            let local_pda_bytes =
                derive_pda(&[b"agent_state", &wallet_bytes], &shillbot_program_bytes);
            assert_eq!(
                sdk_pda.to_bytes(),
                local_pda_bytes,
                "AgentState PDA divergence for wallet {wallet_b58}"
            );

            // ----- PlayerProfile seeds (per-tournament) -----
            for tournament_id in [0u64, 1, 42] {
                let tid_le = tournament_id.to_le_bytes();
                let (sdk_pda, _sdk_bump) = Pubkey::find_program_address(
                    &[b"player", tid_le.as_ref(), wallet_bytes.as_ref()],
                    &coordination_program,
                );
                let local_pda_bytes = derive_pda(
                    &[b"player", &tid_le, &wallet_bytes],
                    &coordination_program_bytes,
                );
                assert_eq!(
                    sdk_pda.to_bytes(),
                    local_pda_bytes,
                    "PlayerProfile PDA divergence for wallet {wallet_b58}, tournament_id {tournament_id}"
                );
            }
        }
    }

    #[test]
    fn anchor_discriminator_matches_canonical() {
        // sha256("account:Task")[0..8] — same value that the AAS
        // verifier in `services/aas-verifier-{ts,py}` computes. The
        // bytes themselves aren't documented anywhere as a
        // hardcoded constant; we just verify determinism + length
        // here. The cross-language cross-check happens in the AAS
        // verifier suite.
        let task_disc = anchor_discriminator("Task");
        let task_disc2 = anchor_discriminator("Task");
        assert_eq!(task_disc, task_disc2);
        assert_eq!(task_disc.len(), 8);

        // Different account-kind names must yield different
        // discriminators (sha256 collision resistance — practically
        // certain).
        let agent_state_disc = anchor_discriminator("AgentState");
        let player_disc = anchor_discriminator("PlayerProfile");
        assert_ne!(task_disc, agent_state_disc);
        assert_ne!(agent_state_disc, player_disc);
        assert_ne!(player_disc, task_disc);
    }
}
