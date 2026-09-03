import * as anchor from "@coral-xyz/anchor";
import { BN } from "@coral-xyz/anchor";
import { PullFeed, Queue, State } from "@switchboard-xyz/on-demand";
// The upstream package does not re-export this utility from its public index.
// @ts-expect-error deep import intentionally pinned by package lock
import { Secp256k1InstructionUtils } from "@switchboard-xyz/on-demand/dist/esm/instruction-utils/secp256k1-instruction-utils.js";
import {
  ComputeBudgetProgram,
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
  TransactionMessage,
  VersionedTransaction,
  type TransactionInstruction,
} from "@solana/web3.js";
import { buildInstruction, type Network, type VerifyBuild } from "./index.js";

const ASSOCIATED_TOKEN_PROGRAM_ID = new PublicKey(
  "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL",
);
const SPL_TOKEN_PROGRAM_ID = new PublicKey(
  "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
);
const SOL_NATIVE_MINT = new PublicKey("So11111111111111111111111111111111111111112");
const SYSVAR_SLOT_HASHES = new PublicKey("SysvarS1otHashes111111111111111111111111111");
const SYSVAR_INSTRUCTIONS = new PublicKey("Sysvar1nstructions1111111111111111111111111");
const SWITCHBOARD_PROGRAMS: Record<Network, PublicKey> = {
  mainnet: new PublicKey("SBondMDrcV3K4kxZR1HNVT7osZxAHVHgYXL5Ze1oMUv"),
  devnet: new PublicKey("Aio4gaXjXzJNVLtzwtNVmSqGKpANtXhybbkhtAC94ji2"),
};

function associatedTokenAddress(mint: PublicKey, owner: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync(
    [owner.toBuffer(), SPL_TOKEN_PROGRAM_ID.toBuffer(), mint.toBuffer()],
    ASSOCIATED_TOKEN_PROGRAM_ID,
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
  request: SwitchboardVerifyRequest,
): Promise<VersionedTransaction> {
  const switchboardProgramId = SWITCHBOARD_PROGRAMS[request.network];
  const dummy = Keypair.generate();
  const provider = new anchor.AnchorProvider(
    request.connection,
    new anchor.Wallet(dummy),
    { commitment: "confirmed" },
  );
  const idl = await anchor.Program.fetchIdl(switchboardProgramId, provider);
  if (!idl) throw new Error("failed to fetch Switchboard IDL");
  const program = new anchor.Program(idl, provider);
  const feedAccount = new PullFeed(program as never, request.feed);
  const feedData = await feedAccount.loadData();
  const feedHashHex = Buffer.from(feedData.feedHash).toString("hex");
  const crossbar = (await fetch(
    `https://crossbar.switchboard.xyz/fetch/${feedHashHex}`,
  ).then((response) => response.json())) as { jobs?: unknown[] };
  const jobs = crossbar.jobs ?? [];
  if (jobs.length === 0) throw new Error(`no Switchboard jobs for feed ${feedHashHex}`);

  const queue = new Queue(program as never, feedData.queue);
  const response = await queue.fetchSignaturesConsensus({
    feedConfigs: [
      {
        maxVariance: feedData.maxVariance.toNumber() / 1e9,
        minResponses: feedData.minResponses,
        jobs,
      },
    ],
    numSignatures: 1,
    variableOverrides: { TASK_ID: request.taskId },
  } as never);
  if (!response.oracle_responses?.length) {
    throw new Error("Switchboard gateway returned no oracle responses");
  }
  const compute: TransactionInstruction[] = [
    ComputeBudgetProgram.setComputeUnitLimit({ units: 400_000 }),
    ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 10_000 }),
  ];
  const secp = Secp256k1InstructionUtils.buildSecp256k1Instruction(
    response.oracle_responses.map((oracle: any, oracleIdx: number) => ({
      ethAddress: Buffer.from(oracle.eth_address, "hex"),
      signature: Buffer.from(oracle.signature, "base64"),
      message: Buffer.from(oracle.checksum, "base64"),
      recoveryId: oracle.recovery_id,
      oracleIdx,
    })),
    compute.length,
  ) as TransactionInstruction;
  const queuePubkey = feedData.queue;
  const oraclePubkeys = response.oracle_responses.map(
    (oracle: any) => new PublicKey(Buffer.from(oracle.oracle_pubkey, "hex")),
  );
  const oracleStats = oraclePubkeys.map(
    (oracle: PublicKey) =>
      PublicKey.findProgramAddressSync(
        [Buffer.from("OracleStats"), oracle.toBuffer()],
        switchboardProgramId,
      )[0],
  );
  const feeds = response.median_responses.map((median: any) =>
    median.feed_hash === feedHashHex ? request.feed : PublicKey.default,
  );
  const submit = program.instruction.pullFeedSubmitResponseConsensus(
    {
      slot: new BN(response.slot),
      values: response.median_responses.map((median: any) => new BN(median.value)),
    },
    {
      accounts: {
        queue: queuePubkey,
        programState: State.keyFromSeed(program as never),
        recentSlothashes: SYSVAR_SLOT_HASHES,
        payer: request.payer,
        systemProgram: SystemProgram.programId,
        rewardVault: associatedTokenAddress(SOL_NATIVE_MINT, queuePubkey),
        tokenProgram: SPL_TOKEN_PROGRAM_ID,
        tokenMint: SOL_NATIVE_MINT,
        ixSysvar: SYSVAR_INSTRUCTIONS,
      },
      remainingAccounts: [
        ...feeds.map((pubkey: PublicKey) => ({ pubkey, isSigner: false, isWritable: true })),
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
    },
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
  const { blockhash } = await request.connection.getLatestBlockhash("confirmed");
  const message = new TransactionMessage({
    payerKey: request.payer,
    recentBlockhash: blockhash,
    instructions: [...compute, secp, submit, verify],
  }).compileToV0Message();
  return new VersionedTransaction(message);
}
