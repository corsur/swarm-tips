/**
 * Build an unsigned VersionedTransaction that bundles:
 *   1. Switchboard oracle feed crank (per-task OracleJob URL)
 *   2. verify_task instruction
 *
 * Outputs base64-encoded unsigned tx to stdout. No signing — the caller signs.
 *
 * Usage:
 *   npx tsx build-verify-tx.ts \
 *     --task-id <id> --payer <pubkey> --score <u64> --hash <hex> \
 *     --task-pda <pubkey> --feed <pubkey> --global-state <pubkey> \
 *     --rpc <url>
 *
 * Uses Queue.fetchSignaturesConsensus directly (not PullFeed.fetchUpdateIx)
 * because fetchUpdateIx has a bug where it drops variableOverrides before
 * the gateway call. We replicate the instruction building from the SDK.
 */
import { PullFeed, Queue, State, Oracle } from "@switchboard-xyz/on-demand";
// @ts-ignore — not re-exported from the main index
import { Secp256k1InstructionUtils } from "@switchboard-xyz/on-demand/dist/esm/instruction-utils/secp256k1-instruction-utils.js";
import {
  Connection,
  Keypair,
  PublicKey,
  TransactionMessage,
  VersionedTransaction,
  TransactionInstruction,
  SystemProgram,
} from "@solana/web3.js";
import * as anchor from "@coral-xyz/anchor";
import { BN } from "@coral-xyz/anchor";
// Inline ATA derivation (avoids @solana/spl-token dependency)
const ASSOCIATED_TOKEN_PROGRAM_ID = new PublicKey(
  "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"
);
function getAssociatedTokenAddressSync(
  mint: PublicKey,
  owner: PublicKey,
  allowOwnerOffCurve: boolean = false
): PublicKey {
  return PublicKey.findProgramAddressSync(
    [owner.toBuffer(), SPL_TOKEN_PROGRAM_ID.toBuffer(), mint.toBuffer()],
    ASSOCIATED_TOKEN_PROGRAM_ID
  )[0];
}

// ---------------------------------------------------------------------------
// Parse CLI args
// ---------------------------------------------------------------------------

function parseArgs(): Record<string, string> {
  const args: Record<string, string> = {};
  const argv = process.argv.slice(2);
  for (let i = 0; i < argv.length; i += 2) {
    const key = argv[i].replace(/^--/, "");
    args[key] = argv[i + 1];
  }
  return args;
}

const SHILLBOT_PROGRAM_ID = new PublicKey(
  "2tR37nqMpwdV4DVUHjzUmL1rH2DtkA8zrRA4EAhT7KMi"
);
const SB_PROGRAM_ID = new PublicKey(
  "SBondMDrcV3K4kxZR1HNVT7osZxAHVHgYXL5Ze1oMUv"
);
// SPL sysvars
const SYSVAR_SLOT_HASHES = new PublicKey(
  "SysvarS1otHashes111111111111111111111111111"
);
const SYSVAR_INSTRUCTIONS = new PublicKey(
  "Sysvar1nstructions1111111111111111111111111"
);
// SOL native mint
const SOL_NATIVE_MINT = new PublicKey(
  "So11111111111111111111111111111111111111112"
);
const SPL_TOKEN_PROGRAM_ID = new PublicKey(
  "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
);

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

