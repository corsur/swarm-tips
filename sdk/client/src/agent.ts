import { Buffer } from "buffer";
import { Connection, PublicKey, Transaction, VersionedTransaction } from "@solana/web3.js";
import { WalletTokenManager } from "./auth/token-manager.js";
import { CoordinationGameApiClient } from "./coordination-game/api.js";
import { SwarmClientError } from "./errors.js";
import { HttpClient, type FetchLike } from "./http.js";
import {
  InboxClient,
  type AckMessageResult,
  type InboxListOptions,
  type InboxListPage,
  type MessagePage,
  type MessageReference,
  type MuteReceipt,
  type OpenMessageResult,
  type PublishReceipt,
  type ReportReceipt,
  type SendReceipt,
  type TopicId,
  type TopicPage,
} from "./inbox/client.js";
import {
  ShillbotApiClient,
  estimateSolanaNetworkFeeLamports,
  normalizeEvmChainId,
  signBroadcastPreparedTransaction,
  validatePreparedEvmCall,
  validatePreparedTransaction,
  type Action,
  type BuildVerifyResponse,
  type CampaignListResponse,
  type ListProofsOptions,
  type PreflightTaskRequest,
  type PreflightTaskResponse,
  type PreparedTransactionResponse,
  type ProofsResponse,
  type SubmitTaskRequest,
  type SubmitTaskResponse,
  type TaskListResponse,
} from "./shillbot/api.js";
import {
  EXTENSION_CREDIT_PROGRAM_ID,
  SHILLBOT_PROGRAM_ID,
  advancePda,
  decodeExtensionCreditAdvance,
  inspectTransaction,
  shillbotInstructionDiscriminator,
} from "./shillbot/index.js";
import type { EvmSwarmSigner, SolanaSwarmSigner, SwarmSigner } from "./signer/types.js";
import {
  SwarmClient,
  type AgentJob,
  type AgentReputation,
  type EigenTrustRecord,
  type SearchHit,
} from "./swarm/index.js";
import type { Verdict } from "./vow/types.js";
import { verifyVow } from "./vow/verify.js";

export type SwarmNetwork = "mainnet" | "devnet";

/** Default EVM gas price ceiling (`maxFeePerGas` = 50 gwei). */
export const DEFAULT_MAX_NETWORK_FEE_WEI = 50_000_000_000n;

/** Recoupable SOL advance (`0.0026 SOL` = `2_600_000` lamports) fronted by proof-gated onboarding / `/agent/onboard`. */
export const ONBOARD_ADVANCE_LAMPORTS = 2_600_000;

/** Minimum SOL balance (`0.00255 SOL` = `2_550_000` lamports) required for self-paid `AgentState` initialization + lifecycle fees. */
export const MIN_SELF_PAID_SOLANA_BALANCE_LAMPORTS = 2_550_000;

/** Minimum SOL balance (`50_000` lamports) required for standalone post-claim `submit_work` fee payment. */
const MIN_POST_CLAIM_SUBMIT_SOLANA_BALANCE_LAMPORTS = 50_000;

export interface SwarmAgentOptions {
  signer?: SwarmSigner;
  token?: string | (() => Promise<string | undefined>);
  network?: SwarmNetwork;
  rpcUrl?: string;
  shillbotUrl?: string;
  gameUrl?: string;
  mcpUrl?: string;
  /** Automatically call `onboard()` when a Solana wallet lacks SOL for self-paid claim/submit; default `true`. */
  autoOnboard?: boolean;
  /** Maximum SOL escrow spend allowed per task creation; default `0n` (C9). */
  maxEscrowSpendLamports?: bigint;
  /** Maximum ETH/EVM escrow spend allowed per task creation; default `0n` (C9). */
  maxEscrowSpendWei?: bigint;
  /** Maximum Solana network fee (base + priority) allowed when wallet is fee payer; default `100_000n` = 0.0001 SOL (C9). */
  maxNetworkFeeLamports?: bigint;
  /** Maximum EVM gas price ceiling (`maxFeePerGas`) in wei; default `50_000_000_000n` = 50 gwei (C9). */
  maxNetworkFeeWei?: bigint;
  fetch?: typeof fetch;
}

export type TaskLifecycleStep =
  | "claim"
  | "submit"
  | "wait_attestation"
  | "verify"
  | "wait_challenge"
  | "finalize"
  | "done";

export interface NextTaskAction {
  step: TaskLifecycleStep;
  not_before?: string;
}

export interface ExecutedTaskResult<TConfirm = unknown> {
  task_id: string;
  signature: string;
  task_pda?: string;
  confirmation?: TConfirm;
  attempt_id?: string;
  open_competition?: boolean;
  status?: string;
  onboarded_advance_tx?: string | null;
}

export interface PreflightTaskOptions {
  proof_lean?: string;
  proofLean?: string;
  compile?: boolean;
  content_id?: string;
  contentId?: string;
  agent_wallet?: string;
  agentWallet?: string;
  network?: SwarmNetwork;
  token?: string | null;
}

export interface ExecuteTaskOptions {
  sponsor?: "auto" | "true" | "none";
  /** Automatically call `onboard()` if `?sponsor=auto` returns self-paid on an underfunded wallet; default `true`. */
  autoOnboard?: boolean;
  connection?: Connection;
  /** Optional explicit sponsor fee payer pubkey (base58). */
  feePayer?: string;
  /** Explicit authorized payout destination pubkey (base58) when `set_payout_to` is expected. */
  payoutTo?: string;
  /** Optional inline Lean 4 proof source forwarded as `proof_lean` on `POST /tasks/:id/submit`. */
  proofLean?: string;
  /** Optional self-reported prover harness tag forwarded on `POST /tasks/:id/submit`. */
  agentHarness?: string;
  /** Optional self-reported model family tag forwarded on `POST /tasks/:id/submit`. */
  modelFamily?: string;
  /** Optional Open-Competition (`M2`) attempt ID forwarded on `POST /tasks/:id/build-verify`. */
  attemptId?: string;
  extra?: Record<string, unknown>;
}

const DEFAULT_SOLANA_RPC: Record<SwarmNetwork, string> = {
  mainnet: "https://api.mainnet-beta.solana.com",
  devnet: "https://api.devnet.solana.com",
};

