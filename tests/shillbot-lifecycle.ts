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

import { createMockPullFeedData } from "./helpers/mock-switchboard-feed";

// Import the IDL type — anchor-bankrun requires the JSON IDL
import { Shillbot } from "../target/types/shillbot";
const IDL = require("../target/idl/shillbot.json");

/// Switchboard On-Demand program ID (owner of feed accounts).
const SWITCHBOARD_PROGRAM_ID = new PublicKey(
  "Aio4gaXjXzJNVLtzwtNVmSqGKpANtXhybbkhtAC94ji2"
);

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

function clientStatePda(
  client: PublicKey,
  programId: PublicKey
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("client_state"), client.toBuffer()],
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
// Must match `programs/shillbot/src/constants.rs::SWITCHBOARD_FEED`.
// `setSwitchboardFeed` was removed in Phase 3 blocker #1 Path A;
// `initialize` now sets `global.switchboard_feed` from this const.
const DUMMY_SWITCHBOARD_FEED = new PublicKey(
  "11111111111111111111111111111112"
);

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

  // Phase 3 blocker #1 Path A removed `setSwitchboardFeed`. `initialize`
  // sets `global.switchboard_feed` to `constants::SWITCHBOARD_FEED`
  // (which equals `DUMMY_SWITCHBOARD_FEED` here). No further setup
  // needed for verify_task to pass the feed-account check.
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

  const [csPda] = clientStatePda(client.publicKey, program.programId);
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
      clientState: csPda,
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

// Phase 3 blocker #3a: client approval gate between Submitted and Verified.
// verify_task now requires Approved (was Submitted), so every test that
// reaches Verified must call this helper after submitWork.
async function approveTask(
  program: Program<Shillbot>,
  client: Keypair,
  taskPdaAddr: PublicKey
): Promise<void> {
  await program.methods
    .approveTask()
    .accountsPartial({
      task: taskPdaAddr,
      client: client.publicKey,
    })
    .signers([client])
    .rpc();
}

