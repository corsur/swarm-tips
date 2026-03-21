use crate::errors::CoordinationError;
use anchor_lang::prelude::*;

/// Transfer lamports directly between two program-owned or system accounts.
///
/// Used by reveal_guess, resolve_timeout, and claim_reward to move lamports
/// out of PDAs. The caller is responsible for ensuring `from` has sufficient
/// balance; this function only performs the checked arithmetic and borrow.
pub fn transfer_lamports(from: &AccountInfo, to: &AccountInfo, lamports: u64) -> Result<()> {
    if lamports == 0 {
        return Ok(());
    }
    **from.try_borrow_mut_lamports()? = from
        .lamports()
        .checked_sub(lamports)
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    **to.try_borrow_mut_lamports()? = to
        .lamports()
        .checked_add(lamports)
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    Ok(())
}
