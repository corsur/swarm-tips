// Shillbot combinatorial cell matrix (bankrun) — the shillbot twin of
// tests/game-harness.ts. One `it` per cell, generated over the confirmed axes:
//
//   verification_kind {0 oracle, 1 attested}
//     × platform {YouTube (kind 0), LeanProof (kind 1)}
//     × score {below-threshold, threshold-exact, mid, MAX / binary 0|MAX}
//     × requires_approval {false, true}
//     × terminal outcome {finalize-paid, challenge→agent-wins,
//                          challenge→challenger-wins, default-resolve, expire}
//
// Each cell plays the composed P1 steps against a fresh task and verifies the
// realized Ledger against deriveTaskOutcome — the single payout oracle pinned to
// the program by the golden vectors. Path-independent checks per cell:
//   - agent + treasury deltas EXACT (neither signs the terminal tx nor receives
//     account-close rent, so their movement is exactly the escrow/bond payout);
//   - client + challenger deltas ≥ payout (they also receive close rent);
//   - whole-system conservation == 0 (the untracked provider wallet pays gas);
//   - the derived TaskOutcomeKind matches the cell's terminal path.
//
// Pruned to legal representatives (< ~60 cells): kind-1 ⇒ LeanProof + binary
// score; the challenge/default/expire paths use one representative score since
// the payout FORMULA across scores is already swept by the golden fixture +
// shillbot-surfaces.unit.test.ts — these cells prove the PROGRAM matches the
// oracle, not the formula. Runs under node 20 (see the harness README gating note).

import { BN } from "@coral-xyz/anchor";
import { Keypair, LAMPORTS_PER_SOL, PublicKey } from "@solana/web3.js";
import { assert } from "chai";

import { startShillbotBankrun } from "./harness/shillbot-bankrun";
import {
  ShillbotCtx,
  setupShillbot,
  createTask,
  claimTask,
  submitWork,
  approveTask,
  verifyTaskOracle,
  verifyTaskAttested,
  challengeTask,
  resolveChallenge,
  resolveChallengeDefault,
  finalizeTask,
  expireTask,
  challengeDeadline,
  LEAN_PROOF_PLATFORM,
} from "./harness/shillbot-steps";
import {
  deriveTaskOutcome,
  TaskOutcomeKind,
  TaskScenario,
} from "./helpers/task-outcome-oracle";
import { Ledger, AccountKind } from "./harness/ledger";
import { assertConservation } from "./harness/assertions";

const ESCROW = new BN(1 * LAMPORTS_PER_SOL);
const CHALLENGE_WINDOW = 3600;
const YOUTUBE_PLATFORM = 0;
const K = TaskOutcomeKind;

interface Cell {
  name: string;
  kind: 0 | 1;
  platform: number;
  score: number;
  requiresApproval: boolean;
  outcome: TaskOutcomeKind;
}

function kind0Cells(): Cell[] {
  const cells: Cell[] = [];
  // Finalize path: full score × approval sweep (the common path).
  for (const score of [1, 200_000, 600_000, 1_000_000]) {
    for (const requiresApproval of [false, true]) {
      cells.push({
        name: `kind0 finalize score=${score} approval=${requiresApproval}`,
        kind: 0,
        platform: YOUTUBE_PLATFORM,
        score,
        requiresApproval,
        outcome: K.Finalized,
      });
    }
  }
  // Challenge / default / expire: one representative score each.
  cells.push({
    name: "kind0 challenger-wins score=600k",
    kind: 0,
    platform: YOUTUBE_PLATFORM,
    score: 600_000,
    requiresApproval: false,
    outcome: K.ResolvedChallengerWins,
  });
  cells.push({
    name: "kind0 challenger-wins below-threshold (escrow refunded)",
    kind: 0,
    platform: YOUTUBE_PLATFORM,
    score: 1,
    requiresApproval: false,
    outcome: K.ResolvedChallengerWins,
  });
  cells.push({
    name: "kind0 agent-wins score=600k (bond slashed)",
    kind: 0,
    platform: YOUTUBE_PLATFORM,
    score: 600_000,
    requiresApproval: true,
    outcome: K.ResolvedAgentWins,
  });
  cells.push({
    name: "kind0 default-resolve score=600k (bond un-slashed)",
    kind: 0,
    platform: YOUTUBE_PLATFORM,
    score: 600_000,
    requiresApproval: false,
    outcome: K.DefaultResolved,
  });
  cells.push({
    name: "kind0 expire (refund from Submitted)",
    kind: 0,
    platform: YOUTUBE_PLATFORM,
    score: 0,
    requiresApproval: false,
    outcome: K.Expired,
  });
  return cells;
}

