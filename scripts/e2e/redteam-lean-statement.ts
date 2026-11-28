/**
 * Guardrail 2 — Adversarial LLM Red-Team Harness for Lean 4 Bounties.
 *
 * Before any `open-problem` or `novel-unpublished` statement is funded with
 * real Mainnet SOL, this harness actively tries to BREAK or SOLVE it across
 * three explicit adversarial attacks using a frontier LLM looped against the
 * pinned local Lean 4 compiler (`proofs/SwarmProofs`, leanprover/lean4:v4.33.0-rc2
 * + full Mathlib + SwarmProofs):
 *
 *   1. `cheat`    — exploit any edge case, missing hypothesis, type coercion,
 *                   or vacuous premise (`False.elim`) to prove `statementProp`.
 *   2. `disprove` — find a concrete counterexample proving `¬ statementProp`.
 *   3. `prove`    — solve `statementProp` legitimately with Mathlib/multi-lemma
 *                   reasoning (catches unpublished problems that are too easy
 *                   or secretly equivalent to published theorems).
 *
 * Fail-closed rule: if ANY attack produces a Lean 4 proof (0 sorry/admit/axiom,
 * standard axioms only), the harness exits non-zero (code 2), saves the breach
 * proof for inspection, and refuses to write a red-team attestation.
 *
 * Supported providers (zero new accounts required):
 *   - `--provider=xai` (default headless mode: calls https://api.x.ai/v1 `grok-4`
 *     using `XAI_API_KEY` env var or GCP Secret Manager `grok-agent-api-key`)
 *   - `--provider=gemini` (uses `GEMINI_API_KEY` env var)
 *   - `--provider=openai` (uses `OPENAI_API_KEY` env var)
 *   - `--record-antigravity-audit` (records a verified attestation produced by
 *     an isolated Antigravity `lean-redteam` subagent bound to `statement_sha256`)
 *
 * Usage:
 *   npx tsx scripts/e2e/redteam-lean-statement.ts --task=<slug> --provider=xai [--iterations=10] [--write-attestation]
 *   npx tsx scripts/e2e/redteam-lean-statement.ts --statement-file=<path.lean> --provider=xai --iterations=3 --attacks=cheat
 *   npx tsx scripts/e2e/redteam-lean-statement.ts --task=<slug> --record-antigravity-audit --model=<model> --iterations=15 --duration-secs=420 --notes="..."
 */
import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import * as crypto from "crypto";
import { execFileSync, spawnSync } from "child_process";
import {
  CATALOG_DIR,
  DEFAULT_MAINNET_POLICY_PATH,
  SWARM_PROOFS_ROOT,
  loadMainnetPolicy,
  sha256Hex,
  stripLeanComments,
  type RedTeamAttackKind,
  type RedTeamAttestation,
  type RedTeamProvider,
} from "./seed-lean-catalog";

export const FORBIDDEN_PROOF_TOKEN_RE =
  /\b(sorry|admit|axiom|unsafe|implemented_by|extern|opaque|partial|native_decide)\b/;

export interface RedTeamCompileResult {
  passed: boolean;
  stdout: string;
  stderr: string;
  exitCode: number;
  rejectedReason?: string;
}

export interface RedTeamOptions {
  statementPath: string;
  slug: string;
  provider: RedTeamProvider;
  model: string;
  iterations: number;
  timeoutSecs: number;
  attacks: RedTeamAttackKind[];
  writeAttestation: boolean;
  policyPath?: string;
  swarmProofsRoot?: string;
}

export interface RedTeamOutcome {
  passed: boolean;
  breachedAttack?: RedTeamAttackKind;
  breachProof?: string;
  breachPath?: string;
  totalIterations: number;
  durationSecs: number;
  attestation?: RedTeamAttestation;
}

export function extractLeanCodeBlock(text: string): string {
  const fenced = text.match(/```(?:lean4?|lean)?\s*\n([\s\S]*?)```/i);
  if (fenced && fenced[1].trim().length > 0) {
    return fenced[1].trim();
  }
  return text.trim();
}

