/**
 * Devnet e2e — the DEPLOYED LeanProof ATTESTER SERVICE, end to end.
 *
 * Unlike scripts/e2e/attested-lifecycle-devnet.ts (which signs
 * verify_task_attested locally with test.json to prove the on-chain
 * instruction), this test exercises the ACTUAL deployed shillbot-attester pod:
 * it drives the orchestrator REST API (api.shillbot.org) to create a LeanProof
 * campaign + fund + claim + submit, then POSTs /internal/run-attest/{task_id}
 * so the pod fetches the proof artifact, runs the pinned Lean v4.31.0 check
 * under policy v1, and lands verify_task_attested on-chain as the devnet
 * oracle_authority (== test.json == the pod's shillbot-attester-keypair).
 *
 *   create LeanProof campaign (statement_lean = `def statementProp : Prop := True`)
 *     → fund (client-signed create_task, content_hash = sha256(statement_lean))
 *     → agent claim + submit (content_id = gist raw URL of the proof artifact)
 *     → POST /internal/run-attest (deployed pod runs Lean, lands verify_task_attested)
 *     → assert on-chain Verified + score + payment == deriveTaskOutcome
 *     → finalize (SDK) → assert agent paid / client refunded vs the oracle
 *
 * PASS proof  (`theorem proof : statementProp := trivial`)  → score = MAX_SCORE, agent paid.
 * FAIL proof  (`theorem proof : statementProp := by sorry`) → sorryAx ∉ allow-list → score 0, client refunded.
 *
 * Proof artifacts live at tests/fixtures/lean/{pass,fail}.lean; the defaults
 * below are their raw.githubusercontent URLs (resolve once this repo's commit
 * is pushed). The attester fetches HTTPS only. Override with PASS_PROOF_URL /
 * FAIL_PROOF_URL — the first live run (2026-07-08) used gist mirrors before the
 * fixtures were committed:
 *   PASS: https://gist.githubusercontent.com/corsur/d035dac4ff095e9565e9be2f10bd4bc5/raw/pass.lean
 *   FAIL: https://gist.githubusercontent.com/corsur/7eced376d3db00cbe4c9bfa3e5f31396/raw/fail.lean
 *
 * Keys (persistent, no ephemeral keypairs): client/authority = id.json,
 * agent = shillbot-game-platform-agent.json, attester = test.json (the pod's key).
 *
 * NOT a CI test (real devnet, live pod, mutates the global challenge window
 * for the run and restores it). Run manually:
 *   DEVNET_RPC="$(gcloud secrets versions access latest \
 *       --secret=solana-rpc-url-devnet --project coordination-game-prod)" \
 *   npx tsx tests/live/shillbot-attester-e2e-testnet.ts    (exit 0 = pass)
 */
import { BN } from "@coral-xyz/anchor";
import {
  Connection,
  Keypair,
  PublicKey,
  Transaction,
  VersionedTransaction,
} from "@solana/web3.js";
import {
  connectDevnet,
  devnetCtx,
  Checker,
  withChallengeWindow,
  ensureFunded,
  sleep,
  DevnetHarness,
} from "../../scripts/e2e/devnet-harness";
import {
  ShillbotCtx,
  finalizeTask,
  LEAN_PROOF_PLATFORM,
} from "../harness/shillbot-steps";
import {
  deriveTaskOutcome,
  TaskOutcomeKind,
  MAX_SCORE,
} from "../../sdk/task-outcome-oracle";

const API_BASE = process.env.SHILLBOT_API ?? "https://api.shillbot.org";
const ESCROW = new BN(2_000_000); // 0.002 SOL
const DEMO_WINDOW = 8; // seconds — shrunk global challenge window for the run
const STATEMENT = "def statementProp : Prop := True";

const RAW_BASE =
  "https://raw.githubusercontent.com/corsur/swarm-tips/main/tests/fixtures/lean";
const PASS_PROOF_URL = process.env.PASS_PROOF_URL ?? `${RAW_BASE}/pass.lean`;
const FAIL_PROOF_URL = process.env.FAIL_PROOF_URL ?? `${RAW_BASE}/fail.lean`;

// --- REST + signing helpers -------------------------------------------------

