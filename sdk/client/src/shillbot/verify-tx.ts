import { Buffer } from "buffer";
import {
  ComputeBudgetProgram,
  PublicKey,
  SYSVAR_INSTRUCTIONS_PUBKEY,
  Secp256k1Program,
} from "@solana/web3.js";
import { SHILLBOT_PROGRAM_ID, shillbotInstructionDiscriminator } from "../contracts/index.js";
import { SwarmClientError, TransactionMismatchError } from "../errors.js";
import {
  EXTENSION_CREDIT_PROGRAM_ID,
  advancePda,
  inspectTransaction,
  routeAndRecoupDiscriminator,
  type Action,
  type Network,
  type TransactionInspection,
} from "./index.js";

/** Alias for `advancePda` (`seeds = [b"advance", sponsorPubkey, agentPubkey]`). */
export const deriveAdvancePda = advancePda;

/** Default maximum allowed Solana priority fee price per compute unit (`100_000` micro-lamports/CU). */
export const MAX_COMPUTE_UNIT_PRICE_MICROLAMPORTS = 100_000n;

/** Solana runtime ceiling for `ComputeBudget::SetComputeUnitLimit` (`1_400_000` CUs). */
export const MAX_COMPUTE_UNIT_LIMIT = 1_400_000;

/** Full passing composite score (`1_000_000`) required on M2 Open-Competition 1-tx verify bundles. */
const MAX_VERIFY_SCORE = 1_000_000n;

const MIN_HEAP_FRAME_BYTES = 32 * 1024;
const MAX_HEAP_FRAME_BYTES = 256 * 1024;

const SWITCHBOARD_PROGRAM_IDS: Readonly<Record<Network, string>> = {
  mainnet: "SBondMDrcV3K4kxZR1HNVT7osZxAHVHgYXL5Ze1oMUv",
  devnet: "Aio4gaXjXzJNVLtzwtNVmSqGKpANtXhybbkhtAC94ji2",
};

export type ShillbotInstructionName =
  | "initialize"
  | "create_task"
  | "claim_task"
  | "claim_task_session"
  | "set_payout_to"
  | "submit_work"
  | "submit_work_session"
  | "approve_task"
  | "verify_task"
  | "verify_task_attested"
  | "challenge_task"
  | "finalize_task"
  | "expire_task"
  | "resolve_challenge"
  | "resolve_challenge_default"
  | "update_params"
  | "transfer_authority"
  | "update_treasury"
  | "update_oracle_authority"
  | "emergency_return"
  | "create_session"
  | "revoke_session"
  | "register_identity"
  | "revoke_identity"
  | "close_agent_state"
  | "migrate_agent_state"
  | "migrate_task";

export type AuthoritySlotRule = number | "fee_payer" | "verify_task" | "verify_task_attested";

export interface ShillbotInstructionSpec {
  readonly name: ShillbotInstructionName;
  readonly minAccounts: number;
  readonly exactAccounts?: number;
  readonly taskSlot: number | null;
  readonly authoritySlot: AuthoritySlotRule;
  /** True when third-party gas sponsorship (`fee_payer !== wallet`) is permitted by protocol design. */
  readonly allowsSponsoredFeePayer: boolean;
}

/**
 * Exact Anchor account-index and authority-slot specification for every Shillbot instruction,
 * derived directly from `programs/shillbot/src/instructions/*.rs` and `shillbot.json` IDL.
 */
export const SHILLBOT_INSTRUCTION_SPECS: Readonly<
  Record<ShillbotInstructionName, ShillbotInstructionSpec>
