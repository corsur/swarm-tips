/**
 * One-shot: finalize mainnet tournament 1.
 *
 * 1. Reads every PlayerProfile account for tournament_id=1
 * 2. Filters to players with >= MIN_GAMES_FOR_PAYOUT (5) games
 * 3. Computes entitlements proportional to score (wins^2 / total_games)
 * 4. Builds the merkle tree using the program's exact scheme:
 *    - Leaf:     keccak256(0x00 || wallet_pubkey || amount_le_bytes)
 *    - Internal: keccak256(0x01 || min(left, right) || max(left, right))
 * 5. Calls finalize_tournament(merkle_root) (authority-gated, signed by id.json)
 * 6. Writes the full tree + per-player proofs to scripts/tournament-1-proofs.json
 *    so players can claim_reward(amount, proof) without re-running this script.
 *
 * Re-running after finalize is a no-op: the on-chain tournament.finalized
 * flag flips once and the require!() rejects subsequent calls.
 *
 * Usage: ANCHOR_WALLET=~/.config/solana/id.json \
 *        ANCHOR_PROVIDER_URL=https://api.mainnet-beta.solana.com \
 *        npx ts-node scripts/finalize-tournament-1.ts
 */

import * as anchor from "@coral-xyz/anchor";
import { PublicKey } from "@solana/web3.js";
import { keccak_256 } from "@noble/hashes/sha3.js";
import * as fs from "fs";
import * as path from "path";

const idl = JSON.parse(
  fs.readFileSync(
    path.join(__dirname, "../target/idl/coordination_game.json"),
    "utf8",
  ),
);

const TOURNAMENT_ID = 1n;
const MIN_GAMES_FOR_PAYOUT = 5n;
const PROOFS_FILE = path.join(__dirname, "tournament-1-proofs.json");

interface PlayerEntry {
  wallet: PublicKey;
  wins: bigint;
  totalGames: bigint;
  score: bigint;
}

interface ClaimEntry {
  wallet: string;
  amount: number;
  proof: string[]; // hex
}

function leaf(wallet: PublicKey, amount: bigint): Uint8Array {
  const amountBytes = new Uint8Array(8);
  new DataView(amountBytes.buffer).setBigUint64(0, amount, true);
  return keccak_256(
    Buffer.concat([Buffer.from([0x00]), wallet.toBuffer(), amountBytes]),
  );
}

function hashInternal(a: Uint8Array, b: Uint8Array): Uint8Array {
  const [left, right] = Buffer.compare(Buffer.from(a), Buffer.from(b)) <= 0
    ? [a, b]
    : [b, a];
  return keccak_256(Buffer.concat([Buffer.from([0x01]), left, right]));
}

function buildMerkleTree(leaves: Uint8Array[]): Uint8Array[][] {
  if (leaves.length === 0) {
    return [[new Uint8Array(32)]];
  }
  const layers: Uint8Array[][] = [leaves];
  while (layers[layers.length - 1].length > 1) {
    const prev = layers[layers.length - 1];
    const next: Uint8Array[] = [];
    for (let i = 0; i < prev.length; i += 2) {
      if (i + 1 < prev.length) {
        next.push(hashInternal(prev[i], prev[i + 1]));
      } else {
        next.push(prev[i]); // odd-leaf carry-up
      }
    }
    layers.push(next);
  }
  return layers;
}

function getProof(layers: Uint8Array[][], idx: number): Uint8Array[] {
  const proof: Uint8Array[] = [];
  let i = idx;
  for (let level = 0; level < layers.length - 1; level++) {
    const layer = layers[level];
    const sib = i ^ 1;
    if (sib < layer.length) {
      proof.push(layer[sib]);
    }
    i = i >> 1;
  }
  return proof;
}

const toHex = (b: Uint8Array): string =>
  Array.from(b).map((x) => x.toString(16).padStart(2, "0")).join("");

