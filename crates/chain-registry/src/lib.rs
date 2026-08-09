#![deny(warnings)]
#![deny(clippy::all)]
#![deny(clippy::arithmetic_side_effects)]

//! The chain registry — single source of truth for every per-chain value
//! (same rule as `game-constants`). Adding a chain is an entry here, not
//! a code change anywhere else. Per the root CLAUDE.md "Multichain
//! Frameworks" standard: no per-chain hardcoded constants outside this
//! crate.

use chain_core::{ChainId, Namespace};

/// How "final" is defined on a chain — the deploy-precondition finality
/// table from `multichain/decision.md` §4.1. Client quorum reads pin to
/// this level before signing a match-live certificate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finality {
    /// Solana `finalized` commitment.
    SolanaFinalized,
    /// EVM `finalized` block tag (post-merge two-epoch finality; on an
    /// L2 this is the sequencer's finalized tag — L1-posting nuance is
    /// testnet-acceptable and revisited at the mainnet gate).
    EvmFinalizedTag,
}

/// A deployed contract's role on a chain, so one chain can expose several (an
/// EVM chain hosts BOTH the cross-chain `CrossChainGame` and the same-chain
/// `CoordinationGame`). The generalized lookup every product shares: adding a
/// future product (e.g. a Shillbot EVM escrow) is a new variant + an address on
/// the relevant entries — no consumer change. See `contract_for`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractPurpose {
    /// The cross-chain match contract — Solana coordination-game program, or the
    /// EVM `CrossChainGame`.
    CrossChainGame,
    /// The same-chain EVM-vs-EVM `CoordinationGame`.
    CoordinationGame,
    /// The Shillbot task-escrow — Solana shillbot program, or the EVM
    /// `ShillbotEscrow`.
    ShillbotEscrow,
}

// ===========================================================================
// WHAT THE GAME COSTS — the unit of account is ETH, not USD.
// ===========================================================================
//
// A match costs STAKE_ANCHOR_WEI. Every other surface converts to it.
//
//   STAKE_ANCHOR_WEI  (0.0027 ETH)
//          │
//          ├─ EVM same-chain  CoordinationGame.stakeWei   ── holds it LITERALLY
//          ├─ EVM cross-chain CrossChainGame.stakeWei      ── holds it LITERALLY
//          │      (both: `msg.value != stakeWei` reverts — CANNOT float)
//          │
//          └─ Solana ── × live SOL/ETH ratio ──┬─ same-chain  GlobalConfig
//                                              └─ cross-chain create_xmatch arg
//
// WHY ETH AND NOT USD. Of the four surfaces above, exactly ONE can carry a
// per-match stake (`create_xmatch` takes `stake_lamports`); the other three are
// pinned to a contract or program config. So one side is necessarily static and
// the other necessarily floats, and the static side is EVM. ETH is therefore the
// anchor by CONSTRAINT, not preference.
//
// The conversion needs only the SOL/ETH RATIO. USD cancels, so a common-mode
// error in the price feed cannot skew the peg.
//
// COROLLARY, so nobody "fixes" it later: if ETH doubles in dollars, the game
// costs twice as many dollars. That is what denominating in ETH means. It is
// not drift and there is nothing to correct.
//
// TWO INVARIANTS, deliberately separate — conflating them is how an absolute-USD
// anchor got shipped and reverted on 2026-08-04:
//
//   MATCH PARITY   the two players in ONE match stake the same amount.
//                  Enforced on-chain. Must NEVER depend on config freshness,
//                  because a stale config would then block settlement.
//   ANCHOR PARITY  every surface is worth STAKE_ANCHOR_WEI. Exact for EVM (they
//                  hold it literally), a live conversion for Solana. Drift here
//                  is a PRICING concern and must never block a match.
//
// This constant prices NOTHING at runtime. Cross-chain derives the Solana leg
// from the EVM leg (game-api `dynamic_sol_stake`) so a match's two legs are equal
// by construction. What lives here is the target each config is pegged to.
//
// BEFORE COMPARING ANYTHING IN THIS REGISTRY, FILTER BY NETWORK CLASS. Use
// `mainnet()` / `testnet()`, never bare `all()`. Testnet stakes are nominal and
// USD-untuned; comparing Base Sepolia's 0.0032 ETH against a mainnet entry is
// meaningless, and doing exactly that produced a "Base is mispriced at $5.98"
// claim that was asserted twice before anyone checked which network it was on.
pub const STAKE_ANCHOR_WEI: u128 = 2_700_000_000_000_000;

/// How a surface's stake is FIXED — i.e. whether it can carry a per-match amount.
///
/// This was tribal knowledge spread across three files, and not knowing it is
/// what produced an absolute-USD anchor that had to be reverted: only ONE of the
/// four game surfaces can float, so the other three necessarily define the price
/// and the floating one necessarily follows.
///
/// It is a property of (namespace, purpose) rather than of a chain, because one
/// Solana entry hosts both a fixed same-chain surface and a floating cross-chain
/// one. See `ChainEntry::stake_binding`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StakeBinding {
    /// EVM contracts. `msg.value != stakeWei` reverts, so the amount is whatever
    /// the deployed config says and cannot vary per match. Changing it is an
    /// owner `setConfig`, converged from this registry by `Reconcile EVM Stake`.
    ContractConfig,
    /// Solana same-chain. `deposit_stake` takes no amount — it reads
    /// `GlobalConfig.stake_lamports` via `live_stake()`. Also cannot vary per
    /// match, converged by `Reconcile Solana Stake`.
    ProgramConfig,
    /// Solana cross-chain. `create_xmatch(stake_lamports)` takes the amount as a
    /// matchmaker-supplied argument. THE ONLY SURFACE THAT CAN FLOAT, which is
    /// why the Solana leg is the one priced against the anchor.
    PerMatchQuoted,
}

impl StakeBinding {
    /// Can this surface carry a stake decided per match?
    ///
    /// Guards the mistake directly: anything that computes a floating stake must
    /// check this first, rather than assuming a leg it can price is a leg the
    /// chain will accept.
    pub fn can_float(self) -> bool {
        matches!(self, StakeBinding::PerMatchQuoted)
    }
}

