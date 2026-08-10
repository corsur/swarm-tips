// Composable Shillbot task-lifecycle steps for the harness — the shillbot twin
// of game-steps.ts. Each step performs ONE action through a SolanaRuntime<Shillbot>
// and asserts its state-machine transition against TASK_GRAPH; scenarios compose
// them and run the property battery over the realized Ledger. The same step code
// drives bankrun (shillbot-bankrun.ts) and, in P3, a validator/devnet runtime —
// which is what makes a task scenario portable and metamorphically checkable.
//
// State machine (mirrors programs/shillbot/CLAUDE.md):
//
//   Open ─claim→ Claimed ─submit→ Submitted ─approve→ Approved
//        │                    │                  │
//        │                    ├── verify (attested, requires_approval=false) ──┐
//        │                    └── (approved) ── verify ───────────────────────►Verified
//        └─────────── expire (any pre-terminal) ──────────► [Expired, closed]
//   Verified ─finalize→ [Finalized, closed]
//   Verified ─challenge→ Disputed ─resolve / resolve_default→ [Resolved, closed]

import { BN, Program } from "@coral-xyz/anchor";
import {
  ComputeBudgetProgram,
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  SystemProgram,
  SYSVAR_SLOT_HASHES_PUBKEY,
} from "@solana/web3.js";
import { createHash } from "crypto";
import { Shillbot } from "../../target/types/shillbot";
import { SolanaRuntime } from "./target";
import {
  StateView,
  TransitionGraph,
  assertLegalTransition,
} from "./assertions";
import type { TaskPayout } from "../../sdk/task-outcome-oracle";

// Switchboard On-Demand program id (owner of feed accounts) and the fixed dummy
// feed pubkey the program's `initialize` records — shared with shillbot-bankrun.ts.
export const SWITCHBOARD_PROGRAM_ID = new PublicKey(
  "Aio4gaXjXzJNVLtzwtNVmSqGKpANtXhybbkhtAC94ji2"
);
export const DUMMY_SWITCHBOARD_FEED = new PublicKey(
  "11111111111111111111111111111112"
);

export const LEAN_PROOF_PLATFORM = 10;

/** Allowed task status transitions. Terminals (Finalized/Resolved/Expired) are absorbing. */
export const TASK_GRAPH: TransitionGraph = {
  Open: ["Claimed", "Expired"],
  Claimed: ["Submitted", "Expired"],
  // verify_task_attested (requires_approval=false) goes Submitted→Verified directly;
  // the oracle path goes through Approved.
  Submitted: ["Approved", "Verified", "Expired"],
  Approved: ["Verified", "Expired"],
  Verified: ["Finalized", "Disputed"],
  Disputed: ["Resolved"],
  Finalized: [],
  Resolved: [],
  Expired: [],
};

// --- PDA derivers (single copy — replaces the triplication across the 3 suites) -

export function contentHash(data: string): number[] {
  return Array.from(createHash("sha256").update(data).digest());
}

export function globalStatePda(programId: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("shillbot_global")],
    programId
  )[0];
}

function u64le(n: BN): Buffer {
  return n.toArrayLike(Buffer, "le", 8);
}

export function taskPda(
  taskCounter: BN,
  client: PublicKey,
  programId: PublicKey
): PublicKey {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("task"), u64le(taskCounter), client.toBuffer()],
    programId
  )[0];
}

export function challengePda(
  taskId: BN,
  challenger: PublicKey,
  programId: PublicKey
): PublicKey {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("challenge"), u64le(taskId), challenger.toBuffer()],
    programId
  )[0];
}

export function agentStatePda(
  agent: PublicKey,
  programId: PublicKey
): PublicKey {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("agent_state"), agent.toBuffer()],
    programId
  )[0];
}

export function clientStatePda(
  client: PublicKey,
  programId: PublicKey
): PublicKey {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("client_state"), client.toBuffer()],
    programId
  )[0];
}

