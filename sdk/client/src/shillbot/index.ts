import { Buffer } from "buffer";
import { sha256 } from "js-sha256";
import {
  BN,
  SHILLBOT_PROGRAM_ID,
  agentStatePda,
  buildShillbotInstruction,
  clientStatePda,
  globalStatePda,
  shillbotInstructionDiscriminator,
  taskPda,
} from "../contracts/index.js";
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
} from "../contracts/index.js";
export type {
  BuildShillbotInstructionRequest,
  CoordinationGame,
  Shillbot,
} from "../contracts/index.js";

export const SCHEMA_VERSION = "swarm.shillbot.transaction-intent/v1" as const;
export const EXTENSION_CREDIT_PROGRAM_ID = new PublicKey(
  "Ec628D7GH3hwgnVf1gqrUh83qcZprYjmTzvtdHHEr7oh"
);

export function advancePda(
  backer: PublicKey | string,
  agent: PublicKey | string
): PublicKey {
  return PublicKey.findProgramAddressSync(
    [
      Buffer.from("advance"),
      new PublicKey(backer).toBuffer(),
      new PublicKey(agent).toBuffer(),
    ],
    EXTENSION_CREDIT_PROGRAM_ID
  )[0];
}

export function routeAndRecoupDiscriminator(): Buffer {
  return Buffer.from(sha256.arrayBuffer("global:route_and_recoup")).subarray(
    0,
    8
  );
}

/** Exact Anchor account byte length of `extension-credit` `Advance` (`8 + 32 + 32 + 8 + 8 + 8 + 1 + 16 = 113`). */
export const ADVANCE_ACCOUNT_SPACE = 113;

export function advanceAccountDiscriminator(): Buffer {
  return Buffer.from(sha256.arrayBuffer("account:Advance")).subarray(0, 8);
}

export interface DecodedAdvanceAccount {
  backer: PublicKey;
  recipient: PublicKey;
  advanceLamports: bigint;
  recoupedLamports: bigint;
}

/**
 * Decode an on-chain `extension-credit` `Advance` account (`programs/extension-credit/src/state/advance.rs`).
 * Returns `null` if the length (`113` bytes) or 8-byte Anchor discriminator (`account:Advance`) does not match.
 */
export function decodeExtensionCreditAdvance(
  data: Uint8Array
): DecodedAdvanceAccount | null {
  const buf = Buffer.from(data.buffer, data.byteOffset, data.byteLength);
  if (buf.length !== ADVANCE_ACCOUNT_SPACE) {
    return null;
  }
  if (!buf.subarray(0, 8).equals(advanceAccountDiscriminator())) {
    return null;
  }
  const backer = new PublicKey(buf.subarray(8, 40));
  const recipient = new PublicKey(buf.subarray(40, 72));
  const view = new DataView(buf.buffer, buf.byteOffset, buf.byteLength);
  const advanceLamports = view.getBigUint64(72, true);
  const recoupedLamports = view.getBigUint64(80, true);
  return {
    backer,
    recipient,
    advanceLamports,
    recoupedLamports,
  };
}

export function buildRouteAndRecoupInstruction(accounts: {
  advance: PublicKey | string;
  backer: PublicKey | string;
  recipient: PublicKey | string;
}): TransactionInstruction {
  return new TransactionInstruction({
    programId: EXTENSION_CREDIT_PROGRAM_ID,
    keys: [
      { pubkey: new PublicKey(accounts.advance), isSigner: false, isWritable: true },
      { pubkey: new PublicKey(accounts.backer), isSigner: false, isWritable: true },
      { pubkey: new PublicKey(accounts.recipient), isSigner: false, isWritable: true },
    ],
    data: routeAndRecoupDiscriminator(),
  });
}

export type Action =
  | "create"
  | "claim"
  | "submit"
  | "approve"
  | "verify"
  | "finalize"
  | "expire";
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
  /** Optional true agent pubkey whose `AgentState` PDA is appended as remaining account `[5]`. */
  agentStateOwner?: string;
  /** Optional `extension-credit` backer pubkey to bundle atomic `route_and_recoup` after `finalize_task`. */
  recoupBacker?: string;
  /** Optional `extension-credit` recipient pubkey (defaults to `agentStateOwner ?? wallet`) for `route_and_recoup`. */
  recoupRecipient?: string;
}

export interface ExpireBuild extends CommonBuild {
  action: "expire";
  taskPda: string;
  /** Campaign client receiving the escrow refund (defaults to `wallet`). */
  client?: string;
  /** Optional claimed agent pubkey whose `AgentState` PDA is appended as remaining account `[3]`. */
  claimedAgent?: string;
}

