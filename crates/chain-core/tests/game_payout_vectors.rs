//! Cross-implementation golden vectors for the Coordination Game payoff matrix.
//!
//! WHAT THIS CLOSES
//! ----------------
//! The *cross-chain* outcome DERIVATION was already held equal across languages
//! (`tests/fixtures/outcome-derivation.json`, read by `CertLib.t.sol`). The
//! *same-chain money split* never was. `chain_core::game::amounts_for_kind` and
//! `evm/src/CoordinationGame.sol::_amounts` each decided who gets paid, and
//! nothing compared them — the two could disagree about a real payout and every
//! test on both sides would still pass.
//!
//! Shillbot already solved this shape: `task-payout-vectors.json` +
//! `ShillbotEscrowVectors.t.sol`. This is the same mechanism for the game.
//!
//! EXHAUSTIVE, NOT SAMPLED. The outcome domain is nine kinds, so every kind is
//! covered at every stake rather than a chosen few. Odd stakes are included
//! deliberately: `half = stake / 2` truncates, and the truncated lamport has to
//! land in the tournament's share instead of vanishing.
//!
//! To regenerate after an intentional rule change:
//!   cargo test -p chain-core --test game_payout_vectors -- --ignored regenerate

#[allow(dead_code)]
mod common;

use chain_core::game::{self, ALL_OUTCOME_KINDS};
use common::{assert_fixture_current, write_fixture};

const FIXTURE: &str = "game-payout-vectors.json";

/// Stakes worth pinning: the two live cluster values, plus small and odd ones
/// where integer division is most likely to leak.
const STAKES: [u64; 7] = [
    1,
    2,
    3,
    7,
    50_000_000,            // Solana devnet
    68_482_585,            // Solana mainnet — the 0.0027 ETH anchor
    2_700_000_000_000_000, // EVM stakeWei, both mainnets
];

fn render() -> String {
    let mut rows = Vec::new();
    for &stake in STAKES.iter() {
        for &kind in ALL_OUTCOME_KINDS.iter() {
            let p = game::amounts_for_kind(kind, stake)
                .unwrap_or_else(|e| panic!("kind {kind} stake {stake}: {e:?}"));
            let (p1_won, p2_won) =
                game::outcome_to_wins(kind).unwrap_or_else(|e| panic!("kind {kind}: {e:?}"));
            rows.push(format!(
                "    {{ \"kind\": {kind}, \"stake\": \"{stake}\", \"p1\": \"{}\", \"p2\": \"{}\", \"gain\": \"{}\", \"p1Won\": {p1_won}, \"p2Won\": {p2_won} }}",
                p.p1, p.p2, p.gain
            ));
        }
    }
    format!(
        "{{\n  \"comment\": \"Coordination Game payoff matrix. chain_core::game::amounts_for_kind is the source; CoordinationGame.sol::_amounts must agree. Amounts are decimal STRINGS because EVM stakes exceed u64 in wei terms. p1Won/p2Won come from outcome_to_wins and are DELIBERATELY not derivable from the amounts.\",\n  \"count\": {},\n  \"vectors\": [\n{}\n  ]\n}}\n",
        rows.len(),
        rows.join(",\n")
    )
}

#[test]
fn payout_vectors_are_current() {
    assert_fixture_current(FIXTURE, &render(), "game_payout_vectors::regenerate");
}

#[test]
#[ignore = "writes the fixture; run deliberately after a rule change"]
fn regenerate() {
    write_fixture(FIXTURE, &render());
}

/// Conservation restated against the emitted rows, so a bad GENERATOR cannot
/// quietly bless a bad matrix. The fixture is only trustworthy if the values in
/// it satisfy the invariant independently of the code that produced them.
#[test]
fn every_emitted_row_conserves_the_pot() {
    for &stake in STAKES.iter() {
        for &kind in ALL_OUTCOME_KINDS.iter() {
            let p = game::amounts_for_kind(kind, stake).unwrap();
            assert_eq!(
                u128::from(p.p1) + u128::from(p.p2) + u128::from(p.gain),
                u128::from(stake) * 2,
                "kind {kind} stake {stake}"
            );
        }
    }
}

/// The trap for whoever mirrors this in Solidity: a "win" is a correct read of
/// the opponent, not a profit. `HOMOG_BOTH_CORRECT` pays each player their own
/// stake back — zero net gain — and yet awards BOTH a win.
#[test]
fn wins_are_not_derivable_from_the_amounts() {
    let p = game::amounts_for_kind(game::HOMOG_BOTH_CORRECT, 100).unwrap();
    assert_eq!((p.p1, p.p2, p.gain), (100, 100, 0), "no net gain to either");
    assert_eq!(
        game::outcome_to_wins(game::HOMOG_BOTH_CORRECT).unwrap(),
        (true, true),
        "but both players earn a win"
    );
}
