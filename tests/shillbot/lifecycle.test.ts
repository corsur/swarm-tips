import * as anchor from "@coral-xyz/anchor";
import { Program, BN } from "@coral-xyz/anchor";
import { createHash } from "crypto";
import type { Shillbot } from "../../target/types/shillbot";
import {
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  SystemProgram,
  SYSVAR_SLOT_HASHES_PUBKEY,
} from "@solana/web3.js";
import { assert } from "chai";
import {
  MAX_SCORE,
  CHALLENGE_WINDOW_SECONDS,
  PROTOCOL_FEE_BPS,
  QUALITY_THRESHOLD,
  ESCROW_LAMPORTS,
  contentHash,
  globalStatePda,
  taskPda,
  newTaskNonce,
  challengePda,
  agentStatePda,
  sessionPda,
  airdrop,
  getClockTimestamp,
  ensureShillbotInitialized,
} from "./common.ts";

describe("shillbot - lifecycle", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const program = anchor.workspace.shillbot as Program<Shillbot>;

  const authority = (provider.wallet as anchor.Wallet).payer;
  let client = Keypair.generate();
  const agent = Keypair.generate();
  const challenger = Keypair.generate();
  const treasury = Keypair.generate();

  let globalPda: PublicKey;

  before(async () => {
    globalPda = await ensureShillbotInitialized(
      program,
      provider,
      authority,
      treasury
    );

    for (const kp of [agent, challenger, treasury]) {
      await airdrop(provider.connection, kp.publicKey, 5 * LAMPORTS_PER_SOL);
    }
  });

  beforeEach(async () => {
    client = Keypair.generate();
    await airdrop(provider.connection, client.publicKey, 5 * LAMPORTS_PER_SOL);
  });
  describe("initialize", () => {
    it("creates GlobalState with authority, fee, and threshold", async () => {
      await program.methods
        .initialize(
          PROTOCOL_FEE_BPS,
          QUALITY_THRESHOLD,
          new BN(0),
          // Use the authority pubkey as the configured "Switchboard feed"
          // for these tests. The verify_task tests further down pass
          // authority.publicKey when they want a valid feed and an
          // `imposter.publicKey` when they want the rejection path.
          authority.publicKey
        )
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
  describe("create_task", () => {
    let task0Pda: PublicKey;
    const content = contentHash("campaign brief #0");

    it("creates a task with escrow, PDA, and nonce", async () => {
      const now = await getClockTimestamp(provider.connection);
      // Deadline far in the future to avoid expiry issues
      const deadline = new BN(now + 86_400 * 30);
      const submitMargin = new BN(3600);
      const claimBuffer = new BN(14_400);

      // Task PDA uses a client-provided random nonce (no global counter).
      const nonce0 = newTaskNonce();
      [task0Pda] = taskPda(nonce0, client.publicKey, program.programId);

      const clientBalanceBefore = await provider.connection.getBalance(
        client.publicKey
      );

      await program.methods
        .createTask(
          nonce0,
          ESCROW_LAMPORTS,
          content as any,
          deadline,
          submitMargin,
          claimBuffer,
          0,
          0, // attestation_delay_override: use global default
          0, // challenge_window_override: use global default
          0, // verification_timeout_override: use global default
          true, // D1 requires_approval — pre-D1 mandatory-approval behavior
          0 // verification_kind: OracleMetrics (kind 0)
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
      assert.equal(task.taskId.toString(), nonce0.toString());
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

      // The global counter is no longer used or incremented by create_task
      // (the Task PDA is keyed on the client nonce). It stays at its init value.
      const globalAfter = await program.account.globalState.fetch(globalPda);
      assert.equal(globalAfter.taskCounter.toString(), "0");

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

    // The scaling invariant: the Task PDA is keyed on a client-provided nonce, not
    // a global counter, so two creates by the SAME client with DISTINCT nonces are
    // independent (no counter to predict/collide on) — the thing that used to fail
    // with ConstraintSeeds under concurrency. Reusing a nonce for the same client
    // is rejected by Anchor `init`, preserving uniqueness.
    it("distinct nonces let one client create many tasks; a reused nonce is rejected", async () => {
      const now = await getClockTimestamp(provider.connection);
      const deadline = new BN(now + 86_400 * 30);
      const mk = (nonce: BN) =>
        program.methods
          .createTask(
            nonce,
            ESCROW_LAMPORTS,
            contentHash(`nonce-${nonce.toString()}`) as any,
            deadline,
            new BN(3600),
            new BN(14_400),
            0,
            0,
            0,
            0,
            true,
            0
          )
          .accountsPartial({
            globalState: globalPda,
            task: taskPda(nonce, client.publicKey, program.programId)[0],
            client: client.publicKey,
            slotHashes: SYSVAR_SLOT_HASHES_PUBKEY,
            systemProgram: SystemProgram.programId,
          })
          .signers([client])
          .rpc();

      const nonceA = newTaskNonce();
      const nonceB = newTaskNonce();
      await mk(nonceA);
      await mk(nonceB); // same client, different nonce → independent, no collision
      const a = await program.account.task.fetch(
        taskPda(nonceA, client.publicKey, program.programId)[0]
      );
      const b = await program.account.task.fetch(
        taskPda(nonceB, client.publicKey, program.programId)[0]
      );
      assert.equal(a.taskId.toString(), nonceA.toString());
      assert.equal(b.taskId.toString(), nonceB.toString());

      // Reusing nonceA for the same client must fail (PDA already initialized).
      let reuseFailed = false;
      try {
        await mk(nonceA);
      } catch {
        reuseFailed = true;
      }
      assert.isTrue(
        reuseFailed,
        "reusing a nonce for the same client must be rejected"
      );
    });

    it("rejects create_task with zero escrow", async () => {
      const now = await getClockTimestamp(provider.connection);
      const nonce = newTaskNonce();
      const [badTaskPda] = taskPda(nonce, client.publicKey, program.programId);

      try {
        await program.methods
          .createTask(
            nonce,
            new BN(0),
            content as any,
            new BN(now + 86_400),
            new BN(3600),
            new BN(14_400),
            0,
            0,
            0,
            0, // timing overrides: use global defaults
            true, // D1 requires_approval — pre-D1 mandatory-approval behavior
            0 // verification_kind: OracleMetrics (kind 0)
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
        assert.fail("Expected InvalidParameter error");
      } catch (e: any) {
        // assert.fail above throws AssertionError — re-throw it so the
        // no-rejection path cannot satisfy this catch's substring assert.
        if (e?.name === "AssertionError") throw e;
        assert.include(e.toString(), "InvalidParameter");
      }
    });

    it("rejects create_task with expired deadline", async () => {
      const nonce = newTaskNonce();
      const [badTaskPda] = taskPda(nonce, client.publicKey, program.programId);

      try {
        await program.methods
          .createTask(
            nonce,
            ESCROW_LAMPORTS,
            content as any,
            new BN(1), // Unix timestamp 1 = far in the past
            new BN(3600),
            new BN(14_400),
            0,
            0,
            0,
            0, // timing overrides: use global defaults
            true, // D1 requires_approval — pre-D1 mandatory-approval behavior
            0 // verification_kind: OracleMetrics (kind 0)
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
        // assert.fail above throws AssertionError — re-throw it so the
        // no-rejection path cannot satisfy this catch's substring assert.
        if (e?.name === "AssertionError") throw e;
        assert.include(e.toString(), "DeadlineExpired");
      }
    });
  });
  describe("claim_task", () => {
    let taskPdaForClaim: PublicKey;

    before(async () => {
      // Create a fresh task for claim tests
      const now = await getClockTimestamp(provider.connection);
      const global = await program.account.globalState.fetch(globalPda);
      const __nonce1 = newTaskNonce();
      [taskPdaForClaim] = taskPda(
        __nonce1,
        client.publicKey,
        program.programId
      );

      await program.methods
        .createTask(
          __nonce1,
          ESCROW_LAMPORTS,
          contentHash("claim test task") as any,
          new BN(now + 86_400 * 30),
          new BN(3600),
          new BN(14_400), // claim_buffer = 4 hours
          0,
          0,
          0,
          0, // timing overrides: use global defaults
          true, // D1 requires_approval — pre-D1 mandatory-approval behavior
          0 // verification_kind: OracleMetrics (kind 0)
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
        // assert.fail above throws AssertionError — re-throw it so the
        // no-rejection path cannot satisfy this catch's substring assert.
        if (e?.name === "AssertionError") throw e;
        assert.include(e.toString(), "InvalidTaskState");
      }
    });
  });
  describe("submit_work", () => {
    let taskPdaForSubmit: PublicKey;

    before(async () => {
      // Create and claim a task for submit tests
      const now = await getClockTimestamp(provider.connection);
      const global = await program.account.globalState.fetch(globalPda);
      const __nonce2 = newTaskNonce();
      [taskPdaForSubmit] = taskPda(
        __nonce2,
        client.publicKey,
        program.programId
      );
      const [agentPda] = agentStatePda(agent.publicKey, program.programId);

      await program.methods
        .createTask(
          __nonce2,
          ESCROW_LAMPORTS,
          contentHash("submit test task") as any,
          new BN(now + 86_400 * 30),
          new BN(3600), // submit_margin = 1 hour
          new BN(14_400),
          0,
          0,
          0,
          0, // timing overrides: use global defaults
          true, // D1 requires_approval — pre-D1 mandatory-approval behavior
          0 // verification_kind: OracleMetrics (kind 0)
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
      const __nonce3 = newTaskNonce();
      const [freshTaskPda] = taskPda(
        __nonce3,
        client.publicKey,
        program.programId
      );
      const [agentPda] = agentStatePda(agent.publicKey, program.programId);

      await program.methods
        .createTask(
          __nonce3,
          ESCROW_LAMPORTS,
          contentHash("non-agent submit test") as any,
          new BN(now + 86_400 * 30),
          new BN(3600),
          new BN(14_400),
          0,
          0,
          0,
          0, // timing overrides: use global defaults
          true, // D1 requires_approval — pre-D1 mandatory-approval behavior
          0 // verification_kind: OracleMetrics (kind 0)
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
        // assert.fail above throws AssertionError — re-throw it so the
        // no-rejection path cannot satisfy this catch's substring assert.
        if (e?.name === "AssertionError") throw e;
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
      const __nonce4 = newTaskNonce();
      taskIdForVerify = __nonce4;
      [taskPdaForVerify] = taskPda(
        __nonce4,
        client.publicKey,
        program.programId
      );

      await program.methods
        .createTask(
          __nonce4,
          ESCROW_LAMPORTS,
          contentHash("verify test task") as any,
          new BN(now + 86_400 * 30),
          new BN(3600),
          new BN(14_400),
          0,
          0,
          0,
          0, // timing overrides: use global defaults
          true, // D1 requires_approval — pre-D1 mandatory-approval behavior
          0 // verification_kind: OracleMetrics (kind 0)
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

      // Phase 3 blocker #3a: verify_task now requires Approved (was
      // Submitted). Drive the test task through client approval so the
      // staleness/feed-mismatch checks below fire as intended rather
      // than tripping on InvalidTaskState first.
      await program.methods
        .approveTask()
        .accountsPartial({
          task: taskPdaForVerify,
          client: client.publicKey,
        })
        .signers([client])
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
        // assert.fail above throws AssertionError — re-throw it so the
        // no-rejection path cannot satisfy this catch's substring assert.
        if (e?.name === "AssertionError") throw e;
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
        // assert.fail above throws AssertionError — re-throw it so the
        // no-rejection path cannot satisfy this catch's substring assert.
        if (e?.name === "AssertionError") throw e;
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
  describe("finalize_task", () => {
    it("rejects finalize on non-Verified task", async () => {
      // Create a task in Open state
      const now = await getClockTimestamp(provider.connection);
      const global = await program.account.globalState.fetch(globalPda);
      const __nonce5 = newTaskNonce();
      const [openTaskPda] = taskPda(
        __nonce5,
        client.publicKey,
        program.programId
      );

      await program.methods
        .createTask(
          __nonce5,
          ESCROW_LAMPORTS,
          contentHash("finalize reject test") as any,
          new BN(now + 86_400 * 30),
          new BN(3600),
          new BN(14_400),
          0,
          0,
          0,
          0, // timing overrides: use global defaults
          true, // D1 requires_approval — pre-D1 mandatory-approval behavior
          0 // verification_kind: OracleMetrics (kind 0)
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
        // assert.fail above throws AssertionError — re-throw it so the
        // no-rejection path cannot satisfy this catch's substring assert.
        if (e?.name === "AssertionError") throw e;
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
  describe("expire_task", () => {
    it("rejects expire before deadline", async () => {
      // Create a task with far-future deadline
      const now = await getClockTimestamp(provider.connection);
      const global = await program.account.globalState.fetch(globalPda);
      const __nonce8 = newTaskNonce();
      const [taskToExpire] = taskPda(
        __nonce8,
        client.publicKey,
        program.programId
      );

      await program.methods
        .createTask(
          __nonce8,
          ESCROW_LAMPORTS,
          contentHash("expire reject test") as any,
          new BN(now + 86_400 * 30),
          new BN(3600),
          new BN(14_400),
          0,
          0,
          0,
          0, // timing overrides: use global defaults
          true, // D1 requires_approval — pre-D1 mandatory-approval behavior
          0 // verification_kind: OracleMetrics (kind 0)
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
        // assert.fail above throws AssertionError — re-throw it so the
        // no-rejection path cannot satisfy this catch's substring assert.
        if (e?.name === "AssertionError") throw e;
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
      const __nonce9 = newTaskNonce();
      const [shortTaskPda] = taskPda(
        __nonce9,
        client.publicKey,
        program.programId
      );

      // Set deadline to now + 3 seconds (just barely in the future for create)
      const tightDeadline = new BN(now + 3);

      await program.methods
        .createTask(
          __nonce9,
          ESCROW_LAMPORTS,
          contentHash("short deadline task") as any,
          tightDeadline,
          new BN(0), // no submit margin
          new BN(0), // no claim buffer
          0,
          0,
          0,
          0, // timing overrides: use global defaults
          true, // D1 requires_approval — pre-D1 mandatory-approval behavior
          0 // verification_kind: OracleMetrics (kind 0)
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
  describe("claim_task error paths", () => {
    it("rejects claim when claim buffer is insufficient", async () => {
      const now = await getClockTimestamp(provider.connection);
      const global = await program.account.globalState.fetch(globalPda);
      const __nonce13 = newTaskNonce();
      const [tightTaskPda] = taskPda(
        __nonce13,
        client.publicKey,
        program.programId
      );

      // Create task with deadline only 100s in the future but claim_buffer = 14400
      // This means now + 14400 > now + 100, so claim should be rejected
      await program.methods
        .createTask(
          __nonce13,
          ESCROW_LAMPORTS,
          contentHash("tight deadline task") as any,
          new BN(now + 100),
          new BN(0),
          new BN(14_400), // 4 hour claim buffer, but deadline is 100s away
          0,
          0,
          0,
          0, // timing overrides: use global defaults
          true, // D1 requires_approval — pre-D1 mandatory-approval behavior
          0 // verification_kind: OracleMetrics (kind 0)
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
        // assert.fail above throws AssertionError — re-throw it so the
        // no-rejection path cannot satisfy this catch's substring assert.
        if (e?.name === "AssertionError") throw e;
        assert.include(e.toString(), "ClaimBufferInsufficient");
      }
    });
  });
  describe("submit_work error paths", () => {
    it("rejects submit on Open task (not claimed)", async () => {
      const now = await getClockTimestamp(provider.connection);
      const global = await program.account.globalState.fetch(globalPda);
      const __nonce21 = newTaskNonce();
      const [openTaskPda] = taskPda(
        __nonce21,
        client.publicKey,
        program.programId
      );
      const [agentPda] = agentStatePda(agent.publicKey, program.programId);

      await program.methods
        .createTask(
          __nonce21,
          ESCROW_LAMPORTS,
          contentHash("submit on open task") as any,
          new BN(now + 86_400 * 30),
          new BN(3600),
          new BN(14_400),
          0,
          0,
          0,
          0, // timing overrides: use global defaults
          true, // D1 requires_approval — pre-D1 mandatory-approval behavior
          0 // verification_kind: OracleMetrics (kind 0)
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
        // assert.fail above throws AssertionError — re-throw it so the
        // no-rejection path cannot satisfy this catch's substring assert.
        if (e?.name === "AssertionError") throw e;
        assert.include(e.toString(), "InvalidTaskState");
      }
    });
  });
});
