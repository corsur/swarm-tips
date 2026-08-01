/**
 * LIVE Base Sepolia OUTCOME/SCORE MATRIX for ShillbotEscrow — the full
 * verification-kind × score × terminal-outcome axis, driven against the real
 * deployed contract (default 0xaFe061778f9A76fCe7da4124dC89DAF8309E5F3c). This
 * is the EVM counterpart of scripts/e2e/matrix-devnet.ts: every cell reaches a
 * terminal state on-chain and asserts the REALIZED pull-payment credits
 * (withdrawable deltas) EXACTLY against sdk/task-outcome-oracle.deriveTaskOutcome.
 *
 * EVM verifyTaskAttested takes the score straight from the attester signature
 * (no Switchboard), so BOTH kind 0 (OracleMetrics continuum) and kind 1
 * (DeterministicAttested {0, MAX}) are fully score-controllable here.
 *
 * Terminals exercised:
 *   finalize (after the challenge window) · challenge→agent-wins ·
 *   challenge→challenger-wins · expire (full refund)
 *
 * Terminals SKIPPED live, with reason (noted in the run log):
 *   - default-resolve: needs the dispute window to lapse; disputeWindowSecs is
 *     already at the on-chain MIN (1 hour), so the permissionless crank can't
 *     fire in-run. Covered by the Solana bankrun matrix with clock-warp.
 *   - Submitted-state expiry: needs verificationTimeoutSecs (MIN 1 hour) to
 *     lapse. We instead drive the SAME Expired oracle outcome (full client
 *     refund) from the Open state via a short task deadline — no long wait,
 *     no owner privilege.
 *
 * setConfig is NOT used: all three windows already sit at their contract MINs
 * (challenge 60s, dispute 1h, verification-timeout 1h), so there is nothing to
 * shrink. The 60s challenge window is waited out once (batched across all
 * finalize cells) so the run stays ~2 minutes.
 *
 * Roles (funded Base Sepolia keystores; deployed config read live):
 *   client/owner = xchain-testnet · worker = evmgame-player-a ·
 *   attester = evmgame-player-b (signs the digest off-chain, also relays the
 *   challenge as challenger). treasury is read from the contract (== worker on
 *   the current deploy — a demo overlap; the oracle's agent+treasury fields are
 *   therefore validated as one combined delta on that address, still exact).
 *
 * NOT a CI test (real testnet gas + a 60s wait). Run:
 *   ESCROW_ADDR=0xaFe061778f9A76fCe7da4124dC89DAF8309E5F3c \
 *   npx tsx tests/live/shillbot-escrow-matrix-testnet.ts
 */
import { readFileSync } from "fs";
import { createHash } from "crypto";
import {
  createPublicClient,
  createWalletClient,
  http,
  fallback,
  encodeAbiParameters,
  keccak256,
  parseAbi,
  parseEventLogs,
  formatEther,
  type Hex,
  type Address,
  type WalletClient,
  type PublicClient,
} from "viem";
import { privateKeyToAccount, type PrivateKeyAccount } from "viem/accounts";
import { baseSepolia, sepolia } from "viem/chains";
import {
  deriveTaskOutcome,
  computePayment,
  TaskOutcomeKind,
  TaskPayout,
  TaskScenario,
  MAX_SCORE,
} from "../../sdk/task-outcome-oracle";

// Chain-parameterized. CAIP2 used to be hardcoded to eip155:84532 while RPC and
// ESCROW were already env-driven, so pointing the runner at another chain
// silently produced a Base-Sepolia run wearing the other chain's label — the
// matrix reported PASS for a chain it never touched. CHAIN_TAG is derived from
// CAIP2 and goes into the attestation the contract verifies, so it MUST track
// the chain actually under test.
const CHAINS: Record<
  string,
  {
    rpcs: string[];
    rpc: string;
    escrow: Address;
    viemChain: typeof baseSepolia;
  }
