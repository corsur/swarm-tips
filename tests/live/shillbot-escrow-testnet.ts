/**
 * Live Base Sepolia e2e for ShillbotEscrow — the deterministic ATTESTED
 * lifecycle end-to-end against the deployed contract, proving the
 * cross-language attester → on-chain `ecrecover` path: an off-chain signer
 * (mirroring chain-core `cosign::sign_digest_eth`, v=27/28) signs the
 * VerifyLib attestation digest, and the deployed bytecode's
 * `verifyTaskAttested` accepts it and pins the payout.
 *
 *   createTask(kind=1) → claimTask (worker) → submitWork → verifyTaskAttested
 *   (attester sig) → finalize (after the 60s challenge window) → assert the
 *   worker's withdrawable = escrow − fee.
 *
 * Roles (funded Base Sepolia keys): xchain-testnet = owner + client;
 * evmgame-player-a = worker AND treasury (demo overlap — so the worker's
 * post-finalize withdrawable is payment + fee = the full escrow; in production
 * the treasury is a distinct DAO address); evmgame-player-b = the attester
 * signer configured on the deployed contract. The attestation digest binds the
 * deployed contract address, so a signature can't replay to another instance.
 *
 * NOT a CI test (real testnet gas + a 60s wait). Run:
 *   ESCROW_ADDR=0x… npx tsx tests/live/shillbot-escrow-testnet.ts
 */
import { readFileSync } from "fs";
import {
  createPublicClient,
  createWalletClient,
  http,
  encodeAbiParameters,
  keccak256,
  parseAbi,
  parseEventLogs,
  getContract,
  type Hex,
  type Address,
  type PublicClient,
} from "viem";
import { privateKeyToAccount } from "viem/accounts";
import { baseSepolia } from "viem/chains";

const RPC = process.env.RPC_URL ?? "https://sepolia.base.org";
const ESCROW = (process.env.ESCROW_ADDR ??
  "0xaFe061778f9A76fCe7da4124dC89DAF8309E5F3c") as Address;
const CAIP2 = "eip155:84532";
const MAX_SCORE = 1_000_000n;
const KIND_DETERMINISTIC = 1;
const VERIFY_MAGIC = keccak256(Buffer.from("SWARM_VERIFY_V1"));
const ATTEST_MAGIC = keccak256(Buffer.from("SWARM_ATTEST_V1"));
const CHAIN_TAG = keccak256(Buffer.from(CAIP2));
// keccak256 of policies/lean-attester-policy-v1.json bytes (VerifyLib.LEAN_POLICY_V1_ID).
const POLICY_ID =
  "0xd52a2aa68bdbc5f34d3acb0bc4dcdfd4936ea8ab930e6d0cd37174df19db1eab" as Hex;

let failures = 0;
function check(cond: boolean, msg: string): void {
  console.log(`  ${cond ? "✓" : "✗"} ${msg}`);
  if (!cond) failures++;
}
function loadKey(name: string): Hex {
  const raw = readFileSync(
    `${process.env.HOME}/.foundry/keystores/${name}.key`,
    "utf8"
  ).trim();
  return `0x${raw.replace(/^0x/, "")}` as Hex;
}
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

// Poll getTask(taskId).state until it reaches `expected`, or time out. The
// public Base Sepolia RPC is load-balanced: waitForTransactionReceipt confirms
// the tx is mined, but a following read (or the NEXT write's eth_estimateGas)
// can hit a replica that hasn't yet seen that block — so it observes the prior
// state and a state-guarded step reverts InvalidStatus (0xf525e320). Gating each
// dependent step on the state actually being visible removes that flake.
async function waitTaskState(
  pub: PublicClient,
  taskId: bigint,
  expected: number,
  timeoutMs = 45_000
): Promise<number> {
  const deadline = Date.now() + timeoutMs;
  let last = -1;
  for (let i = 0; i < 90; i++) {
    const t = (await pub.readContract({
      address: ESCROW,
      abi: ABI,
      functionName: "getTask",
      args: [taskId],
    })) as { state: number };
    last = t.state;
    if (last === expected) return last;
    if (Date.now() >= deadline) break;
    await sleep(1_000);
  }
  return last;
}
const sha256Hex = (s: string): Hex =>
  `0x${require("crypto").createHash("sha256").update(s).digest("hex")}` as Hex;

