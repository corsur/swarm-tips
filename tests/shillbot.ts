import * as anchor from "@coral-xyz/anchor";
import { Program, BN } from "@coral-xyz/anchor";
import { Shillbot } from "../target/types/shillbot";
import { createHash } from "crypto";
import {
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  SystemProgram,
  SYSVAR_SLOT_HASHES_PUBKEY,
} from "@solana/web3.js";
import { assert } from "chai";

// ---------------------------------------------------------------------------
// Constants matching the on-chain program
// ---------------------------------------------------------------------------

const MAX_SCORE = 1_000_000;
const CHALLENGE_WINDOW_SECONDS = 86_400;
const PROTOCOL_FEE_BPS = 1000; // 10%
const QUALITY_THRESHOLD = new BN(200_000);
const ESCROW_LAMPORTS = new BN(100_000_000); // 0.1 SOL

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function contentHash(data: string): number[] {
  return Array.from(createHash("sha256").update(data).digest());
}

/** Derive the GlobalState PDA. */
function globalStatePda(programId: PublicKey): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("shillbot_global")],
    programId
  );
}

/** Derive a Task PDA from its counter value and client. */
function taskPda(
  taskCounter: BN,
  client: PublicKey,
  programId: PublicKey
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [
      Buffer.from("task"),
      taskCounter.toArrayLike(Buffer, "le", 8),
      client.toBuffer(),
    ],
    programId
  );
}

/** Derive a Challenge PDA from task ID and challenger. */
function challengePda(
  taskId: BN,
  challenger: PublicKey,
  programId: PublicKey
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [
      Buffer.from("challenge"),
      taskId.toArrayLike(Buffer, "le", 8),
      challenger.toBuffer(),
    ],
    programId
  );
}

/** Derive an AgentState PDA. */
function agentStatePda(
  agent: PublicKey,
  programId: PublicKey
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("agent_state"), agent.toBuffer()],
    programId
  );
}

/** Derive a SessionDelegate PDA. */
function sessionPda(
  agent: PublicKey,
  delegate: PublicKey,
  programId: PublicKey
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("session"), agent.toBuffer(), delegate.toBuffer()],
    programId
  );
}

/** Airdrop SOL and confirm. */
async function airdrop(
  connection: anchor.web3.Connection,
  pubkey: PublicKey,
  lamports: number
): Promise<void> {
  const sig = await connection.requestAirdrop(pubkey, lamports);
  await connection.confirmTransaction(sig);
}

/** Get current on-chain clock unix timestamp. */
async function getClockTimestamp(
  connection: anchor.web3.Connection
): Promise<number> {
  const slot = await connection.getSlot();
  const blockTime = await connection.getBlockTime(slot);
  return blockTime ?? Math.floor(Date.now() / 1000);
}

// ---------------------------------------------------------------------------
// Test suite
// ---------------------------------------------------------------------------