const ACTIVE_TOURNAMENT_ID: Record<SwarmNetwork, number> = {
  mainnet: 3,
  devnet: 1003,
};

function isFeePayerAlreadySigned(encodedTx: string): boolean {
  const bytes = Buffer.from(encodedTx, "base64");
  try {
    const vtx = VersionedTransaction.deserialize(bytes);
    return vtx.signatures.length > 0 && vtx.signatures[0].some((b) => b !== 0);
  } catch {
    const ltx = Transaction.from(bytes);
    const sig = ltx.signatures[0]?.signature;
    return Boolean(sig && sig.some((b) => b !== 0));
  }
}

function parseTimestampMs(value: unknown): number | undefined {
  if (typeof value === "number" && Number.isFinite(value) && value > 0) {
    return value > 1e12 ? value : value * 1_000;
  }
  if (typeof value === "string" && value.trim()) {
    const trimmed = value.trim();
    if (/^\d+$/.test(trimmed)) {
      const n = Number(trimmed);
      return n > 1e12 ? n : n * 1_000;
    }
    const parsed = Date.parse(trimmed);
    if (!Number.isNaN(parsed)) return parsed;
  }
  return undefined;
}

function extractBodyBigInt(
  body: unknown,
  keys: readonly string[],
): bigint | undefined {
  if (!body || typeof body !== "object") return undefined;
  const rec = body as Record<string, unknown>;
  for (const key of keys) {
    const val = rec[key];
    if (typeof val === "bigint") return val;
    if (typeof val === "number" && Number.isFinite(val)) return BigInt(Math.trunc(val));
    if (typeof val === "string" && /^\d+$/.test(val.trim())) return BigInt(val.trim());
  }
  return undefined;
}

const SHILLBOT_PROGRAM_ID_B58 = SHILLBOT_PROGRAM_ID.toBase58();
const CREATE_TASK_DISCRIMINATOR = shillbotInstructionDiscriminator("create_task");

/**
 * Extract `escrow_lamports` (`u64` at bytes `[16..24]`) strictly from a Shillbot
 * `create_task` instruction (`program_id === SHILLBOT_PROGRAM_ID` AND
 * `data[0..8] === sha256("global:create_task")[0..8]`).
 */
export function extractCreateEscrowLamports(encodedTx: string): bigint | undefined {
  try {
    const inspected = inspectTransaction(encodedTx);
    for (const ix of inspected.instructions) {
      if (ix.program_id !== SHILLBOT_PROGRAM_ID_B58) continue;
      const data = Buffer.from(ix.data_base64, "base64");
      if (data.length >= 24 && data.subarray(0, 8).equals(CREATE_TASK_DISCRIMINATOR)) {
        const view = new DataView(data.buffer, data.byteOffset + 16, 8);
        return view.getBigUint64(0, true);
      }
    }
  } catch {
    // Validated structurally right after via validatePreparedTransaction.
  }
  return undefined;
}

/**
 * Unified non-custodial client for Swarm Tips, Shillbot, Inbox/Topics, and Coordination Game.
 */
export class SwarmAgent {
  readonly network: SwarmNetwork;
  readonly rpcUrl: string;
  readonly shillbotUrl: string;
  readonly gameUrl: string;
  readonly mcpUrl: string;
  readonly autoOnboard: boolean;
  readonly maxEscrowSpendLamports: bigint;
  readonly maxEscrowSpendWei: bigint;
  readonly maxNetworkFeeLamports: bigint;
  readonly maxNetworkFeeWei: bigint;

  private readonly signer?: SwarmSigner;
  private readonly fetchImpl: FetchLike;
  private readonly tokenManager: WalletTokenManager;
  private readonly shillbotClient: ShillbotApiClient;
  private readonly swarmClient: SwarmClient;
  private readonly inboxClient: InboxClient;
  private readonly gameClient: CoordinationGameApiClient;
  private autoOnboardAttempted = false;

  readonly auth: {
    getToken: () => Promise<string | undefined>;
    authenticate: () => Promise<string>;
    readonly walletAddress: string | undefined;
  };

  readonly discovery: {
    listings: (opts?: { limit?: number; category?: string; chain?: string }) => Promise<AgentJob[]>;
    searchMcp: (
      query: string,
      opts?: { limit?: number },
    ) => Promise<{ results: SearchHit[]; corpus_size: number }>;
    earningCandidates: <T = unknown>(opts?: { limit?: number }) => Promise<T>;
    reputation: (wallet: string, network?: SwarmNetwork) => Promise<AgentReputation>;
    leaderboard: (limit?: number) => Promise<EigenTrustRecord[]>;
  };

  readonly tasks: {
    listAvailable: <TTask = unknown>(limit?: number) => Promise<TaskListResponse<TTask>>;
    get: <TTask = unknown>(id: string, network?: SwarmNetwork) => Promise<TTask>;
    mine: <TTask = unknown>() => Promise<TTask[]>;
    earnings: <TResponse = unknown>() => Promise<TResponse>;
    onboard: <TResponse = unknown>() => Promise<TResponse>;
    claim: (id: string, sponsor?: string) => Promise<PreparedTransactionResponse>;
    claimAndExecute: (id: string, opts?: ExecuteTaskOptions) => Promise<ExecutedTaskResult>;
    submit: (
      id: string,
      body: SubmitTaskRequest | Record<string, unknown>,
    ) => Promise<SubmitTaskResponse>;
    preflight: <TResponse = PreflightTaskResponse>(
      id: string,
      opts?: PreflightTaskOptions,
    ) => Promise<TResponse>;
    submitAndExecute: (
      id: string,
      contentId?: string,
      opts?: ExecuteTaskOptions,
    ) => Promise<ExecutedTaskResult>;
    buildVerify: (
      id: string,
      opts?: { attemptId?: string },
    ) => Promise<BuildVerifyResponse>;
    verify: (
      id: string,
      opts?: ExecuteTaskOptions & { attemptId?: string },
    ) => Promise<ExecutedTaskResult>;
    verifyAndExecute: (
      id: string,
      opts?: ExecuteTaskOptions & { attemptId?: string },
    ) => Promise<ExecutedTaskResult>;
    buildFinalize: (id: string) => Promise<PreparedTransactionResponse>;
    finalizeAndExecute: (
      id: string,
      opts?: { connection?: Connection; payoutTo?: string },
    ) => Promise<ExecutedTaskResult>;
    buildExpire: (id: string) => Promise<PreparedTransactionResponse>;
    expireAndExecute: (
      id: string,
      opts?: { connection?: Connection; chain?: string },
    ) => Promise<ExecutedTaskResult>;
    nextAction: (task: Record<string, unknown>) => NextTaskAction;
    verifyVow: (
      urlOrJson: string | Record<string, unknown>,
      opts?: {
        rpcUrl?: string;
        connection?: Connection;
        maxBytes?: number;
        allowLocalhostHttp?: boolean;
      },
    ) => Promise<Verdict>;
  };

