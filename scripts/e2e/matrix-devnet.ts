/**
 * LIVE devnet OUTCOME MATRIX — the full terminal-outcome axis, driven against the
 * real devnet shillbot program with the shared harness + steps. Deterministic
 * (kind-1 attested → the attester sets score 0/MAX, no async verifier), windows
 * shrunk to seconds so challenge/dispute/expire resolve in-run.
 *
 * Covers every terminal the on-chain state machine has:
 *   finalize-paid · challenge→agent-wins · challenge→challenger-wins ·
 *   dispute→default-resolve · expire-refund
 * Each cell asserts the realized agent + treasury lamport deltas EXACTLY against
 * deriveTaskOutcome (the same payout oracle as bankrun/EVM), and challenger ≥ its
 * expected bond return. This is the live counterpart of tests/shillbot-matrix.ts.
 *
 * Keys (persistent): client/global-authority = id.json, attester/oracle_authority
 * = test.json, agent = shillbot-game-platform-agent.json, challenger =
 * shillbot-x-agent.json. NOT CI (real devnet, mutates global params). Run:
 *   DEVNET_RPC=$(gcloud secrets versions access latest --secret=solana-rpc-url-devnet --project coordination-game-prod) \
 *   npx tsx scripts/e2e/matrix-devnet.ts
 */
import { BN } from "@coral-xyz/anchor";
import { Keypair, PublicKey } from "@solana/web3.js";
import {
  connectDevnet,
  devnetCtx,
  Checker,
  withShrunkWindows,
  ensureFunded,
  loadKeypair,
  sleep,
  DevnetHarness,
} from "./devnet-harness";
import {
  ShillbotCtx,
  createTask,
  claimTask,
  submitWork,
  verifyTaskAttested,
  challengeTask,
  resolveChallenge,
  finalizeTask,
  expireTask,
  assertTerminalPayout,
  LEAN_PROOF_PLATFORM,
} from "../../tests/harness/shillbot-steps";
import {
  deriveTaskOutcome,
  computePayment,
  TaskOutcomeKind,
  TaskPayout,
  MAX_SCORE,
} from "../../sdk/task-outcome-oracle";

const ESCROW = new BN(2_000_000); // 0.002 SOL
const CHALLENGE_WINDOW = 8;
const VERIFICATION_TIMEOUT = 20;
const K = TaskOutcomeKind;

interface Cell {
  name: string;
  score: number;
  outcome: TaskOutcomeKind;
}

const CELLS: Cell[] = [
  {
    name: "finalize score=MAX (agent paid)",
    score: MAX_SCORE,
    outcome: K.Finalized,
  },
  { name: "finalize score=0 (full refund)", score: 0, outcome: K.Finalized },
  {
    name: "agent-wins score=MAX (payment + bond slashed)",
    score: MAX_SCORE,
    outcome: K.ResolvedAgentWins,
  },
  {
    name: "agent-wins score=0 (0 payment + bond slashed)",
    score: 0,
    outcome: K.ResolvedAgentWins,
  },
  {
    name: "challenger-wins score=MAX (escrow refunded, bond returned)",
    score: MAX_SCORE,
    outcome: K.ResolvedChallengerWins,
  },
  { name: "expire (refund from Submitted)", score: 0, outcome: K.Expired },
];

// default-resolve is OMITTED live: dispute_resolution_window has an on-chain MIN
// of 1 hour, so the permissionless crank can't fire in-run. It IS covered by the
// bankrun matrix (tests/shillbot-matrix.ts) with clock-warp.

async function main(): Promise<void> {
  const h = connectDevnet();
  const ctx = await devnetCtx(h);
  const chk = new Checker();
  const challenger = loadKeypair("shillbot-x-agent.json");

  console.log(`client/authority: ${h.authority.publicKey.toBase58()}`);
  console.log(`attester:         ${h.attester.publicKey.toBase58()}`);
  console.log(`agent:            ${h.agent.publicKey.toBase58()}`);
  console.log(`challenger:       ${challenger.publicKey.toBase58()}`);

  await ensureFunded(h, h.attester.publicKey, 20_000_000);
  await ensureFunded(h, h.agent.publicKey, 10_000_000);
  await ensureFunded(h, challenger.publicKey, 20_000_000);

  await withShrunkWindows(
    h,
    { challenge: CHALLENGE_WINDOW, verificationTimeout: VERIFICATION_TIMEOUT },
    async () => {
      for (const cell of CELLS) {
        try {
          await runCell(h, ctx, chk, challenger, cell);
        } catch (e) {
          chk.check(false, `${cell.name}: threw ${String(e).slice(0, 160)}`);
        }
      }
    }
  );

  chk.finish();
}

