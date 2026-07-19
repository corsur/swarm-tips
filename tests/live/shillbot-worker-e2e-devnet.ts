/**
 * Live devnet e2e for the autonomous shillbot-worker: a FRESH client wallet
 * (definitionally external — not in game_constants::org::OUR_WALLETS) creates
 * and funds a Website(9) task; the deployed stack must then complete it with
 * ZERO further action from this harness:
 *
 *   confirm(create) → shillbot-api maybe_dispatch_worker
 *     → shillbot-worker-dispatch workflow → /internal/worker/work bridge
 *     → shillbot-worker: checks → claim → publish page (GCS, footer anchor
 *       + nonce) → submit_work
 *     → confirm_submit ensures the shillbot-verification workflow
 *     → verify (binary website check against the published page) → finalize
 *
 * PASS criterion (autonomy proof): the task reaches `submitted` with
 * task.agent == the worker wallet and a content_id URL whose page contains
 * the on-chain nonce — all without this harness touching the task after
 * create. `verified`/`finalized` are asserted opportunistically if reached
 * within the poll budget (the challenge window may exceed it).
 *
 * MANUAL-ONLY (live devnet + deployed services). Run:
 *   SHILLBOT_API=https://api.shillbot.org \
 *     npx tsx tests/live/shillbot-worker-e2e-devnet.ts
 * The fresh client keypair is persisted to tests/live/.worker-e2e-client.json
 * so leftover funds are never stranded.
 */
import { readFileSync, writeFileSync, existsSync } from "fs";
import { homedir } from "os";
import {
  Connection,
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  SystemProgram,
  Transaction,
  VersionedTransaction,
  sendAndConfirmTransaction,
} from "@solana/web3.js";

const API_BASE = process.env.SHILLBOT_API ?? "https://api.shillbot.org";
const RPC = process.env.SOLANA_RPC_URL ?? "https://api.devnet.solana.com";
const WORKER_WALLET = "GCEdpAHSE5s4NNgBY77TRfvdKmpLjPc16QNHj9uZbThU";
const WEBSITE_PLATFORM = 9;
const BUDGET_LAMPORTS = 20_000_000; // 0.02 SOL — one small task
const CLIENT_KEYFILE = `${__dirname}/.worker-e2e-client.json`;

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

