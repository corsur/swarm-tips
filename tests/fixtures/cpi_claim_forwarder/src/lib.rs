use solana_program::{
    account_info::AccountInfo,
    entrypoint,
    entrypoint::ProgramResult,
    instruction::{AccountMeta, Instruction},
    program::invoke,
    program_error::ProgramError,
    pubkey::Pubkey,
};

entrypoint!(process_instruction);

pub fn process_instruction(
    _program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    let target_program = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;
    let forwarded_accounts = &accounts[1..];
    let metas: Vec<AccountMeta> = forwarded_accounts
        .iter()
        .map(|acc| {
            if acc.is_writable {
                AccountMeta::new(*acc.key, acc.is_signer)
            } else {
                AccountMeta::new_readonly(*acc.key, acc.is_signer)
            }
        })
        .collect();
    let ix = Instruction {
        program_id: *target_program.key,
        accounts: metas,
        data: instruction_data.to_vec(),
    };
    invoke(&ix, forwarded_accounts)
}
