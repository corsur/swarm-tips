/**
 * Seed the LeanProof bounty catalog onto DEVNET as v2 (mathlib) campaigns.
 *
 * For every statement in tests/fixtures/lean/catalog/{open,solvable}/*.lean it
 * creates a `lean_policy: 2` LeanProof campaign (statement_lean = the file bytes)
 * and funds one task, so the board shows real standing bounties. Idempotent: a
 * campaign is tagged `brief.topic = "catalog:<name>"`; a re-run skips any tag
 * that already exists among the client's campaigns.
 *
 * `open/` = famous unsolved problems (small escrow — advertisement, unlikely to
 * be claimed). `solvable/` = provable (a companion <name>.proof.lean exists),
 * proven live by tests/live/shillbot-attester-v2-e2e-testnet.ts / the settlement
 * demo. Every statement was elaboration-gated before it was committed.
 *
 * DEVNET ONLY (real devnet SOL from id.json). Seeding with REAL mainnet SOL is a
 * founder decision — do NOT point this at mainnet without explicit sign-off.
 *
 *   DEVNET_RPC="$(gcloud secrets versions access latest \
 *       --secret=solana-rpc-url-devnet --project coordination-game-prod)" \
 *   npx tsx scripts/e2e/seed-lean-catalog.ts
 */
import * as fs from "fs";
import * as path from "path";
import * as crypto from "crypto";
import { execSync } from "child_process";
import { BN } from "@coral-xyz/anchor";
import {
  Connection,
  Keypair,
  LAMPORTS_PER_SOL,
  Transaction,
  VersionedTransaction,
} from "@solana/web3.js";
import { loadKeypair } from "./devnet-harness";
import { LEAN_PROOF_PLATFORM } from "../../tests/harness/shillbot-steps";

const API_BASE = process.env.SHILLBOT_API ?? "https://api.shillbot.org";
// Small standing escrow per bounty (~0.002 SOL). Open problems and solvables use
// the same escrow on devnet and mainnet; the point is a visible board entry, not a prize.
const ESCROW = new BN(2_000_000);
const CATALOG_DIR = path.join(
  __dirname,
  "..",
  "..",
  "tests",
  "fixtures",
  "lean",
  "catalog"
);

function getSecret(name: string): string {
  try {
    return execSync(
      `gcloud secrets versions access latest --secret=${name} --project=coordination-game-prod`,
      { stdio: ["ignore", "pipe", "ignore"] }
    )
      .toString()
      .trim();
  } catch {
    return "";
  }
}

function getJwtSecret(): string {
  return process.env.JWT_SECRET || getSecret("jwt-secret");
}

function getRpcUrl(isMainnet: boolean): string {
  if (isMainnet) {
    return (
      process.env.SOLANA_RPC_URL ??
      process.env.MAINNET_RPC ??
      getSecret("solana-rpc-url-mainnet") ??
      "https://api.mainnet-beta.solana.com"
    );
  }
  return (
    process.env.DEVNET_RPC ??
    process.env.SOLANA_RPC_URL ??
    getSecret("solana-rpc-url-devnet") ??
    "https://api.devnet.solana.com"
  );
}

function mintJwt(wallet: string, secret: string): string {
  if (!secret) return wallet;
  const header = Buffer.from(
    JSON.stringify({ alg: "HS256", typ: "JWT" })
  ).toString("base64url");
  const now = Math.floor(Date.now() / 1000);
  const payload = Buffer.from(
    JSON.stringify({ wallet, exp: now + 86400 })
  ).toString("base64url");
  const sig = crypto
    .createHmac("sha256", secret)
    .update(`${header}.${payload}`)
    .digest("base64url");
  return `${header}.${payload}.${sig}`;
}