/// One chain's complete configuration.
#[derive(Debug, Clone)]
pub struct ChainEntry {
    /// CAIP-2 string, e.g. `eip155:84532`.
    pub chain_id: &'static str,
    pub display_name: &'static str,
    /// Independent RPC endpoints for M-of-N quorum reads. Disagreement
    /// at the pinned finality level → refuse to sign match-live.
    pub rpc_urls: &'static [&'static str],
    /// Minimum agreeing providers for a quorum read.
    pub quorum_m: usize,
    pub finality: Finality,
    /// True for real-money chains (Solana mainnet, Base, Ethereum). Gates
    /// real-money-only guards — e.g. the cross-chain FX sanity band, which is
    /// meaningless on testnets whose stakes are nominal rather than USD-tuned.
    pub is_mainnet: bool,
    pub native_symbol: &'static str,
    pub native_decimals: u8,
    /// Per-match stake in native base units (lamports / wei), tuned to
    /// rough USD parity across chains (config, not oracle).
    pub stake_base_units: u128,
    /// The USD value `stake_base_units` was pegged to, in cents, and the native
    /// price (also cents) it was converted at.
    ///
    /// These exist because the intent used to live only in a code comment, and
    /// nothing checked it. Three EVM anchors ended up pegged at three different
    /// ETH prices — $1,562 (Base Sepolia), $3,000 (Base mainnet), $1,600
    /// (Ethereum mainnet) — so the same product costs 5x more on one mainnet
    /// than another, and no test noticed. `stake_pegs_are_internally_consistent`
    /// now checks the literal against these two numbers, so a stake can no
    /// longer be edited without restating what it is supposed to be worth.
    pub stake_usd_cents: u32,
    /// Native-token price in USD cents used when the stake above was pegged.
    /// NOT a live rate — the honest record of what we assumed, so drift is
    /// measurable instead of invisible.
    pub peg_native_usd_cents: u32,
    /// Float-pool per-match tranche clamp (panel requirement A3).
    pub max_tranche_base_units: u128,
    /// Contested-claim window. Claims close at match_deadline + this;
    /// refundTimeout opens 2×skew_margin_secs later on BOTH legs.
    pub claim_window_secs: u32,
    pub skew_margin_secs: u32,
    /// Cross-chain match contract: coordination-game program ID (solana) or
    /// `CrossChainGame` address (eip155). None until deployed on that chain.
    /// Prefer `contract_for(ContractPurpose::CrossChainGame)` in new code.
    pub game_contract: Option<&'static str>,
    /// Same-chain EVM-vs-EVM `CoordinationGame` address (eip155). None where it
    /// isn't deployed — every Solana entry, and an EVM chain until deployed.
    pub coordination_game_contract: Option<&'static str>,
    /// CoordinationGameV4 PROXY address (UUPS). v4 adds seasons + a merkle
    /// prize claim. Where this is set it is what `contract_for` returns and
    /// therefore what every service plays on; `coordination_game_contract`
    /// keeps the superseded v3 address so historical games stay indexable.
    pub coordination_game_v4_proxy: Option<&'static str>,
    /// Shillbot task-escrow: shillbot program ID (solana) or `ShillbotEscrow`
    /// address (eip155). None until deployed on that chain. Today the ONLY
    /// entry with an address is Base Sepolia — the Solana entries and every EVM
    /// mainnet entry are still None.
    pub shillbot_escrow_contract: Option<&'static str>,
    /// x402 network descriptor name, when this chain settles payments.
    pub x402_network: Option<&'static str>,
}

impl ChainEntry {
    /// The deployed contract for a purpose on this chain, if any. The
    /// generalized lookup all products share — a future product is a new
    /// `ContractPurpose` variant mapped to its address field.
    pub fn contract_for(&self, purpose: ContractPurpose) -> Option<&'static str> {
        match purpose {
            ContractPurpose::CrossChainGame => self.game_contract,
            // THE v3 -> v4 CUTOVER, in one place. Every consumer of the
            // same-chain game resolves its address through here (game-api's
            // `coordination_game_contract()`, mcp-server's
            // `resolve_coordination_game_contract()`), so preferring the v4
            // proxy switches all of them together and cannot leave one service
            // talking to v3 while another talks to v4 — a split that would put
            // two players in the same logical match on different contracts.
            //
            // Falls back to v3 where no v4 proxy is deployed, so a chain that
            // has not been cut over keeps working unchanged.
            //
            // NOT a cutover for INDEXING: reading only v4 would drop every
            // game recorded against v3. `leaderboard_io::index_chain_since_cursor`
            // deliberately reads the address fields directly and scans BOTH.
            ContractPurpose::CoordinationGame => self
                .coordination_game_v4_proxy
                .or(self.coordination_game_contract),
            ContractPurpose::ShillbotEscrow => self.shillbot_escrow_contract,
        }
    }

    /// Whether a product is LIVE (deployed) on this chain. A deployed contract
    /// address IS the deployment truth — a chain scaffolded here with a `None`
    /// address is "coming soon", not playable. Backend and frontend gate every
    /// real transaction on this; no separate `deployed` bool that could drift
    /// from the address (single source of truth). A mainnet entry ships with
    /// `None` and flips live the moment the follow-up commit records its address.
    pub fn is_live(&self, purpose: ContractPurpose) -> bool {
        self.contract_for(purpose).is_some()
    }
}

/// Named CAIP-2 ids for the registered chains — THE single source for these
/// strings in Rust. Consumers (game-api leaderboard, mcp-server xchain, …)
/// import these instead of repeating the literals.
pub const SOLANA_DEVNET_CAIP2: &str = "solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1";
pub const SOLANA_MAINNET_CAIP2: &str = "solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp";
pub const BASE_SEPOLIA_CAIP2: &str = "eip155:84532";
pub const BASE_MAINNET_CAIP2: &str = "eip155:8453";
pub const ETHEREUM_SEPOLIA_CAIP2: &str = "eip155:11155111";
pub const ETHEREUM_MAINNET_CAIP2: &str = "eip155:1";

