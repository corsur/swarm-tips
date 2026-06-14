//! Cross-chain match instructions — the Solana leg of Solana↔EVM matches.
//! The Solana program is ALWAYS leg A in the certificate (the EVM contract
//! is leg B). This module mirrors evm/src/CrossChainGame.sol, translated to
//! Anchor; the cryptographic core (cert digest + signer recovery) and the
//! payoff core (resolve_xleg) live in `crate::cert` / `crate::payoff` and
//! are unit-tested against the same golden vectors as the EVM side.

use crate::cert::{
    recover_eth_address, require_signer, CertLegArg, CheckpointArg, MatchLiveCertArg,
    MatchLiveCertNoA, OutcomeCertArg,
};
use crate::errors::CoordinationError;
use crate::events::{
    XClaimOpened, XEquivocationProven, XMatchCreated, XMatchRefunded, XMatchSettled, XTrancheLocked,
};
use crate::instructions::utils::transfer_lamports;
use crate::payoff::{resolve_xleg, XLegResolution};
use crate::state::{
    GlobalConfig, PlayerProfile, Tournament, XChainMatch, XChainStatus, XPayoutPool,
};
use anchor_lang::prelude::*;

const TERMINAL_STEP_COUNT: u8 = 4;
const XKIND_TIMEOUT_P1_WINS: u8 = 6;
const XKIND_TIMEOUT_P2_WINS: u8 = 7;
const XKIND_TIMEOUT_BOTH_FORFEIT: u8 = 8;

fn is_timeout_kind(kind: u8) -> bool {
    matches!(
        kind,
        XKIND_TIMEOUT_P1_WINS | XKIND_TIMEOUT_P2_WINS | XKIND_TIMEOUT_BOTH_FORFEIT
    )
}

// ---------------------------------------------------------------------------
// Pool initialization + float management
// ---------------------------------------------------------------------------

pub fn initialize_xpool(
    ctx: Context<InitializeXPool>,
    operator: Pubkey,
    operator_signer: [u8; 20],
    max_tranche_lamports: u64,
    max_claim_window_secs: u32,
    skew_margin_secs: u32,
) -> Result<()> {
    require!(operator != Pubkey::default(), CoordinationError::XBadConfig);
    require!(operator_signer != [0u8; 20], CoordinationError::XBadConfig);
    require!(skew_margin_secs > 0, CoordinationError::XBadConfig);
    let pool = &mut ctx.accounts.pool;
    pool.operator = operator;
    pool.operator_signer = operator_signer;
    pool.free_lamports = 0;
    pool.locked_lamports = 0;
    pool.max_tranche_lamports = max_tranche_lamports;
    pool.max_claim_window_secs = max_claim_window_secs;
    pool.skew_margin_secs = skew_margin_secs;
    pool.bump = ctx.bumps.pool;
    require!(pool.locked_lamports == 0, CoordinationError::XBadConfig);
    Ok(())
}

pub fn xpool_deposit(ctx: Context<XPoolDeposit>, amount: u64) -> Result<()> {
    require!(amount > 0, CoordinationError::XBadConfig);
    // Funder is a System-owned wallet: credit the pool PDA via a System CPI
    // (a program cannot debit an account it does not own).
    anchor_lang::system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            anchor_lang::system_program::Transfer {
                from: ctx.accounts.funder.to_account_info(),
                to: ctx.accounts.pool.to_account_info(),
            },
        ),
        amount,
    )?;
    let pool = &mut ctx.accounts.pool;
    pool.free_lamports = pool
        .free_lamports
        .checked_add(amount)
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    Ok(())
}

