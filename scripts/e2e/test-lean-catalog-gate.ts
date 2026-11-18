/**
 * Unit + live regression suite for the Lean catalog triviality & policy-4 ladder gate.
 *
 * Modes:
 *   npx tsx scripts/e2e/test-lean-catalog-gate.ts --unit-only   # Deterministic offline tests
 *   npx tsx scripts/e2e/test-lean-catalog-gate.ts --live        # Unit tests + live runner gate
 */
import * as assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import { spawnSync } from "node:child_process";
import {
  assertRegistryComposableStatement,
  assertSeedingAllowedForEntry,
  buildTrivialityProbes,
  CATALOG_DIR,
  CHAIN_TASK_CONFIG,
  CatalogEntry,
  inlineSwarmProofsImportsForCompat,
  loadCatalog,
  loadProofRegistry,
  negateStatementProp,
  parseLeanPolicyArg,
  ProofRegistryManifest,
  REPO_ROOT,
  verifyStatementElaborates,
} from "./seed-lean-catalog";

function mockJsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

async function runUnitTests(): Promise<void> {
  console.log("=== Running Lean catalog gate offline unit tests ===");

  // 1. Default policy-4 & structured ladder metadata
  assert.equal(parseLeanPolicyArg(["node", "seed-lean-catalog.ts"]), 4);
  assert.equal(
    parseLeanPolicyArg(["node", "seed-lean-catalog.ts", "--lean-policy=3"]),
    3
  );
  assert.throws(() =>
    parseLeanPolicyArg(["node", "seed-lean-catalog.ts", "--lean-policy=9"])
  );

  const openEntries = loadCatalog(false);
  const chainEntries = openEntries.filter((e) => e.name.startsWith("chain-"));
  assert.equal(
    chainEntries.length,
    18,
    "expected all 18 open chain-*.lean entries"
  );

  for (const entry of chainEntries) {
    assert.equal(
      entry.declaredLeanPolicy,
      4,
      `${entry.name} must declare -- @lean-policy: 4`
    );
    assert.equal(
      entry.dependsOn,
      CHAIN_TASK_CONFIG[entry.name]?.dependsOn,
      `${entry.name} @depends-on must match CHAIN_TASK_CONFIG`
    );
    assertRegistryComposableStatement(entry);

    const inlined = inlineSwarmProofsImportsForCompat(entry.statement);
    assert.ok(
      !/\bimport\s+SwarmProofs\b/.test(inlined),
      `${entry.name} inline compat must expand SwarmProofs imports`
    );
    assert.ok(
      !inlined.includes("#guard"),
      `${entry.name} inline compat must strip #guard commands`
    );
  }
  console.log(
    "  [PASS] 18 chain-*.lean headers, composability & inline compat"
  );

  // 2. Negative composability checks
  for (const badToken of [
    "admit",
    "prefix ",
    "scoped ",
    "local ",
    "namespace Foo",
    "section Bar",
  ]) {
    assert.throws(
      () =>
        assertRegistryComposableStatement({
          name: "chain-bad-token",
          kind: "open",
          program: "millennium-chains",
          statement: `-- SPDX-License-Identifier: Apache-2.0\n-- ${badToken}\ndef statementProp : Prop := True`,
        }),
      /COMPOSABILITY GATE FAILED/
    );
  }
  assert.throws(
    () =>
      assertRegistryComposableStatement({
        name: "chain-bad-import",
        kind: "open",
        program: "millennium-chains",
        statement: `import SwarmProofs.Evil.Backdoor\n/-- SPDX-License-Identifier: Apache-2.0 -/\ndef statementProp : Prop := True`,
      }),
    /noncanonical SwarmProofs module/
  );
  assert.throws(
    () =>
      assertRegistryComposableStatement({
        name: "chain-missing-spdx-doc",
        kind: "open",
        program: "millennium-chains",
        statement: `import SwarmProofs.Chains.BSD.Defs\ndef statementProp : Prop := True`,
      }),
    /SPDX-License-Identifier/
  );
  console.log(
    "  [PASS] Negative composability checks reject forbidden tokens & bad imports"
  );

  // 3. negateStatementProp & separate probe construction
  const negated = negateStatementProp(
    "import Mathlib.Data.Nat.Basic\ndef statementProp : Prop := 1 = 2"
  );
  assert.ok(negated.includes("def statementPropOrig : Prop := 1 = 2"));
  assert.ok(
    negated.includes("def statementProp : Prop := ¬ statementPropOrig")
  );
  assert.throws(() => negateStatementProp("theorem foo : True := trivial"));

  const bsd2 = chainEntries.find(
    (e) => e.name === "chain-bsd-2-fermat-noncongruent-1"
  )!;
  const probes = buildTrivialityProbes(bsd2, {
    effectiveStatement: inlineSwarmProofsImportsForCompat(bsd2.statement),
    promotedDependencyModuleName: "SwarmProofs.Generated.H1111",
    parentImplicationStatement:
      "def parentStatementProp : Prop := True\ndef childStatementProp : Prop := True\ndef statementProp : Prop := parentStatementProp → childStatementProp\n",
  });
  const labels = new Set(probes.map((p) => p.label));
  for (const requiredLabel of [
    "positive-exact-search",
    "positive-apply-assumption",
    "positive-hint",
    "positive-aesop-raised-budget",
    "negative-exact-search",
    "negative-apply-assumption",
    "negative-hint",
    "negative-aesop-raised-budget",
    "dependency-consequence-tactic-aesop",
    "dependency-consequence-exact-search",
  ]) {
    assert.ok(labels.has(requiredLabel), `missing probe ${requiredLabel}`);
  }
  for (const p of probes) {
    assert.equal(p.maxBuildSecs, 120);
    assert.ok(p.proof.includes("SwarmProofs.Generated.H1111"));
    // Ensure apply? and hint are NEVER combined before exact? in `first |`
    if (p.label.includes("exact-search")) {
      assert.ok(!p.proof.includes("apply?"));
      assert.ok(!p.proof.includes("\n  hint"));
    }
  }
  console.log(
    "  [PASS] Separate positive/negative/dependency probes & negateStatementProp"
  );

  // 4. Solvable entry NO LONGER bypasses triviality probes!
  {
    const solvableEntry: CatalogEntry = {
      name: "solvable-trivial-prime",
      kind: "solvable",
      program: "lemma-ladder",
      statement:
        "import Mathlib.Data.Nat.Prime.Basic\ndef statementProp : Prop := Nat.Prime 101",
      companionProof: "theorem proof : statementProp := by decide",
    };

    const seenPayloads: Array<Record<string, unknown>> = [];
    const mockTrivialSolvableFetch: typeof fetch = async (input, init) => {
      const url = String(input);
      if (url.endsWith("/health")) {
        return mockJsonResponse(200, { status: "ok", composable: false });
      }
      const payload = JSON.parse(String(init?.body ?? "{}")) as Record<
        string,
        unknown
      >;
      seenPayloads.push(payload);
      const proof = String(payload.proof ?? "");
      if (proof.includes("True.intro")) {
        return mockJsonResponse(200, {
          verdict: "fail",
          detail: "type mismatch",
        });
      }
      if (proof === solvableEntry.companionProof) {
        return mockJsonResponse(200, { verdict: "pass", axioms: [] });
      }
      if (proof.includes("exact?")) {
        return mockJsonResponse(200, {
          verdict: "pass",
          detail: "solved by exact?",
        });
      }
      return mockJsonResponse(200, { verdict: "fail", detail: "unsolved" });
    };

    await assert.rejects(
      () =>
        verifyStatementElaborates(solvableEntry, {
          fetchImpl: mockTrivialSolvableFetch,
          runnerUrl: "https://runner.mock",
          runnerToken: "mock-token",
          checkOnly: true,
          leanPolicy: 2,
          retryDelayBaseMs: 0,
          probeConcurrency: 4,
        }),
      /was solved trivially by probe 'positive-exact-search'/
    );
    assert.ok(
      seenPayloads.length > 2,
      "solvable entry must execute triviality probes after companion check"
    );

    // Non-trivial solvable entry whose companion passes and all probes fail -> passes!
    let checkCount = 0;
    const mockNonTrivialSolvableFetch: typeof fetch = async (input, init) => {
      const url = String(input);
      if (url.endsWith("/health")) {
        return mockJsonResponse(200, { status: "ok", composable: false });
      }
      checkCount++;
      const payload = JSON.parse(String(init?.body ?? "{}")) as Record<
        string,
        unknown
      >;
      if (payload.proof === solvableEntry.companionProof) {
        return mockJsonResponse(200, { verdict: "pass", axioms: [] });
      }
      return mockJsonResponse(200, { verdict: "fail", detail: "unsolved" });
    };
    await verifyStatementElaborates(solvableEntry, {
      fetchImpl: mockNonTrivialSolvableFetch,
      runnerUrl: "https://runner.mock",
      runnerToken: "mock-token",
      checkOnly: true,
      leanPolicy: 2,
      retryDelayBaseMs: 0,
      probeConcurrency: 4,
    });
    assert.equal(
      checkCount,
      15,
      "expected 1 stage-A1 + 1 companion + 13 triviality probes = 15 /check calls"
    );
  }
  console.log(
    "  [PASS] Solvable entries run companion proof AND all triviality probes"
  );

  // 5. Runner HTTP 500, unreachable port, and malformed probe detail fail closed (0 tasks seeded)
  {
    let campaignPostCount = 0;
    const mock500Fetch: typeof fetch = async (input) => {
      const url = String(input);
      if (url.includes("/campaigns")) {
        campaignPostCount++;
      }
      if (url.endsWith("/health")) {
        return mockJsonResponse(200, { status: "ok", composable: true });
      }
      return mockJsonResponse(500, { error: "runner internal failure" });
    };
    await assert.rejects(
      () =>
        verifyStatementElaborates(bsd2, {
          fetchImpl: mock500Fetch,
          runnerUrl: "https://runner.mock",
          runnerToken: "tok",
          checkOnly: true,
          retryDelayBaseMs: 0,
        }),
      /runner check HTTP 500/
    );
    assert.equal(campaignPostCount, 0);

    // Malformed probe output ("unknown tactic") must fail closed immediately
    const mockMalformedFetch: typeof fetch = async (input, init) => {
      const url = String(input);
      if (url.endsWith("/health")) {
        return mockJsonResponse(200, { status: "ok", composable: false });
      }
      const payload = JSON.parse(String(init?.body ?? "{}")) as Record<
        string,
        unknown
      >;
      if (String(payload.proof ?? "").includes("True.intro")) {
        return mockJsonResponse(200, { verdict: "fail", detail: "mismatch" });
      }
      return mockJsonResponse(200, {
        verdict: "fail",
        detail: "Submission.lean:4:2: error: unknown tactic 'bogus'",
      });
    };
    await assert.rejects(
      () =>
        verifyStatementElaborates(bsd2, {
          fetchImpl: mockMalformedFetch,
          runnerUrl: "https://runner.mock",
          runnerToken: "tok",
          checkOnly: true,
          retryDelayBaseMs: 0,
        }),
      /MALFORMED PROBE/
    );

    // Subprocess CLI invocation against closed local port 127.0.0.1:1 -> nonzero exit, nothing seeded
    const cliRun = spawnSync(
      "npx",
      [
        "tsx",
        path.join(REPO_ROOT, "scripts/e2e/seed-lean-catalog.ts"),
        "--check-only",
        "--tasks=chain-bsd-2-fermat-noncongruent-1",
      ],
      {
        cwd: REPO_ROOT,
        env: {
          ...process.env,
          LEAN_RUNNER_URL: "http://127.0.0.1:1",
          RUNNER_ID_TOKEN: "dummy",
        },
        encoding: "utf8",
      }
    );
    assert.notEqual(
      cliRun.status,
      0,
      "seed-lean-catalog.ts must exit non-zero when runner is unreachable"
    );

    const skipPrecheckRun = spawnSync(
      "npx",
      [
        "tsx",
        path.join(REPO_ROOT, "scripts/e2e/seed-lean-catalog.ts"),
        "--skip-precheck",
      ],
      {
        cwd: REPO_ROOT,
        encoding: "utf8",
      }
    );
    assert.notEqual(
      skipPrecheckRun.status,
      0,
      "seed-lean-catalog.ts must forbid --skip-precheck"
    );
    assert.match(skipPrecheckRun.stderr, /--skip-precheck is forbidden/);
  }
  console.log(
    "  [PASS] Runner HTTP 500, malformed probe, unreachable runner & --skip-precheck fail closed"
  );

  // 6. Rung N+1 seeding guard against registry.json + composable health guard
  {
    const emptyRegistry = loadProofRegistry();
    const bsd1 = chainEntries.find(
      (e) => e.name === "chain-bsd-1-weierstrass-bijection"
    )!;

    // Policy 4 refused when runner composable !== true
    assert.throws(
      () =>
        assertSeedingAllowedForEntry(bsd1, 4, emptyRegistry, {
          status: "ok",
          composable: false,
        }),
      /POLICY-4 SEED GUARD/
    );

    // Rung 1 allowed when composable === true
    assert.equal(
      assertSeedingAllowedForEntry(bsd1, 4, emptyRegistry, {
        status: "ok",
        composable: true,
      }),
      undefined
    );

    // Rung 2 refused when Rung 1 is absent from registry.json
    assert.throws(
      () =>
        assertSeedingAllowedForEntry(bsd2, 4, emptyRegistry, {
          status: "ok",
          composable: true,
        }),
      /LADDER DEPENDENCY GUARD/
    );

    // Rung 2 refused when Rung 1 is revoked in registry.json
    const modId = `sha256:${"a".repeat(64)}`;
    const modName = `SwarmProofs.Generated.H${"a".repeat(64)}`;
    const revokedRegistry: ProofRegistryManifest = {
      schema: emptyRegistry.schema,
      modules: [
        {
          module_id: modId,
          module_name: modName,
          originating_task_id: "task-b1",
          status: "revoked",
        },
      ],
      aliases: {
        "catalog:chain-bsd-1-weierstrass-bijection": modId,
      },
    };
    assert.throws(
      () =>
        assertSeedingAllowedForEntry(bsd2, 4, revokedRegistry, {
          status: "ok",
          composable: true,
        }),
      /inactive/
    );

    // Rung 2 allowed when Rung 1 is active in registry.json AND runner composable === true
    const activeRegistry: ProofRegistryManifest = {
      schema: emptyRegistry.schema,
      modules: [
        {
          module_id: modId,
          module_name: modName,
          originating_task_id: "task-b1",
          status: "active",
        },
      ],
      aliases: {
        "catalog:chain-bsd-1-weierstrass-bijection": modId,
      },
    };
    const resolved = assertSeedingAllowedForEntry(bsd2, 4, activeRegistry, {
      status: "ok",
      composable: true,
    });
    assert.deepEqual(resolved, {
      status: "active",
      moduleId: modId,
      moduleName: modName,
    });
  }
  console.log("  [PASS] Rung N+1 dependency guard & policy-4 composable guard");
  console.log("=== All offline unit tests PASSED ===\n");
}