/** Anchor enum object ({ verified: {} }) → canonical label ("Verified"). */
export function taskStatusLabel(state: Record<string, unknown>): string {
  const key = Object.keys(state)[0];
  return key.charAt(0).toUpperCase() + key.slice(1);
}

// --- Context ---------------------------------------------------------------

export interface ShillbotCtx {
  rt: SolanaRuntime<Shillbot>;
  globalPda: PublicKey;
  /** Recorded in GlobalState; distinct from the runtime payer so payout
   *  conservation ledgers don't conflate a fee with gas. */
  treasury: PublicKey;
  /** Both GlobalState.authority AND the initial oracle_authority (attester). */
  authority: Keypair;
  /** Prime the oracle so the next kind-0 `verify_task` reads a chosen score.
   *  bankrun injects a mock feed; a devnet runtime relies on the real crank. */
  primeFeed?: (score: number) => Promise<void>;
}

export interface ShillbotSetup {
  authority: Keypair;
  treasury: PublicKey;
  protocolFeeBps?: number;
  qualityThreshold?: BN;
  startingCounter?: BN;
  primeFeed?: (score: number) => Promise<void>;
}

/** Idempotent singleton init: create GlobalState with the given params. */
export async function setupShillbot(
  rt: SolanaRuntime<Shillbot>,
  s: ShillbotSetup
): Promise<ShillbotCtx> {
  const globalPda = globalStatePda(rt.program.programId);
  await rt.program.methods
    .initialize(
      s.protocolFeeBps ?? 1000,
      s.qualityThreshold ?? new BN(200_000),
      s.startingCounter ?? new BN(0),
      DUMMY_SWITCHBOARD_FEED
    )
    .accountsPartial({
      globalState: globalPda,
      authority: s.authority.publicKey,
      treasury: s.treasury,
      systemProgram: SystemProgram.programId,
    })
    .signers([s.authority])
    .rpc();
  return {
    rt,
    globalPda,
    treasury: s.treasury,
    authority: s.authority,
    primeFeed: s.primeFeed,
  };
}

// --- Step helpers ----------------------------------------------------------

async function fetchTask(ctx: ShillbotCtx, task: PublicKey) {
  return ctx.rt.program.account.task.fetch(task);
}

async function statusOf(ctx: ShillbotCtx, task: PublicKey): Promise<string> {
  const t = await fetchTask(ctx, task);
  return taskStatusLabel(t.state as Record<string, unknown>);
}

async function assertClosedTransition(
  ctx: ShillbotCtx,
  task: PublicKey,
  before: string,
  terminal: "Finalized" | "Resolved" | "Expired"
): Promise<void> {
  assertLegalTransition(TASK_GRAPH, before, terminal);
  const acct = await ctx.rt.getBalance(task);
  if (acct !== 0n) {
    throw new Error(`${terminal}: task account expected closed, still funded`);
  }
}

export interface CreateTaskOpts {
  escrowLamports: BN;
  platform?: number;
  verificationKind?: number;
  requiresApproval?: boolean;
  challengeWindowOverride?: number;
  deadlineSeconds?: number;
  submitMargin?: number;
  claimBuffer?: number;
  attestationDelayOverride?: number;
  verificationTimeoutOverride?: number;
  contentTag?: string;
}

export interface CreatedTask {
  task: PublicKey;
  taskId: BN;
  contentTag: string;
}

/** create_task (client signs): funds escrow, task enters Open. */
export async function createTask(
  ctx: ShillbotCtx,
  client: Keypair,
  opts: CreateTaskOpts
): Promise<CreatedTask> {
  const global = await ctx.rt.program.account.globalState.fetch(ctx.globalPda);
  const taskId = global.taskCounter as BN;
  const task = taskPda(taskId, client.publicKey, ctx.rt.program.programId);
  const contentTag = opts.contentTag ?? `task-${taskId.toString()}`;
  const now = await ctx.rt.now();
  const deadline = new BN(now + (opts.deadlineSeconds ?? 86_400 * 60));

  await ctx.rt.program.methods
    .createTask(
      opts.escrowLamports,
      contentHash(contentTag) as never,
      deadline,
      new BN(opts.submitMargin ?? 3600),
      new BN(opts.claimBuffer ?? 14_400),
      opts.platform ?? 0,
      opts.attestationDelayOverride ?? 0,
      opts.challengeWindowOverride ?? 0,
      opts.verificationTimeoutOverride ?? 0,
      opts.requiresApproval ?? true,
      opts.verificationKind ?? 0
    )
    .accountsPartial({
      globalState: ctx.globalPda,
      task,
      clientState: clientStatePda(client.publicKey, ctx.rt.program.programId),
      client: client.publicKey,
      slotHashes: SYSVAR_SLOT_HASHES_PUBKEY,
      systemProgram: SystemProgram.programId,
    })
    .signers([client])
    .rpc();
  return { task, taskId, contentTag };
}

