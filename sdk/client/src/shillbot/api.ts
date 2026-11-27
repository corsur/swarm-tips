import { Buffer } from "buffer";
import {
  ComputeBudgetProgram,
  Connection,
  Secp256k1Program,
  type BlockhashWithExpiryBlockHeight,
} from "@solana/web3.js";
import { decodeFunctionData, encodeFunctionData, type Hex } from "viem";
import { HttpClient, type HttpClientOptions } from "../http.js";
import { SwarmClientError } from "../errors.js";
import {
  EXTENSION_CREDIT_PROGRAM_ID,
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
  transaction?: string;
  task_pda?: string;
  transaction_intent?: TransactionIntent;
  evm_call?: unknown;
  payout_to?: string;
  advance_backer?: string;
  attempt_id?: string;
  open_competition?: boolean;
  status?: string;
  onboarded_advance_tx?: string | null;
  required_payout_to?: string | null;
  co_signed_transaction?: string | null;
}

export type SubmitTaskResponse = PreparedTransactionResponse;

export type BuildVerifyResponse = PreparedTransactionResponse;

export interface VerificationDataResponse {
  task_id?: string;
  task_pda: string;
  composite_score: number;
  verification_hash: string;
  global_state?: string;
  switchboard_feed: string;
  required_payout_to?: string | null;
  advance_backer?: string | null;
  verification_kind?: number;
  co_signed_transaction?: string | null;
  evm_verify_call?: unknown;
  attempt_id?: string;
  onboarded_advance_tx?: string | null;
}

export interface BuildVerifyOptions {
  payer?: string;
  mcpBaseUrl?: string;
  network?: "mainnet" | "devnet";
  attemptId?: string;
}

export interface ConfirmTransactionRequest {
  tx_signature: string;
  action: Action;
  task_pda?: string;
}

export interface LeanEnvironmentInfo {
  lean_policy: number;
  lean_toolchain: string;
  mode: string;
  mathlib_rev?: string | null;
  zeta23_rev?: string | null;
  swarm_proofs_rev?: string | null;
  environment_id?: string | null;
  import_allowlist: string[];
  allows_disproof: boolean;
  max_proof_lines?: number | null;
  max_proof_bytes?: number | null;
  max_line_width?: number | null;
}

export interface TaskAttemptSummary {
  attempt_id: string;
  task_id?: string;
  agent?: string;
  status: string;
  score?: number | null;
  check_detail?: string | null;
  content_id?: string | null;
  artifact_hash?: string | null;
  submitted_at_unix?: number | null;
  verified_at?: string | null;
  updated_at?: string | null;
}

export interface TaskResponse {
  task_id: string;
  campaign_id?: string;
  campaign_topic?: string;
  state: string;
  platform?: number;
  estimated_payment_lamports?: number;
  quality_threshold?: number;
  created_at?: string;
  submitted_at?: string | null;
  claimed_at?: string | null;
  approved_at?: string | null;
  agent?: string | null;
  client?: string | null;
  task_pda?: string | null;
  content_id?: string | null;
  composite_score?: number | null;
  payment_amount?: number | null;
  requires_approval?: boolean;
  chain?: string;
  statement_lean?: string | null;
  lean_policy?: number | null;
  lean_environment?: LeanEnvironmentInfo | null;
  check_detail?: string | null;
  competing_attempts_count?: number | null;
  my_latest_attempt?: TaskAttemptSummary | null;
  brief?: unknown;
  [key: string]: unknown;
}

export interface LeanDiagnostic {
  file: string;
  line: number;
  column: number;
  severity: "error" | "warning" | "info" | string;
  message: string;
}

export interface ProofMetrics {
  code_lines: number;
  total_lines: number;
  code_bytes: number;
  raw_bytes: number;
  max_line_width: number;
}

export interface ProofLimits {
  max_proof_lines?: number | null;
  max_proof_bytes?: number | null;
  max_line_width?: number | null;
}

export interface PreflightTaskRequest {
  proof_lean?: string;
  compile?: boolean;
  content_id?: string;
  agent_wallet?: string;
}

