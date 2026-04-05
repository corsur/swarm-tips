//! Transaction builder for on-chain game operations.
//!
//! `GameTxBuilder` constructs unsigned Solana transactions for each game
//! instruction. It holds only the player's public key — no private key
//! ever touches this code. Callers sign transactions locally and submit
//! via `submit_signed`.

use anchor_lang::AccountDeserialize;
use anyhow::{Context, Result};
use coordination::state::{Game, GameState, GlobalConfig};
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    message::Message, pubkey::Pubkey, signature::Signature, transaction::Transaction,
};

use crate::{instructions, pda};

/// Default poll interval when waiting for a game state transition.
const POLL_INTERVAL_SECS: u64 = 6;

/// Maximum number of poll attempts before timing out.
const POLL_ATTEMPTS: u32 = 100;

/// An unsigned transaction ready for the caller to sign.
#[derive(Debug, Clone)]
pub struct UnsignedTx {
    /// Serialized `Message` bytes — the caller signs these.
    pub message: Vec<u8>,
    /// Base64-encoded message for transport over MCP/JSON.
    pub message_b64: String,
    /// The blockhash used to build the transaction (for reference).
    pub blockhash: String,
    /// Number of required signatures (index 0 = fee payer).
    pub num_signers: u8,
}

/// Non-custodial transaction builder for the coordination game program.
///
/// Constructs unsigned transactions — never holds or sees private keys.
/// The caller signs locally and submits via `submit_signed`.
pub struct GameTxBuilder {
    rpc: RpcClient,
    player: Pubkey,
}

impl GameTxBuilder {
    /// Create a new builder for the given player pubkey.
    pub fn new(rpc_url: &str, player: Pubkey) -> Self {
        assert!(!rpc_url.is_empty(), "rpc_url must not be empty");

        Self {
            rpc: RpcClient::new(rpc_url.to_string()),
            player,
        }
    }

    /// The player's public key.
    pub fn pubkey(&self) -> Pubkey {
        self.player
    }

    /// Reference to the underlying RPC client (for read operations).
    pub fn rpc(&self) -> &RpcClient {
        &self.rpc
    }

    // -- Transaction builders (return unsigned) --------------------------------

    /// Build an unsigned `DepositStake` transaction.
    pub async fn build_deposit_stake(&self, tournament_id: u64) -> Result<UnsignedTx> {
        anyhow::ensure!(tournament_id > 0, "tournament_id must be non-zero");
        let ix = instructions::build_deposit_stake(tournament_id, &self.player);
        self.build_unsigned(&[ix]).await
    }

    /// Build an unsigned `JoinGame` transaction.
    pub async fn build_join_game(&self, game_id: u64, tournament_id: u64) -> Result<UnsignedTx> {
        anyhow::ensure!(game_id > 0, "game_id must be non-zero");
        anyhow::ensure!(tournament_id > 0, "tournament_id must be non-zero");
        let ix = instructions::build_join_game(game_id, tournament_id, &self.player);
        self.build_unsigned(&[ix]).await
    }

    /// Build an unsigned `CommitGuess` transaction.
    pub async fn build_commit_guess(
        &self,
        game_id: u64,
        commitment: [u8; 32],
    ) -> Result<UnsignedTx> {
        anyhow::ensure!(game_id > 0, "game_id must be non-zero");
        let ix = instructions::build_commit_guess(game_id, commitment, &self.player);
        self.build_unsigned(&[ix]).await
    }

    /// Build an unsigned `RevealGuess` transaction.
    ///
    /// Reads the `GlobalConfig` account to get the treasury address.
    #[allow(clippy::too_many_arguments)]
    pub async fn build_reveal_guess(
        &self,
        game_id: u64,
        tournament_id: u64,
        preimage: [u8; 32],
        r_matchup: Option<[u8; 32]>,
        player_one: Pubkey,
        player_two: Pubkey,
    ) -> Result<UnsignedTx> {
        anyhow::ensure!(game_id > 0, "game_id must be non-zero");
        anyhow::ensure!(tournament_id > 0, "tournament_id must be non-zero");

        let (global_config_pda, _) = pda::global_config_pda();
        let global_config_account = self
            .rpc
            .get_account(&global_config_pda)
            .await
            .context("failed to fetch GlobalConfig account")?;
        let global_config = GlobalConfig::try_deserialize(&mut global_config_account.data.as_ref())
            .context("failed to deserialize GlobalConfig")?;

        let ix = instructions::build_reveal_guess(
            game_id,
            tournament_id,
            preimage,
            r_matchup,
            &self.player,
            player_one,
            player_two,
            global_config_pda,
            global_config.treasury,
        );
        self.build_unsigned(&[ix]).await
    }