pub fn xpool_withdraw(ctx: Context<XPoolWithdraw>, amount: u64) -> Result<()> {
    let pool = &mut ctx.accounts.pool;
    require!(
        amount <= pool.free_lamports,
        CoordinationError::XPoolInsufficient
    );
    pool.free_lamports = pool
        .free_lamports
        .checked_sub(amount)
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    transfer_lamports(
        &pool.to_account_info(),
        &ctx.accounts.operator.to_account_info(),
        amount,
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Match lifecycle — funding and locking
// ---------------------------------------------------------------------------

/// Args for `create_xmatch`, bundled so the instruction stays within the
/// argument-count budget.
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct CreateXMatchArgs {
    pub tournament_id: u64,
    pub player_is_p1: bool,
    pub session_key: [u8; 20],
    pub counter_session_key: [u8; 20],
    pub stake_lamports: u64,
    pub fund_deadline: i64,
    pub match_deadline: i64,
}

pub fn create_xmatch(
    ctx: Context<CreateXMatch>,
    match_id: [u8; 32],
    args: CreateXMatchArgs,
) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    require!(
        ctx.accounts.matchmaker.key() == ctx.accounts.global_config.matchmaker,
        CoordinationError::NotMatchmaker
    );
    require!(args.stake_lamports > 0, CoordinationError::XBadConfig);
    require!(
        args.session_key != [0u8; 20] && args.counter_session_key != [0u8; 20],
        CoordinationError::XBadConfig
    );
    // Distinct seats: equivocation + dual-signature model assume two parties.
    require!(
        args.session_key != args.counter_session_key,
        CoordinationError::XBadConfig
    );
    require!(
        args.fund_deadline > now && args.match_deadline > args.fund_deadline,
        CoordinationError::XDeadlinePassed
    );

    let player_key = ctx.accounts.player.key();
    let m = &mut ctx.accounts.xmatch;
    m.match_id = match_id;
    m.tournament_id = args.tournament_id;
    m.player = player_key;
    m.player_is_p1 = u8::from(args.player_is_p1);
    m.session_key = args.session_key;
    m.counter_session_key = args.counter_session_key;
    m.stake_lamports = args.stake_lamports;
    m.tranche_lamports = 0;
    m.fund_deadline = args.fund_deadline;
    m.match_deadline = args.match_deadline;
    m.best_step_count = 0;
    m.best_outcome_kind = 0;
    m.local_equivocated = false;
    m.counter_equivocated = false;
    m.status = XChainStatus::Funded;
    m.bump = ctx.bumps.xmatch;

    // Player is a System-owned wallet: credit the match PDA via a System CPI.
    anchor_lang::system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            anchor_lang::system_program::Transfer {
                from: ctx.accounts.player.to_account_info(),
                to: ctx.accounts.xmatch.to_account_info(),
            },
        ),
        args.stake_lamports,
    )?;
    require!(
        ctx.accounts.xmatch.status == XChainStatus::Funded,
        CoordinationError::XInvalidStatus
    );
    emit!(XMatchCreated {
        match_id,
        player: player_key,
        stake_lamports: args.stake_lamports,
    });
    Ok(())
}

/// Permissionless tranche lock. Authorization is the operator's match-live
/// signature over the cert (the same signature relayed at pairing — no new
/// operator action); the locked amount is `cert.leg_a.tranche`, so the operator
/// commits the exact tranche by signing. Anyone may submit + pay the fee; the
/// operator never pays to run the lock (gas is external by default). Mirrors the
/// EVM `lockTranche` and the permissionless settle/operator-sig model.
///
/// Unlike settle, the lock takes the FULL cert (not `MatchLiveCertNoA`): leg A's
/// tranche is not yet on-chain (`m.tranche_lamports` is 0 until THIS call sets
/// it), so it cannot be rebuilt — the caller supplies it and the operator sig
/// binds it.
pub fn lock_xtranche(
    ctx: Context<LockXTranche>,
    cert: MatchLiveCertArg,
    operator_sig: [u8; 65],
) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    require!(
        ctx.accounts.xmatch.status == XChainStatus::Funded,
        CoordinationError::XInvalidStatus
    );
    let tranche_lamports = verify_lock_authorization(
        &ctx.accounts.xmatch,
        &cert,
        &operator_sig,
        &ctx.accounts.pool,
        solana_chain_tag(),
        now,
    )?;
    require!(
        tranche_lamports <= ctx.accounts.pool.max_tranche_lamports,
        CoordinationError::XTrancheTooLarge
    );
    require!(
        tranche_lamports <= ctx.accounts.pool.free_lamports,
        CoordinationError::XPoolInsufficient
    );

    let pool = &mut ctx.accounts.pool;
    pool.free_lamports = pool
        .free_lamports
        .checked_sub(tranche_lamports)
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    pool.locked_lamports = pool
        .locked_lamports
        .checked_add(tranche_lamports)
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    let m = &mut ctx.accounts.xmatch;
    m.tranche_lamports = tranche_lamports;
    m.locked_at = now;
    m.status = XChainStatus::Locked;
    emit!(XTrancheLocked {
        match_id: m.match_id,
        tranche_lamports,
    });
    Ok(())
}

/// Bind the cert's leg A to the funded match and verify the operator's match-live
/// signature, returning the leg-A tranche to lock. Mirrors `verify_match_live`
/// EXCEPT: (1) it omits the `leg.tranche == m.tranche_lamports` check (THIS call
/// sets the tranche — the operator sig over the full cert is what fixes it);
/// (2) quote freshness is anchored at `now`, not `m.locked_at` (0 pre-lock);
/// (3) only the operator signature is checked — the lock precedes the players'
/// match-live session signatures.
fn verify_lock_authorization(
    m: &XChainMatch,
    cert: &MatchLiveCertArg,
    operator_sig: &[u8; 65],
    pool: &XPayoutPool,
    chain_tag: [u8; 32],
    now: i64,
) -> Result<u64> {
    let leg = &cert.leg_a; // leg A is ALWAYS the Solana leg
    require!(
        leg.chain_tag == chain_tag
            && leg.contract == crate::ID.to_bytes()
            && leg.player == m.player.to_bytes()
            && leg.session_key == m.session_key
            && cert.leg_b.session_key == m.counter_session_key
            && u64::try_from(leg.stake).unwrap_or(u64::MAX) == m.stake_lamports
            && cert.match_deadline == u64::try_from(m.match_deadline).unwrap_or(0)
            && (cert.a_is_p1 == 1) == (m.player_is_p1 == 1),
        CoordinationError::XCertMismatch
    );
    let quote_ok = u128::from(cert.quote_timestamp)
        .checked_add(u128::from(cert.quote_max_age_secs))
        .ok_or(CoordinationError::ArithmeticOverflow)?
        >= u128::try_from(now).unwrap_or(0);
    require!(quote_ok, CoordinationError::XStaleQuote);

    require_signer(&cert.digest(), operator_sig, &pool.operator_signer)?;
    u64::try_from(leg.tranche).map_err(|_| CoordinationError::XTrancheTooLarge.into())
}

