// Task-outcome oracle — the canonical Shillbot payout matrix, in TypeScript.
//
// This is the off-chain test/tooling mirror of the on-chain payout logic that
// three implementations must agree on (pinned by the golden vectors in
// tests/fixtures/task-payout-vectors.json):
//
//   - programs/shillbot/src/scoring.rs           `compute_payment`
//   - programs/shillbot/src/instructions/resolve_challenge.rs  `slash_bond`
//   - evm/src/ShillbotEscrow.sol                 payment + bond split
//
// The combinatorial harness uses it as the single source of truth for "what
// SHOULD this task pay out" so every scenario — Solana bankrun, Solana devnet,
// EVM ShillbotEscrow, MCP-driven, browser-driven — asserts on-chain reality
// against ONE expectation instead of re-deriving the formula per test. If this
// drifts from the on-chain payout, the parity test + the combinatorial suites
// fail.
//
// Unlike the coordination game (where the outcome is DERIVED from the realized
// transcript), a Shillbot task's terminal PATH is a choice of which crank/
// resolution ran — finalize vs challenge-resolution vs dispute-liveness default
// vs expiry. The scenario carries that choice; the oracle computes the exact
// lamport split it implies. Roles are explicit (client creates, agent claims),
// so there is no FIFO "expected family" — the split is fully determined.

export const MAX_SCORE = 1_000_000;

/** The terminal settlement path a task took. */
export enum TaskOutcomeKind {
  /** Challenge window elapsed unchallenged → finalize_task pays the agent. */
  Finalized = 0,
  /** Challenged, authority resolves agent-favor → payment + bond slashed to agent/treasury. */
  ResolvedAgentWins = 1,
  /** Challenged, authority resolves challenger-favor → escrow to client, bond returned. */
  ResolvedChallengerWins = 2,
  /** Disputed, permissionless liveness crank → pinned payment, bond un-slashed. */
  DefaultResolved = 3,
  /** Verification timeout / deadline → expire_task refunds the client in full. */
  Expired = 4,
}

export interface TaskScenario {
  escrowLamports: bigint;
  qualityThreshold: number;
  protocolFeeBps: number;
  compositeScore: number;
  /** 0 = OracleMetrics (Switchboard), 1 = DeterministicAttested (score ∈ {0, MAX_SCORE}). */
  verificationKind: 0 | 1;
  /** Challenge bond = multiplier × escrow; bounded [2, 10] on-chain. */
  challengeBondMultiplier: number;
  /** Treasury share of a slashed bond, in basis points (e.g. 5000 = 50%). */
  bondSlashTreasuryBps: number;
  outcome: TaskOutcomeKind;
}

/** Realized escrow+bond distribution, EXCLUDING rent (the harness tolerates rent
 *  on player accounts via `>=` and reconciles protocol accounts exactly). */
export interface TaskPayout {
  kind: TaskOutcomeKind;
  agentLamports: bigint;
  treasuryLamports: bigint;
  /** Escrow returned to the client (remainder on payment, or full refund). */
  clientLamports: bigint;
  /** Bond returned to the challenger (0 when the bond was slashed away). */
  challengerLamports: bigint;
}

export interface PaymentSplit {
  payment: bigint;
  fee: bigint;
  remainder: bigint;
}

/**
 * Mirror of `scoring.rs::compute_payment` (payment, fee) plus the escrow
 * remainder. Floor division throughout, u128-safe via bigint. Throws on the
 * same preconditions the program asserts (score/threshold ≤ MAX_SCORE).
 */
export function computePayment(
  compositeScore: number,
  qualityThreshold: number,
  escrowLamports: bigint,
  protocolFeeBps: number
): PaymentSplit {
  if (compositeScore > MAX_SCORE) {
    throw new Error(`score ${compositeScore} exceeds MAX_SCORE ${MAX_SCORE}`);
  }
  if (qualityThreshold > MAX_SCORE) {
    throw new Error(
      `threshold ${qualityThreshold} exceeds MAX_SCORE ${MAX_SCORE}`
    );
  }
  if (escrowLamports < 0n) throw new Error("escrow must be non-negative");

  if (compositeScore < qualityThreshold) {
    return { payment: 0n, fee: 0n, remainder: escrowLamports };
  }
  const scoreRange = MAX_SCORE - qualityThreshold;
  // threshold == MAX_SCORE ⇒ no score can pay (division by zero guard).
  if (scoreRange === 0) {
    return { payment: 0n, fee: 0n, remainder: escrowLamports };
  }
  const scoreAbove = BigInt(compositeScore - qualityThreshold);
  const gross = (escrowLamports * scoreAbove) / BigInt(scoreRange);
  const fee = (gross * BigInt(protocolFeeBps)) / 10_000n;
  const payment = gross - fee;
  const remainder = escrowLamports - payment - fee;

  // Postcondition mirrors the program: payment + fee ≤ escrow.
  if (payment + fee > escrowLamports) {
    throw new Error("payment + fee exceeds escrow");
  }
  return { payment, fee, remainder };
}