> = {
  "eip155:84532": {
    rpcs: ["https://sepolia.base.org"],
    rpc: "https://sepolia.base.org",
    escrow: "0xaFe061778f9A76fCe7da4124dC89DAF8309E5F3c" as Address,
    viemChain: baseSepolia,
  },
  "eip155:11155111": {
    // Measured 2026-08-01 (5 block-number calls each): drpc 5/5 @102ms,
    // publicnode 5/5 @168ms, 1rpc 5/5 @351ms; rpc.sepolia.org and
    // blastapi were 0/5. A single public endpoint still drops writes
    // mid-run ("HTTP request failed" / "gas required"), so fail over.
    rpcs: [
      "https://sepolia.drpc.org",
      "https://ethereum-sepolia-rpc.publicnode.com",
      "https://1rpc.io/sepolia",
    ],
    rpc: "https://sepolia.drpc.org",
    escrow: "0x293AB2b2A7d862d8FbD6EB1E185f984E0a65882F" as Address,
    viemChain: sepolia,
  },
};

const CAIP2 = process.env.SHILLBOT_ESCROW_CHAIN ?? "eip155:84532";
const CHAIN = CHAINS[CAIP2];
if (!CHAIN) {
  console.error(
    `SHILLBOT_ESCROW_CHAIN=${CAIP2} has no ShillbotEscrow deployment. ` +
      `Known: ${Object.keys(CHAINS).join(", ")}. Deploy via ` +
      `deploy-evm-testnet.yml (contract=shillbot) and add it here.`
  );
  process.exit(2);
}

const RPC = process.env.RPC_URL ?? CHAIN.rpc;
// Fail over across endpoints rather than trusting one public node: a single
// dropped write mid-battery reads as a cell failure when it is really the RPC.
const TRANSPORT = process.env.RPC_URL
  ? fallback([http(process.env.RPC_URL)])
  : fallback(CHAIN.rpcs.map((u) => http(u)));
const ESCROW = (process.env.ESCROW_ADDR ?? CHAIN.escrow) as Address;
const VIEM_CHAIN = CHAIN.viemChain;
const CHAIN_TAG = keccak256(Buffer.from(CAIP2));
const ATTEST_MAGIC = keccak256(Buffer.from("SWARM_ATTEST_V1"));
// keccak256 of policies/lean-attester-policy-v1.json (VerifyLib.LEAN_POLICY_V1_ID).
const POLICY_ID =
  "0xd52a2aa68bdbc5f34d3acb0bc4dcdfd4936ea8ab930e6d0cd37174df19db1eab" as Hex;

// Small escrow — value never leaves our controlled keys (it moves between
// worker/client/treasury/challenger, all ours, as withdrawable credit), so real
// spend is gas only; the escrow just bounds ETH locked in the contract.
// Floor comes from the contract, not a constant: a fresh deploy uses
// minEscrowWei = 1e14, while Base Sepolia was lowered to 1000 wei, so the old
// hardcoded 5e13 reverted BadEscrow() on any chain that kept the default.
// Resolved at bootstrap to max(preferred, live minEscrowWei).
const PREFERRED_ESCROW_WEI = 50_000_000_000_000n; // 5e13 = 0.00005 ETH
let ESCROW_WEI = PREFERRED_ESCROW_WEI;
const K = TaskOutcomeKind;

class Checker {
  private failures = 0;
  private passes = 0;
  check(cond: boolean, msg: string): void {
    console.log(`  ${cond ? "✓" : "✗"} ${msg}`);
    if (cond) this.passes++;
    else this.failures++;
  }
  finish(): void {
    console.log(
      `\n${this.failures === 0 ? "PASS" : "FAIL"} — ${this.passes} passed, ${
        this.failures
      } failed`
    );
    process.exit(this.failures === 0 ? 0 : 1);
  }
}

function loadKey(name: string): Hex {
  const raw = readFileSync(
    `${process.env.HOME}/.foundry/keystores/${name}.key`,
    "utf8"
  ).trim();
  return `0x${raw.replace(/^0x/, "")}` as Hex;
}
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const sha256Hex = (s: string): Hex =>
  `0x${createHash("sha256").update(s).digest("hex")}` as Hex;