// ---------------------------------------------------------------------------
// Settlement — terminal certificates (instant path)
// ---------------------------------------------------------------------------

pub fn settle_xmatch(
    ctx: Context<SettleXMatch>,
    cert_no_a: MatchLiveCertNoA,
    outcome: OutcomeCertArg,
    live_sigs: [[u8; 65]; 3],
    oc_sigs: [[u8; 65]; 3],
) -> Result<()> {
    // Checks (read-only).
    require!(
        ctx.accounts.xmatch.status == XChainStatus::Locked,
        CoordinationError::XInvalidStatus
    );
    // Reconstruct leg A from authoritative on-chain state (saves 148 bytes
    // and removes the leg-A tamper surface entirely).
    let cert = cert_no_a.with_leg_a(rebuild_leg_a(&ctx.accounts.xmatch, solana_chain_tag()));
    let live_digest = verify_match_live(
        &ctx.accounts.xmatch,
        &cert,
        &live_sigs,
        &ctx.accounts.pool,
        solana_chain_tag(),
    )?;
    require!(
        outcome.match_live_digest == live_digest && outcome.match_id == cert.match_id,
        CoordinationError::XCertMismatch
    );
    // Reject out-of-range kinds at the boundary: the cert digest normalizes
    // unknown kinds, so an explicit range check keeps the signed value and
    // the executed value the same.
    require!(
        outcome.outcome_kind <= XKIND_TIMEOUT_BOTH_FORFEIT,
        CoordinationError::XBadOutcome
    );
    require!(
        outcome.step_count == TERMINAL_STEP_COUNT || is_timeout_kind(outcome.outcome_kind),
        CoordinationError::XBadOutcome
    );
    let oc_digest = outcome.digest();
    require_signer(&oc_digest, &oc_sigs[0], &cert.leg_a.session_key)?;
    require_signer(&oc_digest, &oc_sigs[1], &cert.leg_b.session_key)?;
    require_signer(&oc_digest, &oc_sigs[2], &ctx.accounts.pool.operator_signer)?;

    // Effects.
    ctx.accounts.xmatch.status = XChainStatus::Settled;
    ctx.accounts.xmatch.match_live_digest = live_digest;
    ctx.accounts.player_profile.init_if_new(
        ctx.accounts.xmatch.player,
        ctx.accounts.xmatch.tournament_id,
        ctx.bumps.player_profile,
    );

    // Interactions.
    execute_xleg(
        outcome.outcome_kind,
        &ctx.accounts.xmatch,
        &mut ctx.accounts.pool,
        &mut ctx.accounts.tournament,
        &mut ctx.accounts.player_profile,
        &ctx.accounts.global_config,
        &ctx.accounts.treasury,
        &ctx.accounts.player,
    )
}

// ---------------------------------------------------------------------------
// Settlement — contested / timeout claims (optimistic path)
// ---------------------------------------------------------------------------

/// The claim window closes at match_deadline + claim_window_secs — anchored
/// to the deadline (shared by both legs via the cert), NOT file time, so a
/// claim on one leg can never race a refund on the other. Bounded by the
/// pool's max so the refund backstop always strictly follows it.
fn claim_window_end(
    match_deadline: i64,
    claim_window_secs: u32,
    pool: &XPayoutPool,
) -> Result<i64> {
    require!(
        claim_window_secs <= pool.max_claim_window_secs,
        CoordinationError::XBadConfig
    );
    match_deadline
        .checked_add(i64::from(claim_window_secs))
        .ok_or(error!(CoordinationError::ArithmeticOverflow))
}

