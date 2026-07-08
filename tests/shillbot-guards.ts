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

import { BN } from "@coral-xyz/anchor";
import { Keypair, LAMPORTS_PER_SOL, PublicKey } from "@solana/web3.js";
import { assert } from "chai";

import { startShillbotBankrun } from "./harness/shillbot-bankrun";
import {
  ShillbotCtx,
  setupShillbot,
  createTask,
  claimTask,
  submitWork,
  verifyTaskAttested,
  challengeTask,
  challengePda,
  agentStatePda,
  finalizeTask,
  resolveChallengeDefault,
  challengeDeadline,
  LEAN_PROOF_PLATFORM,
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
    it("rejects the client self-claiming their own deterministic task", async () => {
      const client = await freshClient();
      const { task } = await createTask(ctx, client, {
        escrowLamports: ESCROW,
        platform: LEAN_PROOF_PLATFORM,
        verificationKind: 1,
        requiresApproval: false,
        challengeWindowOverride: CHALLENGE_WINDOW,
      });
      await expectErr(claimTask(ctx, client, task), /SelfClaimForbidden/);
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
});
