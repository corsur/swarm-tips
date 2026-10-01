import * as anchor from "@coral-xyz/anchor";
import { Program, BN } from "@coral-xyz/anchor";
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

describe("shillbot - session-delegates", () => {
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
        // assert.fail above throws AssertionError — re-throw it so the
        // no-rejection path cannot satisfy this catch's substring assert.
        if (e?.name === "AssertionError") throw e;
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

      // NB: assert.fail must not run inside the try — its own message would
      // satisfy the catch's substring assert and the test could never fail.
      let rejected = false;
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
      } catch (e: any) {
        rejected = true;
        assert.include(e.toString(), "InvalidSessionDelegate");
      }
      assert.isTrue(rejected, "createSession(0x04) must be rejected");
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
        // assert.fail above throws AssertionError — re-throw it so the
        // no-rejection path cannot satisfy this catch's substring assert.
        if (e?.name === "AssertionError") throw e;
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
  describe("session-delegated handlers", () => {
    const sessionAgent = Keypair.generate();
    const fullDelegate = Keypair.generate();
    const claimOnlyDelegate = Keypair.generate();
    let fullSessionPda: PublicKey;
    let claimOnlySessionPda: PublicKey;
    let sessionTaskPda: PublicKey;

    before(async () => {
      // Fresh agent for these tests — keeps the existing `agent` from the
      // earlier "claim_task" / "submit_work" describe blocks isolated.
      await airdrop(
        provider.connection,
        sessionAgent.publicKey,
        2 * LAMPORTS_PER_SOL
      );
      await airdrop(
        provider.connection,
        fullDelegate.publicKey,
        LAMPORTS_PER_SOL
      );
      await airdrop(
        provider.connection,
        claimOnlyDelegate.publicKey,
        LAMPORTS_PER_SOL
      );

      // 0x03 = claim + submit permissions
      [fullSessionPda] = sessionPda(
        sessionAgent.publicKey,
        fullDelegate.publicKey,
        program.programId
      );
      await program.methods
        .createSession(0x03, new BN(86_400))
        .accountsPartial({
          sessionDelegate: fullSessionPda,
          agent: sessionAgent.publicKey,
          delegate: fullDelegate.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([sessionAgent])
        .rpc();

      // 0x01 = claim ONLY (used to verify submit_work_session bitmask check)
      [claimOnlySessionPda] = sessionPda(
        sessionAgent.publicKey,
        claimOnlyDelegate.publicKey,
        program.programId
      );
      await program.methods
        .createSession(0x01, new BN(86_400))
        .accountsPartial({
          sessionDelegate: claimOnlySessionPda,
          agent: sessionAgent.publicKey,
          delegate: claimOnlyDelegate.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([sessionAgent])
        .rpc();
    });

    it("claim_task_session: delegate claims a task on behalf of the agent", async () => {
      const now = await getClockTimestamp(provider.connection);
      const global = await program.account.globalState.fetch(globalPda);
      const __nonce18 = newTaskNonce();
      const [openTaskPda] = taskPda(
        __nonce18,
        client.publicKey,
        program.programId
      );

      await program.methods
        .createTask(
          __nonce18,
          ESCROW_LAMPORTS,
          contentHash("session claim happy") as any,
          new BN(now + 86_400 * 30),
          new BN(3600),
          new BN(14_400),
          0,
          0,
          0,
          0,
          true,
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

      const [agentPda] = agentStatePda(
        sessionAgent.publicKey,
        program.programId
      );

      await program.methods
        .claimTaskSession()
        .accountsPartial({
          task: openTaskPda,
          globalState: globalPda,
          agentState: agentPda,
          sessionDelegate: fullSessionPda,
          delegate: fullDelegate.publicKey,
          payer: fullDelegate.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([fullDelegate])
        .rpc();

      const task = await program.account.task.fetch(openTaskPda);
      assert.deepEqual(task.state, { claimed: {} });
      assert.equal(
        task.agent.toString(),
        sessionAgent.publicKey.toString(),
        "task.agent must be the agent (not the delegate)"
      );

      const agentState = await program.account.agentState.fetch(agentPda);
      assert.equal(
        agentState.agent.toString(),
        sessionAgent.publicKey.toString()
      );
      assert.isAtLeast(agentState.claimedCount, 1);

      sessionTaskPda = openTaskPda;
    });

    it("claim_task_session: rejects delegate without claim permission", async () => {
      // claimOnlyDelegate has 0x01 (claim) — to test the rejection we need a
      // delegate whose bitmask LACKS bit 0. Make a submit-only delegate.
      const submitOnlyDelegate = Keypair.generate();
      await airdrop(
        provider.connection,
        submitOnlyDelegate.publicKey,
        LAMPORTS_PER_SOL
      );
      const [submitOnlySessionPda] = sessionPda(
        sessionAgent.publicKey,
        submitOnlyDelegate.publicKey,
        program.programId
      );
      await program.methods
        .createSession(0x02, new BN(86_400))
        .accountsPartial({
          sessionDelegate: submitOnlySessionPda,
          agent: sessionAgent.publicKey,
          delegate: submitOnlyDelegate.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([sessionAgent])
        .rpc();

      const now = await getClockTimestamp(provider.connection);
      const global = await program.account.globalState.fetch(globalPda);
      const __nonce19 = newTaskNonce();
      const [taskForRejection] = taskPda(
        __nonce19,
        client.publicKey,
        program.programId
      );

      await program.methods
        .createTask(
          __nonce19,
          ESCROW_LAMPORTS,
          contentHash("session claim no-perm") as any,
          new BN(now + 86_400 * 30),
          new BN(3600),
          new BN(14_400),
          0,
          0,
          0,
          0,
          true,
          0 // verification_kind: OracleMetrics (kind 0)
        )
        .accountsPartial({
          globalState: globalPda,
          task: taskForRejection,
          client: client.publicKey,
          slotHashes: SYSVAR_SLOT_HASHES_PUBKEY,
          systemProgram: SystemProgram.programId,
        })
        .signers([client])
        .rpc();

      const [agentPda] = agentStatePda(
        sessionAgent.publicKey,
        program.programId
      );

      try {
        await program.methods
          .claimTaskSession()
          .accountsPartial({
            task: taskForRejection,
            globalState: globalPda,
            agentState: agentPda,
            sessionDelegate: submitOnlySessionPda,
            delegate: submitOnlyDelegate.publicKey,
            payer: submitOnlyDelegate.publicKey,
            systemProgram: SystemProgram.programId,
          })
          .signers([submitOnlyDelegate])
          .rpc();
        assert.fail("Expected InvalidSessionDelegate");
      } catch (e: any) {
        // assert.fail above throws AssertionError — re-throw it so the
        // no-rejection path cannot satisfy this catch's substring assert.
        if (e?.name === "AssertionError") throw e;
        assert.include(e.toString(), "InvalidSessionDelegate");
      }
    });

    it("submit_work_session: delegate submits work on behalf of the agent", async () => {
      // Reuses the task claimed in the first test (sessionTaskPda).
      assert.isOk(
        sessionTaskPda,
        "previous claim_task_session test must have run first"
      );
      const [agentPda] = agentStatePda(
        sessionAgent.publicKey,
        program.programId
      );

      await program.methods
        .submitWorkSession(Buffer.from("video-id-session-test"))
        .accountsPartial({
          task: sessionTaskPda,
          globalState: globalPda,
          agentState: agentPda,
          sessionDelegate: fullSessionPda,
          delegate: fullDelegate.publicKey,
        })
        .signers([fullDelegate])
        .rpc();

      const task = await program.account.task.fetch(sessionTaskPda);
      assert.deepEqual(task.state, { submitted: {} });
      assert.isTrue(
        task.submittedAt.toNumber() > 0,
        "submitted_at should be set"
      );
    });

    it("submit_work_session: rejects delegate without submit permission", async () => {
      // claimOnlyDelegate has bitmask 0x01 (claim, no submit). Reuse the
      // claim-only delegate to claim+submit and expect submit rejection.
      const now = await getClockTimestamp(provider.connection);
      const global = await program.account.globalState.fetch(globalPda);
      const __nonce20 = newTaskNonce();
      const [taskForClaimOnly] = taskPda(
        __nonce20,
        client.publicKey,
        program.programId
      );

      await program.methods
        .createTask(
          __nonce20,
          ESCROW_LAMPORTS,
          contentHash("session submit no-perm") as any,
          new BN(now + 86_400 * 30),
          new BN(3600),
          new BN(14_400),
          0,
          0,
          0,
          0,
          true,
          0 // verification_kind: OracleMetrics (kind 0)
        )
        .accountsPartial({
          globalState: globalPda,
          task: taskForClaimOnly,
          client: client.publicKey,
          slotHashes: SYSVAR_SLOT_HASHES_PUBKEY,
          systemProgram: SystemProgram.programId,
        })
        .signers([client])
        .rpc();

      const [agentPda] = agentStatePda(
        sessionAgent.publicKey,
        program.programId
      );

      // Claim with claim-only delegate (passes bit 0).
      await program.methods
        .claimTaskSession()
        .accountsPartial({
          task: taskForClaimOnly,
          globalState: globalPda,
          agentState: agentPda,
          sessionDelegate: claimOnlySessionPda,
          delegate: claimOnlyDelegate.publicKey,
          payer: claimOnlyDelegate.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([claimOnlyDelegate])
        .rpc();

      // Submit with claim-only delegate — should fail because bit 1 is unset.
      try {
        await program.methods
          .submitWorkSession(Buffer.from("video-id-no-perm"))
          .accountsPartial({
            task: taskForClaimOnly,
            globalState: globalPda,
            agentState: agentPda,
            sessionDelegate: claimOnlySessionPda,
            delegate: claimOnlyDelegate.publicKey,
          })
          .signers([claimOnlyDelegate])
          .rpc();
        assert.fail("Expected InvalidSessionDelegate");
      } catch (e: any) {
        // assert.fail above throws AssertionError — re-throw it so the
        // no-rejection path cannot satisfy this catch's substring assert.
        if (e?.name === "AssertionError") throw e;
        assert.include(e.toString(), "InvalidSessionDelegate");
      }
    });
  });
});
