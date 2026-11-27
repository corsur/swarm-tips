import {
  closeSync,
  constants as fsConstants,
  lstatSync,
  mkdirSync,
  openSync,
  writeFileSync,
} from "node:fs";
import { join, resolve } from "node:path";
import type { LeanEnvironmentInfo, ProofLimits } from "../shillbot/api.js";
import { CliUsageError } from "./keyfile.js";

export const DEFAULT_LEAN_TOOLCHAINS: Readonly<Record<number, string>> = {
  1: "leanprover/lean4:v4.31.0",
  2: "leanprover/lean4:v4.30.0",
  3: "leanprover/lean4:v4.30.0",
  4: "leanprover/lean4:v4.33.0-rc2",
};

export const DEFAULT_MATHLIB_REV_V2_V3 = "c5ea00351c28e24afc9f0f84379aa41082b1188f";
export const DEFAULT_MATHLIB_REV_V4 = "51e6992efd06126df61a496bebf8f49482a4e129";
export const DEFAULT_ZETA23_REV_V4 = "3635e74826a4c1fcece7d1cd2b6fa75e43a00510";

const LEAN_TOOLCHAIN_RE = /^leanprover\/lean4:v[0-9]+\.[0-9]+\.[0-9]+(-rc[0-9]+)?$/;
const GIT_COMMIT_SHA_RE = /^[0-9a-f]{40}$/;
const TASK_ID_RE = /^[A-Za-z0-9:._-]{1,128}$/;
const MODE_RE = /^[a-z0-9_-]{1,32}$/;
const IMPORT_IDENT_RE = /^[A-Za-z0-9_.]+$/;

export interface LeanWorkspaceSpec {
  task_id: string;
  statement_lean: string;
  lean_policy: number;
  lean_toolchain: string;
  mode: string;
  mathlib_rev?: string;
  zeta23_rev?: string;
  swarm_proofs_rev?: string;
  import_allowlist: string[];
  allows_disproof: boolean;
  limits: ProofLimits;
}

export interface ScaffoldLeanWorkspaceResult {
  ok: true;
  task_id: string;
  dir: string;
  lean_policy: number;
  lean_toolchain: string;
  allows_disproof: boolean;
  limits: ProofLimits;
  files: string[];
}

function parsePositiveIntDirective(statementLean: string, key: string): number | undefined {
  const re = new RegExp(`^\\s*--\\s*swarm:${key}\\s*=\\s*(\\d+)\\s*$`, "m");
  const match = re.exec(statementLean);
  if (!match) return undefined;
  const parsed = Number(match[1]);
  return Number.isSafeInteger(parsed) && parsed > 0 ? parsed : undefined;
}

function hasBidirectionalDirective(statementLean: string): boolean {
  return /^\s*--\s*swarm:polarity\s*=\s*bidirectional\s*$/m.test(statementLean);
}

function asLeanEnv(value: unknown): Partial<LeanEnvironmentInfo> | undefined {
  return value && typeof value === "object"
    ? (value as Partial<LeanEnvironmentInfo>)
    : undefined;
}

function positiveInt(value: unknown): number | undefined {
  return typeof value === "number" && Number.isSafeInteger(value) && value > 0 ? value : undefined;
}

function validateOptionalPattern(
  value: unknown,
  pattern: RegExp,
  field: string,
  description: string,
): string | undefined {
  if (value === undefined || value === null) return undefined;
  if (typeof value !== "string" || !pattern.test(value)) {
    throw new CliUsageError(
      `Invalid ${field} in task metadata: expected ${description}.`,
    );
  }
  return value;
}