function kind1Cells(): Cell[] {
  const cells: Cell[] = [];
  // Finalize path: binary score × approval sweep.
  for (const score of [0, 1_000_000]) {
    for (const requiresApproval of [false, true]) {
      cells.push({
        name: `kind1 finalize score=${score} approval=${requiresApproval}`,
        kind: 1,
        platform: LEAN_PROOF_PLATFORM,
        score,
        requiresApproval,
        outcome: K.Finalized,
      });
    }
  }
  cells.push({
    name: "kind1 challenger-wins score=MAX",
    kind: 1,
    platform: LEAN_PROOF_PLATFORM,
    score: 1_000_000,
    requiresApproval: false,
    outcome: K.ResolvedChallengerWins,
  });
  cells.push({
    name: "kind1 agent-wins score=MAX (bond slashed)",
    kind: 1,
    platform: LEAN_PROOF_PLATFORM,
    score: 1_000_000,
    requiresApproval: false,
    outcome: K.ResolvedAgentWins,
  });
  cells.push({
    name: "kind1 default-resolve score=MAX",
    kind: 1,
    platform: LEAN_PROOF_PLATFORM,
    score: 1_000_000,
    requiresApproval: true,
    outcome: K.DefaultResolved,
  });
  cells.push({
    name: "kind1 expire (refund from Submitted)",
    kind: 1,
    platform: LEAN_PROOF_PLATFORM,
    score: 0,
    requiresApproval: false,
    outcome: K.Expired,
  });
  return cells;
}

const CELLS: Cell[] = [...kind0Cells(), ...kind1Cells()];

describe("shillbot-matrix (bankrun, combinatorial)", () => {
  let handle: Awaited<ReturnType<typeof startShillbotBankrun>>;
  let ctx: ShillbotCtx;
  const authority = Keypair.generate();
  const treasury = Keypair.generate();

  before(async function () {
    this.timeout(120_000);
    handle = await startShillbotBankrun();
    for (const kp of [authority, treasury]) {
      await handle.runtime.fund(kp.publicKey, 100 * LAMPORTS_PER_SOL);
    }
    ctx = await setupShillbot(handle.runtime, {
      authority,
      treasury: treasury.publicKey,
      primeFeed: handle.primeFeed,
    });
  });

  it(`generates ${CELLS.length} legal cells (< 60)`, () => {
    assert.isBelow(CELLS.length, 60);
    assert.isAbove(CELLS.length, 15);
  });

  for (const cell of CELLS) {
    it(cell.name, async function () {
      this.timeout(60_000);
      await runCell(ctx, handle, cell, treasury.publicKey);
    });
  }
});

interface Actors {
  client: Keypair;
  agent: Keypair;
  challenger: Keypair;
}

async function freshActors(
  handle: Awaited<ReturnType<typeof startShillbotBankrun>>
): Promise<Actors> {
  const client = Keypair.generate();
  const agent = Keypair.generate();
  const challenger = Keypair.generate();
  for (const kp of [client, agent, challenger]) {
    await handle.runtime.fund(kp.publicKey, 100 * LAMPORTS_PER_SOL);
  }
  return { client, agent, challenger };
}

async function scenarioFor(
  ctx: ShillbotCtx,
  cell: Cell
): Promise<TaskScenario> {
  const g = await ctx.rt.program.account.globalState.fetch(ctx.globalPda);
  return {
    escrowLamports: BigInt(ESCROW.toString()),
    qualityThreshold: g.qualityThreshold.toNumber(),
    protocolFeeBps: g.protocolFeeBps,
    compositeScore: cell.score,
    verificationKind: cell.kind,
    challengeBondMultiplier: g.challengeBondMultiplierBps,
    bondSlashTreasuryBps: g.bondSlashTreasuryBps,
    outcome: cell.outcome,
  };
}

/** Snapshot helper: open a set of labelled accounts on a Ledger. */
async function openAll(
  ctx: ShillbotCtx,
  ledger: Ledger,
  accts: Record<string, { pk: PublicKey; kind: AccountKind }>
): Promise<void> {
  for (const [label, a] of Object.entries(accts)) {
    ledger.open(label, a.kind, await ctx.rt.getBalance(a.pk));
  }
}

async function closeAll(
  ctx: ShillbotCtx,
  ledger: Ledger,
  accts: Record<string, { pk: PublicKey; kind: AccountKind }>
): Promise<void> {
  for (const [label, a] of Object.entries(accts)) {
    ledger.close(label, await ctx.rt.getBalance(a.pk));
  }
}

