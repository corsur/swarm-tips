use anchor_lang::prelude::*;
use anchor_lang::system_program;
use extension_registry::constants::MIN_BOND_LAMPORTS;
use extension_registry::state::Extension;

use crate::constants::MIN_ADVANCE_LAMPORTS;
use crate::errors::ExtensionCreditError;
use crate::events::AdvanceOpened;
use crate::state::Advance;

/// Pure validation that a live, non-unbonding `Extension` backs `(backer, recipient)`.
pub fn validate_backing_extension(
    extension: &Extension,
    backer: &Pubkey,
    recipient: &Pubkey,
) -> Result<()> {
    require!(
        extension.extender == *backer && extension.recipient == *recipient,
        ExtensionCreditError::MissingActiveExtension
    );
    require!(
        !extension.is_unbonding(),
        ExtensionCreditError::ExtensionUnbonding
    );
    require!(
        extension.bond_lamports >= MIN_BOND_LAMPORTS,
        ExtensionCreditError::InsufficientExtensionBond
    );
    Ok(())
}

/// Validates that the canonical `Advance` PDA is currently uninitialized / closed
/// (`owner == SystemProgram && data_is_empty()`), preventing resurrection of an
/// active advance while allowing clean re-opening after `close_advance` / `mark_default`.
pub fn validate_uninitialized_advance(advance_ai: &AccountInfo) -> Result<()> {
    require!(
        advance_ai.owner == &system_program::ID && advance_ai.data_is_empty(),
        ExtensionCreditError::AdvanceAlreadyActive
    );
    Ok(())
}

/// Planned lamport flows for `open_advance` (P1-03):
/// - `recipient` ALWAYS receives 100% of `pre_existing` late payouts + `advance_lamports`
/// - `backer` ALWAYS pays 100% of `rent_floor` + `advance_lamports` (never subsidized by
///   pre-existing lamports sitting on a closed PDA)
/// - `advance` PDA ends with exact balance `rent_floor`.
#[derive(Debug, PartialEq, Eq)]
pub struct OpenAdvanceFlows {
    pub pre_existing_to_recipient: u64,
    pub backer_to_recipient: u64,
    pub backer_to_advance_rent: u64,
}

pub fn plan_open_advance_flows(
    pre_existing: u64,
    advance_lamports: u64,
    rent_floor: u64,
) -> OpenAdvanceFlows {
    OpenAdvanceFlows {
        pre_existing_to_recipient: pre_existing,
        backer_to_recipient: advance_lamports,
        backer_to_advance_rent: rent_floor,
    }
}