pub fn open_xclaim(
    ctx: Context<OpenXClaim>,
    cert: MatchLiveCertArg,
    cp: CheckpointArg,
    live_sigs: [[u8; 65]; 3],
    cp_sigs: [[u8; 65]; 2],
) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    require!(
        ctx.accounts.xmatch.status == XChainStatus::Locked,
        CoordinationError::XInvalidStatus
    );
    let window_end = claim_window_end(
        ctx.accounts.xmatch.match_deadline,
        cert.claim_window_secs,
        &ctx.accounts.pool,
    )?;
    require!(now <= window_end, CoordinationError::XDeadlinePassed);

    let live_digest = verify_match_live(
        &ctx.accounts.xmatch,
        &cert,
        &live_sigs,
        &ctx.accounts.pool,
        solana_chain_tag(),
    )?;
    require!(
        cp.match_live_digest == live_digest,
        CoordinationError::XCertMismatch
    );
    verify_checkpoint(&cert, &cp, &cp_sigs)?;

    let m = &mut ctx.accounts.xmatch;
    m.status = XChainStatus::Claiming;
    m.match_live_digest = live_digest;
    m.best_step_count = cp.step_count;
    m.best_outcome_kind = cp.derive_claim_outcome();
    m.claim_window_end = window_end;
    emit!(XClaimOpened {
        match_id: m.match_id,
        claimed_outcome: m.best_outcome_kind,
        claim_window_end: window_end,
    });
    Ok(())
}

pub fn supersede_xclaim(
    ctx: Context<SupersedeXClaim>,
    cert: MatchLiveCertArg,
    cp: CheckpointArg,
    cp_sigs: [[u8; 65]; 2],
) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let m = &ctx.accounts.xmatch;
    require!(
        m.status == XChainStatus::Claiming,
        CoordinationError::XInvalidStatus
    );
    require!(
        now <= m.claim_window_end,
        CoordinationError::XDeadlinePassed
    );
    require!(
        cert.digest() == m.match_live_digest && cp.match_live_digest == m.match_live_digest,
        CoordinationError::XCertMismatch
    );
    // An equivocation verdict (best_step_count == MAX) is final.
    require!(m.best_step_count != u8::MAX, CoordinationError::XBadOutcome);
    require!(
        cp.step_count > m.best_step_count,
        CoordinationError::XBadOutcome
    );
    verify_checkpoint(&cert, &cp, &cp_sigs)?;

    let m = &mut ctx.accounts.xmatch;
    m.best_step_count = cp.step_count;
    m.best_outcome_kind = cp.derive_claim_outcome();
    Ok(())
}

/// Two distinct co-signed checkpoints at the same step from the same session
/// key = equivocation. The verdict is computed from explicit per-party flags
/// (order-independent; never inferred from the current claim outcome).
pub fn submit_equivocation_proof(
    ctx: Context<SubmitEquivocation>,
    cert: MatchLiveCertArg,
    cp_a: CheckpointArg,
    cp_b: CheckpointArg,
    sig_a: [u8; 65],
    sig_b: [u8; 65],
) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let m = &ctx.accounts.xmatch;
    require!(
        m.status == XChainStatus::Locked || m.status == XChainStatus::Claiming,
        CoordinationError::XInvalidStatus
    );
    let live_digest = cert.digest();
    require!(
        cert.match_deadline == u64::try_from(m.match_deadline).unwrap_or(0),
        CoordinationError::XCertMismatch
    );
    let window_end =
        claim_window_end(m.match_deadline, cert.claim_window_secs, &ctx.accounts.pool)?;
    require!(now <= window_end, CoordinationError::XDeadlinePassed);
    require!(
        cp_a.match_live_digest == live_digest && cp_b.match_live_digest == live_digest,
        CoordinationError::XCertMismatch
    );
    if m.status == XChainStatus::Claiming {
        require!(
            m.match_live_digest == live_digest,
            CoordinationError::XCertMismatch
        );
    }
    require!(
        cp_a.step_count == cp_b.step_count,
        CoordinationError::XBadOutcome
    );
    let digest_a = cp_a.digest();
    let digest_b = cp_b.digest();
    require!(digest_a != digest_b, CoordinationError::XBadOutcome);

    let signer_a = recover_eth_address(&digest_a, &sig_a)?;
    let signer_b = recover_eth_address(&digest_b, &sig_b)?;
    require!(signer_a == signer_b, CoordinationError::XBadSignature);

    let local_is_p1 = m.player_is_p1 == 1;
    let session_key = m.session_key;
    let counter_session_key = m.counter_session_key;
    let m = &mut ctx.accounts.xmatch;
    if signer_a == session_key {
        m.local_equivocated = true;
    } else if signer_a == counter_session_key {
        m.counter_equivocated = true;
    } else {
        return Err(error!(CoordinationError::XBadSignature));
    }
    m.status = XChainStatus::Claiming;
    m.match_live_digest = live_digest;
    m.claim_window_end = window_end;
    m.best_outcome_kind =
        equivocation_outcome(local_is_p1, m.local_equivocated, m.counter_equivocated);
    m.best_step_count = u8::MAX; // equivocation verdict is final
    emit!(XEquivocationProven {
        match_id: m.match_id,
        new_best_outcome: m.best_outcome_kind,
    });
    Ok(())
}

