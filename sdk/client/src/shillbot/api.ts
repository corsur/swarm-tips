import { Buffer } from "buffer";
import {
  ComputeBudgetProgram,
  Connection,
  Secp256k1Program,
  type BlockhashWithExpiryBlockHeight,
} from "@solana/web3.js";
import { HttpClient, type HttpClientOptions } from "../http.js";
import { SwarmClientError } from "../errors.js";
import {
  SHILLBOT_PROGRAM_ID,
  inspectTransaction,
  shillbotInstructionDiscriminator,
  signAndBroadcast,
  type Action,
  type TransactionInspection,
  type TransactionIntent,
  type WalletSignCallback,
} from "./index.js";

export type { Action } from "./index.js";

export interface PreparedTransactionResponse {
  message?: string;
  task_id?: string;
  transaction: string;
  task_pda?: string;
  transaction_intent?: TransactionIntent;
  evm_call?: unknown;
}

export interface VerificationDataResponse {
  task_id?: string;
  task_pda: string;
  composite_score: number;
  verification_hash: string;
  global_state?: string;
  switchboard_feed: string;
}

export interface BuildVerifyOptions {
  payer?: string;
  mcpBaseUrl?: string;
  network?: "mainnet" | "devnet";
}

export interface ConfirmTransactionRequest {
  tx_signature: string;
  action: Action;
  task_pda?: string;
}

export interface TaskListResponse<TTask = unknown> {
  tasks: TTask[];
  next_cursor?: string | null;
}

export interface CampaignListResponse<TCampaign = unknown> {
  campaigns: TCampaign[];
  next_cursor?: string | null;
}

function decodeJwtWallet(token: string | null | undefined): string | undefined {
  if (!token) return undefined;
  const parts = token.split(".");
  if (parts.length < 2) return undefined;
  try {
    const b64 = parts[1].replace(/-/g, "+").replace(/_/g, "/");
    const padded = b64 + "=".repeat((4 - (b64.length % 4)) % 4);
    const decoded = JSON.parse(Buffer.from(padded, "base64").toString("utf8")) as {
      wallet?: unknown;
      sub?: unknown;
    };
    if (typeof decoded.wallet === "string" && decoded.wallet) return decoded.wallet;
    if (typeof decoded.sub === "string" && decoded.sub) return decoded.sub;
  } catch {
    // Non-JWT opaque token; caller can supply opts.payer explicitly.
  }
  return undefined;
}

export class ShillbotApiClient {
  readonly http: HttpClient;

  constructor(options: HttpClientOptions) {
    this.http = new HttpClient(options);
  }
  get<T>(path: string) {
    return this.http.get<T>(path);
  }
  post<T>(path: string, body?: unknown) {
    return this.http.post<T>(path, body);
  }
  patch<T>(path: string, body?: unknown) {
    return this.http.patch<T>(path, body);
  }

