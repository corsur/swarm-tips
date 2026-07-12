/**
 * Devnet e2e — the DEPLOYED attester + lean-runner running REAL MATHLIB (policy v2).
 *
 * The v1 sibling (shillbot-attester-e2e-testnet.ts) proves the self-contained
 * path. This proves the mathlib path end to end: a v2 (lean_policy = 2) campaign
 * whose statement `import Mathlib`s and states `Nat.Prime 101`, attested by the
 * deployed shillbot-attester pod — which selects policy v2, ships mode=mathlib to
 * the zero-credential shillbot-lean-runner, elaborates against the baked mathlib
 * oleans, and lands verify_task_attested bound to the v2 policy id.
 *
 *   create v2 LeanProof campaign (lean_policy=2, statement imports Mathlib)
 *     → assert the campaign's display policy_id == LEAN_POLICY_V2_ID
 *     → fund + claim + submit the proof-artifact URL
 *     → POST /internal/run-attest (deployed attester+runner run mathlib)
 *     → assert on-chain Verified + score + payment == deriveTaskOutcome
 *     → finalize → assert agent paid / client refunded vs the oracle
 *
 * PASS proof (`by unfold statementProp; norm_num`) → score MAX, agent paid.
 * FAIL proof (`by sorry`)                          → sorryAx ∉ allow-list → 0, refund.
 *
 * WHY PASS PROVES V2: a self-contained (v1) check cannot elaborate `import
 * Mathlib` (no lake env) — it would score 0. A PASS on this mathlib-requiring
 * proof is only possible if the attester selected policy v2 and the mathlib
 * runner executed. So the PASS assertion IS the v2-selection proof; the policy_id
 * check is belt-and-suspenders.
 *
 * Proof convention: `statementProp` is a `def` (opaque to norm_num), so the proof
 * must `unfold statementProp` first. Fixtures: tests/fixtures/lean/v2-{pass,fail}.lean.
 *
 * Keys (persistent, no ephemeral): client/authority = id.json, agent =
 * shillbot-game-platform-agent.json, attester = test.json (the pod's key).
 *
 * NOT a CI test (real devnet, live pod, mutates the global challenge window and
 * restores it). Run manually:
 *   DEVNET_RPC="$(gcloud secrets versions access latest \
 *       --secret=solana-rpc-url-devnet --project coordination-game-prod)" \
 *   npx tsx tests/live/shillbot-attester-v2-e2e-testnet.ts   (exit 0 = pass)
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
// A mathlib prime fact; the def makes the goal opaque so the proof must unfold
// it (see fixtures). This statement is unstateable under v1. NOTE: a TARGETED
// import (`Mathlib.Tactic.NormNum.Prime`) elaborates in ~90s; the umbrella
// `import Mathlib` loads all ~5GB of oleans and times out at the 240s cap.
// Catalog statements (V2-5) must likewise import narrowly.
const STATEMENT =
  "import Mathlib.Tactic.NormNum.Prime\ndef statementProp : Prop := Nat.Prime 101";

// keccak256 of the v2 policy manifest — chain_core::verify_schema::LEAN_POLICY_V2_ID.
const LEAN_POLICY_V2_ID =
  "3fd32d5ba49af64e26ecefc7262234e14732feedbebcceaaac3c998c1a4d4ea0";

const RAW_BASE =
  "https://raw.githubusercontent.com/corsur/swarm-tips/main/tests/fixtures/lean";
const PASS_PROOF_URL = process.env.PASS_PROOF_URL ?? `${RAW_BASE}/v2-pass.lean`;
const FAIL_PROOF_URL = process.env.FAIL_PROOF_URL ?? `${RAW_BASE}/v2-fail.lean`;

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
const STATE_VERIFIED = 3;

/** The deployed attester reads Task accounts at FINALIZED commitment and requires
 *  state Submitted(2). Wait for the finalized state before triggering run-attest. */
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

/** Mathlib attest is ASYNC end to end: a mathlib proof elaborates in ~90s, but
 *  the GCP LB backend timeout 504s the synchronous client at ~30s while the
 *  attester keeps running and lands verify_task_attested SERVER-SIDE. (This is
 *  also the real production trigger — a Google Workflow, not a waiting client.)
 *  So we tolerate the run-attest response and poll the on-chain task to Verified. */