pub fn settle_xclaim(ctx: Context<SettleXClaim>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    require!(
        ctx.accounts.xmatch.status == XChainStatus::Claiming,
        CoordinationError::XInvalidStatus
    );
    require!(
        now > ctx.accounts.xmatch.claim_window_end,
        CoordinationError::XDeadlineNotReached
    );
    let kind = ctx.accounts.xmatch.best_outcome_kind;
    ctx.accounts.xmatch.status = XChainStatus::ClaimSettled;
    ctx.accounts.player_profile.init_if_new(
        ctx.accounts.xmatch.player,
        ctx.accounts.xmatch.tournament_id,
        ctx.bumps.player_profile,
    );
    execute_xleg(
        kind,
        &ctx.accounts.xmatch,
        &mut ctx.accounts.pool,
        &mut ctx.accounts.tournament,
        &mut ctx.accounts.player_profile,
        &ctx.accounts.global_config,
        &ctx.accounts.treasury,
        &ctx.accounts.player,
    )
}

/// local-only → local forfeits (counterparty wins this leg); counter-only →
/// local wins; both → both forfeit.
fn equivocation_outcome(local_is_p1: bool, local_eq: bool, counter_eq: bool) -> u8 {
    if local_eq && counter_eq {
        return XKIND_TIMEOUT_BOTH_FORFEIT;
    }
    if local_eq {
        return if local_is_p1 {
            XKIND_TIMEOUT_P2_WINS
        } else {
            XKIND_TIMEOUT_P1_WINS
        };
    }
    if local_is_p1 {
        XKIND_TIMEOUT_P1_WINS
    } else {
        XKIND_TIMEOUT_P2_WINS
    }
}

// ---------------------------------------------------------------------------
// Refund paths — every non-happy path conserves value
// ---------------------------------------------------------------------------

pub fn refund_xmatch_nocert(ctx: Context<RefundXMatch>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let m = &ctx.accounts.xmatch;
    require!(
        m.status == XChainStatus::Funded,
        CoordinationError::XInvalidStatus
    );
    require!(
        now > m.fund_deadline,
        CoordinationError::XDeadlineNotReached
    );
    let amount = m.stake_lamports;
    ctx.accounts.xmatch.status = XChainStatus::RefundedNoCert;
    transfer_lamports(
        &ctx.accounts.xmatch.to_account_info(),
        &ctx.accounts.player,
        amount,
    )?;
    emit!(XMatchRefunded {
        match_id: ctx.accounts.xmatch.match_id,
        to_player: amount,
    });
    Ok(())
}

pub fn refund_xmatch_timeout(ctx: Context<RefundXMatchTimeout>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let pool = &ctx.accounts.pool;
    let m = &ctx.accounts.xmatch;
    require!(
        m.status == XChainStatus::Locked || m.status == XChainStatus::Claiming,
        CoordinationError::XInvalidStatus
    );
    // Opens at match_deadline + max_claim_window + 2*skew — strictly after
    // any leg's settle window (claim_window_secs <= max_claim_window).
    let opens_at = i128::from(m.match_deadline)
        .checked_add(i128::from(pool.max_claim_window_secs))
        .and_then(|v| v.checked_add(2i128.checked_mul(i128::from(pool.skew_margin_secs))?))
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    require!(
        i128::from(now) > opens_at,
        CoordinationError::XDeadlineNotReached
    );

    let tranche = m.tranche_lamports;
    let amount = m.stake_lamports;
    ctx.accounts.xmatch.status = XChainStatus::RefundedTimeout;
    // Release the locked tranche back to the pool's free balance.
    let pool = &mut ctx.accounts.pool;
    pool.locked_lamports = pool
        .locked_lamports
        .checked_sub(tranche)
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    pool.free_lamports = pool
        .free_lamports
        .checked_add(tranche)
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    transfer_lamports(
        &ctx.accounts.xmatch.to_account_info(),
        &ctx.accounts.player,
        amount,
    )?;
    emit!(XMatchRefunded {
        match_id: ctx.accounts.xmatch.match_id,
        to_player: amount,
    });
    Ok(())
}

// ---------------------------------------------------------------------------
// Shared verification + execution
// ---------------------------------------------------------------------------

fn verify_checkpoint(
    cert: &MatchLiveCertArg,
    cp: &CheckpointArg,
    sigs: &[[u8; 65]; 2],
) -> Result<()> {
    let digest = cp.digest();
    require_signer(&digest, &sigs[0], &cert.leg_a.session_key)?;
    require_signer(&digest, &sigs[1], &cert.leg_b.session_key)?;
    Ok(())
}

/// keccak256 of this leg's CAIP-2 chain string — the chain tag a cert's
/// leg A must carry. Mirrors crates/chain-registry (Solana devnet row); a
/// cert for any other chain fails the leg-A check. The program ID is the
/// contract binding.
fn solana_chain_tag() -> [u8; 32] {
    crate::cert::keccak256(b"solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1")
}

/// Reconstruct leg A (the Solana leg) from authoritative on-chain match
/// state. Every field is determined by the program ID + recorded match, so
/// the caller never supplies it — `settle_xmatch` rebuilds it here.
fn rebuild_leg_a(m: &XChainMatch, chain_tag: [u8; 32]) -> CertLegArg {
    CertLegArg {
        chain_tag,
        contract: crate::ID.to_bytes(),
        player: m.player.to_bytes(),
        session_key: m.session_key,
        stake: u128::from(m.stake_lamports),
        tranche: u128::from(m.tranche_lamports),
    }
}