  listTasks<TTask = unknown>(limit = 50) {
    return this.get<TaskListResponse<TTask>>(`/tasks?limit=${encodeURIComponent(String(limit))}`);
  }
  getTask<TTask = unknown>(id: string) {
    return this.get<TTask>(`/tasks/${encodeURIComponent(id)}`);
  }
  listAgentTasks<TTask = unknown>() {
    return this.get<TaskListResponse<TTask> | TTask[]>("/agent/tasks");
  }
  listPendingApproval<TTask = unknown>() {
    return this.get<TaskListResponse<TTask> | TTask[]>("/tasks?status=pending_approval");
  }
  listCampaigns<TCampaign = unknown>() {
    return this.get<CampaignListResponse<TCampaign> | TCampaign[]>("/campaigns");
  }
  campaign<TCampaign = unknown>(id: string) {
    return this.get<TCampaign>(`/campaigns/${encodeURIComponent(id)}`);
  }
  campaignTasks<TTask = unknown>(id: string) {
    return this.get<TaskListResponse<TTask>>(`/campaigns/${encodeURIComponent(id)}/tasks`);
  }
  campaignMetrics<TMetrics = unknown>(id: string) {
    return this.get<TMetrics>(`/campaigns/${encodeURIComponent(id)}/metrics`);
  }
  createCampaign<TResponse = { campaign_id: string }>(body: unknown) {
    return this.post<TResponse>("/campaigns", body);
  }
  updateCampaign<TResponse = unknown>(id: string, action: string) {
    return this.patch<TResponse>(`/campaigns/${encodeURIComponent(id)}`, { action });
  }
  fundCampaignTask(id: string, body: unknown) {
    return this.post<PreparedTransactionResponse>(`/campaigns/${encodeURIComponent(id)}/fund`, body);
  }
  onboard<TResponse = unknown>() {
    return this.post<TResponse>("/agent/onboard");
  }
  earnings<TResponse = unknown>() {
    return this.get<TResponse>("/agent/earnings");
  }
  claimTask(id: string, sponsor = "auto") {
    return this.post<PreparedTransactionResponse>(
      `/tasks/${encodeURIComponent(id)}/claim?sponsor=${encodeURIComponent(sponsor)}`,
    );
  }
  sponsorTask(id: string, body: { action: "claim" | "submit"; unsigned_transaction: string }) {
    return this.post<PreparedTransactionResponse>(`/tasks/${encodeURIComponent(id)}/sponsor`, body);
  }
  submitTask(id: string, body: unknown) {
    return this.post<PreparedTransactionResponse>(`/tasks/${encodeURIComponent(id)}/submit`, body);
  }
  preflightTask<TResponse = unknown>(id: string, body?: unknown) {
    return this.post<TResponse>(`/tasks/${encodeURIComponent(id)}/preflight`, body);
  }
  approveTask(id: string) {
    return this.post<PreparedTransactionResponse>(`/tasks/${encodeURIComponent(id)}/approve`);
  }
  getVerificationData(id: string, network?: "mainnet" | "devnet") {
    const suffix =
      network && network !== "mainnet" ? `?network=${encodeURIComponent(network)}` : "";
    return this.post<VerificationDataResponse>(
      `/tasks/${encodeURIComponent(id)}/build-verify${suffix}`,
    );
  }
  async buildVerify(id: string, opts?: BuildVerifyOptions): Promise<PreparedTransactionResponse> {
    const mcpBaseUrl = (opts?.mcpBaseUrl ?? "https://mcp.swarm.tips").replace(/\/$/, "");
    const resolvedNetwork: "mainnet" | "devnet" =
      opts?.network ?? (this.http.network === "devnet" ? "devnet" : "mainnet");
    const token = await this.http.getToken?.();
    const payer = opts?.payer ?? decodeJwtWallet(token);

    const postMcpBuildVerify = async (
      payload: Record<string, unknown>,
    ): Promise<PreparedTransactionResponse> => {
      const mcpHttp = new HttpClient({
        baseUrl: mcpBaseUrl,
        fetch: this.http.fetchImpl,
        getToken: this.http.getToken,
      });
      return mcpHttp.post<PreparedTransactionResponse>("/internal/build-verify-tx", payload);
    };

    const oneHopBody: Record<string, unknown> = {
      task_id: id,
      network: resolvedNetwork,
      ...(payer ? { payer } : {}),
    };

    try {
      const direct = await postMcpBuildVerify(oneHopBody);
      return {
        ...direct,
        task_id: direct.task_id ?? id,
        transaction: direct.transaction,
        task_pda: direct.task_pda,
        transaction_intent: direct.transaction_intent,
      };
    } catch (err) {
      if (
        !(err instanceof SwarmClientError) ||
        err.code !== "HTTP_ERROR" ||
        (err.status !== 400 && err.status !== 422)
      ) {
        throw err;
      }
      const vdata = await this.getVerificationData(id, resolvedNetwork);
      const fallback = await postMcpBuildVerify({
        task_id: id,
        ...(payer ? { payer } : {}),
        network: resolvedNetwork,
        task_pda: vdata.task_pda,
        score: vdata.composite_score,
        hash: vdata.verification_hash,
        feed: vdata.switchboard_feed,
        ...(vdata.global_state ? { global_state: vdata.global_state } : {}),
      });
      return {
        ...fallback,
        task_id: fallback.task_id ?? id,
        transaction: fallback.transaction,
        task_pda: fallback.task_pda ?? vdata.task_pda,
        transaction_intent: fallback.transaction_intent,
      };
    }
  }
  buildFinalize(id: string) {
    return this.post<PreparedTransactionResponse>(`/tasks/${encodeURIComponent(id)}/build-finalize`);
  }
  confirmTask<TResponse = unknown>(id: string, body: ConfirmTransactionRequest) {
    return this.post<TResponse>(`/tasks/${encodeURIComponent(id)}/confirm`, body);
  }
}

