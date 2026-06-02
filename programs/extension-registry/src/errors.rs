use anchor_lang::prelude::*;

#[error_code]
pub enum ExtensionRegistryError {
    #[msg("Extension type is not supported in the MVP (CapabilityValidation only)")]
    UnsupportedExtensionType,

    #[msg("Bond is below the minimum")]
    BondTooLow,

    #[msg("An extender cannot extend to itself")]
    SelfExtension,

    #[msg("Caller is not the extension's extender")]
    NotExtender,

    #[msg("Caller is not the registry authority")]
    NotAuthority,

    #[msg("Authority pubkey is invalid (default)")]
    InvalidAuthority,

    #[msg("Treasury account does not match the configured treasury")]
    InvalidTreasury,

    #[msg("Recipient account does not match the extension's recipient")]
    InvalidRecipient,

    #[msg("Extender account does not match the extension's extender")]
    InvalidExtender,

    #[msg("Arithmetic overflow")]
    ArithmeticOverflow,

    #[msg("Direct lamport transfer attempted from an account not owned by this program")]
    InvalidLamportSource,
}
