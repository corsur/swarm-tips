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
import { BN } from "@coral-xyz/anchor";
import {
  Connection,
  Keypair,
  Transaction,
  VersionedTransaction,
} from "@solana/web3.js";
import { connectDevnet, ensureFunded } from "./devnet-harness";
import { LEAN_PROOF_PLATFORM } from "../../tests/harness/shillbot-steps";

const API_BASE = process.env.SHILLBOT_API ?? "https://api.shillbot.org";
// Small standing escrow per bounty (~0.002 SOL). Open problems and solvables use
// the same escrow on devnet; the point is a visible board entry, not a prize.
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

interface CatalogEntry {
  name: string;
  kind: "open" | "solvable";
  statement: string;
}

function loadCatalog(): CatalogEntry[] {
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

async function existingTopics(clientPk: string): Promise<Set<string>> {
  const list = (await api(
    "GET",
    `/campaigns?network=devnet`,
    clientPk
  )) as unknown as Array<{ brief?: { topic?: string } }>;
  const topics = new Set<string>();
  for (const c of Array.isArray(list) ? list : []) {
    if (c.brief?.topic) topics.add(c.brief.topic);
  }
  return topics;
}

async function main(): Promise<void> {
  const h = connectDevnet();
  const clientPk = h.authority.publicKey.toBase58();
  console.log(`API:    ${API_BASE}`);
  console.log(`client: ${clientPk}`);

  const catalog = loadCatalog();
  console.log(`catalog: ${catalog.length} statements`);
  await ensureFunded(
    h,
    h.authority.publicKey,
    Number(ESCROW.toString()) * (catalog.length + 2)
  );

  const already = await existingTopics(clientPk);
  let created = 0;
  let skipped = 0;

  for (const entry of catalog) {
    const topic = `catalog:${entry.name}`;
    if (already.has(topic)) {
      console.log(`  skip (exists): ${topic}`);
      skipped++;
      continue;
    }
    const camp = await api("POST", `/campaigns?network=devnet`, clientPk, {
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
      `/campaigns/${campaignId}/fund?network=devnet`,
      clientPk,
      { amount_lamports: Number(ESCROW.toString()) }
    );
    const unsigned = fund.transaction as string;
    const sig = await broadcast(
      h.connection,
      signBase64(unsigned, h.authority)
    );
    await api(
      "POST",
      `/tasks/${fund.task_id}/confirm?network=devnet`,
      clientPk,
      {
        tx_signature: sig,
        action: "create",
        task_pda: fund.task_pda,
      }
    );
    console.log(
      `  seeded ${entry.kind}/${entry.name} -> campaign ${campaignId} task ${fund.task_id}`
    );
    created++;
  }

  console.log(`\nseed complete: ${created} created, ${skipped} skipped`);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