export interface PreparedTransactionExpectation {
  action: Action;
  network: "mainnet" | "devnet";
  wallet: string;
  taskPda?: string;
  feePayer?: string;
  escrowLamports?: bigint;
  allowedProgramIds?: string[];
}

const SWITCHBOARD_PROGRAM_IDS = {
  mainnet: "SBondMDrcV3K4kxZR1HNVT7osZxAHVHgYXL5Ze1oMUv",
  devnet: "Aio4gaXjXzJNVLtzwtNVmSqGKpANtXhybbkhtAC94ji2",
} as const;

/** Canonical deployed ShillbotEscrow EVM contracts keyed by CAIP-2 chain ID. */
export const SHILLBOT_ESCROW_DEPLOYMENTS: Readonly<Record<string, `0x${string}`>> = {
  "eip155:84532": "0xaFe061778f9A76fCe7da4124dC89DAF8309E5F3c",
} as const;

/** Canonical 4-byte function selectors for `ShillbotEscrow.sol` lifecycle actions. */
export const SHILLBOT_EVM_SELECTORS: Readonly<Record<Action, `0x${string}`>> = {
  create: "0xa4aaf1b3", // createTask(bytes32,bytes32,uint8,uint64,bool)
  claim: "0xe6a81fdd", // claimTask(uint64)
  submit: "0xfb78a429", // submitWork(uint64,bytes32,bytes32)
  approve: "0x97312249", // approveTask(uint64)
  verify: "0x3bae608b", // verifyTaskAttested(uint64,uint64,bytes)
  finalize: "0xcb26e65b", // finalizeTask(uint64)
} as const;

export interface PreparedEvmCallExpectation {
  action: Action;
  chain?: string;
  maxValueWei?: bigint;
}

export interface ValidatedEvmCall {
  to: `0x${string}`;
  data: `0x${string}`;
  value: bigint;
  chain: string;
}

function normalizeEvmChainId(chain: string): string {
  const trimmed = chain.trim().toLowerCase();
  if (trimmed === "base-sepolia" || trimmed === "84532") return "eip155:84532";
  if (trimmed === "base" || trimmed === "8453") return "eip155:8453";
  if (trimmed === "ethereum" || trimmed === "1") return "eip155:1";
  if (trimmed === "ethereum-sepolia" || trimmed === "sepolia" || trimmed === "11155111") {
    return "eip155:11155111";
  }
  return trimmed;
}

/**
 * C10 fail-closed local validator for server-prepared EVM `ShillbotEscrow` calls.
 * Verifies canonical contract address for `chain`, 4-byte function selector for
 * `action`, and `value_wei <= maxValueWei` (strictly `0n` on non-create actions).
 */
