use anchor_lang::prelude::*;

/// Session duration: 24 hours in seconds.
pub const SESSION_DURATION_SECONDS: i64 = 86_400;

/// Ephemeral session authority that lets a player delegate transaction signing
/// to a temporary keypair. The player signs once to create the session; all
/// subsequent game instructions can be signed by the session key instead.
///
/// PDA seeds: `["game_session", player, session_key]`
#[account]
pub struct SessionAuthority {
    /// The real wallet that created this session.
    pub player: Pubkey,
    /// The ephemeral keypair's public key.
    pub session_key: Pubkey,
    /// Unix timestamp after which the session is invalid.
    pub expires_at: i64,
    pub bump: u8,
}

impl SessionAuthority {
    // discriminator (8) + player (32) + session_key (32) + expires_at (8) + bump (1) = 81
    pub const SPACE: usize = 8 + 32 + 32 + 8 + 1;

    /// Returns true if the session has not expired relative to `now`.
    pub fn is_valid(&self, now: i64) -> bool {
        now < self.expires_at
    }

    /// Validate that this session authorizes the given player and session signer.
    pub fn validate_session(&self, player: &Pubkey, session_signer: &Pubkey, now: i64) -> bool {
        self.player == *player && self.session_key == *session_signer && self.is_valid(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_session(player: Pubkey, session_key: Pubkey, expires_at: i64) -> SessionAuthority {
        SessionAuthority {
            player,
            session_key,
            expires_at,
            bump: 255,
        }
    }

    #[test]
    fn space_matches_expected() {
        // discriminator (8) + player (32) + session_key (32) + expires_at (8) + bump (1)
        assert_eq!(SessionAuthority::SPACE, 81);
    }

    #[test]
    fn is_valid_before_expiry() {
        let pk = Pubkey::new_unique();
        let sk = Pubkey::new_unique();
        let session = make_session(pk, sk, 1_000_000);
        assert!(session.is_valid(999_999));
    }

    #[test]
    fn is_invalid_at_expiry() {
        let pk = Pubkey::new_unique();
        let sk = Pubkey::new_unique();
        let session = make_session(pk, sk, 1_000_000);
        assert!(!session.is_valid(1_000_000));
    }

    #[test]
    fn is_invalid_after_expiry() {
        let pk = Pubkey::new_unique();
        let sk = Pubkey::new_unique();
        let session = make_session(pk, sk, 1_000_000);
        assert!(!session.is_valid(1_000_001));
    }

    #[test]
    fn validate_session_accepts_valid() {
        let pk = Pubkey::new_unique();
        let sk = Pubkey::new_unique();
        let session = make_session(pk, sk, 1_000_000);
        assert!(session.validate_session(&pk, &sk, 500_000));
    }

    #[test]
    fn validate_session_rejects_wrong_player() {
        let pk = Pubkey::new_unique();
        let sk = Pubkey::new_unique();
        let other = Pubkey::new_unique();
        let session = make_session(pk, sk, 1_000_000);
        assert!(!session.validate_session(&other, &sk, 500_000));
    }

    #[test]
    fn validate_session_rejects_wrong_session_key() {
        let pk = Pubkey::new_unique();
        let sk = Pubkey::new_unique();
        let other = Pubkey::new_unique();
        let session = make_session(pk, sk, 1_000_000);
        assert!(!session.validate_session(&pk, &other, 500_000));
    }

    #[test]
    fn validate_session_rejects_expired() {
        let pk = Pubkey::new_unique();
        let sk = Pubkey::new_unique();
        let session = make_session(pk, sk, 1_000_000);
        assert!(!session.validate_session(&pk, &sk, 2_000_000));
    }

    #[test]
    fn session_duration_is_24_hours() {
        assert_eq!(SESSION_DURATION_SECONDS, 86_400);
    }
}