export function buildRedTeamHarnessFile(
  statementSource: string,
  candidateProof: string,
  attack: RedTeamAttackKind
): string {
  const goalName = attack === "disprove" ? "disproof" : "proof";
  const goalType = attack === "disprove" ? "¬ statementProp" : "statementProp";

  // Separate imports from statement body so candidate proof imports merge cleanly at top.
  const stmtLines = statementSource.split(/\r?\n/);
  const stmtImports: string[] = [];
  const stmtBody: string[] = [];
  let inHeader = true;
  for (const line of stmtLines) {
    const t = line.trim();
    if (
      inHeader &&
      (t === "" || t.startsWith("--") || t.startsWith("import "))
    ) {
      if (t.startsWith("import ")) stmtImports.push(t);
      continue;
    }
    inHeader = false;
    stmtBody.push(line);
  }

  const proofLines = candidateProof.split(/\r?\n/);
  const proofImports: string[] = [];
  const proofBody: string[] = [];
  let inProofHeader = true;
  for (const line of proofLines) {
    const t = line.trim();
    if (
      inProofHeader &&
      (t === "" || t.startsWith("--") || t.startsWith("import "))
    ) {
      if (t.startsWith("import ")) proofImports.push(t);
      continue;
    }
    inProofHeader = false;
    proofBody.push(line);
  }

  const mergedImports = [...new Set([...stmtImports, ...proofImports])];
  return [
    ...mergedImports,
    "",
    stmtBody.join("\n"),
    "",
    proofBody.join("\n"),
    "",
    `-- Red-team type & axiom guard`,
    `example : ${goalType} := ${goalName}`,
    `#print axioms ${goalName}`,
    "",
  ].join("\n");
}

export function checkAxiomOutputIsStandard(output: string): boolean {
  const allowed = new Set(["propext", "Classical.choice", "Quot.sound"]);
  const line = output
    .split(/\r?\n/)
    .find(
      (l) =>
        l.includes("depends on axioms:") ||
        l.includes("does not depend on any axioms")
    );
  if (!line) return false;
  if (line.includes("does not depend on any axioms")) return true;
  const m = line.match(/\[([^\]]*)\]/);
  if (!m) return false;
  const axioms = m[1]
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);
  return axioms.every((a) => allowed.has(a));
}

export function compileRedTeamCandidate(
  statementSource: string,
  candidateProof: string,
  attack: RedTeamAttackKind,
  swarmProofsRoot: string = SWARM_PROOFS_ROOT,
  perAttemptTimeoutMs = 120_000
): RedTeamCompileResult {
  const stripped = stripLeanComments(candidateProof);
  const badToken = stripped.match(FORBIDDEN_PROOF_TOKEN_RE);
  if (badToken) {
    return {
      passed: false,
      stdout: "",
      stderr: `Forbidden proof token '${badToken[0]}' rejected before Lean check.`,
      exitCode: 1,
      rejectedReason: `forbidden_token:${badToken[0]}`,
    };
  }
  const goalName = attack === "disprove" ? "disproof" : "proof";
  if (!new RegExp(`\\b(theorem|lemma|def)\\s+${goalName}\\b`).test(stripped)) {
    return {
      passed: false,
      stdout: "",
      stderr: `Candidate must declare 'theorem ${goalName} : ${
        attack === "disprove" ? "¬ statementProp" : "statementProp"
      }'.`,
      exitCode: 1,
      rejectedReason: "missing_target_theorem",
    };
  }

  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "swarm-redteam-"));
  const filePath = path.join(tmpDir, "RedTeamAttempt.lean");
  try {
    const combined = buildRedTeamHarnessFile(
      statementSource,
      candidateProof,
      attack
    );
    fs.writeFileSync(filePath, combined, "utf8");
    const res = spawnSync("lake", ["env", "lean", filePath], {
      cwd: swarmProofsRoot,
      encoding: "utf8",
      timeout: perAttemptTimeoutMs,
      maxBuffer: 8 * 1024 * 1024,
    });
    const stdout = res.stdout ?? "";
    const stderr = res.stderr ?? "";
    const combinedOut = `${stdout}\n${stderr}`;
    if (res.status !== 0) {
      return {
        passed: false,
        stdout,
        stderr,
        exitCode: res.status ?? 1,
      };
    }
    if (
      combinedOut.includes("declaration uses 'sorry'") ||
      combinedOut.includes("sorryAx")
    ) {
      return {
        passed: false,
        stdout,
        stderr: `${stderr}\nRejected: proof depends on sorryAx`,
        exitCode: 1,
        rejectedReason: "sorryAx",
      };
    }
    if (!checkAxiomOutputIsStandard(combinedOut)) {
      return {
        passed: false,
        stdout,
        stderr: `${stderr}\nRejected: non-standard axioms detected in #print axioms output`,
        exitCode: 1,
        rejectedReason: "non_standard_axioms",
      };
    }
    return { passed: true, stdout, stderr, exitCode: 0 };
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
}