/// Testnet stake parity note: 0.05 SOL and 0.0025 ETH are within the
/// same rough-USD band; exact parity is intentionally NOT enforced
/// (rates are agreed per-match in the co-signed schedule).
const REGISTRY: &[ChainEntry] = &[
    ChainEntry {
        chain_id: SOLANA_DEVNET_CAIP2,
        display_name: "Solana Devnet",
        rpc_urls: &["https://api.devnet.solana.com"],
        quorum_m: 1,
        finality: Finality::SolanaFinalized,
        is_mainnet: false,
        native_symbol: "SOL",
        native_decimals: 9,
        // Cross-chain leg parity with the $5 EVM anchor (0.0032 ETH): ~0.068 SOL
        // at SOL≈$73. Only the xchain Solana leg reads this (same-chain Solana
        // uses the on-chain FIXED_STAKE constant); create_xmatch takes stake as
        // an arg (no fixed on-chain stake), and 0.068 ≤ the live xpool's 0.1-SOL
        // max_tranche, so no Solana redeploy or pool reconfig is needed.
        // PINNED TO DEFAULT_STAKE_LAMPORTS (0.05 SOL), NOT the $5 anchor.
        //
        // create_game requires `escrow.amount == global_config.stake_lamports`,
        // but neither deployed client passes the optional global_config account
        // on deposit_stake — the frontend uses .accounts() only, and
        // coordination-app pins game-chain at def85b47, which predates the
        // append. So both deposit at DEFAULT_STAKE_LAMPORTS. Configuring
        // anything else here makes every Solana devnet game fail StakeMismatch
        // (0x1776) at creation: clients can deposit and can never play.
        //
        // The optional-account design removed the need to coordinate a client
        // release for the ACCOUNT change; it does not remove it for a VALUE
        // change. Raising this to the $5 anchor requires the frontend to send
        // remainingAccounts and coordination-app to bump the game-chain pin.
        // Until then this stays equal to the compile-time default.
        stake_base_units: 50_000_000, // 0.05 SOL == DEFAULT_STAKE_LAMPORTS
        stake_usd_cents: 364,         // $3.64
        peg_native_usd_cents: 7286,
        max_tranche_base_units: 100_000_000, // 0.1 SOL (unchanged; == live xpool max_tranche)
        claim_window_secs: 3_600,
        skew_margin_secs: 900,
        game_contract: Some("2qqVk7kUqffnahiJpcQJCsSd8ErbEUgKTgCn1zYsw64P"),
        coordination_game_contract: None,
        coordination_game_v4_proxy: None, // same-chain EVM game has no Solana deployment
        shillbot_escrow_contract: None,   // not yet deployed on this chain
        x402_network: None,
    },
    ChainEntry {
        chain_id: SOLANA_MAINNET_CAIP2,
        display_name: "Solana Mainnet",
        rpc_urls: &["https://api.mainnet-beta.solana.com"],
        quorum_m: 1,
        finality: Finality::SolanaFinalized,
        is_mainnet: true,
        native_symbol: "SOL",
        native_decimals: 9,
        // PINNED TO DEFAULT_STAKE_LAMPORTS for the same reason as devnet above:
        // no deployed client passes the optional global_config account, so both
        // deposit 0.05 SOL regardless of what is configured here. Mainnet was
        // left at the $5 anchor when devnet was pinned back — a per-entity fix
        // to a cross-entity defect — which made the mainnet game UNPLAYABLE:
        // deposit_stake takes the 0.05, then create_game rejects it with
        // StakeMismatch (0x1776). Recoverable (withdraw_stake only requires
        // amount > 0, so nothing is stranded) but a live outage.
        //
        // This is knowingly $3.64 against a $5 EVM anchor. Playable-and-cheap
        // beats correctly-priced-and-broken; the divergence is the SECOND
        // problem, and closing it needs the client work named above, not
        // another edit here. `check-stake-parity.mjs` now compares the client
        // constants and will fail if anyone raises this first.
        stake_base_units: 68_482_585, // 0.0685 SOL — the 0.0027 ETH anchor at SOL/ETH 25.3639
        stake_usd_cents: 504,         // $5.04
        peg_native_usd_cents: 7361,
        max_tranche_base_units: 100_000_000,
        claim_window_secs: 3_600,
        skew_margin_secs: 900,
        game_contract: Some("2qqVk7kUqffnahiJpcQJCsSd8ErbEUgKTgCn1zYsw64P"),
        coordination_game_contract: None,
        coordination_game_v4_proxy: None,
        shillbot_escrow_contract: None, // not yet deployed on this chain
        x402_network: Some("solana"),
    },
    ChainEntry {
        chain_id: BASE_SEPOLIA_CAIP2,
        display_name: "Base Sepolia",
        rpc_urls: &[
            "https://sepolia.base.org",
            "https://base-sepolia-rpc.publicnode.com",
            "https://base-sepolia.drpc.org",
        ],
        quorum_m: 2,
        finality: Finality::EvmFinalizedTag,
        is_mainnet: false,
        native_symbol: "ETH",
        native_decimals: 18,
        // Testnet: MUST match the deployed CrossChainGame's stakeWei/maxTrancheWei
        // — createMatch records exactly stakeWei, so the cert's leg_b.stake must
        // equal it or settle's digest check fails. Set to the SETTLED $5 anchor
        // (0.0032 / 0.0064 ETH) at the audit-fix redeploy; the Solana devnet leg
        // ABOVE (line ~123) is sized to match (~0.068 SOL ≈ $5). Bump both the deploy config
        // (deploy-evm-testnet.yml XCHAIN_STAKE_WEI/XCHAIN_MAX_TRANCHE_WEI) and
        // this entry in lockstep — a registry-only change breaks the live e2e.
        stake_base_units: 3_200_000_000_000_000, // 0.0032 ETH ($5 anchor, == deployed stakeWei)
        stake_usd_cents: 500,                    // $5.00
        peg_native_usd_cents: 156250,
        max_tranche_base_units: 6_400_000_000_000_000, // 0.0064 ETH (== deployed maxTrancheWei)
        claim_window_secs: 3_600,
        skew_margin_secs: 900,
        // CrossChainGame redeployed 2026-07-01 with the audit-fix hardening
        // (M1 pull-payment, M2/M4 snapshots + createMatch operator-sig, L1 config
        // bounds) at the $5 stake (operatorSigner 0x54a6…9A30; prior tiny-stake
        // 0xd585…6234 orphaned by this redeploy).
        game_contract: Some("0xd38b1fB07Bf64801bCBc3721937D6e2Ba6E5feb4"),
        // Same-chain EVM-vs-EVM CoordinationGame v3, redeployed 2026-07-29 with
        // wallet-as-player staking (Level 2): openSession registers + funds a gas-
        // only session key in one wallet tx; createGame/joinGame take an `address
        // player` (the WALLET, recorded as game.player1/2 even when the session key
        // sends the tx); withdrawFor pushes winnings to the wallet with no popup.
        // The wallet is now the native on-chain player (no off-chain binding). Same
        // config ($5 stake, operatorSigner 0x54a6…9A30 == game-api signer). Prior
        // v2 0x50dB…733F (session-auth only) orphaned by this redeploy; Base MAINNET
        // stays on 0x778F…9fe9 pending external audit + founder go (cutover note).
        coordination_game_contract: Some("0x9E344F6FD80f4b2a20329a8C0dD4E16f70Bcd5ED"),
        coordination_game_v4_proxy: Some("0x4FBBceb96D2814b5d4ac26089Eb7E43471533253"),
        // ShillbotEscrow deployed 2026-07-07 (S5 live demo): chainTag
        // keccak256("eip155:84532"), attesterSigner is a demo key — rotate to
        // the dedicated shillbot-attester EVM key via setConfig before real use.
        shillbot_escrow_contract: Some("0xaFe061778f9A76fCe7da4124dC89DAF8309E5F3c"),
        x402_network: Some("base-sepolia"),
    },
    // Ethereum Sepolia — second EVM testnet, added for full testnet parity so
    // every game combination (same-chain EVM + Solana↔Ethereum / Base↔Ethereum
    // cross-chain) is verifiable with test funds, no real money. SCAFFOLDED:
    // contracts are None until the `ethereum_sepolia` deploy lands (dispatch
    // deploy-evm-testnet.yml network=ethereum_sepolia), at which point a
    // follow-up commit records the addresses and the parity guard validates
    // stakeWei/maxTrancheWei. Stake matches Base Sepolia's $5 testnet anchor
    // (== _deploy-evm.yml ethereum_sepolia XCHAIN_STAKE_WEI, in lockstep).
    ChainEntry {
        chain_id: BASE_MAINNET_CAIP2,
        display_name: "Base",
        rpc_urls: &[
            "https://mainnet.base.org",
            "https://base-rpc.publicnode.com",
            "https://base.drpc.org",
        ],
        quorum_m: 2,
        finality: Finality::EvmFinalizedTag,
        is_mainnet: true,
        native_symbol: "ETH",
        native_decimals: 18,
        // Low launch stake (~$1.50 at ETH≈$3000) for the initial minimal-budget
        // Base-mainnet run (~20 games). Base L2 gas is cents, so even a tiny stake
        // is economically sane here (gas ≪ stake). Bump post-launch as desired;
        // keep in lockstep with the deploy workflow's base XCHAIN_STAKE_WEI.
        stake_base_units: 2_700_000_000_000_000, // 0.0027 ETH ($5 anchor, == deployed stakeWei)
        stake_usd_cents: 500,                    // $5.00
        peg_native_usd_cents: 185808,
        max_tranche_base_units: 5_400_000_000_000_000, // 0.0054 ETH (2x stake)
        claim_window_secs: 3_600,
        skew_margin_secs: 900,
        // Cross-chain CrossChainGame deployed to Base mainnet 2026-07-11 (0.0005
        // ETH stake; owner/operatorSigner = xchain key / xchain-operator-signer).
        game_contract: Some("0xC2DbD950400965b3f4d9A4D6B1af4a0eb65CC365"),
        // Same-chain CoordinationGame v3 (wallet-as-player: openSession +
        // player-param createGame/joinGame + withdrawFor) deployed to Base
        // mainnet 2026-07-30 via deploy-evm-mainnet.yml, founder-authorized.
        // Supersedes v1 0x778F…9fe9 (2026-07-09, session-key-as-player), which
        // the Level 2 client flow can no longer drive (no openSession); v1
        // stays live for its own residual state. Config verified on-chain:
        // owner/treasury 0x9962…770d, operatorSigner 0x54a6…9A30, stake 5e14,
        // split 5000.
        coordination_game_contract: Some("0x567e114EB53228aFd9b20d7121668D4ce082a4F8"),
        // v4 (UUPS proxy). Deployed and verified live: owner 0x996213ed..9770d,
        // stakeWei 0.0027, unpaused, season 1 open for 365 days. v3 above is
        // retained for residual state — it held 0 ETH at cutover, so no escrowed
        // stake or in-flight game was stranded by re-pointing.
        coordination_game_v4_proxy: Some("0xd585baE48901513202dAEb7d4feE4Af508a96234"),
        shillbot_escrow_contract: None,
        x402_network: Some("base"),
    },
    ChainEntry {
        chain_id: ETHEREUM_MAINNET_CAIP2,
        display_name: "Ethereum",
        rpc_urls: &[
            "https://ethereum-rpc.publicnode.com",
            "https://eth.drpc.org",
            "https://cloudflare-eth.com",
        ],
        quorum_m: 2,
        finality: Finality::EvmFinalizedTag,
        is_mainnet: true,
        native_symbol: "ETH",
        native_decimals: 18,
        // Anchored to the decided cross-chain peg: 0.0025 ETH ≈ $4 ≈ 0.05 SOL
        // (decision.md §4.1), so an Ethereum same-chain game costs the same as
        // Solana. For CROSS-chain matches the Solana leg is priced dynamically
        // off this EVM anchor at match time (game-api `build_xmatch_terms`), so
        // both legs are always equal in live USD. Re-tune this literal only at
        // (re)deploy; keep it in lockstep with `_deploy-evm.yml` ethereum stake.
        // Note: L1 gas can rival the stake during congestion — acceptable per
        // founder (uniform pricing preferred over gas-minimized L1 stakes).
        stake_base_units: 2_700_000_000_000_000, // 0.0027 ETH ($5 anchor, == deployed stakeWei)
        stake_usd_cents: 500,                    // $5.00
        peg_native_usd_cents: 185808,
        max_tranche_base_units: 5_400_000_000_000_000, // 0.0054 ETH (2x stake)
        claim_window_secs: 3_600,
        skew_margin_secs: 900,
        // CrossChainGame deployed to Ethereum L1 mainnet 2026-07-11 (0.0025 ETH
        // stake; owner/operatorSigner = xchain key / xchain-operator-signer).
        game_contract: Some("0x5E9eb986927bDF70F2f9fE5BccAFF3dEE74949EB"),
        // Same-chain CoordinationGame v3 (wallet-as-player) deployed to Ethereum
        // mainnet 2026-07-30 via deploy-evm-mainnet.yml, founder-authorized.
        // Supersedes v1 0xd52a…05B3 (2026-07-11), which the Level 2 client flow
        // can no longer drive (no openSession); v1 stays live for its own
        // residual state. Verified on-chain: owner/treasury 0x9962…770d,
        // operatorSigner 0x54a6…9A30, stake 2.5e15, split 5000.
        coordination_game_contract: Some("0x1b75ddB73ebAC8aD7C0B26787B534e7Db0e7917d"),
        // v4 (UUPS proxy). Season 1 deployed with a 900s window by the
        // V4_SEASON_SECS precedence bug (692bde6); corrected on-chain with
        // startSeason(2, 31536000), so the LIVE season here is 2, not 1.
        coordination_game_v4_proxy: Some("0x265818b054E8413Bab870e0Ce0D8aB68400CF0F9"),
        shillbot_escrow_contract: None,
        x402_network: None,
    },
];