    /// Build an unsigned `CreateGame` transaction message.
    ///
    /// Returns the message bytes for the player to sign. The matchmaker
    /// co-signature must be obtained separately (via game-api `/games/cosign`).
    /// The caller assembles the final transaction with both signatures.
    pub async fn build_create_game(
        &self,
        tournament_id: u64,
        stake_lamports: u64,
        matchup_commitment: [u8; 32],
        matchmaker: &Pubkey,
    ) -> Result<UnsignedTx> {
        anyhow::ensure!(tournament_id > 0, "tournament_id must be non-zero");

        let (counter_pda, _) = pda::game_counter_pda();
        let counter_data = self
            .rpc
            .get_account_data(&counter_pda)
            .await
            .context("failed to read game_counter")?;
        anyhow::ensure!(counter_data.len() >= 16, "game_counter data too short");
        let game_counter_value =
            u64::from_le_bytes(counter_data[8..16].try_into().context("parse count")?);

        let ix = instructions::build_create_game(
            stake_lamports,
            matchup_commitment,
            tournament_id,
            game_counter_value,
            &self.player,
            matchmaker,
        );

        let unsigned = self.build_unsigned(&[ix]).await?;
        Ok(unsigned)
    }

    // -- Submit ----------------------------------------------------------------

    /// Submit a pre-signed transaction to the network.
    ///
    /// The transaction must be fully signed (all required signers) and
    /// serialized as bincode bytes.
    pub async fn submit_signed(&self, signed_tx_bytes: &[u8]) -> Result<Signature> {
        let tx: Transaction = bincode::deserialize(signed_tx_bytes)
            .context("failed to deserialize signed transaction")?;

        let wallet = self.player;
        let balance_before = self.rpc.get_balance(&wallet).await.unwrap_or(0);

        // Retry transient failures with exponential backoff (1s, 2s, 4s).
        let mut last_err = None;
        for attempt in 0..3u32 {
            if attempt > 0 {
                let delay = std::time::Duration::from_secs(1u64 << attempt.saturating_sub(1));
                tracing::info!(
                    wallet = %wallet,
                    attempt = attempt.saturating_add(1),
                    delay_ms = delay.as_millis() as u64,
                    "retrying transaction submission"
                );
                tokio::time::sleep(delay).await;
            }

            match self.rpc.send_and_confirm_transaction(&tx).await {
                Ok(sig) => {
                    let balance_after = self.rpc.get_balance(&wallet).await.unwrap_or(0);
                    let cost_lamports = balance_before.saturating_sub(balance_after);
                    tracing::info!(
                        wallet = %wallet,
                        %sig,
                        balance_before,
                        balance_after,
                        cost_lamports,
                        attempt = attempt.saturating_add(1),
                        "signed transaction confirmed"
                    );
                    return Ok(sig);
                }
                Err(e) => {
                    let err_str = e.to_string();
                    let is_transient = err_str.contains("timeout")
                        || err_str.contains("429")
                        || err_str.contains("502")
                        || err_str.contains("503")
                        || err_str.contains("connection")
                        || err_str.contains("ConnectionRefused");

                    // Classify error for structured logging
                    let error_kind = if err_str.contains("Blockhash not found") {
                        "blockhash_expired"
                    } else if err_str.contains("insufficient lamports") {
                        "insufficient_funds"
                    } else if err_str.contains("custom program error") {
                        "program_error"
                    } else if is_transient {
                        "transient"
                    } else {
                        "unknown"
                    };

                    tracing::warn!(
                        wallet = %wallet,
                        attempt = attempt.saturating_add(1),
                        error_kind = error_kind,
                        error = %e,
                        "transaction submission failed"
                    );

                    // Don't retry non-transient errors (program errors, blockhash expired, etc.)
                    if !is_transient {
                        let balance = self.rpc.get_balance(&wallet).await.unwrap_or(0);
                        tracing::error!(
                            wallet = %wallet,
                            balance_lamports = balance,
                            error_kind = error_kind,
                            error = %e,
                            "transaction failed (non-retryable)"
                        );
                        return Err(e).context(format!(
                            "send_and_confirm_transaction ({error_kind}): {err_str}"
                        ));
                    }
                    last_err = Some(e);
                }
            }
        }

        let balance = self.rpc.get_balance(&wallet).await.unwrap_or(0);
        tracing::error!(
            wallet = %wallet,
            balance_lamports = balance,
            attempts = 3,
            "transaction failed after all retries"
        );
        Err(last_err.unwrap()).context("send_and_confirm_transaction: all retries exhausted")
    }

    // -- Read-only operations --------------------------------------------------

    /// Read and deserialize a game account by game ID.
    pub async fn read_game(&self, game_id: u64) -> Result<Option<Game>> {
        let (pda, _) = pda::game_pda(game_id);
        match self.rpc.get_account(&pda).await {
            Ok(account) => {
                let game = Game::try_deserialize(&mut account.data.as_ref())
                    .context("failed to deserialize Game")?;
                Ok(Some(game))
            }
            Err(e) => {
                let msg = format!("{e}");
                if msg.contains("AccountNotFound") || msg.contains("could not find account") {
                    Ok(None)
                } else {
                    Err(anyhow::anyhow!(e).context("failed to fetch Game account"))
                }
            }
        }
    }