  readonly proofs: {
    list: (opts?: ListProofsOptions) => Promise<ProofsResponse>;
  };

  readonly campaigns: {
    list: <TCampaign = unknown>() => Promise<CampaignListResponse<TCampaign> | TCampaign[]>;
    get: <TCampaign = unknown>(id: string) => Promise<TCampaign>;
    tasks: <TTask = unknown>(id: string) => Promise<TaskListResponse<TTask>>;
    metrics: <TMetrics = unknown>(id: string) => Promise<TMetrics>;
    pendingApproval: <TTask = unknown>(
      network?: SwarmNetwork,
    ) => Promise<TaskListResponse<TTask> | TTask[]>;
    create: <TResponse = { campaign_id: string }>(body: unknown) => Promise<TResponse>;
    fund: (id: string, body: unknown) => Promise<PreparedTransactionResponse>;
    fundAndExecute: (
      id: string,
      body: unknown,
      opts?: { connection?: Connection; chain?: string },
    ) => Promise<ExecutedTaskResult>;
    approve: (id: string) => Promise<PreparedTransactionResponse>;
    approveAndExecute: (
      id: string,
      opts?: { connection?: Connection; chain?: string },
    ) => Promise<ExecutedTaskResult>;
    expire: (id: string) => Promise<PreparedTransactionResponse>;
    expireAndExecute: (
      id: string,
      opts?: { connection?: Connection; chain?: string },
    ) => Promise<ExecutedTaskResult>;
  };

  readonly inbox: {
    list: (opts?: InboxListOptions) => Promise<InboxListPage>;
    open: (refs: Array<MessageReference | string>) => Promise<{ results: OpenMessageResult[] }>;
    ackIds: (refs: Array<MessageReference | string>) => Promise<{ results: AckMessageResult[] }>;
    messages: (
      threadId?: string,
      opts?: { cursor?: string; limit?: number; includeSent?: boolean },
    ) => Promise<MessagePage>;
    send: (
      toWallet: string,
      body: string,
      opts?: { threadId?: string; intent?: string; deliveryKey?: string },
    ) => Promise<SendReceipt>;
    ack: (upToCursor: string) => Promise<void>;
    mute: (threadId: string, muted?: boolean) => Promise<MuteReceipt>;
  };

  readonly topics: {
    read: (
      topicId: TopicId,
      opts?: { cursor?: string; limit?: number; minTrust?: number },
    ) => Promise<TopicPage>;
    publish: (
      topicId: TopicId,
      body: string,
      opts?: { replyTo?: string; intent?: string; refId?: string },
    ) => Promise<PublishReceipt>;
    report: (topicId: TopicId, postId: string, reason?: string) => Promise<ReportReceipt>;
  };

  readonly games: {
    readonly api: CoordinationGameApiClient;
    availability: <T = unknown>(network?: SwarmNetwork) => Promise<T>;
    leaderboard: <T = unknown>(
      network?: SwarmNetwork,
      opts?: { tournamentId?: number; limit?: number },
    ) => Promise<T>;
  };

