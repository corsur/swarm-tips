/**
 * Seed the LeanProof bounty catalog onto DEVNET as policy-4 (composable) campaigns.
 *
 * For every statement in tests/fixtures/lean/catalog/{open,solvable}/*.lean it
 * creates a `lean_policy: 4` LeanProof campaign (statement_lean = the exact file bytes)
 * and funds one task, so the board shows real standing bounties. Idempotent: a
 * campaign is tagged `brief.topic = "catalog:<name>"`; a re-run skips any tag
 * that already exists among the client's campaigns.
 *
 * Ladder rungs (`chain-*`) import versioned frozen chain definitions from
 * `SwarmProofs.Chains.*.Defs.V1` and declare `-- @depends-on: <prev-rung>` on Rung N+1.
 * Seeding refuses to post Rung N+1 until Rung N is promoted and `active` in
 * `proofs/SwarmProofs/registry.json`, refuses `lean_policy: 4` unless runner `/health`
 * reports `composable: true`, and gates every statement through both:
 *   1. Full-`import Mathlib` search probes (`exact?`, `solve_by_elim` + `apply?`,
 *      `hint`, `aesop` on positive, negated, and parent-implication statements) via
 *      `lake env lean` inside `proofs/SwarmProofs` with strict exhaustion parsing; and
 *   2. Live `shillbot-lean-runner` Stage A1 elaboration + fast tactic-blast probes.
 *
 * IMMUTABILITY AFTER POSTING: Editing `open/<stem>.lean` in place after a campaign has
 * been posted intentionally blocks registry promotion of in-flight tasks carrying the old
 * `statement_sha256` (`verify_catalog_statement_bytes` enforces exact byte-for-byte hash
 * parity against `open/<stem>.lean`). To revise a posted statement, move the old file to
 * `retired/` and create a new stem under `open/`.
 *
 * Supports `--network devnet` (default) and `--network mainnet` / `--mainnet`. Seeding
 * with REAL mainnet SOL is a founder decision — do NOT point this at mainnet without sign-off.
 */
import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import * as crypto from "crypto";
import { execSync, spawnSync } from "child_process";
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
  "SwarmProofs.Chains.BSD.Defs.V1",
  "SwarmProofs.Chains.Circuits.Defs.V1",
  "SwarmProofs.Chains.RHAnalytic.Defs.V1",
  "SwarmProofs.Chains.RHLagarias.Defs.V1",
]);

export const UNFOLD_DEFS_BY_MODULE: Record<string, readonly string[]> = {
  "SwarmProofs.Chains.RHLagarias.Defs.V1": [
    "lagariasBound",
    "isSuperabundant",
    "isSquarefreePos",
    "lagariasEnvelopeRat",
    "expUpperRat",
    "logUpperRat",
    "sigmaDivisors",
    "sigmaNat",
    "sigmaRat",
    "harmonicNum",
    "harmonicRat",
  ],
  "SwarmProofs.Chains.RHAnalytic.Defs.V1": [
    "mertensSum",
    "mobiusInt",
    "primeFactorCount",
    "squarePrimeFactorCount",
  ],
  "SwarmProofs.Chains.BSD.Defs.V1": [
    "isCongruentNumber",
    "isCongruentTriangle",
  ],
  "SwarmProofs.Chains.Circuits.Defs.V1": [
    "evalCircuitFin",
    "evalCircuit",
    "isDeMorganOp",
  ],
};

/**
 * Wrapper definitions unfolded first via `dsimp only [...]` before `simp only [bridge_*]`
 * rewrites the underlying bridged arithmetic/divisor/harmonic/Möbius definitions.
 */
export const WRAPPER_DEFS_BY_MODULE: Record<string, readonly string[]> = {
  "SwarmProofs.Chains.RHLagarias.Defs.V1": [
    "lagariasBound",
    "isSuperabundant",
    "lagariasEnvelopeRat",
    "expUpperRat",
    "logUpperRat",
  ],
  "SwarmProofs.Chains.RHAnalytic.Defs.V1": ["squarePrimeFactorCount"],
  "SwarmProofs.Chains.BSD.Defs.V1": [
    "isCongruentNumber",
    "isCongruentTriangle",
  ],
  "SwarmProofs.Chains.Circuits.Defs.V1": [
    "evalCircuitFin",
    "evalCircuit",
    "isDeMorganOp",
  ],
};

export const BRIDGE_LEMMAS_BY_DEFS_MODULE: Record<string, readonly string[]> = {
  "SwarmProofs.Chains.RHAnalytic.Defs.V1": [
    "bridge_primeFactorCount",
    "bridge_mobiusInt",
    "bridge_mertensSum",
    "bridge_primorial",
  ],
  "SwarmProofs.Chains.RHLagarias.Defs.V1": [
    "bridge_sigmaNat",
    "bridge_sigmaRat",
    "bridge_sigmaDivisors",
    "bridge_isSquarefreePos",
    "bridge_harmonicRat",
    "bridge_harmonicNum",
  ],
  "SwarmProofs.Chains.BSD.Defs.V1": [],
  "SwarmProofs.Chains.Circuits.Defs.V1": [],
};

export const FORBIDDEN_SHARED_WORD_TOKENS = [
  "axiom",
  "sorry",
  "sorryAx",
  "admit",
  "unsafe",
  "partial",
  "opaque",
  "meta",
  "constant",
  "extern",
  "implemented_by",
  "csimp",
  "macro",
  "macro_rules",
  "syntax",
  "elab",
  "elab_rules",
  "command_elab",
  "term_elab",
  "notation",
  "infix",
  "infixl",
  "infixr",
  "prefix",
  "postfix",
  "attribute",
  "deriving",
  "variable",
  "run_tac",
  "run_cmd",
  "run_elab",
  "by_elab",
  "native_decide",
  "ofReduceBool",
  "trustCompiler",
  "instance",
  "class",
  "initialize",
  "builtin_initialize",
  "unif_hint",
  "declare_syntax_cat",
  "simproc",
  "namespace",
  "section",
  "end",
  "local",
  "scoped",
] as const;

export const FORBIDDEN_COMPOSITION_SUBSTRINGS = [
  "@[",
  "#eval",
  "#check",
  "#print",
  "#reduce",
  "#synth",
] as const;

