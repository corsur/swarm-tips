import { createHash } from "node:crypto";
import {
  BN,
  SHILLBOT_PROGRAM_ID,
  agentStatePda,
  buildShillbotInstruction,
  clientStatePda,
  globalStatePda,
  shillbotInstructionDiscriminator,
  taskPda,
} from "@swarm-tips/contracts";
import bs58 from "bs58";
import {
  Connection,
  PublicKey,
  Transaction,
  TransactionInstruction,
  TransactionMessage,
  VersionedTransaction,
  type BlockhashWithExpiryBlockHeight,
} from "@solana/web3.js";

export {
  COORDINATION_GAME_IDL,
  COORDINATION_GAME_PROGRAM_ID,
  SHILLBOT_IDL,
  SHILLBOT_PROGRAM_ID,
  agentStatePda,
  buildShillbotInstruction,
  clientStatePda,
  globalStatePda,
  shillbotInstructionDiscriminator,
  taskPda,
} from "@swarm-tips/contracts";
export type {
  BuildShillbotInstructionRequest,
  CoordinationGame,
  Shillbot,
} from "@swarm-tips/contracts";

export const SCHEMA_VERSION = "swarm.shillbot.transaction-intent/v1" as const;
export type Action =
  | "create"
  | "claim"
  | "submit"
  | "approve"
  | "verify"
  | "finalize";
export type Risk = "earn" | "spend" | "escrow_control";
export type Network = "mainnet" | "devnet";
export type TransactionVersion = "legacy" | "v0";

export interface TransactionIntent {
  version: typeof SCHEMA_VERSION;
  action: Action;
  network: Network;
  wallet: string;
  task_pda: string;
  program_id: string;
  fee_payer: string;
  accounts: string[];
  signers: string[];
  movements: Array<{
    asset: string;
    from: string;
    to: string;
    amount?: string;
    condition: string;
  }>;
  risk: Risk;
  arguments: Record<string, string | number | boolean>;
  digest: string;
}

export interface BuiltTransaction {
  unsigned_tx: string;
  transaction_intent: TransactionIntent;
}

export interface CommonBuild {
  action: Action;
  wallet: string;
  network: Network;
  recentBlockhash: string;
  version?: TransactionVersion;
}

export interface CreateBuild extends CommonBuild {
  action: "create";
  nonce: string;
  escrowLamports: string;
  contentHash: string;
  deadline: string;
  submitMargin: string;
  claimBuffer: string;
  platform: number;
  attestationDelayOverride: number;
  challengeWindowOverride: number;
  verificationTimeoutOverride: number;
  requiresApproval: boolean;
  verificationKind: number;
}

export interface ClaimBuild extends CommonBuild {
  action: "claim";
  taskPda: string;
  sponsor?: string;
  /** Authorized repayment destination when sponsorship has an open advance. */
  payoutTo?: string;
}

export interface ApproveBuild extends CommonBuild {
  action: "approve";
  taskPda: string;
}

export interface SubmitBuild extends CommonBuild {
  action: "submit";
  taskPda: string;
  contentId: string;
  sponsor?: string;
}

export interface VerifyBuild extends CommonBuild {
  action: "verify";
  taskPda: string;
  switchboardFeed: string;
  compositeScore: string;
  verificationHash: string;
  /** The exact Switchboard crank instructions obtained from its gateway SDK. */
  crankInstructions: TransactionInstruction[];
}

export interface FinalizeBuild extends CommonBuild {
  action: "finalize";
  taskPda: string;
  agent: string;
  client: string;
  treasury: string;
}

export type BuildRequest =
  | CreateBuild
  | ClaimBuild
  | ApproveBuild
  | SubmitBuild
  | VerifyBuild
  | FinalizeBuild;

