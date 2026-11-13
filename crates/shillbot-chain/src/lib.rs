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
const MAX_SCORE: u64 = 1_000_000;
const ONCHAIN_ACTION_PLATFORM: u8 = 11;

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

/// Build `claim_task` with the trailing `sysvar::instructions` account required
/// by `PlatformType::OnChainAction = 11` (`M1` atomic claim+submit introspection).
pub fn claim_task_atomic_instruction(agent: Pubkey, task: Pubkey) -> Instruction {
    let mut ix = claim_task_instruction(agent, task);
    ix.accounts
        .push(AccountMeta::new_readonly(sysvar::instructions::id(), false));
    ix
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

/// Build the canonical atomic `[claim_task, (set_payout_to?), submit_work]` bundle
/// for `PlatformType::OnChainAction = 11` (`M1`).
pub fn claim_and_submit_instructions(
    agent: Pubkey,
    task: Pubkey,
    content_id: Vec<u8>,
    payout_to: Option<Pubkey>,
) -> Vec<Instruction> {
    let mut out = Vec::with_capacity(3);
    out.push(claim_task_atomic_instruction(agent, task));
    if let Some(dest) = payout_to {
        out.push(Instruction {
            program_id: shillbot::ID,
            accounts: vec![
                AccountMeta::new(task, false),
                AccountMeta::new_readonly(agent, true),
            ],
            data: shillbot::instruction::SetPayoutTo { payout_to: dest }.data(),
        });
    }
    out.push(submit_work_instruction(agent, task, content_id));
    out
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

pub fn verify_task_co_signed_instruction(
    task: Pubkey,
    oracle_authority: Pubkey,
    composite_score: u64,
    verification_hash: [u8; 32],
) -> Instruction {
    Instruction {
        program_id: shillbot::ID,
        accounts: vec![
            AccountMeta::new(task, false),
            AccountMeta::new_readonly(global_state_pda(), false),
            AccountMeta::new_readonly(oracle_authority, true),
        ],
        data: shillbot::instruction::VerifyTask {
            composite_score,
            verification_hash,
        }
        .data(),
    }
}

pub fn verify_task_attested_instruction(
    task: Pubkey,
    attester: Pubkey,
    score: u64,
    verification_hash: [u8; 32],
    reopen_agent: Option<Pubkey>,
) -> Instruction {
    let mut accounts = vec![
        AccountMeta::new(task, false),
        AccountMeta::new_readonly(global_state_pda(), false),
        AccountMeta::new_readonly(attester, true),
    ];
    if let Some(agent) = reopen_agent {
        accounts.push(AccountMeta::new(agent_state_pda(&agent), false));
    }
    Instruction {
        program_id: shillbot::ID,
        accounts,
        data: shillbot::instruction::VerifyTaskAttested {
            score,
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
    pub wallet: String,
    pub fee_payer: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sponsor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payout_to: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub escrow_lamports: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_id: Option<String>,
    #[serde(rename = "signers")]
    pub required_signers: Vec<String>,
    pub accounts: Vec<String>,
    pub movements: Vec<Movement>,
    pub task_pda: String,
    pub instruction_count: usize,
    pub instruction_index: usize,
    #[serde(rename = "digest")]
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

/// Return the recipient encoded by a claim's optional `set_payout_to`
/// companion, after checking that it is bound to the expected task and agent.
pub fn payout_to_base64(
    value: &str,
    task: &str,
    wallet: &str,
) -> Result<Option<String>, ValidationError> {
    let tx = decode_base64(value)?;
    ensure_no_lookups(&tx.message)?;
    let task = Pubkey::from_str(task)
        .map_err(|_| ValidationError::Request("task_pda is not a Solana pubkey".into()))?;
    let wallet = Pubkey::from_str(wallet)
        .map_err(|_| ValidationError::Request("wallet is not a Solana pubkey".into()))?;
    let keys = static_keys(&tx.message);
    let disc = discriminator("set_payout_to");
    let mut recipient = None;
    for ix in instructions(&tx.message) {
        let program = keys.get(usize::from(ix.program_id_index));
        if program != Some(&shillbot::ID) || ix.data.get(..8) != Some(disc.as_slice()) {
            continue;
        }
        if recipient.is_some()
            || ix.accounts.len() != 2
            || keys.get(usize::from(ix.accounts[0])) != Some(&task)
            || keys.get(usize::from(ix.accounts[1])) != Some(&wallet)
            || ix.data.len() != 40
        {
            return Err(ValidationError::Rejected(
                "invalid or repeated payout-routing instruction".into(),
            ));
        }
        let pubkey_bytes: [u8; 32] = ix.data[8..40].try_into().map_err(|_| {
            ValidationError::Rejected("invalid or repeated payout-routing instruction".into())
        })?;
        recipient = Some(Pubkey::new_from_array(pubkey_bytes));
    }
    Ok(recipient.map(|key| key.to_string()))
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
            let unit_bytes: [u8; 4] = data[1..5].try_into().map_err(|_| {
                ValidationError::Rejected("unsupported compute-budget instruction".into())
            })?;
            let units = u32::from_le_bytes(unit_bytes);
            if units == 0 || units > 1_400_000 {
                return Err(ValidationError::Rejected(
                    "compute-unit limit is outside the accepted bound".into(),
                ));
            }
        }
        Some(3) if data.len() == 9 => {
            let price_bytes: [u8; 8] = data[1..9].try_into().map_err(|_| {
                ValidationError::Rejected("unsupported compute-budget instruction".into())
            })?;
            let price = u64::from_le_bytes(price_bytes);
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

fn is_static_account_writable(message: &VersionedMessage, index: u8) -> bool {
    let idx = usize::from(index);
    let header = match message {
        VersionedMessage::Legacy(m) => &m.header,
        VersionedMessage::V0(m) => &m.header,
    };
    let num_req = usize::from(header.num_required_signatures);
    let num_ro_signed = usize::from(header.num_readonly_signed_accounts);
    let num_ro_unsigned = usize::from(header.num_readonly_unsigned_accounts);
    let total = static_keys(message).len();
    if idx >= total {
        return false;
    }
    if idx < num_req {
        idx < num_req.saturating_sub(num_ro_signed)
    } else {
        idx < total.saturating_sub(num_ro_unsigned)
    }
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
    let verify_attested_disc = discriminator("verify_task_attested");
    let claim_disc = discriminator("claim_task");
    let set_payout_disc = discriminator("set_payout_to");
    let switchboard_submit_disc = discriminator("pull_feed_submit_response_consensus");
    let compute_program = Pubkey::from_str(COMPUTE_BUDGET_PROGRAM)
        .map_err(|_| ValidationError::Transaction("invalid compute budget program id".into()))?;
    let secp_program = Pubkey::from_str(SECP256K1_PROGRAM)
        .map_err(|_| ValidationError::Transaction("invalid secp256k1 program id".into()))?;
    let switchboard_program = Pubkey::from_str(if expected.network == "devnet" {
        SWITCHBOARD_DEVNET
    } else {
        SWITCHBOARD_MAINNET
    })
    .map_err(|_| ValidationError::Transaction("invalid switchboard program id".into()))?;

    let mut matched = None;
    let mut is_attested_verify = false;
    let mut preceding_claim_pos: Option<usize> = None;
    let mut payout_pos: Option<usize> = None;
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
            let matches_lifecycle = disc == expected_disc
                || (expected.action == Action::Verify && disc == verify_attested_disc);
            if matches_lifecycle {
                if matched.is_some() {
                    return Err(ValidationError::Rejected(
                        "transaction repeats the lifecycle instruction".into(),
                    ));
                }
                if let Some(claim_pos) = preceding_claim_pos {
                    let expected_submit_pos = match payout_pos {
                        Some(p_pos) => p_pos.saturating_add(1),
                        None => claim_pos.saturating_add(1),
                    };
                    if position != expected_submit_pos {
                        return Err(ValidationError::Rejected(
                            "submit_work must immediately follow atomic claim_task".into(),
                        ));
                    }
                }
                if expected.action == Action::Verify && disc == verify_attested_disc {
                    is_attested_verify = true;
                }
                matched = Some((position, ix));
            } else if expected.action == Action::Submit && disc == claim_disc {
                if matched.is_some() || preceding_claim_pos.is_some() || payout_routing > 0 {
                    return Err(ValidationError::Rejected(
                        "atomic claim_task must appear once before submit_work".into(),
                    ));
                }
                validate_companion_claim_ix(ix, keys, &task, &wallet)?;
                preceding_claim_pos = Some(position);
            } else if disc == set_payout_disc
                && (expected.action == Action::Claim
                    || (expected.action == Action::Submit
                        && preceding_claim_pos.is_some()
                        && matched.is_none()))
            {
                if expected.action == Action::Claim && matched.is_none() {
                    return Err(ValidationError::Rejected(
                        "payout-routing instruction must follow claim".into(),
                    ));
                }
                if let Some(claim_pos) = preceding_claim_pos {
                    if position != claim_pos.saturating_add(1) {
                        return Err(ValidationError::Rejected(
                            "payout-routing instruction must immediately follow claim_task".into(),
                        ));
                    }
                }
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
                payout_pos = Some(position);
            } else if disc == set_payout_disc && expected.action == Action::Claim {
                return Err(ValidationError::Rejected(
                    "payout-routing instruction must follow claim".into(),
                ));
            } else {
                return Err(ValidationError::Rejected(
                    "unexpected Shillbot instruction".into(),
                ));
            }
        } else {
            let allowed = if *program == compute_program {
                validate_compute_budget(&ix.data)?;
                let kind = usize::from(ix.data[0].checked_sub(2).ok_or_else(|| {
                    ValidationError::Rejected("invalid compute-budget instruction kind".into())
                })?);
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

    let (lifecycle_index, lifecycle) = matched.ok_or_else(|| {
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
        if is_attested_verify {
            if !secp_indices.is_empty() || !switchboard_indices.is_empty() {
                return Err(ValidationError::Rejected(
                    "Switchboard companion instructions are not allowed with verify_task_attested"
                        .into(),
                ));
            }
            if lifecycle.accounts.len() != 3 && lifecycle.accounts.len() != 4 {
                return Err(ValidationError::Rejected(
                    "verify_task_attested must carry 3 accounts (or 4 for score-0 reopen)".into(),
                ));
            }
            if lifecycle.data.len() != 48 {
                return Err(ValidationError::Rejected(
                    "verify_task_attested instruction data has invalid length".into(),
                ));
            }
            let score_bytes: [u8; 8] = lifecycle.data[8..16].try_into().map_err(|_| {
                ValidationError::Rejected("verify_task_attested score is invalid".into())
            })?;
            let score = u64::from_le_bytes(score_bytes);
            if score != 0 && score != MAX_SCORE {
                return Err(ValidationError::Rejected(
                    "verify_task_attested score must be 0 or MAX_SCORE".into(),
                ));
            }
            if lifecycle.data.get(16..48) == Some([0u8; 32].as_slice()) {
                return Err(ValidationError::Rejected(
                    "verify_task_attested verification_hash must be non-zero".into(),
                ));
            }
            let global_idx = lifecycle.accounts[1];
            if keys.get(usize::from(global_idx)) != Some(&global_state_pda()) {
                return Err(ValidationError::Rejected(
                    "verify_task_attested global_state differs from canonical PDA".into(),
                ));
            }
            let attester_idx = lifecycle.accounts[2];
            if usize::from(attester_idx) >= required {
                return Err(ValidationError::Rejected(
                    "verify_task_attested attester must be a required signer".into(),
                ));
            }
            if let Some(feed) = expected_feed {
                if keys.get(usize::from(attester_idx)) != Some(&feed) {
                    return Err(ValidationError::Rejected(
                        "verify oracle authority differs from expected authority".into(),
                    ));
                }
            }
            if lifecycle.accounts.len() == 4 {
                if score != 0 {
                    return Err(ValidationError::Rejected(
                        "trailing AgentState account is only allowed for score-0 reopen".into(),
                    ));
                }
                if !is_static_account_writable(&tx.message, lifecycle.accounts[3]) {
                    return Err(ValidationError::Rejected(
                        "reopen AgentState account must be writable".into(),
                    ));
                }
            }
        } else {
            if expected.network == "mainnet" {
                return Err(ValidationError::Rejected(
                    "legacy verify_task is restricted to devnet".into(),
                ));
            }
            if secp_indices.is_empty() && switchboard_indices.is_empty() {
                let oracle_index = lifecycle.accounts.get(2).copied().ok_or_else(|| {
                    ValidationError::Rejected("verify instruction has too few accounts".into())
                })?;
                if usize::from(oracle_index) >= required {
                    return Err(ValidationError::Rejected(
                        "verify oracle authority account must be a required signer".into(),
                    ));
                }
                if let Some(feed) = expected_feed {
                    let oracle_key =
                        keys.get(usize::from(oracle_index))
                            .copied()
                            .ok_or_else(|| {
                                ValidationError::Rejected(
                                    "verify oracle account index is invalid".into(),
                                )
                            })?;
                    if oracle_key != feed {
                        return Err(ValidationError::Rejected(
                            "verify oracle authority differs from expected authority".into(),
                        ));
                    }
                }
            } else {
                if secp_indices.len() != 1 || switchboard_indices.len() != 1 {
                    return Err(ValidationError::Rejected(
                        "verify requires exactly one secp256k1 and one Switchboard instruction"
                            .into(),
                    ));
                }
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
    if expected.action == Action::Claim {
        if lifecycle.accounts.len() > 6 {
            return Err(ValidationError::Rejected(
                "claim_task has invalid account count".into(),
            ));
        }
        if lifecycle.accounts.len() == 6 && account_key(5)? != sysvar::instructions::id() {
            return Err(ValidationError::Rejected(
                "claim_task trailing account must be sysvar::instructions".into(),
            ));
        }
    }
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
    }
    if !keys[..required].contains(&wallet) {
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
        if let (Some(&platform), Some(&kind), Some(escrow_slice)) = (
            lifecycle.data.get(80),
            lifecycle.data.get(94),
            lifecycle.data.get(16..24),
        ) {
            if platform == ONCHAIN_ACTION_PLATFORM {
                if kind > 1 {
                    return Err(ValidationError::Rejected(
                        "OnChainAction carries invalid verification_kind".into(),
                    ));
                }
                if kind == 0 && expected.network == "mainnet" {
                    return Err(ValidationError::Rejected(
                        "OnChainAction verification_kind 0 is restricted to devnet".into(),
                    ));
                }
                let escrow_bytes: [u8; 8] = escrow_slice.try_into().map_err(|_| {
                    ValidationError::Rejected("create instruction escrow is invalid".into())
                })?;
                if u64::from_le_bytes(escrow_bytes)
                    < shillbot::constants::MIN_CAMPAIGN_ESCROW_LAMPORTS
                {
                    return Err(ValidationError::Rejected(
                        "OnChainAction escrow is below minimum campaign escrow".into(),
                    ));
                }
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
            let content_end = 4usize.checked_add(len).ok_or_else(|| {
                ValidationError::Rejected("content id length overflows transaction data".into())
            })?;
            if raw.len() != content_end || raw.get(4..content_end) != Some(content_id.as_bytes()) {
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
        wallet: expected.wallet.clone(),
        fee_payer: fee_payer.to_string(),
        sponsor: expected.sponsor.clone(),
        payout_to: if payout_routing == 1 {
            expected.allowed_payout_to.clone()
        } else {
            None
        },
        escrow_lamports: expected.escrow_lamports.map(|amount| amount.to_string()),
        content_id: expected.content_id.clone(),
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
        instruction_index: lifecycle_index,
        intent_digest: hex::encode(digest),
        risk: match expected.action {
            Action::Create => "spend",
            Action::Approve => "escrow_control",
            Action::Claim | Action::Submit | Action::Verify | Action::Finalize => "earn",
        }
        .into(),
    })
}

fn validate_companion_claim_ix(
    ix: &solana_sdk::instruction::CompiledInstruction,
    keys: &[Pubkey],
    task: &Pubkey,
    wallet: &Pubkey,
) -> Result<(), ValidationError> {
    if ix.data.as_slice() != discriminator("claim_task").as_slice() {
        return Err(ValidationError::Rejected(
            "companion claim_task carries unexpected instruction data".into(),
        ));
    }
    if ix.accounts.len() != 6 {
        return Err(ValidationError::Rejected(
            "companion claim_task must carry 6 accounts including sysvar::instructions".into(),
        ));
    }
    let key_at = |pos: usize| -> Option<&Pubkey> {
        ix.accounts
            .get(pos)
            .and_then(|idx| keys.get(usize::from(*idx)))
    };
    if key_at(0) != Some(task) {
        return Err(ValidationError::Rejected(
            "companion claim_task targets a different task".into(),
        ));
    }
    if key_at(1) != Some(&global_state_pda())
        || key_at(2) != Some(&agent_state_pda(wallet))
        || key_at(4) != Some(&solana_system_interface::program::ID)
    {
        return Err(ValidationError::Rejected(
            "companion claim_task accounts differ from canonical PDAs".into(),
        ));
    }
    if key_at(3) != Some(wallet) {
        return Err(ValidationError::Rejected(
            "companion claim_task authority differs from wallet".into(),
        ));
    }
    if key_at(5) != Some(&sysvar::instructions::id()) {
        return Err(ValidationError::Rejected(
            "companion claim_task trailing account must be sysvar::instructions".into(),
        ));
    }
    Ok(())
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

    #[test]
    fn rust_builders_match_the_public_client_golden_vectors() {
        let golden: serde_json::Value = serde_json::from_str(include_str!(
            "../../../sdk/client/test/transaction-vectors.json"
        ))
        .unwrap();
        let wallet = Pubkey::from_str(golden["wallet"].as_str().unwrap()).unwrap();
        let task = Pubkey::from_str(golden["task"].as_str().unwrap()).unwrap();
        let feed = Pubkey::from_str(golden["feed"].as_str().unwrap()).unwrap();
        let client = Pubkey::from_str(golden["client"].as_str().unwrap()).unwrap();
        let instructions = [
            create_task_instruction(
                wallet,
                CreateTaskArgs {
                    nonce: 42,
                    escrow_lamports: 1_000_000,
                    content_hash: [0x11; 32],
                    deadline: 1_900_000_000,
                    submit_margin: 14_400,
                    claim_buffer: 14_400,
                    platform: 0,
                    attestation_delay_override: 0,
                    challenge_window_override: 0,
                    verification_timeout_override: 0,
                    requires_approval: true,
                    verification_kind: 0,
                },
            ),
            claim_task_instruction(wallet, task),
            submit_work_instruction(wallet, task, b"video-123".to_vec()),
            approve_task_instruction(wallet, task),
            verify_task_instruction(task, feed, 900_000, [0x22; 32]),
            finalize_task_instruction(task, wallet, client, feed),
        ];
        for (actual, expected) in instructions
            .iter()
            .zip(golden["vectors"].as_array().unwrap())
        {
            let accounts: Vec<_> = actual
                .accounts
                .iter()
                .map(|a| a.pubkey.to_string())
                .collect();
            assert_eq!(
                accounts,
                serde_json::from_value::<Vec<String>>(expected["accounts"].clone()).unwrap()
            );
            assert_eq!(
                base64::engine::general_purpose::STANDARD.encode(&actual.data),
                expected["data_base64"].as_str().unwrap()
            );
        }
    }

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
        assert_eq!(inspected.wallet, wallet.to_string());
        assert_eq!(inspected.sponsor, None);
        assert_eq!(inspected.payout_to, None);
    }

    #[test]
    fn sponsored_claim_intent_exposes_fee_payer_and_payout_route() {
        let wallet = Keypair::new().pubkey();
        let sponsor = Keypair::new().pubkey();
        let task = Pubkey::new_unique();
        let payout_to = Pubkey::new_unique();
        let route = Instruction {
            program_id: shillbot::ID,
            accounts: vec![
                AccountMeta::new(task, false),
                AccountMeta::new_readonly(wallet, true),
            ],
            data: [
                discriminator("set_payout_to").to_vec(),
                payout_to.to_bytes().to_vec(),
            ]
            .concat(),
        };
        let lifecycle = claim_task_instruction(wallet, task);
        let tx = transaction(vec![lifecycle, route], &sponsor);
        let mut expected = request(Action::Claim, wallet, task);
        expected.sponsor = Some(sponsor.to_string());
        expected.allowed_payout_to = Some(payout_to.to_string());
        expected.require_payout_routing = true;

        let inspected = validate(&tx, &expected).unwrap();
        assert_eq!(inspected.wallet, wallet.to_string());
        assert_eq!(inspected.fee_payer, sponsor.to_string());
        assert_eq!(inspected.sponsor, Some(sponsor.to_string()));
        assert_eq!(inspected.payout_to, Some(payout_to.to_string()));
    }

    #[test]
    fn rejects_payout_route_before_claim_establishes_the_agent() {
        let wallet = Keypair::new().pubkey();
        let sponsor = Keypair::new().pubkey();
        let task = Pubkey::new_unique();
        let payout_to = Pubkey::new_unique();
        let route = Instruction {
            program_id: shillbot::ID,
            accounts: vec![
                AccountMeta::new(task, false),
                AccountMeta::new_readonly(wallet, true),
            ],
            data: [
                discriminator("set_payout_to").to_vec(),
                payout_to.to_bytes().to_vec(),
            ]
            .concat(),
        };
        let lifecycle = claim_task_instruction(wallet, task);
        let tx = transaction(vec![route, lifecycle], &sponsor);
        let mut expected = request(Action::Claim, wallet, task);
        expected.sponsor = Some(sponsor.to_string());
        expected.allowed_payout_to = Some(payout_to.to_string());
        expected.require_payout_routing = true;

        let error = validate(&tx, &expected).unwrap_err();
        assert!(error.to_string().contains("must follow claim"));
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
        let inspected = validate(&tx, &expected).unwrap();
        assert_eq!(inspected.risk, "spend");
        assert_eq!(inspected.escrow_lamports.as_deref(), Some("1000000"));

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
        assert_eq!(
            validate(&tx, &expected).unwrap().content_id.as_deref(),
            Some("content-A")
        );

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

    #[test]
    fn verify_co_signed_binds_oracle_signer_score_and_hash() {
        let payer = Keypair::new().pubkey();
        let oracle = Keypair::new().pubkey();
        let task = Pubkey::new_unique();
        let lifecycle = verify_task_co_signed_instruction(task, oracle, 900_000, [4; 32]);
        let tx = transaction(vec![lifecycle.clone()], &payer);
        let mut expected = exact_request(Action::Verify, payer, task, &lifecycle);
        expected.switchboard_feed = Some(oracle.to_string());
        let inspected = validate(&tx, &expected).unwrap();
        assert_eq!(
            inspected.required_signers,
            vec![payer.to_string(), oracle.to_string()]
        );

        let mut wrong_oracle = expected.clone();
        wrong_oracle.switchboard_feed = Some(Pubkey::new_unique().to_string());
        assert!(validate(&tx, &wrong_oracle)
            .unwrap_err()
            .to_string()
            .contains("differs from expected authority"));

        let unsigned_oracle_ix = verify_task_instruction(task, oracle, 900_000, [4; 32]);
        let unsigned_oracle_tx = transaction(vec![unsigned_oracle_ix.clone()], &payer);
        let unsigned_expected = exact_request(Action::Verify, payer, task, &unsigned_oracle_ix);
        assert!(validate(&unsigned_oracle_tx, &unsigned_expected)
            .unwrap_err()
            .to_string()
            .contains("must be a required signer"));
    }

    #[test]
    fn sponsored_tx_requires_wallet_to_be_signer() {
        let sponsor = Keypair::new().pubkey();
        let wallet = Keypair::new().pubkey();
        let task = Pubkey::new_unique();
        // Build a claim instruction where wallet (account 3) is NOT marked as a signer.
        let mut ix = claim_task_instruction(wallet, task);
        ix.accounts[3].is_signer = false;
        let tx = transaction(vec![ix.clone()], &sponsor);
        let mut expected = exact_request(Action::Claim, wallet, task, &ix);
        expected.sponsor = Some(sponsor.to_string());
        let err = validate(&tx, &expected).unwrap_err();
        assert!(
            err.to_string()
                .contains("registered wallet is not a required signer"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn accepts_atomic_claim_and_submit_bundle_with_instructions_sysvar() {
        let wallet = Keypair::new().pubkey();
        let task = Pubkey::new_unique();
        let proof =
            b"eip155:56:0xabcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789:0";
        let ixs = claim_and_submit_instructions(wallet, task, proof.to_vec(), None);
        assert_eq!(ixs.len(), 2);
        assert_eq!(ixs[0].accounts.len(), 6);

        let tx = transaction(ixs.clone(), &wallet);
        let mut expected = exact_request(Action::Submit, wallet, task, &ixs[1]);
        expected.content_id = Some(String::from_utf8(proof.to_vec()).unwrap());

        let inspected = validate(&tx, &expected).unwrap();
        assert_eq!(inspected.action, Action::Submit);
        assert_eq!(inspected.instruction_count, 2);
        assert_eq!(inspected.instruction_index, 1);
    }

    #[test]
    fn accepts_sponsored_atomic_claim_payout_route_and_submit_bundle() {
        let wallet = Keypair::new().pubkey();
        let sponsor = Keypair::new().pubkey();
        let task = Pubkey::new_unique();
        let payout_to = Pubkey::new_unique();
        let proof =
            b"eip155:8453:0x1111111111111111111111111111111111111111111111111111111111111111:1";

        let ixs = claim_and_submit_instructions(wallet, task, proof.to_vec(), Some(payout_to));
        assert_eq!(ixs.len(), 3);
        let tx = transaction(ixs.clone(), &sponsor);

        let mut expected = exact_request(Action::Submit, wallet, task, &ixs[2]);
        expected.content_id = Some(String::from_utf8(proof.to_vec()).unwrap());
        expected.sponsor = Some(sponsor.to_string());
        expected.allowed_payout_to = Some(payout_to.to_string());
        expected.require_payout_routing = true;

        let inspected = validate(&tx, &expected).unwrap();
        assert_eq!(inspected.fee_payer, sponsor.to_string());
        assert_eq!(inspected.payout_to, Some(payout_to.to_string()));
        assert_eq!(inspected.instruction_index, 2);
    }

    #[test]
    fn rejects_atomic_claim_and_submit_when_claim_targets_different_task_or_agent() {
        let wallet = Keypair::new().pubkey();
        let wrong_task = Pubkey::new_unique();
        let task = Pubkey::new_unique();
        let submit_ix = submit_work_instruction(wallet, task, b"proof-1".to_vec());
        let bad_claim = claim_task_atomic_instruction(wallet, wrong_task);
        let tx = transaction(vec![bad_claim, submit_ix.clone()], &wallet);

        let mut expected = exact_request(Action::Submit, wallet, task, &submit_ix);
        expected.content_id = Some("proof-1".into());
        let err = validate(&tx, &expected).unwrap_err();
        assert!(err.to_string().contains("different task"), "{err}");

        // Lone claim_task without submit_work must also fail Action::Submit.
        let lone_claim_tx = transaction(vec![claim_task_atomic_instruction(wallet, task)], &wallet);
        let err2 = validate(&lone_claim_tx, &expected).unwrap_err();
        assert!(err2.to_string().contains("missing submit_work"), "{err2}");

        // 5-account non-atomic claim_task bundled with submit_work must be rejected.
        let five_acct_claim = claim_task_instruction(wallet, task);
        let bad_bundle_tx = transaction(vec![five_acct_claim, submit_ix], &wallet);
        let err3 = validate(&bad_bundle_tx, &expected).unwrap_err();
        assert!(err3.to_string().contains("6 accounts"), "{err3}");
    }

    #[test]
    fn payout_to_inspection_is_none_when_allowed_payout_to_set_without_routing_ix() {
        let wallet = Keypair::new().pubkey();
        let sponsor = Keypair::new().pubkey();
        let task = Pubkey::new_unique();
        let payout_to = Pubkey::new_unique();
        let lifecycle = claim_task_instruction(wallet, task);
        let tx = transaction(vec![lifecycle], &sponsor);

        let mut expected = request(Action::Claim, wallet, task);
        expected.sponsor = Some(sponsor.to_string());
        expected.allowed_payout_to = Some(payout_to.to_string());
        expected.require_payout_routing = false;

        let inspected = validate(&tx, &expected).unwrap();
        assert_eq!(inspected.payout_to, None);
    }

    #[test]
    fn verify_task_attested_validates_mainnet_pass_and_score_zero_reopen() {
        let payer = Keypair::new().pubkey();
        let attester = Keypair::new().pubkey();
        let agent = Keypair::new().pubkey();
        let task = Pubkey::new_unique();

        // Passing attestation (3 accounts) on mainnet
        let pass_ix = verify_task_attested_instruction(task, attester, MAX_SCORE, [7; 32], None);
        let pass_tx = transaction(vec![pass_ix.clone()], &payer);
        let mut expected = exact_request(Action::Verify, payer, task, &pass_ix);
        expected.network = "mainnet".into();
        expected.switchboard_feed = Some(attester.to_string());
        validate(&pass_tx, &expected).unwrap();

        // Legacy verify_task must be rejected on mainnet
        let legacy_ix = verify_task_co_signed_instruction(task, attester, 900_000, [7; 32]);
        let legacy_tx = transaction(vec![legacy_ix.clone()], &payer);
        let mut legacy_expected = exact_request(Action::Verify, payer, task, &legacy_ix);
        legacy_expected.network = "mainnet".into();
        legacy_expected.switchboard_feed = Some(attester.to_string());
        assert!(validate(&legacy_tx, &legacy_expected)
            .unwrap_err()
            .to_string()
            .contains("restricted to devnet"));

        // Score-0 reopen with writable AgentState (4 accounts) on mainnet
        let reopen_ix = verify_task_attested_instruction(task, attester, 0, [8; 32], Some(agent));
        assert_eq!(reopen_ix.accounts.len(), 4);
        assert!(reopen_ix.accounts[3].is_writable);
        let reopen_tx = transaction(vec![reopen_ix.clone()], &payer);
        let mut reopen_expected = exact_request(Action::Verify, payer, task, &reopen_ix);
        reopen_expected.network = "mainnet".into();
        reopen_expected.switchboard_feed = Some(attester.to_string());
        validate(&reopen_tx, &reopen_expected).unwrap();

        // Non-writable trailing AgentState must be rejected
        let mut readonly_reopen_ix = reopen_ix;
        readonly_reopen_ix.accounts[3].is_writable = false;
        let readonly_tx = transaction(vec![readonly_reopen_ix.clone()], &payer);
        let mut readonly_expected = exact_request(Action::Verify, payer, task, &readonly_reopen_ix);
        readonly_expected.network = "mainnet".into();
        readonly_expected.switchboard_feed = Some(attester.to_string());
        assert!(validate(&readonly_tx, &readonly_expected)
            .unwrap_err()
            .to_string()
            .contains("writable"));
    }

    #[test]
    fn onchain_action_create_accepts_kind_1_on_mainnet_and_rejects_kind_0_or_low_escrow() {
        let wallet = Keypair::new().pubkey();
        let base_args = CreateTaskArgs {
            nonce: 77,
            escrow_lamports: 5_000_000,
            content_hash: [0x55; 32],
            deadline: 1_900_000_000,
            submit_margin: 3_600,
            claim_buffer: 3_600,
            platform: ONCHAIN_ACTION_PLATFORM,
            attestation_delay_override: 60,
            challenge_window_override: 0,
            verification_timeout_override: 0,
            requires_approval: false,
            verification_kind: 1,
        };
        let task = task_pda(base_args.nonce, &wallet);

        // kind == 1 with 5_000_000 escrow succeeds on mainnet
        let ok_ix = create_task_instruction(wallet, base_args);
        let ok_tx = transaction(vec![ok_ix.clone()], &wallet);
        let mut ok_expected = exact_request(Action::Create, wallet, task, &ok_ix);
        ok_expected.network = "mainnet".into();
        ok_expected.escrow_lamports = Some(5_000_000);
        validate(&ok_tx, &ok_expected).unwrap();

        // kind == 0 on mainnet is rejected
        let mut kind0_args = base_args;
        kind0_args.verification_kind = 0;
        let kind0_ix = create_task_instruction(wallet, kind0_args);
        let kind0_tx = transaction(vec![kind0_ix.clone()], &wallet);
        let mut kind0_expected = exact_request(Action::Create, wallet, task, &kind0_ix);
        kind0_expected.network = "mainnet".into();
        assert!(validate(&kind0_tx, &kind0_expected)
            .unwrap_err()
            .to_string()
            .contains("restricted to devnet"));

        // Escrow < 5_000_000 on OnChainAction is rejected
        let mut low_args = base_args;
        low_args.escrow_lamports = 4_999_999;
        let low_ix = create_task_instruction(wallet, low_args);
        let low_tx = transaction(vec![low_ix.clone()], &wallet);
        let low_expected = exact_request(Action::Create, wallet, task, &low_ix);
        assert!(validate(&low_tx, &low_expected)
            .unwrap_err()
            .to_string()
            .contains("minimum campaign escrow"));
    }
}