  constructor(options: SwarmAgentOptions = {}) {
    this.network = options.network ?? "mainnet";
    this.rpcUrl = (options.rpcUrl ?? DEFAULT_SOLANA_RPC[this.network]).replace(/\/$/, "");
    this.shillbotUrl = (options.shillbotUrl ?? "https://api.shillbot.org").replace(/\/$/, "");
    this.gameUrl = (options.gameUrl ?? "https://api.coordination.game").replace(/\/$/, "");
    this.mcpUrl = (options.mcpUrl ?? "https://mcp.swarm.tips").replace(/\/$/, "");
    this.autoOnboard = options.autoOnboard ?? true;
    this.maxEscrowSpendLamports = options.maxEscrowSpendLamports ?? 0n;
    this.maxEscrowSpendWei = options.maxEscrowSpendWei ?? 0n;
    this.maxNetworkFeeLamports = options.maxNetworkFeeLamports ?? 100_000n;
    this.maxNetworkFeeWei = options.maxNetworkFeeWei ?? DEFAULT_MAX_NETWORK_FEE_WEI;
    this.signer = options.signer;

    const rawFetch = options.fetch ?? globalThis.fetch;
    if (!rawFetch) {
      throw new SwarmClientError({
        code: "INVALID_ARGUMENT",
        operation: "agent.constructor",
        message: "A fetch implementation is required",
      });
    }
    this.fetchImpl = options.fetch
      ? (input, init) => options.fetch!(input, init)
      : rawFetch.bind(globalThis);

    this.tokenManager = new WalletTokenManager({
      signer: options.signer,
      token: options.token,
      gameUrl: this.gameUrl,
      fetch: this.fetchImpl,
    });

    const getToken = () => this.tokenManager.getToken();
    const onUnauthorized = () => this.tokenManager.invalidate();

    this.shillbotClient = new ShillbotApiClient({
      baseUrl: this.shillbotUrl,
      fetch: this.fetchImpl,
      network: this.network,
      getToken,
      onUnauthorized,
    });

    this.swarmClient = new SwarmClient({
      baseUrl: this.mcpUrl,
      fetch: this.fetchImpl,
    });

    this.inboxClient = new InboxClient({
      baseUrl: this.mcpUrl,
      storage: null,
      fetchFn: this.fetchImpl as typeof fetch,
      getToken,
      onUnauthorized,
    });

    this.gameClient = new CoordinationGameApiClient({
      baseUrl: this.gameUrl,
      fetch: this.fetchImpl,
      network: this.network,
      getToken,
      onUnauthorized,
    });

    const self = this;
    this.auth = {
      getToken: () => self.tokenManager.getToken(),
      authenticate: () => self.tokenManager.authenticate(),
      get walletAddress() {
        return self.tokenManager.walletAddress;
      },
    };

    this.discovery = {
      async listings(opts) {
        let items = await self.swarmClient.listings();
        if (opts?.category) {
          items = items.filter((item) => item.category === opts.category);
        }
        if (opts?.chain) {
          items = items.filter((item) => item.reward_chain === opts.chain);
        }
        if (opts?.limit !== undefined) {
          items = items.slice(0, opts.limit);
        }
        return items;
      },
      searchMcp: (query, opts) => self.swarmClient.search(query, opts?.limit ?? 30),
      earningCandidates: (opts) => self.swarmClient.earningCandidates(opts?.limit),
      reputation: (wallet, net) => self.swarmClient.reputation(wallet, net ?? self.network),
      leaderboard: (limit) => self.swarmClient.leaderboard(limit),
    };

    this.tasks = {
      listAvailable: (limit = 50) => self.shillbotClient.listTasks(limit),
      get: (id, net) => self.shillbotClient.getTask(id, net),
      async mine<TTask = unknown>(): Promise<TTask[]> {
        const res = await self.shillbotClient.listAgentTasks<TTask>();
        return Array.isArray(res) ? res : (res.tasks ?? []);
      },
      earnings: () => self.shillbotClient.earnings(),
      onboard: () => self.onboard(),
      claim: (id, sponsor = "auto") => self.shillbotClient.claimTask(id, sponsor),
      claimAndExecute: (id, opts) => self.executeClaimOrSubmit(id, "claim", undefined, opts),
      submit: (id, body) => self.shillbotClient.submitTask(id, body),
      preflight<TResponse = PreflightTaskResponse>(id: string, opts?: PreflightTaskOptions) {
        const body: PreflightTaskRequest = {};
        const proofLean = opts?.proof_lean ?? opts?.proofLean;
        const contentId = opts?.content_id ?? opts?.contentId;
        const agentWallet = opts?.agent_wallet ?? opts?.agentWallet;
        if (proofLean !== undefined) body.proof_lean = proofLean;
        if (opts?.compile !== undefined) body.compile = opts.compile;
        if (contentId !== undefined) body.content_id = contentId;
        if (agentWallet !== undefined) body.agent_wallet = agentWallet;
        const callOpts =
          opts?.network !== undefined || opts?.token !== undefined
            ? {
                ...(opts.network !== undefined ? { network: opts.network } : {}),
                ...(opts.token !== undefined ? { token: opts.token } : {}),
              }
            : undefined;
        return self.shillbotClient.preflightTask<TResponse>(id, body, callOpts);
      },
      submitAndExecute: (id, contentId, opts) =>
        self.executeClaimOrSubmit(id, "submit", contentId, opts),
      buildVerify: (id, opts) =>
        self.shillbotClient.buildVerify(id, {
          payer: self.auth.walletAddress,
          mcpBaseUrl: self.mcpUrl,
          network: self.network,
          ...(opts?.attemptId ? { attemptId: opts.attemptId } : {}),
        }),
      verify: (id, opts) => self.verifyTask(id, opts),
      verifyAndExecute: (id, opts) => self.verifyTask(id, opts),
      buildFinalize: (id) => self.shillbotClient.buildFinalize(id),
      async finalizeAndExecute(id, opts) {
        const prepared = await self.tasks.buildFinalize(id);
        return self.executePreparedAction(id, "finalize", prepared, {
          connection: opts?.connection,
          payoutTo: opts?.payoutTo,
        });
      },
      buildExpire: (id) => self.shillbotClient.buildExpire(id),
      async expireAndExecute(id, opts) {
        const prepared = await self.tasks.buildExpire(id);
        return self.executePreparedAction(id, "expire", prepared, {
          connection: opts?.connection,
          chain: opts?.chain,
        });
      },
      nextAction: (task) => SwarmAgent.adviseNextAction(task),
      async verifyVow(urlOrJson, opts) {
        const customAgentRpc =
          self.rpcUrl !== DEFAULT_SOLANA_RPC[self.network] ? self.rpcUrl : undefined;
        return verifyVow(urlOrJson, {
          fetch: self.fetchImpl,
          network: self.network,
          rpcUrl: opts?.rpcUrl ?? customAgentRpc,
          connection: opts?.connection,
          maxBytes: opts?.maxBytes,
          allowLocalhostHttp: opts?.allowLocalhostHttp,
          operation: "tasks.verifyVow",
        });
      },
    };

    this.proofs = {
      list: (opts) => self.shillbotClient.listProofs(opts),
    };

    this.campaigns = {
      list: () => self.shillbotClient.listCampaigns(),
      get: (id) => self.shillbotClient.campaign(id),
      tasks: (id) => self.shillbotClient.campaignTasks(id),
      metrics: (id) => self.shillbotClient.campaignMetrics(id),
      pendingApproval: (net) => self.shillbotClient.listPendingApproval(net),
      create: (body) => self.shillbotClient.createCampaign(body),
      fund: (id, body) => self.shillbotClient.fundCampaignTask(id, body),
      async fundAndExecute(id, body, opts) {
        // Pre-flight C9 spend check on request body if amount is specified directly
        const requestedLamports = extractBodyBigInt(body, [
          "amount_lamports",
          "escrow_lamports",
          "budget_lamports",
        ]);
        if (requestedLamports !== undefined && requestedLamports > self.maxEscrowSpendLamports) {
          throw new SwarmClientError({
            code: "SPEND_CAP_EXCEEDED",
            operation: "campaigns.fundAndExecute",
            message: `Requested escrow ${requestedLamports} lamports exceeds maxEscrowSpendLamports (${self.maxEscrowSpendLamports})`,
          });
        }
        const requestedWei = extractBodyBigInt(body, [
          "amount_wei",
          "escrow_wei",
          "value_wei",
        ]);
        if (requestedWei !== undefined && requestedWei > self.maxEscrowSpendWei) {
          throw new SwarmClientError({
            code: "SPEND_CAP_EXCEEDED",
            operation: "campaigns.fundAndExecute",
            message: `Requested escrow ${requestedWei} wei exceeds maxEscrowSpendWei (${self.maxEscrowSpendWei})`,
          });
        }

        const prepared = await self.shillbotClient.fundCampaignTask(id, body);
        return self.executePreparedAction(prepared.task_id ?? id, "create", prepared, {
          connection: opts?.connection,
          chain: opts?.chain,
          expectedEscrowLamports: requestedLamports,
        });
      },
      approve: (id) => self.shillbotClient.approveTask(id),
      async approveAndExecute(id, opts) {
        const prepared = await self.shillbotClient.approveTask(id);
        return self.executePreparedAction(id, "approve", prepared, {
          connection: opts?.connection,
          chain: opts?.chain,
        });
      },
      expire: (id) => self.shillbotClient.buildExpire(id),
      async expireAndExecute(id, opts) {
        const prepared = await self.shillbotClient.buildExpire(id);
        return self.executePreparedAction(id, "expire", prepared, {
          connection: opts?.connection,
          chain: opts?.chain,
        });
      },
    };

    this.inbox = {
      list: (opts) => self.inboxClient.listMessages(opts),
      open: (refs) =>
        self.inboxClient.openMessages(
          refs.map((ref) => (typeof ref === "string" ? { msg_id: ref } : ref)),
        ),
      ackIds: (refs) =>
        self.inboxClient.ackMessageIds(
          refs.map((ref) => (typeof ref === "string" ? ref : ref.msg_id)),
        ),
      messages: (threadId, opts) =>
        threadId ? self.inboxClient.getThread(threadId, opts) : self.inboxClient.getMessages(opts),
      send: (toWallet, body, opts) =>
        self.inboxClient.send(toWallet, body, opts?.threadId, opts?.intent, opts?.deliveryKey),
      ack: (upToCursor) => self.inboxClient.ack(upToCursor),
      mute: (threadId, muted = true) => self.inboxClient.muteThread(threadId, muted),
    };

    this.topics = {
      read: (topicId, opts) => self.inboxClient.readTopic(topicId, opts),
      publish: (topicId, body, opts) => self.inboxClient.publishPost(topicId, body, opts),
      report: (topicId, postId, reason) => self.inboxClient.reportPost(topicId, postId, reason),
    };

    this.games = {
      api: this.gameClient,
      availability: <T = unknown>(net?: SwarmNetwork) => {
        const targetNet = net ?? self.network;
        const suffix = targetNet === "devnet" ? "?network=devnet" : "";
        return self.gameClient.http.get<T>(`/availability${suffix}`);
      },
      leaderboard: <T = unknown>(
        net?: SwarmNetwork,
        opts?: { tournamentId?: number; limit?: number },
      ) => {
        const targetNet = net ?? self.network;
        const tournamentId = opts?.tournamentId ?? ACTIVE_TOURNAMENT_ID[targetNet];
        const limit = opts?.limit ?? 20;
        return self.gameClient.http.get<T>(
          `/leaderboard?tournament_id=${encodeURIComponent(String(tournamentId))}&limit=${encodeURIComponent(String(limit))}`,
        );
      },
    };
  }