export function buildInstruction(
  request: BuildRequest
): TransactionInstruction {
  const wallet = new PublicKey(request.wallet);
  if (request.action === "create") {
    const nonce = BigInt(request.nonce);
    return buildShillbotInstruction({
      name: "create_task",
      accounts: {
        global_state: globalStatePda(),
        task: taskPda(nonce, wallet),
        client_state: clientStatePda(wallet),
        client: wallet,
      },
      args: {
        nonce: new BN(request.nonce),
        escrow_lamports: new BN(request.escrowLamports),
        content_hash: Array.from(Buffer.from(request.contentHash, "hex")),
        deadline: new BN(request.deadline),
        submit_margin: new BN(request.submitMargin),
        claim_buffer: new BN(request.claimBuffer),
        platform: request.platform,
        attestation_delay_override: request.attestationDelayOverride,
        challenge_window_override: request.challengeWindowOverride,
        verification_timeout_override: request.verificationTimeoutOverride,
        requires_approval: request.requiresApproval,
        verification_kind: request.verificationKind,
      },
    });
  }

  const task = new PublicKey(request.taskPda);
  if (request.action === "claim") {
    return buildShillbotInstruction({
      name: "claim_task",
      accounts: {
        task,
        global_state: globalStatePda(),
        agent_state: agentStatePda(wallet),
        agent: wallet,
      },
    });
  }
  if (request.action === "submit") {
    return buildShillbotInstruction({
      name: "submit_work",
      accounts: {
        task,
        global_state: globalStatePda(),
        agent_state: agentStatePda(wallet),
        agent: wallet,
      },
      args: { content_id: Buffer.from(request.contentId, "utf8") },
    });
  }
  if (request.action === "approve") {
    return buildShillbotInstruction({
      name: "approve_task",
      accounts: { task, client: wallet },
    });
  }
  if (request.action === "verify") {
    return buildShillbotInstruction({
      name: "verify_task",
      accounts: {
        task,
        global_state: globalStatePda(),
        switchboard_feed: request.switchboardFeed,
      },
      args: {
        composite_score: new BN(request.compositeScore),
        verification_hash: Array.from(
          Buffer.from(request.verificationHash, "hex")
        ),
      },
    });
  }
  const finalize = request as FinalizeBuild;
  return buildShillbotInstruction({
    name: "finalize_task",
    accounts: {
      task,
      global_state: globalStatePda(),
      agent: finalize.agent,
      client: finalize.client,
      treasury: finalize.treasury,
    },
  });
}

function payoutInstruction(
  request: ClaimBuild
): TransactionInstruction | undefined {
  if (!request.payoutTo) return undefined;
  return buildShillbotInstruction({
    name: "set_payout_to",
    accounts: { task: request.taskPda, agent: request.wallet },
    args: { payout_to: new PublicKey(request.payoutTo) },
  });
}

function stable(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(stable);
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value as Record<string, unknown>)
        .filter(([, child]) => child !== undefined)
        .sort(([a], [b]) => a.localeCompare(b))
        .map(([key, child]) => [key, stable(child)])
    );
  }
  return value;
}

export function intentDigest(
  intent: Omit<TransactionIntent, "digest">
): string {
  return createHash("sha256")
    .update(JSON.stringify(stable(intent)))
    .digest("hex");
}

function actionRisk(action: Action): Risk {
  if (action === "create") return "spend";
  if (action === "approve") return "escrow_control";
  return "earn";
}

function serializeUnsigned(
  payer: PublicKey,
  blockhash: string,
  instructions: TransactionInstruction[],
  version: TransactionVersion
): string {
  if (version === "v0") {
    const message = new TransactionMessage({
      payerKey: payer,
      recentBlockhash: blockhash,
      instructions,
    }).compileToV0Message();
    return Buffer.from(new VersionedTransaction(message).serialize()).toString(
      "base64"
    );
  }
  const tx = new Transaction({
    feePayer: payer,
    recentBlockhash: blockhash,
  }).add(...instructions);
  return tx
    .serialize({ requireAllSignatures: false, verifySignatures: false })
    .toString("base64");
}

