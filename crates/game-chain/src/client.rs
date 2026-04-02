//! High-level RPC client for on-chain game operations.
//!
//! `GameChainClient` wraps an `RpcClient` and keypair, providing
//! ergonomic async methods for each game instruction: deposit stake,
//! join game, commit guess, reveal guess, read game state, and poll
//! for state transitions.

use std::sync::Arc;

use anchor_lang::AccountDeserialize;
use anyhow::{Context, Result};
use coordination::state::{Game, GameState, GlobalConfig};
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signature},
    signer::Signer,
    transaction::Transaction,
};

use crate::{instructions, pda};

/// Default poll interval when waiting for a game state transition.
const POLL_INTERVAL_SECS: u64 = 6;

/// Maximum number of poll attempts before timing out.
const POLL_ATTEMPTS: u32 = 100;

/// Maximum retry attempts for join_game (account propagation delay).
const JOIN_GAME_MAX_ATTEMPTS: u32 = 5;

/// Retry delay for join_game retries.
const JOIN_GAME_RETRY_DELAY_SECS: u64 = 3;

/// Initial delay before the first join_game attempt, to allow the game
/// account to propagate across validator nodes.
const JOIN_GAME_INITIAL_DELAY_SECS: u64 = 2;

/// Anchor error 3012 = AccountNotInitialized.
const ACCOUNT_NOT_INITIALIZED: &str = "0xbc4";

/// High-level client for interacting with the coordination game program.
///
/// Wraps Solana RPC calls and transaction construction. All methods that
/// submit transactions return the transaction `Signature` on success.
pub struct GameChainClient {
    rpc: RpcClient,
    keypair: Arc<Keypair>,
}

impl GameChainClient {
    /// Create a new client connected to the given RPC URL.
    pub fn new(rpc_url: &str, keypair: Arc<Keypair>) -> Self {
        assert!(!rpc_url.is_empty(), "rpc_url must not be empty");

        Self {
            rpc: RpcClient::new(rpc_url.to_string()),
            keypair,
        }
    }

    /// Return a reference to the underlying keypair's public key.
    pub fn pubkey(&self) -> Pubkey {
        self.keypair.pubkey()
    }

    /// Return a reference to the underlying RPC client.
    pub fn rpc(&self) -> &RpcClient {
        &self.rpc
    }

    /// Deposit the fixed stake into the per-player escrow PDA.
    pub async fn deposit_stake(&self, tournament_id: u64) -> Result<Signature> {
        anyhow::ensure!(tournament_id > 0, "tournament_id must be non-zero");

        let ix = instructions::build_deposit_stake(tournament_id, self.keypair.as_ref());
        let sig = self.send_and_confirm(&[ix]).await?;

        tracing::info!(
            tournament_id,
            wallet = %self.keypair.pubkey(),
            %sig,
            "deposited stake into escrow"
        );
        Ok(sig)
    }

    /// Join an on-chain game as Player 2, with retry logic for account
    /// propagation delays.
    ///
    /// Retries up to 5 times with a 3-second back-off when the game
    /// account is not yet visible (AccountNotInitialized error).
    pub async fn join_game(&self, game_id: u64, tournament_id: u64) -> Result<Signature> {
        anyhow::ensure!(game_id > 0, "game_id must be non-zero");
        anyhow::ensure!(tournament_id > 0, "tournament_id must be non-zero");

        // Initial delay to allow game account propagation.
        tokio::time::sleep(tokio::time::Duration::from_secs(
            JOIN_GAME_INITIAL_DELAY_SECS,
        ))
        .await;

        let ix = instructions::build_join_game(game_id, tournament_id, self.keypair.as_ref());

        for attempt in 1..=JOIN_GAME_MAX_ATTEMPTS {
            match self.send_and_confirm(std::slice::from_ref(&ix)).await {
                Ok(sig) => {
                    tracing::info!(
                        game_id,
                        wallet = %self.keypair.pubkey(),
                        %sig,
                        "joined on-chain game as P2"
                    );
                    return Ok(sig);
                }
                Err(e) => {
                    let msg = format!("{e:#}");
                    if attempt < JOIN_GAME_MAX_ATTEMPTS && msg.contains(ACCOUNT_NOT_INITIALIZED) {
                        tracing::warn!(
                            game_id,
                            attempt,
                            "join_game: game account not yet visible — retrying in {JOIN_GAME_RETRY_DELAY_SECS}s"
                        );
                        tokio::time::sleep(tokio::time::Duration::from_secs(
                            JOIN_GAME_RETRY_DELAY_SECS,
                        ))
                        .await;
                    } else {
                        return Err(e.context("join_game failed"));
                    }
                }
            }
        }
        anyhow::bail!("join_game: gave up after {JOIN_GAME_MAX_ATTEMPTS} attempts")
    }