/// Look up a chain's configuration. None = chain not supported; callers
/// at system boundaries reject rather than guess.
/// Candidate read RPCs for `entry`: the optional premium endpoint from env
/// `EVM_RPC_URL_<CAIP2>` (e.g. `EVM_RPC_URL_EIP155_84532`) first, then the
/// registry's public endpoints as fallback.
///
/// The premium URL is a SECRET, so it lives in Secret Manager and reaches the
/// process as an env var — never in this crate, which is public. This function
/// only reads the variable; it never holds a key.
///
/// Every service that reads an EVM chain should go through here. Taking
/// `entry.rpc_urls.first()` directly silently pins the caller to the public
/// endpoints even when a premium URL is configured, which costs both
/// reliability under load and getLogs range (the official Base endpoints cap
/// at 2,000 blocks).
pub fn read_rpc_urls(entry: &ChainEntry) -> Vec<String> {
    let env_key = format!(
        "EVM_RPC_URL_{}",
        entry.chain_id.replace([':', '-'], "_").to_ascii_uppercase()
    );
    std::env::var(&env_key)
        .ok()
        .into_iter()
        .chain(entry.rpc_urls.iter().map(|s| (*s).to_string()))
        .collect()
}

pub fn entry(chain: &ChainId) -> Option<&'static ChainEntry> {
    REGISTRY.iter().find(|e| e.chain_id == chain.as_str())
}

/// The deployed contract for a (chain, purpose) — None if the chain is
/// unregistered or that contract isn't deployed there. The single lookup every
/// product (game today, a future Shillbot EVM escrow tomorrow) shares.
pub fn contract_for(chain: &ChainId, purpose: ContractPurpose) -> Option<&'static str> {
    entry(chain).and_then(|e| e.contract_for(purpose))
}

