/**
 * Devnet e2e — C3 gasless sponsorship against the LIVE devnet shillbot program.
 * Proves a SPONSORED claim_task on real devnet: the sponsor is the fee-payer
 * (pays gas) and the agent co-signs the claim authority — exactly what the
 * orchestrator's build_sponsored_claim_task_tx produces. Asserts the sponsor
 * (not the agent) paid the transaction fee.
 *
 * Roles: id.json = client (creates + escrows the task) + funder; a fresh
 * `sponsor` keypair = the gas sponsor (stand-in for grok-agent-keypair[0]); a
 * fresh `agent` = the claimer. The agent pays only the one-time AgentState rent
 * (a C1 credit advance would front that); the sponsor pays the per-tx gas — the
 * complementary split by design.
 *
 * Money accounting: the run recovers everything it can in-run. emergency_return
 * (authority = id.json) closes the claimed task and returns its escrow + PDA rent
 * to the client; the fresh sponsor + agent leftovers are swept back to id.json.
 * The ONLY un-recovered lamports are the throwaway agent's AgentState rent — a
 * Claimed task can't reset claimed_count, so close_agent_state (which needs
 * claimed_count == 0) can't run in the same pass. That residual keeps this a
 * DEVNET-ONLY test (free SOL); the mainnet-clean tests are the cycle-complete
 * ones (recoupment-loop-devnet.ts, credit-flow-devnet.ts). Run manually
 * (workflow_dispatch); not on every push.
 *
 * Run: npx tsx scripts/e2e/sponsored-claim-devnet.ts   (exit 0 = pass)
 */
import * as anchor from "@coral-xyz/anchor";
import { BN } from "@coral-xyz/anchor";
import {
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
  SYSVAR_SLOT_HASHES_PUBKEY,
  Transaction,
} from "@solana/web3.js";
import { createHash, randomBytes } from "crypto";
import { readFileSync } from "fs";
import { homedir } from "os";
import { join } from "path";
import type { Shillbot } from "../../target/types/shillbot";

const DEVNET =
  process.env.DEVNET_RPC ??
  process.env.RPC_URL ??
  "https://api.devnet.solana.com";
const ESCROW = new BN(500_000); // 0.0005 SOL — minimal; stays locked (see header)
const AGENT_FUNDING = 4_000_000; // ~enough for AgentState rent (no fee buffer needed — sponsored)

let failures = 0;
function check(cond: boolean, msg: string): void {
  console.log(`  ${cond ? "✓" : "✗"} ${msg}`);
  if (!cond) failures++;
}
function loadKeypair(path: string): Keypair {
  return Keypair.fromSecretKey(
    Uint8Array.from(JSON.parse(readFileSync(path, "utf8")) as number[])
  );
}

