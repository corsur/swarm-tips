import { Buffer } from "buffer";
import { ComputeBudgetProgram, Connection, Secp256k1Program, SystemProgram, type BlockhashWithExpiryBlockHeight } from "@solana/web3.js";
import { HttpClient, type HttpClientOptions } from "../http.js";
import { SwarmClientError } from "../errors.js";
import {
  SHILLBOT_PROGRAM_ID,
  inspectTransaction,
  shillbotInstructionDiscriminator,
  signAndBroadcast,
  type Action,
  type TransactionInspection,
  type WalletSignCallback,
} from "./index.js";

export type { Action } from "./index.js";

export interface PreparedTransactionResponse {
  message?: string;
  task_id?: string;
  transaction: string;
  task_pda?: string;
  evm_call?: unknown;
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

export class ShillbotApiClient {
  readonly http: HttpClient;

  constructor(options: HttpClientOptions) { this.http = new HttpClient(options); }
  get<T>(path: string) { return this.http.get<T>(path); }
  post<T>(path: string, body?: unknown) { return this.http.post<T>(path, body); }
  patch<T>(path: string, body?: unknown) { return this.http.patch<T>(path, body); }

  listTasks<TTask = unknown>(limit = 50) { return this.get<TaskListResponse<TTask>>(`/tasks?limit=${encodeURIComponent(String(limit))}`); }
  listAgentTasks<TTask = unknown>() { return this.get<TaskListResponse<TTask> | TTask[]>("/agent/tasks"); }
  listPendingApproval<TTask = unknown>() { return this.get<TaskListResponse<TTask> | TTask[]>("/tasks?status=pending_approval"); }
  listCampaigns<TCampaign = unknown>() { return this.get<CampaignListResponse<TCampaign> | TCampaign[]>("/campaigns"); }
  campaign<TCampaign = unknown>(id: string) { return this.get<TCampaign>(`/campaigns/${encodeURIComponent(id)}`); }
  campaignTasks<TTask = unknown>(id: string) { return this.get<TaskListResponse<TTask>>(`/campaigns/${encodeURIComponent(id)}/tasks`); }
  campaignMetrics<TMetrics = unknown>(id: string) { return this.get<TMetrics>(`/campaigns/${encodeURIComponent(id)}/metrics`); }
  createCampaign<TResponse = { campaign_id: string }>(body: unknown) { return this.post<TResponse>("/campaigns", body); }
  updateCampaign<TResponse = unknown>(id: string, action: string) { return this.patch<TResponse>(`/campaigns/${encodeURIComponent(id)}`, { action }); }
  fundCampaignTask(id: string, body: unknown) { return this.post<PreparedTransactionResponse>(`/campaigns/${encodeURIComponent(id)}/fund`, body); }
  onboard<TResponse = unknown>() { return this.post<TResponse>("/agent/onboard"); }
  earnings<TResponse = unknown>() { return this.get<TResponse>("/agent/earnings"); }
  claimTask(id: string, sponsor = "auto") { return this.post<PreparedTransactionResponse>(`/tasks/${encodeURIComponent(id)}/claim?sponsor=${encodeURIComponent(sponsor)}`); }
  submitTask(id: string, body: unknown) { return this.post<PreparedTransactionResponse>(`/tasks/${encodeURIComponent(id)}/submit`, body); }
  approveTask(id: string) { return this.post<PreparedTransactionResponse>(`/tasks/${encodeURIComponent(id)}/approve`); }
  buildVerify(id: string) { return this.post<PreparedTransactionResponse>(`/tasks/${encodeURIComponent(id)}/build-verify`); }
  buildFinalize(id: string) { return this.post<PreparedTransactionResponse>(`/tasks/${encodeURIComponent(id)}/build-finalize`); }
  confirmTask<TResponse = unknown>(id: string, body: ConfirmTransactionRequest) { return this.post<TResponse>(`/tasks/${encodeURIComponent(id)}/confirm`, body); }
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
    throw new SwarmClientError({ code: "INVALID_ARGUMENT", operation: "shillbot.validatePreparedTransaction", message: "Unsupported network" });
  }
  const inspected = inspectTransaction(encoded);
  if (!inspected.signers.includes(expected.wallet)) {
    throw new SwarmClientError({ code: "TRANSACTION_MISMATCH", operation: "shillbot.validatePreparedTransaction", message: "Wallet is not a required signer" });
  }
  if (expected.feePayer && inspected.fee_payer !== expected.feePayer) {
    throw new SwarmClientError({ code: "TRANSACTION_MISMATCH", operation: "shillbot.validatePreparedTransaction", message: "Fee payer differs from expectation" });
  }
  const shillbotId = SHILLBOT_PROGRAM_ID.toBase58();
  const lifecycle = inspected.instructions.filter((instruction) => instruction.program_id === shillbotId && matchesDiscriminator(instruction.data_base64, expected.action));
  if (lifecycle.length !== 1) {
    throw new SwarmClientError({ code: "TRANSACTION_MISMATCH", operation: "shillbot.validatePreparedTransaction", message: "Expected exactly one matching Shillbot lifecycle instruction" });
  }
  if (expected.taskPda && !lifecycle[0].accounts.includes(expected.taskPda)) {
    throw new SwarmClientError({ code: "TRANSACTION_MISMATCH", operation: "shillbot.validatePreparedTransaction", message: "Task PDA differs from expectation" });
  }
  const alwaysAllowed = new Set([
    shillbotId,
    ComputeBudgetProgram.programId.toBase58(),
    SystemProgram.programId.toBase58(),
    ...permittedPreparedProgramIds(expected.action, expected.network),
    ...(expected.allowedProgramIds ?? []),
  ]);
  const unrelated = inspected.instructions.find((instruction) => !alwaysAllowed.has(instruction.program_id));
  if (unrelated) {
    throw new SwarmClientError({ code: "TRANSACTION_MISMATCH", operation: "shillbot.validatePreparedTransaction", message: `Unexpected program ${unrelated.program_id}` });
  }
  if (expected.action === "create" && expected.escrowLamports !== undefined) {
    const data = Buffer.from(lifecycle[0].data_base64, "base64");
    if (data.length < 24) throw new SwarmClientError({ code: "TRANSACTION_MISMATCH", operation: "shillbot.validatePreparedTransaction", message: "Create instruction is truncated" });
    const view = new DataView(data.buffer, data.byteOffset + 16, 8);
    const amount = view.getBigUint64(0, true);
    if (amount !== expected.escrowLamports) {
      throw new SwarmClientError({ code: "TRANSACTION_MISMATCH", operation: "shillbot.validatePreparedTransaction", message: `Escrow amount ${amount} differs from expected ${expected.escrowLamports}` });
    }
  }
  return inspected;
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
          () => reject(new SwarmClientError({ code: "TIMEOUT", operation: "shillbot.signBroadcastPreparedTransaction", message: `Transaction did not confirm within ${timeoutMs}ms`, retryable: true })),
          timeoutMs,
        );
      }),
    ]);
  } finally {
    if (timeout !== undefined) (input.clearTimeout ?? globalThis.clearTimeout)(timeout);
  }
}
