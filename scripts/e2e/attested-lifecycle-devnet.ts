/**
 * Devnet e2e — the deterministic ATTESTED verification lifecycle against the
 * LIVE devnet shillbot program. Proves the S2 verify_task_attested path
 * end-to-end on real devnet:
 *
 *   create_task(kind=1, LeanProof) → claim → submit_work → verify_task_attested
 *   (signed by the allow-listed attester) → finalize → payment asserted.
 *
 * Plus the failing-proof branch: a second task attested with score 0 finalizes
 * with zero payment (full escrow refunded to the client).
 *
 * Self-contained: id.json is the devnet GlobalState authority, so this script
 * temporarily (a) rotates oracle_authority to a throwaway demo attester it
 * controls and (b) shrinks the global challenge_window to a few seconds so
 * finalize can run in-demo — then RESTORES both from the pre-run snapshot in a
 * finally block. The attested-verify step is the novel path; the
 * challenge→defaultResolve dispute path is exhaustively covered by the bankrun
 * suite (tests/shillbot-attested.ts) with clock warping and needs a ≥1h live
 * wait, so it is not re-proven here.
 *
 * verification_hash is any non-zero 32 bytes here: the on-chain program only
 * requires it non-zero (the AttestationCert-digest binding is an off-chain
 * audit property the attester SERVICE enforces, not the program).
 *
 * NOT a CI test (real devnet, mutates global params). Run manually:
 *   npx tsx scripts/e2e/attested-lifecycle-devnet.ts   (exit 0 = pass)
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

const DEVNET = "https://api.devnet.solana.com";
const ESCROW = new BN(2_000_000); // 0.002 SOL
const MAX_SCORE = 1_000_000;
const LEAN_PROOF_PLATFORM = 10;
const DEMO_CHALLENGE_WINDOW = 6; // seconds — shrunk global window for the demo
const AGENT_FUNDING = 6_000_000;

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
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

async function main(): Promise<void> {
  const authorityClient = loadKeypair(join(homedir(), ".config/solana/id.json"));
  const attester = Keypair.generate(); // throwaway demo oracle_authority
  const connection = new Connection(DEVNET, "confirmed");
  const provider = new anchor.AnchorProvider(
    connection,
    new anchor.Wallet(authorityClient),
    { commitment: "confirmed" }
  );
  anchor.setProvider(provider);
  const program = new anchor.Program(
    JSON.parse(
      readFileSync(
        join(__dirname, "..", "..", "target", "idl", "shillbot.json"),
        "utf8"
      )
    ) as anchor.Idl,
    provider
  ) as unknown as anchor.Program<Shillbot>;

  const [globalPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("shillbot_global")],
    program.programId
  );
  const bal = (pk: PublicKey) => connection.getBalance(pk, "confirmed");

  console.log(`authority/client: ${authorityClient.publicKey.toBase58()}`);
  console.log(`demo attester:    ${attester.publicKey.toBase58()}`);

  const snap = await program.account.globalState.fetch(globalPda);
  const restoreParams = () =>
    program.methods
      .updateParams(
        snap.protocolFeeBps,
        snap.qualityThreshold as BN,
        snap.challengeWindowSeconds as BN,
        snap.verificationTimeoutSeconds as BN,
        snap.attestationDelaySeconds as BN,
        snap.stalenessWindowSeconds as BN,
        snap.maxConcurrentClaims,
        snap.challengeBondMultiplierBps as unknown as number,
        snap.bondSlashTreasuryBps,
        snap.paused,
        snap.pausedPlatforms,
        snap.rateLimitWindowSeconds as BN,
        snap.maxTasksPerRateWindow,
        snap.disputeResolutionWindowSeconds as BN
      )
      .accountsPartial({
        globalState: globalPda,
        authority: authorityClient.publicKey,
      })
      .rpc();
  const restoreOracle = () =>
    program.methods
      .updateOracleAuthority(snap.oracleAuthority)
      .accountsPartial({
        globalState: globalPda,
        authority: authorityClient.publicKey,
      })
      .rpc();

  try {
    // Fund the demo attester (fee-payer for verify_task_attested).
    await provider.sendAndConfirm(
      new Transaction().add(
        SystemProgram.transfer({
          fromPubkey: authorityClient.publicKey,
          toPubkey: attester.publicKey,
          lamports: 20_000_000,
        })
      )
    );

    // Rotate oracle_authority → demo attester; shrink the challenge window.
    console.log("\n[setup] rotate oracle_authority + shrink challenge window");
    await program.methods
      .updateOracleAuthority(attester.publicKey)
      .accountsPartial({
        globalState: globalPda,
        authority: authorityClient.publicKey,
      })
      .rpc();
    await program.methods
      .updateParams(
        snap.protocolFeeBps,
        snap.qualityThreshold as BN,
        new BN(DEMO_CHALLENGE_WINDOW),
        snap.verificationTimeoutSeconds as BN,
        snap.attestationDelaySeconds as BN,
        snap.stalenessWindowSeconds as BN,
        snap.maxConcurrentClaims,
        snap.challengeBondMultiplierBps as unknown as number,
        snap.bondSlashTreasuryBps,
        snap.paused,
        snap.pausedPlatforms,
        snap.rateLimitWindowSeconds as BN,
        snap.maxTasksPerRateWindow,
        snap.disputeResolutionWindowSeconds as BN
      )
      .accountsPartial({
        globalState: globalPda,
        authority: authorityClient.publicKey,
      })
      .rpc();

    await runLifecycle(
      program,
      connection,
      provider,
      globalPda,
      authorityClient,
      attester,
      MAX_SCORE,
      "happy path (score = MAX): agent paid"
    );
    await runLifecycle(
      program,
      connection,
      provider,
      globalPda,
      authorityClient,
      attester,
      0,
      "failing proof (score = 0): full refund to client"
    );
  } finally {
    console.log("\n[teardown] restore oracle_authority + params");
    try {
      await restoreParams();
      await restoreOracle();
      // Throwaway attester/agent leftovers are devnet dust — a system account
      // can't be swept below its rent-exempt floor, so we leave them.
    } catch (e) {
      console.error("  ✗ teardown error:", e);
      failures++;
    }
  }

  console.log(
    `\n${failures === 0 ? "PASS" : "FAIL"} — ${failures} check(s) failed`
  );
  process.exit(failures === 0 ? 0 : 1);
}

async function runLifecycle(
  program: anchor.Program<Shillbot>,
  connection: Connection,
  provider: anchor.AnchorProvider,
  globalPda: PublicKey,
  client: Keypair,
  attester: Keypair,
  score: number,
  label: string
): Promise<void> {
  console.log(`\n=== ${label} ===`);
  const agent = Keypair.generate();
  await provider.sendAndConfirm(
    new Transaction().add(
      SystemProgram.transfer({
        fromPubkey: client.publicKey,
        toPubkey: agent.publicKey,
        lamports: AGENT_FUNDING,
      })
    )
  );

  const g = await program.account.globalState.fetch(globalPda);
  const counter = g.taskCounter as BN;
  const [taskPda] = PublicKey.findProgramAddressSync(
    [
      Buffer.from("task"),
      counter.toArrayLike(Buffer, "le", 8),
      client.publicKey.toBuffer(),
    ],
    program.programId
  );
  const [clientStatePda] = PublicKey.findProgramAddressSync(
    [Buffer.from("client_state"), client.publicKey.toBuffer()],
    program.programId
  );
  const [agentStatePda] = PublicKey.findProgramAddressSync(
    [Buffer.from("agent_state"), agent.publicKey.toBuffer()],
    program.programId
  );

  const statement = `theorem_${counter.toString()}`;
  const contentHash = Array.from(createHash("sha256").update(statement).digest());
  const now = Math.floor(Date.now() / 1000);

  // create_task kind=1 (LeanProof), requires_approval=false so the attester
  // verifies directly from Submitted.
  await program.methods
    .createTask(
      ESCROW,
      contentHash as never,
      new BN(now + 86_400),
      new BN(3600),
      new BN(14_400),
      LEAN_PROOF_PLATFORM,
      0,
      0, // challenge_window_override 0 = use the (shrunk) global window
      0,
      false,
      1 // verification_kind = DeterministicAttested
    )
    .accountsPartial({
      globalState: globalPda,
      task: taskPda,
      clientState: clientStatePda,
      client: client.publicKey,
      slotHashes: SYSVAR_SLOT_HASHES_PUBKEY,
      systemProgram: SystemProgram.programId,
    })
    .rpc();
  const created = await program.account.task.fetch(taskPda);
  check(created.verificationKind === 1, "task created with verification_kind = 1");

  // claim (fresh agent — arms-length from client) + submit artifact URL.
  await program.methods
    .claimTask()
    .accountsPartial({
      task: taskPda,
      globalState: globalPda,
      agentState: agentStatePda,
      agent: agent.publicKey,
      systemProgram: SystemProgram.programId,
    })
    .signers([agent])
    .rpc();
  await program.methods
    .submitWork(Buffer.from("https://artifacts.example/proof.lean"))
    .accountsPartial({
      task: taskPda,
      agentState: agentStatePda,
      agent: agent.publicKey,
    })
    .signers([agent])
    .rpc();

  // verify_task_attested — signed by the demo attester (= oracle_authority).
  const verificationHash = Array.from(randomBytes(32));
  await program.methods
    .verifyTaskAttested(new BN(score), verificationHash as never)
    .accountsPartial({
      task: taskPda,
      globalState: globalPda,
      attester: attester.publicKey,
    })
    .signers([attester])
    .rpc();
  const verified = await program.account.task.fetch(taskPda);
  check(JSON.stringify(verified.state) === JSON.stringify({ verified: {} }), "state = Verified");
  const escrow = (verified.escrowLamports as BN).toNumber();
  const payment = (verified.paymentAmount as BN).toNumber();
  const fee = (verified.feeAmount as BN).toNumber();
  if (score === MAX_SCORE) {
    const expectedFee = Math.floor((escrow * (g.protocolFeeBps as number)) / 10_000);
    check(payment === escrow - expectedFee, `payment = escrow − fee (${payment})`);
  } else {
    check(payment === 0 && fee === 0, "score 0 ⇒ payment and fee are 0");
  }

  // Wait past the shrunk challenge window, then finalize (permissionless).
  await sleep((DEMO_CHALLENGE_WINDOW + 3) * 1000);
  const agentBefore = await connection.getBalance(agent.publicKey, "confirmed");
  const clientBefore = await connection.getBalance(client.publicKey, "confirmed");
  await program.methods
    .finalizeTask()
    .accountsPartial({
      task: taskPda,
      globalState: globalPda,
      agent: agent.publicKey,
      client: client.publicKey,
      treasury: g.treasury,
    })
    .remainingAccounts([
      { pubkey: agentStatePda, isSigner: false, isWritable: true },
    ])
    .rpc();
  const closed = await connection.getAccountInfo(taskPda, "confirmed");
  check(closed === null, "task account closed after finalize");
  const agentAfter = await connection.getBalance(agent.publicKey, "confirmed");
  const clientAfter = await connection.getBalance(client.publicKey, "confirmed");
  if (score === MAX_SCORE) {
    check(agentAfter - agentBefore === payment, `agent received the payment (${payment})`);
  } else {
    // Full escrow + PDA rent refunded to the client; agent gets nothing.
    check(agentAfter === agentBefore, "agent received nothing on the failing proof");
    check(clientAfter > clientBefore, "client refunded escrow + rent on the failing proof");
  }

  // The throwaway agent's leftover is devnet dust (can't sweep a system
  // account below its rent-exempt floor); left in place.
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