export type BuildRequest =
  | CreateBuild
  | ClaimBuild
  | ApproveBuild
  | SubmitBuild
  | VerifyBuild
  | FinalizeBuild
  | ExpireBuild;

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
  if (request.action === "expire") {
    const ix = buildShillbotInstruction({
      name: "expire_task",
      accounts: {
        task,
        global_state: globalStatePda(),
        client: request.client ?? wallet,
      },
    });
    if (request.claimedAgent) {
      ix.keys.push({
        pubkey: agentStatePda(new PublicKey(request.claimedAgent)),
        isSigner: false,
        isWritable: true,
      });
    }
    return ix;
  }
  const finalize = request;
  const ix = buildShillbotInstruction({
    name: "finalize_task",
    accounts: {
      task,
      global_state: globalStatePda(),
      agent: finalize.agent,
      client: finalize.client,
      treasury: finalize.treasury,
    },
  });
  if (finalize.agentStateOwner) {
    ix.keys.push({
      pubkey: agentStatePda(new PublicKey(finalize.agentStateOwner)),
      isSigner: false,
      isWritable: true,
    });
  }
  return ix;
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
  return sha256(JSON.stringify(stable(intent)));
}

function actionRisk(action: Action): Risk {
  if (action === "create") return "spend";
  if (action === "approve" || action === "expire") return "escrow_control";
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
      : request.action === "finalize" && request.recoupBacker
      ? [
          buildRouteAndRecoupInstruction({
            advance: request.agent,
            backer: request.recoupBacker,
            recipient:
              request.recoupRecipient ??
              request.agentStateOwner ??
              request.wallet,
          }),
        ]
      : [];
  // set_payout_to authorizes against task.agent (after claim_task), and
  // route_and_recoup sweeps after finalize_task lands escrow in the Advance PDA.
  const instructions =
    request.action === "claim" || request.action === "finalize"
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

const COMPUTE_BUDGET_PROGRAM_ID = "ComputeBudget111111111111111111111111111111";
const SECP256K1_PROGRAM_ID = "KeccakSecp256k11111111111111111111111111111";
const SWITCHBOARD_PROGRAM_IDS: Record<Network, string> = {
  mainnet: "SBondMDrcV3K4kxZR1HNVT7osZxAHVHgYXL5Ze1oMUv",
  devnet: "Aio4gaXjXzJNVLtzwtNVmSqGKpANtXhybbkhtAC94ji2",
};

export function inspectTransaction(encoded: string): TransactionInspection {
  const bytes = Buffer.from(encoded, "base64");
  let tx: VersionedTransaction | undefined;
  try {
    tx = VersionedTransaction.deserialize(bytes);
  } catch {
    tx = undefined;
  }
  if (tx) {
    if (tx.message.addressTableLookups.length > 0) {
      throw new Error(
        "address lookup tables are not allowed in Shillbot lifecycle transactions"
      );
    }
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
  }
  const legacyTx = Transaction.from(bytes);
  const message = legacyTx.compileMessage();
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
  const shillbotProgramId = SHILLBOT_PROGRAM_ID.toBase58();
  const extensionCreditId = EXTENSION_CREDIT_PROGRAM_ID.toBase58();
  const allowedPrograms = new Set<string>([
    shillbotProgramId,
    COMPUTE_BUDGET_PROGRAM_ID,
    ...(intent.action === "verify" && intent.network === "devnet"
      ? [SWITCHBOARD_PROGRAM_IDS.devnet, SECP256K1_PROGRAM_ID]
      : []),
    ...(intent.action === "finalize" ? [extensionCreditId] : []),
  ]);
  for (const ix of inspected.instructions) {
    if (!allowedPrograms.has(ix.program_id)) {
      throw new Error(`unexpected companion program ${ix.program_id}`);
    }
  }
  validateComputeBudgetInstructions(inspected.instructions);
  const expectedDiscriminator = shillbotInstructionDiscriminator(
    intent.action === "submit" ? "submit_work" : `${intent.action}_task`
  );
  const lifecycle = inspected.instructions.filter(
    (ix) =>
      ix.program_id === shillbotProgramId &&
      Buffer.from(ix.data_base64, "base64")
        .subarray(0, 8)
        .equals(expectedDiscriminator)
  );
  if (lifecycle.length !== 1)
    throw new Error("expected exactly one Shillbot lifecycle instruction");
  const shillbotInstructions = inspected.instructions.filter(
    (ix) => ix.program_id === shillbotProgramId
  );
  const payoutDiscriminator = shillbotInstructionDiscriminator("set_payout_to");
  const companions = shillbotInstructions.filter((ix) => ix !== lifecycle[0]);
  const lifecycleIndex = inspected.instructions.indexOf(lifecycle[0]);
  const switchboardPullFeedDiscriminator = Buffer.from(
    sha256.arrayBuffer("global:pull_feed_submit_response_consensus")
  ).subarray(0, 8);
  const secpEntries = inspected.instructions
    .map((ix, txIndex) => ({ ix, txIndex }))
    .filter((entry) => entry.ix.program_id === SECP256K1_PROGRAM_ID);
  const switchboardEntries = inspected.instructions
    .map((ix, txIndex) => ({ ix, txIndex }))
    .filter(
      (entry) =>
        entry.ix.program_id === SWITCHBOARD_PROGRAM_IDS.mainnet ||
        entry.ix.program_id === SWITCHBOARD_PROGRAM_IDS.devnet
    );
  if (secpEntries.length > 0 || switchboardEntries.length > 0) {
    if (intent.network !== "devnet" || intent.action !== "verify") {
      throw new Error(
        "Switchboard/Secp256k1 crank instructions are only permitted on devnet verify_task"
      );
    }
    if (secpEntries.length !== 1 || switchboardEntries.length !== 1) {
      throw new Error(
        "devnet verify_task Switchboard crank requires exactly 1 Secp256k1 and 1 Switchboard instruction"
      );
    }
    const secpEntry = secpEntries[0];
    const sbEntry = switchboardEntries[0];
    if (sbEntry.ix.program_id !== SWITCHBOARD_PROGRAM_IDS.devnet) {
      throw new Error(`unexpected Switchboard program ${sbEntry.ix.program_id}`);
    }
    const secpData = Buffer.from(secpEntry.ix.data_base64, "base64");
    if (secpData.length === 0 || secpData[0] === 0) {
      throw new Error(
        "Secp256k1 companion instruction must carry at least 1 signature"
      );
    }
    if (
      secpEntry.txIndex + 1 !== sbEntry.txIndex ||
      sbEntry.txIndex >= lifecycleIndex
    ) {
      throw new Error(
        "Switchboard crank must follow exact order Secp256k1 -> Switchboard -> verify_task"
      );
    }
    const sbData = Buffer.from(sbEntry.ix.data_base64, "base64");
    if (
      sbData.length < 8 ||
      !sbData.subarray(0, 8).equals(switchboardPullFeedDiscriminator)
    ) {
      throw new Error(
        "Switchboard companion instruction must be pull_feed_submit_response_consensus"
      );
    }
    const expectedFeed = lifecycle[0].accounts[2];
    if (!expectedFeed || !sbEntry.ix.accounts.includes(expectedFeed)) {
      throw new Error(
        "Switchboard companion instruction does not reference expected feed"
      );
    }
  }
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
  const creditInstructions = inspected.instructions.filter(
    (ix) => ix.program_id === extensionCreditId
  );
  if (creditInstructions.length > 0) {
    if (intent.action !== "finalize" || creditInstructions.length !== 1) {
      throw new Error("unexpected extension-credit companion instruction");
    }
    const creditIx = creditInstructions[0];
    if (inspected.instructions.indexOf(creditIx) <= lifecycleIndex) {
      throw new Error("route_and_recoup must follow finalize_task");
    }
    const creditData = Buffer.from(creditIx.data_base64, "base64");
    if (
      creditData.length !== 8 ||
      !creditData.equals(routeAndRecoupDiscriminator()) ||
      creditIx.accounts.length !== 3 ||
      creditIx.accounts[0] !== intent.accounts[2]
    ) {
      throw new Error("malformed route_and_recoup companion instruction");
    }
  }
  if (lifecycle[0].accounts.join(",") !== intent.accounts.join(",")) {
    throw new Error("lifecycle accounts differ from intent");
  }
  if (
    intent.action === "create" &&
    intent.arguments.escrowLamports !== undefined
  ) {
    const data = Buffer.from(lifecycle[0].data_base64, "base64");
    if (data.length < 24) throw new Error("create instruction data is truncated");
    const view = new DataView(data.buffer, data.byteOffset + 16, 8);
    if (view.getBigUint64(0, true) !== BigInt(String(intent.arguments.escrowLamports))) {
      throw new Error("escrow amount differs from intent");
    }
  }
  if (
    intent.action === "submit" &&
    typeof intent.arguments.contentId === "string"
  ) {
    const data = Buffer.from(lifecycle[0].data_base64, "base64");
    if (data.length < 12) throw new Error("submit instruction data is truncated");
    const len = data.readUInt32LE(8);
    if (
      data.length !== 12 + len ||
      data.subarray(12, 12 + len).toString("utf8") !== intent.arguments.contentId
    ) {
      throw new Error("content id differs from intent");
    }
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
  const confirmed = confirmation
    ? await connection.confirmTransaction(
        { signature, ...confirmation },
        "confirmed"
      )
    : await connection.confirmTransaction(signature, "confirmed");
  if (confirmed?.value?.err) {
    throw new Error(
      `transaction ${signature} failed on-chain: ${JSON.stringify(confirmed.value.err)}`
    );
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
export const buildExpire = (request: Omit<ExpireBuild, "action">) =>
  buildTransaction({ ...request, action: "expire" });

export { buildSwitchboardCrankAndVerify } from "./switchboard.js";
export type { SwitchboardVerifyRequest } from "./switchboard.js";
export * from "./payout-oracle.js";
import { validateComputeBudgetInstructions } from "./verify-tx.js";
export {
  MAX_COMPUTE_UNIT_LIMIT,
  MAX_COMPUTE_UNIT_PRICE_MICROLAMPORTS,
  SHILLBOT_INSTRUCTION_SPECS,
  deriveAdvancePda,
  validateComputeBudgetInstruction,
  validateComputeBudgetInstructions,
  verifyBuiltTransaction,
  type AuthoritySlotRule,
  type ShillbotInstructionName,
  type ShillbotInstructionSpec,
  type VerifyBuiltTransactionExpectation,
} from "./verify-tx.js";