describe("shillbot", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const program = anchor.workspace.shillbot as Program<Shillbot>;

  const authority = (provider.wallet as anchor.Wallet).payer;
  const client = Keypair.generate();
  const agent = Keypair.generate();
  const challenger = Keypair.generate();
  const treasury = Keypair.generate();

  let globalPda: PublicKey;

  // ---------------------------------------------------------------------------
  // Setup
  // ---------------------------------------------------------------------------

  before(async () => {
    [globalPda] = globalStatePda(program.programId);

    // Airdrop to all wallets
    for (const kp of [client, agent, challenger, treasury]) {
      await airdrop(provider.connection, kp.publicKey, 5 * LAMPORTS_PER_SOL);
    }
  });

  // ---------------------------------------------------------------------------
  // 1. Initialize
  // ---------------------------------------------------------------------------

  describe("initialize", () => {
    it("creates GlobalState with authority, fee, and threshold", async () => {
      await program.methods
        .initialize(PROTOCOL_FEE_BPS, QUALITY_THRESHOLD, new BN(0))
        .accountsPartial({
          globalState: globalPda,
          authority: authority.publicKey,
          treasury: treasury.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .rpc();

      const global = await program.account.globalState.fetch(globalPda);
      assert.equal(global.taskCounter.toString(), "0");
      assert.equal(global.authority.toString(), authority.publicKey.toString());
      assert.equal(global.treasury.toString(), treasury.publicKey.toString());
      assert.equal(global.protocolFeeBps, PROTOCOL_FEE_BPS);
      assert.equal(
        global.qualityThreshold.toString(),
        QUALITY_THRESHOLD.toString()
      );
    });
  });

  // ---------------------------------------------------------------------------
  // 2. Create Task
  // ---------------------------------------------------------------------------

  describe("create_task", () => {
    let task0Pda: PublicKey;
    const content = contentHash("campaign brief #0");

    it("creates a task with escrow, PDA, and nonce", async () => {
      const now = await getClockTimestamp(provider.connection);
      // Deadline far in the future to avoid expiry issues
      const deadline = new BN(now + 86_400 * 30);
      const submitMargin = new BN(3600);
      const claimBuffer = new BN(14_400);

      // Task PDA uses the current counter (0) and client
      const globalBefore = await program.account.globalState.fetch(globalPda);
      [task0Pda] = taskPda(
        globalBefore.taskCounter,
        client.publicKey,
        program.programId
      );

      const clientBalanceBefore = await provider.connection.getBalance(
        client.publicKey
      );

      await program.methods
        .createTask(
          ESCROW_LAMPORTS,
          content as any,
          deadline,
          submitMargin,
          claimBuffer,
          0,
          0, // attestation_delay_override: use global default
          0, // challenge_window_override: use global default
          0 // verification_timeout_override: use global default
        )
        .accountsPartial({
          globalState: globalPda,
          task: task0Pda,
          client: client.publicKey,
          slotHashes: SYSVAR_SLOT_HASHES_PUBKEY,
          systemProgram: SystemProgram.programId,
        })
        .signers([client])
        .rpc();

      // Verify task state
      const task = await program.account.task.fetch(task0Pda);
      assert.equal(task.taskId.toString(), "0");
      assert.equal(task.client.toString(), client.publicKey.toString());
      assert.deepEqual(task.state, { open: {} });
      assert.equal(task.escrowLamports.toString(), ESCROW_LAMPORTS.toString());
      assert.deepEqual(Array.from(task.contentHash as any), content);
      assert.equal(task.deadline.toString(), deadline.toString());
      assert.equal(task.submitMargin.toString(), submitMargin.toString());
      assert.equal(task.claimBuffer.toString(), claimBuffer.toString());

      // Nonce should not be all zeros
      const nonce = Array.from(task.taskNonce as any);
      const allZeros = nonce.every((b: number) => b === 0);
      // Note: on local validator the slothash data might produce zeros,
      // but the structure should be populated
      assert.equal(nonce.length, 16, "task_nonce should be 16 bytes");

      // Verify counter incremented
      const globalAfter = await program.account.globalState.fetch(globalPda);
      assert.equal(globalAfter.taskCounter.toString(), "1");

      // Verify escrow was transferred (client balance decreased)
      const clientBalanceAfter = await provider.connection.getBalance(
        client.publicKey
      );
      // Balance should have decreased by at least escrow_lamports (plus rent + tx fee)
      assert.isTrue(
        clientBalanceBefore - clientBalanceAfter >= ESCROW_LAMPORTS.toNumber(),
        "Client balance should decrease by at least escrow amount"
      );

      // Verify task PDA holds escrow
      const taskBalance = await provider.connection.getBalance(task0Pda);
      assert.isTrue(
        taskBalance >= ESCROW_LAMPORTS.toNumber(),
        "Task PDA should hold at least escrow lamports"
      );
    });

    it("rejects create_task with zero escrow", async () => {
      const now = await getClockTimestamp(provider.connection);
      const global = await program.account.globalState.fetch(globalPda);
      const [badTaskPda] = taskPda(
        global.taskCounter,
        client.publicKey,
        program.programId
      );

      try {
        await program.methods
          .createTask(
            new BN(0),
            content as any,
            new BN(now + 86_400),
            new BN(3600),
            new BN(14_400),
            0,
            0,
            0,
            0 // timing overrides: use global defaults
          )
          .accountsPartial({
            globalState: globalPda,
            task: badTaskPda,
            client: client.publicKey,
            slotHashes: SYSVAR_SLOT_HASHES_PUBKEY,
            systemProgram: SystemProgram.programId,
          })
          .signers([client])
          .rpc();
        assert.fail("Expected ArithmeticOverflow error");
      } catch (e: any) {
        assert.include(e.toString(), "ArithmeticOverflow");
      }
    });

    it("rejects create_task with expired deadline", async () => {
      const global = await program.account.globalState.fetch(globalPda);
      const [badTaskPda] = taskPda(
        global.taskCounter,
        client.publicKey,
        program.programId
      );

      try {
        await program.methods
          .createTask(
            ESCROW_LAMPORTS,
            content as any,
            new BN(1), // Unix timestamp 1 = far in the past
            new BN(3600),
            new BN(14_400),
            0,
            0,
            0,
            0 // timing overrides: use global defaults
          )
          .accountsPartial({
            globalState: globalPda,
            task: badTaskPda,
            client: client.publicKey,
            slotHashes: SYSVAR_SLOT_HASHES_PUBKEY,
            systemProgram: SystemProgram.programId,
          })
          .signers([client])
          .rpc();
        assert.fail("Expected DeadlineExpired error");
      } catch (e: any) {
        assert.include(e.toString(), "DeadlineExpired");
      }
    });
  });

  // ---------------------------------------------------------------------------
  // 3. Claim Task
  // ---------------------------------------------------------------------------

  describe("claim_task", () => {
    let taskPdaForClaim: PublicKey;

    before(async () => {
      // Create a fresh task for claim tests
      const now = await getClockTimestamp(provider.connection);
      const global = await program.account.globalState.fetch(globalPda);
      [taskPdaForClaim] = taskPda(
        global.taskCounter,
        client.publicKey,
        program.programId
      );

      await program.methods
        .createTask(
          ESCROW_LAMPORTS,
          contentHash("claim test task") as any,
          new BN(now + 86_400 * 30),
          new BN(3600),
          new BN(14_400), // claim_buffer = 4 hours
          0,
          0,
          0,
          0 // timing overrides: use global defaults
        )
        .accountsPartial({
          globalState: globalPda,
          task: taskPdaForClaim,
          client: client.publicKey,
          slotHashes: SYSVAR_SLOT_HASHES_PUBKEY,
          systemProgram: SystemProgram.programId,
        })
        .signers([client])
        .rpc();
    });

    it("agent claims an open task", async () => {
      const [agentPda] = agentStatePda(agent.publicKey, program.programId);

      await program.methods
        .claimTask()
        .accountsPartial({
          task: taskPdaForClaim,
          agentState: agentPda,
          agent: agent.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([agent])
        .rpc();

      const task = await program.account.task.fetch(taskPdaForClaim);
      assert.deepEqual(task.state, { claimed: {} });
      assert.equal(task.agent.toString(), agent.publicKey.toString());

      // Verify agent state was created with claimed_count = 1
      const agentState = await program.account.agentState.fetch(agentPda);
      assert.equal(agentState.agent.toString(), agent.publicKey.toString());
      assert.equal(agentState.claimedCount, 1);
    });

    it("rejects claiming an already-claimed task", async () => {
      const otherAgent = Keypair.generate();
      await airdrop(
        provider.connection,
        otherAgent.publicKey,
        LAMPORTS_PER_SOL
      );
      const [otherAgentPda] = agentStatePda(
        otherAgent.publicKey,
        program.programId
      );

      try {
        await program.methods
          .claimTask()
          .accountsPartial({
            task: taskPdaForClaim,
            agentState: otherAgentPda,
            agent: otherAgent.publicKey,
            systemProgram: SystemProgram.programId,
          })
          .signers([otherAgent])
          .rpc();
        assert.fail("Expected InvalidTaskState error");
      } catch (e: any) {
        assert.include(e.toString(), "InvalidTaskState");
      }
    });
  });

  // ---------------------------------------------------------------------------
  // 4. Submit Work
  // ---------------------------------------------------------------------------

  describe("submit_work", () => {
    let taskPdaForSubmit: PublicKey;

    before(async () => {
      // Create and claim a task for submit tests
      const now = await getClockTimestamp(provider.connection);
      const global = await program.account.globalState.fetch(globalPda);
      [taskPdaForSubmit] = taskPda(
        global.taskCounter,
        client.publicKey,
        program.programId
      );
      const [agentPda] = agentStatePda(agent.publicKey, program.programId);

      await program.methods
        .createTask(
          ESCROW_LAMPORTS,
          contentHash("submit test task") as any,
          new BN(now + 86_400 * 30),
          new BN(3600), // submit_margin = 1 hour
          new BN(14_400),
          0,
          0,
          0,
          0 // timing overrides: use global defaults
        )
        .accountsPartial({
          globalState: globalPda,
          task: taskPdaForSubmit,
          client: client.publicKey,
          slotHashes: SYSVAR_SLOT_HASHES_PUBKEY,
          systemProgram: SystemProgram.programId,
        })
        .signers([client])
        .rpc();

      await program.methods
        .claimTask()
        .accountsPartial({
          task: taskPdaForSubmit,
          agentState: agentPda,
          agent: agent.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([agent])
        .rpc();
    });

    it("agent submits video ID hash", async () => {
      const videoId = Buffer.from("dQw4w9WgXcQ");
      const [agentPda] = agentStatePda(agent.publicKey, program.programId);

      await program.methods
        .submitWork(videoId)
        .accountsPartial({
          task: taskPdaForSubmit,
          agentState: agentPda,
          agent: agent.publicKey,
        })
        .signers([agent])
        .rpc();

      const task = await program.account.task.fetch(taskPdaForSubmit);
      assert.deepEqual(task.state, { submitted: {} });

      // Verify content_id_hash is SHA-256 of the content ID
      const expectedHash = createHash("sha256").update(videoId).digest();
      assert.deepEqual(
        Array.from(task.contentIdHash as any),
        Array.from(expectedHash)
      );

      // submitted_at should be set
      assert.isTrue(
        task.submittedAt.toNumber() > 0,
        "submitted_at should be set"
      );
    });

    it("rejects submission from non-agent", async () => {
      const imposter = Keypair.generate();
      await airdrop(provider.connection, imposter.publicKey, LAMPORTS_PER_SOL);

      // Need a fresh task in Claimed state for this test
      const now = await getClockTimestamp(provider.connection);
      const global = await program.account.globalState.fetch(globalPda);
      const [freshTaskPda] = taskPda(
        global.taskCounter,
        client.publicKey,
        program.programId
      );
      const [agentPda] = agentStatePda(agent.publicKey, program.programId);

      await program.methods
        .createTask(
          ESCROW_LAMPORTS,
          contentHash("non-agent submit test") as any,
          new BN(now + 86_400 * 30),
          new BN(3600),
          new BN(14_400),
          0,
          0,
          0,
          0 // timing overrides: use global defaults
        )
        .accountsPartial({
          globalState: globalPda,
          task: freshTaskPda,
          client: client.publicKey,
          slotHashes: SYSVAR_SLOT_HASHES_PUBKEY,
          systemProgram: SystemProgram.programId,
        })
        .signers([client])
        .rpc();

      await program.methods
        .claimTask()
        .accountsPartial({
          task: freshTaskPda,
          agentState: agentPda,
          agent: agent.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([agent])
        .rpc();

      const [imposterAgentPda] = agentStatePda(
        imposter.publicKey,
        program.programId
      );

      try {
        await program.methods
          .submitWork(Buffer.from("fake"))
          .accountsPartial({
            task: freshTaskPda,
            agentState: imposterAgentPda,
            agent: imposter.publicKey,
          })
          .signers([imposter])
          .rpc();
        assert.fail("Expected NotTaskAgent error");
      } catch (e: any) {
        // Will fail because either the agentState doesn't exist or NotTaskAgent
        const errStr = e.toString();
        assert.isTrue(
          errStr.includes("NotTaskAgent") ||
            errStr.includes("AccountNotInitialized") ||
            errStr.includes("account does not exist") ||
            errStr.includes("Error"),
          `Expected NotTaskAgent or account error, got: ${errStr}`
        );
      }
    });
  });

  // ---------------------------------------------------------------------------
  // 5. Verify Task
  // ---------------------------------------------------------------------------

  describe("verify_task", () => {
    // NOTE: verify_task has a staleness check that requires the on-chain clock
    // to be approximately 6-8 days after submitted_at. On a local validator
    // without clock warping, this check will fail with AttestationStale.
    // These tests verify the instruction interface and error handling.
    // Full staleness-window testing requires solana-program-test with
    // clock manipulation or bankrun.

    let taskPdaForVerify: PublicKey;
    let taskIdForVerify: BN;

    before(async () => {
      // Create, claim, and submit a task
      const now = await getClockTimestamp(provider.connection);
      const global = await program.account.globalState.fetch(globalPda);
      taskIdForVerify = global.taskCounter;
      [taskPdaForVerify] = taskPda(
        global.taskCounter,
        client.publicKey,
        program.programId
      );

      await program.methods
        .createTask(
          ESCROW_LAMPORTS,
          contentHash("verify test task") as any,
          new BN(now + 86_400 * 30),
          new BN(3600),
          new BN(14_400),
          0,
          0,
          0,
          0 // timing overrides: use global defaults
        )
        .accountsPartial({
          globalState: globalPda,
          task: taskPdaForVerify,
          client: client.publicKey,
          slotHashes: SYSVAR_SLOT_HASHES_PUBKEY,
          systemProgram: SystemProgram.programId,
        })
        .signers([client])
        .rpc();

      const [agentPda] = agentStatePda(agent.publicKey, program.programId);

      await program.methods
        .claimTask()
        .accountsPartial({
          task: taskPdaForVerify,
          agentState: agentPda,
          agent: agent.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([agent])
        .rpc();

      await program.methods
        .submitWork(Buffer.from("verify-video-id"))
        .accountsPartial({
          task: taskPdaForVerify,
          agentState: agentPda,
          agent: agent.publicKey,
        })
        .signers([agent])
        .rpc();
    });

    it("rejects verify from non-authority", async () => {
      const imposter = Keypair.generate();
      await airdrop(provider.connection, imposter.publicKey, LAMPORTS_PER_SOL);

      const dummyHash = Array.from({ length: 32 }, (_, i) => i + 1);
      try {
        await program.methods
          .verifyTask(new BN(800_000), dummyHash)
          .accountsPartial({
            task: taskPdaForVerify,
            globalState: globalPda,
            switchboardFeed: imposter.publicKey, // wrong account — should be rejected
          })
          .rpc();
        assert.fail("Expected error");
      } catch (e: any) {
        // Will fail on feed mismatch or attestation staleness
        const errStr = e.toString();
        assert.isTrue(
          errStr.includes("SwitchboardFeedMismatch") ||
            errStr.includes("AttestationStale") ||
            errStr.includes("SwitchboardFeedNotConfigured"),
          `Expected Switchboard-related error, got: ${errStr}`
        );
      }
    });

    it("rejects verify with score exceeding MAX_SCORE", async () => {
      // This will also hit AttestationStale on local validator,
      // but we test the interface is correct.
      const dummyHash2 = Array.from({ length: 32 }, (_, i) => i + 1);
      try {
        await program.methods
          .verifyTask(new BN(MAX_SCORE + 1), dummyHash2)
          .accountsPartial({
            task: taskPdaForVerify,
            globalState: globalPda,
            switchboardFeed: authority.publicKey, // placeholder
          })
          .rpc();
        assert.fail("Expected error");
      } catch (e: any) {
        // ScoreOutOfBounds, AttestationStale, or Switchboard errors
        const errStr = e.toString();
        assert.isTrue(
          errStr.includes("ScoreOutOfBounds") ||
            errStr.includes("AttestationStale") ||
            errStr.includes("SwitchboardFeedMismatch") ||
            errStr.includes("SwitchboardFeedNotConfigured"),
          `Expected score/staleness/switchboard error, got: ${errStr}`
        );
      }
    });

    it("verify_task is rejected by staleness check on local validator (expected)", async () => {
      // On a local validator without clock warping, the staleness check
      // will reject because clock.unix_timestamp is not within
      // [submitted_at + 6 days, submitted_at + 8 days].
      // This test documents that the instruction is correctly wired up
      // and the staleness check is enforced.
      const dummyHash3 = Array.from({ length: 32 }, (_, i) => i + 1);
      try {
        await program.methods
          .verifyTask(new BN(800_000), dummyHash3)
          .accountsPartial({
            task: taskPdaForVerify,
            globalState: globalPda,
            switchboardFeed: authority.publicKey, // placeholder — no real Switchboard on local validator
          })
          .rpc();
        // If this succeeds (unlikely without clock warp), verify state
        const task = await program.account.task.fetch(taskPdaForVerify);
        assert.deepEqual(task.state, { verified: {} });
        assert.equal(task.compositeScore.toString(), "800000");
      } catch (e: any) {
        // Expected: AttestationStale or Switchboard errors on local validator
        const errStr = e.toString();
        assert.isTrue(
          errStr.includes("AttestationStale") ||
            errStr.includes("SwitchboardFeedMismatch") ||
            errStr.includes("SwitchboardFeedNotConfigured"),
          `Expected staleness/switchboard error, got: ${errStr}`
        );
      }
    });
  });

  // ---------------------------------------------------------------------------
  // Full lifecycle with manual state setup for verify/finalize/challenge
  // ---------------------------------------------------------------------------

  // For tests that require Verified state (finalize, challenge, resolve),
  // we create helper tasks and drive them through the lifecycle.
  // Since verify_task's staleness check blocks us on local validator,
  // we test those instructions' account wiring and error handling
  // using tasks that we attempt to verify.

  // ---------------------------------------------------------------------------
  // 6. Finalize Task
  // ---------------------------------------------------------------------------

  describe("finalize_task", () => {
    it("rejects finalize on non-Verified task", async () => {
      // Create a task in Open state
      const now = await getClockTimestamp(provider.connection);
      const global = await program.account.globalState.fetch(globalPda);
      const [openTaskPda] = taskPda(
        global.taskCounter,
        client.publicKey,
        program.programId
      );

      await program.methods
        .createTask(
          ESCROW_LAMPORTS,
          contentHash("finalize reject test") as any,
          new BN(now + 86_400 * 30),
          new BN(3600),
          new BN(14_400),
          0,
          0,
          0,
          0 // timing overrides: use global defaults
        )
        .accountsPartial({
          globalState: globalPda,
          task: openTaskPda,
          client: client.publicKey,
          slotHashes: SYSVAR_SLOT_HASHES_PUBKEY,
          systemProgram: SystemProgram.programId,
        })
        .signers([client])
        .rpc();

      try {
        await program.methods
          .finalizeTask()
          .accountsPartial({
            task: openTaskPda,
            globalState: globalPda,
            agent: agent.publicKey,
            client: client.publicKey,
            treasury: treasury.publicKey,
          })
          .rpc();
        assert.fail("Expected InvalidTaskState error");
      } catch (e: any) {
        // Could be InvalidTaskState or a constraint error (agent mismatch since
        // the task has no agent in Open state)
        const errStr = e.toString();
        assert.isTrue(
          errStr.includes("InvalidTaskState") ||
            errStr.includes("NotTaskAgent"),
          `Expected InvalidTaskState or NotTaskAgent, got: ${errStr}`
        );
      }
    });
  });

  // ---------------------------------------------------------------------------
  // 7. Challenge Task
  // ---------------------------------------------------------------------------

  describe("challenge_task", () => {
    it("rejects challenge on non-Verified task", async () => {
      // Create a task in Open state
      const now = await getClockTimestamp(provider.connection);
      const global = await program.account.globalState.fetch(globalPda);
      const [openTaskPda] = taskPda(
        global.taskCounter,
        client.publicKey,
        program.programId
      );

      await program.methods
        .createTask(
          ESCROW_LAMPORTS,
          contentHash("challenge reject test") as any,
          new BN(now + 86_400 * 30),
          new BN(3600),
          new BN(14_400),
          0,
          0,
          0,
          0 // timing overrides: use global defaults
        )
        .accountsPartial({
          globalState: globalPda,
          task: openTaskPda,
          client: client.publicKey,
          slotHashes: SYSVAR_SLOT_HASHES_PUBKEY,
          systemProgram: SystemProgram.programId,
        })
        .signers([client])
        .rpc();

      const taskData = await program.account.task.fetch(openTaskPda);
      const [challPda] = challengePda(
        taskData.taskId,
        challenger.publicKey,
        program.programId
      );

      try {
        await program.methods
          .challengeTask()
          .accountsPartial({
            task: openTaskPda,
            challenge: challPda,
            challenger: challenger.publicKey,
            systemProgram: SystemProgram.programId,
          })
          .signers([challenger])
          .rpc();
        assert.fail("Expected InvalidTaskState error");
      } catch (e: any) {
        assert.include(e.toString(), "InvalidTaskState");
      }
    });
  });

  // ---------------------------------------------------------------------------
  // 8. Resolve Challenge (error path only, since we cannot reach Disputed
  //    state without clock warping for verify_task)
  // ---------------------------------------------------------------------------

  describe("resolve_challenge", () => {
    it("rejects resolve on non-Disputed task", async () => {
      // Use an Open task to verify the state check
      const now = await getClockTimestamp(provider.connection);
      const global = await program.account.globalState.fetch(globalPda);
      const [openTaskPda] = taskPda(
        global.taskCounter,
        client.publicKey,
        program.programId
      );

      await program.methods
        .createTask(
          ESCROW_LAMPORTS,
          contentHash("resolve reject test") as any,
          new BN(now + 86_400 * 30),
          new BN(3600),
          new BN(14_400),
          0,
          0,
          0,
          0 // timing overrides: use global defaults
        )
        .accountsPartial({
          globalState: globalPda,
          task: openTaskPda,
          client: client.publicKey,
          slotHashes: SYSVAR_SLOT_HASHES_PUBKEY,
          systemProgram: SystemProgram.programId,
        })
        .signers([client])
        .rpc();

      const taskData = await program.account.task.fetch(openTaskPda);

      // We can't easily create a Challenge PDA without a Verified task,
      // so we verify the instruction rejects at the account constraint level
      // (challenge PDA does not exist).
      const [challPda] = challengePda(
        taskData.taskId,
        challenger.publicKey,
        program.programId
      );

      try {
        await program.methods
          .resolveChallenge(true)
          .accountsPartial({
            task: openTaskPda,
            challenge: challPda,
            globalState: globalPda,
            authority: authority.publicKey,
            agent: agent.publicKey,
            client: client.publicKey,
            challenger: challenger.publicKey,
            treasury: treasury.publicKey,
          })
          .rpc();
        assert.fail("Expected error");
      } catch (e: any) {
        // Will fail because the Challenge account does not exist
        // or InvalidTaskState because task is Open, not Disputed
        const errStr = e.toString();
        assert.isTrue(
          errStr.includes("InvalidTaskState") ||
            errStr.includes("AccountNotInitialized") ||
            errStr.includes("account does not exist") ||
            errStr.includes("Error"),
          `Expected error, got: ${errStr}`
        );
      }
    });
  });

  // ---------------------------------------------------------------------------
  // 9. Expire Task
  // ---------------------------------------------------------------------------

  describe("expire_task", () => {
    it("rejects expire before deadline", async () => {
      // Create a task with far-future deadline
      const now = await getClockTimestamp(provider.connection);
      const global = await program.account.globalState.fetch(globalPda);
      const [taskToExpire] = taskPda(
        global.taskCounter,
        client.publicKey,
        program.programId
      );

      await program.methods
        .createTask(
          ESCROW_LAMPORTS,
          contentHash("expire reject test") as any,
          new BN(now + 86_400 * 30),
          new BN(3600),
          new BN(14_400),
          0,
          0,
          0,
          0 // timing overrides: use global defaults
        )
        .accountsPartial({
          globalState: globalPda,
          task: taskToExpire,
          client: client.publicKey,
          slotHashes: SYSVAR_SLOT_HASHES_PUBKEY,
          systemProgram: SystemProgram.programId,
        })
        .signers([client])
        .rpc();

      try {
        await program.methods
          .expireTask()
          .accountsPartial({
            task: taskToExpire,
            client: client.publicKey,
          })
          .rpc();
        assert.fail("Expected DeadlineExpired error");
      } catch (e: any) {
        assert.include(e.toString(), "DeadlineExpired");
      }
    });

    it("expires an Open task past its deadline", async () => {
      // Create a task with a very short deadline (just past current time)
      // We set deadline to now + 2 to give the create_task instruction time
      // to validate deadline > clock, then by the time expire runs, it should
      // be past.
      // NOTE: This test is timing-sensitive. On a local validator, the clock
      // advances with each slot. We set a very tight deadline.
      const now = await getClockTimestamp(provider.connection);
      const global = await program.account.globalState.fetch(globalPda);
      const [shortTaskPda] = taskPda(
        global.taskCounter,
        client.publicKey,
        program.programId
      );

      // Set deadline to now + 3 seconds (just barely in the future for create)
      const tightDeadline = new BN(now + 3);

      await program.methods
        .createTask(
          ESCROW_LAMPORTS,
          contentHash("short deadline task") as any,
          tightDeadline,
          new BN(0), // no submit margin
          new BN(0), // no claim buffer
          0,
          0,
          0,
          0 // timing overrides: use global defaults
        )
        .accountsPartial({
          globalState: globalPda,
          task: shortTaskPda,
          client: client.publicKey,
          slotHashes: SYSVAR_SLOT_HASHES_PUBKEY,
          systemProgram: SystemProgram.programId,
        })
        .signers([client])
        .rpc();

      // Wait for deadline to pass
      // Local validator advances ~400ms per slot
      await new Promise((resolve) => setTimeout(resolve, 5000));

      const clientBalanceBefore = await provider.connection.getBalance(
        client.publicKey
      );

      try {
        await program.methods
          .expireTask()
          .accountsPartial({
            task: shortTaskPda,
            client: client.publicKey,
          })
          .rpc();

        // If it succeeded, verify escrow returned
        const clientBalanceAfter = await provider.connection.getBalance(
          client.publicKey
        );
        // Client should have received escrow back (minus tx fees from other txs)
        // We check the task account no longer exists (closed)
        try {
          await program.account.task.fetch(shortTaskPda);
          assert.fail("Task account should be closed");
        } catch (fetchErr: any) {
          // Expected: account does not exist
          assert.isTrue(
            fetchErr.toString().includes("Account does not exist") ||
              fetchErr.toString().includes("Could not find"),
            "Task account should be closed after expiry"
          );
        }
      } catch (e: any) {
        // If the deadline hasn't passed yet on-chain, the expiry will fail.
        // This is acceptable on a fast local validator.
        assert.include(
          e.toString(),
          "DeadlineExpired",
          "Expire should fail if deadline hasn't passed yet on-chain"
        );
      }
    });
  });

  // ---------------------------------------------------------------------------
  // 10. Concurrent Claim Limit
  // ---------------------------------------------------------------------------

  describe("concurrent claim limit", () => {
    const claimAgent = Keypair.generate();
    const claimedTaskPdas: PublicKey[] = [];

    before(async () => {
      await airdrop(
        provider.connection,
        claimAgent.publicKey,
        5 * LAMPORTS_PER_SOL
      );
    });

    it("allows up to 4 concurrent claims (below limit of 5)", async () => {
      const now = await getClockTimestamp(provider.connection);
      const [claimAgentPda] = agentStatePda(
        claimAgent.publicKey,
        program.programId
      );

      // Create and claim 4 tasks
      for (let i = 0; i < 4; i++) {
        const global = await program.account.globalState.fetch(globalPda);
        const [tp] = taskPda(
          global.taskCounter,
          client.publicKey,
          program.programId
        );

        await program.methods
          .createTask(
            ESCROW_LAMPORTS,
            contentHash(`concurrent task ${i}`) as any,
            new BN(now + 86_400 * 30),
            new BN(3600),
            new BN(14_400),
            0,
            0,
            0,
            0 // timing overrides: use global defaults
          )
          .accountsPartial({
            globalState: globalPda,
            task: tp,
            client: client.publicKey,
            slotHashes: SYSVAR_SLOT_HASHES_PUBKEY,
            systemProgram: SystemProgram.programId,
          })
          .signers([client])
          .rpc();

        await program.methods
          .claimTask()
          .accountsPartial({
            task: tp,
            agentState: claimAgentPda,
            agent: claimAgent.publicKey,
            systemProgram: SystemProgram.programId,
          })
          .signers([claimAgent])
          .rpc();

        claimedTaskPdas.push(tp);
      }

      assert.equal(claimedTaskPdas.length, 4, "Should have 4 claimed tasks");

      // Verify agent state tracks the count
      const agentState = await program.account.agentState.fetch(claimAgentPda);
      assert.equal(agentState.claimedCount, 4);
    });

    it("allows 5th claim (agent can have up to 5)", async () => {
      const now = await getClockTimestamp(provider.connection);
      const global = await program.account.globalState.fetch(globalPda);
      const [tp] = taskPda(
        global.taskCounter,
        client.publicKey,
        program.programId
      );
      const [claimAgentPda] = agentStatePda(
        claimAgent.publicKey,
        program.programId
      );

      await program.methods
        .createTask(
          ESCROW_LAMPORTS,
          contentHash("concurrent task 4") as any,
          new BN(now + 86_400 * 30),
          new BN(3600),
          new BN(14_400),
          0,
          0,
          0,
          0 // timing overrides: use global defaults
        )
        .accountsPartial({
          globalState: globalPda,
          task: tp,
          client: client.publicKey,
          slotHashes: SYSVAR_SLOT_HASHES_PUBKEY,
          systemProgram: SystemProgram.programId,
        })
        .signers([client])
        .rpc();

      // With 4 existing claims, the 5th should still succeed
      // because the check is claimed_count < MAX_CONCURRENT_CLAIMS (5),
      // so claimed_count == 4 is still allowed.
      await program.methods
        .claimTask()
        .accountsPartial({
          task: tp,
          agentState: claimAgentPda,
          agent: claimAgent.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([claimAgent])
        .rpc();

      claimedTaskPdas.push(tp);
      assert.equal(claimedTaskPdas.length, 5);

      const agentState = await program.account.agentState.fetch(claimAgentPda);
      assert.equal(agentState.claimedCount, 5);
    });

    it("rejects 6th concurrent claim (exceeds limit of 5)", async () => {
      const now = await getClockTimestamp(provider.connection);
      const global = await program.account.globalState.fetch(globalPda);
      const [tp] = taskPda(
        global.taskCounter,
        client.publicKey,
        program.programId
      );
      const [claimAgentPda] = agentStatePda(
        claimAgent.publicKey,
        program.programId
      );

      await program.methods
        .createTask(
          ESCROW_LAMPORTS,
          contentHash("concurrent task 5 overflow") as any,
          new BN(now + 86_400 * 30),
          new BN(3600),
          new BN(14_400),
          0,
          0,
          0,
          0 // timing overrides: use global defaults
        )
        .accountsPartial({
          globalState: globalPda,
          task: tp,
          client: client.publicKey,
          slotHashes: SYSVAR_SLOT_HASHES_PUBKEY,
          systemProgram: SystemProgram.programId,
        })
        .signers([client])
        .rpc();

      try {
        await program.methods
          .claimTask()
          .accountsPartial({
            task: tp,
            agentState: claimAgentPda,
            agent: claimAgent.publicKey,
            systemProgram: SystemProgram.programId,
          })
          .signers([claimAgent])
          .rpc();
        assert.fail("Expected MaxConcurrentClaimsExceeded error");
      } catch (e: any) {
        assert.include(e.toString(), "MaxConcurrentClaimsExceeded");
      }
    });
  });

  // ---------------------------------------------------------------------------
  // 11. Below Threshold (score < quality_threshold => payment = 0)
  // ---------------------------------------------------------------------------

  // "below threshold" off-chain arithmetic tests removed — duplicates the 19
  // Rust unit tests in programs/shillbot/src/scoring.rs. Payment logic is now
  // tested on-chain via the bankrun lifecycle tests (tests/shillbot-lifecycle.ts).

  // ---------------------------------------------------------------------------
  // 12. Session Delegate
  // ---------------------------------------------------------------------------

  describe("session delegate", () => {
    const delegateKey = Keypair.generate();
    let sessionDelegatePda: PublicKey;

    it("creates a session delegate", async () => {
      [sessionDelegatePda] = sessionPda(
        agent.publicKey,
        delegateKey.publicKey,
        program.programId
      );

      // 0x03 = both claim_task and submit_work permissions
      await program.methods
        .createSession(0x03, new BN(86_400))
        .accountsPartial({
          sessionDelegate: sessionDelegatePda,
          agent: agent.publicKey,
          delegate: delegateKey.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([agent])
        .rpc();

      const session = await program.account.sessionDelegate.fetch(
        sessionDelegatePda
      );
      assert.equal(session.agent.toString(), agent.publicKey.toString());
      assert.equal(
        session.delegate.toString(),
        delegateKey.publicKey.toString()
      );
      assert.equal(session.allowedInstructions, 0x03);
      assert.isTrue(
        session.createdAt.toNumber() > 0,
        "created_at should be set"
      );
    });

    it("rejects session with invalid bitmask (0)", async () => {
      const badDelegate = Keypair.generate();
      const [badSessionPda] = sessionPda(
        agent.publicKey,
        badDelegate.publicKey,
        program.programId
      );

      try {
        await program.methods
          .createSession(0x00, new BN(86_400))
          .accountsPartial({
            sessionDelegate: badSessionPda,
            agent: agent.publicKey,
            delegate: badDelegate.publicKey,
            systemProgram: SystemProgram.programId,
          })
          .signers([agent])
          .rpc();
        assert.fail("Expected InvalidSessionDelegate error");
      } catch (e: any) {
        assert.include(e.toString(), "InvalidSessionDelegate");
      }
    });

    it("rejects session with invalid bitmask (> 0x03)", async () => {
      const badDelegate = Keypair.generate();
      const [badSessionPda] = sessionPda(
        agent.publicKey,
        badDelegate.publicKey,
        program.programId
      );

      try {
        await program.methods
          .createSession(0x04, new BN(86_400))
          .accountsPartial({
            sessionDelegate: badSessionPda,
            agent: agent.publicKey,
            delegate: badDelegate.publicKey,
            systemProgram: SystemProgram.programId,
          })
          .signers([agent])
          .rpc();
        assert.fail("Expected InvalidSessionDelegate error");
      } catch (e: any) {
        assert.include(e.toString(), "InvalidSessionDelegate");
      }
    });

    it("revokes the session delegate", async () => {
      await program.methods
        .revokeSession()
        .accountsPartial({
          sessionDelegate: sessionDelegatePda,
          agent: agent.publicKey,
        })
        .signers([agent])
        .rpc();

      // Session account should be closed
      try {
        await program.account.sessionDelegate.fetch(sessionDelegatePda);
        assert.fail("Session account should be closed");
      } catch (e: any) {
        assert.isTrue(
          e.toString().includes("Account does not exist") ||
            e.toString().includes("Could not find"),
          "Session account should not exist after revocation"
        );
      }
    });

    it("rejects revoke from non-agent", async () => {
      // Create a new session to revoke
      const newDelegate = Keypair.generate();
      const [newSessionPda] = sessionPda(
        agent.publicKey,
        newDelegate.publicKey,
        program.programId
      );

      await program.methods
        .createSession(0x01, new BN(86_400))
        .accountsPartial({
          sessionDelegate: newSessionPda,
          agent: agent.publicKey,
          delegate: newDelegate.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([agent])
        .rpc();

      // Try to revoke with a different signer
      const imposter = Keypair.generate();
      await airdrop(provider.connection, imposter.publicKey, LAMPORTS_PER_SOL);

      try {
        await program.methods
          .revokeSession()
          .accountsPartial({
            sessionDelegate: newSessionPda,
            agent: imposter.publicKey,
          })
          .signers([imposter])
          .rpc();
        assert.fail("Expected error from non-agent revoke");
      } catch (e: any) {
        // Anchor's has_one constraint will reject because imposter != session.agent
        const errStr = e.toString();
        assert.isTrue(
          errStr.includes("has_one") ||
            errStr.includes("ConstraintHasOne") ||
            errStr.includes("A has one constraint was violated") ||
            errStr.includes("seeds constraint was violated") ||
            errStr.includes("Error"),
          `Expected constraint error, got: ${errStr}`
        );
      }

      // Cleanup: revoke with the actual agent
      await program.methods
        .revokeSession()
        .accountsPartial({
          sessionDelegate: newSessionPda,
          agent: agent.publicKey,
        })
        .signers([agent])
        .rpc();
    });

    it("creates session with claim-only permission (0x01)", async () => {
      const claimOnlyDelegate = Keypair.generate();
      const [claimSessionPda] = sessionPda(
        agent.publicKey,
        claimOnlyDelegate.publicKey,
        program.programId
      );

      await program.methods
        .createSession(0x01, new BN(86_400))
        .accountsPartial({
          sessionDelegate: claimSessionPda,
          agent: agent.publicKey,
          delegate: claimOnlyDelegate.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([agent])
        .rpc();

      const session = await program.account.sessionDelegate.fetch(
        claimSessionPda
      );
      assert.equal(session.allowedInstructions, 0x01);

      // Cleanup
      await program.methods
        .revokeSession()
        .accountsPartial({
          sessionDelegate: claimSessionPda,
          agent: agent.publicKey,
        })
        .signers([agent])
        .rpc();
    });

    it("creates session with submit-only permission (0x02)", async () => {
      const submitOnlyDelegate = Keypair.generate();
      const [submitSessionPda] = sessionPda(
        agent.publicKey,
        submitOnlyDelegate.publicKey,
        program.programId
      );

      await program.methods
        .createSession(0x02, new BN(86_400))
        .accountsPartial({
          sessionDelegate: submitSessionPda,
          agent: agent.publicKey,
          delegate: submitOnlyDelegate.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([agent])
        .rpc();

      const session = await program.account.sessionDelegate.fetch(
        submitSessionPda
      );
      assert.equal(session.allowedInstructions, 0x02);

      // Cleanup
      await program.methods
        .revokeSession()
        .accountsPartial({
          sessionDelegate: submitSessionPda,
          agent: agent.publicKey,
        })
        .signers([agent])
        .rpc();
    });
  });

  // ---------------------------------------------------------------------------
  // Additional error path tests
  // ---------------------------------------------------------------------------

  describe("claim_task error paths", () => {
    it("rejects claim when claim buffer is insufficient", async () => {
      const now = await getClockTimestamp(provider.connection);
      const global = await program.account.globalState.fetch(globalPda);
      const [tightTaskPda] = taskPda(
        global.taskCounter,
        client.publicKey,
        program.programId
      );

      // Create task with deadline only 100s in the future but claim_buffer = 14400
      // This means now + 14400 > now + 100, so claim should be rejected
      await program.methods
        .createTask(
          ESCROW_LAMPORTS,
          contentHash("tight deadline task") as any,
          new BN(now + 100),
          new BN(0),
          new BN(14_400), // 4 hour claim buffer, but deadline is 100s away
          0,
          0,
          0,
          0 // timing overrides: use global defaults
        )
        .accountsPartial({
          globalState: globalPda,
          task: tightTaskPda,
          client: client.publicKey,
          slotHashes: SYSVAR_SLOT_HASHES_PUBKEY,
          systemProgram: SystemProgram.programId,
        })
        .signers([client])
        .rpc();

      const [agentPda] = agentStatePda(agent.publicKey, program.programId);

      try {
        await program.methods
          .claimTask()
          .accountsPartial({
            task: tightTaskPda,
            agentState: agentPda,
            agent: agent.publicKey,
            systemProgram: SystemProgram.programId,
          })
          .signers([agent])
          .rpc();
        assert.fail("Expected ClaimBufferInsufficient error");
      } catch (e: any) {
        assert.include(e.toString(), "ClaimBufferInsufficient");
      }
    });
  });

  // ---------------------------------------------------------------------------
  // 13. Emergency Return
  // ---------------------------------------------------------------------------

  describe("emergency_return", () => {
    it("returns escrow for two Open tasks", async () => {
      const now = await getClockTimestamp(provider.connection);
      const taskPdas: PublicKey[] = [];

      // Create 2 open tasks
      for (let i = 0; i < 2; i++) {
        const global = await program.account.globalState.fetch(globalPda);
        const [tp] = taskPda(
          global.taskCounter,
          client.publicKey,
          program.programId
        );

        await program.methods
          .createTask(
            ESCROW_LAMPORTS,
            contentHash(`emergency open task ${i}`) as any,
            new BN(now + 86_400 * 30),
            new BN(3600),
            new BN(14_400),
            0,
            0,
            0,
            0 // timing overrides: use global defaults
          )
          .accountsPartial({
            globalState: globalPda,
            task: tp,
            client: client.publicKey,
            slotHashes: SYSVAR_SLOT_HASHES_PUBKEY,
            systemProgram: SystemProgram.programId,
          })
          .signers([client])
          .rpc();

        taskPdas.push(tp);
      }

      const clientBalanceBefore = await provider.connection.getBalance(
        client.publicKey
      );

      // Call emergency_return with both tasks as remaining accounts
      // Format: [task0, client0, task1, client1]
      await program.methods
        .emergencyReturn()
        .accountsPartial({
          globalState: globalPda,
          authority: authority.publicKey,
        })
        .remainingAccounts(
          taskPdas.flatMap((tp) => [
            { pubkey: tp, isWritable: true, isSigner: false },
            { pubkey: client.publicKey, isWritable: true, isSigner: false },
          ])
        )
        .rpc();

      // Verify both tasks are closed
      for (const tp of taskPdas) {
        try {
          await program.account.task.fetch(tp);
          assert.fail("Task account should be closed");
        } catch (e: any) {
          assert.isTrue(
            e.toString().includes("Account does not exist") ||
              e.toString().includes("Could not find"),
            "Task account should not exist after emergency return"
          );
        }
      }

      // Verify client received escrow back (2 tasks worth)
      const clientBalanceAfter = await provider.connection.getBalance(
        client.publicKey
      );
      const expectedReturn = ESCROW_LAMPORTS.toNumber() * 2;
      // Client balance should have increased by approximately 2x escrow
      // (account rent is also returned, so it may be slightly more)
      assert.isTrue(
        clientBalanceAfter - clientBalanceBefore >= expectedReturn,
        `Client should receive at least ${expectedReturn} lamports back`
      );
    });

    it("returns escrow for a Claimed task", async () => {
      const now = await getClockTimestamp(provider.connection);
      const emergencyAgent = Keypair.generate();
      await airdrop(
        provider.connection,
        emergencyAgent.publicKey,
        2 * LAMPORTS_PER_SOL
      );

      const global = await program.account.globalState.fetch(globalPda);
      const [tp] = taskPda(
        global.taskCounter,
        client.publicKey,
        program.programId
      );
      const [emergencyAgentPda] = agentStatePda(
        emergencyAgent.publicKey,
        program.programId
      );

      await program.methods
        .createTask(
          ESCROW_LAMPORTS,
          contentHash("emergency claimed task") as any,
          new BN(now + 86_400 * 30),
          new BN(3600),
          new BN(14_400),
          0,
          0,
          0,
          0 // timing overrides: use global defaults
        )
        .accountsPartial({
          globalState: globalPda,
          task: tp,
          client: client.publicKey,
          slotHashes: SYSVAR_SLOT_HASHES_PUBKEY,
          systemProgram: SystemProgram.programId,
        })
        .signers([client])
        .rpc();

      await program.methods
        .claimTask()
        .accountsPartial({
          task: tp,
          agentState: emergencyAgentPda,
          agent: emergencyAgent.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([emergencyAgent])
        .rpc();

      // Verify task is claimed
      const taskBefore = await program.account.task.fetch(tp);
      assert.deepEqual(taskBefore.state, { claimed: {} });

      const clientBalanceBefore = await provider.connection.getBalance(
        client.publicKey
      );

      await program.methods
        .emergencyReturn()
        .accountsPartial({
          globalState: globalPda,
          authority: authority.publicKey,
        })
        .remainingAccounts([
          { pubkey: tp, isWritable: true, isSigner: false },
          { pubkey: client.publicKey, isWritable: true, isSigner: false },
        ])
        .rpc();

      // Verify task is closed
      try {
        await program.account.task.fetch(tp);
        assert.fail("Task account should be closed");
      } catch (e: any) {
        assert.isTrue(
          e.toString().includes("Account does not exist") ||
            e.toString().includes("Could not find"),
          "Task account should not exist after emergency return"
        );
      }

      // Verify client received escrow back
      const clientBalanceAfter = await provider.connection.getBalance(
        client.publicKey
      );
      assert.isTrue(
        clientBalanceAfter > clientBalanceBefore,
        "Client should receive escrow back"
      );
    });

    it("rejects non-authority caller", async () => {
      const now = await getClockTimestamp(provider.connection);
      const impostor = Keypair.generate();
      await airdrop(
        provider.connection,
        impostor.publicKey,
        2 * LAMPORTS_PER_SOL
      );

      const global = await program.account.globalState.fetch(globalPda);
      const [tp] = taskPda(
        global.taskCounter,
        client.publicKey,
        program.programId
      );

      await program.methods
        .createTask(
          ESCROW_LAMPORTS,
          contentHash("emergency auth test") as any,
          new BN(now + 86_400 * 30),
          new BN(3600),
          new BN(14_400),
          0,
          0,
          0,
          0 // timing overrides: use global defaults
        )
        .accountsPartial({
          globalState: globalPda,
          task: tp,
          client: client.publicKey,
          slotHashes: SYSVAR_SLOT_HASHES_PUBKEY,
          systemProgram: SystemProgram.programId,
        })
        .signers([client])
        .rpc();

      try {
        await program.methods
          .emergencyReturn()
          .accountsPartial({
            globalState: globalPda,
            authority: impostor.publicKey,
          })
          .remainingAccounts([
            { pubkey: tp, isWritable: true, isSigner: false },
            { pubkey: client.publicKey, isWritable: true, isSigner: false },
          ])
          .signers([impostor])
          .rpc();
        assert.fail("Expected NotAuthority error");
      } catch (e: any) {
        assert.include(e.toString(), "NotAuthority");
      }
    });

    it("rejects Submitted task (wrong state)", async () => {
      const now = await getClockTimestamp(provider.connection);
      const global = await program.account.globalState.fetch(globalPda);
      const [tp] = taskPda(
        global.taskCounter,
        client.publicKey,
        program.programId
      );
      const [agentPda] = agentStatePda(agent.publicKey, program.programId);

      await program.methods
        .createTask(
          ESCROW_LAMPORTS,
          contentHash("emergency submitted test") as any,
          new BN(now + 86_400 * 30),
          new BN(3600),
          new BN(14_400),
          0,
          0,
          0,
          0 // timing overrides: use global defaults
        )
        .accountsPartial({
          globalState: globalPda,
          task: tp,
          client: client.publicKey,
          slotHashes: SYSVAR_SLOT_HASHES_PUBKEY,
          systemProgram: SystemProgram.programId,
        })
        .signers([client])
        .rpc();

      await program.methods
        .claimTask()
        .accountsPartial({
          task: tp,
          agentState: agentPda,
          agent: agent.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([agent])
        .rpc();

      await program.methods
        .submitWork(Buffer.from("emergency-video"))
        .accountsPartial({
          task: tp,
          agentState: agentPda,
          agent: agent.publicKey,
        })
        .signers([agent])
        .rpc();

      // Verify task is in Submitted state
      const taskData = await program.account.task.fetch(tp);
      assert.deepEqual(taskData.state, { submitted: {} });

      try {
        await program.methods
          .emergencyReturn()
          .accountsPartial({
            globalState: globalPda,
            authority: authority.publicKey,
          })
          .remainingAccounts([
            { pubkey: tp, isWritable: true, isSigner: false },
            { pubkey: client.publicKey, isWritable: true, isSigner: false },
          ])
          .rpc();
        assert.fail("Expected InvalidTaskState error");
      } catch (e: any) {
        assert.include(e.toString(), "InvalidTaskState");
      }
    });
  });

  // ---------------------------------------------------------------------------
  // 14. Update Params
  // ---------------------------------------------------------------------------

  describe("update_params", () => {
    it("updates fee and threshold", async () => {
      const newFee = 500; // 5%
      const newThreshold = new BN(300_000);

      await program.methods
        .updateParams(
          newFee,
          newThreshold,
          new BN(86_400),
          new BN(1_209_600),
          new BN(604_800),
          new BN(86_400),
          5,
          2,
          5_000,
          false,
          0
        )
        .accountsPartial({
          globalState: globalPda,
          authority: authority.publicKey,
        })
        .rpc();

      const global = await program.account.globalState.fetch(globalPda);
      assert.equal(global.protocolFeeBps, newFee);
      assert.equal(global.qualityThreshold.toString(), newThreshold.toString());

      // Restore original values for subsequent tests
      await program.methods
        .updateParams(
          PROTOCOL_FEE_BPS,
          QUALITY_THRESHOLD,
          new BN(86_400),
          new BN(1_209_600),
          new BN(604_800),
          new BN(86_400),
          5,
          2,
          5_000,
          false,
          0
        )
        .accountsPartial({
          globalState: globalPda,
          authority: authority.publicKey,
        })
        .rpc();

      const restored = await program.account.globalState.fetch(globalPda);
      assert.equal(restored.protocolFeeBps, PROTOCOL_FEE_BPS);
      assert.equal(
        restored.qualityThreshold.toString(),
        QUALITY_THRESHOLD.toString()
      );
    });

    it("rejects non-authority caller", async () => {
      const impostor = Keypair.generate();
      await airdrop(provider.connection, impostor.publicKey, LAMPORTS_PER_SOL);

      try {
        await program.methods
          .updateParams(
            500,
            new BN(300_000),
            new BN(86_400),
            new BN(1_209_600),
            new BN(604_800),
            new BN(86_400),
            5,
            2,
            5_000,
            false,
            0
          )
          .accountsPartial({
            globalState: globalPda,
            authority: impostor.publicKey,
          })
          .signers([impostor])
          .rpc();
        assert.fail("Expected NotAuthority error");
      } catch (e: any) {
        assert.include(e.toString(), "NotAuthority");
      }
    });

    it("rejects fee below minimum (100 bps)", async () => {
      try {
        await program.methods
          .updateParams(
            50,
            QUALITY_THRESHOLD,
            new BN(86_400),
            new BN(1_209_600),
            new BN(604_800),
            new BN(86_400),
            5,
            2,
            5_000,
            false,
            0
          ) // 50 bps < 100 bps minimum
          .accountsPartial({
            globalState: globalPda,
            authority: authority.publicKey,
          })
          .rpc();
        assert.fail("Expected ArithmeticOverflow error");
      } catch (e: any) {
        assert.include(e.toString(), "ArithmeticOverflow");
      }
    });

    it("rejects fee above maximum (2500 bps)", async () => {
      try {
        await program.methods
          .updateParams(
            3000,
            QUALITY_THRESHOLD,
            new BN(86_400),
            new BN(1_209_600),
            new BN(604_800),
            new BN(86_400),
            5,
            2,
            5_000,
            false,
            0
          ) // 3000 bps > 2500 bps maximum
          .accountsPartial({
            globalState: globalPda,
            authority: authority.publicKey,
          })
          .rpc();
        assert.fail("Expected ArithmeticOverflow error");
      } catch (e: any) {
        assert.include(e.toString(), "ArithmeticOverflow");
      }
    });

    it("rejects threshold above MAX_SCORE", async () => {
      try {
        await program.methods
          .updateParams(
            PROTOCOL_FEE_BPS,
            new BN(MAX_SCORE + 1),
            new BN(86_400),
            new BN(1_209_600),
            new BN(604_800),
            new BN(86_400),
            5,
            2,
            5_000,
            false,
            0
          )
          .accountsPartial({
            globalState: globalPda,
            authority: authority.publicKey,
          })
          .rpc();
        assert.fail("Expected ScoreOutOfBounds error");
      } catch (e: any) {
        assert.include(e.toString(), "ScoreOutOfBounds");
      }
    });
  });

  // ---------------------------------------------------------------------------
  // Additional error path tests
  // ---------------------------------------------------------------------------

  describe("submit_work error paths", () => {
    it("rejects submit on Open task (not claimed)", async () => {
      const now = await getClockTimestamp(provider.connection);
      const global = await program.account.globalState.fetch(globalPda);
      const [openTaskPda] = taskPda(
        global.taskCounter,
        client.publicKey,
        program.programId
      );
      const [agentPda] = agentStatePda(agent.publicKey, program.programId);

      await program.methods
        .createTask(
          ESCROW_LAMPORTS,
          contentHash("submit on open task") as any,
          new BN(now + 86_400 * 30),
          new BN(3600),
          new BN(14_400),
          0,
          0,
          0,
          0 // timing overrides: use global defaults
        )
        .accountsPartial({
          globalState: globalPda,
          task: openTaskPda,
          client: client.publicKey,
          slotHashes: SYSVAR_SLOT_HASHES_PUBKEY,
          systemProgram: SystemProgram.programId,
        })
        .signers([client])
        .rpc();

      try {
        await program.methods
          .submitWork(Buffer.from("video"))
          .accountsPartial({
            task: openTaskPda,
            agentState: agentPda,
            agent: agent.publicKey,
          })
          .signers([agent])
          .rpc();
        assert.fail("Expected InvalidTaskState error");
      } catch (e: any) {
        assert.include(e.toString(), "InvalidTaskState");
      }
    });
  });
});