async function verifyTask(
  program: Program<Shillbot>,
  _authority: Keypair,
  taskPdaAddr: PublicKey,
  globalPda: PublicKey,
  compositeScore: BN,
  bankrunContext: Awaited<ReturnType<typeof startAnchor>>
): Promise<void> {
  // Inject a mock Switchboard feed account with the desired score.
  // bankrun lets us set arbitrary account data, bypassing the need for
  // a real Switchboard program on the local validator.
  const clock = await bankrunContext.banksClient.getClock();
  const currentSlot = clock.slot;
  const feedData = createMockPullFeedData(
    compositeScore.toNumber(),
    currentSlot
  );
  bankrunContext.setAccount(DUMMY_SWITCHBOARD_FEED, {
    lamports: LAMPORTS_PER_SOL,
    data: feedData,
    owner: SWITCHBOARD_PROGRAM_ID,
    executable: false,
  });

  const verificationHash = Array.from({ length: 32 }, (_, i) => i + 1);
  await program.methods
    .verifyTask(compositeScore, verificationHash)
    .accountsPartial({
      task: taskPdaAddr,
      globalState: globalPda,
      switchboardFeed: DUMMY_SWITCHBOARD_FEED,
    })
    .rpc();
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

    it("warps clock to T+7d and verifies with max score", async () => {
      // Warp to submitted_at + 7 days (within staleness window)
      const verifyTime = submittedAt + SEVEN_DAYS_SECONDS;
      await warpToTimestamp(context, verifyTime);

      // Client approves before oracle verification (Phase 3 blocker #3a gate)
      await approveTask(program, client, setup.taskPda);

      await verifyTask(
        program,
        authority,
        setup.taskPda,
        globalPda,
        new BN(MAX_SCORE),
        context
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

    it("warps past challenge deadline and finalizes", async () => {
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

    it("drives task to Verified state", async () => {
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
      await approveTask(program, client, setup.taskPda);
      await verifyTask(
        program,
        authority,
        setup.taskPda,
        globalPda,
        new BN(800_000),
        context
      );

      const verified = await program.account.task.fetch(setup.taskPda);
      assert.deepEqual(verified.state, { verified: {} });
    });

    it("challenger posts bond and task becomes Disputed", async () => {
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

    it("authority resolves: challenger wins — escrow to client, bond returned", async () => {
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

    it("drives task to Verified state", async () => {
      const currentClock = await context.banksClient.getClock();
      const now = Number(currentClock.unixTimestamp);
      const deadline = new BN(now + 86_400 * 60);

      setup = await createTask(program, client, globalPda, deadline);
      await claimTask(program, agent, setup.taskPda);
      await submitWork(program, agent, setup.taskPda, "agent-wins-video");

      const task = await program.account.task.fetch(setup.taskPda);
      submittedAt = task.submittedAt.toNumber();

      await warpToTimestamp(context, submittedAt + SEVEN_DAYS_SECONDS);
      await approveTask(program, client, setup.taskPda);
      await verifyTask(
        program,
        authority,
        setup.taskPda,
        globalPda,
        new BN(score),
        context
      );

      const verified = await program.account.task.fetch(setup.taskPda);
      assert.deepEqual(verified.state, { verified: {} });
    });

    it("challenger posts bond", async () => {
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

    it("authority resolves: agent wins — payment released, bond slashed 50/50", async () => {
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

    it("drives task to Submitted, verifies with score below threshold", async () => {
      const currentClock = await context.banksClient.getClock();
      const now = Number(currentClock.unixTimestamp);
      const deadline = new BN(now + 86_400 * 60);

      setup = await createTask(program, client, globalPda, deadline);
      await claimTask(program, agent, setup.taskPda);
      await submitWork(program, agent, setup.taskPda, "zero-score-video");

      const task = await program.account.task.fetch(setup.taskPda);
      submittedAt = task.submittedAt.toNumber();

      // Warp to T+7d for verification. Use score=1 (below quality_threshold
      // of 200k) — Switchboard's get_value() requires positive values, so 0
      // is not valid. Payment is still $0 because score < threshold.
      await warpToTimestamp(context, submittedAt + SEVEN_DAYS_SECONDS);
      await approveTask(program, client, setup.taskPda);
      await verifyTask(
        program,
        authority,
        setup.taskPda,
        globalPda,
        new BN(1),
        context
      );

      const verified = await program.account.task.fetch(setup.taskPda);
      assert.deepEqual(verified.state, { verified: {} });
      assert.equal(verified.compositeScore.toString(), "1");
      assert.equal(verified.paymentAmount.toString(), "0");
    });

    it("finalizes with zero payment — full escrow to client", async () => {
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

  // -------------------------------------------------------------------------
  // Phase 3 blocker #2: MIN_ESCROW + per-client rate limit gates
  // -------------------------------------------------------------------------

  describe("Phase 3 blocker #2 economic gates", () => {
    // Must match `programs/shillbot/src/constants.rs::MIN_ESCROW_LAMPORTS`.
    const MIN_ESCROW_LAMPORTS = new BN(360_000_000); // 0.36 SOL
    const RATE_LIMIT_WINDOW_SECONDS = 3_600;
    const MAX_TASKS_PER_RATE_WINDOW = 10;

    /** Direct createTask invocation that lets us parametrize escrow + content. */
    async function createTaskRaw(
      c: Keypair,
      escrow: BN,
      contentTag: string,
      deadlineSec: BN
    ): Promise<PublicKey> {
      const global = await program.account.globalState.fetch(globalPda);
      const [tPda] = taskPda(
        global.taskCounter,
        c.publicKey,
        program.programId
      );
      const [csPda] = clientStatePda(c.publicKey, program.programId);
      await program.methods
        .createTask(
          escrow,
          contentHash(contentTag) as any,
          deadlineSec,
          new BN(3600),
          new BN(14_400),
          0,
          0,
          0,
          0
        )
        .accountsPartial({
          globalState: globalPda,
          task: tPda,
          clientState: csPda,
          client: c.publicKey,
          slotHashes: SYSVAR_SLOT_HASHES_PUBKEY,
          systemProgram: SystemProgram.programId,
        })
        .signers([c])
        .rpc();
      return tPda;
    }

    async function freshClient(): Promise<{ kp: Keypair; deadline: BN }> {
      const kp = Keypair.generate();
      await fundAccount(provider, kp.publicKey, 100 * LAMPORTS_PER_SOL);
      const clock = await context.banksClient.getClock();
      const deadline = new BN(Number(clock.unixTimestamp) + 86_400 * 60);
      return { kp, deadline };
    }

    it("rejects createTask with escrow < MIN_ESCROW_LAMPORTS", async () => {
      const { kp, deadline } = await freshClient();
      try {
        await createTaskRaw(
          kp,
          MIN_ESCROW_LAMPORTS.sub(new BN(1)), // 1 lamport below floor
          "below-min",
          deadline
        );
        assert.fail("expected EscrowBelowMinimum, got success");
      } catch (e: any) {
        const msg = String(e);
        assert.match(
          msg,
          /EscrowBelowMinimum/,
          `expected EscrowBelowMinimum, got: ${msg}`
        );
      }
    });

    it("succeeds with escrow == MIN_ESCROW_LAMPORTS (boundary)", async () => {
      const { kp, deadline } = await freshClient();
      await createTaskRaw(kp, MIN_ESCROW_LAMPORTS, "at-min", deadline);
      const [csPda] = clientStatePda(kp.publicKey, program.programId);
      const cs = await program.account.clientState.fetch(csPda);
      assert.equal(cs.tasksInWindow, 1, "first task in window sets count to 1");
      assert.equal(cs.totalTasksCreated.toString(), "1");
      assert.equal(cs.client.toBase58(), kp.publicKey.toBase58());
    });

    it("rejects 11th createTask within rate-limit window", async () => {
      const { kp, deadline } = await freshClient();
      // Fire MAX_TASKS_PER_RATE_WINDOW (10) successful calls.
      for (let i = 0; i < MAX_TASKS_PER_RATE_WINDOW; i++) {
        await createTaskRaw(kp, MIN_ESCROW_LAMPORTS, `rl-${i}`, deadline);
      }
      const [csPda] = clientStatePda(kp.publicKey, program.programId);
      const cs = await program.account.clientState.fetch(csPda);
      assert.equal(
        cs.tasksInWindow,
        MAX_TASKS_PER_RATE_WINDOW,
        "10 tasks should fill the window exactly"
      );
      // 11th must fail.
      try {
        await createTaskRaw(kp, MIN_ESCROW_LAMPORTS, "rl-overflow", deadline);
        assert.fail("expected RateLimitExceeded, got success");
      } catch (e: any) {
        const msg = String(e);
        assert.match(
          msg,
          /RateLimitExceeded/,
          `expected RateLimitExceeded, got: ${msg}`
        );
      }
    });

    it("resets window after RATE_LIMIT_WINDOW_SECONDS; total_tasks_created is monotonic", async () => {
      const { kp, deadline } = await freshClient();
      // Fill the window.
      for (let i = 0; i < MAX_TASKS_PER_RATE_WINDOW; i++) {
        await createTaskRaw(kp, MIN_ESCROW_LAMPORTS, `wreset-${i}`, deadline);
      }
      const [csPda] = clientStatePda(kp.publicKey, program.programId);
      const cs10 = await program.account.clientState.fetch(csPda);
      const windowStart = cs10.windowStartTs.toNumber();

      // Warp 1s past the window.
      await warpToTimestamp(
        context,
        windowStart + RATE_LIMIT_WINDOW_SECONDS + 1
      );

      // 11th should now succeed (new window).
      await createTaskRaw(kp, MIN_ESCROW_LAMPORTS, "wreset-after", deadline);
      const cs11 = await program.account.clientState.fetch(csPda);
      assert.equal(
        cs11.tasksInWindow,
        1,
        "tasks_in_window resets to 1 in fresh window"
      );
      assert.equal(
        cs11.totalTasksCreated.toString(),
        "11",
        "total_tasks_created is monotonic across window resets"
      );
      assert.isTrue(
        cs11.windowStartTs.toNumber() > windowStart,
        "window_start_ts advances on reset"
      );
    });
  });

  // -------------------------------------------------------------------------
  // Phase 3 blocker #3a: client approval gate between Submitted and Verified
  //
  // Behavior coverage for the new `approve_task` instruction. The Reviewer
  // flagged that the wiring-only test additions (approveTask helper inserted
  // between submitWork and verifyTask everywhere) didn't catch:
  //
  //   1. NotTaskClient: arbitrary signer cannot approve someone else's task.
  //   2. InvalidTaskState: approve_task only valid from Submitted (not Open
  //      or already-Approved).
  //   3. The new verify_task gate: rejects state == Submitted (must be
  //      Approved now).
  //   4. Freeze-attack defense: the verification timeout is anchored on
  //      submitted_at, NOT approved_at — a client who approves and then
  //      never funds oracle verification still has the escrow returned at
  //      T+verification_timeout.
  // -------------------------------------------------------------------------

  describe("Phase 3 blocker #3a approve gate", () => {
    // Must match `programs/shillbot::DEFAULT_VERIFICATION_TIMEOUT_SECONDS`.
    const VERIFICATION_TIMEOUT_SECONDS = 1_209_600; // 14 days
    const MIN_ESCROW_LAMPORTS = new BN(360_000_000); // 0.36 SOL

    /** Fresh client + agent + funded task, returned at the requested state. */
    async function freshTask(): Promise<{
      taskPda: PublicKey;
      cKp: Keypair;
      aKp: Keypair;
    }> {
      const cKp = Keypair.generate();
      const aKp = Keypair.generate();
      await fundAccount(provider, cKp.publicKey, 50 * LAMPORTS_PER_SOL);
      await fundAccount(provider, aKp.publicKey, 10 * LAMPORTS_PER_SOL);
      const clock = await context.banksClient.getClock();
      const deadline = new BN(Number(clock.unixTimestamp) + 86_400 * 60);

      const global = await program.account.globalState.fetch(globalPda);
      const [tPda] = taskPda(
        global.taskCounter,
        cKp.publicKey,
        program.programId
      );
      const [csPda] = clientStatePda(cKp.publicKey, program.programId);

      await program.methods
        .createTask(
          MIN_ESCROW_LAMPORTS,
          contentHash("approve-gate-" + global.taskCounter.toString()) as any,
          deadline,
          new BN(3600),
          new BN(14_400),
          0,
          0,
          0,
          0
        )
        .accountsPartial({
          globalState: globalPda,
          task: tPda,
          clientState: csPda,
          client: cKp.publicKey,
          slotHashes: SYSVAR_SLOT_HASHES_PUBKEY,
          systemProgram: SystemProgram.programId,
        })
        .signers([cKp])
        .rpc();

      return { taskPda: tPda, cKp, aKp };
    }

    it("approve_task rejects when caller is not the original client (NotTaskClient)", async () => {
      const { taskPda: tPda, cKp, aKp } = await freshTask();
      await claimTask(program, aKp, tPda);
      await submitWork(program, aKp, tPda, "wrong-signer");

      const imposter = Keypair.generate();
      await fundAccount(provider, imposter.publicKey, 1 * LAMPORTS_PER_SOL);

      try {
        await program.methods
          .approveTask()
          .accountsPartial({
            task: tPda,
            client: imposter.publicKey,
          })
          .signers([imposter])
          .rpc();
        assert.fail("expected NotTaskClient, got success");
      } catch (e: any) {
        const msg = String(e);
        assert.match(
          msg,
          /NotTaskClient/,
          `expected NotTaskClient, got: ${msg}`
        );
      }

      // Sanity: real client can still approve afterwards.
      await approveTask(program, cKp, tPda);
      const t = await program.account.task.fetch(tPda);
      assert.deepEqual(t.state, { approved: {} });
    });

    it("approve_task rejects when state is Open (not yet Submitted)", async () => {
      const { taskPda: tPda, cKp } = await freshTask();
      // Skip claim + submit — leave in Open state.
      try {
        await approveTask(program, cKp, tPda);
        assert.fail("expected InvalidTaskState, got success");
      } catch (e: any) {
        const msg = String(e);
        assert.match(
          msg,
          /InvalidTaskState/,
          `expected InvalidTaskState, got: ${msg}`
        );
      }
    });

    it("approve_task is non-idempotent: second call on already-Approved task rejects", async () => {
      const { taskPda: tPda, cKp, aKp } = await freshTask();
      await claimTask(program, aKp, tPda);
      await submitWork(program, aKp, tPda, "double-approve");
      await approveTask(program, cKp, tPda);

      try {
        await approveTask(program, cKp, tPda);
        assert.fail("expected InvalidTaskState on double-approve, got success");
      } catch (e: any) {
        const msg = String(e);
        assert.match(
          msg,
          /InvalidTaskState/,
          `expected InvalidTaskState, got: ${msg}`
        );
      }
    });

    it("verify_task rejects state == Submitted (must be Approved post-#3a)", async () => {
      const { taskPda: tPda, cKp, aKp } = await freshTask();
      await claimTask(program, aKp, tPda);
      await submitWork(program, aKp, tPda, "skip-approve");

      const t = await program.account.task.fetch(tPda);
      const submittedAt = t.submittedAt.toNumber();
      // Warp into the staleness window so that the InvalidTaskState reject
      // fires BEFORE the staleness check (which would also fail otherwise
      // and mask the regression we're guarding against).
      await warpToTimestamp(context, submittedAt + SEVEN_DAYS_SECONDS);

      try {
        await verifyTask(
          program,
          authority,
          tPda,
          globalPda,
          new BN(800_000),
          context
        );
        assert.fail("expected InvalidTaskState, got success");
      } catch (e: any) {
        const msg = String(e);
        // approve_task gate must reject Submitted state with InvalidTaskState
        // — NOT AttestationStale or any Switchboard error.
        assert.match(
          msg,
          /InvalidTaskState/,
          `expected InvalidTaskState (post-#3a verify gate), got: ${msg}`
        );
      }
    });

    it("freeze-attack defense: timeout is anchored on submitted_at, not approved_at", async () => {
      // The freeze attack: client approves, then never funds oracle
      // verification. If the verification timeout were anchored on
      // approved_at, the client could indefinitely freeze the agent's
      // escrow + claim slot by approving at the last possible moment.
      // The timeout is anchored on submitted_at to defeat this.
      //
      // This test exercises the defense: approve immediately after
      // submitting, then warp to T = submitted_at + verification_timeout
      // + 1, then expire_task must succeed (returning escrow to client).
      const { taskPda: tPda, cKp, aKp } = await freshTask();
      await claimTask(program, aKp, tPda);
      await submitWork(program, aKp, tPda, "freeze-attack");
      await approveTask(program, cKp, tPda);

      const t = await program.account.task.fetch(tPda);
      assert.deepEqual(t.state, { approved: {} });
      const submittedAt = t.submittedAt.toNumber();

      // Warp to past submitted_at + verification_timeout (NOT
      // approved_at + verification_timeout). If a future contributor
      // re-anchors the timeout on approved_at, this assertion fails.
      const expiryTs = submittedAt + VERIFICATION_TIMEOUT_SECONDS + 1;
      await warpToTimestamp(context, expiryTs);

      const clientBalBefore = await getBalance(context, cKp.publicKey);
      const [agentPda] = agentStatePda(aKp.publicKey, program.programId);

      await program.methods
        .expireTask()
        .accountsPartial({
          task: tPda,
          client: cKp.publicKey,
        })
        .remainingAccounts([
          { pubkey: agentPda, isWritable: true, isSigner: false },
        ])
        .rpc();

      // Task account closed.
      const taskAcct = await context.banksClient.getAccount(tPda);
      assert.isNull(taskAcct, "Task account should be closed after expire");

      // Client receives at least the escrow back (plus rent from closure).
      const clientBalAfter = await getBalance(context, cKp.publicKey);
      const delta = Number(clientBalAfter) - Number(clientBalBefore);
      assert.isTrue(
        delta >= MIN_ESCROW_LAMPORTS.toNumber(),
        `client should receive at least the escrow back; got delta=${delta}`
      );
    });
  });
});
