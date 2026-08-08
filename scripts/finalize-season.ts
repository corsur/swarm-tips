/**
 * Finalize a season on any chain.
 *
 *   npx tsx scripts/finalize-season.ts --chain base_sepolia --season 1
 *   npx tsx scripts/finalize-season.ts --chain solana --tournament 3 --dry-run
 *
 * Replaces the Solana-only `finalize-tournament.ts`. The rules (eligibility,
 * score, pot split, tree format) come from `season-core.ts` and are identical
 * everywhere; this file is ONLY the per-chain read and write:
 *
 *   read   Solana: PlayerProfile PDAs   EVM: the `records` mapping via events
 *   write  Solana: finalize_tournament  EVM: finalizeSeason
 *
 * `--dry-run` computes and writes the proofs artifact without sending anything,
 * which is how you inspect a root before committing to it on-chain. A root is
 * immutable once published: `finalizeSeason` reverts if already finalized, and
 * so does Solana's `finalize_tournament`.
 */

import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { buildSeason, assertVectorParity, type PlayerRecord } from "./season-core";

interface Args {
  chain: string;
  season: bigint;
  dryRun: boolean;
}

function parseArgs(): Args {
  const a = process.argv.slice(2);
  const get = (k: string) => {
    const i = a.indexOf(`--${k}`);
    return i === -1 ? undefined : a[i + 1];
  };
  const chain = get("chain");
  if (!chain) throw new Error("--chain is required (solana | base_sepolia | base | ethereum)");
  const season = get("season") ?? get("tournament");
  if (!season) throw new Error("--season (or --tournament for Solana) is required");
  return { chain, season: BigInt(season), dryRun: a.includes("--dry-run") };
}

/**
 * EVM: read each player's season record straight from the on-chain `records`
 * mapping.
 *
 * DELIBERATELY NOT an eth_getLogs scan. Both public RPCs cap the range (Base at
 * 10k blocks; some Ethereum nodes demand archive access), so a naive scan
 * returns a PARTIAL player set, and the resulting tree silently omits people —
 * they cannot claim and nothing looks wrong. Reading the mapping for an
 * explicit player set is exact: a wrong address yields games=0 and is dropped
 * by the eligibility filter, which is a visible no-op rather than a silent
 * omission.
 */
async function readEvmRecords(
  rpc: string,
  contract: string,
  season: bigint,
  players: string[],
): Promise<{ records: PlayerRecord[]; pot: bigint }> {
  if (players.length === 0) {
    throw new Error(
      "--players is required for an EVM season: pass the comma-separated addresses to " +
        "score. A partial set produces a tree that silently omits claimants, so this " +
        "script will not guess it.",
    );
  }
  const call = async (data: string) => {
    const r = await fetch(rpc, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "eth_call", params: [{ to: contract, data }, "latest"] }),
    });
    const j: any = await r.json();
    if (j.error) throw new Error(`eth_call failed: ${JSON.stringify(j.error).slice(0, 140)}`);
    return j.result as string;
  };
  const pad = (h: string) => h.replace(/^0x/, "").padStart(64, "0");

  // records(uint256,address) -> (uint64 wins, uint64 games, bool claimed)
  const RECORDS_SEL = keccakSelector("records(uint256,address)");
  const records: PlayerRecord[] = [];
  for (const p of players) {
    const out = await call(RECORDS_SEL + pad(season.toString(16)) + pad(p));
    const body = out.replace(/^0x/, "");
    records.push({
      account: p.toLowerCase(),
      wins: BigInt("0x" + body.slice(0, 64)),
      games: BigInt("0x" + body.slice(64, 128)),
    });
  }

  // The pot is what THIS season accrued, not the contract balance - a season
  // must never promise another season's money.
  // seasons(uint256) -> (start,end,finalized,root,accrued,prize,remaining)
  const seasonOut = await call(keccakSelector("seasons(uint256)") + pad(season.toString(16)));
  const sb = seasonOut.replace(/^0x/, "");
  const pot = BigInt("0x" + sb.slice(4 * 64, 5 * 64));

  return { records, pot };
}

/** 4-byte selector for a Solidity signature. */
function keccakSelector(sig: string): string {
  const { keccak_256 } = require("@noble/hashes/sha3.js");
  return "0x" + Buffer.from(keccak_256(Buffer.from(sig, "utf8"))).toString("hex").slice(0, 8);
}

/** Solana: every PlayerProfile PDA for the tournament. */
async function readSolanaRecords(tournamentId: bigint): Promise<{ records: PlayerRecord[]; pot: bigint }> {
  throw new Error(
    `Solana record reading is not wired for tournament ${tournamentId}. ` +
      `scripts/finalize-tournament.ts still owns this path; port its ` +
      `playerProfile.all() memcmp read here rather than duplicating the rules.`,
  );
}

/**
 * Publish the root on-chain. A root is IMMUTABLE once published — finalizeSeason
 * reverts if the season is already finalized — so this runs only after the
 * artifact above has been written and can be inspected.
 *
 * `totalWei` is the sum actually distributed, never the pot: the split truncates
 * and a season must not promise more than it holds. The contract enforces the
 * same bound (`totalWei > accruedWei` reverts), so a mismatch here fails loudly
 * rather than stranding a claimant.
 */