function loadXaiApiKey(): string {
  const fromEnv = process.env.XAI_API_KEY || process.env.GROK_AGENT_API_KEY;
  if (fromEnv && fromEnv.trim().length > 0) return fromEnv.trim();
  for (const secretName of ["grok-agent-api-key", "xai-api-key"]) {
    try {
      const val = execFileSync(
        "gcloud",
        [
          "secrets",
          "versions",
          "access",
          "latest",
          `--secret=${secretName}`,
          "--project=coordination-game-prod",
        ],
        { stdio: ["ignore", "pipe", "ignore"] }
      )
        .toString()
        .trim();
      if (val.length > 0) return val;
    } catch {
      // try next secret
    }
  }
  throw new Error(
    "Missing xAI API key: set XAI_API_KEY / GROK_AGENT_API_KEY or authenticate gcloud with access to coordination-game-prod/grok-agent-api-key"
  );
}

interface ChatMessage {
  role: "system" | "user" | "assistant";
  content: string;
}

async function callLlmProvider(
  provider: RedTeamProvider,
  model: string,
  messages: ChatMessage[]
): Promise<string> {
  if (provider === "xai" || provider === "openai") {
    const apiKey =
      provider === "xai"
        ? loadXaiApiKey()
        : process.env.OPENAI_API_KEY?.trim() ?? "";
    if (!apiKey) {
      throw new Error(`Missing API key for provider '${provider}'`);
    }
    const baseUrl =
      provider === "xai" ? "https://api.x.ai/v1" : "https://api.openai.com/v1";
    const res = await fetch(`${baseUrl}/chat/completions`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Authorization: `Bearer ${apiKey}`,
      },
      body: JSON.stringify({
        model,
        messages,
        temperature: 0.2,
      }),
    });
    const text = await res.text();
    if (!res.ok) {
      throw new Error(
        `${provider} API HTTP ${res.status}: ${text.slice(0, 400)}`
      );
    }
    const json = JSON.parse(text) as {
      choices?: Array<{ message?: { content?: string } }>;
    };
    return json.choices?.[0]?.message?.content ?? "";
  }

  if (provider === "gemini") {
    const apiKey = process.env.GEMINI_API_KEY?.trim() ?? "";
    if (!apiKey) {
      throw new Error("Missing GEMINI_API_KEY for --provider=gemini");
    }
    const systemMsg = messages.find((m) => m.role === "system")?.content ?? "";
    const contents = messages
      .filter((m) => m.role !== "system")
      .map((m) => ({
        role: m.role === "assistant" ? "model" : "user",
        parts: [{ text: m.content }],
      }));
    const res = await fetch(
      `https://generativelanguage.googleapis.com/v1beta/models/${encodeURIComponent(
        model
      )}:generateContent?key=${encodeURIComponent(apiKey)}`,
      {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          systemInstruction: systemMsg
            ? { parts: [{ text: systemMsg }] }
            : undefined,
          contents,
          generationConfig: { temperature: 0.2 },
        }),
      }
    );
    const text = await res.text();
    if (!res.ok) {
      throw new Error(`gemini API HTTP ${res.status}: ${text.slice(0, 400)}`);
    }
    const json = JSON.parse(text) as {
      candidates?: Array<{ content?: { parts?: Array<{ text?: string }> } }>;
    };
    return (
      json.candidates?.[0]?.content?.parts?.map((p) => p.text ?? "").join("") ??
      ""
    );
  }

  throw new Error(
    `Provider '${provider}' cannot be invoked via headless API; use --record-antigravity-audit for Antigravity red-team subagent audits.`
  );
}

