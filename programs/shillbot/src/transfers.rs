use anchor_lang::prelude::*;

use crate::errors::ShillbotError;

/// Transfer lamports from a PDA by directly adjusting lamport balances.
///
/// The source account MUST be owned by this program. Direct lamport mutation
/// is only sound on accounts owned by the executing program — the runtime
/// enforces this at exit, and we enforce it explicitly here so the safety
/// contract is part of the function, not caller-discipline.
pub fn transfer_lamports(from: &AccountInfo, to: &AccountInfo, amount: u64) -> Result<()> {
    require!(*from.owner == crate::ID, ShillbotError::InvalidTaskState);

    let from_lamports = from.lamports();
    let to_lamports = to.lamports();

    let new_from = from_lamports
        .checked_sub(amount)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    let new_to = to_lamports
        .checked_add(amount)
        .ok_or(ShillbotError::ArithmeticOverflow)?;

    **from.try_borrow_mut_lamports()? = new_from;
    **to.try_borrow_mut_lamports()? = new_to;

    Ok(())
}