async function submitEvmFinalize(
  rpc: string,
  contract: string,
  season: bigint,
  root: string,
  totalWei: bigint,
): Promise<void> {
  const { createWalletClient, createPublicClient, http } = await import("viem");
  const { privateKeyToAccount } = await import("viem/accounts");
  const { baseSepolia } = await import("viem/chains");

  const raw = process.env.OWNER_KEY;
  if (!raw) throw new Error("OWNER_KEY is required to sign finalizeSeason");
  const owner = privateKeyToAccount(
    (raw.trim().startsWith("0x") ? raw.trim() : `0x${raw.trim()}`) as `0x${string}`,
  );

  const abi = [
    {
      type: "function", name: "finalizeSeason", stateMutability: "nonpayable",
      inputs: [
        { name: "seasonId", type: "uint256" },
        { name: "root", type: "bytes32" },
        { name: "totalWei", type: "uint256" },
      ],
      outputs: [],
    },
    {
      type: "function", name: "seasons", stateMutability: "view",
      inputs: [{ name: "seasonId", type: "uint256" }],
      outputs: [
        { name: "startTime", type: "uint64" }, { name: "endTime", type: "uint64" },
        { name: "finalized", type: "bool" }, { name: "root", type: "bytes32" },
        { name: "accruedWei", type: "uint256" }, { name: "prizeWei", type: "uint256" },
        { name: "remainingWei", type: "uint256" },
      ],
    },
  ] as const;

  const pub = createPublicClient({ chain: baseSepolia, transport: http(rpc) });
  const wallet = createWalletClient({ account: owner, chain: baseSepolia, transport: http(rpc) });

  console.log(`submitting finalizeSeason(${season}, ${root}, ${totalWei}) as ${owner.address}`);
  const hash = await wallet.writeContract({
    address: contract as `0x${string}`, abi, functionName: "finalizeSeason",
    args: [season, root as `0x${string}`, totalWei],
  });
  const rcpt = await pub.waitForTransactionReceipt({ hash });
  if (rcpt.status !== "success") throw new Error(`finalizeSeason reverted (${hash})`);

  // Read the CHAIN back: a receipt says the tx executed, not that the root the
  // claimants will prove against is the one we just computed.
  //
  // RETRIED, because a receipt does not mean the node serving this read has the
  // block: the first attempt here returned a ZERO root for a season that was in
  // fact finalized, which would have reported a false failure on a good publish.
  for (let i = 0; ; i++) {
    const s = (await pub.readContract({
      address: contract as `0x${string}`, abi, functionName: "seasons", args: [season],
    })) as readonly unknown[];
    if (String(s[3]).toLowerCase() === root.toLowerCase()) {
      console.log(`  finalized: root on-chain matches, prizeWei ${String(s[5])}`);
      return;
    }
    if (i >= 20) throw new Error(`on-chain root ${String(s[3])} != computed ${root}`);
    await new Promise((r) => setTimeout(r, 3000));
  }
}

async function main() {
  const { chain, season, dryRun } = parseArgs();

  // Fail before touching a chain if this script has drifted from the contracts.
  const { minGames } = assertVectorParity();
  console.log(`vector parity OK (minGames=${minGames})`);

  const isSolana = chain === "solana" || chain === "devnet";
  const a = process.argv.slice(2);
  const flag = (k: string) => { const i = a.indexOf(`--${k}`); return i === -1 ? undefined : a[i + 1]; };
  const { records, pot } = isSolana
    ? await readSolanaRecords(season)
    : await readEvmRecords(
        flag("rpc") ?? "https://sepolia.base.org",
        flag("contract") ?? (() => { throw new Error("--contract is required for an EVM season"); })(),
        season,
        (flag("players") ?? "").split(",").map((x) => x.trim()).filter(Boolean),
      );

  const accountBytes = isSolana
    ? (a: string) => Buffer.from(a, "utf8") // placeholder: Solana leaves use the 32-byte pubkey
    : (a: string) => Buffer.from(a.replace(/^0x/, ""), "hex");

  const result = buildSeason(records, pot, accountBytes);

  const artifact = join(__dirname, `season-${chain}-${season}-proofs.json`);
  writeFileSync(
    artifact,
    JSON.stringify(
      {
        chain,
        season: season.toString(),
        root: result.root,
        potTotal: pot.toString(),
        totalDistributed: result.totalDistributed.toString(),
        minGamesForPayout: minGames.toString(),
        zeroScored: result.zeroScored,
        claims: result.entitlements.map((e) => ({
          account: e.account,
          score: e.score.toString(),
          amount: e.amount.toString(),
          proof: e.proof,
        })),
      },
      null,
      2,
    ) + "\n",
  );
  console.log(`wrote ${artifact}`);
  console.log(`  root        ${result.root}`);
  console.log(`  distributed ${result.totalDistributed} of ${pot}`);
  console.log(`  claimants   ${result.entitlements.length} (${result.zeroScored.length} eligible but zero-scored)`);

  if (dryRun) {
    console.log("\n--dry-run: nothing sent. A published root is IMMUTABLE, so review the artifact first.");
    return;
  }
  if (isSolana) {
    throw new Error(
      "Solana finalize submission is not wired here; finalize-tournament.ts still owns that path",
    );
  }
  await submitEvmFinalize(
    flag("rpc") ?? "https://sepolia.base.org",
    flag("contract")!,
    season,
    result.root,
    result.totalDistributed,
  );
}

main().catch((e) => {
  console.error(String(e instanceof Error ? e.message : e));
  process.exit(1);
});