const ABI = parseAbi([
  "struct Task { address client; address worker; uint8 state; uint8 verificationKind; bool requiresApproval; uint128 escrowWei; bytes32 statementCommitment; bytes32 policyId; bytes32 contentIdHash; bytes32 artifactHash; uint64 deadline; uint64 submittedAt; uint64 verifiedAt; uint64 challengeDeadline; uint64 resolutionDeadline; uint32 challengeWindowSecs; uint32 disputeWindowSecs; uint32 verificationTimeoutSecs; uint128 paymentWei; uint128 feeWei; address challenger; uint128 bondWei; }",
  "function createTask(bytes32 statementCommitment, bytes32 policyId, uint8 verificationKind, uint64 deadline, bool requiresApproval) payable returns (uint64)",
  "function claimTask(uint64 id)",
  "function submitWork(uint64 id, bytes32 contentIdHash, bytes32 artifactHash)",
  "function verifyTaskAttested(uint64 id, uint64 score, bytes sig)",
  "function challengeTask(uint64 id) payable",
  "function resolveChallenge(uint64 id, bool challengerWon)",
  "function finalizeTask(uint64 id)",
  "function expireTask(uint64 id)",
  "function withdrawable(address) view returns (uint256)",
  "function nextTaskId() view returns (uint64)",
  "function getTask(uint64 id) view returns (Task)",
  "function treasury() view returns (address)",
  "function protocolFeeBps() view returns (uint16)",
  "function qualityThreshold() view returns (uint64)",
  "function minEscrowWei() view returns (uint256)",
  "function challengeBondMultiplier() view returns (uint8)",
  "function bondSlashTreasuryBps() view returns (uint16)",
  "event TaskCreated(uint64 indexed taskId, address indexed client, uint128 escrowWei, uint64 deadline, uint8 verificationKind)",
]);

interface Descriptor {
  verificationKind: number;
  statementCommitment: Hex;
  policyId: Hex;
  artifactHash: Hex;
}

/** Mirror VerifyLib.attestationDigest exactly (abi.encode of value types). */
function attestationDigest(taskId: bigint, d: Descriptor, score: bigint): Hex {
  const contractWord = `0x${ESCROW.slice(2)
    .toLowerCase()
    .padStart(64, "0")}` as Hex;
  const payload = encodeAbiParameters(
    [
      { type: "bytes32" }, // ATTEST_MAGIC
      { type: "uint256" }, // version
      { type: "bytes32" }, // chainTag
      { type: "bytes32" }, // contract (address left-padded to 32)
      { type: "uint256" }, // subjectId
      { type: "uint256" }, // verificationKind
      { type: "bytes32" }, // statementCommitment
      { type: "bytes32" }, // policyId
      { type: "bytes32" }, // artifactHash
      { type: "uint256" }, // score
    ],
    [
      ATTEST_MAGIC,
      1n,
      CHAIN_TAG,
      contractWord,
      taskId,
      BigInt(d.verificationKind),
      d.statementCommitment,
      d.policyId,
      d.artifactHash,
      score,
    ]
  );
  return keccak256(payload);
}

interface Cell {
  name: string;
  kind: 0 | 1;
  score: number;
  outcome: TaskOutcomeKind;
}

