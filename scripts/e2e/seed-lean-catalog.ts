/**
 * Seed the LeanProof bounty catalog onto DEVNET as policy-4 (composable) campaigns.
 *
 * For every statement in tests/fixtures/lean/catalog/{open,solvable}/*.lean it
 * creates a `lean_policy: 4` LeanProof campaign (statement_lean = the file bytes)
 * and funds one task, so the board shows real standing bounties. Idempotent: a
 * campaign is tagged `brief.topic = "catalog:<name>"`; a re-run skips any tag
 * that already exists among the client's campaigns.
 *
 * Ladder rungs (`chain-*`) import shared chain definitions from `SwarmProofs.Chains.*`
 * and declare `-- @depends-on: <prev-rung>` on Rung N+1. Seeding refuses to post
 * Rung N+1 until Rung N is promoted and `active` in `proofs/SwarmProofs/registry.json`,
 * and refuses `lean_policy: 4` unless runner `/health` reports `composable: true`.
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
export const REPO_ROOT = path.join(__dirname, "..", "..");
export const CATALOG_DIR = path.join(
  REPO_ROOT,
  "tests",
  "fixtures",
  "lean",
  "catalog"
);
export const SWARM_PROOFS_ROOT = path.join(REPO_ROOT, "proofs", "SwarmProofs");
export const DEFAULT_REGISTRY_PATH = path.join(
  SWARM_PROOFS_ROOT,
  "registry.json"
);

export const ALLOWED_SWARM_LIBRARY_IMPORTS = new Set([
  "SwarmProofs",
  "SwarmProofs.Registry",
  "SwarmProofs.Chains.BSD.Defs",
  "SwarmProofs.Chains.Circuits.Defs",
  "SwarmProofs.Chains.RHAnalytic.Defs",
  "SwarmProofs.Chains.RHLagarias.Defs",
]);

export const FORBIDDEN_COMPOSITION_SUBSTRINGS = [
  "axiom ",
  "sorry",
  "admit",
  "unsafe ",
  "partial ",
  "opaque ",
  "extern ",
  "implemented_by",
  "csimp",
  "macro ",
  "macro_rules",
  "syntax ",
  "elab ",
  "elab_rules",
  "command_elab",
  "term_elab",
  "notation ",
  "infix",
  "prefix ",
  "postfix ",
  "attribute ",
  "variable ",
  "variable(",
  "variable[",
  "variable{",
  "run_tac",
  "run_elab",
  "by_elab",
  "native_decide",
  "#eval",
  "#check",
  "#print",
  "#reduce",
  "#synth",
  "namespace ",
  "section ",
  "local ",
  "scoped ",
] as const;

export const PROBE_MATHLIB_UMBRELLAS = [
  "Mathlib.NumberTheory.Primorial",
  "Mathlib.NumberTheory.ArithmeticFunction",
  "Mathlib.NumberTheory.Divisors",
  "Mathlib.NumberTheory.LegendreSymbol.QuadraticReciprocity",
  "Mathlib.Analysis.SpecialFunctions.Log.Basic",
  "Mathlib.Analysis.SpecialFunctions.Pow.Real",
  "Mathlib.Algebra.BigOperators.Group.Finset.Basic",
  "Mathlib.Data.Fintype.Basic",
  "Mathlib.Order.Filter.Ultrafilter.Basic",
  "Mathlib.Computability.PartrecCode",
  "Mathlib.Computability.RE",
  "Mathlib.SetTheory.ZFC.Cardinal",
  "Mathlib.Tactic.Linarith",
  "Mathlib.Tactic.NormNum",
  "Mathlib.Tactic.Positivity",
  "Mathlib.Tactic.Ring",
  "Mathlib.Tactic.Tauto",
] as const;

export const MALFORMED_PROBE_DETAIL_MARKERS = [
  "does not exist",
  "unknown tactic",
  "unknown module prefix",
  "forbidden token",
] as const;

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
    JSON.stringify({
      wallet,
      exp: now + 86400,
      typ: "user",
      aud: ["shillbot", "game", "mcp"],
    })
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
  body?: unknown,
  fetchImpl: typeof fetch = fetch,
  apiBase: string = API_BASE
): Promise<Record<string, unknown>> {
  const res = await fetchImpl(apiBase + path_, {
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
  "millennium-rh-lagarias-divisor",
  "millennium-bsd-congruent-squarefree",
  "millennium-ns-dyadic-enstrophy-regularity",
  "millennium-sub-ns-abel-helical-cancellation",
  "millennium-sub-rh-lagarias-superabundant",
  "millennium-sub-bsd-split-prime-frobenius",
  "millennium-sub-pnp-paley-rigidity",
]);

export const LADDER_TASKS = new Set([
  "tier1-escrow-conservation",
  "tier1-weil-trace-hollow-2x2",
  "tier2-weil-gram-symmetry",
  "tier2-weil-hollow-cubic-2x2",
  "tier2-weil-cauchy-schwarz-quartic-2x2",
  "tier2-eigentrust-convex-preservation",
  "tier2-rh-mertens-base",
  "tier2-bsd-congruent-elliptic-map",
  "tier2-ns-dyadic-energy-conservation",
]);

export interface ChainTaskMeta {
  escrowLamports: number;
  cta: string;
  dependsOn?: string;
}

const SPDX_CTA_NOTE =
  "(-- SPDX-License-Identifier: Apache-2.0 header required)";

export const CHAIN_TASK_CONFIG: Record<string, ChainTaskMeta> = {
  "chain-rh-lagarias-1-superabundant-even": {
    escrowLamports: 10_000_000,
    cta: `Chain A [Rung 1/4 -> RH Lagarias]: Prove superabundant n >= 2 is even + base n = 1 ${SPDX_CTA_NOTE}`,
  },
  "chain-rh-lagarias-2-normalized-monotonicity": {
    escrowLamports: 20_000_000,
    dependsOn: "chain-rh-lagarias-1-superabundant-even",
    cta: `Chain A [Rung 2/4 -> RH Lagarias]: Prove normalized Lagarias RHS g(k) monotonicity ${SPDX_CTA_NOTE}`,
  },
  "chain-rh-lagarias-3-odd-and-squarefree": {
    escrowLamports: 35_000_000,
    dependsOn: "chain-rh-lagarias-2-normalized-monotonicity",
    cta: `Chain A [Rung 3/4 -> RH Lagarias]: Prove Lagarias RH bound for all odd & squarefree n ${SPDX_CTA_NOTE}`,
  },
  "chain-rh-lagarias-4-even-superabundant-core": {
    escrowLamports: 60_000_000,
    dependsOn: "chain-rh-lagarias-3-odd-and-squarefree",
    cta: `Chain A [Rung 4/4 -> RH Lagarias]: Open core on even non-squarefree superabundant n > 5040 ${SPDX_CTA_NOTE}`,
  },
  "chain-bsd-1-weierstrass-bijection": {
    escrowLamports: 12_000_000,
    cta: `Chain B [Rung 1/4 -> BSD Congruent]: Prove Koblitz bijection isCongruentNumber n <-> y^2 = x^3 - n^2 x ${SPDX_CTA_NOTE}`,
  },
  "chain-bsd-2-fermat-noncongruent-1": {
    escrowLamports: 25_000_000,
    dependsOn: "chain-bsd-1-weierstrass-bijection",
    cta: `Chain B [Rung 2/4 -> BSD Congruent]: Prove Fermat right-triangle theorem ¬isCongruentNumber 1 ${SPDX_CTA_NOTE}`,
  },
  "chain-bsd-3-heegner-monsky-primes": {
    escrowLamports: 50_000_000,
    dependsOn: "chain-bsd-2-fermat-noncongruent-1",
    cta: `Chain B [Rung 3/4 -> BSD Congruent]: Prove Heegner-Monsky theorem for primes p ≡ 5, 7 (mod 8) ${SPDX_CTA_NOTE}`,
  },
  "chain-bsd-4-congruent-semiprimes-567": {
    escrowLamports: 65_000_000,
    dependsOn: "chain-bsd-3-heegner-monsky-primes",
    cta: `Chain B [Rung 4/4 -> BSD Congruent]: Open semiprime BSD slice pq ≡ 5, 6, 7 (mod 8) ${SPDX_CTA_NOTE}`,
  },
  "chain-rh-analytic-1-mobius-floor-identity": {
    escrowLamports: 15_000_000,
    cta: `Chain C [Rung 1/4 -> RH Analytic]: Prove Meissel Mobius floor-sum identity sum_{k<=N} μ(k)*floor(N/k) = 1 ${SPDX_CTA_NOTE}`,
  },
  "chain-rh-analytic-2-mobius-harmonic-bound": {
    escrowLamports: 20_000_000,
    dependsOn: "chain-rh-analytic-1-mobius-floor-identity",
    cta: `Chain C [Rung 2/4 -> RH Analytic]: Prove Mobius harmonic cancellation (sum_{n<=N} μ(n)/n)^2 <= 1 ${SPDX_CTA_NOTE}`,
  },
  "chain-rh-analytic-3-pnt-mertens-little-o": {
    escrowLamports: 45_000_000,
    dependsOn: "chain-rh-analytic-2-mobius-harmonic-bound",
    cta: `Chain C [Rung 3/4 -> RH Analytic]: Prove Landau PNT equivalence M(N) = o(N) ${SPDX_CTA_NOTE}`,
  },
  "chain-rh-analytic-4-zero-free-strip-99-100": {
    escrowLamports: 65_000_000,
    dependsOn: "chain-rh-analytic-3-pnt-mertens-little-o",
    cta: `Chain C [Rung 4/4 -> RH Analytic]: Open zero-free strip Re(s) > 0.99 (M(N)^100 <= C * N^99) ${SPDX_CTA_NOTE}`,
  },
  "chain-pnp-circuit-1-de-morgan-parity-lower-bound": {
    escrowLamports: 20_000_000,
    cta: `Track D [Rung 1/2 -> Circuit Lower Bounds]: Prove De Morgan parity circuit size s >= 2n - 1 (provable landmark) ${SPDX_CTA_NOTE}`,
  },
  "chain-pnp-circuit-2-shannon-counting-bound": {
    escrowLamports: 30_000_000,
    dependsOn: "chain-pnp-circuit-1-de-morgan-parity-lower-bound",
    cta: `Track D [Rung 2/2 -> Circuit Lower Bounds]: Prove Shannon exponential circuit bound s > 2^n/(4n) over B_2 (provable landmark) ${SPDX_CTA_NOTE}`,
  },
  "chain-rh-meta-1-lagarias-pi1-rational-envelope": {
    escrowLamports: 25_000_000,
    cta: `Track E [Rung 1/4 -> Pi1 RH Metamathematics]: Prove constructive rational Taylor/artanh envelope equivalence for Lagarias RH (provable landmark) ${SPDX_CTA_NOTE}`,
  },
  "chain-rh-meta-2-nonstandard-ultrapower-pi1": {
    escrowLamports: 20_000_000,
    dependsOn: "chain-rh-meta-1-lagarias-pi1-rational-envelope",
    cta: `Track E [Rung 2/4 -> Pi1 RH Metamathematics]: Prove Los Pi1 ultrapower invariance vs hyperfinite counterexamples in stage ultraproducts (provable landmark) ${SPDX_CTA_NOTE}`,
  },
  "chain-rh-meta-3-godel-kleene-pi1-unprovability": {
    escrowLamports: 30_000_000,
    dependsOn: "chain-rh-meta-2-nonstandard-ultrapower-pi1",
    cta: `Track E [Rung 3/4 -> Pi1 RH Metamathematics]: Prove Partrec Pi1 counterexample compiler, RE unprovability & non-conservativity barrier (provable landmark) ${SPDX_CTA_NOTE}`,
  },
  "chain-rh-meta-4-zfc-inaccessible-universe-reflection": {
    escrowLamports: 35_000_000,
    dependsOn: "chain-rh-meta-3-godel-kleene-pi1-unprovability",
    cta: `Track E [Rung 4/4 -> Pi1 RH Metamathematics]: Prove Zermelo-Grothendieck equivalence IsInaccessible κ <-> V_{κ.ord} ZFC Replacement universe (provable landmark) ${SPDX_CTA_NOTE}`,
  },
};

export const REDUCTION_CHAIN_FILES = [
  "rh_lagarias_chain.lean",
  "bsd_congruent_chain.lean",
  "rh_analytic_chain.lean",
  "pnp_circuit_foundations_chain.lean",
  "rh_pi1_metamathematics_chain.lean",
];

export interface CatalogEntry {
  name: string;
  kind: "open" | "solvable";
  program:
    | "millennium-rh-weil-gram"
    | "millennium-chains"
    | "lemma-ladder"
    | "general";
  statement: string;
  companionProof?: string;
  sanityScriptPath?: string;
  defaultEscrowLamports?: number;
  customCta?: string;
  dependsOn?: string;
  declaredLeanPolicy?: 1 | 2 | 3 | 4;
}

export interface ProofRegistryModule {
  module_id: string;
  module_name: string;
  originating_task_id?: string;
  originating_network?: string;
  status?: "active" | "deprecated" | "revoked" | string;
}

export interface ProofRegistryManifest {
  schema: string;
  modules: ProofRegistryModule[];
  aliases: Record<string, string>;
}

export interface ResolvedRungDependency {
  status: "active" | "inactive" | "missing";
  moduleId?: string;
  moduleName?: string;
}

export interface RunnerHealthInfo {
  status?: string;
  composable?: boolean;
}

export function parseStatementHeaderMeta(statement: string): {
  leanPolicy?: 1 | 2 | 3 | 4;
  dependsOn?: string;
} {
  let leanPolicy: 1 | 2 | 3 | 4 | undefined;
  let dependsOn: string | undefined;
  for (const line of statement.split("\n")) {
    const trimmed = line.trim();
    if (!trimmed.startsWith("--")) {
      if (trimmed.length > 0) break;
      continue;
    }
    const meta = trimmed.slice(2).trim();
    if (meta.startsWith("@lean-policy:")) {
      const raw = Number(meta.slice("@lean-policy:".length).trim());
      if (![1, 2, 3, 4].includes(raw)) {
        throw new Error(`Invalid @lean-policy header value '${raw}'`);
      }
      leanPolicy = raw as 1 | 2 | 3 | 4;
    } else if (meta.startsWith("@depends-on:")) {
      const dep = meta.slice("@depends-on:".length).trim();
      if (dep.length > 0) dependsOn = dep;
    }
  }
  return { leanPolicy, dependsOn };
}

export function loadProofRegistry(
  registryPath: string = DEFAULT_REGISTRY_PATH
): ProofRegistryManifest {
  return JSON.parse(
    fs.readFileSync(registryPath, "utf8")
  ) as ProofRegistryManifest;
}

export function resolveRungDependency(
  dependsOn: string,
  registry: ProofRegistryManifest
): ResolvedRungDependency {
  const aliasKey = dependsOn.startsWith("catalog:")
    ? dependsOn
    : `catalog:${dependsOn}`;
  const targetId =
    registry.aliases?.[aliasKey] ?? registry.aliases?.[dependsOn];
  const matched = targetId
    ? registry.modules.find((m) => m.module_id === targetId)
    : registry.modules.find(
        (m) =>
          m.originating_task_id === dependsOn ||
          m.originating_task_id === aliasKey
      );
  if (!matched) {
    return { status: "missing" };
  }
  const st = (matched.status ?? "active").toLowerCase();
  return {
    status: st === "active" ? "active" : "inactive",
    moduleId: matched.module_id,
    moduleName: matched.module_name,
  };
}

export function assertSeedingAllowedForEntry(
  entry: CatalogEntry,
  leanPolicy: 1 | 2 | 3 | 4,
  registry: ProofRegistryManifest,
  runnerHealth: RunnerHealthInfo
): ResolvedRungDependency | undefined {
  if (leanPolicy === 4 && runnerHealth.composable !== true) {
    throw new Error(
      `[POLICY-4 SEED GUARD] Refusing to seed ${
        entry.name
      } with lean_policy=4: runner /health reports composable=${Boolean(
        runnerHealth.composable
      )}`
    );
  }
  if (!entry.dependsOn) {
    return undefined;
  }
  const resolved = resolveRungDependency(entry.dependsOn, registry);
  if (resolved.status !== "active") {
    throw new Error(
      `[LADDER DEPENDENCY GUARD] Refusing to seed ${entry.name}: prerequisite rung '${entry.dependsOn}' is ${resolved.status} in proofs/SwarmProofs/registry.json (must be promoted and active first)`
    );
  }
  return resolved;
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
      const sanityPath = path.join(dir, `${name}.sanity.ts`);
      const companionProof = fs.existsSync(proofPath)
        ? fs.readFileSync(proofPath, "utf8").trimEnd()
        : undefined;
      const sanityScriptPath = fs.existsSync(sanityPath)
        ? sanityPath
        : undefined;
      const statement = fs.readFileSync(path.join(dir, f), "utf8").trimEnd();
      const headerMeta = parseStatementHeaderMeta(statement);
      const chainMeta = CHAIN_TASK_CONFIG[name];
      if (
        headerMeta.dependsOn &&
        chainMeta?.dependsOn &&
        headerMeta.dependsOn !== chainMeta.dependsOn
      ) {
        throw new Error(
          `Mismatched dependsOn for ${name}: header='${headerMeta.dependsOn}' vs CHAIN_TASK_CONFIG='${chainMeta.dependsOn}'`
        );
      }
      const dependsOn = headerMeta.dependsOn ?? chainMeta?.dependsOn;
      const defaultEscrowLamports = chainMeta
        ? chainMeta.escrowLamports
        : name.startsWith("tier1-")
        ? 5_000_000
        : name.startsWith("tier2-")
        ? 15_000_000
        : name.startsWith("millennium-sub-")
        ? 30_000_000
        : name.startsWith("millennium-")
        ? 50_000_000
        : undefined;
      entries.push({
        name,
        kind,
        program: chainMeta
          ? "millennium-chains"
          : MILLENNIUM_TASKS.has(name)
          ? "millennium-rh-weil-gram"
          : LADDER_TASKS.has(name)
          ? "lemma-ladder"
          : "general",
        statement,
        companionProof,
        sanityScriptPath,
        defaultEscrowLamports,
        customCta: chainMeta?.cta,
        dependsOn,
        declaredLeanPolicy: headerMeta.leanPolicy,
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
  const body = propIdx >= 0 ? entry.statement.slice(propIdx) : entry.statement;
  const forallMatch = body.search(/∀\s*\(\s*[nNGP]\b/);
  const existsCMatch = body.search(/∃\s*\(?\s*C\s*:/);
  if (forallMatch >= 0 && existsCMatch > forallMatch) {
    throw new Error(
      `[ANTI-VACUITY GATE FAILED] ${entry.name}: existential constant '∃ C' appears AFTER universal quantifier '∀' in statementProp. Move '∃ (C : ℝ), 0 ≤ C ∧' outside '∀' so C cannot be chosen pointwise!`
    );
  }
}

/**
 * Verifies that a policy-4 statement satisfies `proof_registry::compose_generated_module`
 * token, boundary, import, and SPDX-instruction invariants.
 */
