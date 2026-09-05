import * as anchor from "@coral-xyz/anchor";
import { BN } from "@coral-xyz/anchor";
import { Buffer } from "buffer";
import { OracleJob } from "@switchboard-xyz/common/protos";
import {
  ComputeBudgetProgram,
  Connection,
  Keypair,
  PublicKey,
  Secp256k1Program,
  SystemProgram,
  TransactionInstruction,
  TransactionMessage,
  VersionedTransaction,
} from "@solana/web3.js";
import { buildInstruction, type Network, type VerifyBuild } from "./index.js";

const ASSOCIATED_TOKEN_PROGRAM_ID = new PublicKey(
  "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"
);
const SPL_TOKEN_PROGRAM_ID = new PublicKey(
  "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
);
const SOL_NATIVE_MINT = new PublicKey(
  "So11111111111111111111111111111111111111112"
);
const SYSVAR_SLOT_HASHES = new PublicKey(
  "SysvarS1otHashes111111111111111111111111111"
);
const SYSVAR_INSTRUCTIONS = new PublicKey(
  "Sysvar1nstructions1111111111111111111111111"
);
const SWITCHBOARD_PROGRAMS: Record<Network, PublicKey> = {
  mainnet: new PublicKey("SBondMDrcV3K4kxZR1HNVT7osZxAHVHgYXL5Ze1oMUv"),
  devnet: new PublicKey("Aio4gaXjXzJNVLtzwtNVmSqGKpANtXhybbkhtAC94ji2"),
};

const CROSSBAR_URL = "https://crossbar.switchboard.xyz";
const SWITCHBOARD_GATEWAY_API_VERSION = "1.0.0";

type SwitchboardFeedData = {
  feedHash: Uint8Array;
  queue: PublicKey;
  maxVariance: BN;
  minResponses: number;
};

type SwitchboardOracleResponse = {
  eth_address: string;
  signature: string;
  checksum: string;
  recovery_id: number;
  oracle_pubkey: string;
};

type SwitchboardMedianResponse = {
  feed_hash: string;
  value: string | number;
};

type SwitchboardConsensusResponse = {
  oracle_responses?: SwitchboardOracleResponse[];
  median_responses: SwitchboardMedianResponse[];
  slot: string | number;
};

async function jsonResponse<T>(response: Response, description: string): Promise<T> {
  if (!response.ok) {
    throw new Error(`${description} failed with HTTP ${response.status}`);
  }
  return (await response.json()) as T;
}

function encodeOracleJob(job: unknown): string {
  const encoded = OracleJob.encodeDelimited(
    OracleJob.fromObject(job as Record<string, unknown>)
  ).finish();
  return Buffer.from(encoded).toString("base64");
}

async function fetchConsensus(
  network: Network,
  jobs: unknown[],
  maxVariance: number,
  minResponses: number,
  taskId: string
): Promise<SwitchboardConsensusResponse> {
  const gateways = await jsonResponse<string[]>(
    await fetch(`${CROSSBAR_URL}/gateways?network=${network}`),
    "Switchboard gateway discovery"
  );
  const gateway = gateways[0];
  if (!gateway) throw new Error(`no Switchboard gateways for ${network}`);
  return jsonResponse<SwitchboardConsensusResponse>(
    await fetch(`${gateway.replace(/\/$/, "")}/gateway/api/v1/fetch_signatures_consensus`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        api_version: SWITCHBOARD_GATEWAY_API_VERSION,
        recent_hash: "",
        signature_scheme: "Secp256k1",
        hash_scheme: "Sha256",
        feed_requests: [
          {
            jobs_b64_encoded: jobs.map(encodeOracleJob),
            max_variance: Math.floor(maxVariance * 1e9),
            min_responses: minResponses,
          },
        ],
        num_oracles: 1,
        use_timestamp: false,
        use_ed25519: false,
        variable_overrides: { TASK_ID: taskId },
      }),
    }),
    "Switchboard consensus request"
  );
}

export type SecpSignature = {
  ethAddress: Uint8Array;
  signature: Uint8Array;
  message: Uint8Array;
  recoveryId: number;
  oracleIdx: number;
};

