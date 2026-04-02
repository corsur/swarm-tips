import { startAnchor } from "anchor-bankrun";
import { BankrunProvider } from "anchor-bankrun";
import { BN, Program } from "@coral-xyz/anchor";
import {
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

// Import the IDL type — anchor-bankrun requires the JSON IDL
import { Shillbot } from "../target/types/shillbot";
const IDL = require("../target/idl/shillbot.json");

// ---------------------------------------------------------------------------
// Constants matching the on-chain program
// ---------------------------------------------------------------------------

const MAX_SCORE = 1_000_000;
const SEVEN_DAYS_SECONDS = 604_800; // 7 days
const PROTOCOL_FEE_BPS = 1000; // 10%
const QUALITY_THRESHOLD = new BN(200_000);
const ESCROW_LAMPORTS = new BN(1 * LAMPORTS_PER_SOL); // 1 SOL
const MIN_CHALLENGE_BOND_MULTIPLIER = 2;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function contentHash(data: string): number[] {
  return Array.from(createHash("sha256").update(data).digest());
}

function globalStatePda(programId: PublicKey): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("shillbot_global")],
    programId
  );
}

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

function agentStatePda(
  agent: PublicKey,
  programId: PublicKey
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("agent_state"), agent.toBuffer()],
    programId
  );
}

/** Warp the bankrun clock to a target unix timestamp. */
async function warpToTimestamp(
  context: Awaited<ReturnType<typeof startAnchor>>,
  targetTimestamp: number
): Promise<void> {
  const currentClock = await context.banksClient.getClock();
  const newClock = new Clock(
    currentClock.slot,
    currentClock.epochStartTimestamp,
    currentClock.epoch,
    currentClock.leaderScheduleEpoch,
    BigInt(targetTimestamp)
  );
  context.setClock(newClock);
}

/** Get account lamport balance via bankrun's getAccount. Returns 0 if not found. */
async function getBalance(
  context: Awaited<ReturnType<typeof startAnchor>>,
  pubkey: PublicKey
): Promise<bigint> {
  const account = await context.banksClient.getAccount(pubkey);
  if (account === null) {
    return BigInt(0);
  }
  return BigInt(account.lamports);
}

/**
 * Compute expected payment and fee using the same formula as the on-chain program.
 * Returns [payment, fee, remainder].
 */
function computeExpectedPayment(
  compositeScore: number,
  qualityThreshold: number,
  escrowLamports: number,
  protocolFeeBps: number
): [number, number, number] {
  if (compositeScore < qualityThreshold) {
    return [0, 0, escrowLamports];
  }
  const scoreRange = MAX_SCORE - qualityThreshold;
  if (scoreRange === 0) {
    return [0, 0, escrowLamports];
  }
  const scoreAbove = compositeScore - qualityThreshold;
  const grossPayment = Math.floor((escrowLamports * scoreAbove) / scoreRange);
  const fee = Math.floor((grossPayment * protocolFeeBps) / 10_000);
  const payment = grossPayment - fee;
  const remainder = escrowLamports - payment - fee;
  return [payment, fee, remainder];
}

/** Fund a keypair via transfer from the context payer. */
async function fundAccount(
  provider: BankrunProvider,
  recipient: PublicKey,
  lamports: number
): Promise<void> {
  const tx = new Transaction().add(
    SystemProgram.transfer({
      fromPubkey: provider.wallet.publicKey,
      toPubkey: recipient,
      lamports,
    })
  );
  await provider.sendAndConfirm(tx);
}

// ---------------------------------------------------------------------------
// Lifecycle helpers — drive a task through states
// ---------------------------------------------------------------------------

interface TaskSetup {
  taskPda: PublicKey;
  taskId: BN;
  globalPda: PublicKey;
}

// Dummy Switchboard feed pubkey used in lifecycle tests. The local validator
// has no real Switchboard program, but verify_task checks that globalState's
// switchboard_feed is set (non-default) and that the feed account matches.
const DUMMY_SWITCHBOARD_FEED = Keypair.generate().publicKey;