    /// Poll until the game reaches `target` state, then return it.
    pub async fn wait_for_game_state(&self, game_id: u64, target: GameState) -> Result<Game> {
        for _ in 0..POLL_ATTEMPTS {
            let game = self
                .read_game(game_id)
                .await?
                .context("game account not found while polling")?;
            if game.state == target {
                return Ok(game);
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(POLL_INTERVAL_SECS)).await;
        }
        anyhow::bail!("timed out waiting for game {game_id} to reach state {target:?}")
    }

    // -- Internal helpers ------------------------------------------------------

    /// Build an unsigned transaction from instructions.
    async fn build_unsigned(
        &self,
        ixs: &[solana_sdk::instruction::Instruction],
    ) -> Result<UnsignedTx> {
        let blockhash = self
            .rpc
            .get_latest_blockhash()
            .await
            .context("get_latest_blockhash")?;

        let message = Message::new_with_blockhash(ixs, Some(&self.player), &blockhash);
        let message_bytes = message.serialize();

        use base64::Engine;
        let message_b64 = base64::engine::general_purpose::STANDARD.encode(&message_bytes);

        Ok(UnsignedTx {
            message: message_bytes,
            message_b64,
            blockhash: blockhash.to_string(),
            num_signers: message.header.num_required_signatures,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::{
        hash::Hash,
        instruction::{AccountMeta, Instruction},
        pubkey,
        signature::Keypair,
        signer::Signer,
        transaction::Transaction,
    };

    /// System program ID (avoids deprecated `system_program` module).
    const SYSTEM_PROGRAM: Pubkey = pubkey!("11111111111111111111111111111111");

    /// Helper: build an UnsignedTx from a simple transfer-like instruction.
    fn make_unsigned_tx(payer: &Pubkey) -> UnsignedTx {
        let ix = Instruction {
            program_id: SYSTEM_PROGRAM,
            accounts: vec![
                AccountMeta::new(*payer, true),
                AccountMeta::new(Pubkey::new_unique(), false),
            ],
            data: vec![2, 0, 0, 0, 64, 66, 15, 0, 0, 0, 0, 0], // Transfer 1M lamports
        };

        let blockhash = Hash::new_unique();
        let message = Message::new_with_blockhash(&[ix], Some(payer), &blockhash);
        let message_bytes = message.serialize();

        use base64::Engine;
        let message_b64 = base64::engine::general_purpose::STANDARD.encode(&message_bytes);

        UnsignedTx {
            message: message_bytes,
            message_b64,
            blockhash: blockhash.to_string(),
            num_signers: message.header.num_required_signatures,
        }
    }

    #[test]
    fn unsigned_tx_message_b64_round_trips() {
        let payer = Pubkey::new_unique();
        let unsigned = make_unsigned_tx(&payer);

        use base64::Engine;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&unsigned.message_b64)
            .expect("base64 decode");
        assert_eq!(
            decoded, unsigned.message,
            "b64 must round-trip to message bytes"
        );
    }

    #[test]
    fn unsigned_tx_message_deserializes_to_valid_message() {
        let payer = Pubkey::new_unique();
        let unsigned = make_unsigned_tx(&payer);

        let message: Message =
            bincode::deserialize(&unsigned.message).expect("message must deserialize");
        assert_eq!(message.account_keys[0], payer, "first key must be payer");
        assert_eq!(
            message.header.num_required_signatures, unsigned.num_signers,
            "num_signers must match header"
        );
    }

    #[test]
    fn unsigned_tx_blockhash_parses_back() {
        let payer = Pubkey::new_unique();
        let unsigned = make_unsigned_tx(&payer);

        let hash: Hash = unsigned
            .blockhash
            .parse()
            .expect("blockhash must parse back to Hash");
        let message: Message = bincode::deserialize(&unsigned.message).unwrap();
        assert_eq!(
            message.recent_blockhash, hash,
            "parsed blockhash must match message blockhash"
        );
    }

    #[test]
    fn unsigned_tx_can_be_signed_into_valid_transaction() {
        let keypair = Keypair::new();
        let unsigned = make_unsigned_tx(&keypair.pubkey());

        let message: Message = bincode::deserialize(&unsigned.message).unwrap();
        let blockhash: Hash = unsigned.blockhash.parse().unwrap();

        let mut tx = Transaction::new_unsigned(message);
        tx.sign(&[&keypair], blockhash);

        assert!(
            tx.verify().is_ok(),
            "signed transaction must have valid signatures"
        );
        assert_eq!(
            tx.signatures.len(),
            unsigned.num_signers as usize,
            "signature count must match num_signers"
        );
    }

    #[test]
    fn unsigned_tx_num_signers_is_one_for_single_signer_tx() {
        let payer = Pubkey::new_unique();
        let unsigned = make_unsigned_tx(&payer);
        assert_eq!(
            unsigned.num_signers, 1,
            "single-signer tx must have num_signers=1"
        );
    }

    #[test]
    fn submit_signed_rejects_garbage_bytes() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let builder = GameTxBuilder::new("http://localhost:8899", Pubkey::new_unique());

        let result = rt.block_on(builder.submit_signed(b"not a valid transaction"));
        assert!(result.is_err(), "garbage bytes must fail deserialization");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("deserialize"),
            "error must mention deserialization, got: {err}"
        );
    }
}
