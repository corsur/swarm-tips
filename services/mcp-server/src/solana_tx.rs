use crate::errors::McpServiceError;
use solana_sdk::signer::Signer;
use std::str::FromStr;

/// Shared parameters for Solana transaction construction.
pub struct TxParams<'a> {
    pub task_id: &'a str,
    pub client_pubkey: &'a str,
    pub wallet_pubkey: &'a str,
    pub session_keypair_bytes: &'a [u8],
    pub solana_rpc_url: &'a str,
    pub program_id: &'a str,
    pub rpc_client: &'a reqwest::Client,
}

/// Construct and submit a claim_task Solana transaction using the session key.
pub async fn submit_claim_task(params: &TxParams<'_>) -> Result<String, McpServiceError> {
    let task_id = params.task_id;
    let client_pubkey = params.client_pubkey;
    let wallet_pubkey = params.wallet_pubkey;
    let session_keypair_bytes = params.session_keypair_bytes;
    let solana_rpc_url = params.solana_rpc_url;
    let program_id = params.program_id;
    let rpc_client = params.rpc_client;
    let keypair = solana_sdk::signer::keypair::Keypair::try_from(session_keypair_bytes)
        .map_err(|e| McpServiceError::TransactionError(format!("invalid session keypair: {e}")))?;

    let program_pubkey = solana_sdk::pubkey::Pubkey::from_str(program_id)
        .map_err(|e| McpServiceError::TransactionError(format!("invalid program id: {e}")))?;

    let wallet_key = solana_sdk::pubkey::Pubkey::from_str(wallet_pubkey)
        .map_err(|e| McpServiceError::TransactionError(format!("invalid wallet: {e}")))?;

    let client_key = solana_sdk::pubkey::Pubkey::from_str(client_pubkey)
        .map_err(|e| McpServiceError::TransactionError(format!("invalid client pubkey: {e}")))?;

    let task_counter: u64 = task_id.parse().map_err(|e| {
        McpServiceError::TransactionError(format!("task_id must be a u64 task_counter: {e}"))
    })?;

    let data = build_anchor_instruction_data("claim_task", &[task_id]);

    let session_pubkey = keypair.pubkey();
    let (session_delegate_pda, _) = solana_sdk::pubkey::Pubkey::find_program_address(
        &[b"session", wallet_key.as_ref(), session_pubkey.as_ref()],
        &program_pubkey,
    );

    let (task_pda, _) = solana_sdk::pubkey::Pubkey::find_program_address(
        &[b"task", &task_counter.to_le_bytes(), client_key.as_ref()],
        &program_pubkey,
    );

    let instruction = solana_sdk::instruction::Instruction {
        program_id: program_pubkey,
        accounts: vec![
            solana_sdk::instruction::AccountMeta::new(task_pda, false),
            solana_sdk::instruction::AccountMeta::new_readonly(wallet_key, false),
            solana_sdk::instruction::AccountMeta::new_readonly(session_pubkey, true),
            solana_sdk::instruction::AccountMeta::new_readonly(session_delegate_pda, false),
        ],
        data,
    };

    submit_transaction(rpc_client, &keypair, &[instruction], solana_rpc_url).await
}