export function buildTransaction(request: BuildRequest): BuiltTransaction {
  const lifecycle = buildInstruction(request);
  const sponsor =
    request.action === "claim" || request.action === "submit"
      ? request.sponsor
      : undefined;
  const payer = new PublicKey(sponsor ?? request.wallet);
  const companions =
    request.action === "verify"
      ? request.crankInstructions
      : request.action === "claim"
      ? [payoutInstruction(request)].filter(
          (ix): ix is TransactionInstruction => ix !== undefined
        )
      : [];
  // set_payout_to authorizes against task.agent, which claim_task establishes.
  // Keep the two operations atomic, but execute the claim first.
  const instructions =
    request.action === "claim"
      ? [lifecycle, ...companions]
      : [...companions, lifecycle];
  const unsignedTx = serializeUnsigned(
    payer,
    request.recentBlockhash,
    instructions,
    request.version ?? (request.action === "verify" ? "v0" : "legacy")
  );
  const task =
    lifecycle.keys[request.action === "create" ? 1 : 0].pubkey.toBase58();
  const accounts = lifecycle.keys.map((key) => key.pubkey.toBase58());
  const signers = [payer.toBase58()];
  if (payer.toBase58() !== request.wallet) signers.push(request.wallet);
  const movements =
    request.action === "create"
      ? [
          {
            asset: "SOL",
            from: request.wallet,
            to: task,
            amount: request.escrowLamports,
            condition: "escrow deposit",
          },
        ]
      : request.action === "approve"
      ? [
          {
            asset: "escrow control",
            from: task,
            to: task,
            condition: "authorizes verification; no immediate transfer",
          },
        ]
      : [];
  const actionArguments = Object.fromEntries(
    Object.entries(request).filter(
      ([key, value]) =>
        ![
          "action",
          "wallet",
          "network",
          "recentBlockhash",
          "version",
          "sponsor",
          "crankInstructions",
        ].includes(key) &&
        ["string", "number", "boolean"].includes(typeof value)
    )
  ) as Record<string, string | number | boolean>;
  const withoutDigest: Omit<TransactionIntent, "digest"> = {
    version: SCHEMA_VERSION,
    action: request.action,
    network: request.network,
    wallet: request.wallet,
    task_pda: task,
    program_id: SHILLBOT_PROGRAM_ID.toBase58(),
    fee_payer: payer.toBase58(),
    accounts,
    signers,
    movements,
    risk: actionRisk(request.action),
    arguments: actionArguments,
  };
  return {
    unsigned_tx: unsignedTx,
    transaction_intent: {
      ...withoutDigest,
      digest: intentDigest(withoutDigest),
    },
  };
}

export interface TransactionInspection {
  version: TransactionVersion;
  fee_payer: string;
  signers: string[];
  instructions: Array<{
    program_id: string;
    accounts: string[];
    data_base64: string;
  }>;
}

export function inspectTransaction(encoded: string): TransactionInspection {
  const bytes = Buffer.from(encoded, "base64");
  try {
    const tx = VersionedTransaction.deserialize(bytes);
    const keys = tx.message.staticAccountKeys;
    return {
      version: tx.version === "legacy" ? "legacy" : "v0",
      fee_payer: keys[0].toBase58(),
      signers: keys
        .slice(0, tx.message.header.numRequiredSignatures)
        .map((key) => key.toBase58()),
      instructions: tx.message.compiledInstructions.map((ix) => ({
        program_id: keys[ix.programIdIndex].toBase58(),
        accounts: ix.accountKeyIndexes.map((index) => keys[index].toBase58()),
        data_base64: Buffer.from(ix.data).toString("base64"),
      })),
    };
  } catch {
    const tx = Transaction.from(bytes);
    const message = tx.compileMessage();
    return {
      version: "legacy",
      fee_payer: message.accountKeys[0].toBase58(),
      signers: message.accountKeys
        .slice(0, message.header.numRequiredSignatures)
        .map((key) => key.toBase58()),
      instructions: message.instructions.map((ix) => ({
        program_id: message.accountKeys[ix.programIdIndex].toBase58(),
        accounts: ix.accounts.map((index) =>
          message.accountKeys[index].toBase58()
        ),
        data_base64: Buffer.from(bs58.decode(ix.data)).toString("base64"),
      })),
    };
  }
}