export interface PreflightTaskResponse {
  ok: boolean;
  task_id: string;
  lean_policy?: number;
  detail: string;
  status?: string | null;
  compiled?: boolean;
  verdict?: string | null;
  axioms?: string[] | null;
  diagnostics?: LeanDiagnostic[] | null;
  metrics?: ProofMetrics | null;
  limits?: ProofLimits | null;
  lean_environment?: LeanEnvironmentInfo | null;
}

export interface SubmitTaskRequest {
  content_id?: string;
  proof_lean?: string;
  agent_harness?: string;
  model_family?: string;
  [key: string]: unknown;
}

export interface VerifiedProofRecord {
  task_id: string;
  campaign_id?: string | null;
  topic?: string | null;
  task_pda?: string | null;
  network?: string;
  lean_policy: number;
  solver_wallet: string;
  agent_harness?: string | null;
  model_family?: string | null;
  reward_lamports?: number | null;
  payment_amount?: number | null;
  composite_score?: number | null;
  artifact_sha256?: string | null;
  statement_sha256?: string | null;
  verification_hash?: string | null;
  proof_url?: string | null;
  statement_url?: string | null;
  code_lines?: number | null;
  code_bytes?: number | null;
  metrics?: ProofMetrics | null;
  verified_at?: string | null;
  finalized_at?: string | null;
  [key: string]: unknown;
}

export interface ProofsLeaderboardEntry {
  key: string;
  label?: string | null;
  wallet?: string | null;
  model_family?: string | null;
  agent_harness?: string | null;
  proofs_solved: number;
  total_lamports: number;
  median_code_lines?: number | null;
  shortest_code_lines?: number | null;
  [key: string]: unknown;
}

export interface ProofsResponse {
  network?: string;
  proofs: VerifiedProofRecord[];
  total?: number;
  leaderboard?: {
    by_wallet?: ProofsLeaderboardEntry[];
    by_model?: ProofsLeaderboardEntry[];
    by_harness?: ProofsLeaderboardEntry[];
  };
  [key: string]: unknown;
}

export interface ListProofsOptions {
  network?: "mainnet" | "devnet";
  limit?: number;
  policy?: number;
}

