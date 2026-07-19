/**
 * Live devnet e2e for the autonomous shillbot-worker's LeanProof (platform 10)
 * path: a FRESH external client wallet creates + funds a LeanProof task with a
 * trivially-true statement; from confirm(create) on, the deployed stack must
 * CONSTRUCT a valid Lean 4 proof and complete the task with ZERO harness action:
 *
 *   confirm(create) → dispatch workflow → worker: checks → claim
 *     → proof search (ladder `by trivial` passes → lean-runner validates)
 *     → upload proof.lean to GCS → submit_work(raw https .lean url)
 *     → confirm_submit ensures shillbot-attestation-pipeline
 *     → attester re-checks the proof + lands verify_task_attested → finalize
 *
 * PASS: task reaches `submitted` with agent == worker wallet and a content_id
 * that is a raw storage.googleapis.com .lean URL whose body defines `proof`.
 * verified/finalized asserted opportunistically.
 *
 * The statement `def statementProp : Prop := True` is provable by the worker's
 * first ladder tactic (`by trivial`), so this exercises the full path without
 * depending on the LLM.
 *
 * MANUAL-ONLY. Run:
 *   SHILLBOT_API=https://api.shillbot.org \
 *     npx tsx tests/live/shillbot-worker-lean-e2e-devnet.ts
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
const LEANPROOF_PLATFORM = 10;
// Default is trivially-true (ladder `by trivial` solves it). Override with a
// statement the ladder (trivial|rfl|simp|decide|norm_num|tauto) can't close to
// force the worker's Grok proof-construction path, e.g.
//   STATEMENT='def statementProp : Prop := ∀ a b : Nat, a + b = b + a'
// (add_comm is not a default simp lemma → ladder misses it → Grok must prove it).
const STATEMENT = process.env.STATEMENT ?? "def statementProp : Prop := True";
const LEAN_POLICY = process.env.LEAN_POLICY
  ? Number(process.env.LEAN_POLICY)
  : undefined;
const BUDGET_LAMPORTS = 20_000_000;
const CLIENT_KEYFILE = `${__dirname}/.worker-e2e-client.json`; // shared with the website e2e

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
  const sig = await conn.sendRawTransaction(Buffer.from(signedB64, "base64"), {
    skipPreflight: false,
    maxRetries: 5,
  });
  await conn.confirmTransaction(sig, "confirmed");
  return sig;
}

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

  if (
    (await conn.getBalance(client.publicKey)) <
    BUDGET_LAMPORTS + 20_000_000
  ) {
    await sendAndConfirmTransaction(
      conn,
      new Transaction().add(
        SystemProgram.transfer({
          fromPubkey: funder.publicKey,
          toPubkey: client.publicKey,
          lamports: BUDGET_LAMPORTS + 30_000_000,
        })
      ),
      [funder]
    );
    console.log("funded client");
  }

  // 1) LeanProof campaign (statement_lean set) as the external client.
  const camp = await api("POST", `/campaigns?network=devnet`, clientPk, {
    brief: {
      topic: "autonomous worker lean-proof e2e",
      brand_voice: "precise",
      cta: "verify the proof",
      utm_link: "https://swarm.tips",
    },
    budget_lamports: BUDGET_LAMPORTS,
    platform: LEANPROOF_PLATFORM,
    statement_lean: STATEMENT,
    ...(LEAN_POLICY !== undefined ? { lean_policy: LEAN_POLICY } : {}),
  });
  const campaignId = camp.campaign_id as string;
  console.log(`campaign: ${campaignId}`);

  // 2) Fund → client-signed create_task → confirm. Last harness action.
  const fund = await api(
    "POST",
    `/campaigns/${campaignId}/fund?network=devnet`,
    clientPk,
    {
      amount_lamports: BUDGET_LAMPORTS,
    }
  );
  const taskId = fund.task_id as string;
  const taskPda = fund.task_pda as string | undefined;
  const sig = await broadcast(
    conn,
    signBase64(fund.transaction as string, client)
  );
  await api("POST", `/tasks/${taskId}/confirm?network=devnet`, clientPk, {
    tx_signature: sig,
    action: "create",
    ...(taskPda ? { task_pda: taskPda } : {}),
  });
  console.log(`task created + confirmed: ${taskId}`);
  console.log("hands off — worker must construct + validate a Lean proof...");

  // 3) Poll to submitted (proof construction is the slow part — runner check
  //    of a trivial proof is a few seconds, but claim+build+upload adds up).
  const deadline = Date.now() + 15 * 60 * 1000;
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
  if (agent !== WORKER_WALLET)
    throw new Error(`FAIL: agent ${agent} != worker wallet`);
  if (!/^https:\/\/storage\.googleapis\.com\/.*\.lean$/.test(contentId)) {
    throw new Error(
      `FAIL: content_id is not a raw GCS .lean URL: ${contentId}`
    );
  }

  // 4) The proof artifact must define `proof` at statementProp.
  const proof = await fetch(contentId).then((r) => r.text());
  if (!/theorem\s+proof\s*:/.test(proof)) {
    throw new Error(
      `FAIL: artifact does not define 'theorem proof': ${proof.slice(0, 120)}`
    );
  }
  console.log(`proof artifact OK:\n---\n${proof.trim()}\n---`);

  // 5) Opportunistic: attestation + finalize.
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
    `\nPASS — autonomous worker CONSTRUCTED a Lean proof for an external ` +
      `client's task: claimed + proved + submitted by ${WORKER_WALLET} with ` +
      `zero harness action after create. Final observed state: ${task.state}.`
  );
}

main().catch((e) => {
  console.error(String(e?.message ?? e));
  process.exit(1);
});
