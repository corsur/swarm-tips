//! PDA derivation for the coordination game program.
//!
//! Seeds must match the on-chain program exactly. All functions use
//! `coordination::ID` as the program ID.

use coordination::ID as PROGRAM_ID;
use solana_sdk::pubkey::Pubkey;

/// Derive the game account PDA for a given game ID.
///
/// Seeds: `["game", game_id.to_le_bytes()]`
pub fn game_pda(game_id: u64) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"game", &game_id.to_le_bytes()], &PROGRAM_ID)
}

/// Derive the tournament account PDA for a given tournament ID.
///
/// Seeds: `["tournament", tournament_id.to_le_bytes()]`
pub fn tournament_pda(tournament_id: u64) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"tournament", &tournament_id.to_le_bytes()], &PROGRAM_ID)
}

/// Derive the per-player escrow PDA for a tournament + wallet.
///
/// Seeds: `["escrow", tournament_id.to_le_bytes(), wallet]`
pub fn escrow_pda(tournament_id: u64, wallet: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[b"escrow", &tournament_id.to_le_bytes(), wallet.as_ref()],
        &PROGRAM_ID,
    )
}

/// Derive the player profile PDA for a tournament + wallet.
///
/// Seeds: `["player", tournament_id.to_le_bytes(), wallet]`
pub fn player_profile_pda(tournament_id: u64, wallet: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[b"player", &tournament_id.to_le_bytes(), wallet.as_ref()],
        &PROGRAM_ID,
    )
}

/// Derive the global config PDA (singleton, no variable seeds).
///
/// Seeds: `["global_config"]`
pub fn global_config_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"global_config"], &PROGRAM_ID)
}