export interface TaskListResponse<TTask = TaskResponse> {
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
    return this.http.get<TaskListResponse<TTask>>(
      `/tasks?limit=${encodeURIComponent(String(limit))}`,
      null,
    );
  }
  async getTask<TTask = unknown>(id: string, network?: "mainnet" | "devnet"): Promise<TTask> {
    const trimmed = id.trim();
    if (!trimmed) {
      throw new SwarmClientError({
        code: "INVALID_ARGUMENT",
        operation: "shillbot.getTask",
        message: "Task id is required",
      });
    }
    const suffix = network ? `?network=${encodeURIComponent(network)}` : "";
    return this.http.get<TTask>(`/tasks/${encodeURIComponent(trimmed)}${suffix}`, null);
  }
  listAgentTasks<TTask = unknown>() {
    return this.get<TaskListResponse<TTask> | TTask[]>("/agent/tasks");
  }
  listPendingApproval<TTask = unknown>(
    networkOrOpts?: "mainnet" | "devnet" | { network?: "mainnet" | "devnet"; token?: string },
  ) {
    const network =
      typeof networkOrOpts === "string" ? networkOrOpts : networkOrOpts?.network;
    const token =
      typeof networkOrOpts === "object" && networkOrOpts !== null
        ? networkOrOpts.token
        : undefined;
    const suffix = network ? `?network=${encodeURIComponent(network)}` : "";
    return this.http.get<TaskListResponse<TTask> | TTask[]>(
      `/client/pending-approval${suffix}`,
      token,
    );
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
  submitTask(id: string, body: SubmitTaskRequest | Record<string, unknown>) {
    return this.post<SubmitTaskResponse>(`/tasks/${encodeURIComponent(id)}/submit`, body);
  }
  preflightTask<TResponse = PreflightTaskResponse>(
    taskId: string,
    body?: PreflightTaskRequest | Record<string, unknown>,
    opts?: { network?: "mainnet" | "devnet"; token?: string | null },
  ) {
    const trimmed = taskId.trim();
    if (!trimmed) {
      throw new SwarmClientError({
        code: "INVALID_ARGUMENT",
        operation: "shillbot.preflightTask",
        message: "Task id is required",
      });
    }
    const suffix = opts?.network ? `?network=${encodeURIComponent(opts.network)}` : "";
    return this.http.post<TResponse>(
      `/tasks/${encodeURIComponent(trimmed)}/preflight${suffix}`,
      body ?? {},
      opts?.token,
    );
  }
  listProofs<TResponse = ProofsResponse>(opts?: ListProofsOptions) {
    const params = new URLSearchParams();
    if (opts?.network) params.set("network", opts.network);
    if (opts?.limit !== undefined) params.set("limit", String(opts.limit));
    if (opts?.policy !== undefined) params.set("policy", String(opts.policy));
    const qs = params.toString();
    const path = qs ? `/proofs?${qs}` : "/proofs";
    return this.http.get<TResponse>(path, null);
  }
  approveTask(id: string) {
    return this.post<PreparedTransactionResponse>(`/tasks/${encodeURIComponent(id)}/approve`);
  }
  getVerificationData(id: string, network?: "mainnet" | "devnet", attemptId?: string) {
    const params: string[] = [];
    if (network && network !== "mainnet") {
      params.push(`network=${encodeURIComponent(network)}`);
    }
    const trimmedAttempt = attemptId?.trim();
    if (trimmedAttempt) {
      params.push(`attempt_id=${encodeURIComponent(trimmedAttempt)}`);
    }
    const suffix = params.length > 0 ? `?${params.join("&")}` : "";
    return this.post<VerificationDataResponse>(
      `/tasks/${encodeURIComponent(id)}/build-verify${suffix}`,
    );
  }
  async buildVerify(
    id: string,
    optsOrAttemptId?: BuildVerifyOptions | string,
  ): Promise<BuildVerifyResponse> {
    const opts: BuildVerifyOptions | undefined =
      typeof optsOrAttemptId === "string"
        ? { attemptId: optsOrAttemptId }
        : optsOrAttemptId;
    const mcpBaseUrl = (opts?.mcpBaseUrl ?? "https://mcp.swarm.tips").replace(/\/$/, "");
    const resolvedNetwork: "mainnet" | "devnet" =
      opts?.network ?? (this.http.network === "devnet" ? "devnet" : "mainnet");
    const token = await this.http.getToken?.();
    const payer = opts?.payer ?? decodeJwtWallet(token);
    const attemptId = opts?.attemptId?.trim() || undefined;

    const postMcpBuildVerify = async (
      payload: Record<string, unknown>,
    ): Promise<BuildVerifyResponse> => {
      const mcpHttp = new HttpClient({
        baseUrl: mcpBaseUrl,
        fetch: this.http.fetchImpl,
        getToken: this.http.getToken,
      });
      return mcpHttp.post<BuildVerifyResponse>("/internal/build-verify-tx", payload);
    };

    const normalizePrepared = (
      raw: BuildVerifyResponse,
      fallbackTaskPda?: string,
    ): BuildVerifyResponse => {
      const resolvedTx = raw.transaction || raw.co_signed_transaction || "";
      const resolvedPayoutTo = raw.payout_to ?? raw.required_payout_to ?? undefined;
      return {
        ...raw,
        task_id: raw.task_id ?? id,
        transaction: resolvedTx,
        task_pda: raw.task_pda ?? fallbackTaskPda,
        transaction_intent: raw.transaction_intent,
        ...(resolvedPayoutTo ? { payout_to: resolvedPayoutTo } : {}),
      };
    };

    const formatVerificationData = (vdata: VerificationDataResponse): BuildVerifyResponse => ({
      task_id: vdata.task_id ?? id,
      transaction: vdata.co_signed_transaction ?? "",
      task_pda: vdata.task_pda,
      ...(vdata.evm_verify_call !== undefined ? { evm_call: vdata.evm_verify_call } : {}),
      ...(vdata.required_payout_to
        ? {
            payout_to: vdata.required_payout_to,
            required_payout_to: vdata.required_payout_to,
          }
        : {}),
      ...(vdata.advance_backer ? { advance_backer: vdata.advance_backer } : {}),
      ...(vdata.co_signed_transaction
        ? { co_signed_transaction: vdata.co_signed_transaction }
        : {}),
      ...(vdata.attempt_id ? { attempt_id: vdata.attempt_id } : {}),
      ...(vdata.onboarded_advance_tx !== undefined
        ? { onboarded_advance_tx: vdata.onboarded_advance_tx }
        : {}),
    });

    // When targeting a specific M2 Open-Competition attemptId, call shillbot-api
    // `POST /tasks/:id/build-verify?attempt_id=...` directly (returns `co_signed_transaction`
    // + proof-gated `onboarded_advance_tx` + `required_payout_to` in 1 hop).
    if (attemptId) {
      const vdata = await this.getVerificationData(id, resolvedNetwork, attemptId);
      if (vdata.evm_verify_call || vdata.co_signed_transaction) {
        return formatVerificationData(vdata);
      }
      const fallback = await postMcpBuildVerify({
        task_id: id,
        ...(payer ? { payer } : {}),
        network: resolvedNetwork,
        attempt_id: attemptId,
        task_pda: vdata.task_pda,
        score: vdata.composite_score,
        hash: vdata.verification_hash,
        feed: vdata.switchboard_feed,
        ...(vdata.global_state ? { global_state: vdata.global_state } : {}),
      });
      return normalizePrepared(fallback, vdata.task_pda);
    }

    const oneHopBody: Record<string, unknown> = {
      task_id: id,
      network: resolvedNetwork,
      ...(payer ? { payer } : {}),
    };

    try {
      const direct = await postMcpBuildVerify(oneHopBody);
      return normalizePrepared(direct);
    } catch (err) {
      if (
        !(err instanceof SwarmClientError) ||
        err.code !== "HTTP_ERROR" ||
        (err.status !== 400 && err.status !== 422)
      ) {
        throw err;
      }
      const vdata = await this.getVerificationData(id, resolvedNetwork);
      if (vdata.evm_verify_call || vdata.co_signed_transaction) {
        return formatVerificationData(vdata);
      }
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
      return normalizePrepared(fallback, vdata.task_pda);
    }
  }
  buildFinalize(id: string) {
    return this.post<PreparedTransactionResponse>(`/tasks/${encodeURIComponent(id)}/build-finalize`);
  }
  buildExpire(id: string) {
    return this.post<PreparedTransactionResponse>(`/tasks/${encodeURIComponent(id)}/build-expire`);
  }
  confirmTask<TResponse = unknown>(id: string, body: ConfirmTransactionRequest) {
    return this.post<TResponse>(`/tasks/${encodeURIComponent(id)}/confirm`, body);
  }
}

