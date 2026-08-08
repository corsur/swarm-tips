//! Transaction builder for on-chain game operations.
//!
//! `GameTxBuilder` constructs unsigned Solana transactions for each game
//! instruction. It holds only the player's public key — no private key
//! ever touches this code. Callers sign transactions locally and submit
//! via `submit_signed`.

use anchor_lang::AccountDeserialize;
use anyhow::{Context, Result};
use coordination::state::{Game, GameCounter, GameState, GlobalConfig};
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig, message::Message, pubkey::Pubkey, signature::Signature,
    transaction::Transaction,
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
    /// Used by grok-agent (Rust-native signing) which deserializes as Message.
    pub message: Vec<u8>,
    /// Base64-encoded message for transport over MCP/JSON.
    pub message_b64: String,
    /// Base64-encoded full Transaction (with empty signature slots).
    /// Used by MCP agents (TypeScript) — `Transaction.from()` preserves exact
    /// message bytes, avoiding re-serialization that breaks cosign signatures.
    pub transaction_b64: String,
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
            rpc: RpcClient::new_with_commitment(rpc_url.to_string(), CommitmentConfig::confirmed()),
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

    /// Read the next game id from the `GameCounter` PDA.
    ///
    /// Uses Anchor's `try_deserialize` rather than a raw byte-offset read, so
    /// the 8-byte discriminator is checked: a wrong account at that address
    /// errors instead of being parsed as a plausible-looking counter value.
    /// This matches how `build_reveal_guess` reads `GlobalConfig`.
    pub async fn read_game_counter(&self) -> Result<u64> {
        let (counter_pda, _) = pda::game_counter_pda();
        let data = self
            .rpc
            .get_account_data(&counter_pda)
            .await
            .context("failed to read game_counter")?;

        let counter = GameCounter::try_deserialize(&mut data.as_ref())
            .context("failed to deserialize game_counter")?;

        Ok(counter.count)
    }

    /// The LIVE stake, read from `GlobalConfig` on the cluster this client is
    /// pointed at.
    ///
    /// This is the single source of truth for what a game costs. It is not
    /// `DEFAULT_STAKE_LAMPORTS` — that constant is only the value
    /// `initialize_config` writes at genesis, and mainnet has since been
    /// re-pegged to 68,482,585 while devnet stayed at 50,000,000. Reading the
    /// account is also what makes a re-peg an instruction rather than a
    /// redeploy of every client.
    pub async fn stake_lamports(&self) -> Result<u64> {
        let (global_config_pda, _) = pda::global_config_pda();
        let data = self
            .rpc
            .get_account_data(&global_config_pda)
            .await
            .context("failed to read GlobalConfig")?;
        let global_config = GlobalConfig::try_deserialize(&mut data.as_ref())
            .context("failed to deserialize GlobalConfig")?;
        anyhow::ensure!(
            global_config.stake_lamports > 0,
            "GlobalConfig.stake_lamports is zero"
        );
        Ok(global_config.stake_lamports)
    }

    /// Build an unsigned `CreateGame` transaction message.
    ///
    /// Returns the message bytes for the player to sign. The matchmaker
    /// co-signature must be obtained separately (via game-api `/games/cosign`).
    /// The caller assembles the final transaction with both signatures.
    ///
    /// TAKES NO STAKE ARGUMENT ON PURPOSE. It used to, and the caller
    /// (mcp-server) passed `DEFAULT_STAKE_LAMPORTS` — so when mainnet was
    /// re-pegged to 68,482,585 the program's
    /// `require!(stake_lamports == expected_stake)` rejected every game and
    /// mainnet went down from 2026-07-30. A parameter the caller can get wrong
    /// IS a second source of truth; removing it makes the wrong call
    /// unwriteable rather than merely discouraged.
    pub async fn build_create_game(
        &self,
        tournament_id: u64,
        matchup_commitment: [u8; 32],
        matchmaker: &Pubkey,
    ) -> Result<UnsignedTx> {
        anyhow::ensure!(tournament_id > 0, "tournament_id must be non-zero");

        let stake_lamports = self.stake_lamports().await?;
        let game_counter_value = self.read_game_counter().await?;

        let ix = instructions::build_create_game(
            stake_lamports,
            matchup_commitment,
            tournament_id,
            game_counter_value,
            &self.player,
            matchmaker,
        );

        self.build_unsigned(&[ix]).await
    }

    /// Build the unsigned `create_xmatch` transaction (the Solana leg of a
    /// cross-chain match). Matchmaker-cosigned + player-funded, like
    /// `create_game`; `match_id` seeds the xmatch escrow PDA. No on-chain read
    /// is needed — the match id is supplied by the operator, not derived from a
    /// counter.
    pub async fn build_create_xmatch(
        &self,
        match_id: [u8; 32],
        args: coordination::instructions::xchain::CreateXMatchArgs,
        matchmaker: &Pubkey,
    ) -> Result<UnsignedTx> {
        anyhow::ensure!(args.tournament_id > 0, "tournament_id must be non-zero");
        anyhow::ensure!(args.stake_lamports > 0, "stake_lamports must be non-zero");
        let ix = instructions::build_create_xmatch(match_id, args, &self.player, matchmaker);
        self.build_unsigned(&[ix]).await
    }

    /// Build the unsigned `initialize_xpool` transaction — one-time setup of the
    /// operator float pool. `self.player` is the authority (fee payer + signer)
    /// and MUST equal `global_config.authority`. `operator` is the Solana key
    /// allowed to lock tranches; `operator_signer` is the 20-byte secp256k1
    /// eth-address of the off-chain cosigner.
    pub async fn build_initialize_xpool(
        &self,
        operator: &Pubkey,
        operator_signer: [u8; 20],
        max_tranche_lamports: u64,
        max_claim_window_secs: u32,
        skew_margin_secs: u32,
    ) -> Result<UnsignedTx> {
        anyhow::ensure!(skew_margin_secs > 0, "skew_margin_secs must be non-zero");
        anyhow::ensure!(
            operator_signer != [0u8; 20],
            "operator_signer must not be all zeros"
        );
        let ix = instructions::build_initialize_xpool(
            operator,
            operator_signer,
            max_tranche_lamports,
            max_claim_window_secs,
            skew_margin_secs,
            &self.player,
        );
        self.build_unsigned(&[ix]).await
    }

    /// Build the unsigned `xpool_deposit` transaction — credit `amount` lamports
    /// from `self.player` (the funder + fee payer) into the pool's free balance.
    pub async fn build_xpool_deposit(&self, amount: u64) -> Result<UnsignedTx> {
        anyhow::ensure!(amount > 0, "amount must be non-zero");
        let ix = instructions::build_xpool_deposit(amount, &self.player);
        self.build_unsigned(&[ix]).await
    }

    /// Build the unsigned permissionless `lock_xtranche` transaction — binds the
    /// cert's leg-A tranche from the pool to a funded match, transitioning it to
    /// `Locked` (the precondition for settle). Authorization is `operator_sig`
    /// (the operator's match-live signature over the cert); `self.player` is the
    /// permissionless fee payer — it need NOT be the operator.
    pub async fn build_lock_xtranche(
        &self,
        cert: coordination::cert::MatchLiveCertArg,
        operator_sig: [u8; 65],
    ) -> Result<UnsignedTx> {
        let ix = instructions::build_lock_xtranche(cert, operator_sig, &self.player);
        self.build_unsigned(&[ix]).await
    }

    /// Build the unsigned `refund_xmatch_timeout` transaction (Solana leg) —
    /// permissionless; the player (`self.player`) is the fee payer + refund
    /// recipient. Reclaims the stake and releases any locked tranche after the
    /// claim window.
    pub async fn build_refund_xmatch_timeout(&self, match_id: [u8; 32]) -> Result<UnsignedTx> {
        let ix = instructions::build_refund_xmatch_timeout(match_id, &self.player);
        self.build_unsigned(&[ix]).await
    }

    /// Build the unsigned `refund_xmatch_nocert` transaction (Solana leg) —
    /// permissionless; refunds the player when a funded match never had a
    /// certificate signed.
    pub async fn build_refund_xmatch_nocert(&self, match_id: [u8; 32]) -> Result<UnsignedTx> {
        let ix = instructions::build_refund_xmatch_nocert(match_id, &self.player);
        self.build_unsigned(&[ix]).await
    }

    /// Build the unsigned `close_xmatch` transaction (Solana leg) —
    /// permissionless rent reclaim once the match is terminal. The fee payer
    /// (`self.player`) submits; rent returns to the recorded match player.
    /// Cranked after settle/refund so per-match rent doesn't leak each game.
    pub async fn build_close_xmatch(&self, match_id: [u8; 32]) -> Result<UnsignedTx> {
        let ix = instructions::build_close_xmatch(match_id, &self.player);
        self.build_unsigned(&[ix]).await
    }

    // -- Submit ----------------------------------------------------------------

    /// Wallet balance for TELEMETRY fields only. A failed read logs the error
    /// and yields None (rendered as absent), never a fake 0 — a zero balance
    /// and a failed RPC read are very different diagnostics.
    async fn balance_for_log(&self, wallet: &solana_sdk::pubkey::Pubkey) -> Option<u64> {
        match self.rpc.get_balance(wallet).await {
            Ok(b) => Some(b),
            Err(e) => {
                tracing::warn!(wallet = %wallet, error = %e, "balance read for telemetry failed");
                None
            }
        }
    }

    /// Submit a pre-signed transaction to the network.
    ///
    /// The transaction must be fully signed (all required signers) and
    /// serialized as bincode bytes.
    pub async fn submit_signed(&self, signed_tx_bytes: &[u8]) -> Result<Signature> {
        let tx: Transaction = bincode::deserialize(signed_tx_bytes)
            .context("failed to deserialize signed transaction")?;

        let wallet = self.player;
        let balance_before = self.balance_for_log(&wallet).await;

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
                    let balance_after = self.balance_for_log(&wallet).await;
                    let cost_lamports = match (balance_before, balance_after) {
                        (Some(b), Some(a)) => Some(b.saturating_sub(a)),
                        _ => None,
                    };
                    tracing::info!(
                        wallet = %wallet,
                        %sig,
                        balance_before = ?balance_before,
                        balance_after = ?balance_after,
                        cost_lamports = ?cost_lamports,
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
                        let balance = self.balance_for_log(&wallet).await;
                        tracing::error!(
                            wallet = %wallet,
                            balance_lamports = ?balance,
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

        let balance = self.balance_for_log(&wallet).await;
        tracing::error!(
            wallet = %wallet,
            balance_lamports = ?balance,
            attempts = 3,
            "transaction failed after all retries"
        );
        match last_err {
            Some(e) => Err(e).context("send_and_confirm_transaction: all retries exhausted"),
            None => Err(anyhow::anyhow!(
                "send_and_confirm_transaction: retry loop exited without recording an error"
            )),
        }
    }

    // -- Read-only operations --------------------------------------------------

    /// Read and deserialize a game account by game ID, at the builder's default
    /// `confirmed` commitment.
    pub async fn read_game(&self, game_id: u64) -> Result<Option<Game>> {
        self.read_game_at(game_id, CommitmentConfig::confirmed())
            .await
    }

    /// Read a game at the FRESHEST available state.
    ///
    /// Use this only where reading one slot stale makes the caller build a
    /// transaction the program will reject.
    ///
    /// The reveal path is exactly that case. `reveal_guess` lets only the FIRST
    /// revealer pass `r_matchup`; the second must pass `None` or the program
    /// returns `RMatchupMismatch` (6032). The server decides which it is by
    /// reading `game.matchup_type`. Under `confirmed`, an opponent's reveal that
    /// is already PROCESSED but not yet CONFIRMED is invisible, so the server
    /// still believes it is first, attaches `r_matchup`, and the transaction is
    /// rejected on arrival. Observed live: all four homogeneous cells — where
    /// both reveals land near-simultaneously — burned 17-18 minutes in a retry
    /// loop, while heterogeneous cells (staggered reveals) passed in 2.3.
    ///
    /// `processed` can be rolled back, which is why this is NOT the default.
    /// Here that risk is the safe direction: a rolled-back read makes us omit
    /// `r_matchup` when we were in fact first, and the program answers with a
    /// clean `InvalidGameState` that the caller retries — versus the stale
    /// direction, which fails on every retry until confirmation catches up.
    pub async fn read_game_freshest(&self, game_id: u64) -> Result<Option<Game>> {
        self.read_game_at(game_id, CommitmentConfig::processed())
            .await
    }

    async fn read_game_at(
        &self,
        game_id: u64,
        commitment: CommitmentConfig,
    ) -> Result<Option<Game>> {
        let (pda, _) = pda::game_pda(game_id);
        match self.rpc.get_account_with_commitment(&pda, commitment).await {
            Ok(response) => match response.value {
                Some(account) => {
                    let game = Game::try_deserialize(&mut account.data.as_ref())
                        .context("failed to deserialize Game")?;
                    Ok(Some(game))
                }
                None => Ok(None),
            },
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

        // Full Transaction with empty signature slots — used by TypeScript agents
        // to avoid Message recompilation that breaks cosign signatures.
        let tx = Transaction::new_unsigned(message.clone());
        let tx_bytes =
            bincode::serialize(&tx).context("failed to serialize unsigned transaction")?;
        let transaction_b64 = base64::engine::general_purpose::STANDARD.encode(&tx_bytes);

        Ok(UnsignedTx {
            message: message_bytes,
            message_b64,
            transaction_b64,
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

        let tx = Transaction::new_unsigned(message.clone());
        let tx_bytes = bincode::serialize(&tx).unwrap();
        let transaction_b64 = base64::engine::general_purpose::STANDARD.encode(&tx_bytes);

        UnsignedTx {
            message: message_bytes,
            message_b64,
            transaction_b64,
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
