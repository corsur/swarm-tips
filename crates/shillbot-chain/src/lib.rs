//! Canonical, side-effect-free inspection and validation for Shillbot transactions.
//!
//! This crate deliberately does not sign or broadcast. It gives servers and local
//! clients the same semantic boundary: provenance does not matter, but the message
//! must contain exactly the lifecycle action the caller claims it contains.

use anchor_lang::InstructionData;
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    message::VersionedMessage,
    pubkey::Pubkey,
    sysvar,
    transaction::VersionedTransaction,
};
use std::str::FromStr;

const COMPUTE_BUDGET_PROGRAM: &str = "ComputeBudget111111111111111111111111111111";
const SECP256K1_PROGRAM: &str = "KeccakSecp256k11111111111111111111111111111";
const SWITCHBOARD_MAINNET: &str = "SBondMDrcV3K4kxZR1HNVT7osZxAHVHgYXL5Ze1oMUv";
const SWITCHBOARD_DEVNET: &str = "Aio4gaXjXzJNVLtzwtNVmSqGKpANtXhybbkhtAC94ji2";

pub fn global_state_pda() -> Pubkey {
    Pubkey::find_program_address(&[b"shillbot_global"], &shillbot::ID).0
}

pub fn task_pda(nonce: u64, client: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[b"task", &nonce.to_le_bytes(), client.as_ref()],
        &shillbot::ID,
    )
    .0
}

pub fn client_state_pda(client: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"client_state", client.as_ref()], &shillbot::ID).0
}

pub fn agent_state_pda(agent: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"agent_state", agent.as_ref()], &shillbot::ID).0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateTaskArgs {
    pub nonce: u64,
    pub escrow_lamports: u64,
    pub content_hash: [u8; 32],
    pub deadline: i64,
    pub submit_margin: i64,
    pub claim_buffer: i64,
    pub platform: u8,
    pub attestation_delay_override: u32,
    pub challenge_window_override: u32,
    pub verification_timeout_override: u32,
    pub requires_approval: bool,
    pub verification_kind: u8,
}

pub fn create_task_instruction(client: Pubkey, args: CreateTaskArgs) -> Instruction {
    Instruction {
        program_id: shillbot::ID,
        accounts: vec![
            AccountMeta::new(global_state_pda(), false),
            AccountMeta::new(task_pda(args.nonce, &client), false),
            AccountMeta::new(client_state_pda(&client), false),
            AccountMeta::new(client, true),
            AccountMeta::new_readonly(sysvar::slot_hashes::id(), false),
            AccountMeta::new_readonly(solana_system_interface::program::ID, false),
        ],
        data: shillbot::instruction::CreateTask {
            nonce: args.nonce,
            escrow_lamports: args.escrow_lamports,
            content_hash: args.content_hash,
            deadline: args.deadline,
            submit_margin: args.submit_margin,
            claim_buffer: args.claim_buffer,
            platform: args.platform,
            attestation_delay_override: args.attestation_delay_override,
            challenge_window_override: args.challenge_window_override,
            verification_timeout_override: args.verification_timeout_override,
            requires_approval: args.requires_approval,
            verification_kind: args.verification_kind,
        }
        .data(),
    }
}

pub fn claim_task_instruction(agent: Pubkey, task: Pubkey) -> Instruction {
    Instruction {
        program_id: shillbot::ID,
        accounts: vec![
            AccountMeta::new(task, false),
            AccountMeta::new_readonly(global_state_pda(), false),
            AccountMeta::new(agent_state_pda(&agent), false),
            AccountMeta::new(agent, true),
            AccountMeta::new_readonly(solana_system_interface::program::ID, false),
        ],
        data: shillbot::instruction::ClaimTask {}.data(),
    }
}

pub fn submit_work_instruction(agent: Pubkey, task: Pubkey, content_id: Vec<u8>) -> Instruction {
    Instruction {
        program_id: shillbot::ID,
        accounts: vec![
            AccountMeta::new(task, false),
            AccountMeta::new_readonly(global_state_pda(), false),
            AccountMeta::new(agent_state_pda(&agent), false),
            AccountMeta::new_readonly(agent, true),
        ],
        data: shillbot::instruction::SubmitWork { content_id }.data(),
    }
}