import {
  MAX_COMPUTE_UNIT_LIMIT,
  MAX_COMPUTE_UNIT_PRICE_MICROLAMPORTS,
  SHILLBOT_INSTRUCTION_SPECS,
  deriveAdvancePda,
  validateComputeBudgetInstruction,
  validateComputeBudgetInstructions,
  verifyBuiltTransaction,
  type AuthoritySlotRule,
  type ExpireTaskState,
  type ShillbotInstructionName,
  type ShillbotInstructionSpec,
  type VerifyBuiltTransactionExpectation,
} from "./verify-tx.js";

export {
  EXTENSION_CREDIT_PROGRAM_ID,
  MAX_COMPUTE_UNIT_LIMIT,
  MAX_COMPUTE_UNIT_PRICE_MICROLAMPORTS,
  SHILLBOT_INSTRUCTION_SPECS,
  deriveAdvancePda,
  validateComputeBudgetInstruction,
  validateComputeBudgetInstructions,
  verifyBuiltTransaction,
  type AuthoritySlotRule,
  type ExpireTaskState,
  type ShillbotInstructionName,
  type ShillbotInstructionSpec,
  type VerifyBuiltTransactionExpectation,
};

export interface PreparedTransactionExpectation {
  action: Action;
  network: "mainnet" | "devnet";
  wallet: string;
  taskPda?: string;
  feePayer?: string;
  sponsor?: string;
  payoutTo?: string;
  expireTaskState?: ExpireTaskState;
  oracleAuthority?: string;
  escrowLamports?: bigint;
  contentId?: string;
  maxComputeUnitPriceMicroLamports?: bigint;
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
  expire: "0x5e9f3d19", // expireTask(uint64)
} as const;