  /** Bootstrap a zero-funds Solana agent wallet via recoupable Advance PDA fronting (`2_600_000` lamports). */
  onboard<TResponse = unknown>(): Promise<TResponse> {
    return this.shillbotClient.onboard<TResponse>();
  }

  /** Claim a task and execute the prepared transaction (with automatic `$0` wallet onboarding when enabled). */
  claimTask(id: string, opts?: ExecuteTaskOptions): Promise<ExecutedTaskResult> {
    return this.executeClaimOrSubmit(id, "claim", undefined, opts);
  }

  /** Submit work for a task and execute the prepared transaction (or return immediately on M2 202 Accepted open competition). */
  submitWork(
    id: string,
    contentId?: string,
    opts?: ExecuteTaskOptions,
  ): Promise<ExecutedTaskResult> {
    return this.executeClaimOrSubmit(id, "submit", contentId, opts);
  }

  /** Build, locally validate, sign, broadcast, and confirm a task verification transaction (including M2 1-tx Open->Verified bundles). */
  async verifyTask(
    id: string,
    opts?: ExecuteTaskOptions & { attemptId?: string },
  ): Promise<ExecutedTaskResult> {
    const prepared = await this.tasks.buildVerify(
      id,
      opts?.attemptId ? { attemptId: opts.attemptId } : undefined,
    );
    return this.executePreparedAction(id, "verify", prepared, {
      connection: opts?.connection,
      feePayer: opts?.feePayer,
      payoutTo: opts?.payoutTo,
    });
  }

  private requireSigner(): SwarmSigner {
    if (!this.signer) {
      throw new SwarmClientError({
        code: "AUTH_REQUIRED",
        operation: "agent.signer",
        message: "A non-custodial SwarmSigner is required to sign and execute transactions",
      });
    }
    return this.signer;
  }