/// Construct and submit a submit_work Solana transaction using the session key.
pub async fn submit_work_tx(
    params: &TxParams<'_>,
    content_id: &str,
) -> Result<String, McpServiceError> {
    let task_id = params.task_id;
    let client_pubkey = params.client_pubkey;
    let wallet_pubkey = params.wallet_pubkey;
    let session_keypair_bytes = params.session_keypair_bytes;
    let solana_rpc_url = params.solana_rpc_url;
    let program_id = params.program_id;
    let rpc_client = params.rpc_client;
    let keypair = solana_sdk::signer::keypair::Keypair::try_from(session_keypair_bytes)
        .map_err(|e| McpServiceError::TransactionError(format!("invalid session keypair: {e}")))?;

    let program_pubkey = solana_sdk::pubkey::Pubkey::from_str(program_id)
        .map_err(|e| McpServiceError::TransactionError(format!("invalid program id: {e}")))?;

    let wallet_key = solana_sdk::pubkey::Pubkey::from_str(wallet_pubkey)
        .map_err(|e| McpServiceError::TransactionError(format!("invalid wallet: {e}")))?;

    let client_key = solana_sdk::pubkey::Pubkey::from_str(client_pubkey)
        .map_err(|e| McpServiceError::TransactionError(format!("invalid client pubkey: {e}")))?;

    let task_counter: u64 = task_id.parse().map_err(|e| {
        McpServiceError::TransactionError(format!("task_id must be a u64 task_counter: {e}"))
    })?;

    let data = build_anchor_instruction_data("submit_work", &[task_id, content_id]);

    let session_pubkey = keypair.pubkey();
    let (session_delegate_pda, _) = solana_sdk::pubkey::Pubkey::find_program_address(
        &[b"session", wallet_key.as_ref(), session_pubkey.as_ref()],
        &program_pubkey,
    );

    let (task_pda, _) = solana_sdk::pubkey::Pubkey::find_program_address(
        &[b"task", &task_counter.to_le_bytes(), client_key.as_ref()],
        &program_pubkey,
    );

    let instruction = solana_sdk::instruction::Instruction {
        program_id: program_pubkey,
        accounts: vec![
            solana_sdk::instruction::AccountMeta::new(task_pda, false),
            solana_sdk::instruction::AccountMeta::new_readonly(wallet_key, false),
            solana_sdk::instruction::AccountMeta::new_readonly(session_pubkey, true),
            solana_sdk::instruction::AccountMeta::new_readonly(session_delegate_pda, false),
        ],
        data,
    };

    submit_transaction(rpc_client, &keypair, &[instruction], solana_rpc_url).await
}

/// Validates a content ID: YouTube video IDs (11 alphanumeric/dash/underscore chars)
/// or X/Twitter tweet IDs (numeric, up to 20 digits).
pub fn is_valid_content_id(id: &str) -> bool {
    if id.is_empty() {
        return false;
    }
    let is_youtube = id.len() == 11
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    let is_tweet = id.len() <= 20 && id.bytes().all(|b| b.is_ascii_digit());
    is_youtube || is_tweet
}

/// Submit a signed transaction to the Solana RPC endpoint.
async fn submit_transaction(
    client: &reqwest::Client,
    signer: &solana_sdk::signer::keypair::Keypair,
    instructions: &[solana_sdk::instruction::Instruction],
    rpc_url: &str,
) -> Result<String, McpServiceError> {
    let blockhash = fetch_latest_blockhash(client, rpc_url).await?;
    let message = solana_sdk::message::Message::new(instructions, Some(&signer.pubkey()));
    let transaction = solana_sdk::transaction::Transaction::new(&[signer], message, blockhash);
    let encoded = serialize_transaction(&transaction)?;
    send_raw_transaction(client, &encoded, rpc_url).await
}

async fn fetch_latest_blockhash(
    client: &reqwest::Client,
    rpc_url: &str,
) -> Result<solana_sdk::hash::Hash, McpServiceError> {
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getLatestBlockhash",
        "params": [{"commitment": "finalized"}]
    });

    let response = client
        .post(rpc_url)
        .json(&request)
        .send()
        .await
        .map_err(|e| McpServiceError::SolanaRpcError(format!("blockhash request failed: {e}")))?;

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| McpServiceError::SolanaRpcError(format!("blockhash parse failed: {e}")))?;

    let blockhash_str = json["result"]["value"]["blockhash"]
        .as_str()
        .ok_or_else(|| {
            McpServiceError::SolanaRpcError("missing blockhash in response".to_string())
        })?;

    blockhash_str
        .parse()
        .map_err(|e| McpServiceError::SolanaRpcError(format!("invalid blockhash: {e}")))
}

fn serialize_transaction(
    transaction: &solana_sdk::transaction::Transaction,
) -> Result<String, McpServiceError> {
    let serialized = bincode::serialize(transaction)
        .map_err(|e| McpServiceError::TransactionError(format!("serialization failed: {e}")))?;
    Ok(bs58::encode(&serialized).into_string())
}

