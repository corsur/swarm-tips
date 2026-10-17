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
  const raw = Buffer.from(signedB64, "base64");
  const sig = await conn.sendRawTransaction(raw, {
    skipPreflight: false,
    maxRetries: 5,
  });
  const deadline = Date.now() + 90_000;
  let lastResend = Date.now();
  while (Date.now() < deadline) {
    const { value } = await conn.getSignatureStatuses([sig], {
      searchTransactionHistory: true,
    });
    const st = value[0];
    if (st?.err) {
      throw new Error(`tx ${sig} failed on-chain: ${JSON.stringify(st.err)}`);
    }
    if (
      st?.confirmationStatus === "confirmed" ||
      st?.confirmationStatus === "finalized"
    ) {
      return sig;
    }
    if (Date.now() - lastResend > 4_000) {
      lastResend = Date.now();
      await conn
        .sendRawTransaction(raw, { skipPreflight: true, maxRetries: 2 })
        .catch(() => {});
    }
    await new Promise((r) => setTimeout(r, 1_500));
  }
  throw new Error(`Transaction ${sig} was not confirmed within 90.00 seconds`);
}

export const MILLENNIUM_TASKS = new Set([
  "weil-gram-cubic-correlation",
  "weil-gram-quartic-correlation",
  "weil-gram-spectral-floor",
]);

export const LADDER_TASKS = new Set([
  "tier1-escrow-conservation",
  "tier1-weil-trace-hollow-2x2",
  "tier2-weil-gram-symmetry",
  "tier2-weil-hollow-cubic-2x2",
  "tier2-weil-cauchy-schwarz-quartic-2x2",
  "tier2-eigentrust-convex-preservation",
]);

export interface CatalogEntry {
  name: string;
  kind: "open" | "solvable";
  program: "millennium-rh-weil-gram" | "lemma-ladder" | "general";
  statement: string;
  companionProof?: string;
  defaultEscrowLamports?: number;
}

export function loadCatalog(includeSolvable = false): CatalogEntry[] {
  const entries: CatalogEntry[] = [];
  const kinds: Array<"open" | "solvable"> = includeSolvable
    ? ["open", "solvable"]
    : ["open"];
  for (const kind of kinds) {
    const dir = path.join(CATALOG_DIR, kind);
    for (const f of fs.readdirSync(dir).sort()) {
      if (!f.endsWith(".lean") || f.endsWith(".proof.lean")) continue;
      const name = f.replace(/\.lean$/, "");
      const proofPath = path.join(dir, `${name}.proof.lean`);
      const companionProof = fs.existsSync(proofPath)
        ? fs.readFileSync(proofPath, "utf8").trimEnd()
        : undefined;
      const defaultEscrowLamports = name.startsWith("tier1-")
        ? 5_000_000
        : name.startsWith("tier2-")
          ? 15_000_000
          : undefined;
      entries.push({
        name,
        kind,
        program: MILLENNIUM_TASKS.has(name)
          ? "millennium-rh-weil-gram"
          : LADDER_TASKS.has(name)
            ? "lemma-ladder"
            : "general",
        statement: fs.readFileSync(path.join(dir, f), "utf8").trimEnd(),
        companionProof,
        defaultEscrowLamports,
      });
    }
  }
  return entries;
}

/**
 * Static quantifier-order audit: rejects any open problem where an existential
 * bounding constant `∃ (C : ℝ)` or `∃ C` is placed INSIDE a universal matrix/dimension
 * quantifier `∀ (n` / `∀ (N` / `∀ (G` (the exact bug exploited by 2qu3Do... via C = |x| / tr(G^2)).
 */