// qualityThreshold is 200000 on the live deploy (asserted at startup). The
// kind-0 score axis samples {0, threshold-1, threshold, mid, MAX}; kind-1 the
// legal binary {0, MAX}. Full outcome cross-product is pruned to a coverage-rich
// slate: the whole score axis under finalize (cheap, pins payment), the bond
// mechanics under challenge terminals at representative scores, and one expire.
const THRESHOLD = 200_000;
const CELLS: Cell[] = [
  // ---- finalize: full score axis, both kinds (batched 60s window) ----
  { name: "k0 score=0 finalize", kind: 0, score: 0, outcome: K.Finalized },
  {
    name: "k0 score=threshold-1 finalize",
    kind: 0,
    score: THRESHOLD - 1,
    outcome: K.Finalized,
  },
  {
    name: "k0 score=threshold finalize",
    kind: 0,
    score: THRESHOLD,
    outcome: K.Finalized,
  },
  {
    name: "k0 score=mid(600000) finalize",
    kind: 0,
    score: 600_000,
    outcome: K.Finalized,
  },
  {
    name: "k0 score=MAX finalize",
    kind: 0,
    score: MAX_SCORE,
    outcome: K.Finalized,
  },
  { name: "k1 score=0 finalize", kind: 1, score: 0, outcome: K.Finalized },
  {
    name: "k1 score=MAX finalize",
    kind: 1,
    score: MAX_SCORE,
    outcome: K.Finalized,
  },
  // ---- challenge → agent-wins (payment + slashed bond) ----
  {
    name: "k1 score=MAX agent-wins",
    kind: 1,
    score: MAX_SCORE,
    outcome: K.ResolvedAgentWins,
  },
  {
    name: "k1 score=0 agent-wins (0 pay + slashed bond)",
    kind: 1,
    score: 0,
    outcome: K.ResolvedAgentWins,
  },
  {
    name: "k0 score=mid agent-wins",
    kind: 0,
    score: 600_000,
    outcome: K.ResolvedAgentWins,
  },
  // ---- challenge → challenger-wins (escrow refund, bond returned) ----
  {
    name: "k1 score=MAX challenger-wins",
    kind: 1,
    score: MAX_SCORE,
    outcome: K.ResolvedChallengerWins,
  },
  {
    name: "k0 score=mid challenger-wins",
    kind: 0,
    score: 600_000,
    outcome: K.ResolvedChallengerWins,
  },
  // ---- expire (full client refund; driven from Open via short deadline) ----
  {
    name: "expire (full refund from Open)",
    kind: 1,
    score: 0,
    outcome: K.Expired,
  },
];

interface Config {
  treasury: Address;
  protocolFeeBps: number;
  qualityThreshold: number;
  challengeBondMultiplier: number;
  bondSlashTreasuryBps: number;
}

interface Roles {
  client: PrivateKeyAccount;
  worker: PrivateKeyAccount;
  attester: PrivateKeyAccount;
  challenger: PrivateKeyAccount;
  treasury: Address;
}

interface Env {
  pub: PublicClient;
  clientW: WalletClient;
  workerW: WalletClient;
  challengerW: WalletClient;
  roles: Roles;
  cfg: Config;
  chk: Checker;
  txlog: Record<string, Hex>;
}

async function writeAndWait(
  env: Env,
  wallet: WalletClient,
  account: PrivateKeyAccount,
  functionName: string,
  args: unknown[],
  value?: bigint
): Promise<Hex> {
  const hash = await wallet.writeContract({
    address: ESCROW,
    abi: ABI,
    functionName: functionName as never,
    args: args as never,
    account,
    chain: VIEM_CHAIN,
    ...(value !== undefined ? { value } : {}),
  });
  await env.pub.waitForTransactionReceipt({ hash });
  return hash;
}

// createTask, returning the real task id from THIS tx's TaskCreated event (not a
// pre-read of nextTaskId, which is off-by-one under any interleaving) and gating
// on the task being visible before returning — the public Base Sepolia RPC is
// load-balanced, so a follow-up read/estimateGas can hit a replica behind the
// create block and observe an all-zero (state 0, escrow 0) struct.
async function createTaskViaEvent(
  env: Env,
  cellName: string,
  client: PrivateKeyAccount,
  args: unknown[]
): Promise<bigint> {
  const hash = await env.clientW.writeContract({
    address: ESCROW,
    abi: ABI,
    functionName: "createTask",
    args: args as never,
    account: client,
    chain: VIEM_CHAIN,
    value: ESCROW_WEI,
  });
  const rcpt = await env.pub.waitForTransactionReceipt({ hash });
  env.txlog[`${cellName}/create`] = hash;
  const created = parseEventLogs({
    abi: ABI,
    eventName: "TaskCreated",
    logs: rcpt.logs,
  }).find(
    (l) =>
      (l.args.client as Address).toLowerCase() === client.address.toLowerCase()
  );
  if (!created)
    throw new Error(`${cellName}: createTask emitted no TaskCreated`);
  const taskId = created.args.taskId as bigint;
  for (let i = 0; i < 45; i++) {
    if ((await getTask(env, taskId)).deadline !== 0n) break;
    await sleep(1_000);
  }
  return taskId;
}