export function validatePreparedEvmCall(
  evmCall: unknown,
  expected: PreparedEvmCallExpectation,
): ValidatedEvmCall {
  if (!evmCall || typeof evmCall !== "object") {
    throw new SwarmClientError({
      code: "TRANSACTION_MISMATCH",
      operation: "shillbot.validatePreparedEvmCall",
      message: "Prepared EVM call must be an object",
    });
  }
  const raw = evmCall as {
    to?: unknown;
    data?: unknown;
    value_wei?: unknown;
    value?: unknown;
    chain?: unknown;
    chain_id?: unknown;
  };
  if (typeof raw.to !== "string" || !/^0x[0-9a-fA-F]{40}$/.test(raw.to)) {
    throw new SwarmClientError({
      code: "TRANSACTION_MISMATCH",
      operation: "shillbot.validatePreparedEvmCall",
      message: "Prepared EVM call has an invalid target contract address",
    });
  }
  if (
    typeof raw.data !== "string" ||
    !/^0x[0-9a-fA-F]{8,}$/.test(raw.data) ||
    raw.data.length % 2 !== 0
  ) {
    throw new SwarmClientError({
      code: "TRANSACTION_MISMATCH",
      operation: "shillbot.validatePreparedEvmCall",
      message: "Prepared EVM call data must include at least a 4-byte function selector",
    });
  }

  const callChain =
    typeof raw.chain === "string"
      ? normalizeEvmChainId(raw.chain)
      : typeof raw.chain_id === "number"
      ? `eip155:${raw.chain_id}`
      : undefined;
  const expectedChain = expected.chain ? normalizeEvmChainId(expected.chain) : undefined;
  if (callChain && expectedChain && callChain !== expectedChain) {
    throw new SwarmClientError({
      code: "TRANSACTION_MISMATCH",
      operation: "shillbot.validatePreparedEvmCall",
      message: `Prepared EVM call chain ${callChain} differs from expected ${expectedChain}`,
    });
  }
  const resolvedChain = expectedChain ?? callChain ?? "eip155:84532";
  const canonicalContract = SHILLBOT_ESCROW_DEPLOYMENTS[resolvedChain];
  if (!canonicalContract) {
    throw new SwarmClientError({
      code: "TRANSACTION_MISMATCH",
      operation: "shillbot.validatePreparedEvmCall",
      message: `No canonical ShillbotEscrow contract deployed for chain ${resolvedChain}`,
    });
  }
  if (raw.to.toLowerCase() !== canonicalContract.toLowerCase()) {
    throw new SwarmClientError({
      code: "TRANSACTION_MISMATCH",
      operation: "shillbot.validatePreparedEvmCall",
      message: `Target contract ${raw.to} does not match canonical ShillbotEscrow ${canonicalContract}`,
    });
  }

  const expectedSelector = SHILLBOT_EVM_SELECTORS[expected.action];
  if (!expectedSelector) {
    throw new SwarmClientError({
      code: "INVALID_ARGUMENT",
      operation: "shillbot.validatePreparedEvmCall",
      message: `Unsupported Shillbot action: ${String(expected.action)}`,
    });
  }
  const actualSelector = raw.data.slice(0, 10).toLowerCase();
  if (actualSelector !== expectedSelector.toLowerCase()) {
    throw new SwarmClientError({
      code: "TRANSACTION_MISMATCH",
      operation: "shillbot.validatePreparedEvmCall",
      message: `Function selector ${actualSelector} does not match expected ${expectedSelector} for ${expected.action}`,
    });
  }

  let valueWei: bigint;
  try {
    const rawVal = raw.value_wei ?? raw.value ?? "0";
    valueWei = typeof rawVal === "bigint" ? rawVal : BigInt(String(rawVal));
  } catch {
    throw new SwarmClientError({
      code: "TRANSACTION_MISMATCH",
      operation: "shillbot.validatePreparedEvmCall",
      message: "Prepared EVM call value_wei is not a valid integer",
    });
  }
  if (valueWei < 0n) {
    throw new SwarmClientError({
      code: "TRANSACTION_MISMATCH",
      operation: "shillbot.validatePreparedEvmCall",
      message: "Prepared EVM call value_wei cannot be negative",
    });
  }
  if (expected.action !== "create" && valueWei !== 0n) {
    throw new SwarmClientError({
      code: "TRANSACTION_MISMATCH",
      operation: "shillbot.validatePreparedEvmCall",
      message: `Non-create Shillbot EVM action (${expected.action}) must have 0 value_wei, got ${valueWei}`,
    });
  }
  const maxAllowed = expected.action === "create" ? (expected.maxValueWei ?? 0n) : 0n;
  if (valueWei > maxAllowed) {
    throw new SwarmClientError({
      code: "TRANSACTION_MISMATCH",
      operation: "shillbot.validatePreparedEvmCall",
      message: `Prepared EVM call value_wei ${valueWei} exceeds maximum allowed ${maxAllowed}`,
    });
  }

  return {
    to: canonicalContract,
    data: raw.data as `0x${string}`,
    value: valueWei,
    chain: resolvedChain,
  };
}

/**
 * Estimate the maximum Solana network fee (base signature fees + ComputeBudget
 * priority fee) charged to `wallet` for an inspected transaction. Returns `0n`
 * when a third-party sponsor is the `fee_payer`.
 */
