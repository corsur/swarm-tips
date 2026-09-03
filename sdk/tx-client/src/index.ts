import { createHash } from "node:crypto";
import bs58 from "bs58";
import {
  Connection,
  PublicKey,
  SystemProgram,
  SYSVAR_SLOT_HASHES_PUBKEY,
  Transaction,
  TransactionInstruction,
  TransactionMessage,
  VersionedTransaction,
  type BlockhashWithExpiryBlockHeight,
} from "@solana/web3.js";

export const SCHEMA_VERSION = "swarm.shillbot.transaction-intent/v1" as const;
export const SHILLBOT_PROGRAM_ID = new PublicKey(
  "2tR37nqMpwdV4DVUHjzUmL1rH2DtkA8zrRA4EAhT7KMi",
);

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

function discriminator(name: string): Buffer {
  return createHash("sha256").update(`global:${name}`).digest().subarray(0, 8);
}

function u64(value: string | bigint): Buffer {
  const out = Buffer.alloc(8);
  out.writeBigUInt64LE(BigInt(value));
  return out;
}

function i64(value: string | bigint): Buffer {
  const out = Buffer.alloc(8);
  out.writeBigInt64LE(BigInt(value));
  return out;
}

function u32(value: number): Buffer {
  const out = Buffer.alloc(4);
  out.writeUInt32LE(value);
  return out;
}

function bytes(value: string): Buffer {
  const raw = Buffer.from(value, "utf8");
  return Buffer.concat([u32(raw.length), raw]);
}

export function globalStatePda(): PublicKey {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("shillbot_global")],
    SHILLBOT_PROGRAM_ID,
  )[0];
}

export function taskPda(nonce: bigint, client: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("task"), u64(nonce), client.toBuffer()],
    SHILLBOT_PROGRAM_ID,
  )[0];
}

export function clientStatePda(client: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("client_state"), client.toBuffer()],
    SHILLBOT_PROGRAM_ID,
  )[0];
}

export function agentStatePda(agent: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("agent_state"), agent.toBuffer()],
    SHILLBOT_PROGRAM_ID,
  )[0];
}

