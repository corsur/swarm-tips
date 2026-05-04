/**
 * One-shot crank: finalize the 5 mainnet Verified tasks whose challenge
 * windows have been closed for ~24 days.
 *
 * Discovered by E2E test session 2026-05-04. Indicates the orchestrator's
 * finalize-cranking logic is not running in production. This script fixes
 * the immediate state; the durable fix is the missing finalize crank
 * (separate Builder task).
 *
 * Usage: ANCHOR_WALLET=~/.config/solana/id.json \
 *        ANCHOR_PROVIDER_URL=https://api.mainnet-beta.solana.com \
 *        npx ts-node scripts/finalize-stuck-verified.ts
 */

import * as anchor from "@coral-xyz/anchor";
import { PublicKey } from "@solana/web3.js";
import * as fs from "fs";
import * as path from "path";

const idl = JSON.parse(
  fs.readFileSync(path.join(__dirname, "../target/idl/shillbot.json"), "utf8"),
);

const STUCK_TASKS = [
  { task_id: 152, pda: "2K6jHZ1ZLhA1ZtKUGEzkxMa7TC7Nm1sMPVgKwFE6voci" },
  { task_id: 144, pda: "GvzL45BMnbXp3BL4qBUXLf41HxgQihQc5KeDdczTGSgP" },
  { task_id: 139, pda: "GzkxgT26B8ptkhsX6MJcZeeVfbp3yMa4AkmgdGjSAcqq" },
  { task_id: 146, pda: "DBcbYKdGNadd5Qd1hMTMKSuvm92bHyruUfUVViQYczNY" },
  { task_id: 118, pda: "4unXdmmcxE68iuAVXoLiPcpEQgHBBTNVHXNje4dcusR9" },
];

async function main() {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = new anchor.Program(idl as anchor.Idl, provider);
  const wallet = provider.wallet.publicKey;

  console.log(`Wallet: ${wallet.toBase58()}`);
  console.log(`RPC:    ${provider.connection.rpcEndpoint}`);
  console.log(`Program: ${program.programId.toBase58()}`);
  console.log();

  // Derive global PDA + read treasury.
  const [globalPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("shillbot_global")],
    program.programId,
  );
  const global = await (program.account as any).globalState.fetch(globalPda);
  const treasury = global.treasury as PublicKey;
  console.log(`global_state.treasury = ${treasury.toBase58()}`);
  console.log();

  for (const { task_id, pda } of STUCK_TASKS) {
    const taskPda = new PublicKey(pda);
    let task: any;
    try {
      task = await (program.account as any).task.fetch(taskPda);
    } catch (e) {
      console.log(`task ${task_id}: SKIP (account not found — already finalized?)`);
      continue;
    }
    const stateName = Object.keys(task.state)[0];
    if (stateName !== "verified") {
      console.log(
        `task ${task_id}: SKIP (state=${stateName}, not Verified — already cranked?)`,
      );
      continue;
    }

    console.log(`task ${task_id}: finalizing…`);
    try {
      // Derive the agent's AgentState PDA so reputation counters update.
      const [agentStatePda] = PublicKey.findProgramAddressSync(
        [Buffer.from("agent_state"), task.agent.toBuffer()],
        program.programId,
      );
      const agentStateAccount = await provider.connection.getAccountInfo(
        agentStatePda,
      );
      const remainingAccounts = agentStateAccount
        ? [{ pubkey: agentStatePda, isSigner: false, isWritable: true }]
        : [];

      const sig = await (program.methods as any)
        .finalizeTask()
        .accountsPartial({
          task: taskPda,
          globalState: globalPda,
          agent: task.agent,
          client: task.client,
          treasury,
        })
        .remainingAccounts(remainingAccounts)
        .rpc();

      console.log(`  OK: signature=${sig}`);
    } catch (e: any) {
      console.log(`  FAIL: ${e.message ?? e}`);
    }
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