/** claim_task (agent signs): Open → Claimed. */
export async function claimTask(
  ctx: ShillbotCtx,
  agent: Keypair,
  task: PublicKey
): Promise<void> {
  const before = await statusOf(ctx, task);
  await ctx.rt.program.methods
    .claimTask()
    .accountsPartial({
      task,
      agentState: agentStatePda(agent.publicKey, ctx.rt.program.programId),
      agent: agent.publicKey,
      systemProgram: SystemProgram.programId,
    })
    .signers([agent])
    .rpc();
  assertLegalTransition(TASK_GRAPH, before, await statusOf(ctx, task));
}

/** submit_work (agent signs): Claimed → Submitted. */
export async function submitWork(
  ctx: ShillbotCtx,
  agent: Keypair,
  task: PublicKey,
  contentId: string
): Promise<void> {
  const before = await statusOf(ctx, task);
  await ctx.rt.program.methods
    .submitWork(Buffer.from(contentId))
    .accountsPartial({
      task,
      agentState: agentStatePda(agent.publicKey, ctx.rt.program.programId),
      agent: agent.publicKey,
    })
    .signers([agent])
    .rpc();
  assertLegalTransition(TASK_GRAPH, before, await statusOf(ctx, task));
}

/** approve_task (client signs): Submitted → Approved. */
export async function approveTask(
  ctx: ShillbotCtx,
  client: Keypair,
  task: PublicKey
): Promise<void> {
  const before = await statusOf(ctx, task);
  await ctx.rt.program.methods
    .approveTask()
    .accountsPartial({ task, client: client.publicKey })
    .signers([client])
    .rpc();
  assertLegalTransition(TASK_GRAPH, before, await statusOf(ctx, task));
}

/** verify_task (kind 0, Switchboard): Approved/Submitted → Verified.
 *  Requires the oracle feed primed to `score` (bankrun injects; devnet cranks). */
export async function verifyTaskOracle(
  ctx: ShillbotCtx,
  task: PublicKey,
  score: number
): Promise<void> {
  if (!ctx.primeFeed) {
    throw new Error(
      "verifyTaskOracle: ctx.primeFeed not set (kind-0 needs a feed)"
    );
  }
  await ctx.primeFeed(score);
  const before = await statusOf(ctx, task);
  const verificationHash = Array.from({ length: 32 }, (_, i) => i + 1);
  await ctx.rt.program.methods
    .verifyTask(new BN(score), verificationHash as never)
    .accountsPartial({
      task,
      globalState: ctx.globalPda,
      switchboardFeed: DUMMY_SWITCHBOARD_FEED,
    })
    .rpc();
  assertLegalTransition(TASK_GRAPH, before, await statusOf(ctx, task));
}

/** verify_task_attested (kind 1, attester signs): Submitted/Approved → Verified.
 *  `score` must be 0 or MAX_SCORE; verification_hash is the AttestationCert digest
 *  (any non-zero 32 bytes here — the program stores it opaquely). */