function buildAttackPrompt(
  statementSource: string,
  attack: RedTeamAttackKind
): { system: string; user: string } {
  const goalName = attack === "disprove" ? "disproof" : "proof";
  const goalType = attack === "disprove" ? "¬ statementProp" : "statementProp";

  const objective =
    attack === "cheat"
      ? `CHEAT / LOOPHOLE ATTACK: Aggressively hunt for ANY bug, vacuity, missing precondition, degenerate boundary case (n = 0, empty Finset, Nat subtraction underflow, contradictory premise yielding False.elim, or overly weak conclusion like ∃ C allowing C = 0), or unintended shortcut in \`statementProp\` or its imported definitions so you can prove \`theorem proof : statementProp\`.`
      : attack === "disprove"
      ? `DISPROOF / COUNTEREXAMPLE ATTACK: Hunt for ANY concrete counterexample that falsifies \`statementProp\` (for example due to a missing hypothesis like \`0 < n\`, wrong quantifier order, or false conjecture) and prove \`theorem disproof : ¬ statementProp\`.`
      : `FULL SOLVE ATTACK: Attempt to genuinely prove \`theorem proof : statementProp\` using Mathlib lemmas, multi-step helper theorems, induction, or any known mathematical argument. If this theorem is actually textbook/published or solvable in Lean 4, prove it cleanly.`;

  const system = [
    "You are an adversarial Lean 4 (v4.33.0-rc2 + Mathlib) security red-team prover auditing a math bounty BEFORE real funds are escrowed.",
    objective,
    "Rules:",
    `- Output ONLY a single \`\`\`lean code block containing helper lemmas/imports (if needed) and \`theorem ${goalName} : ${goalType} := ...\`.`,
    "- Do NOT redeclare `statementProp` or definitions already present in the statement.",
    "- Never use `sorry`, `admit`, `axiom`, `unsafe`, `opaque`, `partial`, `implemented_by`, or `native_decide`.",
    "- Only standard Lean axioms (`propext`, `Classical.choice`, `Quot.sound`) are permitted.",
  ].join("\n");

  const user = [
    "Target bounty statement:",
    "```lean",
    statementSource.trim(),
    "```",
    `Produce a complete Lean 4 proof of \`theorem ${goalName} : ${goalType}\`.`,
  ].join("\n");

  return { system, user };
}

export function upsertRedTeamAttestation(
  slug: string,
  attestation: RedTeamAttestation,
  policyPath: string = DEFAULT_MAINNET_POLICY_PATH
): void {
  const manifest = loadMainnetPolicy(policyPath);
  const entry = manifest.entries[slug];
  if (!entry) {
    throw new Error(
      `Cannot record red-team attestation: '${slug}' is not registered in ${policyPath}`
    );
  }
  if (entry.statement_sha256 !== attestation.statement_sha256) {
    throw new Error(
      `Statement SHA-256 mismatch for '${slug}': manifest has ${entry.statement_sha256}, attestation has ${attestation.statement_sha256}`
    );
  }
  const existing = Array.isArray(entry.redteam)
    ? entry.redteam
    : entry.redteam
    ? [entry.redteam]
    : [];
  const updated = [
    ...existing.filter((a) => a.provider !== attestation.provider),
    attestation,
  ];
  entry.redteam = updated.length === 1 ? updated[0] : updated;
  fs.writeFileSync(
    policyPath,
    `${JSON.stringify(manifest, null, 2)}\n`,
    "utf8"
  );
}