async function runLiveRunnerTests(): Promise<void> {
  console.log("=== Running live shillbot-lean-runner gate assertions ===");

  // 1. Retired Chebyshev Rung 1 MUST be REJECTED (solved by exact? + primorial folding)
  const chebyshevPath = path.join(
    CATALOG_DIR,
    "retired",
    "chain-rh-analytic-1-chebyshev-theta-upper.lean"
  );
  const chebyshevEntry: CatalogEntry = {
    name: "chain-rh-analytic-1-chebyshev-theta-upper",
    kind: "open",
    program: "millennium-chains",
    statement: fs.readFileSync(chebyshevPath, "utf8").trimEnd(),
  };
  let chebyshevErr = "";
  await assert.rejects(async () => {
    try {
      await verifyStatementElaborates(chebyshevEntry, {
        checkOnly: true,
        leanPolicy: 3,
      });
    } catch (err) {
      chebyshevErr = err instanceof Error ? err.message : String(err);
      throw err;
    }
  }, /was solved trivially by probe 'positive-exact-search'/);
  console.log(
    `  [LIVE PASS] Chebyshev Rung 1 REJECTED as expected: ${chebyshevErr}`
  );

  // 2. Shannon Rung 2 & Mobius Rung 2 (and replacement Meissel Rung C1) MUST be ACCEPTED
  const allOpen = loadCatalog(false);
  const catalogByName = new Map(allOpen.map((e) => [e.name, e]));
  for (const targetName of [
    "chain-rh-analytic-1-mobius-floor-identity",
    "chain-rh-analytic-2-mobius-harmonic-bound",
    "chain-pnp-circuit-2-shannon-counting-bound",
  ]) {
    const entry = catalogByName.get(targetName);
    assert.ok(entry, `missing open catalog entry ${targetName}`);
    await verifyStatementElaborates(entry, {
      checkOnly: true,
      leanPolicy: 4,
      catalogByName,
    });
    console.log(`  [LIVE PASS] ${targetName} ACCEPTED`);
  }

  console.log("=== All live runner gate assertions PASSED ===");
}

async function main(): Promise<void> {
  const unitOnly = process.argv.includes("--unit-only");
  const live = process.argv.includes("--live");
  if (!unitOnly && !live) {
    throw new Error(
      "Specify --unit-only or --live when running test-lean-catalog-gate.ts"
    );
  }
  await runUnitTests();
  if (live) {
    await runLiveRunnerTests();
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
