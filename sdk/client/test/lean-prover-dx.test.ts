import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { Keypair } from "@solana/web3.js";
import { describe, expect, it, vi } from "vitest";
import {
  createSwarmAgent,
  fromSolanaKeypair,
  type PreflightTaskResponse,
  type ProofsResponse,
  type TaskResponse,
} from "../src/index.js";
import {
  DEFAULT_MATHLIB_REV_V2_V3,
  DEFAULT_MATHLIB_REV_V4,
  DEFAULT_ZETA23_REV_V4,
  buildLeanWorkspaceFiles,
  scaffoldLeanWorkspace,
} from "../src/cli/lean-init.js";
import { parseCliArgs, runSwarmCli } from "../src/cli/swarm.js";

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

function makeJwt(wallet: string): string {
  const b64url = (obj: unknown) =>
    Buffer.from(JSON.stringify(obj), "utf8")
      .toString("base64")
      .replace(/\+/g, "-")
      .replace(/\//g, "_")
      .replace(/=+$/g, "");
  const exp = Math.floor(Date.now() / 1000) + 3600;
  return `${b64url({ alg: "HS256", typ: "JWT" })}.${b64url({ wallet, exp })}.sig`;
}

describe("Lean 4 Workspace Scaffolder (buildLeanWorkspaceFiles & scaffoldLeanWorkspace)", () => {
  it("generates standalone Policy 1 files without external [[require]] deps", () => {
    const task: TaskResponse = {
      task_id: "camp1:p1",
      state: "open",
      platform: 10,
      lean_policy: 1,
      statement_lean: "def statementProp : Prop := ∀ n : Nat, n + 0 = n\n",
    };
    const files = buildLeanWorkspaceFiles(task, "camp1:p1");

    expect(Object.keys(files).sort()).toEqual([
      "README.md",
      "Solution.lean",
      "TargetStatement.lean",
      "lakefile.toml",
      "lean-toolchain",
    ]);
    expect(files["lean-toolchain"]).toBe("leanprover/lean4:v4.31.0\n");
    expect(files["lakefile.toml"]).not.toContain("[[require]]");
    expect(files["lakefile.toml"]).toContain('name = "TargetStatement"');
    expect(files["lakefile.toml"]).toContain('name = "Solution"');
    expect(files["TargetStatement.lean"]).toContain(
      "def statementProp : Prop := ∀ n : Nat, n + 0 = n",
    );
    expect(files["TargetStatement.lean"]).toContain(
      "namespace SwarmVerifyTarget\ndef statementProp : Prop := _root_.statementProp\nend SwarmVerifyTarget\n",
    );
    expect(files["Solution.lean"]).toContain("camp1:p1");
    expect(files["Solution.lean"]).toContain("proof only");
    expect(files["Solution.lean"]).toContain("import TargetStatement");
    expect(files["Solution.lean"]).toContain("theorem proof : statementProp := by\n  sorry\n");
    expect(files["README.md"]).toContain("lake exe cache get && lake build");
    expect(files["README.md"]).toContain(
      "npx @swarm-tips/client tasks check camp1:p1 Solution.lean --compile",
    );
    expect(files["README.md"]).toContain(
      "npx @swarm-tips/client tasks submit camp1:p1 --proof-file Solution.lean",
    );
  });

  it("generates Policy 3 Mathlib workspace with compression limits & bidirectional directive fallback", () => {
    const statement = [
      "-- swarm:polarity=bidirectional",
      "-- swarm:max_proof_lines=75",
      "-- swarm:max_proof_bytes=4096",
      "import Mathlib",
      "def statementProp : Prop := ∀ n : ℕ, n + 0 = n",
    ].join("\n");
    const files = buildLeanWorkspaceFiles(
      {
        task_id: "camp3:golf",
        state: "open",
        platform: 10,
        lean_policy: 3,
        statement_lean: statement,
      },
      "camp3:golf",
    );

    expect(files["lean-toolchain"]).toBe("leanprover/lean4:v4.30.0\n");
    expect(files["lakefile.toml"]).toContain('name = "mathlib"');
    expect(files["lakefile.toml"]).toContain(DEFAULT_MATHLIB_REV_V2_V3);
    expect(files["lakefile.toml"]).not.toContain('name = "Zeta23"');
    expect(files["Solution.lean"]).toContain("bidirectional");
    expect(files["Solution.lean"]).toContain("theorem disproof : ¬ statementProp");
    expect(files["Solution.lean"]).toContain("max_proof_lines=75");
    expect(files["Solution.lean"]).toContain("max_proof_bytes=4096");
    expect(files["Solution.lean"]).toContain("max_line_width=120");
    expect(files["README.md"]).toContain("max_proof_lines=75");
  });

  it("generates Policy 4 composable workspace pinning mathlib, Zeta23, and SwarmProofs from lean_environment", () => {
    const task: TaskResponse = {
      task_id: "camp4:rh",
      state: "open",
      platform: 10,
      lean_policy: 4,
      statement_lean: "import SwarmProofs\ndef statementProp : Prop := True\n",
      lean_environment: {
        lean_policy: 4,
        lean_toolchain: "leanprover/lean4:v4.33.0-rc2",
        mode: "composable",
        mathlib_rev: DEFAULT_MATHLIB_REV_V4,
        zeta23_rev: DEFAULT_ZETA23_REV_V4,
        swarm_proofs_rev: "d33da5f7",
        environment_id: "env-v4-test",
        import_allowlist: ["Mathlib", "Zeta23", "SwarmProofs"],
        allows_disproof: true,
        max_proof_lines: 90,
        max_proof_bytes: 8192,
        max_line_width: 100,
      },
    };
    const files = buildLeanWorkspaceFiles(task, "camp4:rh");

    expect(files["lean-toolchain"]).toBe("leanprover/lean4:v4.33.0-rc2\n");
    expect(files["lakefile.toml"]).toContain(DEFAULT_MATHLIB_REV_V4);
    expect(files["lakefile.toml"]).toContain('name = "Zeta23"');
    expect(files["lakefile.toml"]).toContain(DEFAULT_ZETA23_REV_V4);
    expect(files["lakefile.toml"]).toContain('name = "SwarmProofs"');
    expect(files["lakefile.toml"]).toContain('"d33da5f7"');
    expect(files["Solution.lean"]).toContain("Mathlib, Zeta23, SwarmProofs");
    expect(files["Solution.lean"]).toContain("max_proof_lines=90, max_proof_bytes=8192, max_line_width=100");
    expect(files["Solution.lean"]).toContain("disproof");
  });

  it("rejects non-Lean tasks and tasks with empty statement_lean", () => {
    expect(() =>
      buildLeanWorkspaceFiles({ platform: 5, statement_lean: "def statementProp : Prop := True" }, "t:game"),
    ).toThrow(/expected 10/);
    expect(() =>
      buildLeanWorkspaceFiles({ platform: 10, statement_lean: "   " }, "t:empty"),
    ).toThrow(/statement_lean/);
  });

  it("refuses to overwrite non-empty target files unless --force is set", () => {
    const dir = mkdtempSync(resolve(tmpdir(), "swarm-lean-init-"));
    try {
      const task = {
        platform: 10,
        lean_policy: 1,
        statement_lean: "def statementProp : Prop := True\n",
      };
      const first = scaffoldLeanWorkspace(task, "c:t1", dir, false);
      expect(first.ok).toBe(true);
      expect(readFileSync(join(dir, "lean-toolchain"), "utf8")).toBe("leanprover/lean4:v4.31.0\n");

      // Edit Solution.lean so we prove refusal preserves user edits
      writeFileSync(join(dir, "Solution.lean"), "-- my work in progress\n", "utf8");
      expect(() => scaffoldLeanWorkspace(task, "c:t1", dir, false)).toThrow(/--force/);
      expect(readFileSync(join(dir, "Solution.lean"), "utf8")).toBe("-- my work in progress\n");

      // With force=true, overwrites cleanly
      const forced = scaffoldLeanWorkspace(task, "c:t1", dir, true);
      expect(forced.ok).toBe(true);
      expect(readFileSync(join(dir, "Solution.lean"), "utf8")).toContain("theorem proof");
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});

describe("CLI tasks init, tasks check --compile, tasks submit --proof-file, and agent.proofs.list", () => {
  it("parses --compile and --force as boolean flags", () => {
    const parsed = parseCliArgs(["tasks", "check", "c:t1", "Solution.lean", "--compile", "--force"]);
    expect(parsed.positionals).toEqual(["tasks", "check", "c:t1", "Solution.lean"]);
    expect(parsed.flags.get("--compile")).toBe(true);
    expect(parsed.flags.get("--force")).toBe(true);
  });

  it("runs `swarm tasks init` with zero auth / zero SOL and respects --force", async () => {
    const dir = mkdtempSync(resolve(tmpdir(), "swarm-cli-init-"));
    try {
      const seenAuth: Array<string | null> = [];
      const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = String(input);
        seenAuth.push(new Headers(init?.headers).get("Authorization"));
        expect(url).toBe("https://api.shillbot.org/tasks/camp%3Alean1");
        return jsonResponse({
          task_id: "camp:lean1",
          state: "open",
          platform: 10,
          lean_policy: 3,
          statement_lean: "import Mathlib\ndef statementProp : Prop := True\n",
        });
      });

      let out = "";
      const code = await runSwarmCli(
        ["tasks", "init", "camp:lean1", "--dir", dir, "--json"],
        {
          stdout: { write: (s) => ((out += s), true), isTTY: false },
          stderr: { write: () => true },
          env: {},
          fetch: fetchMock as unknown as typeof fetch,
        },
      );
      expect(code).toBe(0);
      expect(seenAuth).toEqual([null]);
      const parsed = JSON.parse(out);
      expect(parsed).toMatchObject({
        ok: true,
        task_id: "camp:lean1",
        lean_policy: 3,
        lean_toolchain: "leanprover/lean4:v4.30.0",
      });
      expect(readFileSync(join(dir, "lakefile.toml"), "utf8")).toContain(DEFAULT_MATHLIB_REV_V2_V3);

      // Re-run without --force -> exit code 2
      let errOut = "";
      const blockedCode = await runSwarmCli(
        ["tasks", "init", "camp:lean1", "--dir", dir, "--json"],
        {
          stdout: { write: () => true, isTTY: false },
          stderr: { write: (s) => ((errOut += s), true) },
          env: {},
          fetch: fetchMock as unknown as typeof fetch,
        },
      );
      expect(blockedCode).toBe(2);
      expect(errOut).toContain("--force");

      // Re-run with --force -> exit code 0
      let forceOut = "";
      const forceCode = await runSwarmCli(
        ["tasks", "init", "camp:lean1", "--dir", dir, "--force", "--json"],
        {
          stdout: { write: (s) => ((forceOut += s), true), isTTY: false },
          stderr: { write: () => true },
          env: {},
          fetch: fetchMock as unknown as typeof fetch,
        },
      );
      expect(forceCode).toBe(0);
      expect(JSON.parse(forceOut).ok).toBe(true);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("runs `swarm tasks check <id> <file> --compile` in both JSON and human modes with exit code 0/1", async () => {
    const dir = mkdtempSync(resolve(tmpdir(), "swarm-cli-check-"));
    try {
      const proofFile = join(dir, "Solution.lean");
      const proofText = "import TargetStatement\ntheorem proof : statementProp := by\n  exact trivial\n";
      writeFileSync(proofFile, proofText, "utf8");
      const jwt = makeJwt(Keypair.generate().publicKey.toBase58());

      const passPayload: PreflightTaskResponse = {
        ok: true,
        task_id: "camp:t1",
        lean_policy: 3,
        detail: "Compile dry-run passed",
        compiled: true,
        verdict: "Pass",
        axioms: ["propext", "Classical.choice", "Quot.sound"],
        metrics: {
          code_lines: 3,
          total_lines: 3,
          code_bytes: 72,
          raw_bytes: 73,
          max_line_width: 36,
        },
        limits: {
          max_proof_lines: 50,
          max_proof_bytes: 4096,
          max_line_width: 120,
        },
      };

      const failPayload: PreflightTaskResponse = {
        ok: false,
        task_id: "camp:t1",
        lean_policy: 3,
        detail: "Elaboration failed",
        compiled: true,
        verdict: "Fail",
        diagnostics: [
          {
            file: "Solution.lean",
            line: 3,
            column: 2,
            severity: "error",
            message: "unknown identifier 'bad_tactic'",
          },
        ],
        metrics: {
          code_lines: 3,
          total_lines: 3,
          code_bytes: 72,
          raw_bytes: 73,
          max_line_width: 36,
        },
        limits: {
          max_proof_lines: 50,
        },
      };

      let callNum = 0;
      const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        callNum += 1;
        expect(String(input)).toBe("https://api.shillbot.org/tasks/camp%3At1/preflight?network=devnet");
        expect(init?.method).toBe("POST");
        const body = JSON.parse(String(init?.body));
        expect(body).toEqual({ proof_lean: proofText, compile: true });
        expect(new Headers(init?.headers).get("Authorization")).toBe(`Bearer ${jwt}`);
        return jsonResponse(callNum === 1 ? passPayload : failPayload);
      });

      // 1. Passing compile check (--json) -> exit 0
      let passJson = "";
      const passCode = await runSwarmCli(
        ["tasks", "check", "camp:t1", proofFile, "--compile", "--network", "devnet", "--json"],
        {
          stdout: { write: (s) => ((passJson += s), true), isTTY: false },
          stderr: { write: () => true },
          env: { SWARM_AUTH_TOKEN: jwt },
          fetch: fetchMock as unknown as typeof fetch,
        },
      );
      expect(passCode).toBe(0);
      expect(JSON.parse(passJson)).toEqual(passPayload);

      // 2. Failing compile check (human TTY mode) -> exit 1 with formatted diagnostics, metrics, limits
      let failHuman = "";
      const failCode = await runSwarmCli(
        ["tasks", "check", "camp:t1", proofFile, "--compile", "--network", "devnet"],
        {
          stdout: { write: (s) => ((failHuman += s), true), isTTY: true },
          stderr: { write: () => true },
          env: { SWARM_AUTH_TOKEN: jwt },
          fetch: fetchMock as unknown as typeof fetch,
        },
      );
      expect(failCode).toBe(1);
      expect(failHuman).toContain("FAIL (compile) [Fail]");
      expect(failHuman).toContain("Solution.lean:3:2: error: unknown identifier 'bad_tactic'");
      expect(failHuman).toContain("3 code lines");
      expect(failHuman).toContain("max_proof_lines=50");
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("supports `tasks submit --proof-file --harness --model` and `agent.proofs.list`", async () => {
    const dir = mkdtempSync(resolve(tmpdir(), "swarm-cli-submit-"));
    try {
      const proofFile = join(dir, "Solution.lean");
      const proofText = "theorem proof : statementProp := by exact trivial\n";
      writeFileSync(proofFile, proofText, "utf8");
      const kp = Keypair.generate();
      const jwt = makeJwt(kp.publicKey.toBase58());

      const seenBodies: unknown[] = [];
      const fetchSubmit = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        expect(String(input)).toBe("https://api.shillbot.org/tasks/camp%3Alean2/submit");
        seenBodies.push(JSON.parse(String(init?.body)));
        return jsonResponse({
          task_id: "camp:lean2",
          transaction: "AQID",
          task_pda: Keypair.generate().publicKey.toBase58(),
        });
      });

      let out = "";
      const code = await runSwarmCli(
        [
          "tasks",
          "submit",
          "camp:lean2",
          "--proof-file",
          proofFile,
          "--harness",
          "claude-code",
          "--model",
          "claude-opus-4-6",
          "--no-exec",
          "--json",
        ],
        {
          stdout: { write: (s) => ((out += s), true), isTTY: false },
          stderr: { write: () => true },
          env: { SWARM_AUTH_TOKEN: jwt },
          fetch: fetchSubmit as unknown as typeof fetch,
        },
      );
      expect(code).toBe(0);
      expect(seenBodies).toEqual([
        {
          content_id: "",
          proof_lean: proofText,
          agent_harness: "claude-code",
          model_family: "claude-opus-4-6",
        },
      ]);
      expect(JSON.parse(out).task_id).toBe("camp:lean2");

      // Test SDK agent.proofs.list & agent.tasks.preflight (camelCase args)
      const proofsPayload: ProofsResponse = {
        network: "devnet",
        total: 1,
        proofs: [
          {
            task_id: "camp:lean2",
            lean_policy: 4,
            solver_wallet: kp.publicKey.toBase58(),
            agent_harness: "claude-code",
            model_family: "claude-opus-4-6",
            code_lines: 18,
            code_bytes: 512,
            proof_url: "https://swarm.tips/lean-proofs/abc.lean",
          },
        ],
        leaderboard: {
          by_wallet: [
            {
              key: kp.publicKey.toBase58(),
              proofs_solved: 1,
              total_lamports: 50_000_000,
              median_code_lines: 18,
            },
          ],
        },
      };

      const seenRequests: Array<{ url: string; method?: string; auth: string | null; body?: unknown }> = [];
      const sdkFetch = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = String(input);
        seenRequests.push({
          url,
          method: init?.method,
          auth: new Headers(init?.headers).get("Authorization"),
          body: init?.body ? JSON.parse(String(init.body)) : undefined,
        });
        if (url.includes("/proofs")) return jsonResponse(proofsPayload);
        if (url.includes("/preflight")) {
          return jsonResponse({
            ok: true,
            task_id: "camp:lean2",
            lean_policy: 4,
            detail: "ok",
            compiled: true,
          });
        }
        throw new Error(`Unexpected URL: ${url}`);
      });

      const agent = createSwarmAgent({
        signer: fromSolanaKeypair(kp),
        token: jwt,
        network: "mainnet",
        fetch: sdkFetch as unknown as typeof fetch,
      });

      const listed = await agent.proofs.list({ network: "devnet", limit: 25, policy: 4 });
      expect(listed).toEqual(proofsPayload);
      expect(seenRequests[0]).toEqual({
        url: "https://api.shillbot.org/proofs?network=devnet&limit=25&policy=4",
        method: "GET",
        auth: null,
        body: undefined,
      });

      const pre = await agent.tasks.preflight("camp:lean2", {
        proofLean: proofText,
        compile: true,
        network: "devnet",
      });
      expect(pre.ok).toBe(true);
      expect(seenRequests[1]).toEqual({
        url: "https://api.shillbot.org/tasks/camp%3Alean2/preflight?network=devnet",
        method: "POST",
        auth: `Bearer ${jwt}`,
        body: { proof_lean: proofText, compile: true },
      });

      // Also verify `swarm proofs list --json` works unauthenticated
      let cliProofsOut = "";
      const proofsCode = await runSwarmCli(
        ["proofs", "list", "--limit", "25", "--policy", "4", "--network", "devnet", "--json"],
        {
          stdout: { write: (s) => ((cliProofsOut += s), true), isTTY: false },
          stderr: { write: () => true },
          env: {},
          fetch: sdkFetch as unknown as typeof fetch,
        },
      );
      expect(proofsCode).toBe(0);
      expect(JSON.parse(cliProofsOut)).toEqual(proofsPayload);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