export function assertRegistryComposableStatement(entry: CatalogEntry): void {
  for (const token of FORBIDDEN_COMPOSITION_SUBSTRINGS) {
    if (entry.statement.includes(token)) {
      throw new Error(
        `[COMPOSABILITY GATE FAILED] ${entry.name} contains forbidden substring '${token}' (rejected by proof_registry::compose_generated_module)`
      );
    }
  }
  for (const line of entry.statement.split("\n")) {
    const trimmed = line.trimStart();
    if (
      trimmed === "section" ||
      trimmed === "end" ||
      trimmed.startsWith("end ") ||
      trimmed.startsWith("end\t")
    ) {
      throw new Error(
        `[COMPOSABILITY GATE FAILED] ${entry.name} contains forbidden section/end command '${trimmed}'`
      );
    }
    if (trimmed.startsWith("import ") || trimmed.startsWith("import\t")) {
      const withoutComment = trimmed.split("--")[0];
      const mods = withoutComment
        .replace(/^import\s+/, "")
        .split(/\s+/)
        .filter(Boolean);
      for (const modName of mods) {
        if (
          modName.startsWith("SwarmProofs") &&
          !modName.startsWith("SwarmProofs.Generated.H") &&
          !ALLOWED_SWARM_LIBRARY_IMPORTS.has(modName)
        ) {
          throw new Error(
            `[COMPOSABILITY GATE FAILED] ${entry.name} imports noncanonical SwarmProofs module '${modName}'`
          );
        }
      }
    }
  }
  if (
    entry.name.startsWith("chain-") &&
    !entry.statement.includes("SPDX-License-Identifier: Apache-2.0")
  ) {
    throw new Error(
      `[COMPOSABILITY GATE FAILED] ${entry.name} docstring must instruct solvers to include '-- SPDX-License-Identifier: Apache-2.0'`
    );
  }
}