export async function verifyTaskAttested(
  ctx: ShillbotCtx,
  task: PublicKey,
  taskId: BN,
  score: number,
  attester: Keypair = ctx.authority
): Promise<void> {
  const before = await statusOf(ctx, task);
  const hash = contentHash(`attestation ${taskId.toString()} ${score}`);
  await ctx.rt.program.methods
    .verifyTaskAttested(new BN(score), hash as never)
    .accountsPartial({
      task,
      globalState: ctx.globalPda,
      attester: attester.publicKey,
    })
    .signers([attester])
    .rpc();
  assertLegalTransition(TASK_GRAPH, before, await statusOf(ctx, task));
}

/** challenge_task (challenger signs): Verified → Disputed. Returns the challenge PDA. */
export async function challengeTask(
  ctx: ShillbotCtx,
  challenger: Keypair,
  task: PublicKey,
  taskId: BN
): Promise<PublicKey> {
  const before = await statusOf(ctx, task);
  const challenge = challengePda(
    taskId,
    challenger.publicKey,
    ctx.rt.program.programId
  );
  await ctx.rt.program.methods
    .challengeTask()
    .accountsPartial({
      task,
      challenge,
      challenger: challenger.publicKey,
      systemProgram: SystemProgram.programId,
    })
    .signers([challenger])
    .rpc();
  assertLegalTransition(TASK_GRAPH, before, await statusOf(ctx, task));
  return challenge;
}

/** finalize_task (permissionless): Verified → Finalized (closed). */
export async function finalizeTask(
  ctx: ShillbotCtx,
  task: PublicKey,
  agent: PublicKey,
  client: PublicKey
): Promise<void> {
  const before = await statusOf(ctx, task);
  await ctx.rt.program.methods
    .finalizeTask()
    .accountsPartial({
      task,
      globalState: ctx.globalPda,
      agent,
      client,
      treasury: ctx.treasury,
    })
    .remainingAccounts([
      {
        pubkey: agentStatePda(agent, ctx.rt.program.programId),
        isWritable: true,
        isSigner: false,
      },
    ])
    .rpc();
  await assertClosedTransition(ctx, task, before, "Finalized");
}

/** resolve_challenge (authority signs): Disputed → Resolved (closed). */
export async function resolveChallenge(
  ctx: ShillbotCtx,
  task: PublicKey,
  challenge: PublicKey,
  agent: PublicKey,
  client: PublicKey,
  challenger: PublicKey,
  challengerWon: boolean
): Promise<void> {
  const before = await statusOf(ctx, task);
  await ctx.rt.program.methods
    .resolveChallenge(challengerWon)
    .accountsPartial({
      task,
      challenge,
      globalState: ctx.globalPda,
      authority: ctx.authority.publicKey,
      agent,
      client,
      challenger,
      treasury: ctx.treasury,
    })
    .remainingAccounts([
      {
        pubkey: agentStatePda(agent, ctx.rt.program.programId),
        isWritable: true,
        isSigner: false,
      },
    ])
    .signers([ctx.authority])
    .rpc();
  await assertClosedTransition(ctx, task, before, "Resolved");
}

// A per-call compute-budget nonce keeps repeated permissionless cranks from
// colliding on bankrun's static blockhash (which surfaces as "already processed").
let defaultResolveNonce = 400_000;

/** resolve_challenge_default (permissionless liveness crank): Disputed → Resolved. */
export async function resolveChallengeDefault(
  ctx: ShillbotCtx,
  task: PublicKey,
  challenge: PublicKey,
  agent: PublicKey,
  client: PublicKey,
  challenger: PublicKey
): Promise<void> {
  const before = await statusOf(ctx, task);
  await ctx.rt.program.methods
    .resolveChallengeDefault()
    .preInstructions([
      ComputeBudgetProgram.setComputeUnitLimit({
        units: defaultResolveNonce++,
      }),
    ])
    .accountsPartial({
      task,
      challenge,
      globalState: ctx.globalPda,
      agent,
      client,
      challenger,
      treasury: ctx.treasury,
    })
    .rpc();
  await assertClosedTransition(ctx, task, before, "Resolved");
}