async function main(): Promise<void> {
  const client = loadKeypair(join(homedir(), ".config/solana/id.json"));
  const sponsor = Keypair.generate();
  const agent = Keypair.generate();
  const connection = new Connection(DEVNET, "confirmed");
  const provider = new anchor.AnchorProvider(
    connection,
    new anchor.Wallet(client),
    {
      commitment: "confirmed",
    }
  );
  anchor.setProvider(provider);
  const program = anchor.workspace.Shillbot
    ? (anchor.workspace.Shillbot as anchor.Program<Shillbot>)
    : (new anchor.Program(
        JSON.parse(
          readFileSync(
            join(__dirname, "..", "..", "target", "idl", "shillbot.json"),
            "utf8"
          )
        ) as anchor.Idl,
        provider
      ) as unknown as anchor.Program<Shillbot>);

  const bal = (pk: PublicKey): Promise<number> =>
    connection.getBalance(pk, "confirmed");
  console.log(`client (escrow):  ${client.publicKey.toBase58()}`);
  console.log(`sponsor (gas):    ${sponsor.publicKey.toBase58()}`);
  console.log(`agent (claimer):  ${agent.publicKey.toBase58()}`);

  // --- fund the fresh sponsor + agent from id.json ---
  const clientStart = await bal(client.publicKey);
  await provider.sendAndConfirm(
    new Transaction().add(
      SystemProgram.transfer({
        fromPubkey: client.publicKey,
        toPubkey: sponsor.publicKey,
        lamports: 8_000_000,
      }),
      SystemProgram.transfer({
        fromPubkey: client.publicKey,
        toPubkey: agent.publicKey,
        lamports: AGENT_FUNDING,
      })
    )
  );

  // --- client creates an Open task on devnet ---
  console.log("\n[setup] create_task (client escrows a task)");
  const [globalPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("shillbot_global")],
    program.programId
  );
  const globalBefore = await program.account.globalState.fetch(globalPda);
  const counter = globalBefore.taskCounter as BN;
  const [taskPda] = PublicKey.findProgramAddressSync(
    [
      Buffer.from("task"),
      counter.toArrayLike(Buffer, "le", 8),
      client.publicKey.toBuffer(),
    ],
    program.programId
  );
  const content = Array.from(
    createHash("sha256").update(randomBytes(16)).digest()
  );
  const deadline = new BN(Math.floor(Date.now() / 1000) + 86_400 * 30);
  await program.methods
    .createTask(
      ESCROW,
      content as never,
      deadline,
      new BN(3600),
      new BN(14_400),
      0,
      0,
      0,
      0,
      false,
      0 // verification_kind: OracleMetrics (kind 0)
    )
    .accountsPartial({
      globalState: globalPda,
      task: taskPda,
      client: client.publicKey,
      slotHashes: SYSVAR_SLOT_HASHES_PUBKEY,
      systemProgram: SystemProgram.programId,
    })
    .signers([client])
    .rpc({ commitment: "confirmed" });
  check(
    JSON.stringify((await program.account.task.fetch(taskPda)).state) ===
      JSON.stringify({ open: {} }),
    "task created in Open state"
  );

  // --- the SPONSORED claim: sponsor = fee-payer, agent co-signs the authority ---
  console.log(
    "\n[claim] sponsored claim_task — sponsor pays gas, agent co-signs"
  );
  const [agentState] = PublicKey.findProgramAddressSync(
    [Buffer.from("agent_state"), agent.publicKey.toBuffer()],
    program.programId
  );
  const tx: Transaction = await program.methods
    .claimTask()
    .accountsPartial({
      task: taskPda,
      globalState: globalPda,
      agentState,
      agent: agent.publicKey,
      systemProgram: SystemProgram.programId,
    })
    .transaction();
  tx.feePayer = sponsor.publicKey; // sponsor pays the fee
  tx.recentBlockhash = (
    await connection.getLatestBlockhash("confirmed")
  ).blockhash;
  tx.partialSign(sponsor); // fee-payer signs
  tx.partialSign(agent); // agent co-signs the claim authority

  const sponsorBefore = await bal(sponsor.publicKey);
  const agentBefore = await bal(agent.publicKey);
  const sig = await connection.sendRawTransaction(tx.serialize());
  await connection.confirmTransaction(sig, "confirmed");
  const txInfo = await connection.getTransaction(sig, {
    commitment: "confirmed",
    maxSupportedTransactionVersion: 0,
  });
  const fee = txInfo?.meta?.fee ?? 0;

  // --- assertions ---
  const task = await program.account.task.fetch(taskPda);
  check(
    JSON.stringify(task.state) === JSON.stringify({ claimed: {} }),
    "task is now Claimed"
  );
  check(
    task.agent.toString() === agent.publicKey.toBase58(),
    "task.agent == the agent"
  );
  check(
    (
      txInfo?.transaction.message.staticAccountKeys?.[0] ??
      // Legacy (non-v0) messages expose `accountKeys` instead; the union type
      // only declares `staticAccountKeys`, so reach the legacy field via `any`.
      (txInfo?.transaction.message as unknown as { accountKeys?: PublicKey[] })
        .accountKeys?.[0]
    )?.toString() === sponsor.publicKey.toBase58(),
    "fee-payer (account[0]) is the sponsor"
  );
  const sponsorDelta = sponsorBefore - (await bal(sponsor.publicKey));
  const agentDelta = agentBefore - (await bal(agent.publicKey));
  check(
    sponsorDelta === fee,
    `sponsor paid exactly the gas fee (${fee} lamports)`
  );
  const agentStateRent = await connection.getMinimumBalanceForRentExemption(
    (await connection.getAccountInfo(agentState))?.data.length ?? 0
  );
  check(
    agentDelta === agentStateRent,
    `agent paid only the AgentState rent (${agentDelta}), NOT the gas fee`
  );
  check(
    agentDelta !== 0 && sponsorDelta < agentDelta,
    "agent never paid gas — the sponsor did"
  );

  // --- recover the escrow: emergency_return closes the Claimed task and returns
  //     its escrow + PDA rent to the client (authority = id.json). The only
  //     residual is the throwaway agent's AgentState rent — close_agent_state
  //     requires claimed_count == 0, which emergency_return cannot reset on a
  //     Claimed task, so this test stays devnet-only (the residual is free SOL). ---
  const taskRent = await connection.getMinimumBalanceForRentExemption(
    (await connection.getAccountInfo(taskPda))?.data.length ?? 0
  );
  console.log(
    "\n[recover] emergency_return — escrow + Task rent back to the client"
  );
  await program.methods
    .emergencyReturn()
    .accountsPartial({ globalState: globalPda, authority: client.publicKey })
    .remainingAccounts([
      { pubkey: taskPda, isWritable: true, isSigner: false },
      { pubkey: client.publicKey, isWritable: true, isSigner: false },
    ])
    .signers([client])
    .rpc({ commitment: "confirmed" });
  check(
    (await connection.getAccountInfo(taskPda)) === null,
    "task closed + escrow returned via emergency_return"
  );

  // --- cleanup: drain the fresh sponsor + agent leftovers back to id.json ---
  console.log("\n[cleanup] drain sponsor + agent leftovers back to id.json");
  for (const kp of [sponsor, agent]) {
    const remaining = await bal(kp.publicKey);
    const sweep = remaining - 5_000; // leave a hair for the sweep fee
    if (sweep > 0) {
      const sweepTx = new Transaction().add(
        SystemProgram.transfer({
          fromPubkey: kp.publicKey,
          toPubkey: client.publicKey,
          lamports: sweep,
        })
      );
      sweepTx.feePayer = kp.publicKey;
      sweepTx.recentBlockhash = (
        await connection.getLatestBlockhash("confirmed")
      ).blockhash;
      sweepTx.sign(kp);
      const sweepSig = await connection.sendRawTransaction(sweepTx.serialize());
      await connection.confirmTransaction(sweepSig, "confirmed");
    }
  }

  // --- accounting ledger ---
  const clientEnd = await bal(client.publicKey);
  const netClientLoss = clientStart - clientEnd;
  // After emergency_return, the escrow + Task PDA rent are back with the client.
  // The only un-recovered lamports are the throwaway agent's AgentState rent
  // (irreducible: close_agent_state needs claimed_count == 0). Rest is gas.
  console.log(
    `\n[ledger] client net loss ${netClientLoss} lamports; escrow (${ESCROW.toNumber()}) + Task rent (${taskRent}) ` +
      `recovered via emergency_return. Irreducible residual: AgentState rent ${agentStateRent} on the throwaway ` +
      `agent PDA (devnet-only). Remainder is gas across the run's txs.`
  );
  check(
    netClientLoss <= agentStateRent + 200_000,
    "no SOL lost beyond the AgentState residual + gas"
  );

  console.log(
    `\n${failures === 0 ? "PASS" : "FAIL"} — ${failures} failed check(s)`
  );
  process.exit(failures === 0 ? 0 : 1);
}

main().catch((e: unknown) => {
  console.error(e);
  process.exit(1);
});
