#![deny(warnings)]
#![deny(clippy::all)]
#![deny(clippy::arithmetic_side_effects)]

//! Extension-registry — the on-chain edge log for the extension-credit
//! reputation graph (mund-creanc-witer). An *extension* is a bonded,
//! obligation-creating vouch: an extender locks a SOL bond and records that
//! a recipient owes return-substance. Off-chain, the web-position score is
//! computed from these events (see `services` web-position indexer); on-chain
//! this program only holds the bond and emits the edge events.
//!
//! Accounts are intentionally ephemeral: an `Extension` PDA exists only while
//! the obligation is *active*. `attest_return_substance` (fulfilled) and
//! `default_extension` (defaulted) both close it. The durable graph is the
//! emitted event stream — `ExtensionSubmitted`, `ReturnSubstanceAttested`,
//! `ExtensionDefaulted` — which the indexer consumes.

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

    /// Extender attests the recipient fulfilled the return-obligation; the bond
    /// (and the account's rent) is returned to the extender and the edge closes.
    pub fn attest_return_substance(ctx: Context<AttestReturnSubstance>) -> Result<()> {
        instructions::attest_return_substance::attest_return_substance(ctx)
    }

    /// Authority arbitrates a default: the bond is slashed to the treasury, the
    /// account's rent is returned to the extender, and the edge closes.
    pub fn default_extension(ctx: Context<DefaultExtension>) -> Result<()> {
        instructions::default_extension::default_extension(ctx)
    }
}