> = {
  create_task: {
    name: "create_task",
    minAccounts: 6,
    taskSlot: 1,
    authoritySlot: 3, // client (Signer)
    allowsSponsoredFeePayer: false,
  },
  claim_task: {
    name: "claim_task",
    minAccounts: 5,
    taskSlot: 0,
    authoritySlot: 3, // agent (Signer)
    allowsSponsoredFeePayer: true,
  },
  claim_task_session: {
    name: "claim_task_session",
    minAccounts: 7,
    taskSlot: 0,
    authoritySlot: 4, // delegate (Signer)
    allowsSponsoredFeePayer: true,
  },
  set_payout_to: {
    name: "set_payout_to",
    minAccounts: 2,
    exactAccounts: 2,
    taskSlot: 0,
    authoritySlot: 1, // agent (Signer)
    allowsSponsoredFeePayer: true,
  },
  submit_work: {
    name: "submit_work",
    minAccounts: 4,
    taskSlot: 0,
    authoritySlot: 3, // agent (Signer)
    allowsSponsoredFeePayer: true,
  },
  submit_work_session: {
    name: "submit_work_session",
    minAccounts: 5,
    taskSlot: 0,
    authoritySlot: 4, // delegate (Signer)
    allowsSponsoredFeePayer: true,
  },
  approve_task: {
    name: "approve_task",
    minAccounts: 2,
    taskSlot: 0,
    authoritySlot: 1, // client (Signer)
    allowsSponsoredFeePayer: false,
  },
  verify_task: {
    name: "verify_task",
    minAccounts: 3,
    taskSlot: 0,
    authoritySlot: "verify_task",
    allowsSponsoredFeePayer: false,
  },
  verify_task_attested: {
    name: "verify_task_attested",
    minAccounts: 3,
    taskSlot: 0,
    authoritySlot: "verify_task_attested", // attester at [2] (Signer)
    allowsSponsoredFeePayer: true,
  },
  finalize_task: {
    name: "finalize_task",
    minAccounts: 5,
    taskSlot: 0,
    authoritySlot: "fee_payer",
    allowsSponsoredFeePayer: false,
  },
  challenge_task: {
    name: "challenge_task",
    minAccounts: 5,
    taskSlot: 0,
    authoritySlot: 3, // challenger (Signer)
    allowsSponsoredFeePayer: false,
  },
  expire_task: {
    name: "expire_task",
    minAccounts: 3,
    taskSlot: 0,
    authoritySlot: "fee_payer",
    allowsSponsoredFeePayer: false,
  },
  resolve_challenge: {
    name: "resolve_challenge",
    minAccounts: 8,
    taskSlot: 0,
    authoritySlot: 3, // authority (Signer)
    allowsSponsoredFeePayer: false,
  },
  resolve_challenge_default: {
    name: "resolve_challenge_default",
    minAccounts: 7,
    taskSlot: 0,
    authoritySlot: "fee_payer",
    allowsSponsoredFeePayer: false,
  },
  create_session: {
    name: "create_session",
    minAccounts: 4,
    taskSlot: null,
    authoritySlot: 1, // agent (Signer)
    allowsSponsoredFeePayer: false,
  },
  revoke_session: {
    name: "revoke_session",
    minAccounts: 2,
    taskSlot: null,
    authoritySlot: 1, // agent (Signer)
    allowsSponsoredFeePayer: false,
  },
  register_identity: {
    name: "register_identity",
    minAccounts: 3,
    taskSlot: null,
    authoritySlot: 1, // agent (Signer)
    allowsSponsoredFeePayer: false,
  },
  revoke_identity: {
    name: "revoke_identity",
    minAccounts: 2,
    taskSlot: null,
    authoritySlot: 1, // agent (Signer)
    allowsSponsoredFeePayer: false,
  },
  close_agent_state: {
    name: "close_agent_state",
    minAccounts: 2,
    taskSlot: null,
    authoritySlot: 1, // agent (Signer)
    allowsSponsoredFeePayer: false,
  },
  migrate_agent_state: {
    name: "migrate_agent_state",
    minAccounts: 2,
    taskSlot: null,
    authoritySlot: 0, // agent (Signer)
    allowsSponsoredFeePayer: false,
  },
  migrate_task: {
    name: "migrate_task",
    minAccounts: 2,
    taskSlot: 0,
    authoritySlot: 1, // payer (Signer)
    allowsSponsoredFeePayer: false,
  },
  initialize: {
    name: "initialize",
    minAccounts: 4,
    taskSlot: null,
    authoritySlot: 1, // authority (Signer)
    allowsSponsoredFeePayer: false,
  },
  update_params: {
    name: "update_params",
    minAccounts: 2,
    taskSlot: null,
    authoritySlot: 1, // authority (Signer)
    allowsSponsoredFeePayer: false,
  },
  transfer_authority: {
    name: "transfer_authority",
    minAccounts: 2,
    taskSlot: null,
    authoritySlot: 1, // authority (Signer)
    allowsSponsoredFeePayer: false,
  },
  update_treasury: {
    name: "update_treasury",
    minAccounts: 2,
    taskSlot: null,
    authoritySlot: 1, // authority (Signer)
    allowsSponsoredFeePayer: false,
  },
  update_oracle_authority: {
    name: "update_oracle_authority",
    minAccounts: 2,
    taskSlot: null,
    authoritySlot: 1, // authority (Signer)
    allowsSponsoredFeePayer: false,
  },
  emergency_return: {
    name: "emergency_return",
    minAccounts: 2,
    taskSlot: null,
    authoritySlot: 1, // authority (Signer)
    allowsSponsoredFeePayer: false,
  },
};

const DISCRIMINATOR_TO_SPEC: ReadonlyMap<string, ShillbotInstructionSpec> = (() => {
  const map = new Map<string, ShillbotInstructionSpec>();
  for (const spec of Object.values(SHILLBOT_INSTRUCTION_SPECS)) {
    const discHex = shillbotInstructionDiscriminator(spec.name).toString("hex");
    map.set(discHex, spec);
  }
  return map;
})();

export type ExpireTaskState = "open" | "claimed" | "submitted" | "approved";

export interface VerifyBuiltTransactionExpectation {
  action?: Action;
  instructionName?: ShillbotInstructionName;
  network: Network;
  wallet: string;
  taskPda?: string;
  feePayer?: string;
  /** Optional alias for `feePayer` when pinning a known gas sponsor pubkey. */
  sponsor?: string;
  /**
   * Explicit authorized payout destination pubkey (base58).
   * Required whenever a `set_payout_to` instruction is present on self-paid transactions;
   * on sponsored `claim`/`submit` (`fee_payer !== wallet`), deterministic
   * `advancePda(fee_payer, wallet)` is also authorized.
   */
  payoutTo?: string;
  /**
   * Optional task state when validating `expire_task`: enforces exact 3 accounts
   * `[task, global_state, client]` on `open | submitted | approved` (rejecting the
   * 4-account `AgentState` form) and exact 4 accounts on `claimed`.
   */
  expireTaskState?: ExpireTaskState;
  oracleAuthority?: string;
  escrowLamports?: bigint;
  contentId?: string;
  maxComputeUnitPriceMicroLamports?: bigint;
  allowedProgramIds?: string[];
}

function txMismatch(operation: string, message: string): TransactionMismatchError {
  return new TransactionMismatchError({
    operation,
    message,
  });
}

function invalidArg(operation: string, message: string): SwarmClientError {
  return new SwarmClientError({
    code: "INVALID_ARGUMENT",
    operation,
    message,
  });
}

/**
 * Validate a single `ComputeBudget111111111111111111111111111111` instruction's tag,
 * byte length, and value bounds. Never early-returns from a multi-instruction loop.
 */