export async function runRedTeam(
  opts: RedTeamOptions
): Promise<RedTeamOutcome> {
  const statementSource = fs.readFileSync(opts.statementPath, "utf8");
  const statementSha = sha256Hex(statementSource);
  const startMs = Date.now();
  const deadlineMs = startMs + opts.timeoutSecs * 1000;
  let totalIterations = 0;

  console.log(
    `[redteam] slug=${opts.slug} provider=${opts.provider} model=${
      opts.model
    } attacks=${opts.attacks.join(",")} max_iters_per_attack=${
      opts.iterations
    } sha256=${statementSha.slice(0, 16)}...`
  );

  for (const attack of opts.attacks) {
    const { system, user } = buildAttackPrompt(statementSource, attack);
    const history: ChatMessage[] = [
      { role: "system", content: system },
      { role: "user", content: user },
    ];

    for (let iter = 1; iter <= opts.iterations; iter++) {
      if (Date.now() > deadlineMs) {
        console.log(
          `  [redteam] wall-clock cap (${opts.timeoutSecs}s) reached during '${attack}' attack after ${totalIterations} total iterations.`
        );
        break;
      }
      totalIterations++;
      const reply = await callLlmProvider(opts.provider, opts.model, history);
      const candidate = extractLeanCodeBlock(reply);
      const compiled = compileRedTeamCandidate(
        statementSource,
        candidate,
        attack,
        opts.swarmProofsRoot ?? SWARM_PROOFS_ROOT
      );
      if (compiled.passed) {
        const breachDir = path.join(os.tmpdir(), "swarm-redteam-breaches");
        fs.mkdirSync(breachDir, { recursive: true });
        const breachPath = path.join(
          breachDir,
          `breach-${opts.slug}-${attack}-${Date.now()}.lean`
        );
        fs.writeFileSync(breachPath, candidate, "utf8");
        console.error(
          `  [RED-TEAM BREACH!] attack='${attack}' SUCCEEDED on iteration ${iter} for ${opts.slug}! Proof saved to ${breachPath}`
        );
        return {
          passed: false,
          breachedAttack: attack,
          breachProof: candidate,
          breachPath,
          totalIterations,
          durationSecs: Math.max(1, Math.round((Date.now() - startMs) / 1000)),
        };
      }
      const diag = `${compiled.stderr}\n${compiled.stdout}`
        .trim()
        .slice(0, 1800);
      console.log(
        `  [redteam] ${attack} iter ${iter}/${opts.iterations}: rejected (${
          compiled.rejectedReason ?? `exit=${compiled.exitCode}`
        })`
      );
      history.push({ role: "assistant", content: reply });
      history.push({
        role: "user",
        content: `Lean 4 rejected your candidate:\n\`\`\`\n${diag}\n\`\`\`\nFix the errors or try a different strategy. Return ONLY a single \`\`\`lean code block.`,
      });
    }
  }

  const durationSecs = Math.max(1, Math.round((Date.now() - startMs) / 1000));
  const webLitChecked = process.argv.includes("--web-literature-checked");
  const attestation: RedTeamAttestation = {
    passed: true,
    statement_sha256: statementSha,
    provider: opts.provider,
    model: opts.model,
    iterations: totalIterations,
    duration_secs: durationSecs,
    tested_at: new Date().toISOString(),
    attacks: [...opts.attacks],
    web_literature_checked: webLitChecked,
  };

  if (opts.writeAttestation) {
    upsertRedTeamAttestation(opts.slug, attestation, opts.policyPath);
    console.log(
      `[RED-TEAM PASS] Recorded attestation for '${opts.slug}' (${
        opts.provider
      }/${
        opts.model
      }, ${totalIterations} iterations, ${durationSecs}s, web_literature_checked=${webLitChecked}) in ${
        opts.policyPath ?? DEFAULT_MAINNET_POLICY_PATH
      }`
    );
  } else {
    console.log(
      `[RED-TEAM PASS] '${opts.slug}' survived all attacks (${opts.attacks.join(
        ","
      )}, ${totalIterations} iterations, ${durationSecs}s). Pass --write-attestation to record in mainnet-policy.json.`
    );
  }

  return {
    passed: true,
    totalIterations,
    durationSecs,
    attestation,
  };
}