async function main() {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = new anchor.Program(idl as anchor.Idl, provider);
  const wallet = provider.wallet.publicKey;

  console.log(`Wallet:  ${wallet.toBase58()}`);
  console.log(`RPC:     ${provider.connection.rpcEndpoint}`);
  console.log(`Program: ${program.programId.toBase58()}`);
  console.log(`Tournament: ${TOURNAMENT_ID}`);
  console.log();

  const idBuf = Buffer.alloc(8);
  idBuf.writeBigUInt64LE(TOURNAMENT_ID);
  const [tournamentPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("tournament"), idBuf],
    program.programId,
  );
  console.log(`Tournament PDA: ${tournamentPda.toBase58()}`);

  const [globalConfigPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("global_config")],
    program.programId,
  );

  const tournamentData = await (program.account as any).tournament.fetch(tournamentPda);
  console.log(`  prize_lamports: ${tournamentData.prizeLamports.toString()} (${Number(tournamentData.prizeLamports) / 1e9} SOL)`);
  console.log(`  game_count:     ${tournamentData.gameCount.toString()}`);
  console.log(`  end_time:       ${tournamentData.endTime.toString()} (${new Date(Number(tournamentData.endTime) * 1000).toISOString()})`);
  console.log(`  finalized:      ${tournamentData.finalized}`);

  const now = Math.floor(Date.now() / 1000);
  if (Number(tournamentData.endTime) >= now) {
    console.error(`Tournament has not ended (end_time=${tournamentData.endTime}, now=${now})`);
    process.exit(1);
  }
  if (tournamentData.finalized) {
    console.log("Tournament already finalized. Skipping the on-chain call; will only refresh the proofs file.");
  }

  const prizeLamports = BigInt(tournamentData.prizeLamports.toString());

  console.log("\nLoading all PlayerProfile accounts for tournament_id=1...");
  const tournamentIdBytes = Buffer.alloc(8);
  tournamentIdBytes.writeBigUInt64LE(TOURNAMENT_ID);
  const profiles = await (program.account as any).playerProfile.all([
    {
      memcmp: {
        offset: 8 + 32, // disc + wallet
        bytes: anchor.utils.bytes.bs58.encode(tournamentIdBytes),
      },
    },
  ]);
  console.log(`  found ${profiles.length} PlayerProfile accounts`);

  const eligible: PlayerEntry[] = profiles
    .map((p: any) => ({
      wallet: p.account.wallet as PublicKey,
      wins: BigInt(p.account.wins.toString()),
      totalGames: BigInt(p.account.totalGames.toString()),
      score: BigInt(p.account.score.toString()),
    }))
    .filter((p: PlayerEntry) => p.totalGames >= MIN_GAMES_FOR_PAYOUT)
    .sort((a: PlayerEntry, b: PlayerEntry) =>
      a.wallet.toBase58().localeCompare(b.wallet.toBase58()),
    );

  console.log(`  eligible (>= ${MIN_GAMES_FOR_PAYOUT} games): ${eligible.length}`);

  const totalScore = eligible.reduce((s, p) => s + p.score, 0n);
  console.log(`  total score across eligible players: ${totalScore}`);

  if (totalScore === 0n) {
    console.error("Total score is zero — no one is eligible. Aborting.");
    process.exit(1);
  }

  // Compute entitlements. Use floor division and sweep the dust to the top
  // scorer so the sum always equals prize_lamports exactly (the on-chain
  // tournament holds prize_lamports + rent; we must not over-distribute).
  const entitlements: bigint[] = eligible.map(
    (p) => (prizeLamports * p.score) / totalScore,
  );
  const distributedSoFar = entitlements.reduce((s, e) => s + e, 0n);
  const dust = prizeLamports - distributedSoFar;
  if (dust > 0n) {
    let topIdx = 0;
    for (let i = 1; i < eligible.length; i++) {
      if (eligible[i].score > eligible[topIdx].score) topIdx = i;
    }
    entitlements[topIdx] += dust;
  }

  console.log("\nDistribution:");
  for (let i = 0; i < eligible.length; i++) {
    console.log(
      `  ${eligible[i].wallet.toBase58()}: score=${eligible[i].score} ` +
      `wins=${eligible[i].wins}/${eligible[i].totalGames} ` +
      `→ ${entitlements[i]} lamports (${Number(entitlements[i]) / 1e9} SOL)`,
    );
  }
  console.log(`  TOTAL: ${entitlements.reduce((s, e) => s + e, 0n)} = ${prizeLamports} lamports`);

  // Build merkle tree
  const leaves = eligible.map((p, i) => leaf(p.wallet, entitlements[i]));
  const tree = buildMerkleTree(leaves);
  const root = tree[tree.length - 1][0];
  console.log(`\nMerkle root: ${toHex(root)}`);

  // Build per-player proofs file BEFORE the tx so we can re-run safely
  const claimEntries: ClaimEntry[] = eligible.map((p, i) => ({
    wallet: p.wallet.toBase58(),
    amount: Number(entitlements[i]),
    proof: getProof(tree, i).map(toHex),
  }));
  const proofsArtifact = {
    tournament_id: Number(TOURNAMENT_ID),
    prize_lamports: Number(prizeLamports),
    merkle_root: toHex(root),
    min_games: Number(MIN_GAMES_FOR_PAYOUT),
    generated_at: new Date().toISOString(),
    claims: claimEntries,
  };
  fs.writeFileSync(PROOFS_FILE, JSON.stringify(proofsArtifact, null, 2));
  console.log(`\nProofs written to ${PROOFS_FILE}`);

  if (tournamentData.finalized) {
    console.log("\nAlready finalized on-chain — done.");
    return;
  }

  console.log("\nSubmitting finalize_tournament...");
  const sig = await (program.methods as any)
    .finalizeTournament(Array.from(root))
    .accountsPartial({
      tournament: tournamentPda,
      globalConfig: globalConfigPda,
      authority: wallet,
    })
    .rpc();
  console.log(`  OK: signature=${sig}`);

  const after = await (program.account as any).tournament.fetch(tournamentPda);
  console.log(`  finalized: ${after.finalized}`);
  console.log(`  prize_snapshot: ${after.prizeSnapshot.toString()}`);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