export function validateComputeBudgetInstruction(
  data: Uint8Array,
  maxComputeUnitPriceMicroLamports: bigint = MAX_COMPUTE_UNIT_PRICE_MICROLAMPORTS,
  operation = "shillbot.verifyBuiltTransaction",
): void {
  const buf = Buffer.from(data.buffer, data.byteOffset, data.byteLength);
  if (buf.length === 0) {
    throw txMismatch(operation, "Empty ComputeBudget instruction data");
  }
  const tag = buf[0];
  if (tag === 1) {
    // RequestHeapFrame(u32 bytes)
    if (buf.length !== 5) {
      throw txMismatch(
        operation,
        `Malformed ComputeBudget RequestHeapFrame length: expected 5 bytes, got ${buf.length}`,
      );
    }
    const heapBytes = buf.readUInt32LE(1);
    if (
      heapBytes < MIN_HEAP_FRAME_BYTES ||
      heapBytes > MAX_HEAP_FRAME_BYTES ||
      heapBytes % 1024 !== 0
    ) {
      throw txMismatch(
        operation,
        `ComputeBudget RequestHeapFrame ${heapBytes} out of valid range (${MIN_HEAP_FRAME_BYTES}..${MAX_HEAP_FRAME_BYTES}, multiple of 1024)`,
      );
    }
    return;
  }
  if (tag === 2) {
    // SetComputeUnitLimit(u32 units)
    if (buf.length !== 5) {
      throw txMismatch(
        operation,
        `Malformed ComputeBudget SetComputeUnitLimit length: expected 5 bytes, got ${buf.length}`,
      );
    }
    const units = buf.readUInt32LE(1);
    if (units < 1 || units > MAX_COMPUTE_UNIT_LIMIT) {
      throw txMismatch(
        operation,
        `ComputeBudget SetComputeUnitLimit ${units} out of valid range (1..${MAX_COMPUTE_UNIT_LIMIT})`,
      );
    }
    return;
  }
  if (tag === 3) {
    // SetComputeUnitPrice(u64 microLamports)
    if (buf.length !== 9) {
      throw txMismatch(
        operation,
        `Malformed ComputeBudget SetComputeUnitPrice length: expected 9 bytes, got ${buf.length}`,
      );
    }
    const view = new DataView(buf.buffer, buf.byteOffset + 1, 8);
    const microLamports = view.getBigUint64(0, true);
    if (microLamports > maxComputeUnitPriceMicroLamports) {
      throw txMismatch(
        operation,
        `ComputeBudget SetComputeUnitPrice ${microLamports} micro-lamports/CU exceeds maximum allowed ${maxComputeUnitPriceMicroLamports}`,
      );
    }
    return;
  }
  if (tag === 4) {
    // SetLoadedAccountsDataSizeLimit(u32 bytes)
    if (buf.length !== 5) {
      throw txMismatch(
        operation,
        `Malformed ComputeBudget SetLoadedAccountsDataSizeLimit length: expected 5 bytes, got ${buf.length}`,
      );
    }
    const bytesLimit = buf.readUInt32LE(1);
    if (bytesLimit === 0) {
      throw txMismatch(operation, "ComputeBudget SetLoadedAccountsDataSizeLimit cannot be zero");
    }
    return;
  }
  throw txMismatch(operation, `Unsupported or deprecated ComputeBudget instruction tag ${tag}`);
}

/**
 * Validate every `ComputeBudget` instruction in an inspected transaction without
 * short-circuiting on `RequestHeapFrame` (tag 1) or any preceding instruction.
 */
export function validateComputeBudgetInstructions(
  instructions: TransactionInspection["instructions"],
  maxComputeUnitPriceMicroLamports: bigint = MAX_COMPUTE_UNIT_PRICE_MICROLAMPORTS,
  operation = "shillbot.verifyBuiltTransaction",
): void {
  const computeBudgetId = ComputeBudgetProgram.programId.toBase58();
  const seenTags = new Set<number>();
  for (const ix of instructions) {
    if (ix.program_id !== computeBudgetId) {
      continue;
    }
    const data = Buffer.from(ix.data_base64, "base64");
    if (data.length === 0) {
      throw txMismatch(operation, "Empty ComputeBudget instruction data");
    }
    const tag = data[0];
    if (seenTags.has(tag)) {
      throw txMismatch(operation, `Duplicate ComputeBudget instruction tag ${tag}`);
    }
    seenTags.add(tag);
    validateComputeBudgetInstruction(data, maxComputeUnitPriceMicroLamports, operation);
  }
}

interface DecodedShillbotIx {
  readonly ix: TransactionInspection["instructions"][number];
  readonly txIndex: number;
  readonly data: Buffer;
  readonly spec: ShillbotInstructionSpec;
}

function allowedInstructionNamesForAction(action: Action): readonly ShillbotInstructionName[] {
  switch (action) {
    case "create":
      return ["create_task"];
    case "claim":
      return ["claim_task", "claim_task_session"];
    case "submit":
      return ["submit_work", "submit_work_session"];
    case "approve":
      return ["approve_task"];
    case "verify":
      return ["verify_task", "verify_task_attested"];
    case "finalize":
      return ["finalize_task"];
    case "expire":
      return ["expire_task"];
    default:
      return [];
  }
}

function parseValidatedPubkey(value: string, fieldName: string, operation: string): PublicKey {
  try {
    const pk = new PublicKey(value);
    if (pk.toBase58() !== value) {
      throw new Error("non-canonical base58");
    }
    return pk;
  } catch {
    throw invalidArg(operation, `Invalid ${fieldName} public key: ${value}`);
  }
}

function validateSetPayoutToInstruction(
  decoded: DecodedShillbotIx,
  expectedWallet: string,
  expectedTaskPda: string,
  expectedPayoutPubkey: PublicKey | undefined,
  operation: string,
): void {
  if (!expectedPayoutPubkey) {
    throw txMismatch(
      operation,
      "Unexpected set_payout_to instruction: caller did not authorize expected.payoutTo",
    );
  }
  const payoutB58 = expectedPayoutPubkey.toBase58();
  if (payoutB58 === PublicKey.default.toBase58()) {
    throw txMismatch(operation, "set_payout_to destination cannot be the default zero pubkey");
  }
  if (payoutB58 === expectedTaskPda) {
    throw txMismatch(operation, "set_payout_to destination cannot equal the task PDA");
  }
  if (decoded.ix.accounts.length !== 2) {
    throw txMismatch(
      operation,
      `set_payout_to must have exact 2 accounts [task, agent], got ${decoded.ix.accounts.length}`,
    );
  }
  if (decoded.ix.accounts[0] !== expectedTaskPda) {
    throw txMismatch(operation, "set_payout_to account [0] does not match expected task PDA");
  }
  if (decoded.ix.accounts[1] !== expectedWallet) {
    throw txMismatch(
      operation,
      "set_payout_to authority slot [1] does not match expected agent wallet",
    );
  }
  if (decoded.data.length !== 40) {
    throw txMismatch(
      operation,
      `set_payout_to data must be exact 40 bytes (8-byte discriminator + 32-byte pubkey), got ${decoded.data.length}`,
    );
  }
  const actualPayoutBytes = decoded.data.subarray(8, 40);
  const expectedPayoutBytes = Buffer.from(expectedPayoutPubkey.toBytes());
  if (!actualPayoutBytes.equals(expectedPayoutBytes)) {
    throw txMismatch(
      operation,
      `set_payout_to destination pubkey ${new PublicKey(actualPayoutBytes).toBase58()} differs from expected.payoutTo ${payoutB58}`,
    );
  }
}