export function buildInstruction(request: BuildRequest): TransactionInstruction {
  const wallet = new PublicKey(request.wallet);
  if (request.action === "create") {
    const nonce = BigInt(request.nonce);
    return new TransactionInstruction({
      programId: SHILLBOT_PROGRAM_ID,
      keys: [
        { pubkey: globalStatePda(), isSigner: false, isWritable: true },
        { pubkey: taskPda(nonce, wallet), isSigner: false, isWritable: true },
        { pubkey: clientStatePda(wallet), isSigner: false, isWritable: true },
        { pubkey: wallet, isSigner: true, isWritable: true },
        { pubkey: SYSVAR_SLOT_HASHES_PUBKEY, isSigner: false, isWritable: false },
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      ],
      data: Buffer.concat([
        discriminator("create_task"),
        u64(nonce),
        u64(request.escrowLamports),
        Buffer.from(request.contentHash, "hex"),
        i64(request.deadline),
        i64(request.submitMargin),
        i64(request.claimBuffer),
        Buffer.from([request.platform]),
        u32(request.attestationDelayOverride),
        u32(request.challengeWindowOverride),
        u32(request.verificationTimeoutOverride),
        Buffer.from([request.requiresApproval ? 1 : 0, request.verificationKind]),
      ]),
    });
  }

  const task = new PublicKey(request.taskPda);
  if (request.action === "claim") {
    return new TransactionInstruction({
      programId: SHILLBOT_PROGRAM_ID,
      keys: [
        { pubkey: task, isSigner: false, isWritable: true },
        { pubkey: globalStatePda(), isSigner: false, isWritable: false },
        { pubkey: agentStatePda(wallet), isSigner: false, isWritable: true },
        { pubkey: wallet, isSigner: true, isWritable: true },
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      ],
      data: discriminator("claim_task"),
    });
  }
  if (request.action === "submit") {
    return new TransactionInstruction({
      programId: SHILLBOT_PROGRAM_ID,
      keys: [
        { pubkey: task, isSigner: false, isWritable: true },
        { pubkey: globalStatePda(), isSigner: false, isWritable: false },
        { pubkey: agentStatePda(wallet), isSigner: false, isWritable: true },
        { pubkey: wallet, isSigner: true, isWritable: false },
      ],
      data: Buffer.concat([discriminator("submit_work"), bytes(request.contentId)]),
    });
  }
  if (request.action === "approve") {
    return new TransactionInstruction({
      programId: SHILLBOT_PROGRAM_ID,
      keys: [
        { pubkey: task, isSigner: false, isWritable: true },
        { pubkey: wallet, isSigner: true, isWritable: false },
      ],
      data: discriminator("approve_task"),
    });
  }
  if (request.action === "verify") {
    return new TransactionInstruction({
      programId: SHILLBOT_PROGRAM_ID,
      keys: [
        { pubkey: task, isSigner: false, isWritable: true },
        { pubkey: globalStatePda(), isSigner: false, isWritable: false },
        {
          pubkey: new PublicKey(request.switchboardFeed),
          isSigner: false,
          isWritable: false,
        },
      ],
      data: Buffer.concat([
        discriminator("verify_task"),
        u64(request.compositeScore),
        Buffer.from(request.verificationHash, "hex"),
      ]),
    });
  }
  const finalize = request as FinalizeBuild;
  return new TransactionInstruction({
    programId: SHILLBOT_PROGRAM_ID,
    keys: [
      { pubkey: task, isSigner: false, isWritable: true },
      { pubkey: globalStatePda(), isSigner: false, isWritable: false },
      { pubkey: new PublicKey(finalize.agent), isSigner: false, isWritable: true },
      { pubkey: new PublicKey(finalize.client), isSigner: false, isWritable: true },
      { pubkey: new PublicKey(finalize.treasury), isSigner: false, isWritable: true },
    ],
    data: discriminator("finalize_task"),
  });
}

function payoutInstruction(request: ClaimBuild): TransactionInstruction | undefined {
  if (!request.payoutTo) return undefined;
  return new TransactionInstruction({
    programId: SHILLBOT_PROGRAM_ID,
    keys: [
      { pubkey: new PublicKey(request.taskPda), isSigner: false, isWritable: true },
      { pubkey: new PublicKey(request.wallet), isSigner: true, isWritable: false },
    ],
    data: Buffer.concat([
      discriminator("set_payout_to"),
      new PublicKey(request.payoutTo).toBuffer(),
    ]),
  });
}

function stable(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(stable);
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value as Record<string, unknown>)
        .filter(([, child]) => child !== undefined)
        .sort(([a], [b]) => a.localeCompare(b))
        .map(([key, child]) => [key, stable(child)]),
    );
  }
  return value;
}

export function intentDigest(intent: Omit<TransactionIntent, "digest">): string {
  return createHash("sha256").update(JSON.stringify(stable(intent))).digest("hex");
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
  version: TransactionVersion,
): string {
  if (version === "v0") {
    const message = new TransactionMessage({
      payerKey: payer,
      recentBlockhash: blockhash,
      instructions,
    }).compileToV0Message();
    return Buffer.from(new VersionedTransaction(message).serialize()).toString("base64");
  }
  const tx = new Transaction({ feePayer: payer, recentBlockhash: blockhash }).add(
    ...instructions,
  );
  return tx.serialize({ requireAllSignatures: false, verifySignatures: false }).toString("base64");
}