/**
 * Rewrites `def statementProp` to `def statementPropOrig` and defines
 * `def statementProp : Prop := ¬ statementPropOrig` so `Check.lean` + `nanoda`
 * genuinely verify `Submission.proof : ¬ statementPropOrig`.
 */
export function negateStatementProp(statement: string): string {
  const defRegex = /^(\s*(?:noncomputable\s+)?def\s+)statementProp(\b)/m;
  if (!defRegex.test(statement)) {
    throw new Error(
      "negateStatementProp: statement does not declare `def statementProp`"
    );
  }
  const renamed = statement.replace(defRegex, "$1statementPropOrig$2");
  return `${renamed}\n\ndef statementProp : Prop := ¬ statementPropOrig\n`;
}

function mapSwarmImportToRelativeFiles(modName: string): string[] {
  if (modName === "SwarmProofs") {
    return [
      "SwarmProofs/Registry.lean",
      "SwarmProofs/Chains/RHLagarias/Defs.lean",
      "SwarmProofs/Chains/RHAnalytic/Defs.lean",
      "SwarmProofs/Chains/BSD/Defs.lean",
      "SwarmProofs/Chains/Circuits/Defs.lean",
    ];
  }
  if (!ALLOWED_SWARM_LIBRARY_IMPORTS.has(modName)) {
    throw new Error(`Unsupported SwarmProofs library import '${modName}'`);
  }
  return [`${modName.replace(/\./g, "/")}.lean`];
}