// Poll getTask(taskId).state until it reaches `expected` (or time out), so a
// state-guarded next step doesn't estimateGas against a lagging replica and
// revert InvalidStatus (0xf525e320).
async function waitTaskState(
  env: Env,
  taskId: bigint,
  expected: number,
  timeoutMs = 45_000
): Promise<number> {
  const deadline = Date.now() + timeoutMs;
  let last = -1;
  for (let i = 0; i < 90; i++) {
    last = (await getTask(env, taskId)).state;
    if (last === expected) return last;
    if (Date.now() >= deadline) break;
    await sleep(1_000);
  }
  return last;
}

async function getTask(env: Env, taskId: bigint) {
  return (await env.pub.readContract({
    address: ESCROW,
    abi: ABI,
    functionName: "getTask",
    args: [taskId],
  })) as {
    state: number;
    verificationKind: number;
    statementCommitment: Hex;
    policyId: Hex;
    artifactHash: Hex;
    paymentWei: bigint;
    feeWei: bigint;
    challengeDeadline: bigint;
    deadline: bigint;
  };
}

async function withdrawable(env: Env, addr: Address): Promise<bigint> {
  return (await env.pub.readContract({
    address: ESCROW,
    abi: ABI,
    functionName: "withdrawable",
    args: [addr],
  })) as bigint;
}

function scenarioFor(cfg: Config, cell: Cell): TaskScenario {
  return {
    escrowLamports: ESCROW_WEI,
    qualityThreshold: cfg.qualityThreshold,
    protocolFeeBps: cfg.protocolFeeBps,
    compositeScore: cell.score,
    verificationKind: cell.kind,
    challengeBondMultiplier: cfg.challengeBondMultiplier,
    bondSlashTreasuryBps: cfg.bondSlashTreasuryBps,
    outcome: cell.outcome,
  };
}

/** Collapse the oracle's four role payouts onto their on-chain addresses
 *  (handles the worker==treasury overlap by summing). */
function expectedByAddress(roles: Roles, o: TaskPayout): Map<Address, bigint> {
  const m = new Map<Address, bigint>();
  const add = (a: Address, v: bigint) =>
    m.set(
      a.toLowerCase() as Address,
      (m.get(a.toLowerCase() as Address) ?? 0n) + v
    );
  add(roles.worker.address, o.agentLamports);
  add(roles.treasury, o.treasuryLamports);
  add(roles.client.address, o.clientLamports);
  add(roles.challenger.address, o.challengerLamports);
  return m;
}

/** Snapshot withdrawable for every role address (deduped). */
async function snapshot(env: Env): Promise<Map<Address, bigint>> {
  const addrs = new Set<Address>(
    [
      env.roles.worker.address,
      env.roles.treasury,
      env.roles.client.address,
      env.roles.challenger.address,
    ].map((a) => a.toLowerCase() as Address)
  );
  const m = new Map<Address, bigint>();
  for (const a of addrs) m.set(a, await withdrawable(env, a));
  return m;
}

