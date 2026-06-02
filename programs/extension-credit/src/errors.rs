use anchor_lang::prelude::*;

#[error_code]
pub enum ExtensionCreditError {
    #[msg("Advance is below the minimum")]
    AdvanceTooLow,

    #[msg("A backer cannot advance to itself")]
    SelfAdvance,

    #[msg("Caller is not the advance's backer")]
    NotBacker,

    #[msg("Backer account does not match the advance's backer")]
    InvalidBacker,

    #[msg("Recipient account does not match the advance's recipient")]
    InvalidRecipient,

    #[msg("Advance is not fully recouped yet")]
    NotFullyRecouped,

    #[msg("Vault still holds undistributed earnings — crank route_and_recoup first")]
    VaultNotDrained,

    #[msg("Arithmetic overflow")]
    ArithmeticOverflow,

    #[msg("Direct lamport transfer attempted from an account not owned by this program")]
    InvalidLamportSource,
}