async function initializeGlobal(
  program: Program<Shillbot>,
  authority: Keypair,
  treasury: PublicKey,
  globalPda: PublicKey
): Promise<void> {
  await program.methods
    .initialize(PROTOCOL_FEE_BPS, QUALITY_THRESHOLD, new BN(0))
    .accountsPartial({
      globalState: globalPda,
      authority: authority.publicKey,
      treasury: treasury,
      systemProgram: SystemProgram.programId,
    })
    .signers([authority])
    .rpc();

  // Configure switchboard feed so verify_task doesn't reject with
  // SwitchboardFeedNotConfigured.
  await program.methods
    .setSwitchboardFeed(DUMMY_SWITCHBOARD_FEED)
    .accountsPartial({
      globalState: globalPda,
      authority: authority.publicKey,
    })
    .signers([authority])
    .rpc();
}

async function createTask(
  program: Program<Shillbot>,
  client: Keypair,
  globalPda: PublicKey,
  deadline: BN
): Promise<TaskSetup> {
  const global = await program.account.globalState.fetch(globalPda);
  const [tPda] = taskPda(
    global.taskCounter,
    client.publicKey,
    program.programId
  );
  const content = contentHash(
    "lifecycle test task " + global.taskCounter.toString()
  );

  await program.methods
    .createTask(
      ESCROW_LAMPORTS,
      content as any,
      deadline,
      new BN(3600), // submit_margin = 1 hour
      new BN(14_400), // claim_buffer = 4 hours
      0, // platform = YouTube
      0,
      0,
      0 // timing overrides: use global defaults
    )
    .accountsPartial({
      globalState: globalPda,
      task: tPda,
      client: client.publicKey,
      slotHashes: SYSVAR_SLOT_HASHES_PUBKEY,
      systemProgram: SystemProgram.programId,
    })
    .signers([client])
    .rpc();

  return { taskPda: tPda, taskId: global.taskCounter, globalPda };
}

async function claimTask(
  program: Program<Shillbot>,
  agent: Keypair,
  taskPdaAddr: PublicKey
): Promise<void> {
  const [agentPda] = agentStatePda(agent.publicKey, program.programId);
  await program.methods
    .claimTask()
    .accountsPartial({
      task: taskPdaAddr,
      agentState: agentPda,
      agent: agent.publicKey,
      systemProgram: SystemProgram.programId,
    })
    .signers([agent])
    .rpc();
}

async function submitWork(
  program: Program<Shillbot>,
  agent: Keypair,
  taskPdaAddr: PublicKey,
  videoId: string
): Promise<void> {
  const [agentPda] = agentStatePda(agent.publicKey, program.programId);
  await program.methods
    .submitWork(Buffer.from(videoId))
    .accountsPartial({
      task: taskPdaAddr,
      agentState: agentPda,
      agent: agent.publicKey,
    })
    .signers([agent])
    .rpc();
}

// verify_task requires a real Switchboard feed account with valid
// PullFeedAccountData. On a local validator there is no Switchboard program,
// so the feed account cannot be initialized. Tests that depend on verify_task
// are skipped. Use the devnet E2E test for full Switchboard verification.
async function verifyTask(
  _program: Program<Shillbot>,
  _authority: Keypair,
  _taskPdaAddr: PublicKey,
  _globalPda: PublicKey,
  _compositeScore: BN
): Promise<void> {
  throw new Error(
    "verifyTask is not available on local validator (no Switchboard feed)"
  );
}

// ---------------------------------------------------------------------------
// Test suite: lifecycle flows using bankrun clock manipulation
// ---------------------------------------------------------------------------