function scenarioFor(
  h: DevnetHarness,
  g: {
    qualityThreshold: BN;
    protocolFeeBps: number;
    challengeBondMultiplierBps: number;
    bondSlashTreasuryBps: number;
  },
  cell: Cell
) {
  return {
    escrowLamports: BigInt(ESCROW.toString()),
    qualityThreshold: g.qualityThreshold.toNumber(),
    protocolFeeBps: g.protocolFeeBps,
    compositeScore: cell.score,
    verificationKind: 1 as const,
    challengeBondMultiplier: g.challengeBondMultiplierBps,
    bondSlashTreasuryBps: g.bondSlashTreasuryBps,
    outcome: cell.outcome,
  };
}

async function runCell(
  h: DevnetHarness,
  ctx: ShillbotCtx,
  chk: Checker,
  challenger: Keypair,
  cell: Cell
): Promise<void> {
  console.log(`\n=== ${cell.name} ===`);
  const created = await createTask(ctx, h.authority, {
    escrowLamports: ESCROW,
    platform: LEAN_PROOF_PLATFORM,
    verificationKind: 1,
    requiresApproval: false,
    challengeWindowOverride: 0, // use the shrunk global window
    contentTag: `matrix-${cell.outcome}-${cell.score}-${Date.now()}`,
  });
  await claimTask(ctx, h.agent, created.task);
  await submitWork(
    ctx,
    h.agent,
    created.task,
    "https://artifacts.example/proof.lean"
  );

  const g = await h.program.account.globalState.fetch(h.globalPda);
  const expected = deriveTaskOutcome(scenarioFor(h, g as never, cell));

  if (cell.outcome === K.Expired) {
    const t = await h.program.account.task.fetch(created.task);
    const expiry = t.submittedAt.toNumber() + VERIFICATION_TIMEOUT + 4;
    await sleepUntil(h, expiry);
    await assertTerminal(h, ctx, chk, cell, expected, challenger, async () =>
      expireTask(ctx, created.task, h.agent.publicKey, h.authority.publicKey)
    );
    return;
  }

  // Reach Verified (attester-signed, deterministic — no async verifier).
  await verifyTaskAttested(
    ctx,
    created.task,
    created.taskId,
    cell.score,
    h.attester
  );

  // Assert the PINNED on-chain payment/fee match the oracle — a clean check
  // that survives the shared devnet treasury's concurrent activity (unlike a
  // treasury-balance delta, which other tests/users pollute).
  const pinned = computePayment(
    cell.score,
    (g.qualityThreshold as BN).toNumber(),
    BigInt(ESCROW.toString()),
    g.protocolFeeBps
  );
  const vt = await h.program.account.task.fetch(created.task);
  chk.check(
    (vt.paymentAmount as BN).toString() === pinned.payment.toString(),
    `${cell.name}: pinned payment ${vt.paymentAmount} == oracle ${pinned.payment}`
  );
  chk.check(
    (vt.feeAmount as BN).toString() === pinned.fee.toString(),
    `${cell.name}: pinned fee ${vt.feeAmount} == oracle ${pinned.fee}`
  );

  if (cell.outcome === K.Finalized) {
    await sleep((CHALLENGE_WINDOW + 3) * 1000);
    await assertTerminal(h, ctx, chk, cell, expected, challenger, async () =>
      finalizeTask(ctx, created.task, h.agent.publicKey, h.authority.publicKey)
    );
    return;
  }

  // Challenge-based terminals (agent-wins / challenger-wins).
  const challenge = await challengeTask(
    ctx,
    challenger,
    created.task,
    created.taskId
  );
  const challengerWon = cell.outcome === K.ResolvedChallengerWins;
  await assertTerminal(h, ctx, chk, cell, expected, challenger, async () =>
    resolveChallenge(
      ctx,
      created.task,
      challenge,
      h.agent.publicKey,
      h.authority.publicKey,
      challenger.publicKey,
      challengerWon
    )
  );
}

/** Measure agent/challenger deltas across the terminal action and assert them
 *  against the oracle. The body now lives in tests/harness/shillbot-steps.ts as
 *  `assertTerminalPayout` — it was module-private here while five other cells
 *  needed exactly this and each inlined a weaker version (or none). This wrapper
 *  keeps the local call sites unchanged. */
async function assertTerminal(
  h: DevnetHarness,
  _ctx: ShillbotCtx,
  chk: Checker,
  cell: Cell,
  expected: TaskPayout,
  challenger: Keypair,
  action: () => Promise<void>
): Promise<void> {
  await assertTerminalPayout(
    (pk) => bal(h, pk),
    { agent: h.agent.publicKey, challenger: challenger.publicKey },
    expected,
    cell.name,
    (ok, msg) => chk.check(ok, msg),
    action
  );
}

async function bal(h: DevnetHarness, pk: PublicKey): Promise<bigint> {
  return BigInt(await h.connection.getBalance(pk, "confirmed"));
}

/** Sleep until the on-chain-ish wall clock passes `unixTs` (uses local time; the
 *  program reads the cluster clock, so pad the target by a few seconds). */
async function sleepUntil(h: DevnetHarness, unixTs: number): Promise<void> {
  const now = Math.floor(Date.now() / 1000);
  const wait = Math.max(0, unixTs - now);
  if (wait > 0) await sleep(wait * 1000);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