/// Verify the match-live cert's leg A against locally recorded escrow state
/// (existence != agreement), domain binding, quote freshness, and the three
/// signatures (both session keys + operator). Returns the cert digest.
fn verify_match_live(
    m: &XChainMatch,
    cert: &MatchLiveCertArg,
    sigs: &[[u8; 65]; 3],
    pool: &XPayoutPool,
    chain_tag: [u8; 32],
) -> Result<[u8; 32]> {
    let leg = &cert.leg_a; // leg A is ALWAYS the Solana leg
    let program_id = crate::ID.to_bytes();
    require!(
        leg.chain_tag == chain_tag
            && leg.contract == program_id
            && leg.player == m.player.to_bytes()
            && leg.session_key == m.session_key
            && cert.leg_b.session_key == m.counter_session_key
            && u64::try_from(leg.stake).unwrap_or(u64::MAX) == m.stake_lamports
            && u64::try_from(leg.tranche).unwrap_or(u64::MAX) == m.tranche_lamports
            && cert.match_deadline == u64::try_from(m.match_deadline).unwrap_or(0)
            && (cert.a_is_p1 == 1) == (m.player_is_p1 == 1),
        CoordinationError::XCertMismatch
    );
    // Quote freshness: quoted no earlier than max_age before the lock.
    let quote_ok = u128::from(cert.quote_timestamp)
        .checked_add(u128::from(cert.quote_max_age_secs))
        .ok_or(CoordinationError::ArithmeticOverflow)?
        >= u128::try_from(m.locked_at).unwrap_or(0);
    require!(quote_ok, CoordinationError::XStaleQuote);
    let _ = pool;

    let digest = cert.digest();
    require_signer(&digest, &sigs[0], &cert.leg_a.session_key)?;
    require_signer(&digest, &sigs[1], &cert.leg_b.session_key)?;
    require_signer(&digest, &sigs[2], &pool.operator_signer)?;
    Ok(digest)
}

/// Route one leg's funds per `resolve_xleg` and move lamports (CEI: state
/// already advanced by the caller). Stake lives in the match PDA, tranche in
/// the pool PDA; winners draw from both, forfeits split treasury/pool/prize.
#[allow(clippy::too_many_arguments)]
fn execute_xleg<'info>(
    outcome_kind: u8,
    xmatch: &Account<'info, XChainMatch>,
    pool: &mut Account<'info, XPayoutPool>,
    tournament: &mut Account<'info, Tournament>,
    player_profile: &mut Account<'info, PlayerProfile>,
    global_config: &GlobalConfig,
    treasury: &AccountInfo<'info>,
    player: &AccountInfo<'info>,
) -> Result<()> {
    let stake = xmatch.stake_lamports;
    let tranche = xmatch.tranche_lamports;
    let local_is_p1 = xmatch.player_is_p1 == 1;
    let r: XLegResolution = resolve_xleg(
        outcome_kind,
        local_is_p1,
        stake,
        tranche,
        global_config.treasury_split_bps,
    )?;

    // Pool accounting: the locked tranche always leaves `locked`; the
    // released portion plus any reimbursement returns to `free`.
    pool.locked_lamports = pool
        .locked_lamports
        .checked_sub(tranche)
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    let back_to_free = r
        .tranche_released
        .checked_add(r.to_pool_reimburse)
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    pool.free_lamports = pool
        .free_lamports
        .checked_add(back_to_free)
        .ok_or(CoordinationError::ArithmeticOverflow)?;

    // Lamport moves. The tranche consumed by a win comes from the pool PDA;
    // everything else comes from the match PDA's stake.
    let tranche_consumed = tranche
        .checked_sub(r.tranche_released)
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    let from_match = r
        .to_treasury
        .checked_add(r.to_pool_reimburse)
        .and_then(|v| v.checked_add(r.to_prize))
        .and_then(|v| v.checked_add(r.to_player.checked_sub(tranche_consumed)?))
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    // Postcondition: the match PDA pays out EXACTLY its stake (never its
    // rent reserve), and the pool pays exactly the consumed tranche.
    require!(from_match == stake, CoordinationError::ArithmeticOverflow);
    if tranche_consumed > 0 {
        transfer_lamports(&pool.to_account_info(), player, tranche_consumed)?;
    }
    let from_match_to_player = r
        .to_player
        .checked_sub(tranche_consumed)
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    if from_match_to_player > 0 {
        transfer_lamports(&xmatch.to_account_info(), player, from_match_to_player)?;
    }
    if r.to_treasury > 0 {
        transfer_lamports(&xmatch.to_account_info(), treasury, r.to_treasury)?;
    }
    if r.to_pool_reimburse > 0 {
        transfer_lamports(
            &xmatch.to_account_info(),
            &pool.to_account_info(),
            r.to_pool_reimburse,
        )?;
    }
    if r.to_prize > 0 {
        transfer_lamports(
            &xmatch.to_account_info(),
            &tournament.to_account_info(),
            r.to_prize,
        )?;
        tournament.prize_lamports = tournament
            .prize_lamports
            .checked_add(r.to_prize)
            .ok_or(CoordinationError::ArithmeticOverflow)?;
    }

    // Update the local player's profile (the counterparty's profile updates
    // on the other leg from the identical cert).
    let won = r.to_player > stake;
    player_profile.update_after_game(won, xmatch.tournament_id)?;

    emit!(XMatchSettled {
        match_id: xmatch.match_id,
        outcome_kind,
        to_player: r.to_player,
        to_treasury: r.to_treasury,
    });
    Ok(())
}