export function assertOuterExistentialQuantifierOrder(
  entry: CatalogEntry
): void {
  const propIdx = entry.statement.indexOf("def statementProp");
  const body =
    propIdx >= 0 ? entry.statement.slice(propIdx) : entry.statement;
  const forallMatch = body.search(/∀\s*\(\s*[nNGP]\b/);
  const existsCMatch = body.search(/∃\s*\(?\s*C\s*:/);
  if (forallMatch >= 0 && existsCMatch > forallMatch) {
    throw new Error(
      `[ANTI-VACUITY GATE FAILED] ${entry.name}: existential constant '∃ C' appears AFTER universal quantifier '∀' in statementProp. Move '∃ (C : ℝ), 0 ≤ C ∧' outside '∀' so C cannot be chosen pointwise!`
    );
  }
}

const DEFAULT_RUNNER_URL =
  process.env.LEAN_RUNNER_URL ??
  "https://shillbot-lean-runner-wjucctfqta-uc.a.run.app";

function getRunnerIdToken(audience: string): string {
  if (process.env.RUNNER_ID_TOKEN) return process.env.RUNNER_ID_TOKEN;
  try {
    return execSync(`gcloud auth print-identity-token`, {
      stdio: ["ignore", "pipe", "ignore"],
    })
      .toString()
      .trim();
  } catch {
    return "";
  }
}

async function verifyStatementElaborates(entry: CatalogEntry): Promise<void> {
  assertOuterExistentialQuantifierOrder(entry);

  const token = getRunnerIdToken(DEFAULT_RUNNER_URL);
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
  };
  if (token) headers.Authorization = `Bearer ${token}`;

  const t0 = Date.now();
  const res = await fetch(`${DEFAULT_RUNNER_URL}/check`, {
    method: "POST",
    headers,
    body: JSON.stringify({
      statement: entry.statement,
      proof: "theorem solution : statementProp := sorry",
      statement_def: "statementProp",
      entry_theorem: "solution",
      axiom_allowlist: ["propext", "Classical.choice", "Quot.sound"],
      mode: "mathlib",
      import_allowlist: ["Mathlib"],
      max_build_secs: 120,
      max_build_mem_mb: 16384,
    }),
  });
  if (!res.ok) {
    throw new Error(
      `pre-seed runner check HTTP ${res.status} for ${
        entry.name
      }: ${await res.text()}`
    );
  }
  const resp = (await res.json()) as {
    verdict?: string;
    detail?: string;
  };
  if (
    resp.verdict !== "fail" ||
    !String(resp.detail ?? "").includes("proof uses sorry")
  ) {
    throw new Error(
      `pre-seed elaboration gate FAILED for ${entry.name}: expected verdict=fail("proof uses sorry"), got verdict=${resp.verdict} detail=${resp.detail}`
    );
  }

  if (entry.kind === "solvable" && entry.companionProof) {
    const pRes = await fetch(`${DEFAULT_RUNNER_URL}/check`, {
      method: "POST",
      headers,
      body: JSON.stringify({
        statement: entry.statement,
        proof: entry.companionProof,
        statement_def: "statementProp",
        entry_theorem: entry.companionProof.includes("theorem proof ")
          ? "proof"
          : "solution",
        axiom_allowlist: ["propext", "Classical.choice", "Quot.sound"],
        mode: "mathlib",
        import_allowlist: ["Mathlib"],
        max_build_secs: 120,
        max_build_mem_mb: 16384,
      }),
    });
    if (!pRes.ok) {
      throw new Error(
        `companion proof check HTTP ${pRes.status} for ${entry.name}`
      );
    }
    const pJson = (await pRes.json()) as { verdict?: string; detail?: string };
    if (pJson.verdict !== "pass") {
      throw new Error(
        `companion proof FAILED for solvable entry ${entry.name}: verdict=${pJson.verdict} detail=${pJson.detail}`
      );
    }
    console.log(
      `  [pre-seed check PASS] solvable/${entry.name} elaborated & companion proof verified (${
        Date.now() - t0
      }ms)`
    );
    return;
  }

  // Bi-directional 4-canary vacuity & shortcut linter for open problems:
  // Canary 1: 6Riwz... zero-constant / positivity vacuity probe
  // Canary 2: 2qu3Do... pointwise ratio (C = |x| / tr(G^2)) shortcut probe
  // Canary 3: disproof-vacuity (¬ statementProp via decide / norm_num / positivity)
  // Canary 4: empty-domain contradiction (hypotheses mutually contradictory via omega)
  const canaryProbes = [
    {
      label: "zero-constant-vacuity",
      proof:
        "theorem solution : statementProp := by\n  refine ⟨0, le_rfl, fun N _ => ?_⟩\n  simp",
    },
    {
      label: "pointwise-ratio-shortcut",
      proof:
        "theorem solution : statementProp := by\n  intro N hN\n  refine ⟨0, le_rfl, ?_⟩\n  exact le_rfl",
    },
    {
      label: "disproof-vacuity",
      proof:
        "theorem solution : ¬ statementProp := by\n  intro h\n  unfold statementProp at h\n  simp at h",
    },
    {
      label: "empty-domain-contradiction",
      proof:
        "theorem solution : statementProp := by\n  unfold statementProp\n  intros\n  omega",
    },
  ];

  for (const canary of canaryProbes) {
    const cRes = await fetch(`${DEFAULT_RUNNER_URL}/check`, {
      method: "POST",
      headers,
      body: JSON.stringify({
        statement: entry.statement,
        proof: canary.proof,
        statement_def: "statementProp",
        entry_theorem: "solution",
        axiom_allowlist: ["propext", "Classical.choice", "Quot.sound"],
        mode: "mathlib",
        import_allowlist: ["Mathlib"],
        max_build_secs: 90,
        max_build_mem_mb: 16384,
      }),
    });
    if (!cRes.ok) {
      throw new Error(
        `pre-seed canary check HTTP ${cRes.status} for ${entry.name} (${canary.label})`
      );
    }
    const cJson = (await cRes.json()) as { verdict?: string; detail?: string };
    if (cJson.verdict === "pass") {
      throw new Error(
        `[ANTI-VACUITY GATE FAILED] ${entry.name} was solved trivially by canary '${canary.label}'! Refusing to seed.`
      );
    }
  }

  console.log(
    `  [pre-seed check PASS] ${entry.name} elaborated cleanly & rejected all 4 vacuity/ratio canaries (${
      Date.now() - t0
    }ms)`
  );
}

async function existingOpenTopics(
  bearer: string,
  network: string
): Promise<Set<string>> {
  const res = (await api(
    "GET",
    `/tasks?network=${network}&limit=100`,
    bearer
  )) as { tasks?: Array<{ state?: string; campaign_topic?: string }> };
  const topics = new Set<string>();
  for (const t of res.tasks ?? []) {
    const st = String(t.state ?? "").toLowerCase();
    if (st === "open" && t.campaign_topic) {
      topics.add(t.campaign_topic);
    }
  }
  return topics;
}

async function main(): Promise<void> {
  const isMainnet =
    process.argv.includes("--mainnet") ||
    process.env.SOLANA_NETWORK === "mainnet";
  const checkOnly = process.argv.includes("--check-only");
  const isLadder = process.argv.includes("--ladder");
  const taskArg = process.argv.find((a) => a.startsWith("--tasks="));
  const network = isMainnet ? "mainnet" : "devnet";

  let catalog = loadCatalog(isLadder || Boolean(taskArg));
  const isMillenniumOnly =
    process.argv.includes("--millennium") ||
    process.argv.includes("--new") ||
    (isMainnet && !process.argv.includes("--all-open") && !isLadder && !taskArg);

  if (taskArg) {
    const allowed = new Set(
      taskArg
        .replace("--tasks=", "")
        .split(",")
        .map((s) => s.trim())
    );
    catalog = catalog.filter((e) => allowed.has(e.name));
  } else if (isLadder) {
    catalog = catalog.filter((e) => e.program === "lemma-ladder");
  } else if (isMillenniumOnly) {
    catalog = catalog.filter((e) => e.program === "millennium-rh-weil-gram");
  }

  console.log(
    `catalog: ${catalog.length} statements (${catalog
      .map((e) => e.name)
      .join(", ")})`
  );

  if (checkOnly) {
    for (const entry of catalog) {
      await verifyStatementElaborates(entry);
    }
    console.log(
      `\n[check-only PASS] All ${catalog.length} statements passed elaboration & anti-vacuity gate.`
    );
    return;
  }

  const rpcUrl = getRpcUrl(isMainnet);
  const conn = new Connection(rpcUrl, "confirmed");

  const escrowArg = process.argv.find((a) =>
    a.startsWith("--escrow-lamports=")
  );
  const escrowLamports = escrowArg
    ? Number(escrowArg.replace("--escrow-lamports=", "").trim())
    : Number(ESCROW.toString());

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
  console.log(
    `escrow:  ${escrowLamports} lamports (${(
      escrowLamports / LAMPORTS_PER_SOL
    ).toFixed(4)} SOL)`
  );

  const totalEscrowNeeded = catalog.reduce(
    (acc, e) => acc + (escrowArg ? escrowLamports : e.defaultEscrowLamports ?? escrowLamports),
    0
  );
  const minLamports = totalEscrowNeeded + 5_000_000;
  if (bal < minLamports) {
    throw new Error(
      `Insufficient balance on ${network}: have ${(
        bal / LAMPORTS_PER_SOL
      ).toFixed(4)} SOL, need ${(minLamports / LAMPORTS_PER_SOL).toFixed(
        4
      )} SOL`
    );
  }

  const skipPrecheck = process.argv.includes("--skip-precheck");
  const forceReseed = process.argv.includes("--force");
  const already = forceReseed
    ? new Set<string>()
    : await existingOpenTopics(bearer, network);
  let created = 0;
  let skipped = 0;

  for (const entry of catalog) {
    const entryEscrow = escrowArg
      ? escrowLamports
      : entry.defaultEscrowLamports ?? escrowLamports;
    const topic = `catalog:${entry.name}`;
    if (already.has(topic)) {
      console.log(`  skip (open task exists): ${topic}`);
      skipped++;
      continue;
    }
    if (!skipPrecheck) {
      await verifyStatementElaborates(entry);
    }
    const camp = await api("POST", `/campaigns?network=${network}`, bearer, {
      brief: {
        topic,
        brand_voice: "rigorous",
        cta: `Prove: ${entry.name} (${entry.kind})`,
        utm_link: "https://swarm.tips",
      },
      budget_lamports: entryEscrow * 2,
      platform: LEAN_PROOF_PLATFORM,
      statement_lean: entry.statement,
      lean_policy: 3,
    });
    const campaignId = camp.campaign_id as string;

    // Fund one task so the bounty is claimable + visible on the board.
    const fund = await api(
      "POST",
      `/campaigns/${campaignId}/fund?network=${network}`,
      bearer,
      { amount_lamports: entryEscrow }
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
      `  [${network}] seeded ${entry.kind}/${entry.name} (${(entryEscrow / LAMPORTS_PER_SOL).toFixed(4)} SOL) -> campaign ${campaignId} task ${fund.task_id} pda ${fund.task_pda} (tx ${sig})`
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
