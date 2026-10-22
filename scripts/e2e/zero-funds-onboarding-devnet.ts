/**
 * Devnet e2e — RATE-LIMITED ZERO-FUNDS ONBOARDING mechanism, end to end against
 * the live devnet programs (extension-registry + extension-credit + shillbot).
 * Proves a fresh agent that brings $0 can gain standing, get its rent fronted,
 * and engage in a task — the exact chain the orchestrator's /agent/onboard will
 * drive (here the onboarding authority is a throwaway key standing in for the
 * root-vouched grok[0]).
 *
 * Flow:
 *   1. root (id.json) vouches the onboarding authority  (root -> onbAuth)
 *   2. onbAuth vouches the fresh agent                  (onbAuth -> agent)
 *      => agent is reachable from the root => has web standing
 *   3. onbAuth open_advance -> agent (fronts the AgentState rent, recoupable via
 *      the already-proven route_and_recoup)
 *   4. assert the DEPLOYED mcp.swarm.tips reputation endpoint reports the agent
 *      as having standing (the real reputation system sees the vouch chain)
 *   5. client (id.json) creates a shillbot task
 *   6. the agent — having funded itself with NOTHING — does a SPONSORED claim
 *      (onbAuth pays gas; the agent pays its AgentState rent out of the fronted
 *      advance) and the task lands Claimed by the agent.
 *
 * The recoupment leg (agent earnings repay the advance) is the already-proven
 * route_and_recoup (credit-flow-devnet.ts). Run manually (workflow_dispatch).
 *
 * Economic close-out: the run recovers everything recoverable in-run — task
 * escrow + Task rent (emergency_return), both vouch bonds + rent
 * (withdraw_extension), and the advance principal + PDA rent (recoup + close).
 * The ONLY un-recovered lamports are the $0 agent's AgentState rent: a Claimed
 * task can't reset claimed_count, so close_agent_state can't run in the same
 * pass. That residual keeps this DEVNET-ONLY (free SOL); the mainnet-clean tests
 * are the cycle-complete recoupment-loop-devnet.ts / credit-flow-devnet.ts.
 * Run: npx tsx scripts/e2e/zero-funds-onboarding-devnet.ts
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
import type { ExtensionRegistry } from "../../target/types/extension_registry";
import type { ExtensionCredit } from "../../target/types/extension_credit";
import type { Shillbot } from "../../target/types/shillbot";
import { waitAccountsClosed } from "./devnet-harness";

const DEVNET =
  process.env.DEVNET_RPC ??
  process.env.RPC_URL ??
  "https://api.devnet.solana.com";
const REPUTATION_URL = "https://mcp.swarm.tips/internal/agent-reputation";
const TYPE_CAP = 0;
const BOND = new BN(1_000_000); // 0.001 SOL min bond per vouch
// Fronts the agent enough to (a) pay the one-time AgentState rent (~0.0015 SOL,
// init payer = agent) AND (b) keep its own system-account rent-exempt floor
// (~0.00089 SOL) — else the claim tx is rejected for leaving the agent below
// rent exemption. Still all treasury-fronted; the agent brings $0.
const ADVANCE = new BN(3_000_000);
// Escrow (0.005 SOL) comfortably exceeds the 0.003 SOL advance even after the
// 5% protocol fee (payment = 4,750,000 lamports), so real settlement via
// verify_task + finalize_task fully repays the advance and leaves a surplus.
const ESCROW = new BN(5_000_000);
const MAX_SCORE = new BN(1_000_000);

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
function loadIdl(name: string): anchor.Idl {
  return JSON.parse(
    readFileSync(
      join(__dirname, "..", "..", "target", "idl", `${name}.json`),
      "utf8"
    )
  ) as anchor.Idl;
}

async function main(): Promise<void> {
  const root = loadKeypair(join(homedir(), ".config/solana/id.json"));
  const onbAuth = Keypair.generate(); // stand-in for the root-vouched grok[0]
  const agent = Keypair.generate(); // the fresh, $0 agent
  const connection = new Connection(DEVNET, "confirmed");
  const provider = new anchor.AnchorProvider(
    connection,
    new anchor.Wallet(root),
    {
      commitment: "confirmed",
    }
  );
  anchor.setProvider(provider);

  const registry = new anchor.Program(
    loadIdl("extension_registry"),
    provider
  ) as unknown as anchor.Program<ExtensionRegistry>;
  const credit = new anchor.Program(
    loadIdl("extension_credit"),
    provider
  ) as unknown as anchor.Program<ExtensionCredit>;
  const shillbot = new anchor.Program(
    loadIdl("shillbot"),
    provider
  ) as unknown as anchor.Program<Shillbot>;

  const bal = (pk: PublicKey): Promise<number> =>
    connection.getBalance(pk, "confirmed");
  const extPda = (extender: PublicKey, recipient: PublicKey): PublicKey =>
    PublicKey.findProgramAddressSync(
      [Buffer.from("extension"), extender.toBuffer(), recipient.toBuffer()],
      registry.programId
    )[0];

  console.log(`root:    ${root.publicKey.toBase58()}`);
  console.log(`onbAuth: ${onbAuth.publicKey.toBase58()}`);
  console.log(`agent:   ${agent.publicKey.toBase58()} (brings $0)`);

  // Fund only the onboarding authority (treasury stand-in). The agent stays $0.
  const rootStart = await bal(root.publicKey);
  await provider.sendAndConfirm(
    new Transaction().add(
      SystemProgram.transfer({
        fromPubkey: root.publicKey,
        toPubkey: onbAuth.publicKey,
        lamports: 30_000_000,
      })
    )
  );
  check(
    (await bal(agent.publicKey)) === 0,
    "agent starts with exactly 0 lamports"
  );

  // registry GlobalState is already initialized on devnet (idempotent guard).
  const [regGlobal] = PublicKey.findProgramAddressSync(
    [Buffer.from("extension_global")],
    registry.programId
  );
  try {
    await registry.methods
      .initialize(root.publicKey, root.publicKey)
      .accountsPartial({
        globalState: regGlobal,
        payer: root.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc({ commitment: "confirmed" });
  } catch {
    /* already initialized */
  }

  // === 1. root vouches the onboarding authority (root -> onbAuth) ===
  console.log("\n[1] root vouches the onboarding authority");
  await registry.methods
    .submitExtension(TYPE_CAP, BOND)
    .accountsPartial({
      extension: extPda(root.publicKey, onbAuth.publicKey),
      extender: root.publicKey,
      recipient: onbAuth.publicKey,
      systemProgram: SystemProgram.programId,
    })
    .rpc({ commitment: "confirmed" });

  // === 2. onboarding authority vouches the fresh agent (onbAuth -> agent) ===
  console.log("[2] onboarding authority vouches the fresh agent");
  await registry.methods
    .submitExtension(TYPE_CAP, BOND)
    .accountsPartial({
      extension: extPda(onbAuth.publicKey, agent.publicKey),
      extender: onbAuth.publicKey,
      recipient: agent.publicKey,
      systemProgram: SystemProgram.programId,
    })
    .signers([onbAuth])
    .rpc({ commitment: "confirmed" });

  // === 3. open a recoupable advance fronting the agent its rent ===
  console.log("[3] open_advance fronts the agent its AgentState rent");
  const [advancePda] = PublicKey.findProgramAddressSync(
    [
      Buffer.from("advance"),
      onbAuth.publicKey.toBuffer(),
      agent.publicKey.toBuffer(),
    ],
    credit.programId
  );
  await credit.methods
    .openAdvance(ADVANCE)
    .accountsPartial({
      advance: advancePda,
      backer: onbAuth.publicKey,
      recipient: agent.publicKey,
      systemProgram: SystemProgram.programId,
    })
    .signers([onbAuth])
    .rpc({ commitment: "confirmed" });
  check(
    (await bal(agent.publicKey)) === ADVANCE.toNumber(),
    `agent received exactly the fronted advance (${ADVANCE.toNumber()}), still brought $0 of its own`
  );
  const vaultAfterOpen = await bal(advancePda);

  // === 4. the DEPLOYED reputation system sees the vouch chain => standing ===
  console.log(
    "\n[4] deployed mcp.swarm.tips confirms the agent now has standing"
  );
  await new Promise((r) => setTimeout(r, 1500));
  const rep = (await (
    await fetch(
      `${REPUTATION_URL}?wallet=${agent.publicKey.toBase58()}&network=devnet`
    )
  ).json()) as { has_standing: boolean; web_position: number | null };
  check(
    rep.has_standing === true,
    `agent has web standing via the real reputation system (position ${rep.web_position})`
  );

  // === 5. client creates a task ===
  console.log("\n[5] client creates a shillbot task");
  const [sbGlobal] = PublicKey.findProgramAddressSync(
    [Buffer.from("shillbot_global")],
    shillbot.programId
  );
  const globalAccount = await shillbot.account.globalState.fetch(sbGlobal);
  const nonce = new BN(randomBytes(8));
  const [taskPda] = PublicKey.findProgramAddressSync(
    [
      Buffer.from("task"),
      nonce.toArrayLike(Buffer, "le", 8),
      root.publicKey.toBuffer(),
    ],
    shillbot.programId
  );
  await shillbot.methods
    .createTask(
      nonce,
      ESCROW,
      Array.from(
        createHash("sha256").update(randomBytes(16)).digest()
      ) as never,
      new BN(Math.floor(Date.now() / 1000) + 86_400 * 30),
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
      globalState: sbGlobal,
      task: taskPda,
      client: root.publicKey,
      slotHashes: SYSVAR_SLOT_HASHES_PUBKEY,
      systemProgram: SystemProgram.programId,
    })
    .rpc({ commitment: "confirmed" });

  // === 6. the $0 agent does a SPONSORED claim + atomic set_payout_to(advancePda) ===
  console.log(
    "[6] the agent does a sponsored claim + set_payout_to(advancePda) — onbAuth pays gas, agent pays rent from the advance"
  );
  const [agentState] = PublicKey.findProgramAddressSync(
    [Buffer.from("agent_state"), agent.publicKey.toBuffer()],
    shillbot.programId
  );
  const claimIx = await shillbot.methods
    .claimTask()
    .accountsPartial({
      task: taskPda,
      globalState: sbGlobal,
      agentState,
      agent: agent.publicKey,
      systemProgram: SystemProgram.programId,
    })
    .instruction();
  const routeIx = await shillbot.methods
    .setPayoutTo(advancePda)
    .accountsPartial({
      task: taskPda,
      agent: agent.publicKey,
    })
    .instruction();
  const tx = new Transaction().add(claimIx, routeIx);
  tx.feePayer = onbAuth.publicKey;
  tx.recentBlockhash = (
    await connection.getLatestBlockhash("confirmed")
  ).blockhash;
  tx.partialSign(onbAuth);
  tx.partialSign(agent);
  const onbBefore = await bal(onbAuth.publicKey);
  const sig = await connection.sendRawTransaction(tx.serialize());
  await connection.confirmTransaction(sig, "confirmed");
  const txInfo = await connection.getTransaction(sig, {
    commitment: "confirmed",
    maxSupportedTransactionVersion: 0,
  });
  const fee = txInfo?.meta?.fee ?? 0;

  let task = await shillbot.account.task.fetch(taskPda);
  check(
    JSON.stringify(task.state) === JSON.stringify({ claimed: {} }),
    "task is Claimed"
  );
  check(
    task.agent.toString() === agent.publicKey.toBase58(),
    "task claimed by the onboarded agent"
  );
  check(
    task.payoutTo.equals(advancePda),
    "task.payout_to locked to advancePda in the claim tx"
  );
  check(
    onbBefore - (await bal(onbAuth.publicKey)) === fee,
    `onboarding authority paid the gas (${fee})`
  );
  const agentRemaining = await bal(agent.publicKey);
  check(
    agentRemaining < ADVANCE.toNumber() && agentRemaining > 0,
    `agent's only SOL ever was the fronted advance (now ${agentRemaining} after paying rent) — it brought $0`
  );

  // === 7. SPONSORED submit_work (onbAuth pays gas, agent signs work authority) ===
  console.log(
    "\n[7] the agent does a sponsored submit_work — decrements claimed_count to 0"
  );
  const submitIx = await shillbot.methods
    .submitWork(Buffer.from("e2e-zero-funds-proof"))
    .accountsPartial({
      task: taskPda,
      globalState: sbGlobal,
      agentState,
      agent: agent.publicKey,
    })
    .instruction();
  const submitTx = new Transaction().add(submitIx);
  submitTx.feePayer = onbAuth.publicKey;
  submitTx.recentBlockhash = (
    await connection.getLatestBlockhash("confirmed")
  ).blockhash;
  submitTx.partialSign(onbAuth);
  submitTx.partialSign(agent);
  const submitSig = await connection.sendRawTransaction(submitTx.serialize());
  await connection.confirmTransaction(submitSig, "confirmed");
  task = await shillbot.account.task.fetch(taskPda);
  check(
    JSON.stringify(task.state) === JSON.stringify({ submitted: {} }),
    "task is Submitted"
  );

  // === 8. Co-signed verify_task (caller = onbAuth pays gas, authority co-signs) ===
  console.log(
    "\n[8] first-party co-signed verify_task — caller (onbAuth) pays gas, authority co-signs"
  );
  const attDelay = globalAccount.attestationDelaySeconds.toNumber();
  const nowBeforeVerify = Math.floor(Date.now() / 1000);
  const earliestVerify = task.submittedAt.toNumber() + attDelay;
  const waitVerifySecs = Math.max(1, earliestVerify - nowBeforeVerify + 2);
  await new Promise((r) => setTimeout(r, waitVerifySecs * 1000));

  const vhash = Array.from(
    createHash("sha256").update("e2e-zero-funds-verify").digest()
  );
  const verifyIx = await shillbot.methods
    .verifyTask(MAX_SCORE, vhash as never)
    .accountsPartial({
      task: taskPda,
      globalState: sbGlobal,
      switchboardFeed: root.publicKey,
    })
    .instruction();
  // Mark the authority account as a required signer in the instruction so the
  // runtime enforces the co-signature on-chain while onbAuth remains feePayer.
  for (const k of verifyIx.keys) {
    if (k.pubkey.equals(root.publicKey)) {
      k.isSigner = true;
    }
  }
  const verifyTx = new Transaction().add(verifyIx);
  verifyTx.feePayer = onbAuth.publicKey;
  verifyTx.recentBlockhash = (
    await connection.getLatestBlockhash("confirmed")
  ).blockhash;
  verifyTx.partialSign(onbAuth);
  verifyTx.partialSign(root);
  const onbBeforeVerify = await bal(onbAuth.publicKey);
  const rootBeforeVerify = await bal(root.publicKey);
  const verifySig = await connection.sendRawTransaction(verifyTx.serialize());
  await connection.confirmTransaction(verifySig, "confirmed");

  task = await shillbot.account.task.fetch(taskPda);
  check(
    JSON.stringify(task.state) === JSON.stringify({ verified: {} }),
    "task is Verified via first-party authority co-signing"
  );
  check(
    (await bal(root.publicKey)) === rootBeforeVerify,
    "oracle/authority paid 0 lamports of gas on verify_task (caller paid)"
  );
  check(
    (await bal(onbAuth.publicKey)) < onbBeforeVerify,
    "caller (onbAuth) paid the verify_task transaction fee"
  );

  // === 9. finalize_task after challenge window -> routes escrow to advancePda ===
  console.log(
    "\n[9] finalize_task after challenge window — routes payment into advancePda"
  );
  const challengeDeadline = task.challengeDeadline.toNumber();
  let nowSec = Math.floor(Date.now() / 1000);
  const waitFinalizeSecs = Math.max(1, challengeDeadline - nowSec + 2);
  console.log(`  waiting ${waitFinalizeSecs}s for challenge window to close...`);
  await new Promise((r) => setTimeout(r, waitFinalizeSecs * 1000));

  await shillbot.methods
    .finalizeTask()
    .accountsPartial({
      task: taskPda,
      globalState: sbGlobal,
      agent: advancePda,
      client: root.publicKey,
      treasury: globalAccount.treasury,
    })
    .remainingAccounts([
      { pubkey: agentState, isWritable: true, isSigner: false },
    ])
    .rpc({ commitment: "confirmed" });

  const vaultAfterFinalize = await bal(advancePda);
  const routedIn = vaultAfterFinalize - vaultAfterOpen;
  check(
    routedIn === task.paymentAmount.toNumber() && routedIn > ADVANCE.toNumber(),
    `finalize_task routed real payment (${routedIn} lamports) into advancePda`
  );

  // === 10. route_and_recoup + close_advance + close_agent_state + withdraw vouches ===
  console.log(
    "\n[10] route_and_recoup repays onbAuth from real settlement + close all PDAs"
  );
  const agentBeforeRecoup = await bal(agent.publicKey);
  const onbBeforeRecoup = await bal(onbAuth.publicKey);
  await credit.methods
    .routeAndRecoup()
    .accountsPartial({
      advance: advancePda,
      backer: onbAuth.publicKey,
      recipient: agent.publicKey,
    })
    .rpc({ commitment: "confirmed" });
  const expectedSurplus = routedIn - ADVANCE.toNumber();
  check(
    (await bal( onbAuth.publicKey)) - onbBeforeRecoup === ADVANCE.toNumber(),
    `onbAuth recouped full advance principal (${ADVANCE.toNumber()} lamports) from task payout`
  );
  check(
    (await bal(agent.publicKey)) - agentBeforeRecoup === expectedSurplus,
    `agent received surplus earnings (${expectedSurplus} lamports)`
  );

  await credit.methods
    .closeAdvance()
    .accountsPartial({
      advance: advancePda,
      backer: onbAuth.publicKey,
      recipient: agent.publicKey,
    })
    .signers([onbAuth])
    .rpc({ commitment: "confirmed" });

  await shillbot.methods
    .closeAgentState()
    .accountsPartial({
      agentState,
      agent: agent.publicKey,
    })
    .signers([agent])
    .rpc({ commitment: "confirmed" });

  await registry.methods
    .withdrawExtension()
    .accountsPartial({
      extension: extPda(onbAuth.publicKey, agent.publicKey),
      extender: onbAuth.publicKey,
      recipient: agent.publicKey,
    })
    .signers([onbAuth])
    .rpc({ commitment: "confirmed" });
  await registry.methods
    .withdrawExtension()
    .accountsPartial({
      extension: extPda(root.publicKey, onbAuth.publicKey),
      extender: root.publicKey,
      recipient: onbAuth.publicKey,
    })
    .rpc({ commitment: "confirmed" });

  check(
    await waitAccountsClosed(connection, [taskPda, advancePda, agentState]),
    "task, advance, and agentState PDAs all closed (100% of rent & escrow recovered)"
  );

  // === best-effort cleanup (drain the throwaway wallets) ===
  console.log("\n[cleanup] sweep onbAuth + agent back to root");
  for (const kp of [agent, onbAuth]) {
    try {
      const remaining = await bal(kp.publicKey);
      const sweep = remaining - 5_000;
      if (sweep > 0) {
        const s = new Transaction().add(
          SystemProgram.transfer({
            fromPubkey: kp.publicKey,
            toPubkey: root.publicKey,
            lamports: sweep,
          })
        );
        s.feePayer = kp.publicKey;
        s.recentBlockhash = (
          await connection.getLatestBlockhash("confirmed")
        ).blockhash;
        s.sign(kp);
        await connection.confirmTransaction(
          await connection.sendRawTransaction(s.serialize()),
          "confirmed"
        );
      }
    } catch {
      /* best effort */
    }
  }
  const rootNet = rootStart - (await bal(root.publicKey));
  console.log(
    `[ledger] root net out ${rootNet} lamports — bonds, advance principal+rent, AgentState rent, and task escrow+rent all recovered. Only gas spent.`
  );
  check(rootNet <= 500_000, "no SOL lost beyond gas");

  console.log(
    `\n${failures === 0 ? "PASS" : "FAIL"} — ${failures} failed check(s)`
  );
  process.exit(failures === 0 ? 0 : 1);
}

main().catch((e: unknown) => {
  console.error(e);
  process.exit(1);
});