function stripDefsFileForInlineCompat(raw: string): {
  imports: string[];
  body: string;
} {
  const withoutLicense = raw.replace(
    /^\s*\/-[\s\S]*?SPDX-License-Identifier:[\s\S]*?-\/\s*/,
    ""
  );
  const imports: string[] = [];
  const keptLines: string[] = [];
  for (const line of withoutLicense.split("\n")) {
    const trimmed = line.trim();
    if (trimmed.startsWith("import ") || trimmed.startsWith("import\t")) {
      imports.push(trimmed);
      continue;
    }
    if (trimmed.startsWith("#guard ") || trimmed.startsWith("#guard\t")) {
      continue;
    }
    if (trimmed === "namespace SwarmProofs" || trimmed === "end SwarmProofs") {
      continue;
    }
    keptLines.push(line.replace(/^(\s*)@\[simp\]\s+/, "$1"));
  }
  return { imports, body: keptLines.join("\n").trim() };
}

/**
 * Compatibility bridge for `--check-only` while Step 3 runner upgrade is deploying:
 * inlines `SwarmProofs.Chains.*.Defs` into the check statement payload when
 * runner `/health` does not yet report `composable: true`.
 */
export function inlineSwarmProofsImportsForCompat(
  statement: string,
  swarmProofsRoot: string = SWARM_PROOFS_ROOT
): string {
  const hoistedImports = new Set<string>();
  const inlinedRelFiles = new Set<string>();
  const inlinedBodies: string[] = [];
  const remainingLines: string[] = [];

  for (const line of statement.split("\n")) {
    const trimmed = line.trim();
    if (trimmed.startsWith("import ") || trimmed.startsWith("import\t")) {
      const withoutComment = trimmed.split("--")[0];
      const mods = withoutComment
        .replace(/^import\s+/, "")
        .split(/\s+/)
        .filter(Boolean);
      for (const modName of mods) {
        if (modName.startsWith("SwarmProofs")) {
          for (const rel of mapSwarmImportToRelativeFiles(modName)) {
            if (inlinedRelFiles.has(rel)) continue;
            inlinedRelFiles.add(rel);
            const fullPath = path.join(swarmProofsRoot, rel);
            const raw = fs.readFileSync(fullPath, "utf8");
            const parsed = stripDefsFileForInlineCompat(raw);
            for (const imp of parsed.imports) hoistedImports.add(imp);
            if (parsed.body) inlinedBodies.push(parsed.body);
          }
        } else {
          hoistedImports.add(`import ${modName}`);
        }
      }
      continue;
    }
    remainingLines.push(line);
  }

  if (inlinedRelFiles.size === 0) {
    return statement;
  }

  const parts: string[] = [];
  if (hoistedImports.size > 0) {
    parts.push([...hoistedImports].join("\n"));
  }
  if (inlinedBodies.length > 0) {
    parts.push(inlinedBodies.join("\n\n"));
  }
  parts.push(remainingLines.join("\n").trim());
  return parts.join("\n\n") + "\n";
}

/**
 * Builds a combined statement testing whether `parentStatementProp → childStatementProp`
 * is a 1-line consequence of Rung N + Mathlib.
 */
export function buildParentImplicationStatement(
  parentRawStatement: string,
  childRawStatement: string,
  compatInline: boolean,
  swarmProofsRoot: string = SWARM_PROOFS_ROOT
): string {
  const defRegex = /^(\s*(?:noncomputable\s+)?def\s+)statementProp(\b)/m;
  if (!defRegex.test(parentRawStatement) || !defRegex.test(childRawStatement)) {
    throw new Error(
      "buildParentImplicationStatement: parent and child must both define statementProp"
    );
  }
  const imports = new Set<string>();
  const stripImports = (src: string): string => {
    const out: string[] = [];
    for (const line of src.split("\n")) {
      const t = line.trim();
      if (t.startsWith("import ") || t.startsWith("import\t")) {
        imports.add(t);
        continue;
      }
      out.push(line);
    }
    return out.join("\n").trim();
  };

  const parentBody = stripImports(parentRawStatement).replace(
    defRegex,
    "$1parentStatementProp$2"
  );
  const childBody = stripImports(childRawStatement).replace(
    defRegex,
    "$1childStatementProp$2"
  );
  const combined =
    `${[...imports].join("\n")}\n\n` +
    `${parentBody}\n\n` +
    `${childBody}\n\n` +
    `def statementProp : Prop := parentStatementProp → childStatementProp\n`;

  return compatInline
    ? inlineSwarmProofsImportsForCompat(combined, swarmProofsRoot)
    : combined;
}

export const DEFAULT_RUNNER_URL =
  process.env.LEAN_RUNNER_URL ??
  "https://shillbot-lean-runner-wjucctfqta-uc.a.run.app";