async function assertDeltas(
  env: Env,
  cell: Cell,
  before: Map<Address, bigint>,
  expected: Map<Address, bigint>
): Promise<void> {
  // The withdrawable credit from finalize/resolve is not instantly visible on the
  // load-balanced RPC (a challenge cell reads it right after resolve, with no
  // challenge-window sleep to hide the lag) — poll until the observed deltas match
  // the oracle, or time out, so a lagging replica doesn't read a stale 0.
  const matches = (after: Map<Address, bigint>): boolean => {
    for (const [addr, prev] of before.entries()) {
      if ((after.get(addr) ?? 0n) - prev !== (expected.get(addr) ?? 0n)) {
        return false;
      }
    }
    return true;
  };
  let after = await snapshot(env);
  const deadline = Date.now() + 45_000;
  while (!matches(after) && Date.now() < deadline) {
    await sleep(1_000);
    after = await snapshot(env);
  }
  for (const [addr, prev] of before.entries()) {
    const delta = (after.get(addr) ?? 0n) - prev;
    const exp = expected.get(addr) ?? 0n;
    env.chk.check(
      delta === exp,
      `${cell.name}: withdrawable[${addr.slice(
        0,
        8
      )}] delta ${delta} == oracle ${exp}`
    );
  }
}

/** create → claim → submit → verify(attester sig). Returns taskId + on-chain
 *  challengeDeadline. Also asserts the PINNED payment/fee against the oracle. */
async function toVerified(
  env: Env,
  cell: Cell
): Promise<{ taskId: bigint; challengeDeadline: bigint }> {
  const { client, worker, attester } = env.roles;
  const statement = `matrix_${cell.name}_${Date.now()}`;
  const statementCommitment =
    cell.kind === 1 ? sha256Hex(statement) : (`0x${"00".repeat(32)}` as Hex);
  const policyId =
    cell.kind === 1 ? POLICY_ID : (`0x${"00".repeat(32)}` as Hex);
  const artifactHash = sha256Hex("artifact:" + statement);

  const deadline = BigInt(Math.floor(Date.now() / 1000) + 86_400);
  const taskId = await createTaskViaEvent(env, cell.name, client, [
    statementCommitment,
    policyId,
    cell.kind,
    deadline,
    false,
  ]);
  env.txlog[`${cell.name}/claim`] = await writeAndWait(
    env,
    env.workerW,
    worker,
    "claimTask",
    [taskId]
  );
  await waitTaskState(env, taskId, 1);
  env.txlog[`${cell.name}/submit`] = await writeAndWait(
    env,
    env.workerW,
    worker,
    "submitWork",
    [taskId, sha256Hex("content:" + statement), artifactHash]
  );
  await waitTaskState(env, taskId, 2);

  const t = await getTask(env, taskId);
  const digest = attestationDigest(
    taskId,
    {
      verificationKind: t.verificationKind,
      statementCommitment: t.statementCommitment,
      policyId: t.policyId,
      artifactHash: t.artifactHash,
    },
    BigInt(cell.score)
  );
  const sig = await attester.sign({ hash: digest });
  env.txlog[`${cell.name}/verify`] = await writeAndWait(
    env,
    env.clientW,
    client,
    "verifyTaskAttested",
    [taskId, BigInt(cell.score), sig]
  );
  await waitTaskState(env, taskId, 3);

  const v = await getTask(env, taskId);
  const pinned = computePayment(
    cell.score,
    env.cfg.qualityThreshold,
    ESCROW_WEI,
    env.cfg.protocolFeeBps
  );
  env.chk.check(
    v.state === 3,
    `${cell.name}: state == Verified after attested verify`
  );
  env.chk.check(
    v.paymentWei === pinned.payment,
    `${cell.name}: pinned payment ${v.paymentWei} == oracle ${pinned.payment}`
  );
  env.chk.check(
    v.feeWei === pinned.fee,
    `${cell.name}: pinned fee ${v.feeWei} == oracle ${pinned.fee}`
  );
  return { taskId, challengeDeadline: v.challengeDeadline };
}

async function runChallengeCell(env: Env, cell: Cell): Promise<void> {
  console.log(`\n=== ${cell.name} ===`);
  const { taskId } = await toVerified(env, cell);
  const expected = expectedByAddress(
    env.roles,
    deriveTaskOutcome(scenarioFor(env.cfg, cell))
  );

  const bond = ESCROW_WEI * BigInt(env.cfg.challengeBondMultiplier);
  env.txlog[`${cell.name}/challenge`] = await writeAndWait(
    env,
    env.challengerW,
    env.roles.challenger,
    "challengeTask",
    [taskId],
    bond
  );
  await waitTaskState(env, taskId, 5); // Disputed — before resolveChallenge

  const before = await snapshot(env);
  const challengerWon = cell.outcome === K.ResolvedChallengerWins;
  env.txlog[`${cell.name}/resolve`] = await writeAndWait(
    env,
    env.clientW,
    env.roles.client,
    "resolveChallenge",
    [taskId, challengerWon]
  );
  await assertDeltas(env, cell, before, expected);
}