impl ChainEntry {
    /// How this chain fixes the stake for a given contract purpose.
    ///
    /// `None` for `ShillbotEscrow` — that is a per-task escrow, not a game stake,
    /// and has no anchor relationship at all.
    pub fn stake_binding(&self, purpose: ContractPurpose) -> Option<StakeBinding> {
        let ns = ChainId::parse(self.chain_id).ok()?.namespace();
        match (ns, purpose) {
            (Namespace::Eip155, ContractPurpose::CoordinationGame)
            | (Namespace::Eip155, ContractPurpose::CrossChainGame) => {
                Some(StakeBinding::ContractConfig)
            }
            (Namespace::Solana, ContractPurpose::CoordinationGame) => {
                Some(StakeBinding::ProgramConfig)
            }
            (Namespace::Solana, ContractPurpose::CrossChainGame) => {
                Some(StakeBinding::PerMatchQuoted)
            }
            (_, ContractPurpose::ShillbotEscrow) => None,
        }
    }
}

/// Every registered chain, in registry order. Callers that need cross-chain
/// discovery (e.g. the MCP `xchain_supported_chains` tool) iterate this rather
/// than hardcoding the chain set.
pub fn all() -> impl Iterator<Item = &'static ChainEntry> {
    REGISTRY.iter()
}

/// Real-money chains only.
///
/// PREFER THIS OVER `all()` FOR ANY PRICE COMPARISON. Testnet stakes are nominal
/// and USD-untuned by design, so comparing one against a mainnet entry is
/// meaningless — and doing exactly that produced a "Base mainnet is mispriced at
/// $5.98" claim that was asserted twice before anyone noticed the figure came
/// from Base *Sepolia*. Making the correct iteration the easy one is the fix;
/// a comment saying "remember to filter" is not.
pub fn mainnet() -> impl Iterator<Item = &'static ChainEntry> {
    REGISTRY.iter().filter(|e| e.is_mainnet)
}

/// Testnet chains only. Their stakes are NOMINAL — sized for cheap e2e sweeps,
/// not pegged to the anchor — and the cross-chain FX band skips them entirely.
pub fn testnet() -> impl Iterator<Item = &'static ChainEntry> {
    REGISTRY.iter().filter(|e| !e.is_mainnet)
}

/// The Coordination Game tournament currently accepting play, per cluster.
///
/// SINGLE SOURCE. Four consumers must agree with this value and each other:
/// this constant, `coordination-app/infra/cloudrun.tf` (the backend
/// matchmaker's TOURNAMENT_ID), the frontend's `constants.ts` default, and
/// `tests/e2e/harness/network.ts`. A partial move pairs players across two
/// tournaments; `check-tournament-id-parity.mjs` asserts they match.
///
/// Why this exists: on-chain `Tournament.end_time` is IMMUTABLE — there is no
/// extend instruction — so a rollover creates a NEW tournament and every client
/// must be re-pointed. mcp-server used to hardcode `unwrap_or(1)`, and T1 ended
/// 2026-05-01, so any agent taking the documented default got a transaction
/// that failed on-chain with `OutsideTournamentWindow` (6014).
///
/// T2 was created with the tournament script's old 90-day default and expired
/// 2026-08-06. T3 runs to 2027-08-05.
pub const ACTIVE_TOURNAMENT_MAINNET: u64 = 3;
/// Devnet's long-window tournament (T1001's 30-day window expired 2026-06-07).
pub const ACTIVE_TOURNAMENT_DEVNET: u64 = 1003;

/// Same shape as `game_constants::stake_lamports(is_mainnet)` — cluster in,
/// value out, so a caller cannot silently pick the wrong cluster's tournament.
pub const fn active_tournament_id(is_mainnet: bool) -> u64 {
    if is_mainnet {
        ACTIVE_TOURNAMENT_MAINNET
    } else {
        ACTIVE_TOURNAMENT_DEVNET
    }
}

/// The Solana leg the MCP cross-chain registration/queue path is pinned to
/// (devnet), partnering the EVM testnet leg (Base Sepolia) — callers resolving
/// a raw base58 Solana wallet use this single source of truth rather than
/// hardcoding the devnet id. Mainnet CrossChainGame contracts ARE deployed and
/// the game-api matchmaker prices mainnet legs dynamically; only this MCP
/// wallet→chain default remains testnet-pinned (switching it to select by the
/// EVM leg's `is_mainnet` is a real-money routing decision, deliberately not
/// made here). Returns `None` only if the devnet entry is ever removed.
pub fn cross_chain_solana() -> Option<&'static ChainEntry> {
    ChainId::parse(SOLANA_DEVNET_CAIP2)
        .ok()
        .and_then(|id| entry(&id))
}