/// Derive the game counter PDA (singleton, no variable seeds).
///
/// Seeds: `["game_counter"]`
pub fn game_counter_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"game_counter"], &PROGRAM_ID)
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- tournament_pda -------------------------------------------------------

    #[test]
    fn tournament_pda_is_deterministic() {
        let (addr1, bump1) = tournament_pda(42);
        let (addr2, bump2) = tournament_pda(42);
        assert_eq!(addr1, addr2);
        assert_eq!(bump1, bump2);
    }

    #[test]
    fn tournament_pda_differs_by_id() {
        let (addr1, _) = tournament_pda(1);
        let (addr2, _) = tournament_pda(2);
        assert_ne!(
            addr1, addr2,
            "different tournament IDs must yield different PDAs"
        );
    }

    #[test]
    fn tournament_pda_matches_manual_derivation() {
        let id: u64 = 7;
        let (addr, bump) = tournament_pda(id);
        let (expected, expected_bump) =
            Pubkey::find_program_address(&[b"tournament", &id.to_le_bytes()], &PROGRAM_ID);
        assert_eq!(addr, expected);
        assert_eq!(bump, expected_bump);
    }

    // -- game_pda -------------------------------------------------------------

    #[test]
    fn game_pda_is_deterministic() {
        let (addr1, bump1) = game_pda(100);
        let (addr2, bump2) = game_pda(100);
        assert_eq!(addr1, addr2);
        assert_eq!(bump1, bump2);
    }

    #[test]
    fn game_pda_differs_by_id() {
        let (addr1, _) = game_pda(0);
        let (addr2, _) = game_pda(1);
        assert_ne!(addr1, addr2, "different game IDs must yield different PDAs");
    }

    #[test]
    fn game_pda_matches_manual_derivation() {
        let id: u64 = 999;
        let (addr, bump) = game_pda(id);
        let (expected, expected_bump) =
            Pubkey::find_program_address(&[b"game", &id.to_le_bytes()], &PROGRAM_ID);
        assert_eq!(addr, expected);
        assert_eq!(bump, expected_bump);
    }

    #[test]
    fn game_pda_boundary_values() {
        let (addr_zero, _) = game_pda(0);
        let (addr_max, _) = game_pda(u64::MAX);
        assert_ne!(addr_zero, addr_max, "game_pda(0) != game_pda(u64::MAX)");
    }

    // -- player_profile_pda ---------------------------------------------------

    #[test]
    fn player_profile_pda_is_deterministic() {
        let wallet = Pubkey::new_unique();
        let (addr1, bump1) = player_profile_pda(1, &wallet);
        let (addr2, bump2) = player_profile_pda(1, &wallet);
        assert_eq!(addr1, addr2);
        assert_eq!(bump1, bump2);
    }

    #[test]
    fn player_profile_pda_differs_by_wallet() {
        let w1 = Pubkey::new_unique();
        let w2 = Pubkey::new_unique();
        let (addr1, _) = player_profile_pda(1, &w1);
        let (addr2, _) = player_profile_pda(1, &w2);
        assert_ne!(
            addr1, addr2,
            "different wallets must yield different profile PDAs"
        );
    }

    #[test]
    fn player_profile_pda_differs_by_tournament() {
        let wallet = Pubkey::new_unique();
        let (addr1, _) = player_profile_pda(1, &wallet);
        let (addr2, _) = player_profile_pda(2, &wallet);
        assert_ne!(
            addr1, addr2,
            "different tournaments must yield different profile PDAs"
        );
    }

    #[test]
    fn player_profile_pda_matches_manual_derivation() {
        let wallet = Pubkey::new_unique();
        let tid: u64 = 5;
        let (addr, bump) = player_profile_pda(tid, &wallet);
        let (expected, expected_bump) = Pubkey::find_program_address(
            &[b"player", &tid.to_le_bytes(), wallet.as_ref()],
            &PROGRAM_ID,
        );
        assert_eq!(addr, expected);
        assert_eq!(bump, expected_bump);
    }

    // -- escrow_pda -----------------------------------------------------------

    #[test]
    fn escrow_pda_is_deterministic() {
        let wallet = Pubkey::new_unique();
        let (addr1, bump1) = escrow_pda(1, &wallet);
        let (addr2, bump2) = escrow_pda(1, &wallet);
        assert_eq!(addr1, addr2);
        assert_eq!(bump1, bump2);
    }

    #[test]
    fn escrow_pda_differs_by_wallet() {
        let w1 = Pubkey::new_unique();
        let w2 = Pubkey::new_unique();
        let (addr1, _) = escrow_pda(1, &w1);
        let (addr2, _) = escrow_pda(1, &w2);
        assert_ne!(
            addr1, addr2,
            "different wallets must yield different escrow PDAs"
        );
    }

    #[test]
    fn escrow_pda_differs_by_tournament() {
        let wallet = Pubkey::new_unique();
        let (addr1, _) = escrow_pda(1, &wallet);
        let (addr2, _) = escrow_pda(2, &wallet);
        assert_ne!(
            addr1, addr2,
            "different tournaments must yield different escrow PDAs"
        );
    }

    #[test]
    fn escrow_pda_matches_manual_derivation() {
        let wallet = Pubkey::new_unique();
        let tid: u64 = 10;
        let (addr, bump) = escrow_pda(tid, &wallet);
        let (expected, expected_bump) = Pubkey::find_program_address(
            &[b"escrow", &tid.to_le_bytes(), wallet.as_ref()],
            &PROGRAM_ID,
        );
        assert_eq!(addr, expected);
        assert_eq!(bump, expected_bump);
    }

    // -- global_config_pda ----------------------------------------------------

    #[test]
    fn global_config_pda_is_deterministic() {
        let (addr1, bump1) = global_config_pda();
        let (addr2, bump2) = global_config_pda();
        assert_eq!(addr1, addr2);
        assert_eq!(bump1, bump2);
    }

    #[test]
    fn global_config_pda_matches_manual_derivation() {
        let (addr, bump) = global_config_pda();
        let (expected, expected_bump) =
            Pubkey::find_program_address(&[b"global_config"], &PROGRAM_ID);
        assert_eq!(addr, expected);
        assert_eq!(bump, expected_bump);
    }

    // -- game_counter_pda -----------------------------------------------------

    #[test]
    fn game_counter_pda_is_deterministic() {
        let (addr1, bump1) = game_counter_pda();
        let (addr2, bump2) = game_counter_pda();
        assert_eq!(addr1, addr2);
        assert_eq!(bump1, bump2);
    }

    #[test]
    fn game_counter_pda_matches_manual_derivation() {
        let (addr, bump) = game_counter_pda();
        let (expected, expected_bump) =
            Pubkey::find_program_address(&[b"game_counter"], &PROGRAM_ID);
        assert_eq!(addr, expected);
        assert_eq!(bump, expected_bump);
    }

    // -- cross-type distinctness ----------------------------------------------

    #[test]
    fn all_pda_types_produce_distinct_addresses_for_same_inputs() {
        let wallet = Pubkey::new_unique();
        let id: u64 = 1;
        let (game_addr, _) = game_pda(id);
        let (tournament_addr, _) = tournament_pda(id);
        let (profile_addr, _) = player_profile_pda(id, &wallet);
        let (escrow_addr, _) = escrow_pda(id, &wallet);
        let (global_config_addr, _) = global_config_pda();
        let (game_counter_addr, _) = game_counter_pda();

        let addrs = [
            game_addr,
            tournament_addr,
            profile_addr,
            escrow_addr,
            global_config_addr,
            game_counter_addr,
        ];
        for i in 0..addrs.len() {
            for j in (i + 1)..addrs.len() {
                assert_ne!(addrs[i], addrs[j], "PDA type {i} and {j} must not collide");
            }
        }
    }
}
