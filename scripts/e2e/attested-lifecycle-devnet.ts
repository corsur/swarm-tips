/**
 * Devnet e2e — the deterministic ATTESTED verification lifecycle against the
 * LIVE devnet shillbot program, driven through the SHARED harness + steps:
 *
 *   create_task(kind=1, LeanProof) → claim → submit_work → verify_task_attested
 *   (signed by the armed attester = devnet oracle_authority = test.json) →
 *   finalize → payment asserted against deriveTaskOutcome (the SAME payout oracle
 *   the bankrun matrix uses). This is the live "cell" for the attested path.
 *
 * Plus the failing-proof branch: a second task attested with score 0 finalizes
 * with zero payment (full escrow refunded to the client).
 *
 * Keys are persistent (no ephemeral keypairs): client/authority = id.json,
 * attester = test.json, agent = shillbot-game-platform-agent.json (arms-length).
 * The dispute (challenge→defaultResolve) path needs a ≥1h live wait and is
 * exhaustively covered by the bankrun matrix + guards with clock warping, so it
 * is not re-proven here.
 *
 * NOT a CI test (real devnet, mutates global params). Run manually:
 *   npx tsx scripts/e2e/attested-lifecycle-devnet.ts   (exit 0 = pass)
 */
import { BN } from "@coral-xyz/anchor";
import {
  connectDevnet,
  devnetCtx,
  Checker,
  withChallengeWindow,
  ensureFunded,
  sleep,
  DevnetHarness,
} from "./devnet-harness";
import {
  ShillbotCtx,
  createTask,
  claimTask,
  submitWork,
  verifyTaskAttested,
  finalizeTask,
  LEAN_PROOF_PLATFORM,
} from "../../tests/harness/shillbot-steps";
import {
  deriveTaskOutcome,
  TaskOutcomeKind,
  MAX_SCORE,
} from "../../sdk/task-outcome-oracle";

const ESCROW = new BN(2_000_000); // 0.002 SOL
const DEMO_WINDOW = 6; // seconds — shrunk global challenge window for the demo

async function main(): Promise<void> {
  const h = connectDevnet();
  const ctx = await devnetCtx(h);
  const chk = new Checker();

  console.log(`authority/client: ${h.authority.publicKey.toBase58()}`);
  console.log(`attester:         ${h.attester.publicKey.toBase58()}`);
  console.log(`agent:            ${h.agent.publicKey.toBase58()}`);

  const g = await h.program.account.globalState.fetch(h.globalPda);
  chk.check(
    (g.oracleAuthority as { toBase58(): string }).toBase58() ===
      h.attester.publicKey.toBase58(),
    "devnet oracle_authority == test.json (the armed attester)"
  );
  await ensureFunded(h, h.attester.publicKey, 20_000_000);
  await ensureFunded(h, h.agent.publicKey, 6_000_000);

  await withChallengeWindow(h, DEMO_WINDOW, async () => {
    await runLifecycle(
      h,
      ctx,
      chk,
      MAX_SCORE,
      "happy path (score = MAX): agent paid"
    );
    await runLifecycle(
      h,
      ctx,
      chk,
      0,
      "failing proof (score = 0): full refund"
    );
  });

  chk.finish();
}

async function runLifecycle(
  h: DevnetHarness,
  ctx: ShillbotCtx,
  chk: Checker,
  score: number,
  label: string
): Promise<void> {
  console.log(`\n=== ${label} ===`);
  const created = await createTask(ctx, h.authority, {
    escrowLamports: ESCROW,
    platform: LEAN_PROOF_PLATFORM,
    verificationKind: 1,
    requiresApproval: false,
    challengeWindowOverride: 0, // use the shrunk global window
    contentTag: `theorem-${Date.now()}`,
  });
  const createdTask = await h.program.account.task.fetch(created.task);
  chk.check(createdTask.verificationKind === 1, "verification_kind = 1");

  await claimTask(ctx, h.agent, created.task);
  await submitWork(
    ctx,
    h.agent,
    created.task,
    "https://artifacts.example/proof.lean"
  );
  await verifyTaskAttested(
    ctx,
    created.task,
    created.taskId,
    score,
    h.attester
  );

  const verified = await h.program.account.task.fetch(created.task);
  chk.check(
    JSON.stringify(verified.state) === JSON.stringify({ verified: {} }),
    "state = Verified"
  );

  const g = await h.program.account.globalState.fetch(h.globalPda);
  const expected = deriveTaskOutcome({
    escrowLamports: BigInt(ESCROW.toString()),
    qualityThreshold: (g.qualityThreshold as BN).toNumber(),
    protocolFeeBps: g.protocolFeeBps,
    compositeScore: score,
    verificationKind: 1,
    challengeBondMultiplier: g.challengeBondMultiplierBps,
    bondSlashTreasuryBps: g.bondSlashTreasuryBps,
    outcome: TaskOutcomeKind.Finalized,
  });
  chk.check(
    (verified.paymentAmount as BN).toString() ===
      expected.agentLamports.toString(),
    `payment == oracle agentLamports (${expected.agentLamports})`
  );

  await sleep((DEMO_WINDOW + 3) * 1000);
  const agentBefore = await h.connection.getBalance(
    h.agent.publicKey,
    "confirmed"
  );
  await finalizeTask(
    ctx,
    created.task,
    h.agent.publicKey,
    h.authority.publicKey
  );
  const closed = await h.connection.getAccountInfo(created.task, "confirmed");
  chk.check(closed === null, "task account closed after finalize");
  const agentAfter = await h.connection.getBalance(
    h.agent.publicKey,
    "confirmed"
  );
  chk.check(
    BigInt(agentAfter - agentBefore) === expected.agentLamports,
    `agent delta == oracle agentLamports (${expected.agentLamports})`
  );
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