/** Canonical ABI for `ShillbotEscrow.sol` lifecycle functions (`P2-27`). */
export const SHILLBOT_ESCROW_ABI = [
  {
    type: "function",
    name: "createTask",
    stateMutability: "payable",
    inputs: [
      { name: "statementCommitment", type: "bytes32" },
      { name: "policyId", type: "bytes32" },
      { name: "verificationKind", type: "uint8" },
      { name: "deadline", type: "uint64" },
      { name: "requiresApproval", type: "bool" },
    ],
    outputs: [{ name: "taskId", type: "uint64" }],
  },
  {
    type: "function",
    name: "claimTask",
    stateMutability: "nonpayable",
    inputs: [{ name: "taskId", type: "uint64" }],
    outputs: [],
  },
  {
    type: "function",
    name: "submitWork",
    stateMutability: "nonpayable",
    inputs: [
      { name: "taskId", type: "uint64" },
      { name: "contentIdHash", type: "bytes32" },
      { name: "artifactHash", type: "bytes32" },
    ],
    outputs: [],
  },
  {
    type: "function",
    name: "approveTask",
    stateMutability: "nonpayable",
    inputs: [{ name: "taskId", type: "uint64" }],
    outputs: [],
  },
  {
    type: "function",
    name: "verifyTaskAttested",
    stateMutability: "nonpayable",
    inputs: [
      { name: "taskId", type: "uint64" },
      { name: "score", type: "uint64" },
      { name: "sig", type: "bytes" },
    ],
    outputs: [],
  },
  {
    type: "function",
    name: "finalizeTask",
    stateMutability: "nonpayable",
    inputs: [{ name: "taskId", type: "uint64" }],
    outputs: [],
  },
  {
    type: "function",
    name: "expireTask",
    stateMutability: "nonpayable",
    inputs: [{ name: "taskId", type: "uint64" }],
    outputs: [],
  },
] as const;

const SHILLBOT_EVM_FUNCTION_NAMES: Readonly<
  Record<Action, (typeof SHILLBOT_ESCROW_ABI)[number]["name"]>
> = {
  create: "createTask",
  claim: "claimTask",
  submit: "submitWork",
  approve: "approveTask",
  verify: "verifyTaskAttested",
  finalize: "finalizeTask",
  expire: "expireTask",
};

export interface PreparedEvmCallExpectation {
  action: Action;
  chain?: string;
  maxValueWei?: bigint;
  taskId?: bigint | number | string;
  contentIdHash?: `0x${string}`;
  artifactHash?: `0x${string}`;
  statementCommitment?: `0x${string}`;
  policyId?: `0x${string}`;
  verificationKind?: number;
  deadline?: bigint | number | string;
  requiresApproval?: boolean;
  score?: bigint | number | string;
}

export interface ValidatedEvmCall {
  to: `0x${string}`;
  data: `0x${string}`;
  value: bigint;
  chain: string;
}