/// All registered chains in a namespace.
pub fn entries_for(namespace: Namespace) -> impl Iterator<Item = &'static ChainEntry> {
    REGISTRY.iter().filter(move |e| {
        ChainId::parse(e.chain_id)
            .map(|c| c.namespace() == namespace)
            .unwrap_or(false)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_rpc_urls_prefers_the_premium_env_endpoint() {
        let base = ChainId::parse("eip155:84532").unwrap();
        let e = entry(&base).unwrap();
        // Unset: registry public list, unchanged and in order.
        std::env::remove_var("EVM_RPC_URL_EIP155_84532");
        let plain = read_rpc_urls(e);
        assert_eq!(plain.len(), e.rpc_urls.len());
        assert_eq!(plain[0], e.rpc_urls[0]);

        // Set: premium first, public list retained as fallback (never dropped —
        // a premium outage must still fail over).
        std::env::set_var("EVM_RPC_URL_EIP155_84532", "https://premium.example/v2/k");
        let with_env = read_rpc_urls(e);
        assert_eq!(with_env[0], "https://premium.example/v2/k");
        assert_eq!(with_env.len(), e.rpc_urls.len() + 1);
        assert_eq!(with_env[1], e.rpc_urls[0]);
        std::env::remove_var("EVM_RPC_URL_EIP155_84532");
    }

    #[test]
    fn every_registry_entry_has_a_valid_caip2_id() {
        for e in REGISTRY {
            let chain = ChainId::parse(e.chain_id)
                .unwrap_or_else(|err| panic!("{}: invalid CAIP-2: {err}", e.chain_id));
            assert_eq!(chain.as_str(), e.chain_id);
        }
    }

    #[test]
    fn quorum_never_exceeds_provider_count_and_is_nonzero() {
        for e in REGISTRY {
            assert!(e.quorum_m >= 1, "{}: quorum must be >= 1", e.chain_id);
            assert!(
                e.quorum_m <= e.rpc_urls.len(),
                "{}: quorum {} exceeds {} providers",
                e.chain_id,
                e.quorum_m,
                e.rpc_urls.len()
            );
        }
    }

    #[test]
    fn cross_chain_solana_is_a_registered_devnet_entry() {
        let e = cross_chain_solana().expect("devnet solana registered");
        assert_eq!(e.native_symbol, "SOL");
        assert_eq!(e.display_name, "Solana Devnet");
        // Assert on the DEDICATED field. This used to test x402_network.is_none(),
        // which is unrelated to network class — a mainnet entry that simply has
        // no x402 descriptor would have passed it.
        assert!(!e.is_mainnet, "cross-chain solana leg must be devnet");
    }

    #[test]
    fn all_returns_every_entry() {
        let count = all().count();
        assert_eq!(count, REGISTRY.len());
        // all() must be the union of the per-namespace views.
        let by_ns = entries_for(Namespace::Solana).count() + entries_for(Namespace::Eip155).count();
        assert_eq!(
            count, by_ns,
            "a chain exists in a namespace all() doesn't sum"
        );
    }

    #[test]
    fn lookup_by_chain_id_and_namespace() {
        let base = ChainId::parse("eip155:84532").unwrap();
        let entry = entry(&base).expect("base sepolia registered");
        assert_eq!(entry.native_symbol, "ETH");
        assert_eq!(entry.quorum_m, 2);

        let solana_count = entries_for(Namespace::Solana).count();
        assert_eq!(solana_count, 2);
        // Base Sepolia + Ethereum Sepolia + Base mainnet + Ethereum mainnet.
        let evm_count = entries_for(Namespace::Eip155).count();
        assert_eq!(evm_count, 3); // Base Sepolia + Base + Ethereum (Ethereum Sepolia removed)

        // eip155:1 is now Ethereum mainnet (registered); use an unregistered id.
        let unknown = ChainId::parse("eip155:999999").unwrap();
        assert!(super::entry(&unknown).is_none());
    }

    #[test]
    fn is_mainnet_matches_known_networks() {
        let m = |c: &str| entry(&ChainId::parse(c).unwrap()).unwrap().is_mainnet;
        assert!(!m("solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1"), "devnet");
        assert!(!m("eip155:84532"), "base sepolia");
        assert!(
            m("solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp"),
            "solana mainnet"
        );
        assert!(m("eip155:8453"), "base mainnet");
        assert!(m("eip155:1"), "ethereum mainnet");
    }

    #[test]
    fn mainnet_evm_chains_go_live_only_where_deployed() {
        // Ethereum L1: both game contracts deployed 2026-07-11 → live; shillbot
        // escrow not deployed there, stays None.
        let eth = entry(&ChainId::parse("eip155:1").unwrap()).unwrap();
        assert!(
            eth.is_live(ContractPurpose::CoordinationGame),
            "eth same-chain live"
        );
        assert!(
            eth.is_live(ContractPurpose::CrossChainGame),
            "eth cross-chain live"
        );
        assert!(!eth.is_live(ContractPurpose::ShillbotEscrow));
        assert!(eth.contract_for(ContractPurpose::ShillbotEscrow).is_none());
        // Base mainnet: same-chain + cross-chain game contracts live (2026-07-09 /
        // 2026-07-11); shillbot escrow stays None until its deploy.
        let base = entry(&ChainId::parse("eip155:8453").unwrap()).unwrap();
        assert!(
            base.is_live(ContractPurpose::CoordinationGame),
            "base same-chain live"
        );
        assert!(
            base.is_live(ContractPurpose::CrossChainGame),
            "base cross-chain live"
        );
        assert!(!base.is_live(ContractPurpose::ShillbotEscrow));
        // Base Sepolia (testnet) IS live for the game contracts — is_live true.
        let sepolia = entry(&ChainId::parse("eip155:84532").unwrap()).unwrap();
        assert!(sepolia.is_live(ContractPurpose::CrossChainGame));
        assert!(sepolia.is_live(ContractPurpose::CoordinationGame));
    }

    #[test]
    fn contract_for_resolves_by_purpose() {
        let base = ChainId::parse("eip155:84532").unwrap();
        // Base Sepolia hosts BOTH the cross-chain and same-chain contracts.
        assert_eq!(
            contract_for(&base, ContractPurpose::CrossChainGame),
            Some("0xd38b1fB07Bf64801bCBc3721937D6e2Ba6E5feb4")
        );
        // Post-cutover this resolves to the v4 PROXY, not the v3 address that
        // is still stored in `coordination_game_contract` for indexing.
        assert_eq!(
            contract_for(&base, ContractPurpose::CoordinationGame),
            Some("0x4FBBceb96D2814b5d4ac26089Eb7E43471533253")
        );
        // Solana has no same-chain EVM CoordinationGame, but has the cross-chain one.
        let sol = ChainId::parse("solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1").unwrap();
        assert!(contract_for(&sol, ContractPurpose::CoordinationGame).is_none());
        assert!(contract_for(&sol, ContractPurpose::CrossChainGame).is_some());
        // Unregistered chain → None for any purpose.
        let unknown = ChainId::parse("eip155:999999").unwrap();
        assert!(contract_for(&unknown, ContractPurpose::CoordinationGame).is_none());
        assert!(contract_for(&unknown, ContractPurpose::ShillbotEscrow).is_none());
    }

    /// Pin the v4 proxy per chain, and pin that it is DISTINCT from v3.
    ///
    /// v4 is a separate proxy holding its own seasons, player records and
    /// unclaimed balances. Pointing a client at v3 while calling it v4 (or
    /// vice-versa) reads an empty season and pays nobody, with no error — the
    /// call succeeds against the wrong contract. Exact-address pinning is what
    /// catches a silent edit; the inequality assert is what catches the
    /// copy-paste that quietly reuses the v3 address.
    #[test]
    fn v4_proxy_is_pinned_and_distinct_from_v3() {
        let mut seen = 0;
        for e in REGISTRY {
            let Some(v4) = e.coordination_game_v4_proxy else {
                continue;
            };
            seen += 1;
            let expected = match e.chain_id {
                "eip155:84532" => "0x4FBBceb96D2814b5d4ac26089Eb7E43471533253",
                "eip155:8453" => "0xd585baE48901513202dAEb7d4feE4Af508a96234",
                "eip155:1" => "0x265818b054E8413Bab870e0Ce0D8aB68400CF0F9",
                other => panic!("unexpected chain carries a v4 proxy: {other}"),
            };
            assert_eq!(v4, expected, "v4 proxy changed for {}", e.chain_id);
            if let Some(v3) = e.coordination_game_contract {
                assert_ne!(
                    v4.to_lowercase(),
                    v3.to_lowercase(),
                    "{}: v4 proxy must not be the v3 address — a client pointed at the \
                     wrong contract reads an empty season and pays nobody, silently",
                    e.chain_id
                );
            }
        }
        assert_eq!(seen, 3, "expected v4 on Base Sepolia, Base and Ethereum");
    }

    /// The v3 -> v4 cutover itself: resolution must hand back the v4 proxy on
    /// every chain that has one, and must NOT silently fall back to v3 there.
    ///
    /// This is the assert that makes the switchover real. The registry carried
    /// both addresses for a while with nothing reading v4, so the migration
    /// looked done while every service still played on v3. A test on the FIELD
    /// cannot catch that — only a test on what `contract_for` returns can.
    #[test]
    fn coordination_game_resolves_to_v4_where_deployed() {
        let mut cut_over = 0;
        for e in REGISTRY {
            let resolved = e.contract_for(ContractPurpose::CoordinationGame);
            match e.coordination_game_v4_proxy {
                Some(v4) => {
                    cut_over += 1;
                    assert_eq!(
                        resolved,
                        Some(v4),
                        "{}: resolution must return the v4 proxy, not v3 — otherwise the \
                         registry says migrated while every service still plays on v3",
                        e.chain_id
                    );
                }
                // No v4 deployed: keep serving v3 rather than resolving to None,
                // which would take the same-chain game offline on that chain.
                None => assert_eq!(resolved, e.coordination_game_contract, "{}", e.chain_id),
            }
        }
        assert_eq!(cut_over, 3, "expected 3 chains cut over to v4");
    }

    /// v3 must remain REACHABLE after the cutover. Resolution moves to v4, but
    /// the v3 address stays in the registry because the leaderboard indexer
    /// scans both — dropping it would erase every game played before the cut.
    #[test]
    fn v3_address_survives_the_cutover_for_indexing() {
        for e in REGISTRY {
            if e.coordination_game_v4_proxy.is_some() && e.chain_id != "eip155:84532" {
                assert!(
                    e.coordination_game_contract.is_some(),
                    "{}: v3 address was removed — leaderboard history for every game \
                     played on v3 becomes unindexable",
                    e.chain_id
                );
            }
        }
    }

    #[test]
    fn shillbot_escrow_resolves_only_where_deployed() {
        // Base Sepolia 2026-07-07 (S5 live demo); Ethereum Sepolia 2026-08-01
        // (deploy-evm-testnet.yml) so the escrow matrix can exercise a SECOND
        // chain rather than re-running Base Sepolia under an eth-sepolia label.
        // Every other entry stays None until its own deploy lands. Solana
        // entries never carry an EVM escrow address (the Solana leg is the
        // shillbot program itself). Pinning the exact addresses here is what
        // catches a silent registry edit.
        for e in REGISTRY {
            let chain = ChainId::parse(e.chain_id).expect("valid CAIP-2");
            let resolved = contract_for(&chain, ContractPurpose::ShillbotEscrow);
            match e.chain_id {
                "eip155:84532" => assert_eq!(
                    resolved,
                    Some("0xaFe061778f9A76fCe7da4124dC89DAF8309E5F3c"),
                    "Base Sepolia must resolve to the deployed ShillbotEscrow"
                ),
                "eip155:11155111" => assert_eq!(
                    resolved,
                    Some("0x293AB2b2A7d862d8FbD6EB1E185f984E0a65882F"),
                    "Ethereum Sepolia must resolve to the deployed ShillbotEscrow"
                ),
                _ => assert!(
                    resolved.is_none(),
                    "{}: shillbot escrow address must stay None until its deploy lands",
                    e.chain_id
                ),
            }
        }
    }

    /// Parse `XCHAIN_STAKE_WEI` / `XCHAIN_MAX_TRANCHE_WEI` per CAIP-2 out of the
    /// deploy workflow. Reads the YAML as text on purpose — the point is to
    /// compare against what CI will ACTUALLY export, not a re-encoding of it.
    fn workflow_stakes() -> Vec<(String, u128, u128)> {
        let yaml = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../.github/workflows/_deploy-evm.yml"
        ))
        .expect("_deploy-evm.yml must be readable from the crate dir");
        let mut out = Vec::new();
        let (mut caip2, mut stake) = (None::<String>, None::<u128>);
        for line in yaml.lines() {
            let l = line.trim();
            let grab = |key: &str| -> Option<String> {
                // `i + key.len()` trips clippy::arithmetic_side_effects, which
                // this crate denies. split_once does the same job with no
                // arithmetic and no chance of a mid-char slice panic.
                let (_, rest) = l.split_once(key)?;
                Some(rest.split(&['"', ' ', '\''][..]).next()?.to_string())
            };
            if let Some(v) = grab("CAIP2=") {
                caip2 = Some(v);
            } else if let Some(v) = grab("XCHAIN_STAKE_WEI=") {
                stake = v.parse().ok();
            } else if let Some(v) = grab("XCHAIN_MAX_TRANCHE_WEI=") {
                if let (Some(c), Some(s), Ok(tr)) = (caip2.clone(), stake, v.parse::<u128>()) {
                    out.push((c, s, tr));
                    caip2 = None;
                    stake = None;
                }
            }
        }
        out
    }

    #[test]
    fn deploy_workflow_stakes_match_the_registry() {
        // THE SINGLE-SOURCE-OF-TRUTH RULE, ENFORCED. The registry comment asks
        // to "keep in lockstep with the deploy workflow's XCHAIN_STAKE_WEI" —
        // that was a wish with nothing behind it, and the same value lives in
        // 15+ places across four languages plus the chain. A wrong literal here
        // deploys a contract whose stakeWei disagrees with what game-api will
        // ask players for, and every createGame reverts BadStake.
        let wf = workflow_stakes();
        assert!(!wf.is_empty(), "parsed no stakes out of _deploy-evm.yml");
        let mut checked = 0;
        for (caip2, stake, tranche) in wf {
            let Ok(id) = ChainId::parse(&caip2) else {
                continue;
            };
            let Some(e) = entry(&id) else {
                // A chain in the workflow but not the registry is itself drift.
                panic!("_deploy-evm.yml deploys to {caip2}, which the registry does not know");
            };
            assert_eq!(
                stake, e.stake_base_units,
                "{caip2}: workflow deploys stakeWei={stake} but registry says {}",
                e.stake_base_units,
            );
            assert_eq!(
                tranche, e.max_tranche_base_units,
                "{caip2}: workflow maxTranche={tranche} but registry says {}",
                e.max_tranche_base_units,
            );
            checked += 1;
        }
        assert!(
            checked >= 2,
            "expected to check >=2 chains, checked {checked}"
        );
    }

    /// Convert a stake to USD cents at its recorded peg price.
    fn pegged_usd_cents(e: &ChainEntry) -> f64 {
        let native = e.stake_base_units as f64 / 10f64.powi(e.native_decimals as i32);
        native * e.peg_native_usd_cents as f64
    }

    #[test]
    fn stake_pegs_are_internally_consistent() {
        // Each stake literal must actually equal what it CLAIMS to be worth at
        // the price it was pegged at. Before this, intent lived in a prose
        // comment and nothing compared it to the number beside it.
        for e in REGISTRY {
            let got = pegged_usd_cents(e);
            let want = e.stake_usd_cents as f64;
            let drift = (got - want).abs() / want;
            assert!(
                drift < 0.02,
                "{}: stake {} base units at peg price {}c = ${:.2}, but declares ${:.2}",
                e.chain_id,
                e.stake_base_units,
                e.peg_native_usd_cents,
                got / 100.0,
                want / 100.0,
            );
        }
    }

    #[test]
    fn exactly_one_game_surface_can_float() {
        // THE ASYMMETRY, asserted rather than described. It is the reason ETH is
        // the anchor: three of the four game surfaces are pinned to a contract or
        // program config, so the fourth is the only one that can follow. Not
        // knowing this is what produced an absolute-USD anchor that had to be
        // reverted.
        let mut floating = Vec::new();
        let mut fixed = 0;
        for e in all() {
            for purpose in [
                ContractPurpose::CoordinationGame,
                ContractPurpose::CrossChainGame,
            ] {
                match e.stake_binding(purpose) {
                    Some(b) if b.can_float() => floating.push((e.chain_id, purpose)),
                    Some(_) => fixed += 1,
                    None => {}
                }
            }
        }
        assert!(
            fixed > 0,
            "no fixed surfaces found — the enumeration is broken"
        );
        // Every floating surface must be a Solana cross-chain leg, and nothing else.
        for (chain_id, purpose) in &floating {
            assert_eq!(*purpose, ContractPurpose::CrossChainGame);
            assert_eq!(
                ChainId::parse(chain_id).unwrap().namespace(),
                Namespace::Solana,
                "{chain_id}: only a Solana cross-chain leg may float"
            );
        }
        assert!(
            !floating.is_empty(),
            "expected at least one floating surface"
        );
    }

    #[test]
    fn evm_surfaces_never_float_and_shillbot_has_no_binding() {
        let base = entry(&ChainId::parse("eip155:8453").unwrap()).unwrap();
        assert_eq!(
            base.stake_binding(ContractPurpose::CoordinationGame),
            Some(StakeBinding::ContractConfig)
        );
        assert_eq!(
            base.stake_binding(ContractPurpose::CrossChainGame),
            Some(StakeBinding::ContractConfig)
        );
        assert!(!base
            .stake_binding(ContractPurpose::CrossChainGame)
            .unwrap()
            .can_float());
        // A per-task escrow is not a game stake and has no anchor relationship.
        assert_eq!(base.stake_binding(ContractPurpose::ShillbotEscrow), None);

        let sol = entry(&ChainId::parse(SOLANA_MAINNET_CAIP2).unwrap()).unwrap();
        assert_eq!(
            sol.stake_binding(ContractPurpose::CoordinationGame),
            Some(StakeBinding::ProgramConfig),
            "same-chain Solana reads GlobalConfig; deposit_stake takes no amount"
        );
        assert!(sol
            .stake_binding(ContractPurpose::CrossChainGame)
            .unwrap()
            .can_float());
    }

    #[test]
    fn evm_mainnet_chains_hold_the_anchor_literally() {
        // EVM cannot float (`msg.value != stakeWei` reverts), so every EVM
        // mainnet chain must hold STAKE_ANCHOR_WEI exactly. This is the whole of
        // anchor parity on the EVM side — there is no conversion and no
        // tolerance, and two EVM chains disagreeing would be a plain config
        // error rather than any kind of drift.
        let mut checked = 0;
        for e in mainnet() {
            if ChainId::parse(e.chain_id).map(|c| c.namespace()) != Ok(Namespace::Eip155) {
                continue;
            }
            assert_eq!(
                e.stake_base_units, STAKE_ANCHOR_WEI,
                "{}: holds {} wei, anchor is {} — EVM cannot float, so it must \
                 hold the anchor literally",
                e.chain_id, e.stake_base_units, STAKE_ANCHOR_WEI
            );
            checked += 1;
        }
        // A conformance test that checked nothing would pass silently.
        assert!(
            checked >= 2,
            "expected >=2 EVM mainnet chains, checked {checked}"
        );
    }

    #[test]
    fn solana_is_off_anchor_until_it_tracks_the_ratio() {
        // Solana same-chain is pinned to a hardcoded 0.05 SOL that bears no
        // relation to STAKE_ANCHOR_WEI. It cannot be asserted equal (different
        // coin) and cannot be asserted converted (no live rate in a unit test —
        // CLAUDE.md forbids network calls here). So this records the gap
        // explicitly rather than leaving it to be rediscovered.
        //
        // The live conversion is checked by tests/e2e/scripts/check-stake-parity.mjs,
        // which is allowed to fetch a rate. Closing the gap for good is the
        // per-match quote (deposit_stake taking stake_lamports).
        let sol = entry(&ChainId::parse(SOLANA_MAINNET_CAIP2).unwrap()).unwrap();
        assert_eq!(
            sol.stake_base_units, 68_482_585,
            "Solana mainnet moved off its 2026-08-04 re-peg. It was set to the \
             0.0027 ETH anchor at SOL/ETH 25.3639 (SOL $73.61 / ETH $1867.16). \
             If this is another intentional re-peg, update this test and record \
             the ratio it was pegged at."
        );
    }

    #[test]
    fn network_class_accessors_partition_the_registry() {
        // The guard against the error that produced a "Base is mispriced at
        // $5.98" claim: that figure was BASE SEPOLIA's, a testnet, compared
        // against mainnet entries. These accessors exist so a price comparison
        // cannot casually span the boundary, and this asserts they are an exact
        // partition rather than two overlapping filters.
        let m = mainnet().count();
        let t = testnet().count();
        assert_eq!(
            m + t,
            all().count(),
            "mainnet/testnet must partition the registry"
        );
        assert!(
            m >= 2 && t >= 2,
            "expected both classes populated: {m} mainnet, {t} testnet"
        );
        assert!(mainnet().all(|e| e.is_mainnet));
        assert!(testnet().all(|e| !e.is_mainnet));
    }

    #[test]
    fn testnet_stakes_share_one_anchor() {
        // Testnet stakes are nominal, but they should at least agree with each
        // other — a testnet that silently costs 5x more distorts capital
        // planning for the e2e sweeps.
        let usd: Vec<(&str, u32)> = testnet().map(|e| (e.chain_id, e.stake_usd_cents)).collect();
        let lo = usd.iter().map(|(_, v)| *v).min().unwrap() as f64;
        let hi = usd.iter().map(|(_, v)| *v).max().unwrap() as f64;
        assert!(
            hi / lo <= 1.5,
            "testnet stakes diverge {:.1}x: {:?}",
            hi / lo,
            usd
        );
    }

    #[test]
    fn stakes_and_tranches_are_positive_with_sane_windows() {
        for e in REGISTRY {
            assert!(e.stake_base_units > 0, "{}: zero stake", e.chain_id);
            assert!(
                e.max_tranche_base_units >= e.stake_base_units,
                "{}: tranche clamp below stake makes winner-takes unpayable",
                e.chain_id
            );
            // Panel requirement: claim windows bounded (≤1h target) and
            // skew margin nonzero.
            assert!(e.claim_window_secs <= 3_600, "{}: window > 1h", e.chain_id);
            assert!(
                e.skew_margin_secs >= 60,
                "{}: skew margin too small",
                e.chain_id
            );
        }
    }
}
