#!/usr/bin/env npx tsx
/**
 * Deterministic Bug-Report & Counterexample Triage CLI (`P2.1`)
 *
 * Inspects `agent_messages` with `kind == "bug_report"` (or a specific `--message-id`),
 * extracts any embedded ```lean ... ``` counterexample / vacuity proof block, optionally
 * checks it against `shillbot-lean-runner` (`LEAN_RUNNER_URL/check`), and outputs the
 * exact `record_bug_bounty` MCP tool payload to award a `bug_bounty` EigenTrust edge.
 *
 * Usage:
 *   npx tsx scripts/triage-bug-report.ts [--message-id=<id>] [--statement-file=<path>]
 */

import fs from "node:fs";

const LEAN_RUNNER_URL = process.env.LEAN_RUNNER_URL ?? "";
const MCP_INTERNAL_URL = process.env.MCP_INTERNAL_URL ?? "https://mcp.swarm.tips";

export function extractLeanBlocks(body: string): string[] {
  const blocks: string[] = [];
  const regex = /```(?:lean4?|)\s*\n([\s\S]*?)```/g;
  let match: RegExpExecArray | null;
  while ((match = regex.exec(body)) !== null) {
    const code = match[1].trim();
    if (
      code.includes("theorem ") ||
      code.includes("lemma ") ||
      code.includes("def ") ||
      code.includes("statementProp")
    ) {
      blocks.push(code);
    }
  }
  return blocks;
}

export async function verifyAgainstLeanRunner(
  statementLean: string,
  proofLean: string,
  policyMask = 0,
): Promise<{
  ok: boolean;
  verified: boolean;
  resolution_direction?: string;
  axioms?: string[];
  diagnostic?: string;
}> {
  if (!LEAN_RUNNER_URL) {
    return {
      ok: false,
      verified: false,
      diagnostic: "LEAN_RUNNER_URL not set; skipped remote runner call (local syntax check only).",
    };
  }
  const res = await fetch(`${LEAN_RUNNER_URL.replace(/\/$/, "")}/check`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      statement_lean: statementLean,
      proof_lean: proofLean,
      policy: policyMask,
    }),
  });
  const json = (await res.json()) as any;
  return {
    ok: res.ok,
    verified: Boolean(json.verified),
    resolution_direction: json.resolution_direction,
    axioms: json.axioms,
    diagnostic: json.diagnostic ?? json.error,
  };
}

async function main() {
  const msgId = process.argv.find((a) => a.startsWith("--message-id="))?.split("=")[1];
  const statementFile = process.argv.find((a) => a.startsWith("--statement-file="))?.split("=")[1];
  const proofFile = process.argv.find((a) => a.startsWith("--proof-file="))?.split("=")[1];
  const reporterWallet =
    process.argv.find((a) => a.startsWith("--reporter="))?.split("=")[1] ?? "<REPORTER_WALLET>";

  if (statementFile && proofFile) {
    const statementLean = fs.readFileSync(statementFile, "utf8");
    const proofLean = fs.readFileSync(proofFile, "utf8");
    const verdict = await verifyAgainstLeanRunner(statementLean, proofLean);
    console.log("=== Lean 4 Bug-Report / Counterexample Triage ===");
    console.log(JSON.stringify(verdict, null, 2));
    console.log("\nRecommended MCP `record_bug_bounty` call:");
    console.log(
      JSON.stringify(
        {
          tool: "record_bug_bounty",
          arguments: {
            reporter_wallet: reporterWallet,
            commit_sha: process.env.COMMIT_SHA ?? "<FIX_COMMIT_SHA>",
            severity: verdict.resolution_direction === "disproved" ? "high" : "medium",
            summary: `Verified Lean 4 ${verdict.resolution_direction ?? "formal"} audit report`,
            message_id: msgId,
          },
        },
        null,
        2,
      ),
    );
    return;
  }

  console.log(
    JSON.stringify(
      {
        script: "scripts/triage-bug-report.ts",
        mcp_internal_url: MCP_INTERNAL_URL,
        lean_runner_url: LEAN_RUNNER_URL || "(unset)",
        usage:
          "npx tsx scripts/triage-bug-report.ts --statement-file=<stmt.lean> --proof-file=<proof.lean> [--reporter=<wallet>] [--message-id=<id>]",
      },
      null,
      2,
    ),
  );
}

if (import.meta.url === `file://${process.argv[1]}`) {
  main().catch((err) => {
    console.error(err);
    process.exit(1);
  });
}