function isLeanIdentChar(ch: string): boolean {
  return /^[\p{L}\p{N}_'?»₊₋]$/u.test(ch);
}

function matchLeanCharLiteral(chars: string[], idx: number): number | null {
  if (idx + 2 < chars.length) {
    const c1 = chars[idx + 1];
    if (
      c1 !== "'" &&
      c1 !== "\\" &&
      c1 !== "\n" &&
      c1 !== "\r" &&
      chars[idx + 2] === "'"
    ) {
      return 3;
    }
  }
  if (idx + 3 < chars.length && chars[idx + 1] === "\\") {
    const esc = chars[idx + 2];
    if (esc !== "\n" && esc !== "\r" && chars[idx + 3] === "'") {
      return 4;
    }
    if (
      esc === "x" &&
      idx + 5 < chars.length &&
      /^[0-9a-fA-F]$/.test(chars[idx + 3]) &&
      /^[0-9a-fA-F]$/.test(chars[idx + 4]) &&
      chars[idx + 5] === "'"
    ) {
      return 6;
    }
    if (
      esc === "u" &&
      idx + 7 < chars.length &&
      [3, 4, 5, 6].every((k) => /^[0-9a-fA-F]$/.test(chars[idx + k])) &&
      chars[idx + 7] === "'"
    ) {
      return 8;
    }
  }
  return null;
}

export function stripLeanComments(source: string): string {
  const chars = Array.from(source);
  let out = "";
  let blockDepth = 0;
  let prevIsIdent = false;
  let prevWasIdentBang = false;
  let i = 0;
  while (i < chars.length) {
    const ch = chars[i];
    const next = i + 1 < chars.length ? chars[i + 1] : "";
    if (blockDepth > 0) {
      if (ch === "/" && next === "-") {
        blockDepth++;
        out += "  ";
        i += 2;
        continue;
      }
      if (ch === "-" && next === "/") {
        blockDepth--;
        out += "  ";
        i += 2;
        if (blockDepth === 0) {
          prevIsIdent = false;
          prevWasIdentBang = false;
        }
        continue;
      }
      out += ch === "\n" ? "\n" : " ";
      i++;
      continue;
    }
    if (ch === "/" && next === "-") {
      blockDepth = 1;
      out += "  ";
      prevIsIdent = false;
      prevWasIdentBang = false;
      i += 2;
      continue;
    }
    if (ch === "-" && next === "-") {
      while (i < chars.length && chars[i] !== "\n") {
        out += " ";
        i++;
      }
      prevIsIdent = false;
      prevWasIdentBang = false;
      continue;
    }
    if (ch === "r" && !prevIsIdent) {
      let hashCount = 0;
      while (
        i + 1 + hashCount < chars.length &&
        chars[i + 1 + hashCount] === "#"
      ) {
        hashCount++;
      }
      if (
        hashCount > 0 &&
        i + 1 + hashCount < chars.length &&
        chars[i + 1 + hashCount] === '"'
      ) {
        out += `r${"#".repeat(hashCount)}"`;
        i += 2 + hashCount;
        let closed = false;
        while (i < chars.length) {
          const c = chars[i];
          let closingHashes = c === '"';
          for (let k = 1; closingHashes && k <= hashCount; k++) {
            if (i + k >= chars.length || chars[i + k] !== "#") {
              closingHashes = false;
            }
          }
          if (closingHashes) {
            out += `"${"#".repeat(hashCount)}`;
            i += 1 + hashCount;
            closed = true;
            break;
          }
          out += c === "\n" ? "\n" : " ";
          i++;
        }
        if (!closed) {
          throw new Error(
            "[COMPOSABILITY GATE FAILED] unclosed Lean raw string literal in source"
          );
        }
        prevIsIdent = false;
        prevWasIdentBang = false;
        continue;
      }
    }
    if (ch === '"') {
      if (prevWasIdentBang) {
        throw new Error(
          "[COMPOSABILITY GATE FAILED] interpolated Lean string literal is not permitted in promotion source"
        );
      }
      out += '"';
      i++;
      let closed = false;
      while (i < chars.length) {
        const c = chars[i];
        if (c === "\\") {
          out += " ";
          i++;
          if (i >= chars.length) {
            throw new Error(
              "[COMPOSABILITY GATE FAILED] unclosed Lean string literal in source"
            );
          }
          const esc = chars[i];
          out += esc === "\n" ? "\n" : " ";
          i++;
          continue;
        }
        if (c === '"') {
          out += '"';
          i++;
          closed = true;
          break;
        }
        out += c === "\n" ? "\n" : " ";
        i++;
      }
      if (!closed) {
        throw new Error(
          "[COMPOSABILITY GATE FAILED] unclosed Lean string literal in source"
        );
      }
      prevIsIdent = false;
      prevWasIdentBang = false;
      continue;
    }
    if (ch === "'" && !prevIsIdent) {
      const litLen = matchLeanCharLiteral(chars, i);
      if (litLen === null) {
        throw new Error(
          "[COMPOSABILITY GATE FAILED] unclosed or malformed Lean character literal in source"
        );
      }
      out += `'${" ".repeat(Math.max(0, litLen - 2))}'`;
      i += litLen;
      prevIsIdent = false;
      prevWasIdentBang = false;
      continue;
    }
    out += ch;
    prevWasIdentBang = prevIsIdent && ch === "!";
    prevIsIdent = isLeanIdentChar(ch) || (prevIsIdent && ch === "!");
    i++;
  }
  if (blockDepth !== 0) {
    throw new Error(
      "[COMPOSABILITY GATE FAILED] unclosed Lean block comment '/-' in statement"
    );
  }
  return out;
}

export const MALFORMED_PROBE_DETAIL_MARKERS = [
  "does not exist",
  "unknown tactic",
  "unknown module prefix",
  "unknown identifier",
  "forbidden token",
  "exceeded the policy wall-clock cap",
  "wall-clock cap",
  "timed out",
  "timeout",
  "heartbeat",
  "maximum recursion depth",
  "out of memory",
  "memory",
  "killed",
  "Killed",
  "signal",
] as const;

export const VALID_RUNNER_FAIL_INDICATORS = [
  "unsolved goals",
  "type mismatch",
  "tactic",
  "failed",
  "omega",
  "linarith",
  "norm_num",
  "simp",
  "aesop",
  "ring",
  "positivity",
  "nlinarith",
  "tauto",
  "decide",
  "rfl",
  "sorry",
  "application type mismatch",
] as const;

export const MATHLIB_FAIL_CLOSED_MARKERS = [
  "(deterministic) timeout",
  "maximum number of heartbeats",
  "heartbeat",
  "maximum recursion depth",
  "out of memory",
  "memory allocation",
  "timed out",
  "killed",
  "Killed",
  "unknown identifier",
  "unknown tactic",
  "unknown module prefix",
  "does not exist",
  "unexpected token",
  "parse error",
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

/**
 * Retired post-Alpöge (Nov 20, 2026 Lean 4 + Mathlib + nanoda proof of RH & GRH).
 * Excluded from default Mainnet seeding unless explicitly targeted via `--tasks=`.
 */
export const RETIRED_RH_SLUGS = new Set([
  "riemann-hypothesis",
  "millennium-rh-lagarias-divisor",
  "millennium-rh-mertens-quasi",
  "weil-gram-spectral-floor",
  "chain-rh-analytic-4-zero-free-strip-99-100",
  "chain-rh-lagarias-3-odd-and-squarefree",
  "chain-rh-lagarias-4-even-superabundant-core",
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
  "(proof must start with `-- SPDX-License-Identifier: Apache-2.0` and avoid `@[...]` attributes, `instance`/`class`, and `open`/`local`/`scoped`)";

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
  "chain-rh-analytic-1-mertens-first-prime-log-bound": {
    escrowLamports: 15_000_000,
    cta: `Chain C [Rung 1/4 -> RH Analytic]: Prove Mertens First Theorem |sum_{p<=N} (log p)/p - log N| <= C ${SPDX_CTA_NOTE}`,
  },
  "chain-rh-analytic-2-mobius-harmonic-bound": {
    escrowLamports: 20_000_000,
    dependsOn: "chain-rh-analytic-1-mertens-first-prime-log-bound",
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
    cta: `Track E [Rung 2/4 -> Pi1 RH Metamathematics]: Prove full Delta_0 Los ultrapower theorem & Robinson Delta_0 overspill (provable landmark) ${SPDX_CTA_NOTE}`,
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
      // Exact file bytes preserved so on-chain `statement_sha256` matches canonical disk file byte-for-byte (B1).
      const statement = fs.readFileSync(path.join(dir, f), "utf8");
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
 * bounding constant `∃ (C : ℝ)` or `∃ C` is placed INSIDE a universal quantifier.
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
  const strippedCode = stripLeanComments(entry.statement);
  for (const sub of FORBIDDEN_COMPOSITION_SUBSTRINGS) {
    if (strippedCode.includes(sub)) {
      throw new Error(
        `[COMPOSABILITY GATE FAILED] ${entry.name} contains forbidden substring '${sub}' (rejected by proof_registry::compose_generated_module)`
      );
    }
  }
  const forbiddenSet = new Set<string>(FORBIDDEN_SHARED_WORD_TOKENS);
  const codeWords = strippedCode.split(/[^\p{L}\p{N}_']+/u).filter(Boolean);
  for (const word of codeWords) {
    if (forbiddenSet.has(word)) {
      throw new Error(
        `[COMPOSABILITY GATE FAILED] ${entry.name} contains forbidden declaration/command token '${word}' (rejected by proof_registry::compose_generated_module)`
      );
    }
  }
  for (const line of entry.statement.split("\n")) {
    const trimmed = line.trimStart();
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
 * `def statementProp : Prop := ¬ statementPropOrig`.
 */
export function negateStatementProp(statement: string): string {
  const defRegex = /^(\s*(?:noncomputable\s+)?def\s+)statementProp(\b)/m;
  if (!defRegex.test(statement)) {
    throw new Error(
      "negateStatementProp: statement does not declare `def statementProp`"
    );
  }
  const renamed = statement.replace(defRegex, "$1statementPropOrig$2");
  return `${renamed.trimEnd()}\n\ndef statementProp : Prop := ¬ statementPropOrig\n`;
}

function mapSwarmImportToRelativeFiles(modName: string): string[] {
  if (!ALLOWED_SWARM_LIBRARY_IMPORTS.has(modName)) {
    throw new Error(`Unsupported SwarmProofs library import '${modName}'`);
  }
  return [`${modName.replace(/\./g, "/")}.lean`];
}

const BRIDGE_ONLY_DEF_IMPORTS = new Set([
  "import Mathlib.NumberTheory.Primorial",
  "import Mathlib.NumberTheory.ArithmeticFunction.Defs",
  "import Mathlib.NumberTheory.ArithmeticFunction.Moebius",
  "import Mathlib.NumberTheory.ArithmeticFunction.Misc",
  "import Mathlib.NumberTheory.Harmonic.Defs",
]);

function stripDefsFileForInlineCompat(raw: string): {
  imports: string[];
  body: string;
} {
  const withoutLicense = raw.replace(
    /^\s*\/-[\s\S]*?SPDX-License-Identifier:[\s\S]*?-\/\s*/,
    ""
  );
  const bridgeCutIdx = withoutLicense.search(
    /\n(?:\/-- Canonical bridge lemma|theorem bridge_)/
  );
  const defsPart =
    bridgeCutIdx >= 0 ? withoutLicense.slice(0, bridgeCutIdx) : withoutLicense;
  const imports: string[] = [];
  const keptLines: string[] = [];
  for (const line of defsPart.split("\n")) {
    const trimmed = line.trim();
    if (trimmed.startsWith("import ") || trimmed.startsWith("import\t")) {
      if (!BRIDGE_ONLY_DEF_IMPORTS.has(trimmed)) {
        imports.push(trimmed);
      }
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
 * Compatibility bridge for `--check-only` runner probes while Step 3 runner upgrade is deploying:
 * inlines `SwarmProofs.Chains.*.Defs.V1` into the runner check payload when
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

export function hasExistentialUniversalStructure(statement: string): boolean {
  const propIdx = statement.indexOf("def statementProp");
  const body = propIdx >= 0 ? statement.slice(propIdx) : statement;
  return body.includes("∃") && body.includes("∀");
}

export function resolveBridgeLemmasForStatement(rawStatement: string): {
  defsModules: string[];
  unfoldDefs: string[];
  wrapperDefs: string[];
  bridgeLemmas: string[];
} {
  const importedDefs: string[] = [];
  for (const line of rawStatement.split("\n")) {
    const trimmed = line.trim();
    if (!trimmed.startsWith("import ") && !trimmed.startsWith("import\t")) {
      continue;
    }
    const withoutComment = trimmed.split("--")[0];
    const mods = withoutComment
      .replace(/^import\s+/, "")
      .split(/\s+/)
      .filter(Boolean);
    for (const modName of mods) {
      if (modName in BRIDGE_LEMMAS_BY_DEFS_MODULE) {
        if (!importedDefs.includes(modName)) importedDefs.push(modName);
      }
    }
  }
  const defsModules =
    importedDefs.length > 0
      ? importedDefs
      : Object.keys(BRIDGE_LEMMAS_BY_DEFS_MODULE);
  const unfoldSet = new Set<string>();
  const wrapperSet = new Set<string>();
  const bridgeSet = new Set<string>();
  for (const mod of defsModules) {
    for (const d of UNFOLD_DEFS_BY_MODULE[mod] ?? []) unfoldSet.add(d);
    for (const w of WRAPPER_DEFS_BY_MODULE[mod] ?? []) wrapperSet.add(w);
    for (const lem of BRIDGE_LEMMAS_BY_DEFS_MODULE[mod] ?? [])
      bridgeSet.add(lem);
  }
  return {
    defsModules,
    unfoldDefs: [...unfoldSet],
    wrapperDefs: [...wrapperSet],
    bridgeLemmas: [...bridgeSet],
  };
}

export type MathlibProbeKind =
  | "exact"
  | "solve-by-elim"
  | "apply-assumption"
  | "hint"
  | "aesop";

export type MathlibProbeClassification = "SOLVED" | "EXHAUSTED";

export function classifyMathlibProbeSlice(
  kind: MathlibProbeKind,
  theoremIdent: string,
  slice: string
): MathlibProbeClassification {
  for (const marker of MATHLIB_FAIL_CLOSED_MARKERS) {
    if (slice.includes(marker)) {
      throw new Error(
        `[MATHLIB SEARCH FAIL-CLOSED] Probe '${theoremIdent}' (${kind}) hit resource/malformed failure '${marker}':\n${slice}`
      );
    }
  }

  const noAxiomsLine = `'${theoremIdent}' does not depend on any axioms`;
  const depRegex = new RegExp(
    `'${theoremIdent.replace(
      /[.*+?^${}()|[\]\\]/g,
      "\\$&"
    )}' depends on axioms: \\[([^\\]]*)\\]`
  );
  const depMatch = slice.match(depRegex);
  const sawNoAxioms = slice.includes(noAxiomsLine);
  if (!sawNoAxioms && !depMatch) {
    throw new Error(
      `[MATHLIB SEARCH FAIL-CLOSED] Probe '${theoremIdent}' (${kind}) missing '#print axioms' confirmation:\n${slice}`
    );
  }
  const axioms = depMatch
    ? depMatch[1]
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean)
    : [];
  const hasSorryAx = axioms.includes("sorryAx");
  const hasError = /(?:^|\n).*?:\d+:\d+:\s*error:/m.test(slice);
  if (!hasSorryAx && slice.includes("error: No goals to be solved")) {
    return "SOLVED";
  }

  switch (kind) {
    case "exact": {
      if (!hasError && !hasSorryAx) return "SOLVED";
      if (hasSorryAx && slice.includes("`exact?` could not close the goal")) {
        return "EXHAUSTED";
      }
      throw new Error(
        `[MATHLIB SEARCH FAIL-CLOSED] Unexpected 'exact?' output on '${theoremIdent}':\n${slice}`
      );
    }
    case "solve-by-elim": {
      if (!hasError && !hasSorryAx) return "SOLVED";
      if (hasSorryAx && slice.includes("swarm_solve_by_elim: exhausted")) {
        return "EXHAUSTED";
      }
      throw new Error(
        `[MATHLIB SEARCH FAIL-CLOSED] Unexpected 'solve_by_elim' output on '${theoremIdent}':\n${slice}`
      );
    }
    case "apply-assumption": {
      if (!hasError && !hasSorryAx) return "SOLVED";
      if (hasSorryAx && slice.includes("swarm_apply_assumption: exhausted (")) {
        return "EXHAUSTED";
      }
      throw new Error(
        `[MATHLIB SEARCH FAIL-CLOSED] Unexpected 'swarm_apply_assumption' output on '${theoremIdent}':\n${slice}`
      );
    }
    case "hint": {
      if (slice.includes("🎉") || (!hasError && !hasSorryAx)) {
        return "SOLVED";
      }
      if (
        hasSorryAx &&
        (slice.includes("No suggestions available") ||
          slice.includes("Try these:"))
      ) {
        return "EXHAUSTED";
      }
      throw new Error(
        `[MATHLIB SEARCH FAIL-CLOSED] Unexpected 'hint' output on '${theoremIdent}':\n${slice}`
      );
    }
    case "aesop": {
      if (!hasError && !hasSorryAx) return "SOLVED";
      if (
        hasSorryAx &&
        (slice.includes("failed to prove the goal after exhaustive search") ||
          slice.includes("Tactic `aesop` failed, made no progress") ||
          slice.includes("tactic 'aesop' failed, made no progress"))
      ) {
        return "EXHAUSTED";
      }
      throw new Error(
        `[MATHLIB SEARCH FAIL-CLOSED] Unexpected 'aesop' output on '${theoremIdent}':\n${slice}`
      );
    }
  }
}

export interface MathlibProbeSpec {
  label: string;
  kind: MathlibProbeKind;
  targetExpr: string;
  tacticBody: string;
  maxHeartbeats: number;
}

export interface MathlibSearchGateResult {
  entryName: string;
  probeCount: number;
  elapsedMs: number;
  probeVerdicts: Record<string, MathlibProbeClassification>;
}

function envWithElanPath(): NodeJS.ProcessEnv {
  const elanBin = path.join(os.homedir(), ".elan", "bin");
  const curPath = process.env.PATH ?? "";
  return {
    ...process.env,
    PATH: curPath.includes(elanBin) ? curPath : `${elanBin}:${curPath}`,
  };
}

export function isLocalLakeAvailable(
  swarmProofsRoot: string = SWARM_PROOFS_ROOT
): boolean {
  if (!fs.existsSync(path.join(swarmProofsRoot, ".lake"))) return false;
  const res = spawnSync("lake", ["--version"], {
    cwd: swarmProofsRoot,
    env: envWithElanPath(),
    encoding: "utf8",
    timeout: 10_000,
  });
  return res.status === 0;
}

function stripAllImports(raw: string): string {
  return raw
    .split("\n")
    .filter((line) => {
      const t = line.trim();
      return !t.startsWith("import ") && !t.startsWith("import\t");
    })
    .join("\n")
    .trim();
}

export function buildMathlibProbeSpecsForEntry(
  rawStatement: string,
  hasParentImplication: boolean
): MathlibProbeSpec[] {
  const { wrapperDefs, bridgeLemmas } =
    resolveBridgeLemmasForStatement(rawStatement);
  const dsimpAll =
    wrapperDefs.length > 0
      ? `  (try dsimp only [${wrapperDefs.join(", ")}] at *)\n`
      : "";
  const simpAll =
    bridgeLemmas.length > 0
      ? `  (try simp only [${bridgeLemmas.join(", ")}] at *)\n`
      : "";

  const normPos = dsimpAll + simpAll;
  const normNeg = dsimpAll + simpAll;
  const normDep = dsimpAll + simpAll;

  const specs: MathlibProbeSpec[] = [
    // Place `apply-assumption` FIRST so zero earlier theorems exist in the environment when `librarySearch` scans!
    {
      label: "positive-apply-assumption",
      kind: "apply-assumption",
      targetExpr: "statementProp",
      maxHeartbeats: 0,
      tacticBody:
        `  unfold statementProp\n` +
        `  intros\n` +
        normPos +
        `  swarm_apply_assumption`,
    },
    {
      label: "negative-apply-assumption",
      kind: "apply-assumption",
      targetExpr: "¬ statementProp",
      maxHeartbeats: 0,
      tacticBody:
        `  unfold statementProp\n` +
        `  intro __h_neg\n` +
        normNeg +
        `  swarm_apply_assumption`,
    },
    {
      label: "positive-solve-by-elim",
      kind: "solve-by-elim",
      targetExpr: "statementProp",
      maxHeartbeats: 0,
      tacticBody:
        `  unfold statementProp\n` +
        `  intros\n` +
        normPos +
        `  first | done | solve_by_elim (maxDepth := 4) | fail "swarm_solve_by_elim: exhausted"`,
    },
    {
      label: "negative-solve-by-elim",
      kind: "solve-by-elim",
      targetExpr: "¬ statementProp",
      maxHeartbeats: 0,
      tacticBody:
        `  unfold statementProp\n` +
        `  intro __h_neg\n` +
        normNeg +
        `  first | done | solve_by_elim (maxDepth := 4) | fail "swarm_solve_by_elim: exhausted"`,
    },
    {
      label: "positive-exact",
      kind: "exact",
      targetExpr: "statementProp",
      maxHeartbeats: 0,
      tacticBody:
        `  unfold statementProp\n` +
        `  intros\n` +
        normPos +
        `  first | (refine And.intro ?_ ?_ <;> exact?) | exact?`,
    },
    {
      label: "negative-exact",
      kind: "exact",
      targetExpr: "¬ statementProp",
      maxHeartbeats: 0,
      tacticBody: `  unfold statementProp\n` + normPos + `  exact?`,
    },
    {
      label: "positive-hint",
      kind: "hint",
      targetExpr: "statementProp",
      maxHeartbeats: 0,
      tacticBody:
        `  unfold statementProp\n` + `  intros\n` + normPos + `  hint`,
    },
    {
      label: "negative-hint",
      kind: "hint",
      targetExpr: "¬ statementProp",
      maxHeartbeats: 0,
      tacticBody:
        `  unfold statementProp\n` + `  intro __h_neg\n` + normNeg + `  hint`,
    },
    {
      label: "positive-aesop",
      kind: "aesop",
      targetExpr: "statementProp",
      maxHeartbeats: 0,
      tacticBody:
        `  unfold statementProp\n` +
        `  intros\n` +
        normPos +
        `  aesop (config := { maxRuleApplicationDepth := 30, maxRuleApplications := 200 })`,
    },
    {
      label: "negative-aesop",
      kind: "aesop",
      targetExpr: "¬ statementProp",
      maxHeartbeats: 0,
      tacticBody:
        `  unfold statementProp\n` +
        `  intro __h_neg\n` +
        normNeg +
        `  aesop (config := { maxRuleApplicationDepth := 30, maxRuleApplications := 200 })`,
    },
  ];

  if (hasParentImplication) {
    specs.push(
      {
        label: "dependency-exact",
        kind: "exact",
        targetExpr: "parentStatementProp → statementProp",
        maxHeartbeats: 0,
        tacticBody:
          `  unfold parentStatementProp statementProp\n` +
          `  intro __h_parent\n` +
          `  intros\n` +
          normDep +
          `  exact?`,
      },
      {
        label: "dependency-aesop",
        kind: "aesop",
        targetExpr: "parentStatementProp → statementProp",
        maxHeartbeats: 0,
        tacticBody:
          `  unfold parentStatementProp statementProp\n` +
          `  intro __h_parent\n` +
          `  intros\n` +
          normDep +
          `  aesop (config := { maxRuleApplicationDepth := 30, maxRuleApplications := 200 })`,
      }
    );
  }

  return specs;
}

export interface RunMathlibSearchOptions {
  swarmProofsRoot?: string;
  promotedDependencyModuleName?: string;
  parentRawStatement?: string;
  wallTimeoutMs?: number;
  onlyLabels?: Set<string>;
}

/**
 * Runs the full-`import Mathlib` + `Defs.V1` bridge-lemma search gate via `lake env lean`
 * inside `proofs/SwarmProofs`, enforcing `set_option maxHeartbeats 0` + 600s OS wall-clock
 * fail-closed cap and strict exhaustion parsing on every probe.
 */
export function runFullMathlibSearchProbes(
  entry: CatalogEntry,
  opts: RunMathlibSearchOptions = {}
): MathlibSearchGateResult {
  const swarmProofsRoot = opts.swarmProofsRoot ?? SWARM_PROOFS_ROOT;
  const wallTimeoutMs = opts.wallTimeoutMs ?? 600_000;
  const t0 = Date.now();

  const combinedForImports = opts.parentRawStatement
    ? `${opts.parentRawStatement}\n${entry.statement}`
    : entry.statement;
  const { defsModules } = resolveBridgeLemmasForStatement(combinedForImports);

  const headerLines = [
    "import Mathlib",
    ...defsModules.map((m) => `import ${m}`),
    ...(opts.promotedDependencyModuleName
      ? [`import ${opts.promotedDependencyModuleName}`]
      : []),
    "",
    "set_option linter.unusedTactic false",
    "set_option linter.unusedSimpArgs false",
    "set_option linter.unreachableTactic false",
    "set_option linter.style.nameCheck false",
    "set_option maxRecDepth 16384",
    "",
    'elab "swarm_apply_assumption" : tactic => do',
    "  let goal ← Lean.Elab.Tactic.getMainGoal",
    "  goal.withContext do",
    "    let savedMCtx ← Lean.getMCtx",
    "    match ← Lean.Meta.LibrarySearch.librarySearch goal",
    "      (fun gs => Lean.Meta.LibrarySearch.solveByElim [] false gs 1) with",
    "    | none =>",
    "      Lean.Elab.Tactic.setGoals []",
    '      Lean.logInfo "swarm_apply_assumption: closed"',
    "    | some #[] =>",
    '      throwError "swarm_apply_assumption: exhausted (0 partial candidates)"',
    "    | some candidates =>",
    "      for (remGoals, candMCtx) in candidates do",
    "        Lean.setMCtx candMCtx",
    "        let closedAll ← (Lean.Meta.withAssignableSyntheticOpaque do",
    "          for (g : Lean.MVarId) in remGoals do",
    "            let assigned ← Lean.MVarId.isAssigned g",
    "            if !assigned then",
    "              Lean.MVarId.assumption g",
    "          let proofExpr ← Lean.instantiateMVars (Lean.mkMVar goal)",
    "          return (!proofExpr.hasExprMVar && !proofExpr.hasLevelMVar)) <|> pure false",
    "        if closedAll then",
    "          Lean.Elab.Tactic.setGoals []",
    '          Lean.logInfo "swarm_apply_assumption: closed"',
    "          return",
    "      Lean.setMCtx savedMCtx",
    '      throwError s!"swarm_apply_assumption: exhausted ({candidates.size} partial candidates, none closed by assumption)"',
    "",
  ];

  let parentDecls = "";
  if (opts.parentRawStatement) {
    const defRegex = /^(\s*(?:noncomputable\s+)?def\s+)statementProp(\b)/m;
    parentDecls =
      stripAllImports(opts.parentRawStatement).replace(
        defRegex,
        "$1parentStatementProp$2"
      ) + "\n\n";
  }

  const childDecls = stripAllImports(entry.statement);
  let specs = buildMathlibProbeSpecsForEntry(
    combinedForImports,
    Boolean(opts.parentRawStatement)
  );
  if (opts.onlyLabels) {
    specs = specs.filter((s) => opts.onlyLabels!.has(s.label));
  }

  const probeBlocks: string[] = [];
  specs.forEach((spec, idx) => {
    const guardName = `__SwarmProbeGuard_${idx}`;
    const thmName = `__swarm_probe_${idx}_${spec.label.replace(/-/g, "_")}`;
    probeBlocks.push(
      `#check "===SWARM_PROBE_BEGIN:${spec.label}==="\n` +
        `private structure ${guardName} : Type where dummy : Unit\n` +
        `set_option maxHeartbeats ${spec.maxHeartbeats} in\n` +
        `theorem ${thmName} : ${guardName} → (${spec.targetExpr}) := by\n` +
        `  intro __g; clear __g\n` +
        `${spec.tacticBody}\n` +
        `#print axioms ${thmName}\n` +
        `#check "===SWARM_PROBE_END:${spec.label}==="\n`
    );
  });

  const fullLeanSource =
    headerLines.join("\n") +
    parentDecls +
    childDecls +
    "\n\n" +
    probeBlocks.join("\n");

  const proc = spawnSync("lake", ["env", "lean", "--stdin"], {
    cwd: swarmProofsRoot,
    env: envWithElanPath(),
    input: fullLeanSource,
    encoding: "utf8",
    timeout: wallTimeoutMs,
    maxBuffer: 32 * 1024 * 1024,
  });

  if (proc.error) {
    throw new Error(
      `[MATHLIB SEARCH FAIL-CLOSED] ${entry.name}: lake env lean execution error/timeout: ${proc.error.message}`
    );
  }
  if (proc.signal) {
    throw new Error(
      `[MATHLIB SEARCH FAIL-CLOSED] ${entry.name}: lake env lean killed by signal ${proc.signal}`
    );
  }

  const combinedOut = `${proc.stdout ?? ""}\n${proc.stderr ?? ""}`;
  const firstMarkerIdx = combinedOut.indexOf("===SWARM_PROBE_BEGIN:");
  const preamble =
    firstMarkerIdx >= 0 ? combinedOut.slice(0, firstMarkerIdx) : combinedOut;
  if (/(?:^|\n).*?:\d+:\d+:\s*error:/m.test(preamble)) {
    throw new Error(
      `[MATHLIB SEARCH FAIL-CLOSED] ${entry.name}: statement failed elaboration before probes:\n${preamble}`
    );
  }

  const probeVerdicts: Record<string, MathlibProbeClassification> = {};
  specs.forEach((spec, idx) => {
    const beginTag = `"===SWARM_PROBE_BEGIN:${spec.label}===" : String`;
    const endTag = `"===SWARM_PROBE_END:${spec.label}===" : String`;
    const bIdx = combinedOut.indexOf(beginTag);
    const eIdx = combinedOut.indexOf(endTag);
    if (bIdx < 0 || eIdx < 0 || eIdx < bIdx) {
      throw new Error(
        `[MATHLIB SEARCH FAIL-CLOSED] ${entry.name}: missing delimiter output for probe '${spec.label}':\n${combinedOut}`
      );
    }
    const slice = combinedOut.slice(bIdx + beginTag.length, eIdx);
    const thmName = `__swarm_probe_${idx}_${spec.label.replace(/-/g, "_")}`;
    const verdict = classifyMathlibProbeSlice(spec.kind, thmName, slice);
    probeVerdicts[spec.label] = verdict;
  });

  const reportOrder = [...specs].sort((a, b) => {
    const rank = (k: string) =>
      k === "exact"
        ? 0
        : k === "apply-assumption" || k === "solve-by-elim"
        ? 1
        : k === "hint"
        ? 2
        : 3;
    return rank(a.kind) - rank(b.kind);
  });
  for (const spec of reportOrder) {
    if (probeVerdicts[spec.label] === "SOLVED") {
      throw new Error(
        `[ANTI-VACUITY / TRIVIALITY GATE FAILED] ${entry.name} was solved trivially by full-Mathlib probe '${spec.label}'! Refusing to seed.`
      );
    }
  }

  return {
    entryName: entry.name,
    probeCount: specs.length,
    elapsedMs: Date.now() - t0,
    probeVerdicts,
  };
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

/**
 * Fast tactic-blast & structural anti-vacuity probes executed on `shillbot-lean-runner`
 * (complementing `runFullMathlibSearchProbes` which runs full `import Mathlib` search).
 */
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
  const depImportPrefix = promotedDependencyModuleName
    ? `import ${promotedDependencyModuleName}\n`
    : "";

  const probes: TrivialityProbe[] = [];

  // N4: Only run existential-constant vacuity probes when statementProp has `∃` + `∀` quantifier structure.
  if (hasExistentialUniversalStructure(effectiveStatement)) {
    probes.push(
      {
        label: "zero-constant-vacuity",
        statement: effectiveStatement,
        maxBuildSecs: 240,
        proof:
          `${depImportPrefix}theorem proof : statementProp := by\n` +
          `  refine ⟨0, le_rfl, fun N _ => ?_⟩\n` +
          `  simp`,
      },
      {
        label: "pointwise-ratio-shortcut",
        statement: effectiveStatement,
        maxBuildSecs: 240,
        proof:
          `${depImportPrefix}theorem proof : statementProp := by\n` +
          `  intro N hN\n` +
          `  refine ⟨0, le_rfl, ?_⟩\n` +
          `  exact le_rfl`,
      }
    );
  }

  probes.push(
    {
      label: "empty-domain-contradiction",
      statement: effectiveStatement,
      maxBuildSecs: 240,
      proof:
        `${depImportPrefix}theorem proof : statementProp := by\n` +
        `  unfold statementProp\n` +
        `  intros\n` +
        `  omega`,
    },
    {
      label: "positive-tactic-blast-triviality",
      statement: effectiveStatement,
      maxBuildSecs: 240,
      proof:
        `${depImportPrefix}import Mathlib.Tactic.NormNum\nimport Mathlib.Tactic.Ring\nimport Mathlib.Tactic.Linarith\nimport Mathlib.Tactic.Tauto\nimport Aesop\n` +
        `theorem proof : statementProp := by\n` +
        `  unfold statementProp\n` +
        `  intros\n` +
        `  first | rfl | decide | omega | norm_num | ring | linarith | nlinarith | aesop | tauto | simp`,
    },
    {
      label: "negative-tactic-blast-disproof",
      statement: negatedStatement,
      maxBuildSecs: 240,
      proof:
        `${depImportPrefix}import Mathlib.Tactic.NormNum\nimport Mathlib.Tactic.Ring\nimport Mathlib.Tactic.Linarith\nimport Mathlib.Tactic.Tauto\nimport Aesop\n` +
        `theorem proof : statementProp := by\n` +
        `  unfold statementProp statementPropOrig\n` +
        `  intro h\n` +
        `  first | exact False.elim (by omega) | norm_num at h | omega | simp at h | aesop`,
    }
  );

  if (parentImplicationStatement) {
    probes.push({
      label: "dependency-consequence-tactic-aesop",
      statement: parentImplicationStatement,
      maxBuildSecs: 240,
      proof:
        `${depImportPrefix}import Mathlib.Tactic.Linarith\nimport Aesop\n` +
        `theorem proof : statementProp := by\n` +
        `  unfold statementProp parentStatementProp childStatementProp\n` +
        `  intro hParent\n` +
        `  intros\n` +
        `  first | exact hParent | assumption | simp [hParent] | omega | linarith | aesop`,
    });
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
        `[MALFORMED PROBE / RESOURCE TIMEOUT FAIL-CLOSED] ${entryName} (${probeLabel}) hit '${marker}': ${detail}`
      );
    }
  }
  if (resp.verdict === "fail") {
    const trimmed = detail.trim();
    if (trimmed.length === 0) {
      throw new Error(
        `[PRE-SEED GATE FAIL-CLOSED] ${entryName} (${probeLabel}): empty 'fail' detail from runner`
      );
    }
    const lower = trimmed.toLowerCase();
    const hasValidIndicator = VALID_RUNNER_FAIL_INDICATORS.some((ind) =>
      lower.includes(ind)
    );
    if (!hasValidIndicator) {
      throw new Error(
        `[PRE-SEED GATE FAIL-CLOSED] ${entryName} (${probeLabel}): unrecognized 'fail' detail without tactic/goal failure marker: ${detail}`
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
  runLocalMathlibSearch?: boolean;
  skipRunnerStage?: boolean;
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
  const probeConcurrency = Math.max(1, opts.probeConcurrency ?? 2);

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

  const registry = opts.registry ?? loadProofRegistry();
  const resolvedDep = entry.dependsOn
    ? resolveRungDependency(entry.dependsOn, registry)
    : undefined;

  let parentRaw: string | undefined;
  if (entry.dependsOn) {
    const parentEntry = opts.catalogByName?.get(entry.dependsOn);
    parentRaw =
      parentEntry?.statement ??
      (() => {
        const candidatePath = path.join(
          CATALOG_DIR,
          "open",
          `${entry.dependsOn}.lean`
        );
        return fs.existsSync(candidatePath)
          ? fs.readFileSync(candidatePath, "utf8")
          : undefined;
      })();
  }

  const shouldRunMathlibSearch =
    opts.runLocalMathlibSearch ?? opts.fetchImpl === undefined;

  if (shouldRunMathlibSearch) {
    const promotedMod =
      resolvedDep?.status === "active" ? resolvedDep.moduleName : undefined;
    const mathlibRes = runFullMathlibSearchProbes(entry, {
      promotedDependencyModuleName: promotedMod,
      parentRawStatement: parentRaw,
    });
    console.log(
      `  [mathlib-search PASS] ${entry.name}: all ${mathlibRes.probeCount} full-Mathlib probes exhausted cleanly (${mathlibRes.elapsedMs}ms)`
    );
  }

  if (opts.skipRunnerStage) {
    return;
  }

  const token = opts.runnerToken ?? getRunnerIdToken(runnerUrl);
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
  };
  if (token) headers.Authorization = `Bearer ${token}`;

  const health =
    opts.runnerHealth ??
    (await fetchRunnerHealth(runnerUrl, headers, fetchImpl));

  if (!checkOnly) {
    assertSeedingAllowedForEntry(entry, leanPolicy, registry, health);
  }

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
      `  [companion PASS] solvable/${entry.name} companion proof verified; running runner tactic probes...`
    );
  }

  const parentImplicationStatement = parentRaw
    ? buildParentImplicationStatement(
        parentRaw,
        entry.statement,
        !useComposableMode
      )
    : undefined;

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
        return { canary, cJson };
      })
    );
    for (const { canary, cJson } of results) {
      if (cJson.verdict === "pass") {
        assertValidProbeResponse(cJson, entry.name, canary.label);
        throw new Error(
          `[ANTI-VACUITY / TRIVIALITY GATE FAILED] ${entry.name} was solved trivially by probe '${canary.label}'! Refusing to seed.`
        );
      }
    }
    for (const { canary, cJson } of results) {
      assertValidProbeResponse(cJson, entry.name, canary.label);
    }
  }

  console.log(
    `  [runner check PASS] ${entry.name} elaborated cleanly & rejected all ${
      probes.length
    } runner probes (${Date.now() - t0}ms)`
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

export function parseNetworkArg(
  argv: string[] = process.argv,
  env: NodeJS.ProcessEnv = process.env
): "mainnet" | "devnet" {
  const hasMainnetFlag = argv.includes("--mainnet");
  const eqArg = argv.find((a) => a.startsWith("--network="));
  const idx = argv.indexOf("--network");
  const explicitRaw = eqArg
    ? eqArg.slice("--network=".length).trim().toLowerCase()
    : idx >= 0 && idx + 1 < argv.length
    ? argv[idx + 1].trim().toLowerCase()
    : undefined;
  if (explicitRaw !== undefined) {
    if (explicitRaw !== "mainnet" && explicitRaw !== "devnet") {
      throw new Error(
        `Invalid --network value '${explicitRaw}'; expected 'devnet' or 'mainnet'`
      );
    }
    if (hasMainnetFlag && explicitRaw !== "mainnet") {
      throw new Error("Conflicting --mainnet and --network=devnet flags");
    }
    return explicitRaw;
  }
  if (hasMainnetFlag || env.SOLANA_NETWORK === "mainnet") {
    return "mainnet";
  }
  return "devnet";
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

export function parseDeadlineDaysArg(argv: string[]): number {
  const eq = argv.find((a) => a.startsWith("--deadline-days="));
  const idx = argv.indexOf("--deadline-days");
  const raw = eq
    ? eq.slice("--deadline-days=".length).trim()
    : idx >= 0 && idx + 1 < argv.length
    ? argv[idx + 1].trim()
    : undefined;
  if (!raw) return 90;
  const parsed = Number(raw);
  if (!Number.isInteger(parsed) || parsed < 1 || parsed > 90) {
    throw new Error(
      `Invalid --deadline-days value '${raw}'; expected integer in 1..=90`
    );
  }
  return parsed;
}

async function main(): Promise<void> {
  if (process.argv.includes("--skip-precheck")) {
    throw new Error(
      "--skip-precheck is forbidden: every seeded statement must pass the triviality & elaboration gate"
    );
  }
  const parsedNetwork = parseNetworkArg(process.argv, process.env);
  const isMainnet = parsedNetwork === "mainnet";
  const checkOnly = process.argv.includes("--check-only");
  const mathlibOnly = process.argv.includes("--mathlib-only");
  const runnerOnly = process.argv.includes("--runner-only");
  const isLadder = process.argv.includes("--ladder");
  const isChains = process.argv.includes("--chains");
  const taskArg = process.argv.find((a) => a.startsWith("--tasks="));
  const reservedForWalletArg = process.argv
    .find((a) => a.startsWith("--reserved-for-wallet="))
    ?.replace("--reserved-for-wallet=", "")
    .trim();
  const network = parsedNetwork;
  const leanPolicy = parseLeanPolicyArg(process.argv);
  const deadlineDays = parseDeadlineDaysArg(process.argv);

  if ((mathlibOnly || runnerOnly) && !checkOnly) {
    throw new Error(
      "--mathlib-only and --runner-only are only permitted with --check-only"
    );
  }

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

  if (isMainnet && !taskArg) {
    const skippedRh = catalog
      .filter((e) => RETIRED_RH_SLUGS.has(e.name))
      .map((e) => e.name);
    if (skippedRh.length > 0) {
      console.log(
        `excluding ${
          skippedRh.length
        } retired post-Alpöge RH slug(s) from default mainnet seeding: ${skippedRh.join(
          ", "
        )}`
      );
      catalog = catalog.filter((e) => !RETIRED_RH_SLUGS.has(e.name));
    }
  }

  console.log(
    `catalog: ${catalog.length} statements (${catalog
      .map((e) => e.name)
      .join(", ")})`
  );

  const registry = loadProofRegistry();

  if (checkOnly && mathlibOnly) {
    for (const entry of catalog) {
      await verifyStatementElaborates(entry, {
        checkOnly: true,
        leanPolicy,
        registry,
        catalogByName,
        runLocalMathlibSearch: true,
        skipRunnerStage: true,
      });
    }
    console.log(
      `\n[mathlib-only check PASS] All ${catalog.length} statements passed full-Mathlib search gate.`
    );
    return;
  }

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
    if (isChains && !runnerOnly) {
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
        runLocalMathlibSearch: !runnerOnly,
        skipRunnerStage: false,
      });
    }
    console.log(
      `\n[check-only PASS] All ${catalog.length} statements passed elaboration & anti-vacuity/triviality gate.`
    );
    return;
  }

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
  console.log(`deadline: ${deadlineDays}d`);
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
      runLocalMathlibSearch: true,
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
      deadline_days: deadlineDays,
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