export function buildTransaction(request: BuildRequest): BuiltTransaction {
  const lifecycle = buildInstruction(request);
  const sponsor = request.action === "claim" || request.action === "submit" ? request.sponsor : undefined;
  const payer = new PublicKey(sponsor ?? request.wallet);
  const companions = request.action === "verify"
    ? request.crankInstructions
    : request.action === "claim"
      ? [payoutInstruction(request)].filter((ix): ix is TransactionInstruction => ix !== undefined)
      : [];
  const unsignedTx = serializeUnsigned(
    payer,
    request.recentBlockhash,
    [...companions, lifecycle],
    request.version ?? (request.action === "verify" ? "v0" : "legacy"),
  );
  const task = lifecycle.keys[request.action === "create" ? 1 : 0].pubkey.toBase58();
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
        !["action", "wallet", "network", "recentBlockhash", "version", "sponsor", "crankInstructions"].includes(key) &&
        ["string", "number", "boolean"].includes(typeof value),
    ),
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
    transaction_intent: { ...withoutDigest, digest: intentDigest(withoutDigest) },
  };
}

export interface TransactionInspection {
  version: TransactionVersion;
  fee_payer: string;
  signers: string[];
  instructions: Array<{ program_id: string; accounts: string[]; data_base64: string }>;
}

export function inspectTransaction(encoded: string): TransactionInspection {
  const bytes = Buffer.from(encoded, "base64");
  try {
    const tx = VersionedTransaction.deserialize(bytes);
    const keys = tx.message.staticAccountKeys;
    return {
      version: tx.version === "legacy" ? "legacy" : "v0",
      fee_payer: keys[0].toBase58(),
      signers: keys.slice(0, tx.message.header.numRequiredSignatures).map((key) => key.toBase58()),
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
        accounts: ix.accounts.map((index) => message.accountKeys[index].toBase58()),
        data_base64: Buffer.from(bs58.decode(ix.data)).toString("base64"),
      })),
    };
  }
}

export function verifyIntent(encoded: string, intent: TransactionIntent): TransactionInspection {
  const { digest: _digest, ...rest } = intent;
  const expectedDigest = intentDigest(rest);
  if (expectedDigest !== intent.digest) throw new Error("intent digest mismatch");
  const inspected = inspectTransaction(encoded);
  if (inspected.fee_payer !== intent.fee_payer) throw new Error("fee payer differs from intent");
  if (!inspected.signers.includes(intent.wallet)) throw new Error("wallet is not a required signer");
  const expectedDiscriminator = discriminator(
    intent.action === "submit" ? "submit_work" : `${intent.action}_task`,
  );
  const lifecycle = inspected.instructions.filter(
    (ix) =>
      ix.program_id === SHILLBOT_PROGRAM_ID.toBase58() &&
      Buffer.from(ix.data_base64, "base64").subarray(0, 8).equals(expectedDiscriminator),
  );
  if (lifecycle.length !== 1) throw new Error("expected exactly one Shillbot lifecycle instruction");
  const shillbotInstructions = inspected.instructions.filter(
    (ix) => ix.program_id === SHILLBOT_PROGRAM_ID.toBase58(),
  );
  const payoutDiscriminator = discriminator("set_payout_to");
  const companions = shillbotInstructions.filter((ix) => ix !== lifecycle[0]);
  if (
    companions.length > (intent.action === "claim" && intent.arguments.payoutTo ? 1 : 0) ||
    companions.some(
      (ix) =>
        intent.action !== "claim" ||
        !Buffer.from(ix.data_base64, "base64").subarray(0, 8).equals(payoutDiscriminator) ||
        ix.accounts.join(",") !== [intent.task_pda, intent.wallet].join(",") ||
        Buffer.from(ix.data_base64, "base64").subarray(8).toString("hex") !==
          new PublicKey(String(intent.arguments.payoutTo)).toBuffer().toString("hex"),
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
  transaction: Transaction | VersionedTransaction,
) => Promise<Transaction | VersionedTransaction>;

export async function signAndBroadcast(
  connection: Connection,
  encodedUnsigned: string,
  sign: WalletSignCallback,
  confirmation?: BlockhashWithExpiryBlockHeight,
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
      : signed.serialize({ requireAllSignatures: true, verifySignatures: true });
  const signature = await connection.sendRawTransaction(wire);
  if (confirmation) {
    await connection.confirmTransaction({ signature, ...confirmation }, "confirmed");
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