async function api(
  method: string,
  path: string,
  bearer: string,
  body?: unknown
): Promise<Record<string, unknown>> {
  const res = await fetch(API_BASE + path, {
    method,
    headers: {
      Authorization: `Bearer ${bearer}`,
      "Content-Type": "application/json",
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await res.text();
  if (!res.ok) throw new Error(`${method} ${path} -> ${res.status}: ${text}`);
  return text ? (JSON.parse(text) as Record<string, unknown>) : {};
}

/** Sign an unsigned base64 tx (legacy or versioned) with `signer`. */
function signBase64(unsigned: string, signer: Keypair): string {
  const raw = Buffer.from(unsigned, "base64");
  try {
    const vtx = VersionedTransaction.deserialize(raw);
    vtx.sign([signer]);
    return Buffer.from(vtx.serialize()).toString("base64");
  } catch {
    const tx = Transaction.from(raw);
    tx.partialSign(signer);
    return tx.serialize({ requireAllSignatures: false }).toString("base64");
  }
}

async function broadcast(conn: Connection, signedB64: string): Promise<string> {
  const raw = Buffer.from(signedB64, "base64");
  const sig = await conn.sendRawTransaction(raw, {
    skipPreflight: false,
    maxRetries: 5,
  });
  await conn.confirmTransaction(sig, "confirmed");
  return sig;
}

/** On-chain Task.state byte offset (layout pinned in shillbot-attester/solana.rs). */
const TASK_STATE_OFFSET = 80;
const STATE_SUBMITTED = 2;

/** The deployed attester reads Task accounts at FINALIZED commitment (Solana
 *  RpcClient default) and requires state Submitted(2). A freshly-submitted task
 *  is only `confirmed`, and its Open→Claimed→Submitted transitions finalize a
 *  few slots apart, so run-attest would see it absent or still Open until the
 *  submit_work finalizes. The mainnet workflow's 5-min attest_delay absorbs
 *  exactly this lag; here we wait for the finalized state to reach Submitted. */
async function awaitFinalizedSubmitted(
  conn: Connection,
  key: PublicKey,
  label: string
): Promise<void> {
  const MAX_ATTEMPTS = 60; // ~180s at 3s cadence
  for (let i = 0; i < MAX_ATTEMPTS; i++) {
    const acc = await conn.getAccountInfo(key, "finalized");
    if (acc !== null && acc.data[TASK_STATE_OFFSET] === STATE_SUBMITTED) return;
    await sleep(3000);
  }
  throw new Error(
    `${label} not finalized in Submitted state after ${MAX_ATTEMPTS} polls`
  );
}

/** build (orchestrator) → sign → broadcast → confirm (orchestrator). */
async function buildSignConfirm(
  conn: Connection,
  bearer: string,
  signer: Keypair,
  built: Record<string, unknown>,
  taskId: string,
  action: string,
  taskPda?: string
): Promise<void> {
  const unsigned = built.transaction as string;
  if (!unsigned) throw new Error(`${action}: no transaction in response`);
  const sig = await broadcast(conn, signBase64(unsigned, signer));
  await api("POST", `/tasks/${taskId}/confirm?network=devnet`, bearer, {
    tx_signature: sig,
    action,
    ...(taskPda ? { task_pda: taskPda } : {}),
  });
}

// --- lifecycle --------------------------------------------------------------

async function runLifecycle(
  h: DevnetHarness,
  ctx: ShillbotCtx,
  chk: Checker,
  campaignId: string,
  proofUrl: string,
  expectedScore: number,
  label: string
): Promise<void> {
  console.log(`\n=== ${label} ===`);
  const conn = h.connection;
  const clientPk = h.authority.publicKey.toBase58();
  const agentPk = h.agent.publicKey.toBase58();

  // Fund → create_task (client signs, content_hash = sha256(statement_lean)).
  const fund = await api(
    "POST",
    `/campaigns/${campaignId}/fund?network=devnet`,
    clientPk,
    { amount_lamports: Number(ESCROW.toString()) }
  );
  const taskId = fund.task_id as string;
  const taskPda = fund.task_pda as string;
  console.log(`  task_id=${taskId} task_pda=${taskPda}`);
  await buildSignConfirm(
    conn,
    clientPk,
    h.authority,
    fund,
    taskId,
    "create",
    taskPda
  );

  // Claim (agent) then submit the proof-artifact URL (agent).
  const claim = await api(
    "POST",
    `/tasks/${taskId}/claim?network=devnet`,
    agentPk
  );
  await buildSignConfirm(conn, agentPk, h.agent, claim, taskId, "claim");

  const submit = await api(
    "POST",
    `/tasks/${taskId}/submit?network=devnet`,
    agentPk,
    { content_id: proofUrl }
  );
  await buildSignConfirm(conn, agentPk, h.agent, submit, taskId, "submit");
  console.log(`  submitted content_id=${proofUrl}`);

  // The pod reads at finalized commitment — wait for the submitted Task to
  // finalize before triggering, else run-attest sees it absent.
  const taskKey = new PublicKey(taskPda);
  console.log("  waiting for finalized Submitted state of the task...");
  await awaitFinalizedSubmitted(conn, taskKey, "submitted task");

  // Trigger the DEPLOYED attester (public run-attest proxy → in-cluster pod).
  const attest = await api("POST", `/internal/run-attest/${taskId}`, clientPk, {
    task_id: taskId,
  });
  console.log(`  attester response: ${JSON.stringify(attest)}`);
  const landedScore = Number(attest.score);
  chk.check(
    landedScore === expectedScore,
    `attester landed score ${landedScore} (expected ${expectedScore})`
  );
  chk.check(
    typeof attest.tx === "string" && (attest.tx as string).length > 0,
    `verify_task_attested tx: ${attest.tx}`
  );

  // Assert the on-chain Task the pod just verified.
  const verified = await h.program.account.task.fetch(taskKey);
  chk.check(
    JSON.stringify(verified.state) === JSON.stringify({ verified: {} }),
    "on-chain state = Verified"
  );
  chk.check(
    verified.verificationKind === 1,
    "verification_kind = 1 (DeterministicAttested)"
  );

  const g = await h.program.account.globalState.fetch(h.globalPda);
  const expected = deriveTaskOutcome({
    escrowLamports: BigInt(ESCROW.toString()),
    qualityThreshold: (g.qualityThreshold as BN).toNumber(),
    protocolFeeBps: g.protocolFeeBps,
    compositeScore: expectedScore,
    verificationKind: 1,
    challengeBondMultiplier: g.challengeBondMultiplierBps,
    bondSlashTreasuryBps: g.bondSlashTreasuryBps,
    outcome: TaskOutcomeKind.Finalized,
  });
  chk.check(
    (verified.paymentAmount as BN).toString() ===
      expected.agentLamports.toString(),
    `on-chain payment_amount == oracle agentLamports (${expected.agentLamports})`
  );

  // Finalize (permissionless SDK call) once the shrunk challenge window passes.
  await sleep((DEMO_WINDOW + 4) * 1000);
  const agentBefore = await conn.getBalance(h.agent.publicKey, "confirmed");
  await finalizeTask(ctx, taskKey, h.agent.publicKey, h.authority.publicKey);
  const closed = await conn.getAccountInfo(taskKey, "confirmed");
  chk.check(closed === null, "task account closed after finalize");
  const agentAfter = await conn.getBalance(h.agent.publicKey, "confirmed");
  chk.check(
    BigInt(agentAfter - agentBefore) === expected.agentLamports,
    `agent balance delta == oracle agentLamports (${expected.agentLamports})`
  );
}

async function main(): Promise<void> {
  const h = connectDevnet();
  const ctx = await devnetCtx(h);
  const chk = new Checker();

  console.log(`API:              ${API_BASE}`);
  console.log(`authority/client: ${h.authority.publicKey.toBase58()}`);
  console.log(`agent:            ${h.agent.publicKey.toBase58()}`);
  console.log(`attester (pod):   ${h.attester.publicKey.toBase58()}`);

  // The DEPLOYED pod signs + pays verify_task_attested from test.json's
  // on-chain balance, and it must be the devnet oracle_authority.
  const g = await h.program.account.globalState.fetch(h.globalPda);
  chk.check(
    (g.oracleAuthority as { toBase58(): string }).toBase58() ===
      h.attester.publicKey.toBase58(),
    "devnet oracle_authority == test.json (the deployed attester key)"
  );
  await ensureFunded(h, h.attester.publicKey, 30_000_000);
  await ensureFunded(h, h.agent.publicKey, 10_000_000);

  // One LeanProof campaign — statement is identical for both cases; the
  // pass/fail difference is entirely in the submitted proof artifact.
  const camp = await api(
    "POST",
    `/campaigns?network=devnet`,
    h.authority.publicKey.toBase58(),
    {
      brief: {
        topic: "lean attester deterministic verification e2e",
        brand_voice: "precise",
        cta: "verify the proof",
        utm_link: "https://swarm.tips",
      },
      budget_lamports: Number(ESCROW.toString()) * 4,
      platform: LEAN_PROOF_PLATFORM,
      statement_lean: STATEMENT,
    }
  );
  const campaignId = camp.campaign_id as string;
  console.log(`campaign:         ${campaignId}`);

  await withChallengeWindow(h, DEMO_WINDOW, async () => {
    await runLifecycle(
      h,
      ctx,
      chk,
      campaignId,
      PASS_PROOF_URL,
      MAX_SCORE,
      "PASS proof (trivial) → score MAX, agent paid"
    );
    await runLifecycle(
      h,
      ctx,
      chk,
      campaignId,
      FAIL_PROOF_URL,
      0,
      "FAIL proof (by sorry) → score 0, client refunded"
    );
  });

  chk.finish();
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