function parseExpectedUintArg(value: bigint | number | string, field: string): bigint {
  let parsed: bigint;
  if (typeof value === "bigint") {
    parsed = value;
  } else if (typeof value === "number") {
    if (!Number.isSafeInteger(value) || value < 0) {
      throw new SwarmClientError({
        code: "INVALID_ARGUMENT",
        operation: "shillbot.validatePreparedEvmCall",
        message: `Expected ${field} must be a non-negative safe integer`,
      });
    }
    parsed = BigInt(value);
  } else if (typeof value === "string" && /^\d+$/.test(value.trim())) {
    parsed = BigInt(value.trim());
  } else {
    throw new SwarmClientError({
      code: "INVALID_ARGUMENT",
      operation: "shillbot.validatePreparedEvmCall",
      message: `Expected ${field} must be a non-negative integer`,
    });
  }
  if (parsed < 0n || parsed > 0xffff_ffff_ffff_ffffn) {
    throw new SwarmClientError({
      code: "INVALID_ARGUMENT",
      operation: "shillbot.validatePreparedEvmCall",
      message: `Expected ${field} is out of uint64 range`,
    });
  }
  return parsed;
}

function normalizeExpectedBytes32(value: string, field: string): `0x${string}` {
  const trimmed = value.trim();
  if (!/^0x[0-9a-fA-F]{64}$/.test(trimmed)) {
    throw new SwarmClientError({
      code: "INVALID_ARGUMENT",
      operation: "shillbot.validatePreparedEvmCall",
      message: `Expected ${field} must be a 32-byte 0x-prefixed hex string`,
    });
  }
  return trimmed.toLowerCase() as `0x${string}`;
}

export function normalizeEvmChainId(chain: string | number): string {
  if (typeof chain === "number") {
    if (!Number.isSafeInteger(chain) || chain <= 0) {
      throw new SwarmClientError({
        code: "INVALID_ARGUMENT",
        operation: "shillbot.normalizeEvmChainId",
        message: `Invalid EVM chain ID: ${String(chain)}`,
      });
    }
    return `eip155:${chain}`;
  }
  const trimmed = chain.trim().toLowerCase();
  if (/^\d+$/.test(trimmed)) return `eip155:${trimmed}`;
  if (trimmed === "base-sepolia") return "eip155:84532";
  if (trimmed === "base") return "eip155:8453";
  if (trimmed === "ethereum") return "eip155:1";
  if (trimmed === "ethereum-sepolia" || trimmed === "sepolia") {
    return "eip155:11155111";
  }
  return trimmed;
}