const ABI = parseAbi([
  "struct Task { address client; address worker; uint8 state; uint8 verificationKind; bool requiresApproval; uint128 escrowWei; bytes32 statementCommitment; bytes32 policyId; bytes32 contentIdHash; bytes32 artifactHash; uint64 deadline; uint64 submittedAt; uint64 verifiedAt; uint64 challengeDeadline; uint64 resolutionDeadline; uint32 challengeWindowSecs; uint32 disputeWindowSecs; uint32 verificationTimeoutSecs; uint128 paymentWei; uint128 feeWei; address challenger; uint128 bondWei; }",
  "function createTask(bytes32 statementCommitment, bytes32 policyId, uint8 verificationKind, uint64 deadline, bool requiresApproval) payable returns (uint64)",
  "function claimTask(uint64 id)",
  "function submitWork(uint64 id, bytes32 contentIdHash, bytes32 artifactHash)",
  "function verifyTaskAttested(uint64 id, uint64 score, bytes sig)",
  "function finalizeTask(uint64 id)",
  "function withdraw()",
  "function withdrawable(address) view returns (uint256)",
  "function nextTaskId() view returns (uint64)",
  "function getTask(uint64 id) view returns (Task)",
  "event TaskCreated(uint64 indexed taskId, address indexed client, uint128 escrowWei, uint64 deadline, uint8 verificationKind)",
]);

/** Mirror VerifyLib.attestationDigest exactly (abi.encode of value types). */
function attestationDigest(
  taskId: bigint,
  statementCommitment: Hex,
  artifactHash: Hex,
  score: bigint
): Hex {
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
      BigInt(KIND_DETERMINISTIC),
      statementCommitment,
      POLICY_ID,
      artifactHash,
      score,
    ]
  );
  return keccak256(payload);
}