async function runExpireCell(
  env: Env,
  cell: Cell
): Promise<{
  taskId: bigint;
  expireAt: number;
  expected: Map<Address, bigint>;
}> {
  console.log(`\n=== ${cell.name} (setup) ===`);
  // Open-state expiry: create with a short deadline, never claim. expireTask
  // opens strictly after the deadline → full escrow refunded to the client.
  const deadline = Math.floor(Date.now() / 1000) + 40;
  const taskId = await createTaskViaEvent(env, cell.name, env.roles.client, [
    sha256Hex(cell.name),
    POLICY_ID,
    cell.kind,
    BigInt(deadline),
    false,
  ]);
  const expected = expectedByAddress(
    env.roles,
    deriveTaskOutcome(scenarioFor(env.cfg, cell))
  );
  return { taskId, expireAt: deadline, expected };
}

async function main(): Promise<void> {
  const chk = new Checker();
  const client = privateKeyToAccount(loadKey("xchain-testnet"));
  const worker = privateKeyToAccount(loadKey("evmgame-player-a"));
  const attester = privateKeyToAccount(loadKey("evmgame-player-b"));
  const challenger = attester; // attester EOA doubles as challenger (distinct from worker/client)

  const pub = createPublicClient({
    chain: VIEM_CHAIN,
    transport: TRANSPORT,
  }) as PublicClient;
  const clientW = createWalletClient({
    account: client,
    chain: VIEM_CHAIN,
    transport: TRANSPORT,
  });
  const workerW = createWalletClient({
    account: worker,
    chain: VIEM_CHAIN,
    transport: TRANSPORT,
  });
  const challengerW = createWalletClient({
    account: challenger,
    chain: VIEM_CHAIN,
    transport: TRANSPORT,
  });

  const read = async (fn: string) =>
    pub.readContract({ address: ESCROW, abi: ABI, functionName: fn as never });
  const cfg: Config = {
    treasury: (await read("treasury")) as Address,
    protocolFeeBps: Number(await read("protocolFeeBps")),
    qualityThreshold: Number(await read("qualityThreshold")),
    challengeBondMultiplier: Number(await read("challengeBondMultiplier")),
    bondSlashTreasuryBps: Number(await read("bondSlashTreasuryBps")),
  };

  const minEscrow = (await read("minEscrowWei")) as bigint;
  if (minEscrow > ESCROW_WEI) ESCROW_WEI = minEscrow;

  console.log(`escrow contract: ${ESCROW}  (${CAIP2})`);
  console.log(`client/owner:    ${client.address}`);
  console.log(`worker:          ${worker.address}`);
  console.log(`attester/chall:  ${attester.address}`);
  console.log(`treasury(cfg):   ${cfg.treasury}`);
  console.log(
    `cfg: feeBps=${cfg.protocolFeeBps} threshold=${cfg.qualityThreshold} bondMult=${cfg.challengeBondMultiplier} slashTreasuryBps=${cfg.bondSlashTreasuryBps}`
  );
  console.log(`escrow/cell: ${formatEther(ESCROW_WEI)} ETH`);

  chk.check(
    cfg.qualityThreshold === THRESHOLD,
    `live qualityThreshold == ${THRESHOLD} (score axis assumption)`
  );

  const roles: Roles = {
    client,
    worker,
    attester,
    challenger,
    treasury: cfg.treasury,
  };
  const env: Env = {
    pub,
    clientW,
    workerW,
    challengerW,
    roles,
    cfg,
    chk,
    txlog: {},
  };

  const balBefore = await pub.getBalance({ address: client.address });

  const finalizeCells = CELLS.filter((c) => c.outcome === K.Finalized);
  const challengeCells = CELLS.filter(
    (c) =>
      c.outcome === K.ResolvedAgentWins ||
      c.outcome === K.ResolvedChallengerWins
  );
  const expireCell = CELLS.find((c) => c.outcome === K.Expired)!;

  // 1) Challenge terminals — immediate (owner adjudicates within the dispute window).
  for (const cell of challengeCells) {
    try {
      await runChallengeCell(env, cell);
    } catch (e) {
      chk.check(false, `${cell.name}: threw ${String(e).slice(0, 200)}`);
    }
  }

  // 2) Finalize setup + expire setup (no waits yet), then one batched wait.
  const pendingFinalize: {
    cell: Cell;
    taskId: bigint;
    challengeDeadline: bigint;
    expected: Map<Address, bigint>;
  }[] = [];
  for (const cell of finalizeCells) {
    try {
      console.log(`\n=== ${cell.name} (setup) ===`);
      const { taskId, challengeDeadline } = await toVerified(env, cell);
      pendingFinalize.push({
        cell,
        taskId,
        challengeDeadline,
        expected: expectedByAddress(
          roles,
          deriveTaskOutcome(scenarioFor(cfg, cell))
        ),
      });
    } catch (e) {
      chk.check(false, `${cell.name}: setup threw ${String(e).slice(0, 200)}`);
    }
  }
  const expireSetup = await runExpireCell(env, expireCell).catch((e) => {
    chk.check(
      false,
      `${expireCell.name}: setup threw ${String(e).slice(0, 200)}`
    );
    return null;
  });

  const targets = [
    ...pendingFinalize.map((p) => Number(p.challengeDeadline)),
    ...(expireSetup ? [expireSetup.expireAt] : []),
  ];
  const waitUntil = Math.max(...targets) + 5;
  const waitMs = Math.max(0, waitUntil - Math.floor(Date.now() / 1000)) * 1000;
  console.log(
    `\n… batched wait ${Math.round(
      waitMs / 1000
    )}s for challenge windows + expiry deadline`
  );
  await sleep(waitMs);

  // 3) Finalize each verified task + assert.
  for (const p of pendingFinalize) {
    try {
      console.log(`\n=== ${p.cell.name} (finalize) ===`);
      const before = await snapshot(env);
      env.txlog[`${p.cell.name}/finalize`] = await writeAndWait(
        env,
        clientW,
        client,
        "finalizeTask",
        [p.taskId]
      );
      await assertDeltas(env, p.cell, before, p.expected);
    } catch (e) {
      chk.check(
        false,
        `${p.cell.name}: finalize threw ${String(e).slice(0, 200)}`
      );
    }
  }

  // 4) Expire + assert.
  if (expireSetup) {
    try {
      console.log(`\n=== ${expireCell.name} (expire) ===`);
      const before = await snapshot(env);
      env.txlog[`${expireCell.name}/expire`] = await writeAndWait(
        env,
        clientW,
        client,
        "expireTask",
        [expireSetup.taskId]
      );
      await assertDeltas(env, expireCell, before, expireSetup.expected);
    } catch (e) {
      chk.check(
        false,
        `${expireCell.name}: expire threw ${String(e).slice(0, 200)}`
      );
    }
  }

  const balAfter = await pub.getBalance({ address: client.address });
  console.log(
    `\nclient ETH before=${formatEther(balBefore)} after=${formatEther(
      balAfter
    )}`
  );

  console.log("\n--- tx hashes ---");
  for (const [k, v] of Object.entries(env.txlog)) console.log(`  ${k}: ${v}`);

  console.log("\nSKIPPED live (with reason):");
  console.log(
    "  default-resolve: disputeWindowSecs at on-chain MIN (1h) — permissionless crank can't fire in-run"
  );
  console.log(
    "  Submitted-state expiry: verificationTimeoutSecs at on-chain MIN (1h) — used Open-state expiry instead"
  );

  chk.finish();
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