  private async executeClaimOrSubmit(
    id: string,
    action: "claim" | "submit",
    contentId: string | undefined,
    opts?: ExecuteTaskOptions,
  ): Promise<ExecutedTaskResult> {
    const sponsorMode = opts?.sponsor ?? "auto";
    const sponsorQuery =
      sponsorMode === "none" ? "false" : sponsorMode === "true" ? "true" : "auto";
    const shouldAutoOnboard = (opts?.autoOnboard ?? this.autoOnboard) !== false;

    const requestPrepared = (query: string): Promise<PreparedTransactionResponse> => {
      if (action === "claim") {
        return this.shillbotClient.claimTask(id, query);
      }
      const suffix = sponsorMode !== "none" ? `?sponsor=${encodeURIComponent(query)}` : "";
      const payload: Record<string, unknown> = {
        ...(opts?.extra ?? {}),
      };
      if (opts?.proofLean !== undefined && payload.proof_lean === undefined) {
        payload.proof_lean = opts.proofLean;
      }
      if (contentId !== undefined && contentId !== "") {
        payload.content_id = contentId;
      } else if (payload.content_id === undefined && payload.proof_lean === undefined) {
        payload.content_id = contentId ?? "";
      }
      if (opts?.agentHarness && payload.agent_harness === undefined) {
        payload.agent_harness = opts.agentHarness;
      }
      if (opts?.modelFamily && payload.model_family === undefined) {
        payload.model_family = opts.modelFamily;
      }
      return this.shillbotClient.post<PreparedTransactionResponse>(
        `/tasks/${encodeURIComponent(id)}/submit${suffix}`,
        payload,
      );
    };

    let prepared: PreparedTransactionResponse;
    try {
      prepared = await requestPrepared(sponsorQuery);
    } catch (err) {
      if (
        (action === "claim" || action === "submit") &&
        sponsorMode !== "none" &&
        shouldAutoOnboard &&
        !this.autoOnboardAttempted &&
        err instanceof SwarmClientError &&
        err.code === "HTTP_ERROR" &&
        (err.status === 400 || err.status === 403 || err.status === 412) &&
        /onboard|agent_state|bootstrap/i.test(err.message)
      ) {
        this.autoOnboardAttempted = true;
        await this.onboard();
        prepared = await requestPrepared(sponsorQuery);
      } else {
        throw err;
      }
    }

    // M2 Open-Competition off-chain submission (`202 Accepted`, $0 gas):
    // return immediately without calling RPC, signing, broadcasting, or auto-onboarding prematurely.
    if (
      action === "submit" &&
      (prepared.open_competition === true || (!prepared.transaction && Boolean(prepared.attempt_id)))
    ) {
      return {
        task_id: prepared.task_id ?? id,
        signature: "",
        ...(prepared.task_pda ? { task_pda: prepared.task_pda } : {}),
        attempt_id: prepared.attempt_id,
        open_competition: true,
        status: prepared.status ?? "verifying",
      };
    }

    const connection =
      this.signer?.chain === "solana"
        ? (opts?.connection ?? new Connection(this.rpcUrl, "confirmed"))
        : opts?.connection;

    // When a Solana wallet holds less than MIN_SELF_PAID_SOLANA_BALANCE_LAMPORTS (2_550_000 lamports
    // for claim / atomic claim+submit) or MIN_POST_CLAIM_SUBMIT_SOLANA_BALANCE_LAMPORTS (50_000 lamports
    // for standalone submit) and receives a self-paid HTTP 200 transaction, onboard once and re-request.
    if (
      prepared.transaction &&
      sponsorMode !== "none" &&
      shouldAutoOnboard &&
      !this.autoOnboardAttempted &&
      connection &&
      this.signer?.chain === "solana" &&
      typeof connection.getBalance === "function"
    ) {
      let initialFeePayer: string | undefined;
      let bundlesClaim = action === "claim";
      try {
        const initialInspected = inspectTransaction(prepared.transaction);
        initialFeePayer = initialInspected.fee_payer;
        if (!bundlesClaim) {
          const claimDisc = shillbotInstructionDiscriminator("claim_task");
          const claimSessionDisc = shillbotInstructionDiscriminator("claim_task_session");
          bundlesClaim = initialInspected.instructions.some((ix) => {
            if (ix.program_id !== SHILLBOT_PROGRAM_ID_B58) return false;
            const d = Buffer.from(ix.data_base64, "base64");
            return (
              d.length >= 8 &&
              (d.subarray(0, 8).equals(claimDisc) || d.subarray(0, 8).equals(claimSessionDisc))
            );
          });
        }
      } catch {
        // Malformed transaction will fail closed during validation below.
      }
      if (initialFeePayer === this.signer.address) {
        const balanceLamports = await connection.getBalance(new PublicKey(this.signer.address));
        const minRequiredLamports = bundlesClaim
          ? MIN_SELF_PAID_SOLANA_BALANCE_LAMPORTS
          : MIN_POST_CLAIM_SUBMIT_SOLANA_BALANCE_LAMPORTS;
        if (balanceLamports < minRequiredLamports) {
          this.autoOnboardAttempted = true;
          await this.onboard();
          prepared = await requestPrepared("auto");
        }
      }
    }

    let resolvedPayoutTo =
      connection && this.signer?.chain === "solana"
        ? await SwarmAgent.resolveVerifiedAdvancePayoutTo(
            prepared,
            this.signer.address,
            connection,
            opts?.payoutTo,
          )
        : opts?.payoutTo;
    let payoutToResolved = Boolean(connection && this.signer?.chain === "solana");

    // If Solana transaction is a gasless sponsor template (fee_payer !== wallet & unsigned by sponsor),
    // exchange it via POST /tasks/{id}/sponsor before signing locally.
    if (
      prepared.transaction &&
      sponsorMode !== "none" &&
      this.signer?.chain === "solana"
    ) {
      const inspected = validatePreparedTransaction(prepared.transaction, {
        action,
        network: this.network,
        wallet: this.signer.address,
        taskPda: prepared.task_pda,
        ...(opts?.feePayer !== undefined ? { feePayer: opts.feePayer } : {}),
        ...(resolvedPayoutTo !== undefined ? { payoutTo: resolvedPayoutTo } : {}),
      });
      if (
        inspected.fee_payer !== this.signer.address &&
        !isFeePayerAlreadySigned(prepared.transaction)
      ) {
        const prevPayoutTo = prepared.payout_to;
        const sponsored = await this.shillbotClient.sponsorTask(id, {
          action,
          unsigned_transaction: prepared.transaction,
        });
        prepared = {
          ...prepared,
          ...sponsored,
          task_pda: sponsored.task_pda ?? prepared.task_pda,
        };
        if (opts?.payoutTo === undefined && prepared.payout_to !== prevPayoutTo) {
          resolvedPayoutTo = undefined;
          payoutToResolved = false;
        }
      }
    }

    return this.executePreparedAction(id, action, prepared, {
      connection,
      feePayer: opts?.feePayer,
      payoutTo: resolvedPayoutTo,
      payoutToResolved,
    });
  }