export function estimateSolanaNetworkFeeLamports(
  inspected: TransactionInspection,
  wallet: string,
): bigint {
  if (inspected.fee_payer !== wallet) return 0n;
  const computeBudgetId = ComputeBudgetProgram.programId.toBase58();
  let computeUnitLimit: bigint | undefined;
  let microLamportsPerCu = 0n;
  let nonComputeBudgetIxCount = 0;

  for (const ix of inspected.instructions) {
    if (ix.program_id !== computeBudgetId) {
      nonComputeBudgetIxCount += 1;
      continue;
    }
    const data = Buffer.from(ix.data_base64, "base64");
    if (data.length === 0) continue;
    const tag = data[0];
    if (tag === 2 && data.length >= 5) {
      computeUnitLimit = BigInt(data.readUInt32LE(1));
    } else if (tag === 3 && data.length >= 9) {
      const view = new DataView(data.buffer, data.byteOffset + 1, 8);
      microLamportsPerCu = view.getBigUint64(0, true);
    } else if (tag === 1 && data.length >= 9) {
      // Legacy RequestUnitsDeprecated: u32 units + u32 additionalFeeLamports
      const additionalFee = BigInt(data.readUInt32LE(5));
      return BigInt(inspected.signers.length) * 5_000n + additionalFee;
    }
  }

  const effectiveUnits =
    computeUnitLimit ?? BigInt(Math.max(1, nonComputeBudgetIxCount)) * 200_000n;
  const priorityFeeLamports =
    (effectiveUnits * microLamportsPerCu + 999_999n) / 1_000_000n;
  const baseSignatureFeeLamports = BigInt(Math.max(1, inspected.signers.length)) * 5_000n;
  return baseSignatureFeeLamports + priorityFeeLamports;
}

export function permittedPreparedProgramIds(
  action: Action,
  network: "mainnet" | "devnet",
): string[] {
  return action === "verify"
    ? [SWITCHBOARD_PROGRAM_IDS[network], Secp256k1Program.programId.toBase58()]
    : [];
}

function lifecycleName(action: Action): string {
  if (action === "create") return "create_task";
  if (action === "submit") return "submit_work";
  return `${action}_task`;
}

function matchesDiscriminator(dataBase64: string, action: Action): boolean {
  const data = Buffer.from(dataBase64, "base64");
  return data.subarray(0, 8).equals(shillbotInstructionDiscriminator(lifecycleName(action)));
}

/**
 * Fail-closed local validation for a server-prepared Shillbot transaction.
 * The network is an explicit part of the expectation; callers must construct
 * their Connection for that same network before signing.
 */