describe("shillbot-lifecycle (bankrun)", () => {
  // Shared state across all tests — each test gets its own task
  let context: Awaited<ReturnType<typeof startAnchor>>;
  let provider: BankrunProvider;
  let program: Program<Shillbot>;

  const authority = Keypair.generate();
  const client = Keypair.generate();
  const agent = Keypair.generate();
  const challenger = Keypair.generate();
  const treasury = Keypair.generate();

  let globalPda: PublicKey;

  before(async () => {
    context = await startAnchor(".", [], []);
    provider = new BankrunProvider(context);
    program = new Program<Shillbot>(IDL, provider);

    [globalPda] = globalStatePda(program.programId);

    // Fund all test wallets from the bankrun payer
    for (const kp of [authority, client, agent, challenger, treasury]) {
      await fundAccount(provider, kp.publicKey, 100 * LAMPORTS_PER_SOL);
    }

    // Initialize global state
    await initializeGlobal(program, authority, treasury.publicKey, globalPda);
  });

  // -------------------------------------------------------------------------
  // Test 1: Full happy path — create → claim → submit → verify → finalize
  // -------------------------------------------------------------------------

  describe("full happy path: create → claim → submit → verify → finalize", () => {
    let setup: TaskSetup;
    let submittedAt: number;

    it("creates, claims, and submits work", async () => {
      const currentClock = await context.banksClient.getClock();
      const now = Number(currentClock.unixTimestamp);
      const deadline = new BN(now + 86_400 * 60); // 60 days in the future

      setup = await createTask(program, client, globalPda, deadline);
      await claimTask(program, agent, setup.taskPda);
      await submitWork(program, agent, setup.taskPda, "happy-path-video-id");

      const task = await program.account.task.fetch(setup.taskPda);
      assert.deepEqual(task.state, { submitted: {} });
      submittedAt = task.submittedAt.toNumber();
      assert.isTrue(submittedAt > 0, "submitted_at should be set");
    });

    it.skip("warps clock to T+7d and verifies with max score", async () => {
      // Warp to submitted_at + 7 days (within staleness window)
      const verifyTime = submittedAt + SEVEN_DAYS_SECONDS;
      await warpToTimestamp(context, verifyTime);

      await verifyTask(
        program,
        authority,
        setup.taskPda,
        globalPda,
        new BN(MAX_SCORE)
      );

      const task = await program.account.task.fetch(setup.taskPda);
      assert.deepEqual(task.state, { verified: {} });
      assert.equal(task.compositeScore.toString(), MAX_SCORE.toString());

      // Payment should be: gross = 1 SOL * (1M - 200k) / (1M - 200k) = 1 SOL
      // fee = 1 SOL * 1000 / 10000 = 0.1 SOL, payment = 0.9 SOL
      const [expectedPayment] = computeExpectedPayment(
        MAX_SCORE,
        QUALITY_THRESHOLD.toNumber(),
        ESCROW_LAMPORTS.toNumber(),
        PROTOCOL_FEE_BPS
      );
      assert.equal(task.paymentAmount.toString(), expectedPayment.toString());
      assert.isTrue(
        task.challengeDeadline.toNumber() > 0,
        "challenge_deadline should be set"
      );
    });

    it.skip("warps past challenge deadline and finalizes", async () => {
      const task = await program.account.task.fetch(setup.taskPda);
      const pastChallenge = task.challengeDeadline.toNumber() + 1;
      await warpToTimestamp(context, pastChallenge);

      // Record balances before finalize
      const agentBalBefore = await getBalance(context, agent.publicKey);
      const treasuryBalBefore = await getBalance(context, treasury.publicKey);
      const clientBalBefore = await getBalance(context, client.publicKey);

      await program.methods
        .finalizeTask()
        .accountsPartial({
          task: setup.taskPda,
          globalState: globalPda,
          agent: agent.publicKey,
          client: client.publicKey,
          treasury: treasury.publicKey,
        })
        .rpc();

      // Task account should be closed (rent returned to client)
      const taskAccount = await context.banksClient.getAccount(setup.taskPda);
      assert.isNull(
        taskAccount,
        "Task account should be closed after finalize"
      );

      // Check balance changes
      const agentBalAfter = await getBalance(context, agent.publicKey);
      const treasuryBalAfter = await getBalance(context, treasury.publicKey);
      const clientBalAfter = await getBalance(context, client.publicKey);

      const [expectedPayment, expectedFee, expectedRemainder] =
        computeExpectedPayment(
          MAX_SCORE,
          QUALITY_THRESHOLD.toNumber(),
          ESCROW_LAMPORTS.toNumber(),
          PROTOCOL_FEE_BPS
        );

      // Agent should receive payment_amount
      const agentDelta = Number(agentBalAfter) - Number(agentBalBefore);
      assert.equal(
        agentDelta,
        expectedPayment,
        "Agent should receive payment_amount"
      );

      // Treasury should receive fee
      const treasuryDelta =
        Number(treasuryBalAfter) - Number(treasuryBalBefore);
      assert.equal(treasuryDelta, expectedFee, "Treasury should receive fee");

      // Client should receive remainder + rent (from account closure)
      const clientDelta = Number(clientBalAfter) - Number(clientBalBefore);
      assert.isTrue(
        clientDelta >= expectedRemainder,
        "Client should receive at least the remainder (plus rent from closure)"
      );
    });
  });

  // -------------------------------------------------------------------------
  // Test 2: Challenge — challenger wins
  // -------------------------------------------------------------------------

  describe("challenge flow: challenger wins", () => {
    let setup: TaskSetup;
    let submittedAt: number;

    it.skip("drives task to Verified state", async () => {
      const currentClock = await context.banksClient.getClock();
      const now = Number(currentClock.unixTimestamp);
      const deadline = new BN(now + 86_400 * 60);

      setup = await createTask(program, client, globalPda, deadline);
      await claimTask(program, agent, setup.taskPda);
      await submitWork(program, agent, setup.taskPda, "challenge-won-video");

      const task = await program.account.task.fetch(setup.taskPda);
      submittedAt = task.submittedAt.toNumber();

      // Warp to T+7d for verification
      await warpToTimestamp(context, submittedAt + SEVEN_DAYS_SECONDS);
      await verifyTask(
        program,
        authority,
        setup.taskPda,
        globalPda,
        new BN(800_000)
      );

      const verified = await program.account.task.fetch(setup.taskPda);
      assert.deepEqual(verified.state, { verified: {} });
    });

    it.skip("challenger posts bond and task becomes Disputed", async () => {
      const [challPda] = challengePda(
        setup.taskId,
        challenger.publicKey,
        program.programId
      );

      await program.methods
        .challengeTask()
        .accountsPartial({
          task: setup.taskPda,
          challenge: challPda,
          challenger: challenger.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([challenger])
        .rpc();

      const task = await program.account.task.fetch(setup.taskPda);
      assert.deepEqual(task.state, { disputed: {} });

      const challenge = await program.account.challenge.fetch(challPda);
      // Bond = MIN_CHALLENGE_BOND_MULTIPLIER * escrow = 2 * 1 SOL = 2 SOL
      const expectedBond =
        ESCROW_LAMPORTS.toNumber() * MIN_CHALLENGE_BOND_MULTIPLIER;
      assert.equal(
        challenge.bondLamports.toString(),
        expectedBond.toString(),
        "Bond should be 2x escrow"
      );
    });

    it.skip("authority resolves: challenger wins — escrow to client, bond returned", async () => {
      const [challPda] = challengePda(
        setup.taskId,
        challenger.publicKey,
        program.programId
      );

      const clientBalBefore = await getBalance(context, client.publicKey);
      const challengerBalBefore = await getBalance(
        context,
        challenger.publicKey
      );
      const agentBalBefore = await getBalance(context, agent.publicKey);

      await program.methods
        .resolveChallenge(true) // challenger_won = true
        .accountsPartial({
          task: setup.taskPda,
          challenge: challPda,
          globalState: globalPda,
          authority: authority.publicKey,
          agent: agent.publicKey,
          client: client.publicKey,
          challenger: challenger.publicKey,
          treasury: treasury.publicKey,
        })
        .signers([authority])
        .rpc();

      // Task and challenge accounts should be closed
      const taskAccount = await context.banksClient.getAccount(setup.taskPda);
      assert.isNull(taskAccount, "Task account should be closed");
      const challAccount = await context.banksClient.getAccount(challPda);
      assert.isNull(challAccount, "Challenge account should be closed");

      // Client gets escrow back + task rent
      const clientBalAfter = await getBalance(context, client.publicKey);
      const clientDelta = Number(clientBalAfter) - Number(clientBalBefore);
      assert.isTrue(
        clientDelta >= ESCROW_LAMPORTS.toNumber(),
        "Client should receive at least the escrow amount back"
      );

      // Challenger gets bond back + challenge rent
      const challengerBalAfter = await getBalance(
        context,
        challenger.publicKey
      );
      const challengerDelta =
        Number(challengerBalAfter) - Number(challengerBalBefore);
      const expectedBond =
        ESCROW_LAMPORTS.toNumber() * MIN_CHALLENGE_BOND_MULTIPLIER;
      assert.isTrue(
        challengerDelta >= expectedBond,
        "Challenger should receive at least the bond back"
      );

      // Agent gets nothing (balance unchanged)
      const agentBalAfter = await getBalance(context, agent.publicKey);
      const agentDelta = Number(agentBalAfter) - Number(agentBalBefore);
      assert.equal(
        agentDelta,
        0,
        "Agent should receive nothing when challenger wins"
      );
    });
  });

  // -------------------------------------------------------------------------
  // Test 3: Challenge — agent wins (bond slashed)
  // -------------------------------------------------------------------------

  describe("challenge flow: agent wins (bond slashed)", () => {
    let setup: TaskSetup;
    let submittedAt: number;
    const score = 800_000;

    it.skip("drives task to Verified state", async () => {
      const currentClock = await context.banksClient.getClock();
      const now = Number(currentClock.unixTimestamp);
      const deadline = new BN(now + 86_400 * 60);

      setup = await createTask(program, client, globalPda, deadline);
      await claimTask(program, agent, setup.taskPda);
      await submitWork(program, agent, setup.taskPda, "agent-wins-video");

      const task = await program.account.task.fetch(setup.taskPda);
      submittedAt = task.submittedAt.toNumber();

      await warpToTimestamp(context, submittedAt + SEVEN_DAYS_SECONDS);
      await verifyTask(
        program,
        authority,
        setup.taskPda,
        globalPda,
        new BN(score)
      );

      const verified = await program.account.task.fetch(setup.taskPda);
      assert.deepEqual(verified.state, { verified: {} });
    });

    it.skip("challenger posts bond", async () => {
      const [challPda] = challengePda(
        setup.taskId,
        challenger.publicKey,
        program.programId
      );

      await program.methods
        .challengeTask()
        .accountsPartial({
          task: setup.taskPda,
          challenge: challPda,
          challenger: challenger.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([challenger])
        .rpc();

      const task = await program.account.task.fetch(setup.taskPda);
      assert.deepEqual(task.state, { disputed: {} });
    });

    it.skip("authority resolves: agent wins — payment released, bond slashed 50/50", async () => {
      const [challPda] = challengePda(
        setup.taskId,
        challenger.publicKey,
        program.programId
      );

      const agentBalBefore = await getBalance(context, agent.publicKey);
      const treasuryBalBefore = await getBalance(context, treasury.publicKey);
      const clientBalBefore = await getBalance(context, client.publicKey);
      const challengerBalBefore = await getBalance(
        context,
        challenger.publicKey
      );

      await program.methods
        .resolveChallenge(false) // challenger_won = false => agent wins
        .accountsPartial({
          task: setup.taskPda,
          challenge: challPda,
          globalState: globalPda,
          authority: authority.publicKey,
          agent: agent.publicKey,
          client: client.publicKey,
          challenger: challenger.publicKey,
          treasury: treasury.publicKey,
        })
        .signers([authority])
        .rpc();

      // Accounts should be closed
      const taskAccount = await context.banksClient.getAccount(setup.taskPda);
      assert.isNull(taskAccount, "Task account should be closed");
      const challAccount = await context.banksClient.getAccount(challPda);
      assert.isNull(challAccount, "Challenge account should be closed");

      // Compute expected payment from escrow
      const [expectedPayment, expectedFee, expectedRemainder] =
        computeExpectedPayment(
          score,
          QUALITY_THRESHOLD.toNumber(),
          ESCROW_LAMPORTS.toNumber(),
          PROTOCOL_FEE_BPS
        );

      // Bond = 2 * escrow
      const bondLamports =
        ESCROW_LAMPORTS.toNumber() * MIN_CHALLENGE_BOND_MULTIPLIER;
      const bondHalf = Math.floor(bondLamports / 2);
      const bondOtherHalf = bondLamports - bondHalf;

      // Agent receives: payment from escrow + half of slashed bond
      const agentBalAfter = await getBalance(context, agent.publicKey);
      const agentDelta = Number(agentBalAfter) - Number(agentBalBefore);
      assert.equal(
        agentDelta,
        expectedPayment + bondHalf,
        "Agent should receive payment + half of slashed bond"
      );

      // Treasury receives: protocol fee from escrow + other half of slashed bond
      const treasuryBalAfter = await getBalance(context, treasury.publicKey);
      const treasuryDelta =
        Number(treasuryBalAfter) - Number(treasuryBalBefore);
      assert.equal(
        treasuryDelta,
        expectedFee + bondOtherHalf,
        "Treasury should receive fee + other half of slashed bond"
      );

      // Client receives: remainder of escrow + task rent (from close)
      const clientBalAfter = await getBalance(context, client.publicKey);
      const clientDelta = Number(clientBalAfter) - Number(clientBalBefore);
      assert.isTrue(
        clientDelta >= expectedRemainder,
        "Client should receive at least the escrow remainder (plus rent from closure)"
      );

      // Challenger gets nothing from escrow; challenge account rent returned by close
      const challengerBalAfter = await getBalance(
        context,
        challenger.publicKey
      );
      const challengerDelta =
        Number(challengerBalAfter) - Number(challengerBalBefore);
      // Challenger only gets back the challenge account rent (from `close = challenger`)
      // but the bond itself is slashed. So delta should be small (just rent).
      assert.isTrue(
        challengerDelta < bondLamports,
        "Challenger should NOT receive the full bond back"
      );
    });
  });

  // -------------------------------------------------------------------------
  // Test 4: Zero score — full escrow return to client
  // -------------------------------------------------------------------------

  describe("zero score: full escrow returned to client", () => {
    let setup: TaskSetup;
    let submittedAt: number;

    it.skip("drives task to Submitted, verifies with score=0", async () => {
      const currentClock = await context.banksClient.getClock();
      const now = Number(currentClock.unixTimestamp);
      const deadline = new BN(now + 86_400 * 60);

      setup = await createTask(program, client, globalPda, deadline);
      await claimTask(program, agent, setup.taskPda);
      await submitWork(program, agent, setup.taskPda, "zero-score-video");

      const task = await program.account.task.fetch(setup.taskPda);
      submittedAt = task.submittedAt.toNumber();

      // Warp to T+7d for verification
      await warpToTimestamp(context, submittedAt + SEVEN_DAYS_SECONDS);
      await verifyTask(program, authority, setup.taskPda, globalPda, new BN(0));

      const verified = await program.account.task.fetch(setup.taskPda);
      assert.deepEqual(verified.state, { verified: {} });
      assert.equal(verified.compositeScore.toString(), "0");
      assert.equal(verified.paymentAmount.toString(), "0");
    });

    it.skip("finalizes with zero payment — full escrow to client", async () => {
      const task = await program.account.task.fetch(setup.taskPda);
      const pastChallenge = task.challengeDeadline.toNumber() + 1;
      await warpToTimestamp(context, pastChallenge);

      const clientBalBefore = await getBalance(context, client.publicKey);
      const agentBalBefore = await getBalance(context, agent.publicKey);
      const treasuryBalBefore = await getBalance(context, treasury.publicKey);

      await program.methods
        .finalizeTask()
        .accountsPartial({
          task: setup.taskPda,
          globalState: globalPda,
          agent: agent.publicKey,
          client: client.publicKey,
          treasury: treasury.publicKey,
        })
        .rpc();

      // Task should be closed
      const taskAccount = await context.banksClient.getAccount(setup.taskPda);
      assert.isNull(taskAccount, "Task account should be closed");

      // Agent gets nothing
      const agentBalAfter = await getBalance(context, agent.publicKey);
      const agentDelta = Number(agentBalAfter) - Number(agentBalBefore);
      assert.equal(agentDelta, 0, "Agent should receive nothing on zero score");

      // Treasury gets nothing (fee=0 when payment=0)
      const treasuryBalAfter = await getBalance(context, treasury.publicKey);
      const treasuryDelta =
        Number(treasuryBalAfter) - Number(treasuryBalBefore);
      assert.equal(
        treasuryDelta,
        0,
        "Treasury should receive nothing on zero score"
      );

      // Client gets full escrow back + rent from closure
      const clientBalAfter = await getBalance(context, client.publicKey);
      const clientDelta = Number(clientBalAfter) - Number(clientBalBefore);
      assert.isTrue(
        clientDelta >= ESCROW_LAMPORTS.toNumber(),
        "Client should receive at least full escrow back"
      );
    });
  });

  // -------------------------------------------------------------------------
  // Test 5: close_agent_state
  // -------------------------------------------------------------------------

  describe("close_agent_state", () => {
    it("creates AgentState via claim, submits to decrement, then closes", async () => {
      const closeAgent = Keypair.generate();
      await fundAccount(provider, closeAgent.publicKey, 10 * LAMPORTS_PER_SOL);

      const currentClock = await context.banksClient.getClock();
      const now = Number(currentClock.unixTimestamp);
      const deadline = new BN(now + 86_400 * 60);

      // Create and claim a task (creates AgentState with claimed_count=1)
      const setup = await createTask(program, client, globalPda, deadline);
      await claimTask(program, closeAgent, setup.taskPda);

      const [closeAgentPda] = agentStatePda(
        closeAgent.publicKey,
        program.programId
      );

      // Verify claimed_count = 1
      let agentState = await program.account.agentState.fetch(closeAgentPda);
      assert.equal(
        agentState.claimedCount,
        1,
        "claimed_count should be 1 after claim"
      );

      // Submit work (decrements claimed_count to 0)
      await submitWork(program, closeAgent, setup.taskPda, "close-agent-video");

      agentState = await program.account.agentState.fetch(closeAgentPda);
      assert.equal(
        agentState.claimedCount,
        0,
        "claimed_count should be 0 after submit"
      );

      // Record agent balance before closing
      const agentBalBefore = await getBalance(context, closeAgent.publicKey);

      // Close agent state
      await program.methods
        .closeAgentState()
        .accountsPartial({
          agentState: closeAgentPda,
          agent: closeAgent.publicKey,
        })
        .signers([closeAgent])
        .rpc();

      // AgentState account should be closed
      const closedAccount = await context.banksClient.getAccount(closeAgentPda);
      assert.isNull(closedAccount, "AgentState account should be closed");

      // Agent should receive rent back
      const agentBalAfter = await getBalance(context, closeAgent.publicKey);
      assert.isTrue(
        Number(agentBalAfter) > Number(agentBalBefore),
        "Agent balance should increase from rent reclaim (minus tx fee)"
      );
    });
  });
});
