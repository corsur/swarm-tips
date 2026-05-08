/**
 * One-shot: expire the 3 mainnet Submitted Website tasks from 2026-04-23
 * whose verification timeout (T+14d) has passed. Permissionless instruction;
 * fee paid by id.json (CKsZ...). Escrow returns to id.json since we are
 * also the client on each task.
 *
 * Why these are stuck: the agents are 3 different ephemeral burner wallets
 * (~0.0085 SOL each, classic single-shot pattern) — Claude generated them
 * during a Website-platform test on 2026-04-23 instead of using a persisted
 * keypair (test.json or grok-pool). Verification didn't fire (Workflow
 * trigger broken at the time), and the burner keys are now lost. Paying
 * verify→finalize would lock 0.06 SOL in unrecoverable wallets forever, so
 * expire_task is the correct path.
 *
 * Usage: ANCHOR_WALLET=~/.config/solana/id.json \
 *        ANCHOR_PROVIDER_URL=https://api.mainnet-beta.solana.com \
 *        npx ts-node scripts/expire-stuck-website.ts
 */

import * as anchor from "@coral-xyz/anchor";
import { PublicKey } from "@solana/web3.js";
import * as fs from "fs";
import * as path from "path";

const idl = JSON.parse(
  fs.readFileSync(path.join(__dirname, "../target/idl/shillbot.json"), "utf8")
);

const STUCK_TASKS = [
  { task_id: 174, pda: "E73mBSegVN65AnJ638q46M3KGuFXuAKwMHPUVahqtpWB" },
  { task_id: 180, pda: "44Lvnk9mdbCL9BzUBaa8niM54LNFHcPFYopiXDWS8Y5k" },
  { task_id: 181, pda: "HY23JP3PSYPJG1YtZoF795GomcHgwd6KZwQNRr916azy" },
];

async function main() {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = new anchor.Program(idl as anchor.Idl, provider);
  const wallet = provider.wallet.publicKey;

  console.log(`Wallet:  ${wallet.toBase58()}`);
  console.log(`RPC:     ${provider.connection.rpcEndpoint}`);
  console.log(`Program: ${program.programId.toBase58()}`);
  console.log();

  const [globalPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("shillbot_global")],
    program.programId
  );

  for (const { task_id, pda } of STUCK_TASKS) {
    const taskPda = new PublicKey(pda);
    let task: any;
    try {
      task = await (program.account as any).task.fetch(taskPda);
    } catch (e) {
      console.log(`task ${task_id}: SKIP (account not found — already expired?)`);
      continue;
    }
    const stateName = Object.keys(task.state)[0];
    if (stateName !== "submitted" && stateName !== "approved") {
      console.log(
        `task ${task_id}: SKIP (state=${stateName} — expire_task only valid from submitted/approved/open/claimed)`
      );
      continue;
    }

    const balanceBefore = await provider.connection.getBalance(wallet);
    console.log(`task ${task_id}: expiring (state=${stateName}, escrow=${task.escrowLamports.toString()} lamports)…`);
    try {
      const sig = await (program.methods as any)
        .expireTask()
        .accountsPartial({
          task: taskPda,
          globalState: globalPda,
          client: task.client,
        })
        .rpc();
      console.log(`  OK: signature=${sig}`);
      const balanceAfter = await provider.connection.getBalance(wallet);
      console.log(`  client balance change: ${(balanceAfter - balanceBefore) / 1e9} SOL`);
    } catch (e: any) {
      console.log(`  FAIL: ${e.message ?? e}`);
    }
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