function writeUInt16LE(value: number): Uint8Array {
  if (!Number.isInteger(value) || value < 0 || value > 0xffff) {
    throw new Error(`invalid secp256k1 offset: ${value}`);
  }
  const bytes = new Uint8Array(2);
  new DataView(bytes.buffer).setUint16(0, value, true);
  return bytes;
}

/** Build Solana's compact multi-signature secp256k1 verification instruction. */
export function buildSecp256k1Instruction(
  signatures: readonly SecpSignature[],
  instructionIndex: number
): TransactionInstruction {
  if (signatures.length === 0) {
    throw new Error("Switchboard returned no secp256k1 signatures");
  }
  if (!Number.isInteger(instructionIndex) || instructionIndex < 0 || instructionIndex > 0xff) {
    throw new Error("invalid secp256k1 instruction index");
  }
  const sorted = [...signatures].sort((a, b) => a.oracleIdx - b.oracleIdx);
  const message = sorted[0].message;
  if (sorted.some((signature) => !Buffer.from(signature.message).equals(Buffer.from(message)))) {
    throw new Error("all Switchboard signatures must share one message");
  }
  for (const signature of sorted) {
    if (signature.signature.length !== 64) throw new Error("invalid secp256k1 signature length");
    if (signature.ethAddress.length !== 20) throw new Error("invalid secp256k1 address length");
    if (!Number.isInteger(signature.recoveryId) || signature.recoveryId < 0 || signature.recoveryId > 0xff) {
      throw new Error("invalid secp256k1 recovery id");
    }
  }

  const offsetsSize = 1 + sorted.length * 11;
  const signatureBlockSize = 64 + 1 + 20;
  const messageOffset = offsetsSize + sorted.length * signatureBlockSize;
  const data = new Uint8Array(messageOffset + message.length);
  data[0] = sorted.length;
  let blockOffset = offsetsSize;
  sorted.forEach((signature, index) => {
    const offset = 1 + index * 11;
    data.set(writeUInt16LE(blockOffset), offset);
    data[offset + 2] = instructionIndex;
    data.set(writeUInt16LE(blockOffset + 65), offset + 3);
    data[offset + 5] = instructionIndex;
    data.set(writeUInt16LE(messageOffset), offset + 6);
    data.set(writeUInt16LE(message.length), offset + 8);
    data[offset + 10] = instructionIndex;
    data.set(signature.signature, blockOffset);
    data[blockOffset + 64] = signature.recoveryId;
    data.set(signature.ethAddress, blockOffset + 65);
    blockOffset += signatureBlockSize;
  });
  data.set(message, messageOffset);
  return new TransactionInstruction({
    programId: Secp256k1Program.programId,
    keys: [],
    data: Buffer.from(data),
  });
}

function associatedTokenAddress(mint: PublicKey, owner: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync(
    [owner.toBuffer(), SPL_TOKEN_PROGRAM_ID.toBuffer(), mint.toBuffer()],
    ASSOCIATED_TOKEN_PROGRAM_ID
  )[0];
}

export interface SwitchboardVerifyRequest {
  connection: Connection;
  taskId: string;
  payer: PublicKey;
  taskPda: PublicKey;
  feed: PublicKey;
  compositeScore: bigint;
  verificationHash: Uint8Array;
  network: Network;
}

/**
 * Fetch Switchboard consensus with the task-id variable override and build the
 * exact v0 bundle: bounded compute budget, secp256k1 proof, feed update, then
 * Shillbot verify. The returned transaction is unsigned.
 */