  /**
   * Resolve an authorized `payoutTo` pubkey (`P1-2` / `HIGH-02`):
   * - If caller explicitly passed `explicitPayoutTo`, return it verbatim.
   * - Otherwise, NEVER trust `advance_backer` or `required_payout_to` from the API response alone:
   *   1. When `advance_backer` is provided, verify `advancePda(advance_backer, walletAddress) === candidatePayoutTo`.
   *   2. Fetch the PDA on-chain via `connection.getAccountInfo(candidatePk, "confirmed")` and require:
   *      - account exists (`acct !== null && acct.lamports > 0`),
   *      - `acct.owner.equals(EXTENSION_CREDIT_PROGRAM_ID)`,
   *      - valid 113-byte `Advance` account (`sha256("account:Advance")[0..8]`) with
   *        `recipient === walletAddress`, canonical `advancePda(decoded.backer, walletAddress) === candidatePk`,
   *        `backer === advance_backer` (when provided), and `recouped_lamports < advance_lamports`.
   *   3. On any mismatch or RPC error, return `undefined` so `validatePreparedTransaction`
   *      rejects any `set_payout_to` fail-closed.
   */
  private static async resolveVerifiedAdvancePayoutTo(
    prepared: PreparedTransactionResponse,
    walletAddress: string | undefined,
    connection: Connection,
    explicitPayoutTo?: string,
  ): Promise<string | undefined> {
    if (explicitPayoutTo !== undefined) {
      return explicitPayoutTo;
    }
    const candidatePayoutTo = prepared.payout_to ?? prepared.required_payout_to ?? undefined;
    if (!walletAddress || !candidatePayoutTo) {
      return undefined;
    }
    try {
      const expectedRecipient = new PublicKey(walletAddress);
      let expectedBacker: PublicKey | undefined;
      let candidatePk: PublicKey;
      if (prepared.advance_backer) {
        expectedBacker = new PublicKey(prepared.advance_backer);
        const derived = advancePda(expectedBacker, expectedRecipient);
        if (derived.toBase58() !== candidatePayoutTo) {
          return undefined;
        }
        candidatePk = derived;
      } else {
        candidatePk = new PublicKey(candidatePayoutTo);
      }
      const acct = await connection.getAccountInfo(candidatePk, "confirmed");
      if (!acct || acct.lamports <= 0) {
        return undefined;
      }
      if (!acct.owner.equals(EXTENSION_CREDIT_PROGRAM_ID)) {
        return undefined;
      }
      const decoded = decodeExtensionCreditAdvance(acct.data);
      if (!decoded) {
        return undefined;
      }
      if (expectedBacker && !decoded.backer.equals(expectedBacker)) {
        return undefined;
      }
      if (
        !decoded.recipient.equals(expectedRecipient) ||
        !advancePda(decoded.backer, expectedRecipient).equals(candidatePk) ||
        decoded.advanceLamports === 0n ||
        decoded.recoupedLamports >= decoded.advanceLamports
      ) {
        return undefined;
      }
      return candidatePk.toBase58();
    } catch {
      return undefined;
    }
  }

  private async executePreparedAction(
    id: string,
    action: Action,
    prepared: PreparedTransactionResponse,
    opts: {
      connection?: Connection;
      chain?: string;
      expectedEscrowLamports?: bigint;
      feePayer?: string;
      payoutTo?: string;
      payoutToResolved?: boolean;
    },
  ): Promise<ExecutedTaskResult> {
    const signer = this.requireSigner();

    if (prepared.evm_call && signer.chain === "evm") {
      return this.executeEvmAction(id, action, prepared, signer, opts.chain);
    }
    if (signer.chain !== "solana") {
      throw new SwarmClientError({
        code: "INVALID_ARGUMENT",
        operation: `tasks.${action}AndExecute`,
        message: "Prepared transaction targets Solana, but configured signer is EVM",
      });
    }
    return this.executeSolanaAction(id, action, prepared, signer, opts);
  }

  private async executeEvmAction(
    id: string,
    action: Action,
    prepared: PreparedTransactionResponse,
    signer: EvmSwarmSigner,
    chain?: string,
  ): Promise<ExecutedTaskResult> {
    if (!signer.sendTransaction) {
      throw new SwarmClientError({
        code: "INVALID_ARGUMENT",
        operation: `tasks.${action}AndExecute`,
        message: "Configured EVM signer does not implement sendTransaction",
      });
    }
    // Check C9 spend cap first so spend cap errors use SPEND_CAP_EXCEEDED
    const rawVal = (prepared.evm_call as { value_wei?: unknown; value?: unknown })?.value_wei ??
      (prepared.evm_call as { value?: unknown })?.value ??
      "0";
    const callValueWei = BigInt(String(rawVal));
    if (action === "create" && callValueWei > this.maxEscrowSpendWei) {
      throw new SwarmClientError({
        code: "SPEND_CAP_EXCEEDED",
        operation: "campaigns.fundAndExecute",
        message: `EVM escrow value ${callValueWei} wei exceeds maxEscrowSpendWei (${this.maxEscrowSpendWei})`,
      });
    }
    const validated = validatePreparedEvmCall(prepared.evm_call, {
      action,
      chain,
      maxValueWei: action === "create" ? this.maxEscrowSpendWei : 0n,
    });
    if (signer.chainId !== undefined) {
      const normalizedSignerChain = normalizeEvmChainId(signer.chainId);
      if (normalizedSignerChain !== validated.chain) {
        throw new SwarmClientError({
          code: "TRANSACTION_MISMATCH",
          operation: `tasks.${action}AndExecute`,
          message: `EVM signer chain ${normalizedSignerChain} differs from validated call chain ${validated.chain}`,
        });
      }
    }
    const caipMatch = /^eip155:(\d+)$/.exec(validated.chain);
    const numericChainId = caipMatch ? Number(caipMatch[1]) : undefined;
    const txHash = await signer.sendTransaction({
      to: validated.to,
      data: validated.data,
      value: validated.value,
      chain: validated.chain,
      ...(numericChainId !== undefined && Number.isSafeInteger(numericChainId)
        ? { chainId: numericChainId }
        : {}),
      maxFeePerGas: this.maxNetworkFeeWei,
    });
    const confirmation = await this.shillbotClient.confirmTask(id, {
      tx_signature: txHash,
      action,
      task_pda: prepared.task_pda,
    });
    return {
      task_id: id,
      signature: txHash,
      task_pda: prepared.task_pda,
      confirmation,
    };
  }