export function extractLeanWorkspaceSpec(
  task: Record<string, unknown>,
  taskId: string,
): LeanWorkspaceSpec {
  if (typeof taskId !== "string" || !TASK_ID_RE.test(taskId)) {
    throw new CliUsageError(
      `Invalid task_id "${String(taskId)}": expected 1-128 characters matching [A-Za-z0-9:._-].`,
    );
  }
  if (task.platform !== 10) {
    throw new CliUsageError(
      `Task "${taskId}" is not a LeanProof task (platform=${String(task.platform)}, expected 10).`,
    );
  }
  const statementLean =
    typeof task.statement_lean === "string" ? task.statement_lean : "";
  if (!statementLean.trim()) {
    throw new CliUsageError(
      `Task "${taskId}" has no statement_lean; cannot scaffold a Lean 4 workspace.`,
    );
  }

  const env = asLeanEnv(task.lean_environment);
  const rawPolicy = positiveInt(env?.lean_policy) ?? positiveInt(task.lean_policy) ?? 1;
  const leanToolchain =
    validateOptionalPattern(
      env?.lean_toolchain,
      LEAN_TOOLCHAIN_RE,
      "lean_toolchain",
      "official leanprover/lean4:v<major>.<minor>.<patch>[-rc<N>] toolchain",
    ) ??
    DEFAULT_LEAN_TOOLCHAINS[rawPolicy] ??
    DEFAULT_LEAN_TOOLCHAINS[1];

  const defaultMode =
    rawPolicy === 4 ? "composable" : rawPolicy === 2 || rawPolicy === 3 ? "mathlib" : "self-contained";
  const mode =
    validateOptionalPattern(
      env?.mode,
      MODE_RE,
      "mode",
      "1-32 characters matching [a-z0-9_-]",
    ) ?? defaultMode;

  const defaultImports =
    rawPolicy === 4
      ? ["Mathlib", "Zeta23", "SwarmProofs"]
      : rawPolicy === 2 || rawPolicy === 3
      ? ["Mathlib"]
      : [];
  let importAllowlist: string[];
  if (env?.import_allowlist !== undefined && env?.import_allowlist !== null) {
    if (!Array.isArray(env.import_allowlist)) {
      throw new CliUsageError("Invalid import_allowlist in task metadata: expected array of Lean module names.");
    }
    importAllowlist = env.import_allowlist.map((item) => {
      if (typeof item !== "string" || !IMPORT_IDENT_RE.test(item)) {
        throw new CliUsageError(
          `Invalid import_allowlist entry "${String(item)}": expected Lean module identifier matching [A-Za-z0-9_.]+.`,
        );
      }
      return item;
    });
  } else {
    importAllowlist = defaultImports;
  }

  const mathlibRev =
    validateOptionalPattern(
      env?.mathlib_rev,
      GIT_COMMIT_SHA_RE,
      "mathlib_rev",
      "40-character lowercase hex git commit SHA",
    ) ??
    (rawPolicy === 4
      ? DEFAULT_MATHLIB_REV_V4
      : rawPolicy === 2 || rawPolicy === 3
      ? DEFAULT_MATHLIB_REV_V2_V3
      : undefined);

  const zeta23Rev =
    validateOptionalPattern(
      env?.zeta23_rev,
      GIT_COMMIT_SHA_RE,
      "zeta23_rev",
      "40-character lowercase hex git commit SHA",
    ) ?? (rawPolicy === 4 ? DEFAULT_ZETA23_REV_V4 : undefined);

  const swarmProofsRev = validateOptionalPattern(
    env?.swarm_proofs_rev,
    GIT_COMMIT_SHA_RE,
    "swarm_proofs_rev",
    "40-character lowercase hex git commit SHA",
  );

  const allowsDisproof =
    typeof env?.allows_disproof === "boolean"
      ? env.allows_disproof
      : hasBidirectionalDirective(statementLean);

  const maxProofLines =
    positiveInt(env?.max_proof_lines) ?? parsePositiveIntDirective(statementLean, "max_proof_lines");
  const maxProofBytes =
    positiveInt(env?.max_proof_bytes) ?? parsePositiveIntDirective(statementLean, "max_proof_bytes");
  const explicitMaxLineWidth =
    positiveInt(env?.max_line_width) ?? parsePositiveIntDirective(statementLean, "max_line_width");
  const maxLineWidth =
    explicitMaxLineWidth ?? (maxProofLines !== undefined ? 120 : undefined);

  const limits: ProofLimits = {
    ...(maxProofLines !== undefined ? { max_proof_lines: maxProofLines } : {}),
    ...(maxProofBytes !== undefined ? { max_proof_bytes: maxProofBytes } : {}),
    ...(maxLineWidth !== undefined ? { max_line_width: maxLineWidth } : {}),
  };

  return {
    task_id: taskId,
    statement_lean: statementLean,
    lean_policy: rawPolicy,
    lean_toolchain: leanToolchain,
    mode,
    ...(mathlibRev ? { mathlib_rev: mathlibRev } : {}),
    ...(zeta23Rev ? { zeta23_rev: zeta23Rev } : {}),
    ...(swarmProofsRev ? { swarm_proofs_rev: swarmProofsRev } : {}),
    import_allowlist: importAllowlist,
    allows_disproof: allowsDisproof,
    limits,
  };
}