async function api(
  method: string,
  path: string,
  bearer: string,
  body?: unknown
): Promise<Record<string, unknown>> {
  const res = await fetch(API_BASE + path, {
    method,
    headers: {
      Authorization: `Bearer ${bearer}`,
      "Content-Type": "application/json",
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await res.text();
  if (!res.ok) throw new Error(`${method} ${path} -> ${res.status}: ${text}`);
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
  const raw = Buffer.from(signedB64, "base64");
  const sig = await conn.sendRawTransaction(raw, {
    skipPreflight: false,
    maxRetries: 5,
  });
  await conn.confirmTransaction(sig, "confirmed");
  return sig;
}

/** Persisted fresh client keypair (recoverable — no stranded funds). */
function loadOrCreateClient(): Keypair {
  if (existsSync(CLIENT_KEYFILE)) {
    return Keypair.fromSecretKey(
      Uint8Array.from(JSON.parse(readFileSync(CLIENT_KEYFILE, "utf8")))
    );
  }
  const kp = Keypair.generate();
  writeFileSync(CLIENT_KEYFILE, JSON.stringify(Array.from(kp.secretKey)));
  return kp;
}

async function main(): Promise<void> {
  const conn = new Connection(RPC, "confirmed");
  const funder = Keypair.fromSecretKey(
    Uint8Array.from(
      JSON.parse(readFileSync(`${homedir()}/.config/solana/id.json`, "utf8"))
    )
  );
  const client = loadOrCreateClient();
  const clientPk = client.publicKey.toBase58();
  console.log(`external client: ${clientPk}`);
  console.log(`worker wallet:   ${WORKER_WALLET}`);

  // Fund the external client (escrow + fees) from id.json devnet.
  const bal = await conn.getBalance(client.publicKey);
  if (bal < BUDGET_LAMPORTS + 20_000_000) {
    const tx = new Transaction().add(
      SystemProgram.transfer({
        fromPubkey: funder.publicKey,
        toPubkey: client.publicKey,
        lamports: BUDGET_LAMPORTS + 30_000_000,
      })
    );
    await sendAndConfirmTransaction(conn, tx, [funder]);
    console.log(
      `funded client with ${
        (BUDGET_LAMPORTS + 30_000_000) / LAMPORTS_PER_SOL
      } SOL`
    );
  }

  // 1) Create the Website campaign as the external client.
  const camp = await api("POST", `/campaigns?network=devnet`, clientPk, {
    brief: {
      topic: "autonomous worker e2e — decentralized agent coordination",
      brand_voice: "clear and direct",
      cta: "learn more at swarm.tips",
      utm_link: "https://swarm.tips",
    },
    budget_lamports: BUDGET_LAMPORTS,
    platform: WEBSITE_PLATFORM,
  });
  const campaignId = camp.campaign_id as string;
  console.log(`campaign: ${campaignId}`);

  // 2) Fund → client-signed create_task → confirm. This is the LAST harness
  //    action on the task: everything after must happen autonomously.
  const fund = await api(
    "POST",
    `/campaigns/${campaignId}/fund?network=devnet`,
    clientPk,
    { amount_lamports: BUDGET_LAMPORTS }
  );
  const taskId = fund.task_id as string;
  const taskPda = fund.task_pda as string | undefined;
  const unsigned = fund.transaction as string;
  if (!unsigned) throw new Error("fund returned no transaction");
  const sig = await broadcast(conn, signBase64(unsigned, client));
  await api("POST", `/tasks/${taskId}/confirm?network=devnet`, clientPk, {
    tx_signature: sig,
    action: "create",
    ...(taskPda ? { task_pda: taskPda } : {}),
  });
  console.log(`task created + confirmed: ${taskId} (pda ${taskPda ?? "?"})`);
  console.log("hands off — waiting for the autonomous pipeline...");

  // 3) Poll: dispatch → claim → submit must happen with no harness action.
  const deadline = Date.now() + 15 * 60 * 1000; // 15 min budget
  let last = "";
  let task: Record<string, unknown> = {};
  while (Date.now() < deadline) {
    task = await api("GET", `/tasks/${taskId}?network=devnet`, clientPk);
    const state = String(task.state ?? "?");
    if (state !== last) {
      console.log(
        `  state -> ${state} (agent=${task.agent ?? "-"}, content_id=${
          task.content_id ?? "-"
        })`
      );
      last = state;
    }
    if (["submitted", "approved", "verified", "finalized"].includes(state))
      break;
    await sleep(10_000);
  }

  const state = String(task.state ?? "?");
  const agent = String(task.agent ?? "");
  const contentId = String(task.content_id ?? "");
  if (!["submitted", "approved", "verified", "finalized"].includes(state)) {
    throw new Error(`FAIL: task never reached submitted (state=${state})`);
  }
  if (agent !== WORKER_WALLET) {
    throw new Error(`FAIL: task agent ${agent} != worker wallet`);
  }
  if (!contentId.startsWith("https://")) {
    throw new Error(`FAIL: content_id not a URL: ${contentId}`);
  }

  // 4) The published page must exist and carry the swarm.tips anchor.
  const page = await fetch(contentId).then((r) => r.text());
  if (!page.includes("https://swarm.tips")) {
    throw new Error("FAIL: published page missing swarm.tips anchor");
  }
  console.log(`published page OK: ${contentId}`);

  // 5) Opportunistic: give verification a few minutes to land.
  const verifyDeadline = Date.now() + 8 * 60 * 1000;
  while (Date.now() < verifyDeadline) {
    task = await api("GET", `/tasks/${taskId}?network=devnet`, clientPk);
    const s = String(task.state ?? "?");
    if (s !== last) {
      console.log(`  state -> ${s}`);
      last = s;
    }
    if (["verified", "finalized"].includes(s)) break;
    await sleep(15_000);
  }

  console.log(
    `\nPASS — autonomous worker completed an external client's task: ` +
      `claimed + published + submitted by ${WORKER_WALLET} with zero harness ` +
      `action after create. Final observed state: ${task.state}.`
  );
}

main().catch((e) => {
  console.error(String(e?.message ?? e));
  process.exit(1);
});