async function runCell(
  ctx: ShillbotCtx,
  handle: Awaited<ReturnType<typeof startShillbotBankrun>>,
  cell: Cell,
  treasury: PublicKey
): Promise<void> {
  const { client, agent, challenger } = await freshActors(handle);
  const created = await createTask(ctx, client, {
    escrowLamports: ESCROW,
    platform: cell.platform,
    verificationKind: cell.kind,
    requiresApproval: cell.requiresApproval,
    challengeWindowOverride: CHALLENGE_WINDOW,
  });
  const { task, taskId } = created;
  await claimTask(ctx, agent, task);
  await submitWork(ctx, agent, task, `content-${taskId.toString()}`);

  const expected = deriveTaskOutcome(await scenarioFor(ctx, cell));

  if (cell.outcome === K.Expired) {
    await runExpire(ctx, task, agent, client, treasury, expected);
    return;
  }

  // Reach Verified.
  if (cell.requiresApproval) await approveTask(ctx, client, task);
  if (cell.kind === 0) {
    // Kind-0 (Switchboard) enforces an attestation-staleness window centered on
    // submitted_at + attestation_delay — warp there so the mock feed is fresh.
    // (Kind-1 attested has no feed/staleness gate; the attester signs directly.)
    await warpToAttestation(ctx, task);
    await verifyTaskOracle(ctx, task, cell.score);
  } else {
    await verifyTaskAttested(ctx, task, taskId, cell.score);
  }

  switch (cell.outcome) {
    case K.Finalized:
      return runFinalize(ctx, task, agent, client, treasury, expected);
    case K.ResolvedChallengerWins:
      return runResolve(
        ctx,
        task,
        taskId,
        agent,
        client,
        challenger,
        treasury,
        true,
        expected
      );
    case K.ResolvedAgentWins:
      return runResolve(
        ctx,
        task,
        taskId,
        agent,
        client,
        challenger,
        treasury,
        false,
        expected
      );
    case K.DefaultResolved:
      return runDefault(
        ctx,
        task,
        taskId,
        agent,
        client,
        challenger,
        treasury,
        expected
      );
    default:
      throw new Error(`unhandled outcome ${cell.outcome}`);
  }
}

/** Warp to submitted_at + effective attestation_delay so a kind-0 verify lands
 *  inside the staleness window (center of the ± staleness_window band). */
async function warpToAttestation(
  ctx: ShillbotCtx,
  task: PublicKey
): Promise<void> {
  const t = await ctx.rt.program.account.task.fetch(task);
  const g = await ctx.rt.program.account.globalState.fetch(ctx.globalPda);
  const delay =
    t.attestationDelayOverride > 0
      ? t.attestationDelayOverride
      : g.attestationDelaySeconds.toNumber();
  await ctx.rt.warpTo(t.submittedAt.toNumber() + delay);
}

function assertExact(actual: bigint, expected: bigint, label: string): void {
  assert.equal(actual.toString(), expected.toString(), `${label} delta`);
}

function assertAtLeast(actual: bigint, expected: bigint, label: string): void {
  assert.isTrue(
    actual >= expected,
    `${label} delta ${actual} < expected ${expected}`
  );
}

async function runFinalize(
  ctx: ShillbotCtx,
  task: PublicKey,
  agent: Keypair,
  client: Keypair,
  treasury: PublicKey,
  expected: ReturnType<typeof deriveTaskOutcome>
): Promise<void> {
  await ctx.rt.warpTo((await challengeDeadline(ctx, task)) + 1);
  const ledger = new Ledger();
  const accts = {
    task: { pk: task, kind: "protocol" as AccountKind },
    treasury: { pk: treasury, kind: "protocol" as AccountKind },
    agent: { pk: agent.publicKey, kind: "player" as AccountKind },
    client: { pk: client.publicKey, kind: "player" as AccountKind },
  };
  await openAll(ctx, ledger, accts);
  await finalizeTask(ctx, task, agent.publicKey, client.publicKey);
  await closeAll(ctx, ledger, accts);

  assertExact(ledger.delta("agent"), expected.agentLamports, "agent");
  assertExact(ledger.delta("treasury"), expected.treasuryLamports, "treasury");
  assertAtLeast(ledger.delta("client"), expected.clientLamports, "client");
  assertConservation(ledger, { feesLamports: 0n });
}

