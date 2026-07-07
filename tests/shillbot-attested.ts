import { startAnchor } from "anchor-bankrun";
import { BankrunProvider } from "anchor-bankrun";
import { BN, Program } from "@coral-xyz/anchor";
import {
  ComputeBudgetProgram,
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  SystemProgram,
  SYSVAR_SLOT_HASHES_PUBKEY,
  Transaction,
} from "@solana/web3.js";
import { Clock } from "solana-bankrun";
import { assert } from "chai";
import { createHash } from "crypto";

import { Shillbot } from "../target/types/shillbot";
const IDL = require("../target/idl/shillbot.json");

// ---------------------------------------------------------------------------
// Deterministic attested-verification suite (bankrun).
//
// Covers the 2026-07-07 S2 surface:
//   - verification_kind carve: create_task kind arg + bidirectional
//     kind↔platform validation
//   - verify_task_attested: attester gating (oracle_authority Signer),
//     binary-score rule, threshold guard, mutual exclusion with verify_task
//   - claim self-claim guard on kind-1 tasks
//   - challenge_deadline boundary battery at deadline−1 / deadline / deadline+1
//     (the Truebit-class dead-second, pinned)
//   - resolve_challenge_default: permissionless dispute-liveness crank with
//     its own −1 / == / +1 battery, agent-favoring payout, bond un-slashed
// ---------------------------------------------------------------------------

const MAX_SCORE = 1_000_000;
const PROTOCOL_FEE_BPS = 1000; // 10%
const QUALITY_THRESHOLD = new BN(200_000);
const ESCROW_LAMPORTS = new BN(1 * LAMPORTS_PER_SOL);
const LEAN_PROOF_PLATFORM = 10;
const LEAN_CHALLENGE_WINDOW = 3600; // per-task override armed at create
const DISPUTE_WINDOW = 604_800; // initialize default (7 days)
const MIN_CHALLENGE_BOND_MULTIPLIER = 2;

const DUMMY_SWITCHBOARD_FEED = new PublicKey(
  "11111111111111111111111111111112"
);

function contentHash(data: string): number[] {
  return Array.from(createHash("sha256").update(data).digest());
}

function pda(seeds: (Buffer | Uint8Array)[], programId: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync(seeds, programId)[0];
}