/** expire_task (permissionless): any pre-terminal → Expired (closed), escrow refunded. */
export async function expireTask(
  ctx: ShillbotCtx,
  task: PublicKey,
  agent: PublicKey,
  client: PublicKey
): Promise<void> {
  const before = await statusOf(ctx, task);
  await ctx.rt.program.methods
    .expireTask()
    .accountsPartial({ task, client })
    .remainingAccounts([
      {
        pubkey: agentStatePda(agent, ctx.rt.program.programId),
        isWritable: true,
        isSigner: false,
      },
    ])
    .rpc();
  await assertClosedTransition(ctx, task, before, "Expired");
}

/** Read a verified task's challenge deadline (for the boundary battery). */
export async function challengeDeadline(
  ctx: ShillbotCtx,
  task: PublicKey
): Promise<number> {
  return (await fetchTask(ctx, task)).challengeDeadline.toNumber();
}

/** A StateView over a task for the property battery. Once the task account is
 *  closed (a terminal crank ran) readStatus returns "Closed" — the realized
 *  outcome is verified through the Ledger + deriveTaskOutcome, not a dead
 *  account (shillbot closes on terminal, unlike the game). */
export function taskView(ctx: ShillbotCtx, task: PublicKey): StateView {
  return {
    async readStatus() {
      const bal = await ctx.rt.getBalance(task);
      if (bal === 0n) return "Closed";
      return statusOf(ctx, task);
    },
    async readOutcomeKind() {
      return -1;
    },
  };
}

export const _fundLamports = 100 * LAMPORTS_PER_SOL;

// ---------------------------------------------------------------------------
// Payout assertion — the shared "was the agent actually PAID?" check.
//
// WHY THIS IS SHARED AND WHY IT EXISTS. A task whose work was REJECTED scores
// 0, refunds the client, and STILL reaches Finalized. So `assertClosedTransition
// (ctx, task, before, "Finalized")` — what finalizeTask asserts on its own — is
// satisfied identically by a paid task and an unpaid one. Every cell that wanted
// more than that inlined its own balance-delta check, and the ones that didn't
// silently accepted rejections as success.
//
// The reader is INJECTED rather than taken from ShillbotCtx because the balance
// source differs per runtime: bankrun reads through its BanksClient, devnet
// through a web3 Connection. That seam is the only reason this could not simply
// live behind `ctx`.
// ---------------------------------------------------------------------------

/** Reads a lamport balance for the runtime under test. */
export type BalanceReader = (pk: PublicKey) => Promise<bigint>;

/** Who the terminal action is expected to move lamports to. */
export interface PayoutParties {
  agent: PublicKey;
  /** Only for challenge-resolution cells; omit for the plain finalize path. */
  challenger?: PublicKey;
}

/**
 * Run `action` and assert the lamports it moved match the oracle.
 *
 * The agent delta is EXACT: the agent neither signs the terminal transaction
 * nor receives the closed account's rent, so nothing else perturbs its balance.
 * The challenger is a floor (`>=`) because it may also be the payer.
 *
 * Lifted verbatim in behaviour from scripts/e2e/matrix-devnet.ts, where it was
 * module-private and therefore unavailable to the five other cells that needed
 * exactly this.
 */
export async function assertTerminalPayout(
  readBalance: BalanceReader,
  parties: PayoutParties,
  expected: Pick<TaskPayout, "agentLamports" | "challengerLamports">,
  label: string,
  check: (ok: boolean, msg: string) => void,
  action: () => Promise<void>
): Promise<void> {
  const agentBefore = await readBalance(parties.agent);
  const challengerBefore = parties.challenger
    ? await readBalance(parties.challenger)
    : 0n;

  await action();

  const agentDelta = (await readBalance(parties.agent)) - agentBefore;
  check(
    agentDelta === expected.agentLamports,
    `${label}: agent delta ${agentDelta} == oracle ${expected.agentLamports}`
  );

  if (expected.challengerLamports > 0n && parties.challenger) {
    const challengerDelta =
      (await readBalance(parties.challenger)) - challengerBefore;
    check(
      challengerDelta >= expected.challengerLamports,
      `${label}: challenger delta ${challengerDelta} >= bond ${expected.challengerLamports}`
    );
  }
}