async function runResolve(
  ctx: ShillbotCtx,
  task: PublicKey,
  taskId: BN,
  agent: Keypair,
  client: Keypair,
  challenger: Keypair,
  treasury: PublicKey,
  challengerWon: boolean,
  expected: ReturnType<typeof deriveTaskOutcome>
): Promise<void> {
  const challenge = await challengeTask(ctx, challenger, task, taskId);
  const ledger = new Ledger();
  const accts = {
    task: { pk: task, kind: "protocol" as AccountKind },
    challenge: { pk: challenge, kind: "protocol" as AccountKind },
    treasury: { pk: treasury, kind: "protocol" as AccountKind },
    agent: { pk: agent.publicKey, kind: "player" as AccountKind },
    client: { pk: client.publicKey, kind: "player" as AccountKind },
    challenger: { pk: challenger.publicKey, kind: "player" as AccountKind },
  };
  await openAll(ctx, ledger, accts);
  await resolveChallenge(
    ctx,
    task,
    challenge,
    agent.publicKey,
    client.publicKey,
    challenger.publicKey,
    challengerWon
  );
  await closeAll(ctx, ledger, accts);

  assertExact(ledger.delta("agent"), expected.agentLamports, "agent");
  assertExact(ledger.delta("treasury"), expected.treasuryLamports, "treasury");
  assertAtLeast(ledger.delta("client"), expected.clientLamports, "client");
  assertAtLeast(
    ledger.delta("challenger"),
    expected.challengerLamports,
    "challenger"
  );
  assertConservation(ledger, { feesLamports: 0n });
}

async function runDefault(
  ctx: ShillbotCtx,
  task: PublicKey,
  taskId: BN,
  agent: Keypair,
  client: Keypair,
  challenger: Keypair,
  treasury: PublicKey,
  expected: ReturnType<typeof deriveTaskOutcome>
): Promise<void> {
  const challenge = await challengeTask(ctx, challenger, task, taskId);
  const ch = await ctx.rt.program.account.challenge.fetch(challenge);
  const g = await ctx.rt.program.account.globalState.fetch(ctx.globalPda);
  const deadline =
    ch.createdAt.toNumber() + g.disputeResolutionWindowSeconds.toNumber();
  await ctx.rt.warpTo(deadline + 1);

  const ledger = new Ledger();
  const accts = {
    task: { pk: task, kind: "protocol" as AccountKind },
    challenge: { pk: challenge, kind: "protocol" as AccountKind },
    treasury: { pk: treasury, kind: "protocol" as AccountKind },
    agent: { pk: agent.publicKey, kind: "player" as AccountKind },
    client: { pk: client.publicKey, kind: "player" as AccountKind },
    challenger: { pk: challenger.publicKey, kind: "player" as AccountKind },
  };
  await openAll(ctx, ledger, accts);
  await resolveChallengeDefault(
    ctx,
    task,
    challenge,
    agent.publicKey,
    client.publicKey,
    challenger.publicKey
  );
  await closeAll(ctx, ledger, accts);

  assertExact(ledger.delta("agent"), expected.agentLamports, "agent");
  assertExact(ledger.delta("treasury"), expected.treasuryLamports, "treasury");
  assertAtLeast(ledger.delta("client"), expected.clientLamports, "client");
  assertAtLeast(
    ledger.delta("challenger"),
    expected.challengerLamports,
    "challenger"
  );
  assertConservation(ledger, { feesLamports: 0n });
}

async function runExpire(
  ctx: ShillbotCtx,
  task: PublicKey,
  agent: Keypair,
  client: Keypair,
  treasury: PublicKey,
  expected: ReturnType<typeof deriveTaskOutcome>
): Promise<void> {
  const t = await ctx.rt.program.account.task.fetch(task);
  const g = await ctx.rt.program.account.globalState.fetch(ctx.globalPda);
  const expiry =
    t.submittedAt.toNumber() + g.verificationTimeoutSeconds.toNumber() + 1;
  await ctx.rt.warpTo(expiry);

  const ledger = new Ledger();
  const accts = {
    task: { pk: task, kind: "protocol" as AccountKind },
    treasury: { pk: treasury, kind: "protocol" as AccountKind },
    agent: { pk: agent.publicKey, kind: "player" as AccountKind },
    client: { pk: client.publicKey, kind: "player" as AccountKind },
  };
  await openAll(ctx, ledger, accts);
  await expireTask(ctx, task, agent.publicKey, client.publicKey);
  await closeAll(ctx, ledger, accts);

  assertExact(ledger.delta("agent"), expected.agentLamports, "agent");
  assertExact(ledger.delta("treasury"), expected.treasuryLamports, "treasury");
  assertAtLeast(ledger.delta("client"), expected.clientLamports, "client");
  assertConservation(ledger, { feesLamports: 0n });
}
