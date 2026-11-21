/**
 * Unit + full-Mathlib positive-control + live runner regression suite for the Lean catalog gate.
 *
 * Modes:
 *   npx tsx scripts/e2e/test-lean-catalog-gate.ts --unit-only          # Deterministic offline tests
 *   npx tsx scripts/e2e/test-lean-catalog-gate.ts --mathlib-controls   # Offline + live lake env lean per-probe positive controls
 *   npx tsx scripts/e2e/test-lean-catalog-gate.ts --live               # Full suite: unit + Mathlib controls + live runner gate
 *   npx tsx scripts/e2e/test-lean-catalog-gate.ts --live --runner-only # Unit + live runner stage (used after mathlib-search-gate job)
 */
import * as assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import { spawnSync } from "node:child_process";
import * as catalogModule from "./seed-lean-catalog";
import {
  ALLOWED_SWARM_LIBRARY_IMPORTS,
  assertRegistryComposableStatement,
  assertSeedingAllowedForEntry,
  assertValidProbeResponse,
  BRIDGE_LEMMAS_BY_DEFS_MODULE,
  buildMathlibProbeSpecsForEntry,
  buildTrivialityProbes,
  CATALOG_DIR,
  CHAIN_TASK_CONFIG,
  CatalogEntry,
  classifyMathlibProbeSlice,
  hasExistentialUniversalStructure,
  inlineSwarmProofsImportsForCompat,
  loadCatalog,
  loadProofRegistry,
  negateStatementProp,
  parseLeanPolicyArg,
  parseNetworkArg,
  ProofRegistryManifest,
  UNFOLD_DEFS_BY_MODULE,
  WRAPPER_DEFS_BY_MODULE,
  REPO_ROOT,
  resolveBridgeLemmasForStatement,
  runFullMathlibSearchProbes,
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

  // 0. S1 + N1: Verify PROBE_MATHLIB_UMBRELLAS and swarm_fold_primorial are gone and ALLOWED_SWARM_LIBRARY_IMPORTS has ONLY the 4 Defs.V1 modules
  assert.equal(
    "PROBE_MATHLIB_UMBRELLAS" in catalogModule,
    false,
    "PROBE_MATHLIB_UMBRELLAS must be completely removed (S1)"
  );
  const seedSrc = fs.readFileSync(
    path.join(REPO_ROOT, "scripts/e2e/seed-lean-catalog.ts"),
    "utf8"
  );
  assert.equal(
    seedSrc.includes("swarm_fold_primorial"),
    false,
    "swarm_fold_primorial must be completely removed (S1)"
  );
  assert.deepEqual([...ALLOWED_SWARM_LIBRARY_IMPORTS].sort(), [
    "SwarmProofs.Chains.BSD.Defs.V1",
    "SwarmProofs.Chains.Circuits.Defs.V1",
    "SwarmProofs.Chains.RHAnalytic.Defs.V1",
    "SwarmProofs.Chains.RHLagarias.Defs.V1",
  ]);
  assert.deepEqual(
    BRIDGE_LEMMAS_BY_DEFS_MODULE["SwarmProofs.Chains.RHAnalytic.Defs.V1"],
    [
      "bridge_primeFactorCount",
      "bridge_mobiusInt",
      "bridge_mertensSum",
      "bridge_primorial",
    ]
  );
  assert.deepEqual(
    BRIDGE_LEMMAS_BY_DEFS_MODULE["SwarmProofs.Chains.RHLagarias.Defs.V1"],
    [
      "bridge_sigmaNat",
      "bridge_sigmaRat",
      "bridge_sigmaDivisors",
      "bridge_isSquarefreePos",
      "bridge_harmonicRat",
      "bridge_harmonicNum",
    ]
  );
  assert.deepEqual(
    UNFOLD_DEFS_BY_MODULE["SwarmProofs.Chains.RHLagarias.Defs.V1"],
    [
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
    ]
  );
  assert.deepEqual(
    UNFOLD_DEFS_BY_MODULE["SwarmProofs.Chains.RHAnalytic.Defs.V1"],
    ["mertensSum", "mobiusInt", "primeFactorCount", "squarePrimeFactorCount"]
  );
  assert.deepEqual(UNFOLD_DEFS_BY_MODULE["SwarmProofs.Chains.BSD.Defs.V1"], [
    "isCongruentNumber",
    "isCongruentTriangle",
  ]);
  assert.deepEqual(
    UNFOLD_DEFS_BY_MODULE["SwarmProofs.Chains.Circuits.Defs.V1"],
    ["evalCircuitFin", "evalCircuit", "isDeMorganOp"]
  );
  assert.ok(
    WRAPPER_DEFS_BY_MODULE["SwarmProofs.Chains.RHLagarias.Defs.V1"].includes(
      "lagariasBound"
    ) &&
      !WRAPPER_DEFS_BY_MODULE["SwarmProofs.Chains.RHLagarias.Defs.V1"].includes(
        "sigmaDivisors"
      ),
    "wrapper defs must unfold outer definitions without unfolding bridged defs prematurely"
  );

  // 1. Default policy-4, --network parsing (W7) & structured ladder metadata + exact file bytes parity (B1)
  assert.equal(parseNetworkArg(["node", "seed-lean-catalog.ts"], {}), "devnet");
  assert.equal(
    parseNetworkArg(
      ["node", "seed-lean-catalog.ts", "--network", "devnet"],
      {}
    ),
    "devnet"
  );
  assert.equal(
    parseNetworkArg(["node", "seed-lean-catalog.ts", "--network=devnet"], {}),
    "devnet"
  );
  assert.equal(
    parseNetworkArg(
      ["node", "seed-lean-catalog.ts", "--network", "mainnet"],
      {}
    ),
    "mainnet"
  );
  assert.equal(
    parseNetworkArg(["node", "seed-lean-catalog.ts", "--network=mainnet"], {}),
    "mainnet"
  );
  assert.equal(
    parseNetworkArg(["node", "seed-lean-catalog.ts", "--mainnet"], {}),
    "mainnet"
  );
  assert.throws(() =>
    parseNetworkArg(["node", "seed-lean-catalog.ts", "--network=testnet"], {})
  );
  assert.throws(() =>
    parseNetworkArg(
      ["node", "seed-lean-catalog.ts", "--mainnet", "--network=devnet"],
      {}
    )
  );
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
    const rawDiskBytes = fs.readFileSync(
      path.join(CATALOG_DIR, "open", `${entry.name}.lean`)
    );
    assert.deepEqual(
      Buffer.from(entry.statement, "utf8"),
      rawDiskBytes,
      `${entry.name} loaded statement must match disk bytes byte-for-byte (B1)`
    );
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
    const cta = CHAIN_TASK_CONFIG[entry.name]?.cta ?? "";
    assert.ok(
      cta.includes("-- SPDX-License-Identifier: Apache-2.0") &&
        cta.includes("@[...]") &&
        cta.includes("instance") &&
        cta.includes("open"),
      `N4: ${entry.name} CTA must document SPDX header and forbidden attribute/instance/open tokens`
    );
  }
  console.log(
    "  [PASS] 18 chain-*.lean headers, byte-for-byte parity, composability, CTA notes & inline compat"
  );

  // 2. Negative composability checks (including N1 umbrella rejection, S2 instance/class rejection & SF3 string/char literal comment spoofing)
  for (const badToken of [
    "admit",
    "prefix ",
    "scoped ",
    "local ",
    "namespace Foo",
    "section Bar",
    "instance ",
    "instance(",
    "instance[",
    "instance{",
    "instance:\n  Inhabited Nat := ⟨0⟩",
    "@[simp] theorem helper : True := trivial",
    "inductive Sneaky where | mk\n  deriving DecidableEq",
    "meta def sneakyMeta : Nat := 0",
    "opaque sneakyOpaque : Nat := 0",
    "class ",
    // SF3 mandatory negative cases: string & char literals must NEVER spoof line/block comments!
    'theorem spoof1 : True := by\n  have h : "--" = "--" := rfl; native_decide',
    'theorem spoof2 : True := by\n  have a : String := "/-"; native_decide; have b : String := "-/"',
    'theorem spoof3 : True := by\n  have c : String := "\\"-" ++ "-"; sorry',
    'theorem spoof3_exact : True := by\n  have c : String := "\\"--"; sorry',
    "theorem spoof4 : True := by\n  have c1 : Char := '/'; have c2 : Char := '-'; sorry",
    "theorem spoof5 : True := by\n  let f' : Char → Char := id; let _ := f' '-'; sorry",
    'theorem spoof6 : True := by\n  have s : String := s!"{sorry}"; exact trivial',
    'theorem spoof7 : True := by\n  have r : String := r#"\\"#; sorry; have q : String := "#"',
    'theorem spoof8 : True := by\n  have u : String := "unclosed',
  ]) {
    assert.throws(
      () =>
        assertRegistryComposableStatement({
          name: "chain-bad-token",
          kind: "open",
          program: "millennium-chains",
          statement: `-- SPDX-License-Identifier: Apache-2.0\n${badToken}\ndef statementProp : Prop := True`,
        }),
      /COMPOSABILITY GATE FAILED/,
      `Expected assertRegistryComposableStatement to reject:\n${badToken}`
    );
  }
  // SF3 positive cases: identifier primes (`local'`, `f' x`) and benign char/string literals pass cleanly:
  for (const okSnippet of [
    "def local' : Prop := True\ndef f' (x : Nat) : Nat := x\ntheorem okPrime : f' 0 = 0 := rfl",
    "def dashChar : Char := '-'\ndef quoteChar : Char := '\\''\ndef slashStr : String := \"\\\\\"",
  ]) {
    assertRegistryComposableStatement({
      name: "chain-ok-prime-literals",
      kind: "open",
      program: "millennium-chains",
      statement: `-- SPDX-License-Identifier: Apache-2.0\n${okSnippet}\ndef statementProp : Prop := True`,
    });
  }
  for (const badImport of [
    "SwarmProofs",
    "SwarmProofs.Registry",
    "SwarmProofs.Chains.BSD.Defs",
    "SwarmProofs.Evil.Backdoor",
  ]) {
    assert.throws(
      () =>
        assertRegistryComposableStatement({
          name: "chain-bad-import",
          kind: "open",
          program: "millennium-chains",
          statement: `import ${badImport}\n/-- SPDX-License-Identifier: Apache-2.0 -/\ndef statementProp : Prop := True`,
        }),
      /noncanonical SwarmProofs module/
    );
  }
  assert.throws(
    () =>
      assertRegistryComposableStatement({
        name: "chain-missing-spdx-doc",
        kind: "open",
        program: "millennium-chains",
        statement: `import SwarmProofs.Chains.BSD.Defs.V1\ndef statementProp : Prop := True`,
      }),
    /SPDX-License-Identifier/
  );
  console.log(
    "  [PASS] Negative composability checks reject instance/class, SF3 string/char comment spoofing, unversioned imports & umbrella SwarmProofs"
  );

  // 3. negateStatementProp, N4 quantifier filtering & Mathlib probe spec construction
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
  const c1 = chainEntries.find(
    (e) => e.name === "chain-rh-analytic-1-mertens-first-prime-log-bound"
  )!;
  assert.equal(hasExistentialUniversalStructure(bsd2.statement), false);
  assert.equal(hasExistentialUniversalStructure(c1.statement), true);

  const bsd2RunnerProbes = buildTrivialityProbes(bsd2, {
    effectiveStatement: inlineSwarmProofsImportsForCompat(bsd2.statement),
    promotedDependencyModuleName: "SwarmProofs.Generated.H1111",
    parentImplicationStatement:
      "def parentStatementProp : Prop := True\ndef childStatementProp : Prop := True\ndef statementProp : Prop := parentStatementProp → childStatementProp\n",
  });
  const bsd2Labels = new Set(bsd2RunnerProbes.map((p) => p.label));
  assert.equal(
    bsd2Labels.has("zero-constant-vacuity"),
    false,
    "N4: non-∃∀ statement must omit zero-constant-vacuity"
  );
  assert.equal(
    bsd2Labels.has("pointwise-ratio-shortcut"),
    false,
    "N4: non-∃∀ statement must omit pointwise-ratio-shortcut"
  );

  const c1RunnerProbes = buildTrivialityProbes(c1, {
    effectiveStatement: inlineSwarmProofsImportsForCompat(c1.statement),
  });
  const c1Labels = new Set(c1RunnerProbes.map((p) => p.label));
  assert.equal(c1Labels.has("zero-constant-vacuity"), true);
  assert.equal(c1Labels.has("pointwise-ratio-shortcut"), true);

  const mathlibSpecs = buildMathlibProbeSpecsForEntry(c1.statement, true);
  const mathlibLabels = new Set(mathlibSpecs.map((s) => s.label));
  for (const req of [
    "positive-exact",
    "positive-apply-assumption",
    "positive-solve-by-elim",
    "positive-hint",
    "positive-aesop",
    "negative-exact",
    "negative-apply-assumption",
    "negative-solve-by-elim",
    "negative-hint",
    "negative-aesop",
    "dependency-exact",
    "dependency-aesop",
  ]) {
    assert.ok(mathlibLabels.has(req), `missing Mathlib probe spec ${req}`);
  }
  for (const s of mathlibSpecs) {
    assert.equal(
      s.maxHeartbeats,
      0,
      `X1: Mathlib probe '${s.label}' must run with maxHeartbeats = 0`
    );
  }
  assert.deepEqual(resolveBridgeLemmasForStatement(c1.statement).bridgeLemmas, [
    "bridge_primeFactorCount",
    "bridge_mobiusInt",
    "bridge_mertensSum",
    "bridge_primorial",
  ]);
  console.log(
    "  [PASS] N4 existential-constant probe scoping & full-Mathlib probe spec generation"
  );

  // 4. B2: Strict exhaustion & fail-closed unit tests (timeout, deterministic timeout, max recursion, empty fail)
  {
    // Exact solved vs exhausted vs fail-closed
    assert.equal(
      classifyMathlibProbeSlice(
        "exact",
        "__p0",
        "Try this:\n  [apply] exact rfl\n'__p0' does not depend on any axioms\n"
      ),
      "SOLVED"
    );
    assert.equal(
      classifyMathlibProbeSlice(
        "exact",
        "__p0",
        "<stdin>:10:2: error: `exact?` could not close the goal.\n'__p0' depends on axioms: [sorryAx]\n"
      ),
      "EXHAUSTED"
    );
    for (const badSlice of [
      "<stdin>:10:2: error: (deterministic) timeout at `whnf`, maximum number of heartbeats (200000) has been reached\n'__p0' depends on axioms: [sorryAx]\n",
      "<stdin>:10:2: error: maximum recursion depth has been reached\n'__p0' depends on axioms: [sorryAx]\n",
      "<stdin>:10:2: error: process timed out after 300s\n'__p0' depends on axioms: [sorryAx]\n",
      "<stdin>:10:2: error: out of memory\n'__p0' depends on axioms: [sorryAx]\n",
      "<stdin>:10:2: error: unsolved goals\n'__p0' depends on axioms: [sorryAx]\n",
      "missing print axioms line entirely\n",
    ]) {
      assert.throws(
        () => classifyMathlibProbeSlice("exact", "__p0", badSlice),
        /MATHLIB SEARCH FAIL-CLOSED/,
        `expected fail-closed on slice: ${badSlice}`
      );
    }

    // Hint solved vs exhausted vs fail-closed
    assert.equal(
      classifyMathlibProbeSlice(
        "hint",
        "__ph",
        "Try these:\n  [apply] 🎉️ ring\n'__ph' depends on axioms: [propext]\n"
      ),
      "SOLVED"
    );
    assert.equal(
      classifyMathlibProbeSlice(
        "hint",
        "__ph",
        "Try these:\n  [apply] norm_num\n  Remaining subgoals:\n  ⊢ False\n'__ph' depends on axioms: [sorryAx]\n"
      ),
      "EXHAUSTED"
    );
    assert.throws(
      () =>
        classifyMathlibProbeSlice(
          "hint",
          "__ph",
          "<stdin>:5:2: error: maximum number of heartbeats (4000000) has been reached\n'__ph' depends on axioms: [sorryAx]\n"
        ),
      /MATHLIB SEARCH FAIL-CLOSED/
    );

    // Apply-assumption & solve-by-elim strict exhaustion markers (X1):
    assert.equal(
      classifyMathlibProbeSlice(
        "apply-assumption",
        "__pa",
        "<stdin>:8:2: error: swarm_apply_assumption: exhausted (0 partial candidates)\n'__pa' depends on axioms: [sorryAx]\n"
      ),
      "EXHAUSTED"
    );
    assert.equal(
      classifyMathlibProbeSlice(
        "apply-assumption",
        "__pa",
        "<stdin>:8:2: error: swarm_apply_assumption: exhausted (14 partial candidates, none closed by assumption)\n'__pa' depends on axioms: [sorryAx]\n"
      ),
      "EXHAUSTED"
    );
    for (const badApplySlice of [
      "<stdin>:8:2: error: `apply?` could not close the goal\n'__pa' depends on axioms: [sorryAx]\n",
      "<stdin>:8:2: error: failed to synthesize instance\n'__pa' depends on axioms: [sorryAx]\n",
    ]) {
      assert.throws(
        () =>
          classifyMathlibProbeSlice("apply-assumption", "__pa", badApplySlice),
        /MATHLIB SEARCH FAIL-CLOSED/
      );
    }

    assert.equal(
      classifyMathlibProbeSlice(
        "solve-by-elim",
        "__ps",
        "<stdin>:9:2: error: swarm_solve_by_elim: exhausted\n'__ps' depends on axioms: [sorryAx]\n"
      ),
      "EXHAUSTED"
    );
    for (const badSbeSlice of [
      "<stdin>:9:2: error: failed\n'__ps' depends on axioms: [sorryAx]\n",
      "<stdin>:9:2: error: failed to synthesize\n'__ps' depends on axioms: [sorryAx]\n",
      "<stdin>:9:2: error: unsolved goals\n'__ps' depends on axioms: [sorryAx]\n",
    ]) {
      assert.throws(
        () => classifyMathlibProbeSlice("solve-by-elim", "__ps", badSbeSlice),
        /MATHLIB SEARCH FAIL-CLOSED/
      );
    }

    // N1: aesop MUST only count exhaustive search or made no progress as EXHAUSTED;
    // hitting maxRuleApplicationDepth or maxRuleApplications MUST fail closed!
    assert.equal(
      classifyMathlibProbeSlice(
        "aesop",
        "__paesop",
        "<stdin>:12:2: error: tactic 'aesop' failed, failed to prove the goal after exhaustive search.\n'__paesop' depends on axioms: [sorryAx]\n"
      ),
      "EXHAUSTED"
    );
    assert.equal(
      classifyMathlibProbeSlice(
        "aesop",
        "__paesop",
        "<stdin>:12:2: error: Tactic `aesop` failed, made no progress\n'__paesop' depends on axioms: [sorryAx]\n"
      ),
      "EXHAUSTED"
    );
    for (const nonExhaustiveAesopSlice of [
      "<stdin>:12:2: error: aesop: failed to prove the goal. Some goals were not explored because the maximum rule application depth (30) was reached\n'__paesop' depends on axioms: [sorryAx]\n",
      "<stdin>:12:2: error: tactic 'aesop' failed, failed to prove the goal. Some goals were not explored because the maximum rule application depth (30) was reached. Set option 'maxRuleApplicationDepth' to increase the limit.\n'__paesop' depends on axioms: [sorryAx]\n",
      "<stdin>:12:2: error: tactic 'aesop' failed, failed to prove the goal. Some goals were not explored because the maximum number of rule applications (200) was reached.\n'__paesop' depends on axioms: [sorryAx]\n",
    ]) {
      assert.throws(
        () =>
          classifyMathlibProbeSlice(
            "aesop",
            "__paesop",
            nonExhaustiveAesopSlice
          ),
        /MATHLIB SEARCH FAIL-CLOSED/,
        `N1: expected aesop non-exhaustive slice to fail closed:\n${nonExhaustiveAesopSlice}`
      );
    }

    // Runner probe response fail-closed on empty detail, timeout, heartbeat, memory, killed
    for (const badDetail of [
      "",
      "   ",
      "build timed out after 120s",
      "(deterministic) timeout at whnf, maximum number of heartbeats reached",
      "container killed (OOM / memory limit exceeded)",
      "process terminated by signal SIGKILL",
      "opaque error with zero Lean failure markers",
    ]) {
      assert.throws(
        () =>
          assertValidProbeResponse(
            { verdict: "fail", detail: badDetail },
            "test-entry",
            "test-probe"
          ),
        /FAIL-CLOSED/
      );
    }
    assertValidProbeResponse(
      { verdict: "fail", detail: "Submission.lean:4:2: error: unsolved goals" },
      "test-entry",
      "test-probe"
    );
  }
  console.log(
    "  [PASS] B2 strict exhaustion parser & runner fail-closed checks reject timeouts, heartbeats, recursion & empty details"
  );

  // 5. Solvable entry runs companion proof AND runner tactic probes
  {
    const solvableEntry: CatalogEntry = {
      name: "solvable-trivial-prime",
      kind: "solvable",
      program: "lemma-ladder",
      statement:
        "import Mathlib.Data.Nat.Prime.Basic\ndef statementProp : Prop := Nat.Prime 101\n",
      companionProof: "theorem proof : statementProp := by decide",
    };

    const mockTrivialSolvableFetch: typeof fetch = async (input, init) => {
      const url = String(input);
      if (url.endsWith("/health")) {
        return mockJsonResponse(200, { status: "ok", composable: false });
      }
      const payload = JSON.parse(String(init?.body ?? "{}")) as Record<
        string,
        unknown
      >;
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
      if (proof.includes("first | rfl | decide")) {
        return mockJsonResponse(200, {
          verdict: "pass",
          detail: "solved by decide",
        });
      }
      return mockJsonResponse(200, {
        verdict: "fail",
        detail: "unsolved goals",
      });
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
      /was solved trivially by probe 'positive-tactic-blast-triviality'/
    );

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
      return mockJsonResponse(200, {
        verdict: "fail",
        detail: "unsolved goals",
      });
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
      5,
      "expected 1 stage-A1 + 1 companion + 3 fast runner probes (non-∃∀) = 5 /check calls"
    );
  }
  console.log(
    "  [PASS] Solvable entries run companion proof AND all runner tactic probes"
  );

  // 6. Runner HTTP 500, unreachable port, and malformed probe detail fail closed (0 tasks seeded)
  {
    let campaignPostCount = 0;
    const mock500Fetch: typeof fetch = async (input) => {
      const url = String(input);
      if (url.includes("/campaigns")) campaignPostCount++;
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
        return mockJsonResponse(200, {
          verdict: "fail",
          detail: "type mismatch",
        });
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
      /FAIL-CLOSED/
    );

    const cliRun = spawnSync(
      "npx",
      [
        "tsx",
        path.join(REPO_ROOT, "scripts/e2e/seed-lean-catalog.ts"),
        "--check-only",
        "--runner-only",
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
    assert.notEqual(skipPrecheckRun.status, 0);
    assert.match(skipPrecheckRun.stderr, /--skip-precheck is forbidden/);
  }
  console.log(
    "  [PASS] Runner HTTP 500, malformed probe, unreachable runner & --skip-precheck fail closed"
  );

  // 7. Rung N+1 seeding guard against registry.json + composable health guard
  {
    const emptyRegistry = loadProofRegistry();
    const bsd1 = chainEntries.find(
      (e) => e.name === "chain-bsd-1-weierstrass-bijection"
    )!;

    assert.throws(
      () =>
        assertSeedingAllowedForEntry(bsd1, 4, emptyRegistry, {
          status: "ok",
          composable: false,
        }),
      /POLICY-4 SEED GUARD/
    );
    assert.equal(
      assertSeedingAllowedForEntry(bsd1, 4, emptyRegistry, {
        status: "ok",
        composable: true,
      }),
      undefined
    );
    assert.throws(
      () =>
        assertSeedingAllowedForEntry(bsd2, 4, emptyRegistry, {
          status: "ok",
          composable: true,
        }),
      /LADDER DEPENDENCY GUARD/
    );

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

async function runMathlibControlTests(): Promise<void> {
  console.log(
    "=== Running live lake env lean full-Mathlib per-probe, bridge, and late-search positive controls ==="
  );

  // 1. Retired Chebyshev Rung 1 MUST be caught via general `bridge_primorial` + `import Mathlib` `positive-exact`
  const chebyshevPath = path.join(
    CATALOG_DIR,
    "retired",
    "chain-rh-analytic-1-chebyshev-theta-upper.lean"
  );
  const chebyshevEntry: CatalogEntry = {
    name: "chain-rh-analytic-1-chebyshev-theta-upper",
    kind: "open",
    program: "millennium-chains",
    statement: fs.readFileSync(chebyshevPath, "utf8"),
  };
  assert.throws(
    () =>
      runFullMathlibSearchProbes(chebyshevEntry, {
        onlyLabels: new Set(["positive-exact"]),
      }),
    /was solved trivially by full-Mathlib probe 'positive-exact'/
  );
  console.log(
    "  [CONTROL PASS] retired/chain-rh-analytic-1-chebyshev-theta-upper caught by bridge_primorial + positive-exact"
  );

  // 2. S-c Wrapped-def + Bridge controls (lagariasBound + bridges, isSquarefreePos, mertensSum, sigmaRat, mobiusInt, harmonicRat)
  const bridgeAndWrapperControls: Array<{
    controlName: string;
    statement: string;
    onlyLabel: string;
    expectedRegex: RegExp;
  }> = [
    {
      controlName: "control-mobiusInt-prime",
      statement:
        "import SwarmProofs.Chains.RHAnalytic.Defs.V1\ndef statementProp : Prop := ∀ (p : ℕ), Nat.Prime p → mobiusInt p = -1\n",
      onlyLabel: "positive-exact",
      expectedRegex:
        /was solved trivially by full-Mathlib probe 'positive-exact'/,
    },
    {
      controlName: "control-mertensSum-bridge",
      statement:
        "import SwarmProofs.Chains.RHAnalytic.Defs.V1\ndef statementProp : Prop := ∀ (N : ℕ), mertensSum N = Finset.sum (Finset.range (N + 1)) (fun n => ArithmeticFunction.moebius n)\n",
      onlyLabel: "positive-exact",
      expectedRegex:
        /was solved trivially by full-Mathlib probe 'positive-exact'/,
    },
    {
      controlName: "control-harmonicRat-pos",
      statement:
        "import SwarmProofs.Chains.RHLagarias.Defs.V1\ndef statementProp : Prop := ∀ (n : ℕ), n ≠ 0 → 0 < harmonicRat n\n",
      onlyLabel: "positive-exact",
      expectedRegex:
        /was solved trivially by full-Mathlib probe 'positive-exact'/,
    },
    {
      controlName: "control-sigmaRat-bridge",
      statement:
        "import SwarmProofs.Chains.RHLagarias.Defs.V1\ndef statementProp : Prop := ∀ (n : ℕ), sigmaRat n = (ArithmeticFunction.sigma 1 n : ℚ)\n",
      onlyLabel: "positive-exact",
      expectedRegex:
        /was solved trivially by full-Mathlib probe 'positive-exact'/,
    },
    {
      controlName: "control-isSquarefreePos-exact",
      statement:
        "import SwarmProofs.Chains.RHLagarias.Defs.V1\ndef statementProp : Prop := ∀ (p : ℕ), 0 < p → Squarefree p → isSquarefreePos p\n",
      onlyLabel: "positive-exact",
      expectedRegex:
        /was solved trivially by full-Mathlib probe 'positive-exact'/,
    },
    {
      controlName: "control-isSquarefreePos-prime",
      statement:
        "import SwarmProofs.Chains.RHLagarias.Defs.V1\ndef statementProp : Prop := ∀ (p : ℕ), Nat.Prime p → isSquarefreePos p\n",
      onlyLabel: "positive-exact",
      expectedRegex:
        /was solved trivially by full-Mathlib probe 'positive-exact'/,
    },
    {
      controlName: "control-wrapper-lagariasBound-bridges",
      statement:
        "import SwarmProofs.Chains.RHLagarias.Defs.V1\ndef statementProp : Prop := ∀ (n : ℕ), lagariasBound n → (ArithmeticFunction.sigma 1 n : ℝ) ≤ (harmonic n : ℝ) + Real.exp (harmonic n : ℝ) * Real.log (harmonic n : ℝ)\n",
      onlyLabel: "positive-exact",
      expectedRegex:
        /was solved trivially by full-Mathlib probe 'positive-exact'/,
    },
  ];

  for (const ctrl of bridgeAndWrapperControls) {
    assert.throws(
      () =>
        runFullMathlibSearchProbes(
          {
            name: ctrl.controlName,
            kind: "open",
            program: "millennium-chains",
            statement: ctrl.statement,
          },
          { onlyLabels: new Set([ctrl.onlyLabel]) }
        ),
      ctrl.expectedRegex
    );
    console.log(
      `  [CONTROL PASS] ${ctrl.controlName} caught by dsimp[wrappers]+simp[bridges]+${ctrl.onlyLabel}`
    );
  }

  // 3. X1 Late-search positive control: verify empirically in Lean 4.33 that `maxHeartbeats 100000`
  //    aborts early with "`exact?` could not close the goal", while our `maxHeartbeats 0` gate CATCHES & REJECTS IT!
  {
    const lateSearchStmt =
      "def statementProp : Prop := ∀ (α : Type) [LinearOrder α] (a b : α), a ≤ b → b ≤ a → a = b\n";
    const cappedLeanSrc = [
      "import Mathlib",
      lateSearchStmt,
      "set_option maxHeartbeats 100000 in",
      "theorem __capped_late_ctrl : statementProp := by",
      "  unfold statementProp",
      "  intros",
      "  exact?",
    ].join("\n");
    const elanBin = path.join(process.env.HOME ?? "", ".elan", "bin");
    const curPath = process.env.PATH ?? "";
    const cappedProc = spawnSync("lake", ["env", "lean", "--stdin"], {
      cwd: path.join(REPO_ROOT, "proofs", "SwarmProofs"),
      env: {
        ...process.env,
        PATH: curPath.includes(elanBin) ? curPath : `${elanBin}:${curPath}`,
      },
      input: cappedLeanSrc,
      encoding: "utf8",
      timeout: 120_000,
    });
    const cappedOut = `${cappedProc.stdout ?? ""}\n${cappedProc.stderr ?? ""}`;
    assert.match(
      cappedOut,
      /`exact\?` could not close the goal/,
      `Expected capped maxHeartbeats 100000 exact? to abort early on late-search goal, got:\n${cappedOut}`
    );
    assert.throws(
      () =>
        runFullMathlibSearchProbes(
          {
            name: "control-late-search-exact-unbounded",
            kind: "open",
            program: "general",
            statement: lateSearchStmt,
          },
          { onlyLabels: new Set(["positive-exact"]) }
        ),
      /was solved trivially by full-Mathlib probe 'positive-exact'/
    );
    console.log(
      "  [CONTROL PASS] X1 late-search positive control: aborted under maxHeartbeats 100000, CAUGHT & REJECTED under maxHeartbeats 0"
    );
  }

  // 4. S-b Dedicated independent single-label positive controls for ALL 10 probe kinds (strict regex per label):
  const perProbeControls: Array<{
    controlName: string;
    statement: string;
    onlyLabel: string;
    expectedMessage: RegExp;
  }> = [
    {
      controlName: "control-positive-exact",
      statement: "def statementProp : Prop := ∀ (p : ℕ), Nat.Prime p → 2 ≤ p\n",
      onlyLabel: "positive-exact",
      expectedMessage:
        /was solved trivially by full-Mathlib probe 'positive-exact'/,
    },
    {
      controlName: "control-negative-exact",
      statement: "def statementProp : Prop := Nat.Prime 1\n",
      onlyLabel: "negative-exact",
      expectedMessage:
        /was solved trivially by full-Mathlib probe 'negative-exact'/,
    },
    {
      controlName: "control-positive-apply-assumption",
      statement:
        "def statementProp : Prop := ∀ (a b c : ℕ), a ∣ b → b ∣ c → a ∣ c\n",
      onlyLabel: "positive-apply-assumption",
      expectedMessage:
        /was solved trivially by full-Mathlib probe 'positive-apply-assumption'/,
    },
    {
      controlName: "control-negative-apply-assumption",
      statement: "def statementProp : Prop := Nat.Prime 1\n",
      onlyLabel: "negative-apply-assumption",
      expectedMessage:
        /was solved trivially by full-Mathlib probe 'negative-apply-assumption'/,
    },
    {
      controlName: "control-positive-solve-by-elim",
      statement:
        "def statementProp : Prop := ∀ (P Q R : Prop), (P → Q) → (Q → R) → P → R\n",
      onlyLabel: "positive-solve-by-elim",
      expectedMessage:
        /was solved trivially by full-Mathlib probe 'positive-solve-by-elim'/,
    },
    {
      controlName: "control-negative-solve-by-elim",
      statement: "def statementProp : Prop := ∀ (P : Prop), P\n",
      onlyLabel: "negative-solve-by-elim",
      expectedMessage:
        /was solved trivially by full-Mathlib probe 'negative-solve-by-elim'/,
    },
    {
      controlName: "control-positive-hint",
      statement:
        "def statementProp : Prop := ∀ (x y : ℚ), (x + y) ^ 2 - (x - y) ^ 2 = 4 * x * y\n",
      onlyLabel: "positive-hint",
      expectedMessage:
        /was solved trivially by full-Mathlib probe 'positive-hint'/,
    },
    {
      controlName: "control-negative-hint",
      statement: "def statementProp : Prop := (2 : ℕ) + 2 = 5\n",
      onlyLabel: "negative-hint",
      expectedMessage:
        /was solved trivially by full-Mathlib probe 'negative-hint'/,
    },
    {
      controlName: "control-positive-aesop",
      statement:
        "def statementProp : Prop := ∀ (P Q R : Prop), (P ∧ Q) ∧ (Q → R) → R ∧ P\n",
      onlyLabel: "positive-aesop",
      expectedMessage:
        /was solved trivially by full-Mathlib probe 'positive-aesop'/,
    },
    {
      controlName: "control-negative-aesop",
      statement: "def statementProp : Prop := ∃ (P : Prop), P ∧ ¬ P\n",
      onlyLabel: "negative-aesop",
      expectedMessage:
        /was solved trivially by full-Mathlib probe 'negative-aesop'/,
    },
  ];

  for (const ctrl of perProbeControls) {
    const entry: CatalogEntry = {
      name: ctrl.controlName,
      kind: "open",
      program: "general",
      statement: ctrl.statement,
    };
    assert.throws(
      () =>
        runFullMathlibSearchProbes(entry, {
          onlyLabels: new Set([ctrl.onlyLabel]),
        }),
      ctrl.expectedMessage,
      `Expected ${ctrl.controlName} to be caught by single probe '${ctrl.onlyLabel}'`
    );
    console.log(
      `  [CONTROL PASS] ${ctrl.controlName} caught independently by '${ctrl.onlyLabel}'`
    );
  }

  console.log("=== All full-Mathlib per-probe positive controls PASSED ===\n");
}

async function runLiveRunnerTests(runnerOnly: boolean): Promise<void> {
  console.log("=== Running live shillbot-lean-runner gate assertions ===");

  // 1. Trivial tactic-blast statement MUST be rejected by the live runner stage
  const trivialRunnerControl: CatalogEntry = {
    name: "control-runner-trivial-ring",
    kind: "open",
    program: "general",
    statement:
      "import Mathlib.Data.Nat.Basic\ndef statementProp : Prop := ∀ (n : ℕ), n + 0 = n\n",
  };
  await assert.rejects(
    () =>
      verifyStatementElaborates(trivialRunnerControl, {
        checkOnly: true,
        leanPolicy: 2,
        runLocalMathlibSearch: false,
      }),
    /was solved trivially by probe/
  );
  console.log(
    "  [LIVE RUNNER PASS] trivial arithmetic statement REJECTED by live runner"
  );

  // 2. When local lake is enabled (`--live` without `--runner-only`), verify retired Chebyshev is REJECTED end-to-end
  if (!runnerOnly) {
    const chebyshevPath = path.join(
      CATALOG_DIR,
      "retired",
      "chain-rh-analytic-1-chebyshev-theta-upper.lean"
    );
    const chebyshevEntry: CatalogEntry = {
      name: "chain-rh-analytic-1-chebyshev-theta-upper",
      kind: "open",
      program: "millennium-chains",
      statement: fs.readFileSync(chebyshevPath, "utf8"),
    };
    await assert.rejects(
      () =>
        verifyStatementElaborates(chebyshevEntry, {
          checkOnly: true,
          leanPolicy: 3,
          runLocalMathlibSearch: true,
        }),
      /was solved trivially by full-Mathlib probe 'positive-exact'/
    );
    console.log(
      "  [LIVE FULL PASS] retired Chebyshev Rung 1 REJECTED end-to-end"
    );
  }

  // 3. Hardened C1 (Mertens), C2 (Mobius), E2 (Delta_0 Los + Lagarias), and D2 (Shannon) MUST be ACCEPTED
  const allOpen = loadCatalog(false);
  const catalogByName = new Map(allOpen.map((e) => [e.name, e]));
  for (const targetName of [
    "chain-rh-analytic-1-mertens-first-prime-log-bound",
    "chain-rh-analytic-2-mobius-harmonic-bound",
    "chain-rh-meta-2-nonstandard-ultrapower-pi1",
    "chain-pnp-circuit-2-shannon-counting-bound",
  ]) {
    const entry = catalogByName.get(targetName);
    assert.ok(entry, `missing open catalog entry ${targetName}`);
    await verifyStatementElaborates(entry, {
      checkOnly: true,
      leanPolicy: 4,
      catalogByName,
      runLocalMathlibSearch: !runnerOnly,
    });
    console.log(`  [LIVE PASS] ${targetName} ACCEPTED`);
  }

  console.log("=== All live runner gate assertions PASSED ===");
}

async function main(): Promise<void> {
  const unitOnly = process.argv.includes("--unit-only");
  const mathlibControls = process.argv.includes("--mathlib-controls");
  const live = process.argv.includes("--live");
  const runnerOnly = process.argv.includes("--runner-only");

  if (!unitOnly && !mathlibControls && !live) {
    throw new Error(
      "Specify --unit-only, --mathlib-controls, or --live when running test-lean-catalog-gate.ts"
    );
  }

  await runUnitTests();
  if (mathlibControls || (live && !runnerOnly)) {
    await runMathlibControlTests();
  }
  if (live) {
    await runLiveRunnerTests(runnerOnly);
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
