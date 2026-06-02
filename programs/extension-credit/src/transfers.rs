use anchor_lang::prelude::*;

use crate::errors::ExtensionCreditError;

/// Transfer lamports out of a PDA owned by this program by directly adjusting
/// lamport balances. Mirrors the shillbot / extension-registry pattern: direct
/// lamport mutation is only sound on accounts owned by the executing program,
/// enforced here explicitly.
pub fn transfer_lamports(from: &AccountInfo, to: &AccountInfo, amount: u64) -> Result<()> {
    require!(
        *from.owner == crate::ID,
        ExtensionCreditError::InvalidLamportSource
    );

    let from_lamports = from.lamports();
    let to_lamports = to.lamports();

    let new_from = from_lamports
        .checked_sub(amount)
        .ok_or(ExtensionCreditError::ArithmeticOverflow)?;
    let new_to = to_lamports
        .checked_add(amount)
        .ok_or(ExtensionCreditError::ArithmeticOverflow)?;

    **from.try_borrow_mut_lamports()? = new_from;
    **to.try_borrow_mut_lamports()? = new_to;

    Ok(())
}