/// Backer fronts working capital to the recipient and opens the advance (which
/// is also the earnings-routing vault). Requires an active, non-unbonding
/// `extension-registry` vouch (`Extension` PDA) from `backer` to `recipient`
/// (HIGH-16).
///
/// Manual PDA creation (P1-03): Anchor's `#[account(init)]` deducts any
/// pre-existing lamports on the PDA from the payer's rent cost, which would
/// silently consume a recipient's late-arriving Shillbot payout as the backer's
/// PDA rent. Instead we drain 100% of `pre_existing` lamports to `recipient`
/// via PDA-signed System transfer first, then call `system_program::create_account`
/// so `backer` ALWAYS funds 100% of `rent_floor`.
pub fn open_advance(ctx: Context<OpenAdvance>, advance_lamports: u64) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let backer_key = ctx.accounts.backer.key();
    let recipient_key = ctx.accounts.recipient.key();
    let advance_ai = ctx.accounts.advance.to_account_info();

    // Checks
    require!(
        advance_lamports >= MIN_ADVANCE_LAMPORTS,
        ExtensionCreditError::AdvanceTooLow
    );
    require!(
        backer_key != recipient_key,
        ExtensionCreditError::SelfAdvance
    );
    validate_backing_extension(&ctx.accounts.extension, &backer_key, &recipient_key)?;
    validate_uninitialized_advance(&advance_ai)?;

    let rent_floor = Rent::get()?.minimum_balance(Advance::SPACE);
    let flows = plan_open_advance_flows(advance_ai.lamports(), advance_lamports, rent_floor);

    let bump = ctx.bumps.advance;
    let signer_seeds: &[&[&[u8]]] = &[&[
        b"advance",
        backer_key.as_ref(),
        recipient_key.as_ref(),
        &[bump],
    ]];

    // 1. Front working capital from backer to recipient first (guarantees
    //    recipient is rent-exempt even if pre_existing is sub-rent dust).
    system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            system_program::Transfer {
                from: ctx.accounts.backer.to_account_info(),
                to: ctx.accounts.recipient.to_account_info(),
            },
        ),
        flows.backer_to_recipient,
    )?;

    // 2. Forward 100% of any late-arriving payout sitting on the closed PDA
    //    directly to recipient, draining the PDA to 0 lamports.
    if flows.pre_existing_to_recipient > 0 {
        system_program::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.system_program.to_account_info(),
                system_program::Transfer {
                    from: advance_ai.clone(),
                    to: ctx.accounts.recipient.to_account_info(),
                },
                signer_seeds,
            ),
            flows.pre_existing_to_recipient,
        )?;
    }

    // Postcondition: PDA holds 0 lamports so `create_account` charges backer 100% of rent.
    require!(
        advance_ai.lamports() == 0,
        ExtensionCreditError::VaultNotDrained
    );

    // 3. Create the Advance PDA with backer paying 100% of `rent_floor`.
    let space_u64 = u64::try_from(Advance::SPACE)
        .map_err(|_| error!(ExtensionCreditError::ArithmeticOverflow))?;
    system_program::create_account(
        CpiContext::new_with_signer(
            ctx.accounts.system_program.to_account_info(),
            system_program::CreateAccount {
                from: ctx.accounts.backer.to_account_info(),
                to: advance_ai.clone(),
            },
            signer_seeds,
        ),
        flows.backer_to_advance_rent,
        space_u64,
        &crate::ID,
    )?;

    // 4. Write the Advance discriminator + state fields.
    let state = Advance {
        backer: backer_key,
        recipient: recipient_key,
        advance_lamports,
        recouped_lamports: 0,
        created_at: now,
        bump,
        _reserved: [0u8; 16],
    };
    let mut data = advance_ai.try_borrow_mut_data()?;
    state.try_serialize(&mut &mut data[..])?;

    emit!(AdvanceOpened {
        backer: backer_key,
        recipient: recipient_key,
        advance_lamports,
        created_at: now,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct OpenAdvance<'info> {
    /// CHECK: Uninitialized / closed Advance PDA (`owner == SystemProgram && data_is_empty()`),
    /// verified by canonical seeds and `validate_uninitialized_advance`, then created via
    /// System `create_account` with `backer` funding 100% of `rent_floor` (P1-03).
    #[account(
        mut,
        seeds = [b"advance", backer.key().as_ref(), recipient.key().as_ref()],
        bump,
    )]
    pub advance: UncheckedAccount<'info>,
    #[account(
        seeds = [b"extension", backer.key().as_ref(), recipient.key().as_ref()],
        bump = extension.bump,
        seeds::program = extension_registry::ID,
    )]
    pub extension: Account<'info, Extension>,
    #[account(mut)]
    pub backer: Signer<'info>,
    /// CHECK: receives the fronted capital and is used to derive the PDA seed.
    #[account(mut)]
    pub recipient: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_extension(extender: Pubkey, recipient: Pubkey) -> Extension {
        Extension {
            extender,
            recipient,
            extension_type: 0,
            bond_lamports: MIN_BOND_LAMPORTS,
            created_at: 1_700_000_000,
            bump: 254,
            unbond_requested_at: 0,
            _reserved: [0u8; 8],
        }
    }

    #[test]
    fn validate_backing_extension_accepts_active_and_rejects_unbonding_or_mismatched() {
        let backer = Pubkey::new_unique();
        let recipient = Pubkey::new_unique();
        let mut ext = sample_extension(backer, recipient);

        assert!(validate_backing_extension(&ext, &backer, &recipient).is_ok());

        // Mismatched backer or recipient -> rejected.
        assert!(validate_backing_extension(&ext, &Pubkey::new_unique(), &recipient).is_err());
        assert!(validate_backing_extension(&ext, &backer, &Pubkey::new_unique()).is_err());

        // Unbonding in progress -> rejected.
        ext.unbond_requested_at = 1_700_000_500;
        assert!(validate_backing_extension(&ext, &backer, &recipient).is_err());

        // Below minimum bond -> rejected.
        ext.unbond_requested_at = 0;
        ext.bond_lamports = MIN_BOND_LAMPORTS.saturating_sub(1);
        assert!(validate_backing_extension(&ext, &backer, &recipient).is_err());
    }

    #[test]
    fn open_advance_flows_always_charge_backer_full_rent_and_forward_all_pre_existing_to_recipient()
    {
        let rent_floor: u64 = 1_678_080;
        let advance_lamports: u64 = 2_000_000;

        // Case 1: Late-arriving payout < rent_floor (e.g. 500_000 lamports):
        // previously Anchor `init` consumed ALL 500_000 as backer's rent!
        let f_small = plan_open_advance_flows(500_000, advance_lamports, rent_floor);
        assert_eq!(f_small.pre_existing_to_recipient, 500_000);
        assert_eq!(f_small.backer_to_advance_rent, rent_floor);
        assert_eq!(f_small.backer_to_recipient, advance_lamports);

        // Case 2: Late-arriving payout > rent_floor (e.g. 5_000_000 lamports):
        // recipient receives 100% of 5_000_000 (not 5_000_000 - rent_floor),
        // and backer pays 100% of rent_floor.
        let f_large = plan_open_advance_flows(5_000_000, advance_lamports, rent_floor);
        assert_eq!(f_large.pre_existing_to_recipient, 5_000_000);
        assert_eq!(f_large.backer_to_advance_rent, rent_floor);
        assert_eq!(f_large.backer_to_recipient, advance_lamports);
    }

    #[test]
    fn validate_uninitialized_advance_and_manual_serialization_round_trip() {
        let key = Pubkey::new_unique();
        let sys_owner = system_program::ID;
        let prog_owner = crate::ID;

        // Closed / pre-funded PDA (System-owned, 0 bytes) -> accepted.
        let mut lamports = 500_000u64;
        let mut empty_data = vec![];
        let closed_ai = AccountInfo::new(
            &key,
            false,
            true,
            &mut lamports,
            &mut empty_data,
            &sys_owner,
            false,
            0,
        );
        assert!(validate_uninitialized_advance(&closed_ai).is_ok());

        // Already-active PDA (program-owned, Advance::SPACE bytes) -> rejected.
        let mut active_lamports = 2_000_000u64;
        let mut active_data = vec![0u8; Advance::SPACE];
        let active_ai = AccountInfo::new(
            &key,
            false,
            true,
            &mut active_lamports,
            &mut active_data,
            &prog_owner,
            false,
            0,
        );
        assert!(validate_uninitialized_advance(&active_ai).is_err());

        // Manual Anchor try_serialize writes discriminator + fields and round-trips cleanly.
        let backer = Pubkey::new_unique();
        let recipient = Pubkey::new_unique();
        let state = Advance {
            backer,
            recipient,
            advance_lamports: 2_000_000,
            recouped_lamports: 0,
            created_at: 1_765_000_000,
            bump: 253,
            _reserved: [0u8; 16],
        };
        let mut buf = [0u8; Advance::SPACE];
        state.try_serialize(&mut &mut buf[..]).unwrap();
        let decoded = Advance::try_deserialize(&mut &buf[..]).unwrap();
        assert_eq!(decoded.backer, backer);
        assert_eq!(decoded.recipient, recipient);
        assert_eq!(decoded.advance_lamports, 2_000_000);
        assert_eq!(decoded.recouped_lamports, 0);
        assert_eq!(decoded.created_at, 1_765_000_000);
        assert_eq!(decoded.bump, 253);
    }
}