export function getRunnerIdToken(_audience?: string): string {
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

export async function fetchRunnerHealth(
  runnerUrl: string = DEFAULT_RUNNER_URL,
  headers: Record<string, string> = {},
  fetchImpl: typeof fetch = fetch
): Promise<RunnerHealthInfo> {
  const res = await fetchImpl(`${runnerUrl}/health`, {
    method: "GET",
    headers,
  });
  const text = await res.text();
  if (!res.ok) {
    throw new Error(`runner /health HTTP ${res.status}: ${text.slice(0, 300)}`);
  }
  const parsed = text ? (JSON.parse(text) as RunnerHealthInfo) : {};
  if (parsed.status && parsed.status !== "ok") {
    throw new Error(`runner /health unhealthy status='${parsed.status}'`);
  }
  return parsed;
}

export async function runnerCheckWithRetry(
  headers: Record<string, string>,
  payload: Record<string, unknown>,
  label: string,
  runnerUrl: string = DEFAULT_RUNNER_URL,
  fetchImpl: typeof fetch = fetch,
  retryDelayBaseMs = 2_000
): Promise<{ verdict?: string; detail?: string; axioms?: string[] }> {
  const maxAttempts = 4;
  for (let attempt = 1; attempt <= maxAttempts; attempt++) {
    let res: Response;
    try {
      res = await fetchImpl(`${runnerUrl}/check`, {
        method: "POST",
        headers,
        body: JSON.stringify(payload),
      });
    } catch (err) {
      if (attempt < maxAttempts && retryDelayBaseMs > 0) {
        await new Promise((r) => setTimeout(r, retryDelayBaseMs * attempt));
        continue;
      }
      throw new Error(
        `runner check network failure for ${label}: ${
          err instanceof Error ? err.message : String(err)
        }`
      );
    }
    if (res.ok) {
      return (await res.json()) as {
        verdict?: string;
        detail?: string;
        axioms?: string[];
      };
    }
    const text = await res.text();
    if (attempt < maxAttempts && (res.status === 429 || res.status >= 500)) {
      const delayMs = retryDelayBaseMs * attempt;
      if (delayMs > 0) {
        console.warn(
          `  [runner retry ${attempt}/${maxAttempts}] ${label} HTTP ${res.status}; retrying in ${delayMs}ms...`
        );
        await new Promise((r) => setTimeout(r, delayMs));
      }
      continue;
    }
    throw new Error(
      `runner check HTTP ${res.status} for ${label}: ${text.slice(0, 300)}`
    );
  }
  throw new Error(`runner check exhausted retries for ${label}`);
}

export interface TrivialityProbe {
  label: string;
  statement: string;
  proof: string;
  maxBuildSecs: number;
}

export interface ProbeBuildOptions {
  effectiveStatement: string;
  promotedDependencyModuleName?: string;
  parentImplicationStatement?: string;
}

function extractStatementImports(statement: string): string[] {
  const imports: string[] = [];
  for (const line of statement.split("\n")) {
    const t = line.trim();
    if (t.startsWith("import ") || t.startsWith("import\t")) {
      const clean = t.split("--")[0].trim();
      if (clean) imports.push(clean);
    }
  }
  return imports;
}

export function buildTrivialityProbes(
  _entry: CatalogEntry,
  options: ProbeBuildOptions
): TrivialityProbe[] {
  const {
    effectiveStatement,
    promotedDependencyModuleName,
    parentImplicationStatement,
  } = options;
  const negatedStatement = negateStatementProp(effectiveStatement);

  const stmtImports = extractStatementImports(effectiveStatement);
  const buildSearchHeader = (extraImports: string[]): string => {
    const set = new Set<string>(stmtImports);
    for (const u of PROBE_MATHLIB_UMBRELLAS) {
      set.add(`import ${u}`);
    }
    for (const ext of extraImports) {
      set.add(`import ${ext}`);
    }
    if (promotedDependencyModuleName) {
      set.add(`import ${promotedDependencyModuleName}`);
    }
    return (
      [...set].join("\n") +
      "\n\ntheorem swarm_fold_primorial (N : ℕ) : (∏ p ∈ (Finset.range (N + 1)).filter Nat.Prime, p) = primorial N := rfl\n"
    );
  };

  const depImportPrefix = promotedDependencyModuleName
    ? `import ${promotedDependencyModuleName}\n`
    : "";

  const probes: TrivialityProbe[] = [
    {
      label: "positive-exact-search",
      statement: effectiveStatement,
      maxBuildSecs: 120,
      proof:
        buildSearchHeader([]) +
        `theorem proof : statementProp := by\n` +
        `  unfold statementProp\n` +
        `  intros\n` +
        `  (try simp only [swarm_fold_primorial])\n` +
        `  exact?`,
    },
    {
      label: "zero-constant-vacuity",
      statement: effectiveStatement,
      maxBuildSecs: 120,
      proof:
        `${depImportPrefix}theorem proof : statementProp := by\n` +
        `  refine ⟨0, le_rfl, fun N _ => ?_⟩\n` +
        `  simp`,
    },
    {
      label: "pointwise-ratio-shortcut",
      statement: effectiveStatement,
      maxBuildSecs: 120,
      proof:
        `${depImportPrefix}theorem proof : statementProp := by\n` +
        `  intro N hN\n` +
        `  refine ⟨0, le_rfl, ?_⟩\n` +
        `  exact le_rfl`,
    },
    {
      label: "empty-domain-contradiction",
      statement: effectiveStatement,
      maxBuildSecs: 120,
      proof:
        `${depImportPrefix}theorem proof : statementProp := by\n` +
        `  unfold statementProp\n` +
        `  intros\n` +
        `  omega`,
    },
    {
      label: "positive-tactic-blast-triviality",
      statement: effectiveStatement,
      maxBuildSecs: 120,
      proof:
        `${depImportPrefix}import Mathlib.Tactic.NormNum\nimport Mathlib.Tactic.Ring\nimport Mathlib.Tactic.Linarith\nimport Mathlib.Tactic.Positivity\nimport Mathlib.Tactic.Tauto\nimport Aesop\n` +
        `theorem proof : statementProp := by\n` +
        `  unfold statementProp\n` +
        `  intros\n` +
        `  first | rfl | decide | omega | norm_num | ring | linarith | positivity | nlinarith | aesop | tauto | simp`,
    },
    {
      label: "negative-tactic-blast-disproof",
      statement: negatedStatement,
      maxBuildSecs: 120,
      proof:
        `${depImportPrefix}import Mathlib.Tactic.NormNum\nimport Mathlib.Tactic.Ring\nimport Mathlib.Tactic.Linarith\nimport Mathlib.Tactic.Positivity\nimport Mathlib.Tactic.Tauto\nimport Aesop\n` +
        `theorem proof : statementProp := by\n` +
        `  unfold statementProp statementPropOrig\n` +
        `  intro h\n` +
        `  first | exact False.elim (by omega) | norm_num at h | omega | simp at h | aesop`,
    },
    {
      label: "positive-apply-assumption",
      statement: effectiveStatement,
      maxBuildSecs: 120,
      proof:
        buildSearchHeader([]) +
        `set_option maxHeartbeats 200000 in\n` +
        `theorem proof : statementProp := by\n` +
        `  unfold statementProp\n` +
        `  intros\n` +
        `  (try simp only [swarm_fold_primorial])\n` +
        `  apply? <;> assumption`,
    },
    {
      label: "positive-hint",
      statement: effectiveStatement,
      maxBuildSecs: 120,
      proof:
        buildSearchHeader(["Mathlib.Tactic.Hint", "Aesop"]) +
        `set_option maxHeartbeats 200000 in\n` +
        `theorem proof : statementProp := by\n` +
        `  unfold statementProp\n` +
        `  intros\n` +
        `  (try simp only [swarm_fold_primorial])\n` +
        `  hint`,
    },
    {
      label: "positive-aesop-raised-budget",
      statement: effectiveStatement,
      maxBuildSecs: 120,
      proof:
        buildSearchHeader(["Aesop"]) +
        `theorem proof : statementProp := by\n` +
        `  unfold statementProp\n` +
        `  intros\n` +
        `  (try simp only [swarm_fold_primorial])\n` +
        `  aesop (config := { maxRuleApplicationDepth := 60, maxRuleApplications := 600 })`,
    },
    {
      label: "negative-exact-search",
      statement: negatedStatement,
      maxBuildSecs: 120,
      proof:
        buildSearchHeader([]) +
        `theorem proof : statementProp := by\n` +
        `  unfold statementProp statementPropOrig\n` +
        `  (try simp only [swarm_fold_primorial])\n` +
        `  first | exact? | (intro h; exact?)`,
    },
    {
      label: "negative-apply-assumption",
      statement: negatedStatement,
      maxBuildSecs: 120,
      proof:
        buildSearchHeader([]) +
        `set_option maxHeartbeats 200000 in\n` +
        `theorem proof : statementProp := by\n` +
        `  unfold statementProp statementPropOrig\n` +
        `  (try simp only [swarm_fold_primorial])\n` +
        `  apply? <;> assumption`,
    },
    {
      label: "negative-hint",
      statement: negatedStatement,
      maxBuildSecs: 120,
      proof:
        buildSearchHeader(["Mathlib.Tactic.Hint", "Aesop"]) +
        `set_option maxHeartbeats 200000 in\n` +
        `theorem proof : statementProp := by\n` +
        `  unfold statementProp statementPropOrig\n` +
        `  (try simp only [swarm_fold_primorial])\n` +
        `  hint`,
    },
    {
      label: "negative-aesop-raised-budget",
      statement: negatedStatement,
      maxBuildSecs: 120,
      proof:
        buildSearchHeader(["Aesop"]) +
        `theorem proof : statementProp := by\n` +
        `  unfold statementProp statementPropOrig\n` +
        `  intro h\n` +
        `  (try simp only [swarm_fold_primorial] at h)\n` +
        `  aesop (config := { maxRuleApplicationDepth := 60, maxRuleApplications := 600 })`,
    },
  ];

  if (parentImplicationStatement) {
    probes.push(
      {
        label: "dependency-consequence-tactic-aesop",
        statement: parentImplicationStatement,
        maxBuildSecs: 120,
        proof:
          buildSearchHeader(["Aesop"]) +
          `theorem proof : statementProp := by\n` +
          `  unfold statementProp parentStatementProp childStatementProp\n` +
          `  intro hParent\n` +
          `  intros\n` +
          `  first | exact hParent | assumption | simp [hParent] | omega | linarith | aesop (config := { maxRuleApplicationDepth := 60, maxRuleApplications := 600 })`,
      },
      {
        label: "dependency-consequence-exact-search",
        statement: parentImplicationStatement,
        maxBuildSecs: 120,
        proof:
          buildSearchHeader([]) +
          `theorem proof : statementProp := by\n` +
          `  unfold statementProp parentStatementProp childStatementProp\n` +
          `  intro hParent\n` +
          `  intros\n` +
          `  (try simp only [swarm_fold_primorial] at hParent ⊢)\n` +
          `  exact?`,
      }
    );
  }

  return probes;
}

export function assertValidProbeResponse(
  resp: { verdict?: string; detail?: string },
  entryName: string,
  probeLabel: string
): void {
  if (resp.verdict !== "pass" && resp.verdict !== "fail") {
    throw new Error(
      `[PRE-SEED GATE ERROR] ${entryName} (${probeLabel}): unexpected runner verdict=${JSON.stringify(
        resp.verdict
      )} detail=${resp.detail ?? ""}`
    );
  }
  const detail = String(resp.detail ?? "");
  for (const marker of MALFORMED_PROBE_DETAIL_MARKERS) {
    if (detail.includes(marker)) {
      throw new Error(
        `[MALFORMED PROBE / RUNNER POLICY ERROR] ${entryName} (${probeLabel}) hit '${marker}': ${detail}`
      );
    }
  }
}

export interface VerifyStatementOptions {
  fetchImpl?: typeof fetch;
  runnerUrl?: string;
  runnerToken?: string;
  checkOnly?: boolean;
  leanPolicy?: 1 | 2 | 3 | 4;
  registry?: ProofRegistryManifest;
  catalogByName?: Map<string, CatalogEntry>;
  runnerHealth?: RunnerHealthInfo;
  skipSanityScript?: boolean;
  retryDelayBaseMs?: number;
  probeConcurrency?: number;
}

export async function verifyStatementElaborates(
  entry: CatalogEntry,
  opts: VerifyStatementOptions = {}
): Promise<void> {
  const fetchImpl = opts.fetchImpl ?? fetch;
  const runnerUrl = opts.runnerUrl ?? DEFAULT_RUNNER_URL;
  const checkOnly = opts.checkOnly ?? true;
  const leanPolicy = opts.leanPolicy ?? entry.declaredLeanPolicy ?? 4;
  const retryDelayBaseMs = opts.retryDelayBaseMs ?? 2_000;
  const probeConcurrency = Math.max(1, opts.probeConcurrency ?? 5);

  assertOuterExistentialQuantifierOrder(entry);
  if (/^\s*open\s+Classical\b/m.test(entry.statement)) {
    throw new Error(
      `[PRE-SEED GATE FAILED] ${entry.name} contains top-level 'open Classical', which causes Lean '#print axioms' to shorten 'Classical.choice' to 'choice' and fail allow-list verification!`
    );
  }
  if (leanPolicy === 4 || entry.declaredLeanPolicy === 4) {
    assertRegistryComposableStatement(entry);
  }

  if (entry.sanityScriptPath && !opts.skipSanityScript) {
    execSync(`npx tsx "${entry.sanityScriptPath}"`, { stdio: "inherit" });
  }

  const token = opts.runnerToken ?? getRunnerIdToken(runnerUrl);
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
  };
  if (token) headers.Authorization = `Bearer ${token}`;

  const health =
    opts.runnerHealth ??
    (await fetchRunnerHealth(runnerUrl, headers, fetchImpl));
  const registry = opts.registry ?? loadProofRegistry();

  if (!checkOnly) {
    assertSeedingAllowedForEntry(entry, leanPolicy, registry, health);
  }

  const resolvedDep = entry.dependsOn
    ? resolveRungDependency(entry.dependsOn, registry)
    : undefined;
  const useComposableMode = health.composable === true;
  if (
    !useComposableMode &&
    !checkOnly &&
    /\bimport\s+SwarmProofs\b/.test(entry.statement)
  ) {
    throw new Error(
      `[PRE-SEED GATE FAILED] ${entry.name} imports SwarmProofs, which requires runner composable mode when seeding`
    );
  }

  const effectiveStatement = useComposableMode
    ? entry.statement
    : inlineSwarmProofsImportsForCompat(entry.statement);
  const mode = useComposableMode ? "composable" : "mathlib";
  const promotedDepMod =
    useComposableMode && resolvedDep?.status === "active"
      ? resolvedDep.moduleName
      : undefined;
  const importAllowlist = useComposableMode
    ? [
        "Mathlib",
        "Zeta23",
        "SwarmProofs",
        "Aesop",
        ...(promotedDepMod ? [promotedDepMod] : []),
      ]
    : ["Mathlib", "Aesop"];

  const t0 = Date.now();
  const resp = await runnerCheckWithRetry(
    headers,
    {
      statement: effectiveStatement,
      proof: "theorem proof : statementProp := True.intro",
      statement_def: "statementProp",
      entry_theorem: "proof",
      axiom_allowlist: ["propext", "Classical.choice", "Quot.sound"],
      mode,
      import_allowlist: importAllowlist,
      max_build_secs: 180,
      max_build_mem_mb: 16384,
    },
    entry.name,
    runnerUrl,
    fetchImpl,
    retryDelayBaseMs
  );
  assertValidProbeResponse(resp, entry.name, "stage-a1-elaboration");
  if (resp.verdict !== "fail") {
    throw new Error(
      `pre-seed elaboration gate FAILED for ${entry.name}: expected Stage A1 statement elaboration + Stage A2 fail, got verdict=${resp.verdict} detail=${resp.detail}`
    );
  }

  if (entry.kind === "solvable" && entry.companionProof) {
    const pJson = await runnerCheckWithRetry(
      headers,
      {
        statement: effectiveStatement,
        proof: entry.companionProof,
        statement_def: "statementProp",
        entry_theorem: "proof",
        axiom_allowlist: ["propext", "Classical.choice", "Quot.sound"],
        mode,
        import_allowlist: importAllowlist,
        max_build_secs: 180,
        max_build_mem_mb: 16384,
      },
      `${entry.name} (companion)`,
      runnerUrl,
      fetchImpl,
      retryDelayBaseMs
    );
    assertValidProbeResponse(pJson, entry.name, "companion-proof");
    if (pJson.verdict !== "pass") {
      throw new Error(
        `companion proof FAILED for solvable entry ${entry.name}: verdict=${pJson.verdict} detail=${pJson.detail}`
      );
    }
    console.log(
      `  [companion PASS] solvable/${entry.name} companion proof verified; running full triviality probes...`
    );
  }

  let parentImplicationStatement: string | undefined;
  if (entry.dependsOn) {
    const parentEntry = opts.catalogByName?.get(entry.dependsOn);
    const parentRaw =
      parentEntry?.statement ??
      (() => {
        const candidatePath = path.join(
          CATALOG_DIR,
          "open",
          `${entry.dependsOn}.lean`
        );
        return fs.existsSync(candidatePath)
          ? fs.readFileSync(candidatePath, "utf8").trimEnd()
          : undefined;
      })();
    if (parentRaw) {
      parentImplicationStatement = buildParentImplicationStatement(
        parentRaw,
        entry.statement,
        !useComposableMode
      );
    }
  }

  const probes = buildTrivialityProbes(entry, {
    effectiveStatement,
    promotedDependencyModuleName: promotedDepMod,
    parentImplicationStatement,
  });

  for (let i = 0; i < probes.length; i += probeConcurrency) {
    const batch = probes.slice(i, i + probeConcurrency);
    const results = await Promise.all(
      batch.map(async (canary) => {
        const cJson = await runnerCheckWithRetry(
          headers,
          {
            statement: canary.statement,
            proof: canary.proof,
            statement_def: "statementProp",
            entry_theorem: "proof",
            axiom_allowlist: ["propext", "Classical.choice", "Quot.sound"],
            mode,
            import_allowlist: importAllowlist,
            max_build_secs: canary.maxBuildSecs,
            max_build_mem_mb: 16384,
          },
          `${entry.name} (${canary.label})`,
          runnerUrl,
          fetchImpl,
          retryDelayBaseMs
        );
        assertValidProbeResponse(cJson, entry.name, canary.label);
        return { canary, cJson };
      })
    );
    for (const { canary, cJson } of results) {
      if (cJson.verdict === "pass") {
        throw new Error(
          `[ANTI-VACUITY / TRIVIALITY GATE FAILED] ${entry.name} was solved trivially by probe '${canary.label}'! Refusing to seed.`
        );
      }
    }
  }

  console.log(
    `  [pre-seed check PASS] ${entry.name} elaborated cleanly & rejected all ${
      probes.length
    } triviality/vacuity probes (${Date.now() - t0}ms)`
  );
}