async function main() {
  const args = parseArgs();
  const taskId = args["task-id"];
  const payer = new PublicKey(args["payer"]);
  const score = BigInt(args["score"]);
  const hashHex = args["hash"];
  const taskPda = new PublicKey(args["task-pda"]);
  const feedPubkey = new PublicKey(args["feed"]);
  const globalState = new PublicKey(args["global-state"]);
  const rpcUrl = args["rpc"];

  if (!taskId || !rpcUrl) {
    process.stderr.write(
      "required: --task-id, --payer, --score, --hash, --task-pda, --feed, --global-state, --rpc\n"
    );
    process.exit(1);
  }

  const connection = new Connection(rpcUrl, "confirmed");

  // Dummy wallet (we never sign — just need Anchor provider for SDK)
  const dummyKeypair = Keypair.generate();
  const wallet = new anchor.Wallet(dummyKeypair);
  const provider = new anchor.AnchorProvider(connection, wallet, {
    commitment: "confirmed",
  });
  anchor.setProvider(provider);

  const idl = await anchor.Program.fetchIdl(SB_PROGRAM_ID, provider);
  if (!idl) {
    process.stderr.write("Failed to fetch Switchboard IDL\n");
    process.exit(1);
  }
  const program = new anchor.Program(idl, provider);

  // 1. Build verify_task instruction
  const verificationHash = Buffer.from(hashHex, "hex");
  const verifyIx = buildVerifyTaskIx(
    taskPda,
    globalState,
    feedPubkey,
    score,
    verificationHash
  );

  // 2. Load feed data to get queue and feed hash.
  // Cast: @switchboard-xyz/on-demand pins @coral-xyz/anchor ^0.31, the rest of the
  // repo is on 0.32. Runtime behaves identically; the type mismatch is a
  // private-property nominal conflict only.
  const feedAccount = new PullFeed(program as any, feedPubkey);
  const feedData = await feedAccount.loadData();
  const queuePubkey = feedData.queue;
  const feedHashHex = Buffer.from(feedData.feedHash).toString("hex");

  // 3. Fetch jobs from crossbar (same as SDK does internally)
  const crossbarResp = await fetch(
    `https://crossbar.switchboard.xyz/fetch/${feedHashHex}`
  ).then((r) => r.json() as Promise<any>);
  const jobs: any[] = crossbarResp.jobs || [];

  if (jobs.length === 0) {
    process.stderr.write(
      `No jobs found on crossbar for feed hash ${feedHashHex}\n`
    );
    process.exit(1);
  }

  // 4. Call gateway directly with variableOverrides via Queue.fetchSignaturesConsensus
  //    This properly passes variableOverrides to the gateway (unlike fetchUpdateIx).
  const queueAccount = new Queue(program as any, queuePubkey);
  const response = await queueAccount.fetchSignaturesConsensus({
    feedConfigs: [
      {
        maxVariance: feedData.maxVariance.toNumber() / 1e9,
        minResponses: feedData.minResponses,
        jobs,
      },
    ],
    numSignatures: 1,
    variableOverrides: { TASK_ID: taskId },
  });

  if (!response.oracle_responses || response.oracle_responses.length === 0) {
    process.stderr.write("No oracle responses received from gateway\n");
    process.exit(1);
  }

  // Check for oracle errors
  for (const oracleResp of response.oracle_responses) {
    if (oracleResp.errors && oracleResp.errors.length > 0) {
      process.stderr.write(`Oracle errors: ${oracleResp.errors.join("; ")}\n`);
    }
  }

  // 5. Build secp256k1 verification instruction from oracle signatures
  //    (replicates what fetchUpdateManyIx does internally)
  const secpSignatures = response.oracle_responses.map(
    (oracleResponse: any, responseIdx: number) => ({
      ethAddress: Buffer.from(oracleResponse.eth_address, "hex"),
      signature: Buffer.from(oracleResponse.signature, "base64"),
      message: Buffer.from(oracleResponse.checksum, "base64"),
      recoveryId: oracleResponse.recovery_id,
      oracleIdx: responseIdx,
    })
  );

  if (secpSignatures.length === 0) {
    process.stderr.write("No valid oracle signatures\n");
    process.exit(1);
  }

  // Build Secp256k1 native instruction using SDK's implementation
  const secpIx = Secp256k1InstructionUtils.buildSecp256k1Instruction(
    secpSignatures,
    0
  );

  // 6. Build pullFeedSubmitResponseConsensus instruction
  const instructionData = {
    slot: new BN(response.slot),
    values: response.median_responses.map((mr: any) => new BN(mr.value)),
  };

  const programState = State.keyFromSeed(program as any);
  const rewardVault = getAssociatedTokenAddressSync(
    SOL_NATIVE_MINT,
    queuePubkey,
    true
  );

  const oraclePubkeys = response.oracle_responses.map(
    (r: any) => new PublicKey(Buffer.from(r.oracle_pubkey, "hex"))
  );
  const oracleStatsPubkeys = oraclePubkeys.map(
    (oracle: PublicKey) =>
      PublicKey.findProgramAddressSync(
        [Buffer.from("OracleStats"), oracle.toBuffer()],
        SB_PROGRAM_ID
      )[0]
  );

  // Match feed pubkeys from median_responses
  const feedPubkeys = response.median_responses.map((mr: any) => {
    if (mr.feed_hash === feedHashHex) return feedPubkey;
    return PublicKey.default;
  });

  const remainingAccounts = [
    ...feedPubkeys.map((pk: PublicKey) => ({
      pubkey: pk,
      isSigner: false,
      isWritable: true,
    })),
    ...oraclePubkeys.map((pk: PublicKey) => ({
      pubkey: pk,
      isSigner: false,
      isWritable: false,
    })),
    ...oracleStatsPubkeys.map((pk: PublicKey) => ({
      pubkey: pk,
      isSigner: false,
      isWritable: true,
    })),
  ];

  const submitResponseIx = program.instruction.pullFeedSubmitResponseConsensus(
    instructionData,
    {
      accounts: {
        queue: queuePubkey,
        programState,
        recentSlothashes: SYSVAR_SLOT_HASHES,
        payer,
        systemProgram: SystemProgram.programId,
        rewardVault,
        tokenProgram: SPL_TOKEN_PROGRAM_ID,
        tokenMint: SOL_NATIVE_MINT,
        ixSysvar: SYSVAR_INSTRUCTIONS,
      },
      remainingAccounts,
    }
  );

  // 7. Bundle [secp256k1 verify, submit response, verify_task]
  const allIxs = [secpIx, submitResponseIx, verifyIx];

  const { blockhash } = await connection.getLatestBlockhash("confirmed");
  const messageV0 = new TransactionMessage({
    payerKey: payer,
    recentBlockhash: blockhash,
    instructions: allIxs,
  }).compileToV0Message();

  const vtx = new VersionedTransaction(messageV0);

  // Output unsigned tx as base64 to stdout (no newline — MCP reads exactly this)
  process.stdout.write(Buffer.from(vtx.serialize()).toString("base64"));
}

