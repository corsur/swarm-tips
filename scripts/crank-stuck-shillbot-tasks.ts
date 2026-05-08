/**
 * Crank stuck shillbot tasks across mainnet:
 *  - On-chain Submitted/Approved past T+14d → call expire_task (escrow back to client)
 *  - On-chain Verified past challenge_window → call finalize_task (payout to agent)
 *  - On-chain CLOSED but Firestore stale → reconcile Firestore state to expired/finalized
 *
 * Permissionless on-chain crank — anyone with gas can drive these. Updates
 * Firestore directly because the orchestrator's /tasks/:id/confirm endpoint
 * doesn't expose an "expire" action (only Create/Claim/Submit/Approve/Verify/
 * Finalize). Direct write is safe — the orchestrator doesn't run a continuous
 * on-chain mirror, it only updates Firestore in response to specific API calls.
 *
 * Usage: ANCHOR_WALLET=~/.config/solana/id.json \
 *        ANCHOR_PROVIDER_URL=https://api.mainnet-beta.solana.com \
 *        npx ts-node scripts/crank-stuck-shillbot-tasks.ts
 */

import * as anchor from "@coral-xyz/anchor";
import { PublicKey } from "@solana/web3.js";
import * as fs from "fs";
import * as path from "path";
import { execSync } from "child_process";

const idl = JSON.parse(
  fs.readFileSync(path.join(__dirname, "../target/idl/shillbot.json"), "utf8")
);

const FIRESTORE_PROJECT = "coordination-game-prod";
const SHILLBOT_PROGRAM_ID = "2tR37nqMpwdV4DVUHjzUmL1rH2DtkA8zrRA4EAhT7KMi";
const EXPIRE_TIMEOUT_SECS = 14 * 24 * 3600;
const CHALLENGE_WINDOW_SECS = 24 * 3600;

interface StuckTask {
  task_id: string;
  on_chain_address: string;
  fs_state: string;
  agent: string | null;
  client: string | null;
  submitted_at_ts: number | null;
  on_chain_state: number; // 0xff if closed
  on_chain_escrow: number;
}

interface AuditOutput {
  needs_expire: StuckTask[];
  needs_finalize: StuckTask[];
  fs_stale: StuckTask[];
  recent: StuckTask[];
}

function loadAudit(): AuditOutput {
  const result = execSync(
    `/tmp/fs-query-venv/bin/python3 ${path.join(
      __dirname,
      "audit-stuck-shillbot.py"
    )}`,
    { encoding: "utf8" }
  );
  return JSON.parse(result);
}

function updateFirestoreState(taskId: string, newState: string): void {
  execSync(
    `/tmp/fs-query-venv/bin/python3 ${path.join(
      __dirname,
      "reconcile-firestore-state.py"
    )} ${JSON.stringify(taskId)} ${newState}`,
    { encoding: "utf8" }
  );
}

async function main() {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const program = new anchor.Program(idl as anchor.Idl, provider);
  const wallet = provider.wallet.publicKey;

  console.log(`Wallet:  ${wallet.toBase58()}`);
  console.log(`RPC:     ${provider.connection.rpcEndpoint}`);
  console.log();

  const audit = loadAudit();
  const needsExpire = audit.needs_expire;
  const needsFinalize = audit.needs_finalize;
  const firestoreStale = audit.fs_stale;

  console.log(`  expire candidates:        ${needsExpire.length}`);
  console.log(`  finalize candidates:      ${needsFinalize.length}`);
  console.log(`  firestore-stale (closed): ${firestoreStale.length}`);
  console.log(`  recent (skip):            ${audit.recent.length}`);
  console.log();

  const [globalPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("shillbot_global")],
    new PublicKey(SHILLBOT_PROGRAM_ID)
  );

  // ---- Expire ----
  for (const t of needsExpire) {
    const taskPda = new PublicKey(t.on_chain_address);
    let task: any;
    try {
      task = await (program.account as any).task.fetch(taskPda);
    } catch (e) {
      console.log(
        `  ${t.on_chain_address}: account not found (already expired) — reconcile fs only`
      );
      try {
        updateFirestoreState(t.task_id, "expired");
      } catch (e: any) {
        console.log(`    fs update failed: ${e.message}`);
      }
      continue;
    }

    process.stdout.write(
      `  expire ${t.on_chain_address} (state=${
        Object.keys(task.state)[0]
      }, escrow=${task.escrowLamports.toString()})… `
    );
    try {
      const sig = await (program.methods as any)
        .expireTask()
        .accountsPartial({
          task: taskPda,
          globalState: globalPda,
          client: task.client,
        })
        .rpc();
      console.log(`OK ${sig.slice(0, 16)}…`);
      try {
        updateFirestoreState(t.task_id, "expired");
      } catch (e: any) {
        console.log(`    fs update failed: ${e.message}`);
      }
    } catch (e: any) {
      console.log(`FAIL: ${e.message ?? e}`);
    }
  }

  // ---- Finalize ----
  for (const t of needsFinalize) {
    const taskPda = new PublicKey(t.on_chain_address);
    let task: any;
    try {
      task = await (program.account as any).task.fetch(taskPda);
    } catch (e) {
      console.log(
        `  ${t.on_chain_address}: account not found — reconcile fs only`
      );
      updateFirestoreState(t.task_id, "finalized");
      continue;
    }
    const stateName = Object.keys(task.state)[0];
    if (stateName !== "verified") {
      console.log(
        `  ${t.on_chain_address}: state=${stateName} not Verified — skip`
      );
      continue;
    }

    const [agentStatePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("agent_state"), task.agent.toBuffer()],
      new PublicKey(SHILLBOT_PROGRAM_ID)
    );
    const agentStateAccount = await provider.connection.getAccountInfo(
      agentStatePda
    );
    const remainingAccounts = agentStateAccount
      ? [{ pubkey: agentStatePda, isSigner: false, isWritable: true }]
      : [];

    const global = await (program.account as any).globalState.fetch(globalPda);

    process.stdout.write(`  finalize ${t.on_chain_address}… `);
    try {
      const sig = await (program.methods as any)
        .finalizeTask()
        .accountsPartial({
          task: taskPda,
          globalState: globalPda,
          agent: task.agent,
          client: task.client,
          treasury: global.treasury,
        })
        .remainingAccounts(remainingAccounts)
        .rpc();
      console.log(`OK ${sig.slice(0, 16)}…`);
      updateFirestoreState(t.task_id, "finalized");
    } catch (e: any) {
      console.log(`FAIL: ${e.message ?? e}`);
    }
  }

  // ---- Reconcile already-closed ----
  for (const t of firestoreStale) {
    process.stdout.write(
      `  reconcile fs ${t.task_id.slice(0, 36)}… (was ${t.fs_state}) `
    );
    try {
      // Already closed — pick "expired" as the canonical reconciled state
      // since we can't tell from the closed account whether it was expire or finalize.
      // If the prior state was 'verified', it almost certainly went to finalized.
      const newState = t.fs_state === "verified" ? "finalized" : "expired";
      updateFirestoreState(t.task_id, newState);
      console.log(`→ ${newState}`);
    } catch (e: any) {
      console.log(`FAIL: ${e.message ?? e}`);
    }
  }

  console.log("\nDone.");
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
