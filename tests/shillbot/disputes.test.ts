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

describe("shillbot - disputes", () => {
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
  describe("challenge_task", () => {
    it("rejects challenge on non-Verified task", async () => {
      // Create a task in Open state
      const now = await getClockTimestamp(provider.connection);
      const global = await program.account.globalState.fetch(globalPda);
      const __nonce6 = newTaskNonce();
      const [openTaskPda] = taskPda(
        __nonce6,
        client.publicKey,
        program.programId
      );

      await program.methods
        .createTask(
          __nonce6,
          ESCROW_LAMPORTS,
          contentHash("challenge reject test") as any,
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
        // assert.fail above throws AssertionError — re-throw it so the
        // no-rejection path cannot satisfy this catch's substring assert.
        if (e?.name === "AssertionError") throw e;
        assert.include(e.toString(), "InvalidTaskState");
      }
    });
  });
  describe("resolve_challenge", () => {
    it("rejects resolve on non-Disputed task", async () => {
      // Use an Open task to verify the state check
      const now = await getClockTimestamp(provider.connection);
      const global = await program.account.globalState.fetch(globalPda);
      const __nonce7 = newTaskNonce();
      const [openTaskPda] = taskPda(
        __nonce7,
        client.publicKey,
        program.programId
      );

      await program.methods
        .createTask(
          __nonce7,
          ESCROW_LAMPORTS,
          contentHash("resolve reject test") as any,
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
        // assert.fail above throws AssertionError — re-throw it so the
        // no-rejection path cannot satisfy this catch's substring assert.
        if (e?.name === "AssertionError") throw e;
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
  describe("emergency_return", () => {
    it("returns escrow for two Open tasks", async () => {
      const now = await getClockTimestamp(provider.connection);
      const taskPdas: PublicKey[] = [];

      // Create 2 open tasks
      for (let i = 0; i < 2; i++) {
        const global = await program.account.globalState.fetch(globalPda);
        const __nonce14 = newTaskNonce();
        const [tp] = taskPda(__nonce14, client.publicKey, program.programId);

        await program.methods
          .createTask(
            __nonce14,
            ESCROW_LAMPORTS,
            contentHash(`emergency open task ${i}`) as any,
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
      const __nonce15 = newTaskNonce();
      const [tp] = taskPda(__nonce15, client.publicKey, program.programId);
      const [emergencyAgentPda] = agentStatePda(
        emergencyAgent.publicKey,
        program.programId
      );

      await program.methods
        .createTask(
          __nonce15,
          ESCROW_LAMPORTS,
          contentHash("emergency claimed task") as any,
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
      const __nonce16 = newTaskNonce();
      const [tp] = taskPda(__nonce16, client.publicKey, program.programId);

      await program.methods
        .createTask(
          __nonce16,
          ESCROW_LAMPORTS,
          contentHash("emergency auth test") as any,
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
        // assert.fail above throws AssertionError — re-throw it so the
        // no-rejection path cannot satisfy this catch's substring assert.
        if (e?.name === "AssertionError") throw e;
        assert.include(e.toString(), "NotAuthority");
      }
    });

    it("rejects Submitted task (wrong state)", async () => {
      const now = await getClockTimestamp(provider.connection);
      const global = await program.account.globalState.fetch(globalPda);
      const __nonce17 = newTaskNonce();
      const [tp] = taskPda(__nonce17, client.publicKey, program.programId);
      const [agentPda] = agentStatePda(agent.publicKey, program.programId);

      await program.methods
        .createTask(
          __nonce17,
          ESCROW_LAMPORTS,
          contentHash("emergency submitted test") as any,
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
        // assert.fail above throws AssertionError — re-throw it so the
        // no-rejection path cannot satisfy this catch's substring assert.
        if (e?.name === "AssertionError") throw e;
        assert.include(e.toString(), "InvalidTaskState");
      }
    });
  });
});