export async function buildSwitchboardCrankAndVerify(
  request: SwitchboardVerifyRequest
): Promise<VersionedTransaction> {
  const switchboardProgramId = SWITCHBOARD_PROGRAMS[request.network];
  const dummy = Keypair.generate();
  const provider = new anchor.AnchorProvider(
    request.connection,
    new anchor.Wallet(dummy),
    { commitment: "confirmed" }
  );
  const idl = await anchor.Program.fetchIdl(switchboardProgramId, provider);
  if (!idl) throw new Error("failed to fetch Switchboard IDL");
  const program = new anchor.Program(idl, provider);
  const switchboardAccounts = program.account as unknown as {
    pullFeedAccountData: { fetch(pubkey: PublicKey): Promise<unknown> };
  };
  const feedData = (await switchboardAccounts.pullFeedAccountData.fetch(
    request.feed
  )) as SwitchboardFeedData;
  const feedHashHex = Buffer.from(feedData.feedHash).toString("hex");
  const crossbar = (await fetch(
    `https://crossbar.switchboard.xyz/fetch/${feedHashHex}`
  ).then((response) => response.json())) as { jobs?: unknown[] };
  const jobs = crossbar.jobs ?? [];
  if (jobs.length === 0)
    throw new Error(`no Switchboard jobs for feed ${feedHashHex}`);

  const response = await fetchConsensus(
    request.network,
    jobs,
    feedData.maxVariance.toNumber() / 1e9,
    feedData.minResponses,
    request.taskId
  );
  if (!response.oracle_responses?.length) {
    throw new Error("Switchboard gateway returned no oracle responses");
  }
  const compute: TransactionInstruction[] = [
    ComputeBudgetProgram.setComputeUnitLimit({ units: 400_000 }),
    ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 10_000 }),
  ];
  const secp = buildSecp256k1Instruction(
    response.oracle_responses.map((oracle, oracleIdx) => ({
      ethAddress: Buffer.from(oracle.eth_address, "hex"),
      signature: Buffer.from(oracle.signature, "base64"),
      message: Buffer.from(oracle.checksum, "base64"),
      recoveryId: oracle.recovery_id,
      oracleIdx,
    })),
    compute.length
  );
  const queuePubkey = feedData.queue;
  const oraclePubkeys = response.oracle_responses.map(
    (oracle) => new PublicKey(Buffer.from(oracle.oracle_pubkey, "hex"))
  );
  const oracleStats = oraclePubkeys.map(
    (oracle: PublicKey) =>
      PublicKey.findProgramAddressSync(
        [Buffer.from("OracleStats"), oracle.toBuffer()],
        switchboardProgramId
      )[0]
  );
  const feeds = response.median_responses.map((median) =>
    median.feed_hash === feedHashHex ? request.feed : PublicKey.default
  );
  const submit = program.instruction.pullFeedSubmitResponseConsensus(
    {
      slot: new BN(response.slot),
      values: response.median_responses.map(
        (median: any) => new BN(median.value)
      ),
    },
    {
      accounts: {
        queue: queuePubkey,
        programState: PublicKey.findProgramAddressSync(
          [Buffer.from("STATE")],
          switchboardProgramId
        )[0],
        recentSlothashes: SYSVAR_SLOT_HASHES,
        payer: request.payer,
        systemProgram: SystemProgram.programId,
        rewardVault: associatedTokenAddress(SOL_NATIVE_MINT, queuePubkey),
        tokenProgram: SPL_TOKEN_PROGRAM_ID,
        tokenMint: SOL_NATIVE_MINT,
        ixSysvar: SYSVAR_INSTRUCTIONS,
      },
      remainingAccounts: [
        ...feeds.map((pubkey: PublicKey) => ({
          pubkey,
          isSigner: false,
          isWritable: true,
        })),
        ...oraclePubkeys.map((pubkey: PublicKey) => ({
          pubkey,
          isSigner: false,
          isWritable: false,
        })),
        ...oracleStats.map((pubkey: PublicKey) => ({
          pubkey,
          isSigner: false,
          isWritable: true,
        })),
      ],
    }
  ) as TransactionInstruction;
  const verify = buildInstruction({
    action: "verify",
    wallet: request.payer.toBase58(),
    network: request.network,
    recentBlockhash: PublicKey.default.toBase58(),
    taskPda: request.taskPda.toBase58(),
    switchboardFeed: request.feed.toBase58(),
    compositeScore: request.compositeScore.toString(),
    verificationHash: Buffer.from(request.verificationHash).toString("hex"),
    crankInstructions: [],
  } satisfies VerifyBuild);
  const { blockhash } = await request.connection.getLatestBlockhash(
    "confirmed"
  );
  const message = new TransactionMessage({
    payerKey: request.payer,
    recentBlockhash: blockhash,
    instructions: [...compute, secp, submit, verify],
  }).compileToV0Message();
  return new VersionedTransaction(message);
}
