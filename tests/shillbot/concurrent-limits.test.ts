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

describe("shillbot - concurrent-limits", () => {
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
        const __nonce10 = newTaskNonce();
        const [tp] = taskPda(__nonce10, client.publicKey, program.programId);

        await program.methods
          .createTask(
            __nonce10,
            ESCROW_LAMPORTS,
            contentHash(`concurrent task ${i}`) as any,
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
      const __nonce11 = newTaskNonce();
      const [tp] = taskPda(__nonce11, client.publicKey, program.programId);
      const [claimAgentPda] = agentStatePda(
        claimAgent.publicKey,
        program.programId
      );

      await program.methods
        .createTask(
          __nonce11,
          ESCROW_LAMPORTS,
          contentHash("concurrent task 4") as any,
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
      const __nonce12 = newTaskNonce();
      const [tp] = taskPda(__nonce12, client.publicKey, program.programId);
      const [claimAgentPda] = agentStatePda(
        claimAgent.publicKey,
        program.programId
      );

      await program.methods
        .createTask(
          __nonce12,
          ESCROW_LAMPORTS,
          contentHash("concurrent task 5 overflow") as any,
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
        // assert.fail above throws AssertionError — re-throw it so the
        // no-rejection path cannot satisfy this catch's substring assert.
        if (e?.name === "AssertionError") throw e;
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
});
