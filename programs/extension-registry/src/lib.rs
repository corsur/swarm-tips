#![deny(warnings)]
#![deny(clippy::all)]
#![deny(clippy::arithmetic_side_effects)]

//! Extension-registry — the on-chain edge log for the extension-credit
//! reputation graph (mund-creanc-witer). An *extension* is a bonded vouch: an
//! extender locks a SOL bond to back a recipient, granting it borrowed standing
//! in the web for as long as the vouch is live. Off-chain, the web-position score is
//! computed from these events (see `services` web-position indexer); on-chain
//! this program only holds the bond and emits the edge events.
//!
//! Accounts are intentionally ephemeral: an `Extension` PDA exists only while
//! the vouch is *live*. A vouch is a live, collateralized edge — the recipient
//! holds borrowed standing while it exists. `withdraw_extension` (extender
//! reclaims its vouch + bond) and `default_extension` (authority slashes a
//! defaulted recipient) both close it. The durable graph is the emitted event
//! stream — `ExtensionSubmitted`, `ExtensionWithdrawn`, `ExtensionDefaulted` —
//! which the indexer consumes.

use anchor_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod events;
pub mod instructions;
pub mod state;
pub mod transfers;

use instructions::*;

declare_id!("H7whziapWzGDH1b3QQzxno69TD4braekyBZhfjNGof4j");

#[program]
pub mod extension_registry {
    use super::*;

    /// One-time registry setup: record the authority (default arbiter) and
    /// treasury (slashed-bond sink).
    pub fn initialize(ctx: Context<Initialize>, authority: Pubkey, treasury: Pubkey) -> Result<()> {
        instructions::initialize::initialize(ctx, authority, treasury)
    }

    /// Extender locks a bond and records an obligation-creating extension to
    /// `recipient`. MVP accepts CapabilityValidation only.
    pub fn submit_extension(
        ctx: Context<SubmitExtension>,
        extension_type: u8,
        bond_lamports: u64,
    ) -> Result<()> {
        instructions::submit_extension::submit_extension(ctx, extension_type, bond_lamports)
    }

    /// Extender withdraws its vouch: the edge closes, the recipient's borrowed
    /// standing drops, and the bond (plus the account's rent) returns to the
    /// extender. Makes no claim that the recipient "fulfilled" anything.
    pub fn withdraw_extension(ctx: Context<WithdrawExtension>) -> Result<()> {
        instructions::withdraw_extension::withdraw_extension(ctx)
    }

    /// Authority arbitrates a default: the bond is slashed to the treasury, the
    /// account's rent is returned to the extender, and the edge closes.
    pub fn default_extension(ctx: Context<DefaultExtension>) -> Result<()> {
        instructions::default_extension::default_extension(ctx)
    }
}
