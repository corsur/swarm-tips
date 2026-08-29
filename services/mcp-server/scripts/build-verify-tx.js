"use strict";
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
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
 *     --rpc <url> [--network <mainnet|devnet>]
 *
 * --network selects the Switchboard queue; it defaults to mainnet. It was read
 * by the arg parser but missing from this block.
 *
 * Uses Queue.fetchSignaturesConsensus directly (not PullFeed.fetchUpdateIx)
 * because fetchUpdateIx has a bug where it drops variableOverrides before
 * the gateway call. We replicate the instruction building from the SDK.
 */
const on_demand_1 = require("@switchboard-xyz/on-demand");
// @ts-ignore — not re-exported from the main index
const secp256k1_instruction_utils_js_1 = require("@switchboard-xyz/on-demand/dist/esm/instruction-utils/secp256k1-instruction-utils.js");
const web3_js_1 = require("@solana/web3.js");
const anchor = __importStar(require("@coral-xyz/anchor"));
const anchor_1 = require("@coral-xyz/anchor");
// Inline ATA derivation (avoids @solana/spl-token dependency)
const ASSOCIATED_TOKEN_PROGRAM_ID = new web3_js_1.PublicKey("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
// Local ATA derivation so the script needs no @solana/spl-token dependency.
// The upstream helper takes an allowOwnerOffCurve flag; this one does not,
// because the body never branched on it — carrying the parameter implied a
// behaviour the function did not have.
function getAssociatedTokenAddressSync(mint, owner) {
    return web3_js_1.PublicKey.findProgramAddressSync([owner.toBuffer(), SPL_TOKEN_PROGRAM_ID.toBuffer(), mint.toBuffer()], ASSOCIATED_TOKEN_PROGRAM_ID)[0];
}
// ---------------------------------------------------------------------------
// Parse CLI args
// ---------------------------------------------------------------------------
function parseArgs() {
    const args = {};
    const argv = process.argv.slice(2);
    for (let i = 0; i < argv.length; i += 2) {
        const key = argv[i].replace(/^--/, "");
        args[key] = argv[i + 1];
    }
    return args;
}
const SHILLBOT_PROGRAM_ID = new web3_js_1.PublicKey("2tR37nqMpwdV4DVUHjzUmL1rH2DtkA8zrRA4EAhT7KMi");
// Switchboard On-Demand has different program IDs per network.
//   mainnet: SBondMDrcV3K4kxZR1HNVT7osZxAHVHgYXL5Ze1oMUv
//   devnet:  Aio4gaXjXzJNVLtzwtNVmSqGKpANtXhybbkhtAC94ji2
// The SDK's `PullFeed`/`Queue` constructors derive PDAs from this program ID,
// so picking the wrong one means we can't read the feed account or build
// valid crank instructions. Selected per `--network` arg below.
const SB_PROGRAM_ID_MAINNET = new web3_js_1.PublicKey("SBondMDrcV3K4kxZR1HNVT7osZxAHVHgYXL5Ze1oMUv");
const SB_PROGRAM_ID_DEVNET = new web3_js_1.PublicKey("Aio4gaXjXzJNVLtzwtNVmSqGKpANtXhybbkhtAC94ji2");
// SPL sysvars
const SYSVAR_SLOT_HASHES = new web3_js_1.PublicKey("SysvarS1otHashes111111111111111111111111111");
const SYSVAR_INSTRUCTIONS = new web3_js_1.PublicKey("Sysvar1nstructions1111111111111111111111111");
// SOL native mint
const SOL_NATIVE_MINT = new web3_js_1.PublicKey("So11111111111111111111111111111111111111112");
const SPL_TOKEN_PROGRAM_ID = new web3_js_1.PublicKey("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------
async function main() {
    const args = parseArgs();
    const taskId = args["task-id"];
    const payer = new web3_js_1.PublicKey(args["payer"]);
    const score = BigInt(args["score"]);
    const hashHex = args["hash"];
    const taskPda = new web3_js_1.PublicKey(args["task-pda"]);
    const feedPubkey = new web3_js_1.PublicKey(args["feed"]);
    const globalState = new web3_js_1.PublicKey(args["global-state"]);
    const rpcUrl = args["rpc"];
    const network = args["network"] === "devnet" || rpcUrl.includes("devnet")
        ? "devnet"
        : "mainnet";
    const SB_PROGRAM_ID = network === "devnet" ? SB_PROGRAM_ID_DEVNET : SB_PROGRAM_ID_MAINNET;
    if (!taskId || !rpcUrl) {
        process.stderr.write("required: --task-id, --payer, --score, --hash, --task-pda, --feed, --global-state, --rpc\n");
        process.exit(1);
    }
    const connection = new web3_js_1.Connection(rpcUrl, "confirmed");
    // Dummy wallet (we never sign — just need Anchor provider for SDK)
    const dummyKeypair = web3_js_1.Keypair.generate();
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
    const verifyIx = buildVerifyTaskIx(taskPda, globalState, feedPubkey, score, verificationHash);
    // 2. Load feed data to get queue and feed hash.
    // Cast: @switchboard-xyz/on-demand pins @coral-xyz/anchor ^0.31, the rest of the
    // repo is on 0.32. Runtime behaves identically; the type mismatch is a
    // private-property nominal conflict only.
    const feedAccount = new on_demand_1.PullFeed(program, feedPubkey);
    const feedData = await feedAccount.loadData();
    const queuePubkey = feedData.queue;
    const feedHashHex = Buffer.from(feedData.feedHash).toString("hex");
    // 3. Fetch jobs from crossbar (same as SDK does internally)
    const crossbarResp = await fetch(`https://crossbar.switchboard.xyz/fetch/${feedHashHex}`).then((r) => r.json());
    const jobs = crossbarResp.jobs || [];
    if (jobs.length === 0) {
        process.stderr.write(`No jobs found on crossbar for feed hash ${feedHashHex}\n`);
        process.exit(1);
    }
    // 4. Call gateway directly with variableOverrides via Queue.fetchSignaturesConsensus
    //    This properly passes variableOverrides to the gateway (unlike fetchUpdateIx).
    const queueAccount = new on_demand_1.Queue(program, queuePubkey);
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
    const secpSignatures = response.oracle_responses.map((oracleResponse, responseIdx) => ({
        ethAddress: Buffer.from(oracleResponse.eth_address, "hex"),
        signature: Buffer.from(oracleResponse.signature, "base64"),
        message: Buffer.from(oracleResponse.checksum, "base64"),
        recoveryId: oracleResponse.recovery_id,
        oracleIdx: responseIdx,
    }));
    if (secpSignatures.length === 0) {
        process.stderr.write("No valid oracle signatures\n");
        process.exit(1);
    }
    // Compute-budget instructions the tx carries ITSELF, at the very front.
    // This is load-bearing, not a perf tweak: the secp256k1 instruction below
    // encodes the ABSOLUTE instruction index at which its signature data lives,
    // so the bundle's ordering is fixed. Wallets with auto-priority-fee (Phantom's
    // "smart priority fee" et al.) prepend their own ComputeBudget instructions
    // when a tx carries none — which shifts secp256k1 off the index it points at,
    // and the precompile then reads a ComputeBudget instruction as its signature
    // data and fails with "custom program error: 0x2" at the shifted index.
    // Carrying our own ComputeBudget instructions makes wallets leave the layout
    // alone (they skip auto-fee when compute budget is already present), and the
    // secp index below is built for this exact position.
    const computeBudgetIxs = [
        web3_js_1.ComputeBudgetProgram.setComputeUnitLimit({ units: 400000 }),
        web3_js_1.ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 10000 }),
    ];
    // Build Secp256k1 native instruction using SDK's implementation. The second
    // arg is the absolute index at which this instruction sits in the tx — it
    // follows the two compute-budget instructions above, so it is their count.
    const secpIx = secp256k1_instruction_utils_js_1.Secp256k1InstructionUtils.buildSecp256k1Instruction(secpSignatures, computeBudgetIxs.length);
    // 6. Build pullFeedSubmitResponseConsensus instruction
    const instructionData = {
        slot: new anchor_1.BN(response.slot),
        values: response.median_responses.map((mr) => new anchor_1.BN(mr.value)),
    };
    const programState = on_demand_1.State.keyFromSeed(program);
    // (the removed third arg was allowOwnerOffCurve, which the helper ignored)
    const rewardVault = getAssociatedTokenAddressSync(SOL_NATIVE_MINT, queuePubkey);
    const oraclePubkeys = response.oracle_responses.map((r) => new web3_js_1.PublicKey(Buffer.from(r.oracle_pubkey, "hex")));
    const oracleStatsPubkeys = oraclePubkeys.map((oracle) => web3_js_1.PublicKey.findProgramAddressSync([Buffer.from("OracleStats"), oracle.toBuffer()], SB_PROGRAM_ID)[0]);
    // Match feed pubkeys from median_responses
    const feedPubkeys = response.median_responses.map((mr) => {
        if (mr.feed_hash === feedHashHex)
            return feedPubkey;
        return web3_js_1.PublicKey.default;
    });
    const remainingAccounts = [
        ...feedPubkeys.map((pk) => ({
            pubkey: pk,
            isSigner: false,
            isWritable: true,
        })),
        ...oraclePubkeys.map((pk) => ({
            pubkey: pk,
            isSigner: false,
            isWritable: false,
        })),
        ...oracleStatsPubkeys.map((pk) => ({
            pubkey: pk,
            isSigner: false,
            isWritable: true,
        })),
    ];
    const submitResponseIx = program.instruction.pullFeedSubmitResponseConsensus(instructionData, {
        accounts: {
            queue: queuePubkey,
            programState,
            recentSlothashes: SYSVAR_SLOT_HASHES,
            payer,
            systemProgram: web3_js_1.SystemProgram.programId,
            rewardVault,
            tokenProgram: SPL_TOKEN_PROGRAM_ID,
            tokenMint: SOL_NATIVE_MINT,
            ixSysvar: SYSVAR_INSTRUCTIONS,
        },
        remainingAccounts,
    });
    // 7. Bundle [compute-budget x2, secp256k1 verify, submit response, verify_task].
    // secp256k1 stays immediately before the Switchboard submit-response (its
    // relative position is what the on-demand program checks) AND at the absolute
    // index its own offsets encode (computeBudgetIxs.length). See the note above.
    const allIxs = [...computeBudgetIxs, secpIx, submitResponseIx, verifyIx];
    const { blockhash } = await connection.getLatestBlockhash("confirmed");
    const messageV0 = new web3_js_1.TransactionMessage({
        payerKey: payer,
        recentBlockhash: blockhash,
        instructions: allIxs,
    }).compileToV0Message();
    const vtx = new web3_js_1.VersionedTransaction(messageV0);
    // Output unsigned tx as base64 to stdout (no newline — MCP reads exactly this)
    process.stdout.write(Buffer.from(vtx.serialize()).toString("base64"));
}
// ---------------------------------------------------------------------------
// Build the shillbot verify_task instruction from raw params
// ---------------------------------------------------------------------------
function buildVerifyTaskIx(taskPda, globalState, switchboardFeed, compositeScore, verificationHash) {
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
    return new web3_js_1.TransactionInstruction({
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