function verifyInstructionAuthorityAndShape(
  decoded: DecodedShillbotIx,
  inspected: TransactionInspection,
  expected: VerifyBuiltTransactionExpectation,
  operation: string,
): void {
  const { spec, ix, data } = decoded;
  if (ix.accounts.length < spec.minAccounts) {
    throw txMismatch(
      operation,
      `Instruction ${spec.name} requires at least ${spec.minAccounts} accounts, got ${ix.accounts.length}`,
    );
  }
  if (spec.exactAccounts !== undefined && ix.accounts.length !== spec.exactAccounts) {
    throw txMismatch(
      operation,
      `Instruction ${spec.name} requires exact ${spec.exactAccounts} accounts, got ${ix.accounts.length}`,
    );
  }

  const expectedFeePayer = expected.feePayer ?? expected.sponsor ?? expected.wallet;
  if (!spec.allowsSponsoredFeePayer && inspected.fee_payer !== expectedFeePayer) {
    throw txMismatch(
      operation,
      `Instruction ${spec.name} requires fee_payer ${expectedFeePayer}, got ${inspected.fee_payer}`,
    );
  }

  if (typeof spec.authoritySlot === "number") {
    const slotAccount = ix.accounts[spec.authoritySlot];
    if (slotAccount !== expected.wallet) {
      throw txMismatch(
        operation,
        `Instruction ${spec.name} authority slot [${spec.authoritySlot}] is ${slotAccount}, expected ${expected.wallet}`,
      );
    }
    if (!inspected.signers.includes(slotAccount)) {
      throw txMismatch(
        operation,
        `Instruction ${spec.name} authority slot [${spec.authoritySlot}] (${slotAccount}) is not a required signer`,
      );
    }
    // Session instructions require payer at [5] (claim_task_session) to also be a signer
    if (spec.name === "claim_task_session" && !inspected.signers.includes(ix.accounts[5])) {
      throw txMismatch(
        operation,
        `Instruction claim_task_session payer slot [5] (${ix.accounts[5]}) is not a required signer`,
      );
    }
  } else if (spec.authoritySlot === "fee_payer") {
    if (inspected.fee_payer !== expectedFeePayer) {
      throw txMismatch(
        operation,
        `Instruction ${spec.name} requires fee_payer ${expectedFeePayer}, got ${inspected.fee_payer}`,
      );
    }
  } else if (spec.authoritySlot === "verify_task") {
    if (data.length !== 48) {
      throw txMismatch(
        operation,
        `Instruction verify_task data must be 48 bytes, got ${data.length}`,
      );
    }
    if (expected.oracleAuthority && ix.accounts[2] !== expected.oracleAuthority) {
      throw txMismatch(
        operation,
        `verify_task switchboard_feed [2] (${ix.accounts[2]}) differs from expected ${expected.oracleAuthority}`,
      );
    }
    if (inspected.fee_payer !== expectedFeePayer) {
      throw txMismatch(
        operation,
        `Instruction verify_task requires fee_payer ${expectedFeePayer}, got ${inspected.fee_payer}`,
      );
    }
  } else if (spec.authoritySlot === "verify_task_attested") {
    if (data.length !== 48) {
      throw txMismatch(
        operation,
        `Instruction verify_task_attested data must be 48 bytes, got ${data.length}`,
      );
    }
    const attester = ix.accounts[2];
    if (!inspected.signers.includes(attester)) {
      throw txMismatch(
        operation,
        `Instruction verify_task_attested attester slot [2] (${attester}) must be a required signer`,
      );
    }
    if (expected.oracleAuthority && attester !== expected.oracleAuthority) {
      throw txMismatch(
        operation,
        `Instruction verify_task_attested attester slot [2] (${attester}) differs from expected oracleAuthority ${expected.oracleAuthority}`,
      );
    }
    const walletIsAttester = attester === expected.wallet;
    const walletIsExpectedFeePayer =
      inspected.fee_payer === expectedFeePayer && expected.wallet === expectedFeePayer;
    if (!walletIsAttester && !walletIsExpectedFeePayer) {
      throw txMismatch(
        operation,
        `Instruction verify_task_attested requires wallet ${expected.wallet} to occupy attester slot [2] or fee_payer`,
      );
    }
  }
}

/**
 * Strict fail-closed local validator for built or server-prepared Shillbot Solana transactions.
 *
 * Enforces:
 * - Allowed program IDs (Shillbot + ComputeBudget, Switchboard + Secp256k1 on `verify` / score-0 reopen,
 *   and `extension-credit` `route_and_recoup` strictly on `finalize`).
 * - Full loop validation of all `ComputeBudget` instructions (no early return on `RequestHeapFrame`),
 *   capping `SetComputeUnitPrice <= MAX_COMPUTE_UNIT_PRICE_MICROLAMPORTS` (`100_000` micro-lamports/CU).
 * - Exact Anchor account-index positions for `task` PDA and authority signer slots per discriminator.
 * - Explicit `set_payout_to` opt-in guard: rejects any `set_payout_to` instruction unless
 *   `expected.payoutTo` is explicitly provided AND matches `ix.data[8..40]` and `accounts == [task, wallet]`.
 */