async function existingOpenTopics(
  bearer: string,
  network: string,
  fetchImpl: typeof fetch = fetch,
  apiBase: string = API_BASE
): Promise<Set<string>> {
  const res = (await api(
    "GET",
    `/tasks?network=${network}&limit=100`,
    bearer,
    undefined,
    fetchImpl,
    apiBase
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

async function verifyReductionChainFiles(): Promise<void> {
  const chainsDir = path.join(REPO_ROOT, "proofs", "millennium-chains");
  const token = getRunnerIdToken(DEFAULT_RUNNER_URL);
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
  };
  if (token) headers.Authorization = `Bearer ${token}`;

  for (const fileName of REDUCTION_CHAIN_FILES) {
    const t0 = Date.now();
    const src = fs.readFileSync(path.join(chainsDir, fileName), "utf8");
    const splitIdx = src.search(/\n(?=lemma |theorem proof\b)/);
    if (splitIdx < 0) {
      throw new Error(
        `Reduction file ${fileName} missing 'theorem proof' split`
      );
    }
    const statement = src.slice(0, splitIdx).trim();
    const proof = src.slice(splitIdx + 1).trim();
    const json = await runnerCheckWithRetry(
      headers,
      {
        statement,
        proof,
        statement_def: "statementProp",
        entry_theorem: "proof",
        axiom_allowlist: ["propext", "Classical.choice", "Quot.sound"],
        mode: "mathlib",
        import_allowlist: ["Mathlib"],
        max_build_secs: 600,
        max_build_mem_mb: 16384,
      },
      fileName
    );
    if (json.verdict !== "pass") {
      throw new Error(
        `[REDUCTION GATE FAILED] ${fileName}: verdict=${json.verdict} detail=${json.detail}`
      );
    }
    console.log(
      `  [reduction PASS] proofs/millennium-chains/${fileName} kernel-verified with 0 sorry (${
        Date.now() - t0
      }ms, axioms=${JSON.stringify(json.axioms ?? [])})`
    );
  }
}

export function parseLeanPolicyArg(
  argv: string[] = process.argv
): 1 | 2 | 3 | 4 {
  const eqArg = argv.find((a) => a.startsWith("--lean-policy="));
  const idx = argv.indexOf("--lean-policy");
  const raw = eqArg
    ? eqArg.slice("--lean-policy=".length).trim()
    : idx >= 0 && idx + 1 < argv.length
    ? argv[idx + 1].trim()
    : undefined;
  if (!raw) return 4;
  const parsed = Number(raw);
  if (![1, 2, 3, 4].includes(parsed)) {
    throw new Error(
      `Invalid --lean-policy value '${raw}'; expected 1, 2, 3, or 4`
    );
  }
  return parsed as 1 | 2 | 3 | 4;
}

async function main(): Promise<void> {
  if (process.argv.includes("--skip-precheck")) {
    throw new Error(
      "--skip-precheck is forbidden: every seeded statement must pass the triviality & elaboration gate"
    );
  }
  const isMainnet =
    process.argv.includes("--mainnet") ||
    process.env.SOLANA_NETWORK === "mainnet";
  const checkOnly = process.argv.includes("--check-only");
  const isLadder = process.argv.includes("--ladder");
  const isChains = process.argv.includes("--chains");
  const taskArg = process.argv.find((a) => a.startsWith("--tasks="));
  const reservedForWalletArg = process.argv
    .find((a) => a.startsWith("--reserved-for-wallet="))
    ?.replace("--reserved-for-wallet=", "")
    .trim();
  const network = isMainnet ? "mainnet" : "devnet";
  const leanPolicy = parseLeanPolicyArg(process.argv);

  const allLoaded = loadCatalog(isLadder || Boolean(taskArg));
  const catalogByName = new Map(allLoaded.map((e) => [e.name, e]));
  let catalog = allLoaded;

  const isMillenniumOnly =
    process.argv.includes("--millennium") ||
    process.argv.includes("--new") ||
    (isMainnet &&
      !process.argv.includes("--all-open") &&
      !isLadder &&
      !isChains &&
      !taskArg);

  if (taskArg) {
    const allowed = new Set(
      taskArg
        .replace("--tasks=", "")
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean)
    );
    catalog = catalog.filter((e) => allowed.has(e.name));
    if (catalog.length !== allowed.size) {
      const found = new Set(catalog.map((e) => e.name));
      const missing = [...allowed].filter((n) => !found.has(n));
      throw new Error(`Unknown --tasks entry(s): ${missing.join(", ")}`);
    }
  } else if (isChains) {
    catalog = catalog.filter((e) => e.program === "millennium-chains");
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

  const registry = loadProofRegistry();
  const runnerToken = getRunnerIdToken(DEFAULT_RUNNER_URL);
  const runnerHeaders: Record<string, string> = {
    "Content-Type": "application/json",
  };
  if (runnerToken) runnerHeaders.Authorization = `Bearer ${runnerToken}`;
  const runnerHealth = await fetchRunnerHealth(
    DEFAULT_RUNNER_URL,
    runnerHeaders
  );

  if (checkOnly) {
    if (isChains) {
      await verifyReductionChainFiles();
    }
    for (const entry of catalog) {
      await verifyStatementElaborates(entry, {
        checkOnly: true,
        leanPolicy,
        registry,
        catalogByName,
        runnerHealth,
        runnerToken,
      });
    }
    console.log(
      `\n[check-only PASS] All ${catalog.length} statements passed elaboration & anti-vacuity/triviality gate.`
    );
    return;
  }

  // Fail closed BEFORE touching RPC or creating any campaigns if policy-4 or rung dependencies are unsatisfied:
  for (const entry of catalog) {
    assertSeedingAllowedForEntry(entry, leanPolicy, registry, runnerHealth);
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
  if (reservedForWalletArg) {
    console.log(`reserved_for_wallet: ${reservedForWalletArg}`);
  }

  const totalEscrowNeeded = catalog.reduce(
    (acc, e) =>
      acc +
      (escrowArg
        ? escrowLamports
        : isMainnet
        ? e.defaultEscrowLamports ?? escrowLamports
        : escrowLamports),
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

  if (process.argv.includes("--skip-precheck")) {
    throw new Error(
      "--skip-precheck is forbidden: every seeded statement must pass the triviality & elaboration gate"
    );
  }
  const forceReseed = process.argv.includes("--force");
  console.log(`lean_policy: ${leanPolicy}`);
  const already = forceReseed
    ? new Set<string>()
    : await existingOpenTopics(bearer, network);
  let created = 0;
  let skipped = 0;

  for (const entry of catalog) {
    const entryEscrow = escrowArg
      ? escrowLamports
      : isMainnet
      ? entry.defaultEscrowLamports ?? escrowLamports
      : escrowLamports;
    const topic = `catalog:${entry.name}`;
    if (already.has(topic)) {
      console.log(`  skip (open task exists): ${topic}`);
      skipped++;
      continue;
    }
    const resolvedDep = assertSeedingAllowedForEntry(
      entry,
      leanPolicy,
      registry,
      runnerHealth
    );
    await verifyStatementElaborates(entry, {
      checkOnly: false,
      leanPolicy,
      registry,
      catalogByName,
      runnerHealth,
      runnerToken,
    });
    const campPayload: Record<string, unknown> = {
      brief: {
        topic,
        brand_voice: "rigorous",
        cta: entry.customCta ?? `Prove: ${entry.name} (${entry.kind})`,
        utm_link: "https://swarm.tips",
      },
      budget_lamports: entryEscrow,
      platform: LEAN_PROOF_PLATFORM,
      statement_lean: entry.statement,
      lean_policy: leanPolicy,
    };
    if (resolvedDep?.moduleId) {
      campPayload.lean_dependencies = [resolvedDep.moduleId];
    }
    const camp = await api(
      "POST",
      `/campaigns?network=${network}`,
      bearer,
      campPayload
    );
    const campaignId = camp.campaign_id as string;

    // Fund one task so the bounty is claimable + visible on the board.
    const fundBody: Record<string, unknown> = { amount_lamports: entryEscrow };
    if (reservedForWalletArg) {
      fundBody.reserved_for_wallet = reservedForWalletArg;
    }
    const fund = await api(
      "POST",
      `/campaigns/${campaignId}/fund?network=${network}`,
      bearer,
      fundBody
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
        escrow_lamports: entryEscrow,
      }
    );
    const taskDoc = await api(
      "GET",
      `/tasks/${encodeURIComponent(String(fund.task_id))}?network=${network}`,
      bearer
    );
    if (String(taskDoc.escrow_lamports) !== String(entryEscrow)) {
      throw new Error(
        `[escrow parity failed] task ${
          fund.task_id
        }: expected escrow_lamports="${entryEscrow}", got ${JSON.stringify(
          taskDoc.escrow_lamports
        )}`
      );
    }
    if (
      typeof taskDoc.estimated_payment_lamports !== "number" ||
      taskDoc.estimated_payment_lamports <= 0 ||
      taskDoc.estimated_payment_lamports > entryEscrow
    ) {
      throw new Error(
        `[escrow parity failed] task ${
          fund.task_id
        }: estimated_payment_lamports=${JSON.stringify(
          taskDoc.estimated_payment_lamports
        )} must be in (0, ${entryEscrow}]`
      );
    }
    console.log(
      `  [${network}] seeded ${entry.kind}/${entry.name} (${(
        entryEscrow / LAMPORTS_PER_SOL
      ).toFixed(4)} SOL) -> campaign ${campaignId} task ${fund.task_id} pda ${
        fund.task_pda
      } (tx ${sig})`
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