// ---------------------------------------------------------------------------
// Account contexts
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct InitializeXPool<'info> {
    #[account(
        init,
        payer = authority,
        space = XPayoutPool::SPACE,
        seeds = [b"xpool"],
        bump,
    )]
    pub pool: Account<'info, XPayoutPool>,
    #[account(
        seeds = [b"global_config"],
        bump = global_config.bump,
        constraint = global_config.authority == authority.key() @ CoordinationError::XBadConfig,
    )]
    pub global_config: Account<'info, GlobalConfig>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct XPoolDeposit<'info> {
    #[account(mut, seeds = [b"xpool"], bump = pool.bump)]
    pub pool: Account<'info, XPayoutPool>,
    #[account(mut)]
    pub funder: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct XPoolWithdraw<'info> {
    #[account(
        mut,
        seeds = [b"xpool"],
        bump = pool.bump,
        constraint = pool.operator == operator.key() @ CoordinationError::XBadConfig,
    )]
    pub pool: Account<'info, XPayoutPool>,
    #[account(mut)]
    pub operator: Signer<'info>,
}

#[derive(Accounts)]
#[instruction(match_id: [u8; 32])]
pub struct CreateXMatch<'info> {
    #[account(
        init,
        payer = player,
        space = XChainMatch::SPACE,
        seeds = [b"xmatch", match_id.as_ref()],
        bump,
    )]
    pub xmatch: Account<'info, XChainMatch>,
    #[account(seeds = [b"global_config"], bump = global_config.bump)]
    pub global_config: Account<'info, GlobalConfig>,
    /// Matchmaker co-signs; does not pay gas.
    pub matchmaker: Signer<'info>,
    #[account(mut)]
    pub player: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(cert: MatchLiveCertArg)]
pub struct LockXTranche<'info> {
    #[account(
        mut,
        seeds = [b"xmatch", cert.match_id.as_ref()],
        bump = xmatch.bump,
    )]
    pub xmatch: Account<'info, XChainMatch>,
    #[account(
        mut,
        seeds = [b"xpool"],
        bump = pool.bump,
    )]
    pub pool: Account<'info, XPayoutPool>,
    /// Permissionless fee payer — authorization is the operator signature on the
    /// cert, not this account. The caller pays the tx fee (gas external).
    pub cranker: Signer<'info>,
}

#[derive(Accounts)]
#[instruction(cert: MatchLiveCertArg)]
pub struct SettleXMatch<'info> {
    #[account(
        mut,
        seeds = [b"xmatch", cert.match_id.as_ref()],
        bump = xmatch.bump,
    )]
    pub xmatch: Account<'info, XChainMatch>,
    #[account(mut, seeds = [b"xpool"], bump = pool.bump)]
    pub pool: Account<'info, XPayoutPool>,
    #[account(
        mut,
        seeds = [b"tournament", xmatch.tournament_id.to_le_bytes().as_ref()],
        bump = tournament.bump,
    )]
    pub tournament: Account<'info, Tournament>,
    #[account(
        init_if_needed,
        payer = cranker,
        space = PlayerProfile::SPACE,
        seeds = [
            b"player",
            xmatch.tournament_id.to_le_bytes().as_ref(),
            xmatch.player.as_ref(),
        ],
        bump,
    )]
    pub player_profile: Account<'info, PlayerProfile>,
    #[account(
        seeds = [b"global_config"],
        bump = global_config.bump,
        constraint = global_config.treasury == treasury.key() @ CoordinationError::XCertMismatch,
    )]
    pub global_config: Account<'info, GlobalConfig>,
    /// CHECK: validated to equal global_config.treasury above.
    #[account(mut)]
    pub treasury: AccountInfo<'info>,
    /// CHECK: validated to equal the recorded match player below.
    #[account(mut, constraint = player.key() == xmatch.player @ CoordinationError::XCertMismatch)]
    pub player: AccountInfo<'info>,
    /// Pays profile rent if the cross-chain player has none yet for this
    /// tournament. Settle stays permissionless — anyone can crank.
    #[account(mut)]
    pub cranker: Signer<'info>,
    pub system_program: Program<'info, System>,
}