export function validatePreparedTransaction(
  encoded: string,
  expected: PreparedTransactionExpectation,
): TransactionInspection {
  if (expected.network !== "mainnet" && expected.network !== "devnet") {
    throw new SwarmClientError({
      code: "INVALID_ARGUMENT",
      operation: "shillbot.validatePreparedTransaction",
      message: "Unsupported network",
    });
  }
  const inspected = inspectTransaction(encoded);
  if (!inspected.signers.includes(expected.wallet)) {
    throw new SwarmClientError({
      code: "TRANSACTION_MISMATCH",
      operation: "shillbot.validatePreparedTransaction",
      message: "Wallet is not a required signer",
    });
  }
  if (expected.feePayer && inspected.fee_payer !== expected.feePayer) {
    throw new SwarmClientError({
      code: "TRANSACTION_MISMATCH",
      operation: "shillbot.validatePreparedTransaction",
      message: "Fee payer differs from expectation",
    });
  }
  const shillbotId = SHILLBOT_PROGRAM_ID.toBase58();
  const shillbotIxs = inspected.instructions.filter(
    (instruction) => instruction.program_id === shillbotId,
  );
  const lifecycle = shillbotIxs.filter((instruction) =>
    matchesDiscriminator(instruction.data_base64, expected.action),
  );
  const companions = shillbotIxs.filter((instruction) => instruction !== lifecycle[0]);
  const claimDisc = shillbotInstructionDiscriminator("claim_task");
  const payoutDisc = shillbotInstructionDiscriminator("set_payout_to");
  const hasDisc = (ix: TransactionInspection["instructions"][number], disc: Buffer) =>
    Buffer.from(ix.data_base64, "base64").subarray(0, 8).equals(disc);
  const ixPos = (ix: TransactionInspection["instructions"][number]) =>
    inspected.instructions.indexOf(ix);
  const validClaimCompanion = (ix: TransactionInspection["instructions"][number]) =>
    hasDisc(ix, claimDisc) &&
    ix.accounts.length >= 4 &&
    (!expected.taskPda || ix.accounts[0] === expected.taskPda) &&
    ix.accounts[0] === lifecycle[0]?.accounts[0] &&
    ix.accounts[3] === expected.wallet;
  const validPayoutCompanion = (ix: TransactionInspection["instructions"][number]) =>
    hasDisc(ix, payoutDisc) &&
    ix.accounts.length === 2 &&
    (!expected.taskPda || ix.accounts[0] === expected.taskPda) &&
    ix.accounts[0] === lifecycle[0]?.accounts[0] &&
    ix.accounts[1] === expected.wallet;
  const validCompanions =
    companions.length === 0 ||
    (expected.action === "claim" &&
      companions.length === 1 &&
      validPayoutCompanion(companions[0]) &&
      ixPos(companions[0]) > ixPos(lifecycle[0])) ||
    (expected.action === "submit" &&
      companions.length === 1 &&
      validClaimCompanion(companions[0]) &&
      ixPos(lifecycle[0]) === ixPos(companions[0]) + 1) ||
    (expected.action === "submit" &&
      companions.length === 2 &&
      validClaimCompanion(companions[0]) &&
      validPayoutCompanion(companions[1]) &&
      ixPos(companions[1]) === ixPos(companions[0]) + 1 &&
      ixPos(lifecycle[0]) === ixPos(companions[1]) + 1);
  if (lifecycle.length !== 1 || !validCompanions) {
    throw new SwarmClientError({
      code: "TRANSACTION_MISMATCH",
      operation: "shillbot.validatePreparedTransaction",
      message: "Expected exactly one matching Shillbot lifecycle instruction",
    });
  }
  const taskAccountIndex = expected.action === "create" ? 1 : 0;
  if (expected.taskPda && lifecycle[0].accounts[taskAccountIndex] !== expected.taskPda) {
    throw new SwarmClientError({
      code: "TRANSACTION_MISMATCH",
      operation: "shillbot.validatePreparedTransaction",
      message: "Task PDA differs from expectation",
    });
  }
  const alwaysAllowed = new Set([
    shillbotId,
    ComputeBudgetProgram.programId.toBase58(),
    ...permittedPreparedProgramIds(expected.action, expected.network),
    ...(expected.allowedProgramIds ?? []),
  ]);
  const unrelated = inspected.instructions.find(
    (instruction) => !alwaysAllowed.has(instruction.program_id),
  );
  if (unrelated) {
    throw new SwarmClientError({
      code: "TRANSACTION_MISMATCH",
      operation: "shillbot.validatePreparedTransaction",
      message: `Unexpected program ${unrelated.program_id}`,
    });
  }
  if (expected.action === "create" && expected.escrowLamports !== undefined) {
    const data = Buffer.from(lifecycle[0].data_base64, "base64");
    if (data.length < 24) {
      throw new SwarmClientError({
        code: "TRANSACTION_MISMATCH",
        operation: "shillbot.validatePreparedTransaction",
        message: "Create instruction is truncated",
      });
    }
    const view = new DataView(data.buffer, data.byteOffset + 16, 8);
    const amount = view.getBigUint64(0, true);
    if (amount !== expected.escrowLamports) {
      throw new SwarmClientError({
        code: "TRANSACTION_MISMATCH",
        operation: "shillbot.validatePreparedTransaction",
        message: `Escrow amount ${amount} differs from expected ${expected.escrowLamports}`,
      });
    }
  }
  return inspected;
}

export interface BuildOnChainActionProofRefParams {
  /** CAIP-2 target chain ID (e.g. `"eip155:56"`, `"eip155:8453"`, `"solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp"`). */
  targetChainId: string;
  /** Transaction hash (`0x` + 64 hex chars on EVM) or base58 signature (on Solana). */
  txId: string;
  /** Zero-based log index within the EVM receipt or matched instruction index on Solana. */
  index: number;
  /**
   * Optional 65-byte EIP-191 signature (`0x` + 130 hex chars or `Uint8Array`)
   * over `formatOnChainActionAttributionMessage(targetChainId, agentSolanaPubkey)`
   * binding an EVM referral address to the Solana settlement wallet (`M2a`).
   */
  attributionSignature?: string | Uint8Array;
}

