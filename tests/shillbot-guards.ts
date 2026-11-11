// Shillbot discrete guard/rejection suite (bankrun) — the cases that are NOT
// cells: create-time validation rejections and the timed-gate dead-second
// batteries. These stay discrete (not in the combinatorial matrix) the same way
// the game keeps its timeout-harness discrete — an illegal transition has no
// "payout" to verify, only a rejection to assert.
//
// The three timed gates are driven through the SHARED boundary battery
// (tests/harness/shillbot-boundary.ts), the TS twin of BoundaryBattery.sol:
// each is probed at deadline−1 / deadline / deadline+1 against a fresh subject.
// Runs under node 20 (harness README gating note).

import { BN, BorshCoder, EventParser } from "@coral-xyz/anchor";
import {
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  SystemProgram,
  SYSVAR_INSTRUCTIONS_PUBKEY,
  Transaction,
  TransactionInstruction,
} from "@solana/web3.js";
import { assert } from "chai";

import {
  CPI_CLAIM_FORWARDER_ID,
  startShillbotBankrun,
} from "./harness/shillbot-bankrun";
import {
  ShillbotCtx,
  setupShillbot,
  createTask,
  claimTask,
  claimAndSubmitAtomic,
  submitWork,
  verifyTaskOracle,
  verifyTaskAttested,
  challengeTask,
  challengePda,
  agentStatePda,
  finalizeTask,
  resolveChallengeDefault,
  challengeDeadline,
  DUMMY_SWITCHBOARD_FEED,
  LEAN_PROOF_PLATFORM,
  ONCHAIN_ACTION_PLATFORM,
  CreateTaskOpts,
} from "./harness/shillbot-steps";
import {
  assertLiveStrictlyBefore,
  assertLiveStrictlyAfter,
} from "./harness/shillbot-boundary";

const ESCROW = new BN(1 * LAMPORTS_PER_SOL);
const MAX_SCORE = 1_000_000;
const CHALLENGE_WINDOW = 3600;

async function expectErr(p: Promise<unknown>, pattern: RegExp): Promise<void> {
  try {
    await p;
    assert.fail(`expected ${pattern}, got success`);
  } catch (e: unknown) {
    assert.match(String(e), pattern);
  }
}