    /// Submit a commit-guess transaction on-chain.
    pub async fn commit_guess(&self, game_id: u64, commitment: [u8; 32]) -> Result<Signature> {
        anyhow::ensure!(game_id > 0, "game_id must be non-zero");

        let ix = instructions::build_commit_guess(game_id, commitment, self.keypair.as_ref());
        let sig = self.send_and_confirm(&[ix]).await?;

        tracing::info!(
            game_id,
            wallet = %self.keypair.pubkey(),
            %sig,
            "committed guess on-chain"
        );
        Ok(sig)
    }

    /// Submit a reveal-guess transaction on-chain.
    ///
    /// Reads the `GlobalConfig` account to determine the treasury address,
    /// then builds and submits the reveal instruction.
    #[allow(clippy::too_many_arguments)]
    pub async fn reveal_guess(
        &self,
        game_id: u64,
        tournament_id: u64,
        preimage: [u8; 32],
        r_matchup: Option<[u8; 32]>,
        player_one: Pubkey,
        player_two: Pubkey,
    ) -> Result<Signature> {
        anyhow::ensure!(game_id > 0, "game_id must be non-zero");
        anyhow::ensure!(tournament_id > 0, "tournament_id must be non-zero");

        // Read the global config to get the treasury address.
        let (global_config_pda, _) = pda::global_config_pda();
        let global_config_account = self
            .rpc
            .get_account(&global_config_pda)
            .await
            .context("failed to fetch GlobalConfig account")?;
        let global_config = GlobalConfig::try_deserialize(&mut global_config_account.data.as_ref())
            .context("failed to deserialize GlobalConfig")?;
        let treasury = global_config.treasury;

        let ix = instructions::build_reveal_guess(
            game_id,
            tournament_id,
            preimage,
            r_matchup,
            self.keypair.as_ref(),
            player_one,
            player_two,
            global_config_pda,
            treasury,
        );

        let sig = self.send_and_confirm(&[ix]).await?;

        tracing::info!(
            game_id,
            wallet = %self.keypair.pubkey(),
            %sig,
            "revealed guess on-chain"
        );
        Ok(sig)
    }