function buildLakefileToml(spec: LeanWorkspaceSpec): string {
  const lines: string[] = [
    'name = "SwarmTask"',
    'version = "0.1.0"',
    'defaultTargets = ["TargetStatement", "Solution"]',
    "",
    "[leanOptions]",
    "pp.unicode.fun = true",
    "relaxedAutoImplicit = false",
    "",
  ];

  if (spec.lean_policy === 4) {
    const mathlibRev = spec.mathlib_rev ?? DEFAULT_MATHLIB_REV_V4;
    const zeta23Rev = spec.zeta23_rev ?? DEFAULT_ZETA23_REV_V4;
    const swarmProofsRev = spec.swarm_proofs_rev ?? "main";
    lines.push(
      "[[require]]",
      'name = "mathlib"',
      'git = "https://github.com/leanprover-community/mathlib4.git"',
      `rev = ${JSON.stringify(mathlibRev)}`,
      "",
      "[[require]]",
      'name = "Zeta23"',
      'git = "https://github.com/anthropics/formal-math"',
      `rev = ${JSON.stringify(zeta23Rev)}`,
      'subDir = "zeta23"',
      "",
      "[[require]]",
      'name = "SwarmProofs"',
      'git = "https://github.com/corsur/swarm-tips"',
      `rev = ${JSON.stringify(swarmProofsRev)}`,
      'subDir = "proofs/SwarmProofs"',
      "",
    );
  } else if (spec.lean_policy === 2 || spec.lean_policy === 3 || spec.mathlib_rev) {
    const mathlibRev = spec.mathlib_rev ?? DEFAULT_MATHLIB_REV_V2_V3;
    lines.push(
      "[[require]]",
      'name = "mathlib"',
      'git = "https://github.com/leanprover-community/mathlib4.git"',
      `rev = ${JSON.stringify(mathlibRev)}`,
      "",
    );
  }

  lines.push(
    "[[lean_lib]]",
    'name = "TargetStatement"',
    "",
    "[[lean_lib]]",
    'name = "Solution"',
    "",
  );
  return lines.join("\n");
}

function formatLimitsSummary(limits: ProofLimits): string | undefined {
  const parts: string[] = [];
  if (limits.max_proof_lines !== undefined && limits.max_proof_lines !== null) {
    parts.push(`max_proof_lines=${limits.max_proof_lines}`);
  }
  if (limits.max_proof_bytes !== undefined && limits.max_proof_bytes !== null) {
    parts.push(`max_proof_bytes=${limits.max_proof_bytes}`);
  }
  if (limits.max_line_width !== undefined && limits.max_line_width !== null) {
    parts.push(`max_line_width=${limits.max_line_width}`);
  }
  return parts.length > 0 ? parts.join(", ") : undefined;
}

function buildTargetStatementLean(spec: LeanWorkspaceSpec): string {
  const body = spec.statement_lean.endsWith("\n")
    ? spec.statement_lean
    : `${spec.statement_lean}\n`;
  return (
    `${body}\n` +
    "namespace SwarmVerifyTarget\n" +
    "def statementProp : Prop := _root_.statementProp\n" +
    "end SwarmVerifyTarget\n"
  );
}

function buildSolutionLean(spec: LeanWorkspaceSpec): string {
  const importsLabel =
    spec.import_allowlist.length > 0
      ? spec.import_allowlist.join(", ")
      : "none (self-contained)";
  const polarityLine = spec.allows_disproof
    ? "-- Polarity: bidirectional — prove `theorem proof : statementProp` OR refute with `theorem disproof : ¬ statementProp`"
    : "-- Polarity: proof only — prove `theorem proof : statementProp`";
  const limitsSummary = formatLimitsSummary(spec.limits);

  const lines = [
    `-- Swarm Tips Lean 4 Task: ${spec.task_id}`,
    `-- Policy: v${spec.lean_policy} (${spec.mode}) | Toolchain: ${spec.lean_toolchain}`,
    `-- Allowed imports: ${importsLabel}`,
    polarityLine,
  ];
  if (limitsSummary) {
    lines.push(`-- Compression limits: ${limitsSummary}`);
  }
  lines.push(
    "",
    "import TargetStatement",
    "",
    "theorem proof : statementProp := by",
    "  sorry",
    "",
  );
  return lines.join("\n");
}

function buildReadmeMd(spec: LeanWorkspaceSpec): string {
  const importsLabel =
    spec.import_allowlist.length > 0
      ? spec.import_allowlist.map((i) => `\`${i}\``).join(", ")
      : "None (self-contained)";
  const polarityLabel = spec.allows_disproof
    ? "Bidirectional (`theorem proof : statementProp` or `theorem disproof : ¬ statementProp`)"
    : "Proof only (`theorem proof : statementProp`)";
  const limitsSummary = formatLimitsSummary(spec.limits);

  const lines = [
    `# Swarm Tips Lean 4 Workspace — \`${spec.task_id}\``,
    "",
    `- **Policy:** \`v${spec.lean_policy}\` (\`${spec.mode}\`)`,
    `- **Toolchain:** \`${spec.lean_toolchain}\``,
    `- **Allowed Imports:** ${importsLabel}`,
    `- **Polarity:** ${polarityLabel}`,
  ];
  if (limitsSummary) {
    lines.push(`- **Compression Limits:** \`${limitsSummary}\``);
  }
  lines.push(
    "",
    "## 1. Local Build (Optional)",
    "",
    "```bash",
    "lake exe cache get && lake build",
    "```",
    "",
    "## 2. Remote Compile Dry-Run (Zero SOL)",
    "",
    "```bash",
    `npx @swarm-tips/client tasks check ${spec.task_id} Solution.lean --compile`,
    "```",
    "",
    "## 3. Submit Proof",
    "",
    "```bash",
    `npx @swarm-tips/client tasks submit ${spec.task_id} --proof-file Solution.lean`,
    "```",
    "",
  );
  return lines.join("\n");
}