describe("shillbot-guards (bankrun, discrete)", () => {
  let handle: Awaited<ReturnType<typeof startShillbotBankrun>>;
  let ctx: ShillbotCtx;
  const authority = Keypair.generate();
  const treasury = Keypair.generate();

  before(async function () {
    this.timeout(120_000);
    handle = await startShillbotBankrun();
    for (const kp of [authority, treasury]) {
      await handle.runtime.fund(kp.publicKey, 100 * LAMPORTS_PER_SOL);
    }
    ctx = await setupShillbot(handle.runtime, {
      authority,
      treasury: treasury.publicKey,
      primeFeed: handle.primeFeed,
    });
  });

  async function freshClient(): Promise<Keypair> {
    const kp = Keypair.generate();
    await handle.runtime.fund(kp.publicKey, 100 * LAMPORTS_PER_SOL);
    return kp;
  }

  // --- create-time verification-kind validation --------------------------

  describe("create_task verification-kind validation", () => {
    async function create(opts: Partial<CreateTaskOpts>): Promise<void> {
      const client = await freshClient();
      await createTask(ctx, client, {
        escrowLamports: ESCROW,
        platform: LEAN_PROOF_PLATFORM,
        verificationKind: 1,
        requiresApproval: false,
        challengeWindowOverride: CHALLENGE_WINDOW,
        ...opts,
      });
    }

    it("rejects an unknown kind (2)", async () => {
      await expectErr(
        create({ verificationKind: 2 }),
        /InvalidVerificationKind/
      );
    });

    it("rejects kind 1 on a non-LeanProof platform", async () => {
      await expectErr(
        create({ verificationKind: 1, platform: 9 }),
        /VerificationKindMismatch/
      );
    });

    it("rejects kind 0 on the LeanProof platform", async () => {
      await expectErr(
        create({ verificationKind: 0, platform: LEAN_PROOF_PLATFORM }),
        /VerificationKindMismatch/
      );
    });
  });

  // --- claim / verify guards --------------------------------------------

  describe("claim + verify guards", () => {
    it("allows the client to self-claim their own deterministic task (guard removed)", async () => {
      // The kind-1 self-claim guard was removed — it only blocked same-wallet
      // credential wash, which a two-wallet split bypasses, so it guarded
      // nothing real. Self-claiming a deterministic task must now succeed.
      const client = await freshClient();
      const { task } = await createTask(ctx, client, {
        escrowLamports: ESCROW,
        platform: LEAN_PROOF_PLATFORM,
        verificationKind: 1,
        requiresApproval: false,
        challengeWindowOverride: CHALLENGE_WINDOW,
      });
      await claimTask(ctx, client, task); // no longer throws SelfClaimForbidden
    });

    it("verify_task_attested rejects a non-binary score", async () => {
      const client = await freshClient();
      const agent = await freshClient();
      const { task, taskId } = await createTask(ctx, client, {
        escrowLamports: ESCROW,
        platform: LEAN_PROOF_PLATFORM,
        verificationKind: 1,
        requiresApproval: false,
        challengeWindowOverride: CHALLENGE_WINDOW,
      });
      await claimTask(ctx, agent, task);
      await submitWork(ctx, agent, task, "proof.lean");
      await expectErr(
        verifyTaskAttested(ctx, task, taskId, 500_000),
        /AttestedScoreNotBinary/
      );
    });

    it("verify_task_attested rejects a signer that is not the oracle_authority", async () => {
      const client = await freshClient();
      const agent = await freshClient();
      const imposter = await freshClient();
      const { task, taskId } = await createTask(ctx, client, {
        escrowLamports: ESCROW,
        platform: LEAN_PROOF_PLATFORM,
        verificationKind: 1,
        requiresApproval: false,
        challengeWindowOverride: CHALLENGE_WINDOW,
      });
      await claimTask(ctx, agent, task);
      await submitWork(ctx, agent, task, "proof.lean");
      await expectErr(
        verifyTaskAttested(ctx, task, taskId, MAX_SCORE, imposter),
        /OracleAuthorityMismatch/
      );
    });
  });

  // --- timed-gate dead-second batteries (shared boundary battery) --------

  interface Verified {
    task: PublicKey;
    taskId: BN;
    agent: Keypair;
    client: Keypair;
    challenger: Keypair;
  }

  async function freshVerified(): Promise<Verified> {
    const client = await freshClient();
    const agent = await freshClient();
    const challenger = await freshClient();
    const { task, taskId } = await createTask(ctx, client, {
      escrowLamports: ESCROW,
      platform: LEAN_PROOF_PLATFORM,
      verificationKind: 1,
      requiresApproval: false,
      challengeWindowOverride: CHALLENGE_WINDOW,
    });
    await claimTask(ctx, agent, task);
    await submitWork(ctx, agent, task, "proof.lean");
    await verifyTaskAttested(ctx, task, taskId, MAX_SCORE);
    return { task, taskId, agent, client, challenger };
  }

  it("challenge_task is live strictly before the challenge deadline", async () => {
    await assertLiveStrictlyBefore<Verified>({
      fresh: async () => {
        const v = await freshVerified();
        return { subject: v, deadline: await challengeDeadline(ctx, v.task) };
      },
      warpTo: (ts) => handle.runtime.warpTo(ts),
      action: async (v) => {
        await challengeTask(ctx, v.challenger, v.task, v.taskId);
      },
      deadError: /ChallengeWindowClosed/,
    });
  });

  it("finalize_task is live strictly after the challenge deadline", async () => {
    await assertLiveStrictlyAfter<Verified>({
      fresh: async () => {
        const v = await freshVerified();
        return { subject: v, deadline: await challengeDeadline(ctx, v.task) };
      },
      warpTo: (ts) => handle.runtime.warpTo(ts),
      action: (v) =>
        finalizeTask(ctx, v.task, v.agent.publicKey, v.client.publicKey),
      deadError: /ChallengeWindowOpen/,
    });
  });

  it("resolve_challenge_default is live strictly after the dispute window", async () => {
    await assertLiveStrictlyAfter<Verified>({
      fresh: async () => {
        const v = await freshVerified();
        const challenge = await challengeTask(
          ctx,
          v.challenger,
          v.task,
          v.taskId
        );
        const ch = await ctx.rt.program.account.challenge.fetch(challenge);
        const g = await ctx.rt.program.account.globalState.fetch(ctx.globalPda);
        const deadline =
          ch.createdAt.toNumber() + g.disputeResolutionWindowSeconds.toNumber();
        return { subject: v, deadline };
      },
      warpTo: (ts) => handle.runtime.warpTo(ts),
      action: (v) =>
        resolveChallengeDefault(
          ctx,
          v.task,
          challengePda(
            v.taskId,
            v.challenger.publicKey,
            ctx.rt.program.programId
          ),
          v.agent.publicKey,
          v.client.publicKey,
          v.challenger.publicKey
        ),
      deadError: /DisputeWindowStillOpen/,
    });
  });

  it("agentStatePda derives deterministically (shared helper smoke)", () => {
    const a = Keypair.generate();
    const pda1 = agentStatePda(a.publicKey, ctx.rt.program.programId);
    const pda2 = agentStatePda(a.publicKey, ctx.rt.program.programId);
    assert.equal(pda1.toBase58(), pda2.toBase58());
  });

  // --- OnChainAction (platform = 11), Switchboard retirement & S02-F01 -----

  describe("OnChainAction (platform 11) + co-signed oracle + S02-F01 guards", () => {
    it("creates OnChainAction (kind=0, 60s delay, 900s challenge) and rejects kind=1, zero content_hash, and requires_approval=true", async () => {
      const client = await freshClient();
      const { task } = await createTask(ctx, client, {
        escrowLamports: ESCROW,
        platform: ONCHAIN_ACTION_PLATFORM,
        verificationKind: 0,
        requiresApproval: false,
        attestationDelayOverride: 60,
        challengeWindowOverride: 900,
      });
      const fetched = await ctx.rt.program.account.task.fetch(task);
      assert.equal(fetched.platform, ONCHAIN_ACTION_PLATFORM);
      assert.equal(fetched.attestationDelayOverride, 60);
      assert.equal(fetched.challengeWindowOverride, 900);

      // Non-OnChainAction platforms MUST keep the 3600s floor on both overrides.
      await expectErr(
        createTask(ctx, client, {
          escrowLamports: ESCROW,
          platform: 0,
          verificationKind: 0,
          requiresApproval: false,
          attestationDelayOverride: 60,
        }),
        /TimingOverrideOutOfBounds/
      );
      await expectErr(
        createTask(ctx, client, {
          escrowLamports: ESCROW,
          platform: 0,
          verificationKind: 0,
          requiresApproval: false,
          challengeWindowOverride: 900,
        }),
        /TimingOverrideOutOfBounds/
      );

      await expectErr(
        createTask(ctx, client, {
          escrowLamports: ESCROW,
          platform: ONCHAIN_ACTION_PLATFORM,
          verificationKind: 1,
          requiresApproval: false,
        }),
        /VerificationKindMismatch/
      );
      await expectErr(
        createTask(ctx, client, {
          escrowLamports: ESCROW,
          platform: ONCHAIN_ACTION_PLATFORM,
          verificationKind: 0,
          requiresApproval: false,
          contentHashOverride: new Array(32).fill(0),
        }),
        /InvalidParameter/
      );
      await expectErr(
        createTask(ctx, client, {
          escrowLamports: ESCROW,
          platform: ONCHAIN_ACTION_PLATFORM,
          verificationKind: 0,
          requiresApproval: true,
        }),
        /InvalidParameter/
      );
    });

    it("enforces M1 instruction-bundle invariants across bare, wrong-sysvar, wrong-task, and session paths", async () => {
      const client = await freshClient();
      const agent = await freshClient();
      const delegate = await freshClient();
      const { task } = await createTask(ctx, client, {
        escrowLamports: ESCROW,
        platform: ONCHAIN_ACTION_PLATFORM,
        verificationKind: 0,
        requiresApproval: false,
        attestationDelayOverride: 60,
        challengeWindowOverride: 900,
      });
      const { task: otherTask } = await createTask(ctx, client, {
        escrowLamports: ESCROW,
        platform: 0,
        verificationKind: 0,
        requiresApproval: false,
      });
      await claimTask(ctx, agent, otherTask);

      // (a1) Bare claim_task (no sysvar).
      await expectErr(
        claimTask(ctx, agent, task),
        /InvalidInstructionBundle|6053/
      );

      // (a2) Wrong remainingAccounts[0] key (SystemProgram instead of SYSVAR_INSTRUCTIONS).
      const wrongSysvarClaimIx = await ctx.rt.program.methods
        .claimTask()
        .accountsPartial({
          task,
          globalState: ctx.globalPda,
          agentState: agentStatePda(agent.publicKey, ctx.rt.program.programId),
          agent: agent.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .remainingAccounts([
          {
            pubkey: SystemProgram.programId,
            isSigner: false,
            isWritable: false,
          },
        ])
        .instruction();
      const validSubmitForTaskIx = await ctx.rt.program.methods
        .submitWork(Buffer.from("eip155:56:0x" + "aa".repeat(32) + ":0"))
        .accountsPartial({
          task,
          globalState: ctx.globalPda,
          agentState: agentStatePda(agent.publicKey, ctx.rt.program.programId),
          agent: agent.publicKey,
        })
        .instruction();
      await expectErr(
        ctx.rt.program.provider.sendAndConfirm!(
          new Transaction().add(wrongSysvarClaimIx, validSubmitForTaskIx),
          [agent]
        ),
        /InvalidInstructionBundle|6053/
      );

      // (a3) Correct sysvar, but companion submit_work targets a DIFFERENT task!
      const sysvarClaimIx = await ctx.rt.program.methods
        .claimTask()
        .accountsPartial({
          task,
          globalState: ctx.globalPda,
          agentState: agentStatePda(agent.publicKey, ctx.rt.program.programId),
          agent: agent.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .remainingAccounts([
          {
            pubkey: SYSVAR_INSTRUCTIONS_PUBKEY,
            isSigner: false,
            isWritable: false,
          },
        ])
        .instruction();
      const wrongTaskSubmitIx = await ctx.rt.program.methods
        .submitWork(Buffer.from("https://x.com/test/status/1"))
        .accountsPartial({
          task: otherTask,
          globalState: ctx.globalPda,
          agentState: agentStatePda(agent.publicKey, ctx.rt.program.programId),
          agent: agent.publicKey,
        })
        .instruction();
      await expectErr(
        ctx.rt.program.provider.sendAndConfirm!(
          new Transaction().add(sysvarClaimIx, wrongTaskSubmitIx),
          [agent]
        ),
        /InvalidInstructionBundle|6053/
      );

      // (c) CPI-wrapped claim_task via cpi_claim_forwarder (stack_height == 2) MUST be rejected
      // even when followed by a valid top-level submit_work at ix[1]!
      const cpiWrappedClaimIx = new TransactionInstruction({
        programId: CPI_CLAIM_FORWARDER_ID,
        keys: [
          {
            pubkey: ctx.rt.program.programId,
            isSigner: false,
            isWritable: false,
          },
          ...sysvarClaimIx.keys,
        ],
        data: sysvarClaimIx.data,
      });
      await expectErr(
        ctx.rt.program.provider.sendAndConfirm!(
          new Transaction().add(cpiWrappedClaimIx, validSubmitForTaskIx),
          [agent]
        ),
        /InvalidInstructionBundle|6053/
      );

      // (d) Session path: bare claim_task_session fails, atomic [claim_task_session, submit_work_session] succeeds!
      const [sessionPda] = PublicKey.findProgramAddressSync(
        [
          Buffer.from("session"),
          agent.publicKey.toBuffer(),
          delegate.publicKey.toBuffer(),
        ],
        ctx.rt.program.programId
      );
      await ctx.rt.program.methods
        .createSession(0x03, new BN(3600))
        .accountsPartial({
          sessionDelegate: sessionPda,
          agent: agent.publicKey,
          delegate: delegate.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([agent])
        .rpc();

      await expectErr(
        ctx.rt.program.methods
          .claimTaskSession()
          .accountsPartial({
            task,
            globalState: ctx.globalPda,
            agentState: agentStatePda(
              agent.publicKey,
              ctx.rt.program.programId
            ),
            sessionDelegate: sessionPda,
            delegate: delegate.publicKey,
            payer: delegate.publicKey,
            systemProgram: SystemProgram.programId,
          })
          .signers([delegate])
          .rpc(),
        /InvalidInstructionBundle|6053/
      );

      const sessionClaimIx = await ctx.rt.program.methods
        .claimTaskSession()
        .accountsPartial({
          task,
          globalState: ctx.globalPda,
          agentState: agentStatePda(agent.publicKey, ctx.rt.program.programId),
          sessionDelegate: sessionPda,
          delegate: delegate.publicKey,
          payer: delegate.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .remainingAccounts([
          {
            pubkey: SYSVAR_INSTRUCTIONS_PUBKEY,
            isSigner: false,
            isWritable: false,
          },
        ])
        .instruction();
      const sessionSubmitIx = await ctx.rt.program.methods
        .submitWorkSession(Buffer.from("eip155:56:0x" + "99".repeat(32) + ":0"))
        .accountsPartial({
          task,
          globalState: ctx.globalPda,
          agentState: agentStatePda(agent.publicKey, ctx.rt.program.programId),
          sessionDelegate: sessionPda,
          delegate: delegate.publicKey,
        })
        .instruction();
      await ctx.rt.program.provider.sendAndConfirm!(
        new Transaction().add(sessionClaimIx, sessionSubmitIx),
        [delegate]
      );
      const afterSession = await ctx.rt.program.account.task.fetch(task);
      assert.deepEqual(afterSession.state, { submitted: {} });
      assert.equal(afterSession.agent.toBase58(), agent.publicKey.toBase58());
    });

    it("accepts atomic [claim_task, submit_work] and [claim_task, set_payout_to, submit_work] leaving claimed_count unchanged", async () => {
      const client = await freshClient();
      const agent = await freshClient();
      const vault = await freshClient();

      const t1 = await createTask(ctx, client, {
        escrowLamports: ESCROW,
        platform: ONCHAIN_ACTION_PLATFORM,
        verificationKind: 0,
        requiresApproval: false,
        attestationDelayOverride: 60,
        challengeWindowOverride: 900,
      });
      await claimAndSubmitAtomic(
        ctx,
        agent,
        t1.task,
        "eip155:56:0x" + "ab".repeat(32) + ":0"
      );
      const stAfter1 = await ctx.rt.program.account.agentState.fetch(
        agentStatePda(agent.publicKey, ctx.rt.program.programId)
      );
      assert.equal(stAfter1.claimedCount, 0);
      assert.equal(stAfter1.totalTasksClaimed.toNumber(), 1);

      const t2 = await createTask(ctx, client, {
        escrowLamports: ESCROW,
        platform: ONCHAIN_ACTION_PLATFORM,
        verificationKind: 0,
        requiresApproval: false,
        attestationDelayOverride: 60,
        challengeWindowOverride: 900,
      });
      await claimAndSubmitAtomic(
        ctx,
        agent,
        t2.task,
        "eip155:56:0x" + "cd".repeat(32) + ":1",
        vault.publicKey
      );
      const task2Acct = await ctx.rt.program.account.task.fetch(t2.task);
      assert.equal(task2Acct.payoutTo.toBase58(), vault.publicKey.toBase58());
      const stAfter2 = await ctx.rt.program.account.agentState.fetch(
        agentStatePda(agent.publicKey, ctx.rt.program.programId)
      );
      assert.equal(stAfter2.claimedCount, 0);
      assert.equal(stAfter2.totalTasksClaimed.toNumber(), 2);
    });

    it("verify_task(score=0) before deadline reopens OnChainAction to Open, resets payout_to, and second agent completes + finalizes with exact lamport conservation", async () => {
      const client = await freshClient();
      const badAgent = await freshClient();
      const badVault = await freshClient();
      const goodAgent = await freshClient();
      const { task } = await createTask(ctx, client, {
        escrowLamports: ESCROW,
        platform: ONCHAIN_ACTION_PLATFORM,
        verificationKind: 0,
        requiresApproval: false,
        attestationDelayOverride: 60,
        challengeWindowOverride: 900,
        deadlineSeconds: 86_400 * 7,
      });

      await claimAndSubmitAtomic(
        ctx,
        badAgent,
        task,
        "eip155:56:0x" + "11".repeat(32) + ":0",
        badVault.publicKey
      );
      const sub1 = await ctx.rt.program.account.task.fetch(task);
      await handle.runtime.warpTo(sub1.submittedAt.toNumber() + 65);
      const reopenHash = Array.from({ length: 32 }, (_, i) => i + 1);
      const reopenIx = await ctx.rt.program.methods
        .verifyTask(new BN(0), reopenHash as never)
        .accountsPartial({
          task,
          globalState: ctx.globalPda,
          switchboardFeed: ctx.authority.publicKey,
        })
        .instruction();
      reopenIx.keys[2].isSigner = true;
      const reopenTx = new Transaction().add(reopenIx);
      reopenTx.feePayer = ctx.authority.publicKey;
      reopenTx.recentBlockhash =
        (await handle.context.banksClient.getLatestBlockhash())![0];
      reopenTx.sign(ctx.authority);
      const reopenRes = await handle.context.banksClient.processTransaction(
        reopenTx
      );
      const parser = new EventParser(
        ctx.rt.program.programId,
        new BorshCoder(ctx.rt.program.idl)
      );
      const emitted = Array.from(parser.parseLogs(reopenRes.logMessages));
      assert.equal(emitted.length, 1);
      assert.equal(emitted[0].name, "taskReopened");
      assert.equal(
        (emitted[0].data.taskId as BN).toString(),
        sub1.taskId.toString()
      );
      assert.equal(
        (emitted[0].data.prevAgent as PublicKey).toBase58(),
        badAgent.publicKey.toBase58()
      );

      const reopened = await ctx.rt.program.account.task.fetch(task);
      assert.deepEqual(reopened.state, { open: {} });
      assert.equal(reopened.agent.toBase58(), PublicKey.default.toBase58());
      assert.equal(reopened.payoutTo.toBase58(), PublicKey.default.toBase58());
      assert.equal(reopened.submittedAt.toNumber(), 0);

      await claimAndSubmitAtomic(
        ctx,
        goodAgent,
        task,
        "eip155:56:0x" + "22".repeat(32) + ":0"
      );
      const sub2 = await ctx.rt.program.account.task.fetch(task);
      await handle.runtime.warpTo(sub2.submittedAt.toNumber() + 65);
      await verifyTaskOracle(ctx, task, MAX_SCORE);

      const verified = await ctx.rt.program.account.task.fetch(task);
      const taskLamportsBefore = await ctx.rt.getBalance(task);
      const agentBefore = await ctx.rt.getBalance(goodAgent.publicKey);
      const treasuryBefore = await ctx.rt.getBalance(ctx.treasury);
      const clientBefore = await ctx.rt.getBalance(client.publicKey);
      const badVaultBefore = await ctx.rt.getBalance(badVault.publicKey);

      await handle.runtime.warpTo(verified.challengeDeadline.toNumber() + 1);
      await finalizeTask(ctx, task, goodAgent.publicKey, client.publicKey);

      const taskLamportsAfter = await ctx.rt.getBalance(task);
      const agentAfter = await ctx.rt.getBalance(goodAgent.publicKey);
      const treasuryAfter = await ctx.rt.getBalance(ctx.treasury);
      const clientAfter = await ctx.rt.getBalance(client.publicKey);
      const badVaultAfter = await ctx.rt.getBalance(badVault.publicKey);

      assert.equal(taskLamportsAfter, 0n);
      assert.equal(badVaultAfter, badVaultBefore);
      assert.equal(
        agentAfter - agentBefore + (treasuryAfter - treasuryBefore),
        BigInt(ESCROW.toString())
      );
      assert.equal(
        agentAfter -
          agentBefore +
          (treasuryAfter - treasuryBefore) +
          (clientAfter - clientBefore),
        taskLamportsBefore
      );
    });

    it("verify_task(score=0) near deadline does NOT reopen and refunds client on finalize_task", async () => {
      const client = await freshClient();
      const badAgent = await freshClient();
      const { task } = await createTask(ctx, client, {
        escrowLamports: ESCROW,
        platform: ONCHAIN_ACTION_PLATFORM,
        verificationKind: 0,
        requiresApproval: false,
        attestationDelayOverride: 60,
        challengeWindowOverride: 900,
        claimBuffer: 14_400,
        submitMargin: 3_600,
        deadlineSeconds: 14_500,
      });

      await claimAndSubmitAtomic(
        ctx,
        badAgent,
        task,
        "eip155:56:0x" + "33".repeat(32) + ":0"
      );
      const sub = await ctx.rt.program.account.task.fetch(task);
      // Warp into [submitted_at + 60, submitted_at + staleness] where now + claim_buffer >= deadline.
      await handle.runtime.warpTo(sub.submittedAt.toNumber() + 200);
      await verifyTaskOracle(ctx, task, 0);

      const verified = await ctx.rt.program.account.task.fetch(task);
      assert.deepEqual(verified.state, { verified: {} });
      assert.equal(verified.paymentAmount.toNumber(), 0);
      assert.equal(verified.feeAmount.toNumber(), 0);

      const clientBefore = await ctx.rt.getBalance(client.publicKey);
      await handle.runtime.warpTo(verified.challengeDeadline.toNumber() + 1);
      await finalizeTask(ctx, task, badAgent.publicKey, client.publicKey);
      const clientAfter = await ctx.rt.getBalance(client.publicKey);
      assert.isTrue(clientAfter - clientBefore >= BigInt(ESCROW.toString()));
    });

    it("rejects non-signer oracle authority, legacy Switchboard feed, imposter signer, and S02-F01 invalid timeout ordering", async () => {
      const client = await freshClient();
      const agent = await freshClient();

      await expectErr(
        createTask(ctx, client, {
          escrowLamports: ESCROW,
          platform: ONCHAIN_ACTION_PLATFORM,
          verificationKind: 0,
          requiresApproval: false,
          attestationDelayOverride: 60,
          verificationTimeoutOverride: 86_400,
        }),
        /InvalidTimeoutOrdering/
      );

      const { task } = await createTask(ctx, client, {
        escrowLamports: ESCROW,
        platform: ONCHAIN_ACTION_PLATFORM,
        verificationKind: 0,
        requiresApproval: false,
        attestationDelayOverride: 60,
        challengeWindowOverride: 900,
      });
      await claimAndSubmitAtomic(
        ctx,
        agent,
        task,
        "eip155:56:0x" + "44".repeat(32) + ":0"
      );
      const sub = await ctx.rt.program.account.task.fetch(task);
      await handle.runtime.warpTo(sub.submittedAt.toNumber() + 65);

      const hash = Array.from({ length: 32 }, (_, i) => i + 1);
      // (g1) Legacy non-signer Switchboard feed rejected.
      await expectErr(
        ctx.rt.program.methods
          .verifyTask(new BN(MAX_SCORE), hash as never)
          .accountsPartial({
            task,
            globalState: ctx.globalPda,
            switchboardFeed: DUMMY_SWITCHBOARD_FEED,
          })
          .rpc(),
        /OracleAuthorityMismatch/
      );

      // (g2) Authorized oracle pubkey passed WITHOUT signing (payer = client) rejected!
      const unsignedOracleIx = await ctx.rt.program.methods
        .verifyTask(new BN(MAX_SCORE), hash as never)
        .accountsPartial({
          task,
          globalState: ctx.globalPda,
          switchboardFeed: ctx.authority.publicKey,
        })
        .instruction();
      unsignedOracleIx.keys[2].isSigner = false;
      const unsignedTx = new Transaction().add(unsignedOracleIx);
      unsignedTx.feePayer = client.publicKey;
      unsignedTx.recentBlockhash =
        (await handle.context.banksClient.getLatestBlockhash())![0];
      unsignedTx.sign(client);
      await expectErr(
        handle.context.banksClient.processTransaction(unsignedTx),
        /OracleAuthorityMismatch|6026|0x178a/
      );

      // (g3) Imposter signer rejected.
      const imposter = await freshClient();
      const badIx = await ctx.rt.program.methods
        .verifyTask(new BN(MAX_SCORE), hash as never)
        .accountsPartial({
          task,
          globalState: ctx.globalPda,
          switchboardFeed: imposter.publicKey,
        })
        .instruction();
      badIx.keys[2].isSigner = true;
      await expectErr(
        ctx.rt.program.provider.sendAndConfirm!(new Transaction().add(badIx), [
          imposter,
        ]),
        /OracleAuthorityMismatch/
      );
    });
  });
});