async function awaitVerified(
  conn: Connection,
  key: PublicKey,
  label: string
): Promise<void> {
  // Generous: the runner elaborates ~90s, but concurrent checks queue behind
  // the per-pod concurrency cap, so a busy runner can push a single attest well
  // past 90s. 8min covers the mathlib cap + queueing.
  const MAX_ATTEMPTS = 160; // ~480s at 3s cadence
  for (let i = 0; i < MAX_ATTEMPTS; i++) {
    const acc = await conn.getAccountInfo(key, "confirmed");
    if (acc !== null && acc.data[TASK_STATE_OFFSET] === STATE_VERIFIED) return;
    await sleep(3000);
  }
  throw new Error(`${label} not Verified after ${MAX_ATTEMPTS} polls`);
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

  const taskKey = new PublicKey(taskPda);
  console.log("  waiting for finalized Submitted state of the task...");
  await awaitFinalizedSubmitted(conn, taskKey, "submitted task");

  // Trigger the DEPLOYED attester+runner. Mathlib elaboration is ~90s; tolerate
  // the client-side 504 (the attester finishes server-side — see awaitVerified).
  console.log("  triggering attest (deployed pod runs mathlib, async ~90s)...");
  try {
    await api("POST", `/internal/run-attest/${taskId}`, clientPk, {
      task_id: taskId,
    });
  } catch (e) {
    console.log(
      `  run-attest client returned (tolerated): ${String(e).slice(0, 90)}`
    );
  }
  console.log(
    "  polling on-chain for Verified (attester lands it server-side)..."
  );
  await awaitVerified(conn, taskKey, "attested task");

  // Everything below is asserted from ON-CHAIN state — the deployed
  // attester+runner's verdict, not the (possibly-504'd) HTTP response. The
  // payment_amount == oracle check below IS the score assertion: a mathlib PASS
  // paying MAX is only reachable via policy v2 (v1 can't elaborate mathlib).
  const verified = await h.program.account.task.fetch(taskKey);
  chk.check(
    JSON.stringify(verified.state) === JSON.stringify({ verified: {} }),
    "on-chain state = Verified"
  );
  chk.check(
    verified.verificationKind === 1,
    "verification_kind = 1 (DeterministicAttested)"
  );
  const vhash = Buffer.from(verified.verificationHash as number[]).toString(
    "hex"
  );
  chk.check(
    /[1-9a-f]/.test(vhash),
    `on-chain verification_hash is non-zero (${vhash.slice(0, 16)}…)`
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

  const g = await h.program.account.globalState.fetch(h.globalPda);
  chk.check(
    (g.oracleAuthority as { toBase58(): string }).toBase58() ===
      h.attester.publicKey.toBase58(),
    "devnet oracle_authority == test.json (the deployed attester key)"
  );
  await ensureFunded(h, h.attester.publicKey, 30_000_000);
  await ensureFunded(h, h.agent.publicKey, 10_000_000);

  // One v2 (mathlib) LeanProof campaign — lean_policy = 2.
  const camp = await api(
    "POST",
    `/campaigns?network=devnet`,
    h.authority.publicKey.toBase58(),
    {
      brief: {
        topic: "lean attester mathlib (policy v2) verification e2e",
        brand_voice: "precise",
        cta: "verify the mathlib proof",
        utm_link: "https://swarm.tips",
      },
      budget_lamports: Number(ESCROW.toString()) * 4,
      platform: LEAN_PROOF_PLATFORM,
      statement_lean: STATEMENT,
      lean_policy: 2,
    }
  );
  const campaignId = camp.campaign_id as string;
  console.log(`campaign:         ${campaignId} (lean_policy=2)`);

  // The display-mirror policy_id must be the v2 id.
  const doc = await api(
    "GET",
    `/campaigns/${campaignId}?network=devnet`,
    h.authority.publicKey.toBase58()
  );
  chk.check(
    String(doc.policy_id ?? "").toLowerCase() === LEAN_POLICY_V2_ID,
    `campaign policy_id == LEAN_POLICY_V2_ID (got ${doc.policy_id})`
  );

  await withChallengeWindow(h, DEMO_WINDOW, async () => {
    await runLifecycle(
      h,
      ctx,
      chk,
      campaignId,
      PASS_PROOF_URL,
      MAX_SCORE,
      "PASS: import Mathlib + Nat.Prime 101 by norm_num → score MAX, agent paid"
    );
    await runLifecycle(
      h,
      ctx,
      chk,
      campaignId,
      FAIL_PROOF_URL,
      0,
      "FAIL: by sorry → sorryAx ∉ allow-list → score 0, client refunded"
    );
  });

  chk.finish();
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