/** Challenge bond = multiplier × escrow, bounded [2, 10] like the program. */
export function computeChallengeBond(
  escrowLamports: bigint,
  multiplier: number
): bigint {
  if (multiplier < 2 || multiplier > 10) {
    throw new Error(`bond multiplier ${multiplier} out of [2, 10]`);
  }
  return escrowLamports * BigInt(multiplier);
}

/** Treasury/agent split of a slashed bond, mirror of `resolve_challenge.rs::slash_bond`. */
export function splitSlashedBond(
  bond: bigint,
  bondSlashTreasuryBps: number
): { treasuryShare: bigint; agentShare: bigint } {
  const treasuryShare = (bond * BigInt(bondSlashTreasuryBps)) / 10_000n;
  return { treasuryShare, agentShare: bond - treasuryShare };
}

/**
 * The canonical payout derivation. Given a task scenario (its escrow, score,
 * verification kind, and the terminal PATH it took), returns the exact
 * escrow+bond distribution the on-chain program produces. Conserves value:
 * escrow + bond is fully redistributed (asserted before return).
 */
export function deriveTaskOutcome(s: TaskScenario): TaskPayout {
  // Kind-1 (attested) enforces a binary score on-chain — surface a drift early.
  if (
    s.verificationKind === 1 &&
    s.compositeScore !== 0 &&
    s.compositeScore !== MAX_SCORE
  ) {
    throw new Error(
      `attested (kind 1) score must be 0 or MAX_SCORE, got ${s.compositeScore}`
    );
  }

  const { payment, fee, remainder } = computePayment(
    s.compositeScore,
    s.qualityThreshold,
    s.escrowLamports,
    s.protocolFeeBps
  );
  const bond = computeChallengeBond(
    s.escrowLamports,
    s.challengeBondMultiplier
  );

  const payout = build(s.outcome, {
    escrow: s.escrowLamports,
    bond,
    payment,
    fee,
    remainder,
    bondSlashTreasuryBps: s.bondSlashTreasuryBps,
  });

  assertConserves(s.outcome, payout, s.escrowLamports, bond);
  return payout;
}

interface Parts {
  escrow: bigint;
  bond: bigint;
  payment: bigint;
  fee: bigint;
  remainder: bigint;
  bondSlashTreasuryBps: number;
}

function build(kind: TaskOutcomeKind, p: Parts): TaskPayout {
  switch (kind) {
    case TaskOutcomeKind.Finalized:
      return {
        kind,
        agentLamports: p.payment,
        treasuryLamports: p.fee,
        clientLamports: p.remainder,
        challengerLamports: 0n,
      };
    case TaskOutcomeKind.ResolvedAgentWins: {
      const { treasuryShare, agentShare } = splitSlashedBond(
        p.bond,
        p.bondSlashTreasuryBps
      );
      return {
        kind,
        agentLamports: p.payment + agentShare,
        treasuryLamports: p.fee + treasuryShare,
        clientLamports: p.remainder,
        challengerLamports: 0n,
      };
    }
    case TaskOutcomeKind.ResolvedChallengerWins:
      return {
        kind,
        agentLamports: 0n,
        treasuryLamports: 0n,
        clientLamports: p.escrow,
        challengerLamports: p.bond,
      };
    case TaskOutcomeKind.DefaultResolved:
      return {
        kind,
        agentLamports: p.payment,
        treasuryLamports: p.fee,
        clientLamports: p.remainder,
        challengerLamports: p.bond,
      };
    case TaskOutcomeKind.Expired:
      return {
        kind,
        agentLamports: 0n,
        treasuryLamports: 0n,
        clientLamports: p.escrow,
        challengerLamports: 0n,
      };
    default:
      throw new Error(`unknown TaskOutcomeKind ${kind}`);
  }
}

/** Every path redistributes exactly (escrow) or (escrow + bond) of value. */
function assertConserves(
  kind: TaskOutcomeKind,
  o: TaskPayout,
  escrow: bigint,
  bond: bigint
): void {
  const total =
    o.agentLamports +
    o.treasuryLamports +
    o.clientLamports +
    o.challengerLamports;
  // A challenge occurred (bond posted) in every kind except Finalized/Expired.
  const bondInPlay =
    kind === TaskOutcomeKind.ResolvedAgentWins ||
    kind === TaskOutcomeKind.ResolvedChallengerWins ||
    kind === TaskOutcomeKind.DefaultResolved;
  const expected = bondInPlay ? escrow + bond : escrow;
  if (total !== expected) {
    throw new Error(
      `payout conservation broken for ${TaskOutcomeKind[kind]}: ${total} != ${expected}`
    );
  }
}
