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
                    "signed transaction confirmed"
                );
                Ok(sig)
            }
            Err(e) => {
                let balance = self.rpc.get_balance(&wallet).await.unwrap_or(0);
                tracing::error!(
                    wallet = %wallet,
                    balance_lamports = balance,
                    error = %e,
                    error_debug = ?e,
                    "signed transaction failed"
                );
                Err(e).context("send_and_confirm_transaction")
            }
        }
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