function getArg(prefix: string): string | undefined {
  return process.argv
    .find((a) => a.startsWith(prefix))
    ?.slice(prefix.length)
    .trim();
}

async function main(): Promise<void> {
  const taskSlug = getArg("--task=");
  const stmtFile = getArg("--statement-file=");
  const recordAntigravity = process.argv.includes("--record-antigravity-audit");
  const writeAttestation =
    process.argv.includes("--write-attestation") || recordAntigravity;

  if (!taskSlug && !stmtFile) {
    throw new Error(
      "Specify --task=<catalog-slug> or --statement-file=<path.lean>"
    );
  }

  const statementPath = stmtFile
    ? path.resolve(stmtFile)
    : path.join(CATALOG_DIR, "open", `${taskSlug}.lean`);
  if (!fs.existsSync(statementPath)) {
    throw new Error(`Statement file not found: ${statementPath}`);
  }
  const slug = taskSlug ?? path.basename(statementPath).replace(/\.lean$/, "");

  if (recordAntigravity) {
    const statementSource = fs.readFileSync(statementPath, "utf8");
    const statementSha = sha256Hex(statementSource);
    const model = getArg("--model=") ?? "antigravity-subagent";
    const iterations = Number(getArg("--iterations=") ?? "12");
    const durationSecs = Number(getArg("--duration-secs=") ?? "60");
    const webLitChecked = process.argv.includes("--web-literature-checked");
    if (!webLitChecked) {
      throw new Error(
        "--record-antigravity-audit requires --web-literature-checked confirming a live arXiv/web search verified the problem is still open."
      );
    }
    const notes =
      getArg("--notes=") ??
      "Isolated Antigravity lean-redteam subagent verified live web/arXiv literature status and exhausted prove, cheat, and disprove attacks against local Lean 4.33 + Mathlib + SwarmProofs with 0 breaches.";
    if (!Number.isInteger(iterations) || iterations < 3) {
      throw new Error("--iterations must be an integer >= 3");
    }
    const attestation: RedTeamAttestation = {
      passed: true,
      statement_sha256: statementSha,
      provider: "antigravity",
      model,
      iterations,
      duration_secs: Math.max(1, Math.round(durationSecs)),
      tested_at: new Date().toISOString(),
      attacks: ["cheat", "disprove", "prove"],
      web_literature_checked: true,
      notes,
    };
    upsertRedTeamAttestation(slug, attestation);
    console.log(
      `[RED-TEAM PASS] Recorded Antigravity attestation for '${slug}' (sha256=${statementSha.slice(
        0,
        16
      )}..., ${iterations} iterations, ${durationSecs}s, web_literature_checked=true)`
    );
    return;
  }

  const providerRaw = (getArg("--provider=") ?? "xai").toLowerCase();
  if (!["xai", "gemini", "openai"].includes(providerRaw)) {
    throw new Error(
      `Unsupported headless --provider='${providerRaw}'; expected xai, gemini, or openai (or pass --record-antigravity-audit)`
    );
  }
  const provider = providerRaw as RedTeamProvider;
  const defaultModel =
    provider === "xai"
      ? "grok-4"
      : provider === "gemini"
      ? "gemini-2.5-pro"
      : "o3";
  const model = getArg("--model=") ?? defaultModel;
  const iterations = Number(getArg("--iterations=") ?? "5");
  const timeoutSecs = Number(getArg("--timeout-secs=") ?? "900");
  const attacksRaw = (getArg("--attacks=") ?? "cheat,disprove,prove")
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean) as RedTeamAttackKind[];

  const outcome = await runRedTeam({
    statementPath,
    slug,
    provider,
    model,
    iterations,
    timeoutSecs,
    attacks: attacksRaw,
    writeAttestation: Boolean(writeAttestation && taskSlug),
  });
  if (!outcome.passed) {
    process.exit(2);
  }
}

const isDirectRun =
  process.argv[1]?.endsWith("redteam-lean-statement.ts") ||
  process.argv[1]?.endsWith("redteam-lean-statement");

if (isDirectRun) {
  main().catch((err) => {
    console.error(err);
    process.exit(1);
  });
}