  private async executeSolanaAction(
    id: string,
    action: Action,
    prepared: PreparedTransactionResponse,
    signer: SolanaSwarmSigner,
    opts: {
      connection?: Connection;
      expectedEscrowLamports?: bigint;
      feePayer?: string;
      payoutTo?: string;
      payoutToResolved?: boolean;
    },
  ): Promise<ExecutedTaskResult> {
    const encodedTx = prepared.transaction || prepared.co_signed_transaction || "";
    if (!encodedTx) {
      throw new SwarmClientError({
        code: "INVALID_RESPONSE",
        operation: `tasks.${action}AndExecute`,
        message: `Server response for ${action} omitted base64 transaction`,
      });
    }

    if (action === "create") {
      const txEscrow = extractCreateEscrowLamports(encodedTx);
      if (txEscrow !== undefined && txEscrow > this.maxEscrowSpendLamports) {
        throw new SwarmClientError({
          code: "SPEND_CAP_EXCEEDED",
          operation: "campaigns.fundAndExecute",
          message: `Prepared create_task escrow ${txEscrow} lamports exceeds maxEscrowSpendLamports (${this.maxEscrowSpendLamports})`,
        });
      }
    }

    // C9 Network Fee Cap check first so fee cap breaches report SPEND_CAP_EXCEEDED
    let preInspected;
    try {
      preInspected = inspectTransaction(encodedTx);
    } catch (err) {
      throw new SwarmClientError({
        code: "TRANSACTION_MISMATCH",
        operation: `tasks.${action}AndExecute`,
        message: err instanceof Error ? err.message : `Malformed transaction: ${String(err)}`,
      });
    }
    const estimatedFeeLamports = estimateSolanaNetworkFeeLamports(preInspected, signer.address);
    if (estimatedFeeLamports > this.maxNetworkFeeLamports) {
      throw new SwarmClientError({
        code: "SPEND_CAP_EXCEEDED",
        operation: `tasks.${action}AndExecute`,
        message: `Estimated Solana network fee ${estimatedFeeLamports} lamports exceeds maxNetworkFeeLamports (${this.maxNetworkFeeLamports})`,
      });
    }

    const connection = opts.connection ?? new Connection(this.rpcUrl, "confirmed");
    const resolvedPayoutTo = opts.payoutToResolved
      ? opts.payoutTo
      : await SwarmAgent.resolveVerifiedAdvancePayoutTo(
          prepared,
          signer.address,
          connection,
          opts.payoutTo,
        );
    const expected = {
      action,
      network: this.network,
      wallet: signer.address,
      taskPda: prepared.task_pda,
      ...(opts.feePayer !== undefined ? { feePayer: opts.feePayer } : {}),
      ...(resolvedPayoutTo !== undefined ? { payoutTo: resolvedPayoutTo } : {}),
      ...(action === "create" && opts.expectedEscrowLamports !== undefined
        ? { escrowLamports: opts.expectedEscrowLamports }
        : {}),
    };
    validatePreparedTransaction(encodedTx, expected);

    const signature = await signBroadcastPreparedTransaction({
      connection,
      transaction: encodedTx,
      expected,
      sign: (tx) => signer.signTransaction(tx),
    });

    const confirmation = await this.shillbotClient.confirmTask(id, {
      tx_signature: signature,
      action,
      task_pda: prepared.task_pda,
    });

    return {
      task_id: id,
      signature,
      task_pda: prepared.task_pda,
      confirmation,
      ...(prepared.attempt_id !== undefined ? { attempt_id: prepared.attempt_id } : {}),
      ...(prepared.onboarded_advance_tx !== undefined
        ? { onboarded_advance_tx: prepared.onboarded_advance_tx }
        : {}),
    };
  }

  static adviseNextAction(task: Record<string, unknown>, nowMs = Date.now()): NextTaskAction {
    const rawState = String(task.state ?? task.status ?? "").trim().toLowerCase();
    const rawPlatform = task.platform;
    const normalizedPlatform =
      typeof rawPlatform === "string" ? rawPlatform.trim().toLowerCase() : undefined;
    const isOpenCompetition =
      rawPlatform === 10 ||
      rawPlatform === 11 ||
      normalizedPlatform === "10" ||
      normalizedPlatform === "11" ||
      normalizedPlatform === "lean_proof" ||
      normalizedPlatform === "onchain_action";
    if (rawState === "open") {
      if (isOpenCompetition) {
        const latestAttempt =
          task.my_latest_attempt && typeof task.my_latest_attempt === "object"
            ? (task.my_latest_attempt as Record<string, unknown>)
            : undefined;
        const attemptStatus =
          typeof latestAttempt?.status === "string"
            ? latestAttempt.status.trim().toLowerCase()
            : "";
        if (attemptStatus === "passed") {
          return { step: "verify" };
        }
        if (attemptStatus === "verifying") {
          return { step: "wait_attestation" };
        }
        return { step: "submit" };
      }
      return { step: "claim" };
    }
    if (rawState === "claimed") {
      return { step: "submit" };
    }
    if (rawState === "submitted" || rawState === "pending_approval" || rawState === "approved") {
      if (
        rawState === "pending_approval" ||
        (rawState === "submitted" && task.requires_approval === true && !task.approved_at)
      ) {
        return { step: "wait_attestation" };
      }
      const attestationReadyMs =
        parseTimestampMs(task.attestation_ready_at) ??
        parseTimestampMs(task.verify_not_before) ??
        parseTimestampMs(task.not_before);
      if (attestationReadyMs !== undefined && attestationReadyMs > nowMs) {
        return {
          step: "wait_attestation",
          not_before: new Date(attestationReadyMs).toISOString(),
        };
      }
      if (
        task.composite_score === null &&
        task.verification_hash === undefined &&
        attestationReadyMs === undefined &&
        task.awaiting_attestation === true
      ) {
        return { step: "wait_attestation" };
      }
      return { step: "verify" };
    }
    if (rawState === "verified") {
      const challengeDeadlineMs =
        parseTimestampMs(task.challenge_deadline) ??
        parseTimestampMs(task.challengeDeadline) ??
        parseTimestampMs(task.finalize_not_before);
      if (challengeDeadlineMs !== undefined && challengeDeadlineMs > nowMs) {
        return {
          step: "wait_challenge",
          not_before: new Date(challengeDeadlineMs).toISOString(),
        };
      }
      return { step: "finalize" };
    }
    return { step: "done" };
  }
}

export function createSwarmAgent(options: SwarmAgentOptions = {}): SwarmAgent {
  return new SwarmAgent(options);
}