export function verifyBuiltTransaction(
  encoded: string,
  expected: VerifyBuiltTransactionExpectation,
  operation = "shillbot.verifyBuiltTransaction",
): TransactionInspection {
  if (expected.network !== "mainnet" && expected.network !== "devnet") {
    throw invalidArg(operation, "Unsupported network");
  }
  if (!expected.action && !expected.instructionName) {
    throw invalidArg(operation, "Either action or instructionName is required");
  }
  parseValidatedPubkey(expected.wallet, "wallet", operation);
  if (expected.taskPda !== undefined) {
    parseValidatedPubkey(expected.taskPda, "taskPda", operation);
  }
  if (expected.feePayer !== undefined) {
    parseValidatedPubkey(expected.feePayer, "feePayer", operation);
  }
  if (expected.sponsor !== undefined) {
    parseValidatedPubkey(expected.sponsor, "sponsor", operation);
  }
  if (
    expected.feePayer !== undefined &&
    expected.sponsor !== undefined &&
    expected.feePayer !== expected.sponsor
  ) {
    throw invalidArg(
      operation,
      "expected.feePayer and expected.sponsor must match when both are provided",
    );
  }
  const explicitFeePayer = expected.feePayer ?? expected.sponsor;
  if (expected.oracleAuthority !== undefined) {
    parseValidatedPubkey(expected.oracleAuthority, "oracleAuthority", operation);
  }
  const expectedPayoutPubkey =
    expected.payoutTo !== undefined
      ? parseValidatedPubkey(expected.payoutTo, "payoutTo", operation)
      : undefined;
  if (
    expectedPayoutPubkey &&
    expectedPayoutPubkey.toBase58() === PublicKey.default.toBase58()
  ) {
    throw invalidArg(operation, "expected.payoutTo cannot be the default zero pubkey");
  }

  let inspected: TransactionInspection;
  try {
    inspected = inspectTransaction(encoded);
  } catch (err) {
    throw txMismatch(
      operation,
      err instanceof Error ? err.message : `Malformed transaction: ${String(err)}`,
    );
  }

  if (!inspected.signers.includes(expected.wallet)) {
    throw txMismatch(operation, "Wallet is not a required signer");
  }
  if (explicitFeePayer && inspected.fee_payer !== explicitFeePayer) {
    throw txMismatch(operation, "Fee payer differs from expectation");
  }

  const isVerifyContext =
    expected.action === "verify" ||
    expected.instructionName === "verify_task" ||
    expected.instructionName === "verify_task_attested";
  const isFinalizeContext =
    expected.action === "finalize" || expected.instructionName === "finalize_task";
  const shillbotId = SHILLBOT_PROGRAM_ID.toBase58();
  const extensionCreditId = EXTENSION_CREDIT_PROGRAM_ID.toBase58();
  const alwaysAllowed = new Set<string>([
    shillbotId,
    ComputeBudgetProgram.programId.toBase58(),
    ...(isVerifyContext
      ? [SWITCHBOARD_PROGRAM_IDS[expected.network], Secp256k1Program.programId.toBase58()]
      : []),
    ...(isFinalizeContext ? [extensionCreditId] : []),
    ...(expected.allowedProgramIds ?? []),
  ]);
  const unrelated = inspected.instructions.find(
    (instruction) => !alwaysAllowed.has(instruction.program_id),
  );
  if (unrelated) {
    throw txMismatch(operation, `Unexpected program ${unrelated.program_id}`);
  }

  validateComputeBudgetInstructions(
    inspected.instructions,
    expected.maxComputeUnitPriceMicroLamports ?? MAX_COMPUTE_UNIT_PRICE_MICROLAMPORTS,
    operation,
  );

  const decodedShillbotIxs: DecodedShillbotIx[] = inspected.instructions.flatMap((ix, txIndex) => {
    if (ix.program_id !== shillbotId) return [];
    const data = Buffer.from(ix.data_base64, "base64");
    if (data.length < 8) {
      throw txMismatch(operation, "Shillbot instruction is shorter than 8-byte discriminator");
    }
    const spec = DISCRIMINATOR_TO_SPEC.get(data.subarray(0, 8).toString("hex"));
    if (!spec) {
      throw txMismatch(operation, "Unknown Shillbot instruction discriminator");
    }
    return [{ ix, txIndex, data, spec }];
  });

  const targetNames = expected.instructionName
    ? [expected.instructionName]
    : allowedInstructionNamesForAction(expected.action!);

  const lifecycleMatches = decodedShillbotIxs.filter((d) => targetNames.includes(d.spec.name));
  if (lifecycleMatches.length !== 1) {
    throw txMismatch(operation, "Expected exactly one matching Shillbot lifecycle instruction");
  }
  const primary = lifecycleMatches[0];
  const companions = decodedShillbotIxs.filter((d) => d !== primary);

  // Check task PDA before or alongside authority check
  if (primary.spec.taskSlot !== null) {
    if (primary.ix.accounts.length <= primary.spec.taskSlot) {
      throw txMismatch(operation, `Instruction ${primary.spec.name} is missing task account`);
    }
    const actualTaskPda = primary.ix.accounts[primary.spec.taskSlot];
    if (expected.taskPda && actualTaskPda !== expected.taskPda) {
      throw txMismatch(operation, "Task PDA differs from expectation");
    }
  }

  verifyInstructionAuthorityAndShape(primary, inspected, expected, operation);

  // Validate optional `extension-credit::route_and_recoup` companion on `finalize`
  const creditIxs = inspected.instructions
    .map((ix, txIndex) => ({ ix, txIndex }))
    .filter((entry) => entry.ix.program_id === extensionCreditId);
  if (creditIxs.length > 0) {
    if (!isFinalizeContext || primary.spec.name !== "finalize_task") {
      throw txMismatch(operation, "extension-credit instruction only permitted on finalize_task");
    }
    if (creditIxs.length !== 1) {
      throw txMismatch(operation, "At most one extension-credit instruction is permitted");
    }
    const { ix: creditIx, txIndex: creditTxIndex } = creditIxs[0];
    if (creditTxIndex <= primary.txIndex) {
      throw txMismatch(operation, "route_and_recoup must execute after finalize_task");
    }
    const creditData = Buffer.from(creditIx.data_base64, "base64");
    if (creditData.length !== 8 || !creditData.equals(routeAndRecoupDiscriminator())) {
      throw txMismatch(operation, "Unsupported or malformed extension-credit instruction");
    }
    if (creditIx.accounts.length !== 3) {
      throw txMismatch(
        operation,
        `route_and_recoup requires exact 3 accounts [advance, backer, recipient], got ${creditIx.accounts.length}`,
      );
    }
    const [advanceAcct, backerAcct, recipientAcct] = creditIx.accounts;
    if (advanceAcct !== primary.ix.accounts[2]) {
      throw txMismatch(
        operation,
        `route_and_recoup advance account (${advanceAcct}) must match finalize_task payout slot [2] (${primary.ix.accounts[2]})`,
      );
    }
    if (expectedPayoutPubkey && advanceAcct !== expectedPayoutPubkey.toBase58()) {
      throw txMismatch(
        operation,
        `route_and_recoup advance account (${advanceAcct}) differs from expected.payoutTo (${expectedPayoutPubkey.toBase58()})`,
      );
    }
    if (recipientAcct !== expected.wallet) {
      throw txMismatch(
        operation,
        `route_and_recoup recipient (${recipientAcct}) must equal caller wallet (${expected.wallet})`,
      );
    }
    const derivedAdvance = advancePda(backerAcct, recipientAcct).toBase58();
    if (advanceAcct !== derivedAdvance) {
      throw txMismatch(
        operation,
        `route_and_recoup advance account (${advanceAcct}) does not match canonical Advance PDA (${derivedAdvance})`,
      );
    }
  }

  // Instruction-specific payload validation
  if (primary.spec.name === "create_task") {
    if (primary.data.length < 24) {
      throw txMismatch(operation, "Create instruction is truncated");
    }
    if (expected.escrowLamports !== undefined) {
      const view = new DataView(primary.data.buffer, primary.data.byteOffset + 16, 8);
      const amount = view.getBigUint64(0, true);
      if (amount !== expected.escrowLamports) {
        throw txMismatch(
          operation,
          `Escrow amount ${amount} differs from expected ${expected.escrowLamports}`,
        );
      }
    }
  } else if (
    primary.spec.name === "submit_work" ||
    primary.spec.name === "submit_work_session"
  ) {
    if (primary.data.length < 12) {
      throw txMismatch(operation, "Submit instruction is truncated");
    }
    const contentLen = primary.data.readUInt32LE(8);
    if (primary.data.length !== 12 + contentLen) {
      throw txMismatch(operation, "Submit instruction content_id length mismatch");
    }
    if (expected.contentId !== undefined) {
      const actualContent = primary.data.subarray(12, 12 + contentLen).toString("utf8");
      if (actualContent !== expected.contentId) {
        throw txMismatch(operation, "Submit instruction content_id differs from expectation");
      }
    }
  } else if (primary.spec.name === "set_payout_to") {
    validateSetPayoutToInstruction(
      primary,
      expected.wallet,
      primary.ix.accounts[0],
      expectedPayoutPubkey,
      operation,
    );
  } else if (primary.spec.name === "finalize_task" && expectedPayoutPubkey) {
    if (primary.ix.accounts[2] !== expectedPayoutPubkey.toBase58()) {
      throw txMismatch(
        operation,
        `finalize_task payout recipient slot [2] (${primary.ix.accounts[2]}) differs from expected.payoutTo ${expectedPayoutPubkey.toBase58()}`,
      );
    }
  } else if (primary.spec.name === "expire_task") {
    if (primary.data.length !== 8) {
      throw txMismatch(
        operation,
        `expire_task instruction data must be exact 8-byte discriminator, got ${primary.data.length}`,
      );
    }
    if (primary.ix.accounts.length !== 3 && primary.ix.accounts.length !== 4) {
      throw txMismatch(
        operation,
        `expire_task requires 3 accounts (or 4 when expiring a Claimed task), got ${primary.ix.accounts.length}`,
      );
    }
    if (expected.expireTaskState !== undefined) {
      if (
        expected.expireTaskState === "open" ||
        expected.expireTaskState === "submitted" ||
        expected.expireTaskState === "approved"
      ) {
        if (primary.ix.accounts.length !== 3) {
          throw txMismatch(
            operation,
            `expire_task on ${expected.expireTaskState} task requires exact 3 accounts [task, global_state, client], got ${primary.ix.accounts.length}`,
          );
        }
      } else if (expected.expireTaskState === "claimed") {
        if (primary.ix.accounts.length !== 4) {
          throw txMismatch(
            operation,
            `expire_task on claimed task requires exact 4 accounts [task, global_state, client, agent_state], got ${primary.ix.accounts.length}`,
          );
        }
      } else {
        throw invalidArg(
          operation,
          `Unsupported expireTaskState: ${String(expected.expireTaskState)}`,
        );
      }
    }
  }

  // Validate companion Shillbot instructions (strictly fail-closed)
  const validateClaimCompanion = (claimIx: DecodedShillbotIx): boolean => {
    if (primary.spec.name === "submit_work") {
      return (
        claimIx.spec.name === "claim_task" &&
        claimIx.ix.accounts.length >= SHILLBOT_INSTRUCTION_SPECS.claim_task.minAccounts &&
        claimIx.ix.accounts[0] === primary.ix.accounts[0] &&
        claimIx.ix.accounts[3] === expected.wallet
      );
    }
    if (primary.spec.name === "submit_work_session") {
      return (
        claimIx.spec.name === "claim_task_session" &&
        claimIx.ix.accounts.length >= SHILLBOT_INSTRUCTION_SPECS.claim_task_session.minAccounts &&
        claimIx.ix.accounts[0] === primary.ix.accounts[0] &&
        claimIx.ix.accounts[3] === primary.ix.accounts[3] &&
        claimIx.ix.accounts[4] === expected.wallet &&
        inspected.signers.includes(claimIx.ix.accounts[5])
      );
    }
    return false;
  };

  const isClaimLifecycle =
    expected.action === "claim" ||
    primary.spec.name === "claim_task" ||
    primary.spec.name === "claim_task_session";
  const isSubmitLifecycle =
    expected.action === "submit" ||
    primary.spec.name === "submit_work" ||
    primary.spec.name === "submit_work_session";
  const isVerifyLifecycle =
    expected.action === "verify" ||
    primary.spec.name === "verify_task" ||
    primary.spec.name === "verify_task_attested";

  const isSponsoredClaimOrSubmit =
    (isClaimLifecycle || isSubmitLifecycle) && inspected.fee_payer !== expected.wallet;
  const derivedAdvancePda = isSponsoredClaimOrSubmit
    ? advancePda(inspected.fee_payer, expected.wallet)
    : undefined;
  const requiresAdvancePayout = isSponsoredClaimOrSubmit && explicitFeePayer === undefined;

  if (
    inspected.fee_payer !== expected.wallet &&
    explicitFeePayer === undefined &&
    !isSponsoredClaimOrSubmit &&
    !(primary.spec.name === "verify_task_attested" && primary.ix.accounts[2] === expected.wallet)
  ) {
    throw txMismatch(
      operation,
      `Fee payer ${inspected.fee_payer} differs from expected wallet ${expected.wallet}`,
    );
  }

  if (
    requiresAdvancePayout &&
    expectedPayoutPubkey &&
    !expectedPayoutPubkey.equals(derivedAdvancePda!)
  ) {
    throw txMismatch(
      operation,
      `set_payout_to destination pubkey ${expectedPayoutPubkey.toBase58()} differs from deterministic Advance PDA ${derivedAdvancePda!.toBase58()}`,
    );
  }

  const authorizedPayoutPubkey = requiresAdvancePayout
    ? derivedAdvancePda
    : (expectedPayoutPubkey ?? derivedAdvancePda);

  if (companions.length === 0) {
    if (
      expectedPayoutPubkey &&
      primary.spec.name !== "set_payout_to" &&
      primary.spec.name !== "finalize_task"
    ) {
      throw txMismatch(
        operation,
        "Expected set_payout_to instruction matching expected.payoutTo, but none was present",
      );
    }
    return inspected;
  }

  if (
    isVerifyLifecycle &&
    companions.length === 1 &&
    companions[0].spec.name === "set_payout_to"
  ) {
    if (companions[0].txIndex >= primary.txIndex) {
      throw txMismatch(operation, "set_payout_to must precede verify instruction");
    }
    validateSetPayoutToInstruction(
      companions[0],
      expected.wallet,
      primary.ix.accounts[0],
      expectedPayoutPubkey,
      operation,
    );
    return inspected;
  }

  // M2 Open-Competition 1-tx verify bundle:
  // `[ComputeBudget?, claim_task(sysvar), set_payout_to?, submit_work, verify_task_attested(MAX_SCORE)]`
  const isM2VerifyNoPayout =
    isVerifyLifecycle &&
    primary.spec.name === "verify_task_attested" &&
    companions.length === 2 &&
    companions[0].spec.name === "claim_task" &&
    companions[1].spec.name === "submit_work";
  const isM2VerifyWithPayout =
    isVerifyLifecycle &&
    primary.spec.name === "verify_task_attested" &&
    companions.length === 3 &&
    companions[0].spec.name === "claim_task" &&
    companions[1].spec.name === "set_payout_to" &&
    companions[2].spec.name === "submit_work";

  if (isM2VerifyNoPayout || isM2VerifyWithPayout) {
    const disallowedInM2 = new Set<string>([
      SWITCHBOARD_PROGRAM_IDS[expected.network],
      Secp256k1Program.programId.toBase58(),
    ]);
    for (const extraAllowed of expected.allowedProgramIds ?? []) {
      disallowedInM2.delete(extraAllowed);
    }
    const unexpectedProgramInM2 = inspected.instructions.find((ix) =>
      disallowedInM2.has(ix.program_id),
    );
    if (unexpectedProgramInM2) {
      throw txMismatch(
        operation,
        `Unexpected program ${unexpectedProgramInM2.program_id} in M2 verify bundle`,
      );
    }
    if (inspected.fee_payer !== expected.wallet) {
      throw txMismatch(
        operation,
        `M2 verify bundle requires fee_payer ${expected.wallet}, got ${inspected.fee_payer}`,
      );
    }
    if (primary.ix.accounts[2] === inspected.fee_payer) {
      throw txMismatch(
        operation,
        "M2 verify bundle attester cannot be fee_payer",
      );
    }
    if (primary.ix.accounts.length !== 3) {
      throw txMismatch(
        operation,
        `M2 verify_task_attested must carry exact 3 accounts [task, global_state, attester], got ${primary.ix.accounts.length}`,
      );
    }
    const scoreView = new DataView(
      primary.data.buffer,
      primary.data.byteOffset + 8,
      8,
    );
    const verifyScore = scoreView.getBigUint64(0, true);
    if (verifyScore !== MAX_VERIFY_SCORE) {
      throw txMismatch(
        operation,
        `M2 verify bundle requires MAX_SCORE (${MAX_VERIFY_SCORE}), got ${verifyScore}`,
      );
    }
    if (!primary.data.subarray(16, 48).some((b) => b !== 0)) {
      throw txMismatch(
        operation,
        "M2 verify_task_attested verification_hash must be non-zero",
      );
    }

    const lifecycleTaskPda = primary.ix.accounts[0];
    const claimIx = companions[0];
    const payoutIx = isM2VerifyWithPayout ? companions[1] : undefined;
    const submitIx = isM2VerifyWithPayout ? companions[2] : companions[1];

    if (payoutIx) {
      if (
        payoutIx.txIndex !== claimIx.txIndex + 1 ||
        submitIx.txIndex !== payoutIx.txIndex + 1 ||
        primary.txIndex !== submitIx.txIndex + 1
      ) {
        throw txMismatch(
          operation,
          "M2 verify bundle instructions must execute in strict order claim_task < set_payout_to < submit_work < verify_task_attested",
        );
      }
    } else {
      if (expectedPayoutPubkey) {
        throw txMismatch(
          operation,
          "Expected set_payout_to instruction matching expected.payoutTo, but none was present",
        );
      }
      if (
        submitIx.txIndex !== claimIx.txIndex + 1 ||
        primary.txIndex !== submitIx.txIndex + 1
      ) {
        throw txMismatch(
          operation,
          "M2 verify bundle instructions must execute in strict order claim_task < submit_work < verify_task_attested",
        );
      }
    }

    // Validate companion claim_task
    const validClaimAccounts =
      claimIx.ix.accounts.length === SHILLBOT_INSTRUCTION_SPECS.claim_task.minAccounts ||
      (claimIx.ix.accounts.length === 6 &&
        claimIx.ix.accounts[5] === SYSVAR_INSTRUCTIONS_PUBKEY.toBase58());
    if (claimIx.data.length !== 8 || !validClaimAccounts) {
      throw txMismatch(operation, "Malformed claim_task companion in M2 verify bundle");
    }
    if (claimIx.ix.accounts[0] !== lifecycleTaskPda) {
      throw txMismatch(
        operation,
        "M2 verify bundle claim_task account [0] does not match expected task PDA",
      );
    }
    if (
      claimIx.ix.accounts[3] !== expected.wallet ||
      !inspected.signers.includes(claimIx.ix.accounts[3])
    ) {
      throw txMismatch(
        operation,
        "M2 verify bundle claim_task authority slot [3] does not match expected wallet signer",
      );
    }

    // Validate optional companion set_payout_to
    if (payoutIx) {
      validateSetPayoutToInstruction(
        payoutIx,
        expected.wallet,
        lifecycleTaskPda,
        expectedPayoutPubkey,
        operation,
      );
    }

    // Validate companion submit_work
    if (submitIx.ix.accounts.length !== SHILLBOT_INSTRUCTION_SPECS.submit_work.minAccounts) {
      throw txMismatch(
        operation,
        `M2 verify bundle submit_work must carry exact 4 accounts, got ${submitIx.ix.accounts.length}`,
      );
    }
    if (submitIx.ix.accounts[0] !== lifecycleTaskPda) {
      throw txMismatch(
        operation,
        "M2 verify bundle submit_work account [0] does not match expected task PDA",
      );
    }
    if (
      submitIx.ix.accounts[3] !== expected.wallet ||
      !inspected.signers.includes(submitIx.ix.accounts[3])
    ) {
      throw txMismatch(
        operation,
        "M2 verify bundle submit_work authority slot [3] does not match expected wallet signer",
      );
    }
    if (submitIx.data.length < 12) {
      throw txMismatch(operation, "M2 verify bundle submit_work instruction is truncated");
    }
    const submitContentLen = submitIx.data.readUInt32LE(8);
    if (submitIx.data.length !== 12 + submitContentLen) {
      throw txMismatch(operation, "M2 verify bundle submit_work content_id length mismatch");
    }
    if (expected.contentId !== undefined) {
      const actualContent = submitIx.data.subarray(12, 12 + submitContentLen).toString("utf8");
      if (actualContent !== expected.contentId) {
        throw txMismatch(
          operation,
          "M2 verify bundle submit_work content_id differs from expectation",
        );
      }
    }

    return inspected;
  }

  if (
    (isClaimLifecycle || isSubmitLifecycle) &&
    companions.length === 1 &&
    companions[0].spec.name === "set_payout_to"
  ) {
    if (isClaimLifecycle && primary.spec.name !== "claim_task") {
      throw txMismatch(operation, "set_payout_to companion requires direct agent claim_task");
    }
    if (isSubmitLifecycle && primary.spec.name !== "submit_work") {
      throw txMismatch(operation, "set_payout_to companion requires direct agent submit_work");
    }
    if (isClaimLifecycle && companions[0].txIndex <= primary.txIndex) {
      throw txMismatch(operation, "payout route must follow claim");
    }
    validateSetPayoutToInstruction(
      companions[0],
      expected.wallet,
      primary.ix.accounts[0],
      authorizedPayoutPubkey,
      operation,
    );
    return inspected;
  }

  // Optional leading score-0 reopen (`verify_task` or `verify_task_attested` with score == 0)
  // before `[claim_task, (set_payout_to?), submit_work]` (1d / Q4).
  let submitCompanions = companions;
  if (
    isSubmitLifecycle &&
    (companions[0].spec.name === "verify_task" ||
      companions[0].spec.name === "verify_task_attested")
  ) {
    const reopenIx = companions[0];
    if (reopenIx.data.length !== 48 || reopenIx.ix.accounts.length < 3) {
      throw txMismatch(operation, "Malformed score-0 reopen verify companion");
    }
    if (reopenIx.ix.accounts[0] !== primary.ix.accounts[0]) {
      throw txMismatch(operation, "Score-0 reopen verify companion targets a different task PDA");
    }
    const scoreView = new DataView(
      reopenIx.data.buffer,
      reopenIx.data.byteOffset + 8,
      8,
    );
    if (scoreView.getBigUint64(0, true) !== 0n) {
      throw txMismatch(
        operation,
        "Reopen verify companion in submit bundle must have composite_score === 0",
      );
    }
    if (reopenIx.spec.name === "verify_task_attested") {
      const attester = reopenIx.ix.accounts[2];
      if (!inspected.signers.includes(attester)) {
        throw txMismatch(operation, "verify_task_attested reopen attester must be a signer");
      }
      if (expected.oracleAuthority && attester !== expected.oracleAuthority) {
        throw txMismatch(operation, "verify_task_attested reopen attester mismatch");
      }
    } else if (expected.oracleAuthority && reopenIx.ix.accounts[2] !== expected.oracleAuthority) {
      throw txMismatch(operation, "verify_task reopen oracle mismatch");
    }
    submitCompanions = companions.slice(1);
    if (submitCompanions.length > 0 && reopenIx.txIndex >= submitCompanions[0].txIndex) {
      throw txMismatch(operation, "Score-0 reopen verify must precede claim_task");
    }
  }

  if (
    isSubmitLifecycle &&
    submitCompanions.length === 1 &&
    (submitCompanions[0].spec.name === "claim_task" ||
      submitCompanions[0].spec.name === "claim_task_session")
  ) {
    if (expectedPayoutPubkey) {
      throw txMismatch(
        operation,
        "Expected set_payout_to instruction matching expected.payoutTo, but none was present",
      );
    }
    if (
      primary.txIndex !== submitCompanions[0].txIndex + 1 ||
      !validateClaimCompanion(submitCompanions[0])
    ) {
      throw txMismatch(operation, "Expected exactly one matching Shillbot lifecycle instruction");
    }
    return inspected;
  }

  if (
    isSubmitLifecycle &&
    submitCompanions.length === 2 &&
    submitCompanions[0].spec.name === "claim_task" &&
    submitCompanions[1].spec.name === "set_payout_to"
  ) {
    const isClaimPayoutSubmit =
      submitCompanions[1].txIndex === submitCompanions[0].txIndex + 1 &&
      primary.txIndex === submitCompanions[1].txIndex + 1;
    const isClaimSubmitPayout =
      primary.txIndex === submitCompanions[0].txIndex + 1 &&
      submitCompanions[1].txIndex === primary.txIndex + 1;
    if (
      primary.spec.name !== "submit_work" ||
      (!isClaimPayoutSubmit && !isClaimSubmitPayout) ||
      !validateClaimCompanion(submitCompanions[0])
    ) {
      throw txMismatch(operation, "Expected exactly one matching Shillbot lifecycle instruction");
    }
    validateSetPayoutToInstruction(
      submitCompanions[1],
      expected.wallet,
      primary.ix.accounts[0],
      authorizedPayoutPubkey,
      operation,
    );
    return inspected;
  }

  if (companions.some((c) => c.spec.name === "set_payout_to")) {
    throw txMismatch(
      operation,
      "Unexpected set_payout_to companion instruction in transaction",
    );
  }

  throw txMismatch(operation, "Expected exactly one matching Shillbot lifecycle instruction");
}