/// Terminal match whose stake/tranche have been distributed: reclaim rent.
/// Permissionless; rent returns to the recorded player.
pub fn close_xmatch(ctx: Context<CloseXMatch>) -> Result<()> {
    let status = ctx.accounts.xmatch.status;
    require!(
        matches!(
            status,
            XChainStatus::Settled
                | XChainStatus::ClaimSettled
                | XChainStatus::RefundedNoCert
                | XChainStatus::RefundedTimeout
        ),
        CoordinationError::XInvalidStatus
    );
    Ok(())
}

#[derive(Accounts)]
#[instruction(cert: MatchLiveCertArg)]
pub struct OpenXClaim<'info> {
    #[account(mut, seeds = [b"xmatch", cert.match_id.as_ref()], bump = xmatch.bump)]
    pub xmatch: Account<'info, XChainMatch>,
    #[account(seeds = [b"xpool"], bump = pool.bump)]
    pub pool: Account<'info, XPayoutPool>,
}

#[derive(Accounts)]
#[instruction(cert: MatchLiveCertArg)]
pub struct SupersedeXClaim<'info> {
    #[account(mut, seeds = [b"xmatch", cert.match_id.as_ref()], bump = xmatch.bump)]
    pub xmatch: Account<'info, XChainMatch>,
}

#[derive(Accounts)]
#[instruction(cert: MatchLiveCertArg)]
pub struct SubmitEquivocation<'info> {
    #[account(mut, seeds = [b"xmatch", cert.match_id.as_ref()], bump = xmatch.bump)]
    pub xmatch: Account<'info, XChainMatch>,
    #[account(seeds = [b"xpool"], bump = pool.bump)]
    pub pool: Account<'info, XPayoutPool>,
}

#[derive(Accounts)]
pub struct SettleXClaim<'info> {
    #[account(
        mut,
        seeds = [b"xmatch", xmatch.match_id.as_ref()],
        bump = xmatch.bump,
    )]
    pub xmatch: Account<'info, XChainMatch>,
    #[account(mut, seeds = [b"xpool"], bump = pool.bump)]
    pub pool: Account<'info, XPayoutPool>,
    #[account(
        mut,
        seeds = [b"tournament", xmatch.tournament_id.to_le_bytes().as_ref()],
        bump = tournament.bump,
    )]
    pub tournament: Account<'info, Tournament>,
    #[account(
        init_if_needed,
        payer = cranker,
        space = PlayerProfile::SPACE,
        seeds = [
            b"player",
            xmatch.tournament_id.to_le_bytes().as_ref(),
            xmatch.player.as_ref(),
        ],
        bump,
    )]
    pub player_profile: Account<'info, PlayerProfile>,
    #[account(
        seeds = [b"global_config"],
        bump = global_config.bump,
        constraint = global_config.treasury == treasury.key() @ CoordinationError::XCertMismatch,
    )]
    pub global_config: Account<'info, GlobalConfig>,
    /// CHECK: validated to equal global_config.treasury above.
    #[account(mut)]
    pub treasury: AccountInfo<'info>,
    /// CHECK: validated to equal the recorded match player below.
    #[account(mut, constraint = player.key() == xmatch.player @ CoordinationError::XCertMismatch)]
    pub player: AccountInfo<'info>,
    /// Pays profile rent if the cross-chain player has none yet for this
    /// tournament. Settle stays permissionless — anyone can crank.
    #[account(mut)]
    pub cranker: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct RefundXMatch<'info> {
    #[account(
        mut,
        seeds = [b"xmatch", xmatch.match_id.as_ref()],
        bump = xmatch.bump,
    )]
    pub xmatch: Account<'info, XChainMatch>,
    /// CHECK: validated to equal the recorded match player.
    #[account(mut, constraint = player.key() == xmatch.player @ CoordinationError::XCertMismatch)]
    pub player: AccountInfo<'info>,
}

#[derive(Accounts)]
pub struct RefundXMatchTimeout<'info> {
    #[account(
        mut,
        seeds = [b"xmatch", xmatch.match_id.as_ref()],
        bump = xmatch.bump,
    )]
    pub xmatch: Account<'info, XChainMatch>,
    #[account(mut, seeds = [b"xpool"], bump = pool.bump)]
    pub pool: Account<'info, XPayoutPool>,
    /// CHECK: validated to equal the recorded match player.
    #[account(mut, constraint = player.key() == xmatch.player @ CoordinationError::XCertMismatch)]
    pub player: AccountInfo<'info>,
}

#[derive(Accounts)]
pub struct CloseXMatch<'info> {
    #[account(
        mut,
        seeds = [b"xmatch", xmatch.match_id.as_ref()],
        bump = xmatch.bump,
        close = player,
    )]
    pub xmatch: Account<'info, XChainMatch>,
    /// CHECK: rent recipient, validated to equal the recorded match player.
    #[account(mut, constraint = player.key() == xmatch.player @ CoordinationError::XCertMismatch)]
    pub player: AccountInfo<'info>,
}