/**
 * Format the canonical EIP-191 message signed by an agent's EVM referral wallet
 * to bind cross-chain EVM conversions to their Solana settlement pubkey (`M2a`).
 */
export function formatOnChainActionAttributionMessage(
  targetChainId: string,
  agentSolanaPubkey: string,
): string {
  const caip2 = targetChainId.trim();
  const agent = agentSolanaPubkey.trim();
  if (!caip2.includes(":") || !agent) {
    throw new SwarmClientError({
      code: "INVALID_ARGUMENT",
      operation: "shillbot.formatOnChainActionAttributionMessage",
      message: "targetChainId (CAIP-2) and agentSolanaPubkey are required",
    });
  }
  return `swarm-attr-v1:${caip2}:${agent}`;
}

/**
 * Build a canonical `content_id` proof reference for `PlatformType::OnChainAction = 11`:
 * `<caip2>:<tx_id>:<index>[:sig:<base64url_nopad_65B>]`.
 */
export function buildOnChainActionProofRef(params: BuildOnChainActionProofRefParams): string {
  const caip2 = params.targetChainId.trim();
  const txId = params.txId.trim();
  if (!caip2.includes(":") || !txId) {
    throw new SwarmClientError({
      code: "INVALID_ARGUMENT",
      operation: "shillbot.buildOnChainActionProofRef",
      message: "targetChainId and txId are required",
    });
  }
  if (!Number.isInteger(params.index) || params.index < 0) {
    throw new SwarmClientError({
      code: "INVALID_ARGUMENT",
      operation: "shillbot.buildOnChainActionProofRef",
      message: "index must be a non-negative integer",
    });
  }
  const normalizedTxId = caip2.startsWith("eip155:") ? txId.toLowerCase() : txId;
  if (params.attributionSignature === undefined) {
    return `${caip2}:${normalizedTxId}:${params.index}`;
  }
  let sigBytes: Buffer;
  if (typeof params.attributionSignature === "string") {
    const hex = params.attributionSignature.trim().replace(/^0x/i, "");
    if (!/^[0-9a-fA-F]{130}$/.test(hex)) {
      throw new SwarmClientError({
        code: "INVALID_ARGUMENT",
        operation: "shillbot.buildOnChainActionProofRef",
        message: "attributionSignature hex must be 65 bytes (130 hex chars)",
      });
    }
    sigBytes = Buffer.from(hex, "hex");
  } else {
    sigBytes = Buffer.from(params.attributionSignature);
  }
  if (sigBytes.length !== 65) {
    throw new SwarmClientError({
      code: "INVALID_ARGUMENT",
      operation: "shillbot.buildOnChainActionProofRef",
      message: `attributionSignature must be 65 bytes, got ${sigBytes.length}`,
    });
  }
  const b64url = sigBytes
    .toString("base64")
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/g, "");
  return `${caip2}:${normalizedTxId}:${params.index}:sig:${b64url}`;
}

export async function signBroadcastPreparedTransaction(input: {
  connection: Connection;
  transaction: string;
  expected: PreparedTransactionExpectation;
  sign: WalletSignCallback;
  confirmation?: BlockhashWithExpiryBlockHeight;
  timeoutMs?: number;
  setTimeout?: typeof globalThis.setTimeout;
  clearTimeout?: typeof globalThis.clearTimeout;
}): Promise<string> {
  validatePreparedTransaction(input.transaction, input.expected);
  const timeoutMs = input.timeoutMs ?? 60_000;
  let timeout: ReturnType<typeof globalThis.setTimeout> | undefined;
  try {
    return await Promise.race([
      signAndBroadcast(input.connection, input.transaction, input.sign, input.confirmation),
      new Promise<never>((_, reject) => {
        timeout = (input.setTimeout ?? globalThis.setTimeout)(
          () =>
            reject(
              new SwarmClientError({
                code: "TIMEOUT",
                operation: "shillbot.signBroadcastPreparedTransaction",
                message: `Transaction did not confirm within ${timeoutMs}ms`,
                retryable: true,
              }),
            ),
          timeoutMs,
        );
      }),
    ]);
  } finally {
    if (timeout !== undefined) (input.clearTimeout ?? globalThis.clearTimeout)(timeout);
  }
}
