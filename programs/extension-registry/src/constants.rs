//! Program constants. The authority + treasury moved to `GlobalState` (set by
//! `initialize`) so they are testable and configurable; only the bond floor and
//! the extension-type taxonomy remain compiled in.

/// Minimum bond, in lamports (0.001 SOL). Bonds are SOL for the MVP (reuses the
/// lamport-vault pattern, no ATA); a USDC SPL vault is a later option.
pub const MIN_BOND_LAMPORTS: u64 = 1_000_000;

/// Cooldown window (7 days = 604,800 seconds) between `request_unbond` and
/// `withdraw_extension`. Prevents flash-bond sybil standing amplification and
/// front-running `default_extension` slashes (HIGH-16).
pub const UNBOND_COOLDOWN_SECS: i64 = 7 * 86_400;

/// Extension types (the verifiability taxonomy; sybil-layer 6). MVP records
/// CapabilityValidation only — the others gate on weaker-verifiability handling.
pub const EXTENSION_TYPE_CAPABILITY_VALIDATION: u8 = 0;
pub const EXTENSION_TYPE_MENTORSHIP: u8 = 1;
pub const EXTENSION_TYPE_SPONSORSHIP: u8 = 2;
pub const EXTENSION_TYPE_VALIDATION: u8 = 3;