/**
 * C10 + P2-27 fail-closed local validator for server-prepared EVM `ShillbotEscrow` calls.
 * Verifies canonical contract address for `chain`, 4-byte function selector and full ABI-decoded
 * calldata arguments for `action`, and `value_wei <= maxValueWei` (strictly `0n` on non-create actions).
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
      ? normalizeEvmChainId(raw.chain_id)
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
  const expectedFnName = SHILLBOT_EVM_FUNCTION_NAMES[expected.action];
  if (!expectedSelector || !expectedFnName) {
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

  // Decode calldata via the canonical ShillbotEscrow ABI and verify canonical encoding + arguments.
  let decoded: ReturnType<typeof decodeFunctionData<typeof SHILLBOT_ESCROW_ABI>>;
  try {
    decoded = decodeFunctionData({
      abi: SHILLBOT_ESCROW_ABI,
      data: raw.data as Hex,
    });
  } catch (err) {
    throw new SwarmClientError({
      code: "TRANSACTION_MISMATCH",
      operation: "shillbot.validatePreparedEvmCall",
      message: `Prepared EVM call data failed ABI decoding for ${expectedFnName}: ${
        err instanceof Error ? err.message : String(err)
      }`,
    });
  }
  if (decoded.functionName !== expectedFnName) {
    throw new SwarmClientError({
      code: "TRANSACTION_MISMATCH",
      operation: "shillbot.validatePreparedEvmCall",
      message: `Decoded function ${decoded.functionName} does not match expected ${expectedFnName}`,
    });
  }
  const canonicalData = encodeFunctionData({
    abi: SHILLBOT_ESCROW_ABI,
    functionName: decoded.functionName,
    args: decoded.args,
  } as Parameters<typeof encodeFunctionData<typeof SHILLBOT_ESCROW_ABI>>[0]);
  if (canonicalData.toLowerCase() !== raw.data.toLowerCase()) {
    throw new SwarmClientError({
      code: "TRANSACTION_MISMATCH",
      operation: "shillbot.validatePreparedEvmCall",
      message: `Prepared EVM call data for ${expectedFnName} is non-canonical or has trailing bytes`,
    });
  }

  if (decoded.functionName === "createTask") {
    const [statementCommitment, policyId, verificationKind, deadline, requiresApproval] =
      decoded.args;
    if (expected.statementCommitment !== undefined) {
      const want = normalizeExpectedBytes32(expected.statementCommitment, "statementCommitment");
      if (statementCommitment.toLowerCase() !== want) {
        throw new SwarmClientError({
          code: "TRANSACTION_MISMATCH",
          operation: "shillbot.validatePreparedEvmCall",
          message: `createTask statementCommitment ${statementCommitment} differs from expected ${want}`,
        });
      }
    }
    if (expected.policyId !== undefined) {
      const want = normalizeExpectedBytes32(expected.policyId, "policyId");
      if (policyId.toLowerCase() !== want) {
        throw new SwarmClientError({
          code: "TRANSACTION_MISMATCH",
          operation: "shillbot.validatePreparedEvmCall",
          message: `createTask policyId ${policyId} differs from expected ${want}`,
        });
      }
    }
    if (expected.verificationKind !== undefined) {
      if (
        !Number.isInteger(expected.verificationKind) ||
        expected.verificationKind < 0 ||
        expected.verificationKind > 255
      ) {
        throw new SwarmClientError({
          code: "INVALID_ARGUMENT",
          operation: "shillbot.validatePreparedEvmCall",
          message: "Expected verificationKind must be an integer in [0, 255]",
        });
      }
      if (Number(verificationKind) !== expected.verificationKind) {
        throw new SwarmClientError({
          code: "TRANSACTION_MISMATCH",
          operation: "shillbot.validatePreparedEvmCall",
          message: `createTask verificationKind ${verificationKind} differs from expected ${expected.verificationKind}`,
        });
      }
    }
    if (expected.deadline !== undefined) {
      const want = parseExpectedUintArg(expected.deadline, "deadline");
      if (deadline !== want) {
        throw new SwarmClientError({
          code: "TRANSACTION_MISMATCH",
          operation: "shillbot.validatePreparedEvmCall",
          message: `createTask deadline ${deadline} differs from expected ${want}`,
        });
      }
    }
    if (expected.requiresApproval !== undefined && requiresApproval !== expected.requiresApproval) {
      throw new SwarmClientError({
        code: "TRANSACTION_MISMATCH",
        operation: "shillbot.validatePreparedEvmCall",
        message: `createTask requiresApproval ${requiresApproval} differs from expected ${expected.requiresApproval}`,
      });
    }
  } else if (
    decoded.functionName === "claimTask" ||
    decoded.functionName === "approveTask" ||
    decoded.functionName === "finalizeTask" ||
    decoded.functionName === "expireTask"
  ) {
    const [taskId] = decoded.args;
    if (expected.taskId !== undefined) {
      const want = parseExpectedUintArg(expected.taskId, "taskId");
      if (taskId !== want) {
        throw new SwarmClientError({
          code: "TRANSACTION_MISMATCH",
          operation: "shillbot.validatePreparedEvmCall",
          message: `${decoded.functionName} taskId ${taskId} differs from expected ${want}`,
        });
      }
    }
  } else if (decoded.functionName === "submitWork") {
    const [taskId, contentIdHash, artifactHash] = decoded.args;
    if (expected.taskId !== undefined) {
      const want = parseExpectedUintArg(expected.taskId, "taskId");
      if (taskId !== want) {
        throw new SwarmClientError({
          code: "TRANSACTION_MISMATCH",
          operation: "shillbot.validatePreparedEvmCall",
          message: `submitWork taskId ${taskId} differs from expected ${want}`,
        });
      }
    }
    if (expected.contentIdHash !== undefined) {
      const want = normalizeExpectedBytes32(expected.contentIdHash, "contentIdHash");
      if (contentIdHash.toLowerCase() !== want) {
        throw new SwarmClientError({
          code: "TRANSACTION_MISMATCH",
          operation: "shillbot.validatePreparedEvmCall",
          message: `submitWork contentIdHash ${contentIdHash} differs from expected ${want}`,
        });
      }
    }
    if (expected.artifactHash !== undefined) {
      const want = normalizeExpectedBytes32(expected.artifactHash, "artifactHash");
      if (artifactHash.toLowerCase() !== want) {
        throw new SwarmClientError({
          code: "TRANSACTION_MISMATCH",
          operation: "shillbot.validatePreparedEvmCall",
          message: `submitWork artifactHash ${artifactHash} differs from expected ${want}`,
        });
      }
    }
  } else if (decoded.functionName === "verifyTaskAttested") {
    const [taskId, score] = decoded.args;
    if (expected.taskId !== undefined) {
      const want = parseExpectedUintArg(expected.taskId, "taskId");
      if (taskId !== want) {
        throw new SwarmClientError({
          code: "TRANSACTION_MISMATCH",
          operation: "shillbot.validatePreparedEvmCall",
          message: `verifyTaskAttested taskId ${taskId} differs from expected ${want}`,
        });
      }
    }
    if (expected.score !== undefined) {
      const want = parseExpectedUintArg(expected.score, "score");
      if (score !== want) {
        throw new SwarmClientError({
          code: "TRANSACTION_MISMATCH",
          operation: "shillbot.validatePreparedEvmCall",
          message: `verifyTaskAttested score ${score} differs from expected ${want}`,
        });
      }
    }
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
  let additionalFeeLamports = 0n;
  let nonComputeBudgetIxCount = 0;

  for (const ix of inspected.instructions) {
    if (ix.program_id !== computeBudgetId) {
      nonComputeBudgetIxCount += 1;
      continue;
    }
    const data = Buffer.from(ix.data_base64, "base64");
    if (data.length === 0) continue;
    const tag = data[0];
    if (tag === 0 && data.length >= 9) {
      // Legacy RequestUnitsDeprecated: u32 units + u32 additionalFeeLamports (never early-return!)
      computeUnitLimit = BigInt(data.readUInt32LE(1));
      additionalFeeLamports += BigInt(data.readUInt32LE(5));
      continue;
    }
    if (tag === 1) {
      // RequestHeapFrame(u32 bytes): does not alter fee, continue checking subsequent instructions!
      continue;
    }
    if (tag === 2 && data.length >= 5) {
      computeUnitLimit = BigInt(data.readUInt32LE(1));
      continue;
    }
    if (tag === 3 && data.length >= 9) {
      const view = new DataView(data.buffer, data.byteOffset + 1, 8);
      const price = view.getBigUint64(0, true);
      if (price > microLamportsPerCu) {
        microLamportsPerCu = price;
      }
      continue;
    }
  }

  const effectiveUnits =
    computeUnitLimit ?? BigInt(Math.max(1, nonComputeBudgetIxCount)) * 200_000n;
  const priorityFeeLamports =
    (effectiveUnits * microLamportsPerCu + 999_999n) / 1_000_000n;
  const baseSignatureFeeLamports = BigInt(Math.max(1, inspected.signers.length)) * 5_000n;
  return baseSignatureFeeLamports + priorityFeeLamports + additionalFeeLamports;
}

export function permittedPreparedProgramIds(
  action: Action,
  network: "mainnet" | "devnet",
): string[] {
  if (action === "verify" && network === "devnet") {
    return [SWITCHBOARD_PROGRAM_IDS.devnet, Secp256k1Program.programId.toBase58()];
  }
  if (action === "finalize") {
    return [EXTENSION_CREDIT_PROGRAM_ID.toBase58()];
  }
  return [];
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
  return verifyBuiltTransaction(encoded, expected, "shillbot.validatePreparedTransaction");
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