async function api(
  method: string,
  path_: string,
  bearer: string,
  body?: unknown
): Promise<Record<string, unknown>> {
  const res = await fetch(API_BASE + path_, {
    method,
    headers: {
      Authorization: `Bearer ${bearer}`,
      "Content-Type": "application/json",
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await res.text();
  if (!res.ok) throw new Error(`${method} ${path_} -> ${res.status}: ${text}`);
  return text ? (JSON.parse(text) as Record<string, unknown>) : {};
}

function signBase64(unsigned: string, signer: Keypair): string {
  const raw = Buffer.from(unsigned, "base64");
  try {
    const vtx = VersionedTransaction.deserialize(raw);
    vtx.sign([signer]);
    return Buffer.from(vtx.serialize()).toString("base64");
  } catch {
    const tx = Transaction.from(raw);
    tx.partialSign(signer);
    return tx.serialize({ requireAllSignatures: false }).toString("base64");
  }
}

async function broadcast(conn: Connection, signedB64: string): Promise<string> {
  const sig = await conn.sendRawTransaction(Buffer.from(signedB64, "base64"), {
    skipPreflight: false,
    maxRetries: 5,
  });
  await conn.confirmTransaction(sig, "confirmed");
  return sig;
}

export interface CatalogEntry {
  name: string;
  kind: "open" | "solvable";
  statement: string;
}

export function loadCatalog(): CatalogEntry[] {
  const entries: CatalogEntry[] = [];
  for (const kind of ["solvable", "open"] as const) {
    const dir = path.join(CATALOG_DIR, kind);
    for (const f of fs.readdirSync(dir).sort()) {
      if (!f.endsWith(".lean") || f.endsWith(".proof.lean")) continue;
      entries.push({
        name: f.replace(/\.lean$/, ""),
        kind,
        statement: fs.readFileSync(path.join(dir, f), "utf8").trimEnd(),
      });
    }
  }
  return entries;
}

async function existingTopics(
  bearer: string,
  network: string
): Promise<Set<string>> {
  const list = (await api(
    "GET",
    `/campaigns?network=${network}`,
    bearer
  )) as unknown as Array<{ brief?: { topic?: string } }>;
  const topics = new Set<string>();
  const items = Array.isArray(list) ? list : (list as any)?.campaigns ?? [];
  for (const c of items) {
    if (c.brief?.topic) topics.add(c.brief.topic);
  }
  return topics;
}

async function main(): Promise<void> {
  const isMainnet =
    process.argv.includes("--mainnet") ||
    process.env.SOLANA_NETWORK === "mainnet";
  const network = isMainnet ? "mainnet" : "devnet";
  const rpcUrl = getRpcUrl(isMainnet);
  const conn = new Connection(rpcUrl, "confirmed");

  const authority = loadKeypair("id.json");
  const clientPk = authority.publicKey.toBase58();
  const jwtSecret = getJwtSecret();
  const bearer = mintJwt(clientPk, jwtSecret);

  const bal = await conn.getBalance(authority.publicKey, "confirmed");
  console.log(`network: ${network}`);
  console.log(`RPC:     ${rpcUrl.replace(/api-key=[^&]+/, "api-key=***")}`);
  console.log(`API:     ${API_BASE}`);
  console.log(
    `wallet:  ${clientPk} (${(bal / LAMPORTS_PER_SOL).toFixed(4)} SOL)`
  );

  let catalog = loadCatalog();
  const taskArg = process.argv.find((a) => a.startsWith("--tasks="));
  const isNewOnly = process.argv.includes("--new");

  if (taskArg) {
    const allowed = new Set(
      taskArg
        .replace("--tasks=", "")
        .split(",")
        .map((s) => s.trim())
    );
    catalog = catalog.filter((e) => allowed.has(e.name));
  } else if (isNewOnly) {
    const newSet = new Set([
      "weil-gram-cubic-correlation",
      "weil-gram-quartic-correlation",
      "matrix-cubic",
      "matrix-quartic",
    ]);
    catalog = catalog.filter((e) => newSet.has(e.name));
  }

  console.log(`catalog: ${catalog.length} statements`);
  const minLamports = Number(ESCROW.toString()) * (catalog.length + 2);
  if (bal < minLamports) {
    throw new Error(
      `Insufficient balance on ${network}: have ${(
        bal / LAMPORTS_PER_SOL
      ).toFixed(4)} SOL, need ${(minLamports / LAMPORTS_PER_SOL).toFixed(
        4
      )} SOL`
    );
  }

  const already = await existingTopics(bearer, network);
  let created = 0;
  let skipped = 0;

  for (const entry of catalog) {
    const topic = `catalog:${entry.name}`;
    if (already.has(topic)) {
      console.log(`  skip (exists): ${topic}`);
      skipped++;
      continue;
    }
    const camp = await api("POST", `/campaigns?network=${network}`, bearer, {
      brief: {
        topic,
        brand_voice: "rigorous",
        cta: `Prove: ${entry.name} (${entry.kind})`,
        utm_link: "https://swarm.tips",
      },
      budget_lamports: Number(ESCROW.toString()) * 2,
      platform: LEAN_PROOF_PLATFORM,
      statement_lean: entry.statement,
      lean_policy: 2,
    });
    const campaignId = camp.campaign_id as string;

    // Fund one task so the bounty is claimable + visible on the board.
    const fund = await api(
      "POST",
      `/campaigns/${campaignId}/fund?network=${network}`,
      bearer,
      { amount_lamports: Number(ESCROW.toString()) }
    );
    const unsigned = fund.transaction as string;
    const sig = await broadcast(conn, signBase64(unsigned, authority));
    await api(
      "POST",
      `/tasks/${fund.task_id}/confirm?network=${network}`,
      bearer,
      {
        tx_signature: sig,
        action: "create",
        task_pda: fund.task_pda,
      }
    );
    console.log(
      `  [${network}] seeded ${entry.kind}/${entry.name} -> campaign ${campaignId} task ${fund.task_id}`
    );
    created++;
  }

  console.log(`\nseed complete: ${created} created, ${skipped} skipped`);
}

const isDirectRun =
  process.argv[1]?.endsWith("seed-lean-catalog.ts") ||
  process.argv[1]?.endsWith("seed-lean-catalog");

if (isDirectRun) {
  main().catch((e) => {
    console.error(e);
    process.exit(1);
  });
}