    /// Read and deserialize a game account by game ID.
    ///
    /// Returns `None` if the account does not exist.
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
                // AccountNotFound is expected for games that don't exist yet.
                if msg.contains("AccountNotFound") || msg.contains("could not find account") {
                    Ok(None)
                } else {
                    Err(anyhow::anyhow!(e).context("failed to fetch Game account"))
                }
            }
        }
    }

    /// Poll the game account until it reaches `target` state, then return it.
    ///
    /// Polls every 6 seconds for up to ~10 minutes (100 attempts).
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

    /// Submit a commit and return the preimage for later reveal.
    ///
    /// Combines `generate_commit_secret` with `commit_guess`. The caller
    /// must keep the returned preimage to call `reveal_guess` later.
    pub async fn submit_commit(&self, game_id: u64, guess: u8) -> Result<[u8; 32]> {
        anyhow::ensure!(game_id > 0, "game_id must be non-zero");
        anyhow::ensure!(guess <= 1, "guess must be 0 or 1");

        let (preimage, commitment) =
            crate::commit::generate_commit_secret(guess).map_err(|e| anyhow::anyhow!(e))?;
        self.commit_guess(game_id, commitment).await?;
        Ok(preimage)
    }

    /// Wait for both commits, then reveal and wait for resolution.
    ///
    /// Returns `(p1_guess, p2_guess)` from the resolved game account.
    pub async fn wait_and_reveal(
        &self,
        game_id: u64,
        tournament_id: u64,
        preimage: [u8; 32],
        r_matchup: Option<[u8; 32]>,
    ) -> Result<(u8, u8)> {
        let revealing = self
            .wait_for_game_state(game_id, GameState::Revealing)
            .await?;

        self.reveal_guess(
            game_id,
            tournament_id,
            preimage,
            r_matchup,
            revealing.player_one,
            revealing.player_two,
        )
        .await?;

        let resolved = self
            .wait_for_game_state(game_id, GameState::Resolved)
            .await?;
        Ok((resolved.p1_guess, resolved.p2_guess))
    }

    /// Create an on-chain game as Player 1, with matchmaker co-signature.
    ///
    /// The `cosign_fn` callback sends the serialized transaction message to the
    /// game-api `/games/cosign` endpoint and returns the matchmaker's ed25519
    /// signature. This keeps HTTP concerns out of the chain client.
    ///
    /// Returns `(game_id, tx_signature)`.
    pub async fn create_game<F, Fut>(
        &self,
        tournament_id: u64,
        stake_lamports: u64,
        matchup_commitment: [u8; 32],
        matchmaker: &Pubkey,
        cosign_fn: F,
    ) -> Result<(u64, Signature)>
    where
        F: FnOnce(Vec<u8>) -> Fut,
        Fut: std::future::Future<Output = Result<Signature>>,
    {
        anyhow::ensure!(tournament_id > 0, "tournament_id must be non-zero");

        // Read current game counter to derive the game PDA.
        let (counter_pda, _) = pda::game_counter_pda();
        let counter_data = self
            .rpc
            .get_account_data(&counter_pda)
            .await
            .context("failed to read game_counter")?;
        // GameCounter layout: 8-byte discriminator + 8-byte count + 1-byte bump
        anyhow::ensure!(counter_data.len() >= 16, "game_counter data too short");
        let game_counter_value =
            u64::from_le_bytes(counter_data[8..16].try_into().context("parse count")?);

        let ix = instructions::build_create_game(
            stake_lamports,
            matchup_commitment,
            tournament_id,
            game_counter_value,
            self.keypair.as_ref(),
            matchmaker,
        );

        // Build the transaction message (not yet signed).
        let blockhash = self
            .rpc
            .get_latest_blockhash()
            .await
            .context("get_latest_blockhash")?;
        let message = solana_sdk::message::Message::new_with_blockhash(
            &[ix],
            Some(&self.keypair.pubkey()),
            &blockhash,
        );
        let message_bytes = message.serialize();

        // Get matchmaker co-signature via callback.
        let matchmaker_sig = cosign_fn(message_bytes.clone()).await?;

        // Build the fully signed transaction.
        // Signers order: matchmaker (index in message), player (fee payer).
        let player_sig = self.keypair.sign_message(&message_bytes);
        let mut tx = Transaction::new_unsigned(message);

        // The message has two required signers. The fee payer (player) is
        // always index 0 in the account keys. The matchmaker is another signer.
        // We need to place signatures in the correct order.
        let num_signers = tx.message.header.num_required_signatures as usize;
        tx.signatures = vec![Signature::default(); num_signers];

        // Find the matchmaker's index in the account keys.
        for (i, key) in tx.message.account_keys.iter().enumerate() {
            if key == &self.keypair.pubkey() {
                tx.signatures[i] = player_sig;
            } else if key == matchmaker {
                tx.signatures[i] = matchmaker_sig;
            }
        }

        let sig = self
            .rpc
            .send_and_confirm_transaction(&tx)
            .await
            .context("send_and_confirm create_game")?;

        tracing::info!(
            tournament_id,
            game_id = game_counter_value,
            wallet = %self.keypair.pubkey(),
            %sig,
            "created game on-chain"
        );

        Ok((game_counter_value, sig))
    }

    // -- internal helpers -----------------------------------------------------

    /// Sign and submit a transaction, returning the signature.
    async fn send_and_confirm(
        &self,
        ixs: &[solana_sdk::instruction::Instruction],
    ) -> Result<Signature> {
        let blockhash = self
            .rpc
            .get_latest_blockhash()
            .await
            .context("get_latest_blockhash")?;
        let tx = Transaction::new_signed_with_payer(
            ixs,
            Some(&self.keypair.pubkey()),
            &[self.keypair.as_ref()],
            blockhash,
        );
        let sig = self
            .rpc
            .send_and_confirm_transaction(&tx)
            .await
            .context("send_and_confirm_transaction")?;
        Ok(sig)
    }
}