pub fn approve_task_instruction(client: Pubkey, task: Pubkey) -> Instruction {
    Instruction {
        program_id: shillbot::ID,
        accounts: vec![
            AccountMeta::new(task, false),
            AccountMeta::new_readonly(client, true),
        ],
        data: shillbot::instruction::ApproveTask {}.data(),
    }
}

pub fn verify_task_instruction(
    task: Pubkey,
    switchboard_feed: Pubkey,
    composite_score: u64,
    verification_hash: [u8; 32],
) -> Instruction {
    Instruction {
        program_id: shillbot::ID,
        accounts: vec![
            AccountMeta::new(task, false),
            AccountMeta::new_readonly(global_state_pda(), false),
            AccountMeta::new_readonly(switchboard_feed, false),
        ],
        data: shillbot::instruction::VerifyTask {
            composite_score,
            verification_hash,
        }
        .data(),
    }
}

pub fn finalize_task_instruction(
    task: Pubkey,
    agent: Pubkey,
    client: Pubkey,
    treasury: Pubkey,
) -> Instruction {
    Instruction {
        program_id: shillbot::ID,
        accounts: vec![
            AccountMeta::new(task, false),
            AccountMeta::new_readonly(global_state_pda(), false),
            AccountMeta::new(agent, false),
            AccountMeta::new(client, false),
            AccountMeta::new(treasury, false),
        ],
        data: shillbot::instruction::FinalizeTask {}.data(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Create,
    Claim,
    Submit,
    Approve,
    Verify,
    Finalize,
}

impl Action {
    pub const fn instruction_name(self) -> &'static str {
        match self {
            Self::Create => "create_task",
            Self::Claim => "claim_task",
            Self::Submit => "submit_work",
            Self::Approve => "approve_task",
            Self::Verify => "verify_task",
            Self::Finalize => "finalize_task",
        }
    }

    fn task_account_position(self) -> usize {
        match self {
            Self::Create => 1,
            Self::Claim | Self::Submit | Self::Approve | Self::Verify | Self::Finalize => 0,
        }
    }

    fn authority_account_position(self) -> Option<usize> {
        match self {
            Self::Create | Self::Claim | Self::Submit => Some(3),
            Self::Approve => Some(1),
            Self::Verify | Self::Finalize => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationRequest {
    pub action: Action,
    pub network: String,
    pub wallet: String,
    pub task_pda: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub escrow_lamports: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_id: Option<String>,
    /// Sponsor fee payer allowed for claim/submit. When absent, the registered
    /// wallet must be the fee payer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sponsor: Option<String>,
    /// Exact ordered lifecycle accounts, when the caller has the persisted
    /// task/campaign model needed to derive them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_accounts: Option<Vec<String>>,
    /// Exact lifecycle instruction data as standard base64. This binds create
    /// terms and verify scores/hashes without teaching the validator about a
    /// database schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_data: Option<String>,
    /// The only payout recipient accepted for an optional sponsored-claim
    /// `set_payout_to` companion. Omit to forbid payout routing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_payout_to: Option<String>,
    /// Require the authorized payout route to be present (rather than merely
    /// allowing it) when an open sponsor advance must be repaid.
    #[serde(default)]
    pub require_payout_routing: bool,
    /// Expected feed for the Switchboard crank bundled with verify.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub switchboard_feed: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Inspection {
    pub version: String,
    pub action: Action,
    pub program_id: String,
    pub network: String,
    pub fee_payer: String,
    pub required_signers: Vec<String>,
    pub accounts: Vec<String>,
    pub movements: Vec<Movement>,
    pub task_pda: String,
    pub instruction_count: usize,
    pub instruction_index: usize,
    pub intent_digest: String,
    pub risk: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Movement {
    pub asset: String,
    pub from: String,
    pub to: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<u64>,
    pub condition: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ValidationError {
    #[error("invalid base64 transaction: {0}")]
    Base64(String),
    #[error("invalid Solana transaction: {0}")]
    Transaction(String),
    #[error("invalid validation request: {0}")]
    Request(String),
    #[error("transaction rejected: {0}")]
    Rejected(String),
}

fn discriminator(name: &str) -> [u8; 8] {
    let hash = Sha256::digest(format!("global:{name}").as_bytes());
    let mut result = [0_u8; 8];
    result.copy_from_slice(&hash[..8]);
    result
}

fn static_keys(message: &VersionedMessage) -> &[Pubkey] {
    match message {
        VersionedMessage::Legacy(message) => &message.account_keys,
        VersionedMessage::V0(message) => &message.account_keys,
    }
}

fn instructions(message: &VersionedMessage) -> &[solana_sdk::instruction::CompiledInstruction] {
    match message {
        VersionedMessage::Legacy(message) => &message.instructions,
        VersionedMessage::V0(message) => &message.instructions,
    }
}

fn required_signatures(message: &VersionedMessage) -> usize {
    match message {
        VersionedMessage::Legacy(message) => usize::from(message.header.num_required_signatures),
        VersionedMessage::V0(message) => usize::from(message.header.num_required_signatures),
    }
}

fn ensure_no_lookups(message: &VersionedMessage) -> Result<(), ValidationError> {
    if matches!(message, VersionedMessage::V0(m) if !m.address_table_lookups.is_empty()) {
        return Err(ValidationError::Rejected(
            "address lookup tables are not allowed in Shillbot lifecycle transactions".into(),
        ));
    }
    Ok(())
}

pub fn decode_base64(value: &str) -> Result<VersionedTransaction, ValidationError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|error| ValidationError::Base64(error.to_string()))?;
    bincode::deserialize(&bytes).map_err(|error| ValidationError::Transaction(error.to_string()))
}

pub fn validate_base64(
    value: &str,
    expected: &ValidationRequest,
) -> Result<Inspection, ValidationError> {
    let tx = decode_base64(value)?;
    validate(&tx, expected)
}

pub fn fee_payer_base64(value: &str) -> Result<String, ValidationError> {
    let tx = decode_base64(value)?;
    let keys = static_keys(&tx.message);
    keys.first()
        .map(ToString::to_string)
        .ok_or_else(|| ValidationError::Rejected("transaction has no fee payer".into()))
}

pub fn validate_signed_base64(
    value: &str,
    expected: &ValidationRequest,
) -> Result<Inspection, ValidationError> {
    let tx = decode_base64(value)?;
    let inspection = validate(&tx, expected)?;
    tx.verify_and_hash_message()
        .map_err(|_| ValidationError::Rejected("transaction signatures do not verify".into()))?;
    Ok(inspection)
}

fn validate_compute_budget(data: &[u8]) -> Result<(), ValidationError> {
    match data.first().copied() {
        Some(2) if data.len() == 5 => {
            let units = u32::from_le_bytes(data[1..5].try_into().expect("length checked"));
            if units == 0 || units > 1_400_000 {
                return Err(ValidationError::Rejected(
                    "compute-unit limit is outside the accepted bound".into(),
                ));
            }
        }
        Some(3) if data.len() == 9 => {
            let price = u64::from_le_bytes(data[1..9].try_into().expect("length checked"));
            if price > 100_000 {
                return Err(ValidationError::Rejected(
                    "compute-unit price is outside the accepted bound".into(),
                ));
            }
        }
        _ => {
            return Err(ValidationError::Rejected(
                "unsupported compute-budget instruction".into(),
            ));
        }
    }
    Ok(())
}

pub fn validate(
    tx: &VersionedTransaction,
    expected: &ValidationRequest,
) -> Result<Inspection, ValidationError> {
    ensure_no_lookups(&tx.message)?;
    if expected.network != "mainnet" && expected.network != "devnet" {
        return Err(ValidationError::Request(
            "network must be mainnet or devnet".into(),
        ));
    }
    let wallet = Pubkey::from_str(&expected.wallet)
        .map_err(|_| ValidationError::Request("wallet is not a Solana pubkey".into()))?;
    let task = Pubkey::from_str(&expected.task_pda)
        .map_err(|_| ValidationError::Request("task_pda is not a Solana pubkey".into()))?;
    let sponsor = expected
        .sponsor
        .as_deref()
        .map(Pubkey::from_str)
        .transpose()
        .map_err(|_| ValidationError::Request("sponsor is not a Solana pubkey".into()))?;
    let allowed_payout_to = expected
        .allowed_payout_to
        .as_deref()
        .map(Pubkey::from_str)
        .transpose()
        .map_err(|_| ValidationError::Request("allowed_payout_to is not a Solana pubkey".into()))?;
    let expected_feed = expected
        .switchboard_feed
        .as_deref()
        .map(Pubkey::from_str)
        .transpose()
        .map_err(|_| ValidationError::Request("switchboard_feed is not a Solana pubkey".into()))?;

    let keys = static_keys(&tx.message);
    let required = required_signatures(&tx.message);
    if keys.is_empty() || required == 0 || required > keys.len() {
        return Err(ValidationError::Rejected(
            "transaction has no valid fee payer".into(),
        ));
    }
    if tx.signatures.len() != required {
        return Err(ValidationError::Rejected(
            "transaction signature slots do not match required signers".into(),
        ));
    }
    let fee_payer = keys[0];
    if sponsor.is_some() && !matches!(expected.action, Action::Claim | Action::Submit) {
        return Err(ValidationError::Request(
            "sponsorship is only valid for claim and submit".into(),
        ));
    }
    if fee_payer != sponsor.unwrap_or(wallet) {
        return Err(ValidationError::Rejected("unexpected fee payer".into()));
    }

    let shillbot_program = shillbot::ID;
    let expected_disc = discriminator(expected.action.instruction_name());
    let set_payout_disc = discriminator("set_payout_to");
    let switchboard_submit_disc = discriminator("pull_feed_submit_response_consensus");
    let compute_program = Pubkey::from_str(COMPUTE_BUDGET_PROGRAM).expect("valid constant");
    let secp_program = Pubkey::from_str(SECP256K1_PROGRAM).expect("valid constant");
    let switchboard_program = Pubkey::from_str(if expected.network == "devnet" {
        SWITCHBOARD_DEVNET
    } else {
        SWITCHBOARD_MAINNET
    })
    .expect("valid constant");

    let mut matched = None;
    let mut payout_routing = 0_usize;
    let mut secp_indices = Vec::new();
    let mut switchboard_indices = Vec::new();
    let mut compute_kinds = [false; 2];
    for (position, ix) in instructions(&tx.message).iter().enumerate() {
        let program = keys.get(usize::from(ix.program_id_index)).ok_or_else(|| {
            ValidationError::Rejected("instruction program index is invalid".into())
        })?;
        if *program == shillbot_program {
            let disc = ix.data.get(..8).ok_or_else(|| {
                ValidationError::Rejected("Shillbot instruction has no discriminator".into())
            })?;
            if disc == expected_disc {
                if matched.is_some() {
                    return Err(ValidationError::Rejected(
                        "transaction repeats the lifecycle instruction".into(),
                    ));
                }
                matched = Some(ix);
            } else if expected.action == Action::Claim && disc == set_payout_disc {
                payout_routing = payout_routing.saturating_add(1);
                if payout_routing > 1 || allowed_payout_to.is_none() {
                    return Err(ValidationError::Rejected(
                        "unexpected payout-routing instruction".into(),
                    ));
                }
                if ix.accounts.len() != 2
                    || keys.get(usize::from(ix.accounts[0])) != Some(&task)
                    || keys.get(usize::from(ix.accounts[1])) != Some(&wallet)
                    || ix.data.get(8..40)
                        != allowed_payout_to
                            .map(|p| p.to_bytes())
                            .as_ref()
                            .map(|p| p.as_slice())
                {
                    return Err(ValidationError::Rejected(
                        "payout routing differs from the authorized sponsor route".into(),
                    ));
                }
            } else {
                return Err(ValidationError::Rejected(
                    "unexpected Shillbot instruction".into(),
                ));
            }
        } else {
            let allowed = if *program == compute_program {
                validate_compute_budget(&ix.data)?;
                let kind = usize::from(ix.data[0] - 2);
                if compute_kinds[kind] {
                    return Err(ValidationError::Rejected(
                        "duplicate compute-budget instruction".into(),
                    ));
                }
                compute_kinds[kind] = true;
                true
            } else if expected.action == Action::Verify && *program == secp_program {
                if ix.data.first().copied().unwrap_or(0) == 0 {
                    return Err(ValidationError::Rejected(
                        "secp256k1 verification contains no signatures".into(),
                    ));
                }
                secp_indices.push(position);
                true
            } else if expected.action == Action::Verify && *program == switchboard_program {
                if ix.data.get(..8) != Some(switchboard_submit_disc.as_slice()) {
                    return Err(ValidationError::Rejected(
                        "unexpected Switchboard instruction".into(),
                    ));
                }
                switchboard_indices.push(position);
                true
            } else {
                false
            };
            if !allowed {
                return Err(ValidationError::Rejected(format!(
                    "unexpected companion program {program}"
                )));
            }
        }
    }

    let lifecycle = matched.ok_or_else(|| {
        ValidationError::Rejected(format!(
            "missing {} instruction",
            expected.action.instruction_name()
        ))
    })?;
    if expected.require_payout_routing && payout_routing != 1 {
        return Err(ValidationError::Rejected(
            "required sponsor payout routing is missing".into(),
        ));
    }
    if expected.action == Action::Verify {
        if secp_indices.len() != 1 || switchboard_indices.len() != 1 {
            return Err(ValidationError::Rejected(
                "verify requires exactly one secp256k1 and one Switchboard instruction".into(),
            ));
        }
        let lifecycle_index = instructions(&tx.message)
            .iter()
            .position(|candidate| std::ptr::eq(candidate, lifecycle))
            .expect("matched instruction belongs to message");
        if secp_indices[0].saturating_add(1) != switchboard_indices[0]
            || switchboard_indices[0] >= lifecycle_index
        {
            return Err(ValidationError::Rejected(
                "Switchboard verification instructions have an unsafe order".into(),
            ));
        }
        if let Some(feed) = expected_feed {
            let switchboard_ix = &instructions(&tx.message)[switchboard_indices[0]];
            let includes_feed = switchboard_ix.accounts.iter().any(|index| {
                keys.get(usize::from(*index))
                    .is_some_and(|key| *key == feed)
            });
            if !includes_feed {
                return Err(ValidationError::Rejected(
                    "Switchboard instruction targets a different feed".into(),
                ));
            }
        }
    } else if !secp_indices.is_empty() || !switchboard_indices.is_empty() {
        return Err(ValidationError::Rejected(
            "oracle instructions are only allowed for verify".into(),
        ));
    }
    let account_key = |position: usize| -> Result<Pubkey, ValidationError> {
        let index = lifecycle.accounts.get(position).ok_or_else(|| {
            ValidationError::Rejected("lifecycle instruction has too few accounts".into())
        })?;
        keys.get(usize::from(*index))
            .copied()
            .ok_or_else(|| ValidationError::Rejected("lifecycle account index is invalid".into()))
    };
    if account_key(expected.action.task_account_position())? != task {
        return Err(ValidationError::Rejected(
            "transaction targets a different task".into(),
        ));
    }
    if let Some(position) = expected.action.authority_account_position() {
        if account_key(position)? != wallet {
            return Err(ValidationError::Rejected(
                "transaction authority differs from wallet".into(),
            ));
        }
    } else if !keys[..required].contains(&wallet) {
        return Err(ValidationError::Rejected(
            "registered wallet is not a required signer".into(),
        ));
    }

    let lifecycle_accounts: Vec<String> = lifecycle
        .accounts
        .iter()
        .map(|index| {
            keys.get(usize::from(*index))
                .map(ToString::to_string)
                .ok_or_else(|| {
                    ValidationError::Rejected("lifecycle account index is invalid".into())
                })
        })
        .collect::<Result<_, _>>()?;
    if let Some(expected_accounts) = expected.expected_accounts.as_ref() {
        if lifecycle_accounts != *expected_accounts {
            return Err(ValidationError::Rejected(
                "lifecycle accounts differ from intent".into(),
            ));
        }
    }
    if let Some(expected_data) = expected.expected_data.as_deref() {
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(expected_data)
            .map_err(|error| {
                ValidationError::Request(format!("expected_data is invalid base64: {error}"))
            })?;
        if lifecycle.data != decoded {
            return Err(ValidationError::Rejected(
                "lifecycle arguments differ from intent".into(),
            ));
        }
    }

    if expected.action == Action::Create {
        if let Some(amount) = expected.escrow_lamports {
            let encoded = lifecycle.data.get(16..24).ok_or_else(|| {
                ValidationError::Rejected("create instruction data is truncated".into())
            })?;
            if encoded != amount.to_le_bytes() {
                return Err(ValidationError::Rejected(
                    "escrow amount differs from intent".into(),
                ));
            }
        }
    }
    if expected.action == Action::Submit {
        if let Some(content_id) = expected.content_id.as_deref() {
            let raw = lifecycle.data.get(8..).ok_or_else(|| {
                ValidationError::Rejected("submit instruction data is truncated".into())
            })?;
            let len_bytes: [u8; 4] = raw
                .get(..4)
                .ok_or_else(|| ValidationError::Rejected("content id length is missing".into()))?
                .try_into()
                .map_err(|_| ValidationError::Rejected("content id length is invalid".into()))?;
            let len = usize::try_from(u32::from_le_bytes(len_bytes))
                .map_err(|_| ValidationError::Rejected("content id is too large".into()))?;
            if raw.get(4..4 + len) != Some(content_id.as_bytes()) {
                return Err(ValidationError::Rejected(
                    "content id differs from intent".into(),
                ));
            }
        }
    }

    let intent_json = serde_json::to_vec(&("swarm.shillbot.transaction-intent/v1", expected))
        .map_err(|error| ValidationError::Transaction(error.to_string()))?;
    let digest = Sha256::digest(intent_json);
    Ok(Inspection {
        version: "swarm.shillbot.transaction-intent/v1".into(),
        action: expected.action,
        program_id: shillbot_program.to_string(),
        network: expected.network.clone(),
        fee_payer: fee_payer.to_string(),
        required_signers: keys[..required].iter().map(ToString::to_string).collect(),
        accounts: lifecycle_accounts,
        movements: match expected.action {
            Action::Create => vec![Movement {
                asset: "SOL".into(),
                from: wallet.to_string(),
                to: task.to_string(),
                amount: expected.escrow_lamports,
                condition: "escrow deposit".into(),
            }],
            Action::Finalize => vec![Movement {
                asset: "SOL".into(),
                from: task.to_string(),
                to: "task agent, protocol treasury, and rent recipient".into(),
                amount: None,
                condition: "program-calculated settlement".into(),
            }],
            Action::Approve => vec![Movement {
                asset: "escrow control".into(),
                from: task.to_string(),
                to: task.to_string(),
                amount: None,
                condition: "authorizes verification; does not transfer escrow immediately".into(),
            }],
            Action::Claim | Action::Submit | Action::Verify => Vec::new(),
        },
        task_pda: task.to_string(),
        instruction_count: instructions(&tx.message).len(),
        instruction_index: instructions(&tx.message)
            .iter()
            .position(|candidate| std::ptr::eq(candidate, lifecycle))
            .expect("matched instruction belongs to message"),
        intent_digest: hex::encode(digest),
        risk: match expected.action {
            Action::Create => "spend",
            Action::Approve => "escrow_control",
            Action::Claim | Action::Submit | Action::Verify | Action::Finalize => "earn",
        }
        .into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::{
        hash::Hash,
        instruction::{AccountMeta, Instruction},
        message::Message,
        signature::{Keypair, Signer},
        transaction::Transaction,
    };

    fn test_tx(action: Action, wallet: &Pubkey, task: &Pubkey) -> VersionedTransaction {
        let (accounts, data) = match action {
            Action::Claim => (
                vec![
                    AccountMeta::new(*task, false),
                    AccountMeta::new_readonly(Pubkey::new_unique(), false),
                    AccountMeta::new(Pubkey::new_unique(), false),
                    AccountMeta::new(*wallet, true),
                ],
                discriminator("claim_task").to_vec(),
            ),
            Action::Approve => (
                vec![
                    AccountMeta::new(*task, false),
                    AccountMeta::new_readonly(*wallet, true),
                ],
                discriminator("approve_task").to_vec(),
            ),
            _ => unimplemented!(),
        };
        let ix = Instruction {
            program_id: shillbot::ID,
            accounts,
            data,
        };
        let message = Message::new_with_blockhash(&[ix], Some(wallet), &Hash::new_unique());
        VersionedTransaction::from(Transaction::new_unsigned(message))
    }

    fn request(action: Action, wallet: Pubkey, task: Pubkey) -> ValidationRequest {
        ValidationRequest {
            action,
            network: "devnet".into(),
            wallet: wallet.to_string(),
            task_pda: task.to_string(),
            escrow_lamports: None,
            content_id: None,
            sponsor: None,
            expected_accounts: None,
            expected_data: None,
            allowed_payout_to: None,
            require_payout_routing: false,
            switchboard_feed: None,
        }
    }

    fn transaction(ixs: Vec<Instruction>, payer: &Pubkey) -> VersionedTransaction {
        let message = Message::new_with_blockhash(&ixs, Some(payer), &Hash::new_unique());
        VersionedTransaction::from(Transaction::new_unsigned(message))
    }

    fn exact_request(
        action: Action,
        wallet: Pubkey,
        task: Pubkey,
        ix: &Instruction,
    ) -> ValidationRequest {
        let mut expected = request(action, wallet, task);
        expected.expected_accounts = Some(
            ix.accounts
                .iter()
                .map(|account| account.pubkey.to_string())
                .collect(),
        );
        expected.expected_data = Some(base64::engine::general_purpose::STANDARD.encode(&ix.data));
        expected
    }

    #[test]
    fn accepts_locally_constructed_claim_without_server_provenance() {
        let wallet = Keypair::new().pubkey();
        let task = Pubkey::new_unique();
        let inspected = validate(
            &test_tx(Action::Claim, &wallet, &task),
            &request(Action::Claim, wallet, task),
        )
        .unwrap();
        assert_eq!(inspected.action, Action::Claim);
        assert_eq!(inspected.risk, "earn");
    }

    #[test]
    fn rejects_valid_transaction_for_another_task() {
        let wallet = Keypair::new().pubkey();
        let actual = Pubkey::new_unique();
        let expected = Pubkey::new_unique();
        let error = validate(
            &test_tx(Action::Claim, &wallet, &actual),
            &request(Action::Claim, wallet, expected),
        )
        .unwrap_err();
        assert!(error.to_string().contains("different task"));
    }

    #[test]
    fn rejects_action_substitution() {
        let wallet = Keypair::new().pubkey();
        let task = Pubkey::new_unique();
        let error = validate(
            &test_tx(Action::Approve, &wallet, &task),
            &request(Action::Claim, wallet, task),
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("unexpected Shillbot instruction"));
    }

    #[test]
    fn rejects_unrelated_companion_instruction() {
        let wallet = Keypair::new().pubkey();
        let task = Pubkey::new_unique();
        let mut tx = test_tx(Action::Claim, &wallet, &task);
        if let VersionedMessage::Legacy(message) = &mut tx.message {
            message.account_keys.push(Pubkey::new_unique());
            let program_index = u8::try_from(message.account_keys.len() - 1).unwrap();
            message.instructions.push(
                solana_sdk::instruction::CompiledInstruction::new_from_raw_parts(
                    program_index,
                    vec![],
                    vec![],
                ),
            );
        }
        let error = validate(&tx, &request(Action::Claim, wallet, task)).unwrap_err();
        assert!(error.to_string().contains("unexpected companion program"));
    }

    #[test]
    fn create_binds_amount_terms_accounts_and_wallet() {
        let wallet = Keypair::new().pubkey();
        let args = CreateTaskArgs {
            nonce: 42,
            escrow_lamports: 1_000_000,
            content_hash: [9; 32],
            deadline: 1_900_000_000,
            submit_margin: 14_400,
            claim_buffer: 14_400,
            platform: 0,
            attestation_delay_override: 0,
            challenge_window_override: 0,
            verification_timeout_override: 0,
            requires_approval: true,
            verification_kind: 0,
        };
        let ix = create_task_instruction(wallet, args);
        let task = task_pda(args.nonce, &wallet);
        let tx = transaction(vec![ix.clone()], &wallet);
        let mut expected = exact_request(Action::Create, wallet, task, &ix);
        expected.escrow_lamports = Some(args.escrow_lamports);
        assert_eq!(validate(&tx, &expected).unwrap().risk, "spend");

        let mut altered_amount = expected.clone();
        altered_amount.escrow_lamports = Some(args.escrow_lamports + 1);
        assert!(validate(&tx, &altered_amount)
            .unwrap_err()
            .to_string()
            .contains("escrow amount"));

        let mut altered_terms = expected;
        altered_terms.expected_data =
            Some(base64::engine::general_purpose::STANDARD.encode([0_u8; 128]));
        assert!(validate(&tx, &altered_terms)
            .unwrap_err()
            .to_string()
            .contains("arguments"));
    }

    #[test]
    fn submit_binds_content_id_and_exact_accounts() {
        let wallet = Keypair::new().pubkey();
        let task = Pubkey::new_unique();
        let ix = submit_work_instruction(wallet, task, b"content-A".to_vec());
        let tx = transaction(vec![ix.clone()], &wallet);
        let mut expected = exact_request(Action::Submit, wallet, task, &ix);
        expected.content_id = Some("content-A".into());
        validate(&tx, &expected).unwrap();

        let mut wrong_content = expected.clone();
        wrong_content.content_id = Some("content-B".into());
        assert!(validate(&tx, &wrong_content)
            .unwrap_err()
            .to_string()
            .contains("content id"));

        let mut wrong_accounts = expected;
        wrong_accounts.expected_accounts.as_mut().unwrap()[2] = Pubkey::new_unique().to_string();
        assert!(validate(&tx, &wrong_accounts)
            .unwrap_err()
            .to_string()
            .contains("accounts"));
    }

    #[test]
    fn rejects_wrong_wallet_network_and_excessive_compute_fee() {
        let wallet = Keypair::new().pubkey();
        let task = Pubkey::new_unique();
        let ix = claim_task_instruction(wallet, task);
        let tx = transaction(vec![ix.clone()], &wallet);

        let mut wrong_wallet = exact_request(Action::Claim, Pubkey::new_unique(), task, &ix);
        assert!(validate(&tx, &wrong_wallet)
            .unwrap_err()
            .to_string()
            .contains("fee payer"));
        wrong_wallet.wallet = wallet.to_string();
        wrong_wallet.network = "testnet".into();
        assert!(validate(&tx, &wrong_wallet)
            .unwrap_err()
            .to_string()
            .contains("network"));

        let compute = Instruction {
            program_id: Pubkey::from_str(COMPUTE_BUDGET_PROGRAM).unwrap(),
            accounts: vec![],
            data: [vec![3], 100_001_u64.to_le_bytes().to_vec()].concat(),
        };
        let tx = transaction(vec![compute, ix], &wallet);
        assert!(validate(&tx, &request(Action::Claim, wallet, task))
            .unwrap_err()
            .to_string()
            .contains("compute-unit price"));
    }

    #[test]
    fn verify_binds_feed_score_hash_and_bundle_order() {
        let payer = Keypair::new().pubkey();
        let task = Pubkey::new_unique();
        let feed = Pubkey::new_unique();
        let lifecycle = verify_task_instruction(task, feed, 900_000, [4; 32]);
        let secp = Instruction {
            program_id: Pubkey::from_str(SECP256K1_PROGRAM).unwrap(),
            accounts: vec![],
            data: vec![1, 0],
        };
        let switchboard = Instruction {
            program_id: Pubkey::from_str(SWITCHBOARD_DEVNET).unwrap(),
            accounts: vec![AccountMeta::new(feed, false)],
            data: discriminator("pull_feed_submit_response_consensus").to_vec(),
        };
        let tx = transaction(
            vec![secp.clone(), switchboard.clone(), lifecycle.clone()],
            &payer,
        );
        let mut expected = exact_request(Action::Verify, payer, task, &lifecycle);
        expected.switchboard_feed = Some(feed.to_string());
        validate(&tx, &expected).unwrap();

        let mut wrong_feed = expected.clone();
        wrong_feed.switchboard_feed = Some(Pubkey::new_unique().to_string());
        assert!(validate(&tx, &wrong_feed)
            .unwrap_err()
            .to_string()
            .contains("different feed"));

        let reordered = transaction(vec![switchboard, secp, lifecycle], &payer);
        assert!(validate(&reordered, &expected)
            .unwrap_err()
            .to_string()
            .contains("unsafe order"));
    }
}