async function main(): Promise<void> {
  const client = privateKeyToAccount(loadKey("xchain-testnet"));
  const worker = privateKeyToAccount(loadKey("evmgame-player-a"));
  const attester = privateKeyToAccount(loadKey("evmgame-player-b"));

  const pub = createPublicClient({ chain: baseSepolia, transport: http(RPC) });
  const clientW = createWalletClient({
    account: client,
    chain: baseSepolia,
    transport: http(RPC),
  });
  const workerW = createWalletClient({
    account: worker,
    chain: baseSepolia,
    transport: http(RPC),
  });

  console.log(`escrow contract: ${ESCROW}`);
  console.log(`client:   ${client.address}`);
  console.log(`worker:   ${worker.address}`);
  console.log(`attester: ${attester.address}`);

  const escrowWei = 100_000n; // 0.0000000000001 ETH — dust; testnet
  const statement = `theorem_${Date.now()}`;
  const statementCommitment = sha256Hex(statement);
  const artifactHash = sha256Hex("Proof.lean:" + statement);

  // createTask (kind=1) with escrow. The real task id comes from THIS tx's
  // TaskCreated event, not a pre-read of nextTaskId() — the counter is global
  // and a concurrent createTask (parallel session / the matrix suite share
  // these keys) would advance it between the read and our create, leaving us
  // claiming someone else's already-claimed task (InvalidStatus / 0xf525e320).
  const deadline = BigInt(Math.floor(Date.now() / 1000) + 86_400);
  let hash = await clientW.writeContract({
    address: ESCROW,
    abi: ABI,
    functionName: "createTask",
    args: [statementCommitment, POLICY_ID, KIND_DETERMINISTIC, deadline, false],
    value: escrowWei,
  });
  const createRcpt = await pub.waitForTransactionReceipt({ hash });
  const created = parseEventLogs({
    abi: ABI,
    eventName: "TaskCreated",
    logs: createRcpt.logs,
  }).find(
    (l) =>
      (l.args.client as Address).toLowerCase() ===
      clientW.account.address.toLowerCase()
  );
  if (!created) throw new Error("createTask emitted no TaskCreated for client");
  const taskId = created.args.taskId as bigint;
  console.log(`\n=== attested lifecycle (task ${taskId}) ===`);
  // Gate on the created task being VISIBLE, not on state==0: a not-yet-propagated
  // task on a lagging RPC replica also reads as an all-zero struct (state 0,
  // kind 0, escrow 0), so state==0 can't distinguish "Open" from "not there yet".
  // Poll until the escrow (set at creation) is visible.
  let task = (await pub.readContract({
    address: ESCROW,
    abi: ABI,
    functionName: "getTask",
    args: [taskId],
  })) as {
    state: number;
    verificationKind: number;
    escrowWei: bigint;
    paymentWei: bigint;
    feeWei: bigint;
  };
  for (let i = 0; i < 45 && task.escrowWei === 0n; i++) {
    await sleep(1_000);
    task = (await pub.readContract({
      address: ESCROW,
      abi: ABI,
      functionName: "getTask",
      args: [taskId],
    })) as typeof task;
  }
  check(
    task.verificationKind === KIND_DETERMINISTIC,
    "task created with verificationKind = 1"
  );

  // claimTask (worker, arms-length) + submitWork.
  hash = await workerW.writeContract({
    address: ESCROW,
    abi: ABI,
    functionName: "claimTask",
    args: [taskId],
  });
  await pub.waitForTransactionReceipt({ hash });
  check(
    (await waitTaskState(pub, taskId, 1)) === 1,
    "claimTask accepted — state = Claimed"
  );
  hash = await workerW.writeContract({
    address: ESCROW,
    abi: ABI,
    functionName: "submitWork",
    args: [taskId, sha256Hex("content-id"), artifactHash],
  });
  await pub.waitForTransactionReceipt({ hash });
  check(
    (await waitTaskState(pub, taskId, 2)) === 2,
    "submitWork accepted — state = Submitted"
  );

  // Attester signs the VerifyLib digest off-chain (v=27/28), anyone relays it.
  const digest = attestationDigest(
    taskId,
    statementCommitment,
    artifactHash,
    MAX_SCORE
  );
  const sig = await attester.sign({ hash: digest });
  hash = await clientW.writeContract({
    address: ESCROW,
    abi: ABI,
    functionName: "verifyTaskAttested",
    args: [taskId, MAX_SCORE, sig],
  });
  await pub.waitForTransactionReceipt({ hash });
  await waitTaskState(pub, taskId, 3); // Verified — ensure propagation before the read
  task = (await pub.readContract({
    address: ESCROW,
    abi: ABI,
    functionName: "getTask",
    args: [taskId],
  })) as {
    state: number;
    verificationKind: number;
    paymentWei: bigint;
    feeWei: bigint;
  };
  check(
    task.state === 3,
    "state = Verified (attester signature accepted on-chain)"
  );
  const payment = task.paymentWei;
  check(
    payment === escrowWei - (escrowWei * 1000n) / 10_000n,
    `payment pinned = escrow − fee (${payment})`
  );

  // Wait past the 60s challenge window, then finalize + withdraw.
  console.log("  … waiting out the 60s challenge window");
  await sleep(65_000);
  hash = await clientW.writeContract({
    address: ESCROW,
    abi: ABI,
    functionName: "finalizeTask",
    args: [taskId],
  });
  await pub.waitForTransactionReceipt({ hash });
  check(
    (await waitTaskState(pub, taskId, 4)) === 4,
    "finalizeTask accepted — state = Finalized"
  );
  const wd = await pub.readContract({
    address: ESCROW,
    abi: ABI,
    functionName: "withdrawable",
    args: [worker.address],
  });
  check(wd >= payment, `worker withdrawable ≥ pinned payment (${wd})`);

  console.log(
    `\n${failures === 0 ? "PASS" : "FAIL"} — ${failures} check(s) failed`
  );
  process.exit(failures === 0 ? 0 : 1);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