/**
 * Pure helper generating the complete Lean 4 workspace file map for a
 * `platform === 10` task without touching the filesystem.
 */
export function buildLeanWorkspaceFiles(
  task: Record<string, unknown>,
  taskId: string,
): Record<string, string> {
  const spec = extractLeanWorkspaceSpec(task, taskId);
  return {
    "lean-toolchain": `${spec.lean_toolchain}\n`,
    "lakefile.toml": buildLakefileToml(spec),
    "TargetStatement.lean": buildTargetStatementLean(spec),
    "Solution.lean": buildSolutionLean(spec),
    "README.md": buildReadmeMd(spec),
  };
}

function safeLstat(path: string) {
  try {
    return lstatSync(path);
  } catch (err) {
    if ((err as NodeJS.ErrnoException)?.code === "ENOENT") return undefined;
    throw err;
  }
}

function writeFileNoFollow(dest: string, content: string): void {
  const noFollow = fsConstants.O_NOFOLLOW ?? 0;
  const flags =
    fsConstants.O_WRONLY | fsConstants.O_CREAT | fsConstants.O_TRUNC | noFollow;
  let fd: number;
  try {
    fd = openSync(dest, flags, 0o644);
  } catch (err) {
    const code = (err as NodeJS.ErrnoException)?.code;
    if (code === "ELOOP" || code === "EMLINK") {
      throw new CliUsageError(
        `Refusing to write through symbolic link at ${dest}.`,
      );
    }
    throw err;
  }
  try {
    writeFileSync(fd, content, "utf8");
  } finally {
    closeSync(fd);
  }
}

/**
 * Materialize a Lean 4 prover workspace on disk. Refuses to overwrite any
 * existing non-empty target file unless `force === true`, and unconditionally
 * refuses symbolic links (live or dangling) at the target directory or any
 * destination file path even when `force === true`.
 */
export function scaffoldLeanWorkspace(
  task: Record<string, unknown>,
  taskId: string,
  targetDir: string,
  force = false,
): ScaffoldLeanWorkspaceResult {
  const spec = extractLeanWorkspaceSpec(task, taskId);
  const files = buildLeanWorkspaceFiles(task, taskId);
  const resolvedDir = resolve(targetDir);

  const dirStat = safeLstat(resolvedDir);
  if (dirStat) {
    if (dirStat.isSymbolicLink()) {
      throw new CliUsageError(
        `Refusing to scaffold into symbolic link directory: ${resolvedDir}.`,
      );
    }
    if (!dirStat.isDirectory()) {
      throw new CliUsageError(
        `Target path exists and is not a directory: ${resolvedDir}.`,
      );
    }
  }

  const existingNonEmpty: string[] = [];
  for (const relName of Object.keys(files)) {
    const dest = join(resolvedDir, relName);
    const stat = safeLstat(dest);
    if (stat) {
      if (stat.isSymbolicLink()) {
        throw new CliUsageError(
          `Refusing to write through symbolic link target file: ${dest}.`,
        );
      }
      if (stat.isDirectory() || stat.size > 0) {
        existingNonEmpty.push(relName);
      }
    }
  }
  if (existingNonEmpty.length > 0 && !force) {
    throw new CliUsageError(
      `Refusing to overwrite non-empty target file(s) in ${resolvedDir}: ${existingNonEmpty.join(", ")}. Pass --force to overwrite.`,
    );
  }

  mkdirSync(resolvedDir, { recursive: true });
  const written: string[] = [];
  for (const [relName, content] of Object.entries(files)) {
    const dest = join(resolvedDir, relName);
    const postStat = safeLstat(dest);
    if (postStat?.isSymbolicLink()) {
      throw new CliUsageError(
        `Refusing to write through symbolic link target file: ${dest}.`,
      );
    }
    writeFileNoFollow(dest, content);
    written.push(relName);
  }

  return {
    ok: true,
    task_id: taskId,
    dir: resolvedDir,
    lean_policy: spec.lean_policy,
    lean_toolchain: spec.lean_toolchain,
    allows_disproof: spec.allows_disproof,
    limits: spec.limits,
    files: written,
  };
}
