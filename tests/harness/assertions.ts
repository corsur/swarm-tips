// The property battery — what a COMPOSED scenario can verify when its end state
// is variable. Every check here is a function of the REALIZED run (the Ledger +
// Transcript + a StateView), not a hardcoded outcome, so arbitrary step
// combinations stay verifiable. See the harness README for the model.

import { assert } from "chai";
import {
  deriveClaimOutcome,
  resolvePayoff,
  OutcomeKind,
  Transcript as OracleInput,
} from "../helpers/outcome-oracle";
import { Ledger } from "./ledger";

/**
 * Minimal read-only view of realized state the battery inspects. Each Target
 * (Slice 2+) implements it for its runtime (Solana account, EVM call, backend
 * HTTP/Firestore); Slice 1 exercises the battery against an in-memory fake.
 */
export interface StateView {
  /** Canonical status label, e.g. "Resolved" / "ClaimSettled". */
  readStatus(): Promise<string>;
  /** Realized OutcomeKind (0..8), or -1 if the scenario has no outcome yet. */
  readOutcomeKind(): Promise<number>;
}

/** Aggregate of one scenario run, used for metamorphic comparison across targets. */
export interface RunResult {
  outcome: OutcomeKind;
  protocolNet: bigint;
}

// (1) Conservation -----------------------------------------------------------

/**
 * Protocol-controlled lamports/wei net to ZERO exactly; the whole tracked system
 * (protocol + players) loses only `feesLamports` to the network. Holds for any
 * composition — value is never created or destroyed. Requires the Ledger to have
 * opened+closed every value-bearing account the scenario touched.
 */
export function assertConservation(
  ledger: Ledger,
  opts: { feesLamports?: bigint } = {}
): void {
  const protocolNet = ledger.netByKind("protocol");
  assert.equal(
    protocolNet.toString(),
    "0",
    "conservation: protocol lamports not conserved"
  );
  const fees = opts.feesLamports ?? 0n;
  const total = protocolNet + ledger.netByKind("player");
  assert.equal(
    total.toString(),
    (-fees).toString(),
    "conservation: total value changed by more than fees"
  );
}

// (2) Oracle-from-realized-transcript ---------------------------------------

/**
 * The chain's realized outcome must equal what the shared oracle DERIVES from
 * what actually happened. The outcome is computed from the transcript, never
 * assumed, so a variable end state is fine.
 */
export async function assertOracleOutcome(
  view: StateView,
  realized: OracleInput
): Promise<OutcomeKind> {
  const expected = deriveClaimOutcome(realized);
  const actual = await view.readOutcomeKind();
  assert.equal(
    actual,
    expected,
    `oracle: on-chain ${OutcomeKind[actual] ?? actual} != derived ${
      OutcomeKind[expected]
    }`
  );
  return expected;
}

/**
 * Stronger same-chain terminal check: the realized protocol-account deltas match
 * the oracle's payoff split exactly. `tournamentGain` is the protocol's net take
 * (treasury + prize), so for a conserving same-chain game the protocol net equals
 * the oracle's tournamentGain.
 */
export function assertPayoffMatchesLedger(
  ledger: Ledger,
  realized: OracleInput & { stake: bigint }
): void {
  const payoff = resolvePayoff(realized);
  assert.equal(
    ledger.netByKind("protocol").toString(),
    payoff.tournamentGain.toString(),
    "payoff: protocol net != oracle tournamentGain"
  );
}

// (3) State-machine legality -------------------------------------------------

/** Adjacency list of allowed status transitions. Terminal statuses map to []. */
export type TransitionGraph = Record<string, string[]>;

/** Every observed transition must be in the allowed graph. */
export function assertLegalTransition(
  graph: TransitionGraph,
  from: string,
  to: string
): void {
  const allowed = graph[from] ?? [];
  assert.include(
    allowed,
    to,
    `state machine: illegal transition ${from} -> ${to}`
  );
}

/** A terminal status must be absorbing (no outgoing transitions). */
export function assertTerminal(graph: TransitionGraph, status: string): void {
  assert.deepEqual(
    graph[status] ?? [],
    [],
    `state machine: '${status}' expected terminal but has outgoing transitions`
  );
}

// (4) Cross-layer agreement --------------------------------------------------

/** All independent views of the same scenario must report the same status. */
export async function assertCrossLayer(
  views: StateView[],
  context = "cross-layer"
): Promise<void> {
  assert.isAtLeast(views.length, 2, `${context}: needs >= 2 views`);
  const statuses = await Promise.all(views.map((v) => v.readStatus()));
  const head = statuses[0];
  for (const s of statuses.slice(1)) {
    assert.equal(
      s,
      head,
      `${context}: views disagree (${statuses.join(" vs ")})`
    );
  }
}

// (5) Metamorphic / cross-target equivalence ---------------------------------

/** The same scenario on two targets must reach the same oracle outcome and the
 *  same protocol-net movement. Verification without a fixed answer. */
export function assertMetamorphic(
  a: RunResult,
  b: RunResult,
  context = "metamorphic"
): void {
  assert.equal(
    a.outcome,
    b.outcome,
    `${context}: outcomes differ (${OutcomeKind[a.outcome]} vs ${
      OutcomeKind[b.outcome]
    })`
  );
  assert.equal(
    a.protocolNet.toString(),
    b.protocolNet.toString(),
    `${context}: protocol nets differ`
  );
}

// (6) Monotonic bounds -------------------------------------------------------

/** A quantity that must never decrease across a step (wins, treasury on forfeit). */
export function assertNonDecreasing(
  prev: bigint | number,
  next: bigint | number,
  label: string
): void {
  assert.isAtLeast(Number(next), Number(prev), `monotonic: ${label} decreased`);
}

/** A quantity that must strictly increase (e.g. supersede best_step_count). */
export function assertStrictlyIncreasing(
  prev: bigint | number,
  next: bigint | number,
  label: string
): void {
  assert.isAbove(
    Number(next),
    Number(prev),
    `monotonic: ${label} not strictly increasing`
  );
}