async fn send_raw_transaction(
    client: &reqwest::Client,
    encoded_tx: &str,
    rpc_url: &str,
) -> Result<String, McpServiceError> {
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "sendTransaction",
        "params": [encoded_tx, {"encoding": "base58"}]
    });

    let response = client
        .post(rpc_url)
        .json(&request)
        .send()
        .await
        .map_err(|e| McpServiceError::SolanaRpcError(format!("send transaction failed: {e}")))?;

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| McpServiceError::SolanaRpcError(format!("send response parse failed: {e}")))?;

    if let Some(error) = json.get("error") {
        return Err(McpServiceError::SolanaRpcError(format!(
            "transaction rejected: {error}"
        )));
    }

    json["result"]
        .as_str()
        .ok_or_else(|| McpServiceError::SolanaRpcError("missing signature in response".to_string()))
        .map(|s| s.to_string())
}

/// Build Anchor instruction data: discriminator (8 bytes) + Borsh-serialized string args.
fn build_anchor_instruction_data(instruction_name: &str, string_args: &[&str]) -> Vec<u8> {
    let discriminator = compute_anchor_discriminator(instruction_name);
    let args_size: usize = string_args
        .iter()
        .map(|s| s.len().saturating_add(4))
        .fold(0usize, |acc, x| acc.saturating_add(x));
    let total_size = 8usize.saturating_add(args_size);
    let mut data = Vec::with_capacity(total_size);
    data.extend_from_slice(&discriminator);
    for arg in string_args {
        let len = arg.len() as u32;
        data.extend_from_slice(&len.to_le_bytes());
        data.extend_from_slice(arg.as_bytes());
    }
    data
}

/// Compute the 8-byte Anchor instruction discriminator: SHA-256("global:<name>")[..8]
fn compute_anchor_discriminator(instruction_name: &str) -> [u8; 8] {
    use sha2::{Digest, Sha256};
    let input = format!("global:{instruction_name}");
    let hash = Sha256::digest(input.as_bytes());
    let mut discriminator = [0u8; 8];
    discriminator.copy_from_slice(&hash[..8]);
    discriminator
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchor_discriminator_is_deterministic() {
        let disc1 = compute_anchor_discriminator("claim_task");
        let disc2 = compute_anchor_discriminator("claim_task");
        assert_eq!(disc1, disc2);
    }

    #[test]
    fn anchor_discriminators_differ() {
        let disc1 = compute_anchor_discriminator("claim_task");
        let disc2 = compute_anchor_discriminator("submit_work");
        assert_ne!(disc1, disc2);
        assert_eq!(disc1.len(), 8);
    }

    #[test]
    fn instruction_data_single_arg() {
        let data = build_anchor_instruction_data("claim_task", &["task_001"]);
        let disc = compute_anchor_discriminator("claim_task");
        assert_eq!(&data[..8], &disc);
        let len_bytes: [u8; 4] = data[8..12].try_into().unwrap();
        let str_len = u32::from_le_bytes(len_bytes) as usize;
        assert_eq!(str_len, "task_001".len());
        let content = std::str::from_utf8(&data[12..12 + str_len]).unwrap();
        assert_eq!(content, "task_001");
    }

    #[test]
    fn instruction_data_two_args() {
        let data = build_anchor_instruction_data("submit_work", &["42", "dQw4w9WgXcQ"]);
        assert_eq!(&data[..8], &compute_anchor_discriminator("submit_work"));
        let len1 = u32::from_le_bytes(data[8..12].try_into().unwrap()) as usize;
        assert_eq!(&data[12..12 + len1], b"42");
        let offset2 = 12 + len1;
        let len2 = u32::from_le_bytes(data[offset2..offset2 + 4].try_into().unwrap()) as usize;
        assert_eq!(&data[offset2 + 4..offset2 + 4 + len2], b"dQw4w9WgXcQ");
    }

    #[test]
    fn instruction_data_empty_args() {
        let data = build_anchor_instruction_data("some_ix", &[]);
        assert_eq!(data.len(), 8);
    }

    #[test]
    fn valid_content_ids() {
        assert!(is_valid_content_id("dQw4w9WgXcQ"));
        assert!(is_valid_content_id("abc-_12AB9z"));
        assert!(is_valid_content_id("2039199347657884078"));
        assert!(is_valid_content_id("1234567890"));
    }

    #[test]
    fn rejects_invalid_content_ids() {
        assert!(!is_valid_content_id(""));
        assert!(!is_valid_content_id("abc!@#$%^&*"));
        assert!(!is_valid_content_id("hello world"));
    }
}