// ---------------------------------------------------------------------------
// Build the shillbot verify_task instruction from raw params
// ---------------------------------------------------------------------------

function buildVerifyTaskIx(
  taskPda: PublicKey,
  globalState: PublicKey,
  switchboardFeed: PublicKey,
  compositeScore: bigint,
  verificationHash: Buffer
): TransactionInstruction {
  // Anchor discriminator: SHA256("global:verify_task")[:8]
  const crypto = require("crypto");
  const disc = crypto
    .createHash("sha256")
    .update("global:verify_task")
    .digest()
    .subarray(0, 8);

  // Instruction data: 8 disc + 8 composite_score (u64 LE) + 32 verification_hash
  const data = Buffer.alloc(48);
  disc.copy(data, 0);
  data.writeBigUInt64LE(compositeScore, 8);
  verificationHash.copy(data, 16);

  return new TransactionInstruction({
    programId: SHILLBOT_PROGRAM_ID,
    keys: [
      { pubkey: taskPda, isSigner: false, isWritable: true },
      { pubkey: globalState, isSigner: false, isWritable: false },
      { pubkey: switchboardFeed, isSigner: false, isWritable: false },
    ],
    data,
  });
}

main().catch((err) => {
  process.stderr.write(`build-verify-tx failed: ${err}\n`);
  process.exit(1);
});