describe("shillbot-attested (bankrun)", () => {
  let context: Awaited<ReturnType<typeof startAnchor>>;
  let provider: BankrunProvider;
  let program: Program<Shillbot>;

  const authority = Keypair.generate(); // also the initial oracle_authority
  const client = Keypair.generate();
  const agent = Keypair.generate();
  const challenger = Keypair.generate();
  const treasury = Keypair.generate();

  let globalPda: PublicKey;

  async function warpTo(ts: number): Promise<void> {
    const c = await context.banksClient.getClock();
    context.setClock(
      new Clock(
        c.slot,
        c.epochStartTimestamp,
        c.epoch,
        c.leaderScheduleEpoch,
        BigInt(ts)
      )
    );
  }

  async function now(): Promise<number> {
    return Number((await context.banksClient.getClock()).unixTimestamp);
  }

  async function balance(pubkey: PublicKey): Promise<bigint> {
    const account = await context.banksClient.getAccount(pubkey);
    return account === null ? BigInt(0) : BigInt(account.lamports);
  }

  /** Create a task; kind/platform parameterized for the validation tests. */
  async function createTask(opts?: {
    kind?: number;
    platform?: number;
    challengeWindowOverride?: number;
  }): Promise<{ taskPda: PublicKey; taskId: BN }> {
    const kind = opts?.kind ?? 1;
    const platform = opts?.platform ?? LEAN_PROOF_PLATFORM;
    const windowOverride =
      opts?.challengeWindowOverride ?? LEAN_CHALLENGE_WINDOW;
    const global = await program.account.globalState.fetch(globalPda);
    const taskId = global.taskCounter;
    const taskPda = pda(
      [
        Buffer.from("task"),
        taskId.toArrayLike(Buffer, "le", 8),
        client.publicKey.toBuffer(),
      ],
      program.programId
    );
    const deadline = new BN((await now()) + 86_400 * 30);
    await program.methods
      .createTask(
        ESCROW_LAMPORTS,
        contentHash(`statement ${taskId.toString()}`) as any,
        deadline,
        new BN(3600),
        new BN(14_400),
        platform,
        0,
        windowOverride,
        0,
        false, // requires_approval off — attester verifies from Submitted
        kind
      )
      .accountsPartial({
        globalState: globalPda,
        task: taskPda,
        clientState: pda(
          [Buffer.from("client_state"), client.publicKey.toBuffer()],
          program.programId
        ),
        client: client.publicKey,
        slotHashes: SYSVAR_SLOT_HASHES_PUBKEY,
        systemProgram: SystemProgram.programId,
      })
      .signers([client])
      .rpc();
    return { taskPda, taskId };
  }

  async function claimAndSubmit(taskPda: PublicKey): Promise<void> {
    const agentState = pda(
      [Buffer.from("agent_state"), agent.publicKey.toBuffer()],
      program.programId
    );
    await program.methods
      .claimTask()
      .accountsPartial({
        task: taskPda,
        agentState,
        agent: agent.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([agent])
      .rpc();
    await program.methods
      .submitWork(Buffer.from("https://artifacts.test/proof.lean"))
      .accountsPartial({
        task: taskPda,
        agentState,
        agent: agent.publicKey,
      })
      .signers([agent])
      .rpc();
  }

  function attestationHash(taskId: BN, score: number): number[] {
    // Stand-in for the chain-core AttestationCert digest — any non-zero
    // 32 bytes; the program stores it opaquely as verification_hash.
    return contentHash(`attestation ${taskId.toString()} ${score}`);
  }

  async function verifyAttested(
    taskPda: PublicKey,
    taskId: BN,
    score: number,
    attester: Keypair
  ): Promise<void> {
    await program.methods
      .verifyTaskAttested(new BN(score), attestationHash(taskId, score) as any)
      .accountsPartial({
        task: taskPda,
        globalState: globalPda,
        attester: attester.publicKey,
      })
      .signers([attester])
      .rpc();
  }

  async function challenge(taskPda: PublicKey, taskId: BN): Promise<PublicKey> {
    const challengePda = pda(
      [
        Buffer.from("challenge"),
        taskId.toArrayLike(Buffer, "le", 8),
        challenger.publicKey.toBuffer(),
      ],
      program.programId
    );
    await program.methods
      .challengeTask()
      .accountsPartial({
        task: taskPda,
        challenge: challengePda,
        challenger: challenger.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([challenger])
      .rpc();
    return challengePda;
  }

  async function expectError(
    p: Promise<unknown>,
    pattern: RegExp
  ): Promise<void> {
    try {
      await p;
      assert.fail(`expected ${pattern}, got success`);
    } catch (e: any) {
      assert.match(String(e), pattern);
    }
  }

  before(async () => {
    context = await startAnchor(".", [], []);
    provider = new BankrunProvider(context);
    program = new Program<Shillbot>(IDL, provider);
    globalPda = pda([Buffer.from("shillbot_global")], program.programId);
    for (const kp of [authority, client, agent, challenger, treasury]) {
      const tx = new Transaction().add(
        SystemProgram.transfer({
          fromPubkey: provider.wallet.publicKey,
          toPubkey: kp.publicKey,
          lamports: 100 * LAMPORTS_PER_SOL,
        })
      );
      await provider.sendAndConfirm(tx);
    }
    await program.methods
      .initialize(
        PROTOCOL_FEE_BPS,
        QUALITY_THRESHOLD,
        new BN(0),
        DUMMY_SWITCHBOARD_FEED
      )
      .accountsPartial({
        globalState: globalPda,
        authority: authority.publicKey,
        treasury: treasury.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([authority])
      .rpc();
  });

  describe("create_task verification_kind validation", () => {
    it("initialize arms the 7-day dispute-resolution window", async () => {
      const global = await program.account.globalState.fetch(globalPda);
      assert.equal(
        global.disputeResolutionWindowSeconds.toNumber(),
        DISPUTE_WINDOW
      );
    });

    it("rejects unknown kind (2)", async () => {
      await expectError(createTask({ kind: 2 }), /InvalidVerificationKind/);
    });

    it("rejects kind 1 on a non-LeanProof platform", async () => {
      await expectError(
        createTask({ kind: 1, platform: 9 }),
        /VerificationKindMismatch/
      );
    });

    it("rejects kind 0 on the LeanProof platform", async () => {
      await expectError(
        createTask({ kind: 0, platform: LEAN_PROOF_PLATFORM }),
        /VerificationKindMismatch/
      );
    });

    it("persists kind 1 on a LeanProof task", async () => {
      const { taskPda } = await createTask();
      const task = await program.account.task.fetch(taskPda);
      assert.equal(task.verificationKind, 1);
      assert.equal(task.platform, LEAN_PROOF_PLATFORM);
    });
  });

  describe("claim arms-length guard (kind 1)", () => {
    it("rejects the client claiming their own deterministic task", async () => {
      const { taskPda } = await createTask();
      const clientAgentState = pda(
        [Buffer.from("agent_state"), client.publicKey.toBuffer()],
        program.programId
      );
      await expectError(
        program.methods
          .claimTask()
          .accountsPartial({
            task: taskPda,
            agentState: clientAgentState,
            agent: client.publicKey,
            systemProgram: SystemProgram.programId,
          })
          .signers([client])
          .rpc(),
        /SelfClaimForbidden/
      );
    });
  });

  describe("verify_task_attested", () => {
    it("full lifecycle: attested pass pays full escrow minus fee", async () => {
      const { taskPda, taskId } = await createTask();
      await claimAndSubmit(taskPda);
      await verifyAttested(taskPda, taskId, MAX_SCORE, authority);

      const task = await program.account.task.fetch(taskPda);
      assert.deepEqual(task.state, { verified: {} });
      assert.equal(task.compositeScore.toNumber(), MAX_SCORE);

      // score = MAX ⇒ gross = full escrow; fee = 10%; payment = 90%.
      const gross = ESCROW_LAMPORTS.toNumber();
      const fee = Math.floor((gross * PROTOCOL_FEE_BPS) / 10_000);
      assert.equal(task.paymentAmount.toNumber(), gross - fee);
      assert.equal(task.feeAmount.toNumber(), fee);
      assert.equal(
        task.challengeDeadline.toNumber(),
        task.verifiedAt.toNumber() + LEAN_CHALLENGE_WINDOW
      );

      // Finalize after the window: agent paid, treasury fee'd, task closed.
      await warpTo(task.challengeDeadline.toNumber() + 1);
      const agentBefore = await balance(agent.publicKey);
      const treasuryBefore = await balance(treasury.publicKey);
      await program.methods
        .finalizeTask()
        .accountsPartial({
          task: taskPda,
          globalState: globalPda,
          agent: agent.publicKey,
          client: client.publicKey,
          treasury: treasury.publicKey,
        })
        .rpc();
      assert.isNull(await context.banksClient.getAccount(taskPda));
      assert.equal(
        (await balance(agent.publicKey)) - agentBefore,
        BigInt(gross - fee)
      );
      assert.equal(
        (await balance(treasury.publicKey)) - treasuryBefore,
        BigInt(fee)
      );
    });

    it("attested fail (score 0) verifies with zero payment", async () => {
      const { taskPda, taskId } = await createTask();
      await claimAndSubmit(taskPda);
      await verifyAttested(taskPda, taskId, 0, authority);
      const task = await program.account.task.fetch(taskPda);
      assert.deepEqual(task.state, { verified: {} });
      assert.equal(task.paymentAmount.toNumber(), 0);
      assert.equal(task.feeAmount.toNumber(), 0);
    });

    it("rejects a non-binary score", async () => {
      const { taskPda, taskId } = await createTask();
      await claimAndSubmit(taskPda);
      await expectError(
        verifyAttested(taskPda, taskId, 500_000, authority),
        /AttestedScoreNotBinary/
      );
    });

    it("rejects a signer that is not the oracle_authority", async () => {
      const { taskPda, taskId } = await createTask();
      await claimAndSubmit(taskPda);
      await expectError(
        verifyAttested(taskPda, taskId, MAX_SCORE, challenger),
        /OracleAuthorityMismatch/
      );
    });

    it("honors oracle_authority rotation", async () => {
      const newAttester = Keypair.generate();
      const tx = new Transaction().add(
        SystemProgram.transfer({
          fromPubkey: provider.wallet.publicKey,
          toPubkey: newAttester.publicKey,
          lamports: LAMPORTS_PER_SOL,
        })
      );
      await provider.sendAndConfirm(tx);

      await program.methods
        .updateOracleAuthority(newAttester.publicKey)
        .accountsPartial({
          globalState: globalPda,
          authority: authority.publicKey,
        })
        .signers([authority])
        .rpc();

      const { taskPda, taskId } = await createTask();
      await claimAndSubmit(taskPda);
      // Old attester (authority) now rejected; the rotated key verifies.
      await expectError(
        verifyAttested(taskPda, taskId, MAX_SCORE, authority),
        /OracleAuthorityMismatch/
      );
      await verifyAttested(taskPda, taskId, MAX_SCORE, newAttester);
      const task = await program.account.task.fetch(taskPda);
      assert.deepEqual(task.state, { verified: {} });

      // Restore for subsequent tests.
      await program.methods
        .updateOracleAuthority(authority.publicKey)
        .accountsPartial({
          globalState: globalPda,
          authority: authority.publicKey,
        })
        .signers([authority])
        .rpc();
    });

    it("rejects a zero verification hash", async () => {
      const { taskPda } = await createTask();
      await claimAndSubmit(taskPda);
      await expectError(
        program.methods
          .verifyTaskAttested(new BN(MAX_SCORE), new Array(32).fill(0) as any)
          .accountsPartial({
            task: taskPda,
            globalState: globalPda,
            attester: authority.publicKey,
          })
          .signers([authority])
          .rpc(),
        /InvalidVerificationHash/
      );
    });
  });

  describe("verification-kind mutual exclusion", () => {
    it("verify_task rejects a kind-1 task before touching the feed", async () => {
      const { taskPda } = await createTask();
      await claimAndSubmit(taskPda);
      await expectError(
        program.methods
          .verifyTask(new BN(MAX_SCORE), contentHash("switchboard") as any)
          .accountsPartial({
            task: taskPda,
            globalState: globalPda,
            switchboardFeed: DUMMY_SWITCHBOARD_FEED,
          })
          .rpc(),
        /VerificationKindMismatch/
      );
    });

    it("verify_task_attested rejects a kind-0 task", async () => {
      const { taskPda, taskId } = await createTask({
        kind: 0,
        platform: 9, // Website — a real oracle-kind platform
      });
      await claimAndSubmit(taskPda);
      await expectError(
        verifyAttested(taskPda, taskId, MAX_SCORE, authority),
        /VerificationKindMismatch/
      );
    });
  });

  describe("quality-threshold guard", () => {
    it("fails loud when quality_threshold == MAX_SCORE", async () => {
      const setParams = (threshold: BN) =>
        program.methods
          .updateParams(
            PROTOCOL_FEE_BPS,
            threshold,
            new BN(86_400),
            new BN(1_209_600),
            new BN(604_800),
            new BN(86_400),
            5,
            2,
            5_000,
            false,
            0,
            new BN(3600),
            10,
            new BN(DISPUTE_WINDOW)
          )
          .accountsPartial({
            globalState: globalPda,
            authority: authority.publicKey,
          })
          .signers([authority])
          .rpc();

      await setParams(new BN(MAX_SCORE));
      const { taskPda, taskId } = await createTask();
      await claimAndSubmit(taskPda);
      await expectError(
        verifyAttested(taskPda, taskId, MAX_SCORE, authority),
        /QualityThresholdTooHighForBinary/
      );
      await setParams(QUALITY_THRESHOLD);
      // Verifies fine once the parameter is sane again.
      await verifyAttested(taskPda, taskId, MAX_SCORE, authority);
    });
  });

  describe("challenge_deadline boundary battery (the Truebit dead second)", () => {
    // challenge uses `now < deadline`, finalize uses `now > deadline`:
    // at deadline−1 only challenge is live; at deadline exactly NEITHER is
    // (dead second, mutual exclusion); at deadline+1 only finalize is.
    async function verifiedTask(): Promise<{
      taskPda: PublicKey;
      taskId: BN;
      deadline: number;
    }> {
      const { taskPda, taskId } = await createTask();
      await claimAndSubmit(taskPda);
      await verifyAttested(taskPda, taskId, MAX_SCORE, authority);
      const task = await program.account.task.fetch(taskPda);
      return { taskPda, taskId, deadline: task.challengeDeadline.toNumber() };
    }

    const finalize = (taskPda: PublicKey) =>
      program.methods
        .finalizeTask()
        .accountsPartial({
          task: taskPda,
          globalState: globalPda,
          agent: agent.publicKey,
          client: client.publicKey,
          treasury: treasury.publicKey,
        })
        .rpc();

    it("deadline−1: finalize rejected, challenge accepted", async () => {
      const t = await verifiedTask();
      await warpTo(t.deadline - 1);
      await expectError(finalize(t.taskPda), /ChallengeWindowOpen/);
      await challenge(t.taskPda, t.taskId); // succeeds
    });

    it("deadline exactly: BOTH rejected — the dead second is mutual-exclusive", async () => {
      const t = await verifiedTask();
      await warpTo(t.deadline);
      await expectError(finalize(t.taskPda), /ChallengeWindowOpen/);
      await expectError(
        challenge(t.taskPda, t.taskId),
        /ChallengeWindowClosed/
      );
    });

    it("deadline+1: challenge rejected, finalize accepted", async () => {
      const t = await verifiedTask();
      await warpTo(t.deadline + 1);
      await expectError(
        challenge(t.taskPda, t.taskId),
        /ChallengeWindowClosed/
      );
      await finalize(t.taskPda); // succeeds
      assert.isNull(await context.banksClient.getAccount(t.taskPda));
    });
  });

  describe("resolve_challenge_default (dispute liveness)", () => {
    async function disputedTask(): Promise<{
      taskPda: PublicKey;
      taskId: BN;
      challengePda: PublicKey;
      resolutionDeadline: number;
    }> {
      const { taskPda, taskId } = await createTask();
      await claimAndSubmit(taskPda);
      await verifyAttested(taskPda, taskId, MAX_SCORE, authority);
      const challengePda = await challenge(taskPda, taskId);
      const ch = await program.account.challenge.fetch(challengePda);
      return {
        taskPda,
        taskId,
        challengePda,
        resolutionDeadline: ch.createdAt.toNumber() + DISPUTE_WINDOW,
      };
    }

    // A per-call compute-budget nonce keeps repeated (identical) failing
    // attempts from colliding on bankrun's static blockhash, which would
    // surface as "already processed" instead of the program error.
    let resolveNonce = 300_000;
    const defaultResolve = (t: {
      taskPda: PublicKey;
      challengePda: PublicKey;
    }) =>
      program.methods
        .resolveChallengeDefault()
        .preInstructions([
          ComputeBudgetProgram.setComputeUnitLimit({ units: resolveNonce++ }),
        ])
        .accountsPartial({
          task: t.taskPda,
          challenge: t.challengePda,
          globalState: globalPda,
          agent: agent.publicKey,
          client: client.publicKey,
          challenger: challenger.publicKey,
          treasury: treasury.publicKey,
        })
        .rpc();

    it("deadline−1 and deadline exactly: still the authority's window", async () => {
      const t = await disputedTask();
      await warpTo(t.resolutionDeadline - 1);
      await expectError(defaultResolve(t), /DisputeWindowStillOpen/);
      await warpTo(t.resolutionDeadline);
      await expectError(defaultResolve(t), /DisputeWindowStillOpen/);
    });

    it("deadline+1: pays the pinned amounts and returns the bond un-slashed", async () => {
      const t = await disputedTask();
      const task = await program.account.task.fetch(t.taskPda);
      const payment = task.paymentAmount.toNumber();
      const fee = task.feeAmount.toNumber();
      const bond = ESCROW_LAMPORTS.toNumber() * MIN_CHALLENGE_BOND_MULTIPLIER;

      await warpTo(t.resolutionDeadline + 1);
      const agentBefore = await balance(agent.publicKey);
      const treasuryBefore = await balance(treasury.publicKey);
      const challengerBefore = await balance(challenger.publicKey);

      await defaultResolve(t);

      // Task + challenge accounts closed.
      assert.isNull(await context.banksClient.getAccount(t.taskPda));
      assert.isNull(await context.banksClient.getAccount(t.challengePda));
      // Agent-favoring: pinned payment executes; fee to treasury.
      assert.equal(
        (await balance(agent.publicKey)) - agentBefore,
        BigInt(payment)
      );
      assert.equal(
        (await balance(treasury.publicKey)) - treasuryBefore,
        BigInt(fee)
      );
      // Bond returns un-slashed (plus the challenge account's rent).
      const challengerDelta =
        (await balance(challenger.publicKey)) - challengerBefore;
      assert.isTrue(
        challengerDelta >= BigInt(bond),
        `challenger got ${challengerDelta}, expected >= bond ${bond}`
      );
    });

    it("authority resolve_challenge still works inside the window", async () => {
      const t = await disputedTask();
      await warpTo(t.resolutionDeadline - 100);
      await program.methods
        .resolveChallenge(false) // agent wins
        .accountsPartial({
          task: t.taskPda,
          challenge: t.challengePda,
          globalState: globalPda,
          authority: authority.publicKey,
          agent: agent.publicKey,
          client: client.publicKey,
          challenger: challenger.publicKey,
          treasury: treasury.publicKey,
        })
        .signers([authority])
        .rpc();
      assert.isNull(await context.banksClient.getAccount(t.taskPda));
    });

    it("window=0 disables default resolution (legacy behavior)", async () => {
      // Disable, build a dispute, warp far ahead — default resolve must
      // stay closed; re-enable for any later suites.
      const setWindow = (w: number) =>
        program.methods
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
            0,
            new BN(3600),
            10,
            new BN(w)
          )
          .accountsPartial({
            globalState: globalPda,
            authority: authority.publicKey,
          })
          .signers([authority])
          .rpc();

      await setWindow(0);
      const t = await disputedTask();
      await warpTo((await now()) + DISPUTE_WINDOW * 10);
      await expectError(defaultResolve(t), /DisputeWindowDisabled/);
      await setWindow(DISPUTE_WINDOW);
    });
  });
});