export function verifyIntent(
  encoded: string,
  intent: TransactionIntent
): TransactionInspection {
  const { digest: _digest, ...rest } = intent;
  const expectedDigest = intentDigest(rest);
  if (expectedDigest !== intent.digest)
    throw new Error("intent digest mismatch");
  const inspected = inspectTransaction(encoded);
  if (inspected.fee_payer !== intent.fee_payer)
    throw new Error("fee payer differs from intent");
  if (!inspected.signers.includes(intent.wallet))
    throw new Error("wallet is not a required signer");
  const expectedDiscriminator = shillbotInstructionDiscriminator(
    intent.action === "submit" ? "submit_work" : `${intent.action}_task`
  );
  const lifecycle = inspected.instructions.filter(
    (ix) =>
      ix.program_id === SHILLBOT_PROGRAM_ID.toBase58() &&
      Buffer.from(ix.data_base64, "base64")
        .subarray(0, 8)
        .equals(expectedDiscriminator)
  );
  if (lifecycle.length !== 1)
    throw new Error("expected exactly one Shillbot lifecycle instruction");
  const shillbotInstructions = inspected.instructions.filter(
    (ix) => ix.program_id === SHILLBOT_PROGRAM_ID.toBase58()
  );
  const payoutDiscriminator = shillbotInstructionDiscriminator("set_payout_to");
  const companions = shillbotInstructions.filter((ix) => ix !== lifecycle[0]);
  const lifecycleIndex = inspected.instructions.indexOf(lifecycle[0]);
  if (
    intent.action === "claim" &&
    companions.some(
      (ix) =>
        Buffer.from(ix.data_base64, "base64")
          .subarray(0, 8)
          .equals(payoutDiscriminator) &&
        inspected.instructions.indexOf(ix) < lifecycleIndex
    )
  ) {
    throw new Error("payout route must follow claim");
  }
  if (
    companions.length >
      (intent.action === "claim" && intent.arguments.payoutTo ? 1 : 0) ||
    companions.some(
      (ix) =>
        intent.action !== "claim" ||
        !Buffer.from(ix.data_base64, "base64")
          .subarray(0, 8)
          .equals(payoutDiscriminator) ||
        ix.accounts.join(",") !== [intent.task_pda, intent.wallet].join(",") ||
        Buffer.from(ix.data_base64, "base64").subarray(8).toString("hex") !==
          new PublicKey(String(intent.arguments.payoutTo))
            .toBuffer()
            .toString("hex")
    )
  ) {
    throw new Error("unexpected Shillbot companion instruction");
  }
  if (lifecycle[0].accounts.join(",") !== intent.accounts.join(",")) {
    throw new Error("lifecycle accounts differ from intent");
  }
  return inspected;
}

export type WalletSignCallback = (
  transaction: Transaction | VersionedTransaction
) => Promise<Transaction | VersionedTransaction>;

export async function signAndBroadcast(
  connection: Connection,
  encodedUnsigned: string,
  sign: WalletSignCallback,
  confirmation?: BlockhashWithExpiryBlockHeight
): Promise<string> {
  const raw = Buffer.from(encodedUnsigned, "base64");
  let transaction: Transaction | VersionedTransaction;
  try {
    transaction = VersionedTransaction.deserialize(raw);
  } catch {
    transaction = Transaction.from(raw);
  }
  const signed = await sign(transaction);
  const wire =
    signed instanceof VersionedTransaction
      ? signed.serialize()
      : signed.serialize({
          requireAllSignatures: true,
          verifySignatures: true,
        });
  const signature = await connection.sendRawTransaction(wire);
  if (confirmation) {
    await connection.confirmTransaction(
      { signature, ...confirmation },
      "confirmed"
    );
  } else {
    await connection.confirmTransaction(signature, "confirmed");
  }
  return signature;
}

// Convenience aliases keep exact action names easy to discover and call.
export const buildCreate = (request: Omit<CreateBuild, "action">) =>
  buildTransaction({ ...request, action: "create" });
export const buildClaim = (request: Omit<ClaimBuild, "action">) =>
  buildTransaction({ ...request, action: "claim" });
export const buildSubmit = (request: Omit<SubmitBuild, "action">) =>
  buildTransaction({ ...request, action: "submit" });
export const buildApprove = (request: Omit<ApproveBuild, "action">) =>
  buildTransaction({ ...request, action: "approve" });
export const buildVerify = (request: Omit<VerifyBuild, "action">) =>
  buildTransaction({ ...request, action: "verify" });
export const buildFinalize = (request: Omit<FinalizeBuild, "action">) =>
  buildTransaction({ ...request, action: "finalize" });

export { buildSwitchboardCrankAndVerify } from "./switchboard.js";
export type { SwitchboardVerifyRequest } from "./switchboard.js";
