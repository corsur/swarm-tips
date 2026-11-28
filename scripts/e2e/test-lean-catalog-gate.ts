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
  assertMainnetEntryAllowed,
  assertMainnetPolicyCoverage,
  assertNotPreviouslySettled,
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
  fetchSettledMainnetIndex,
  findUnbakedSwarmDefsImports,
  hasExistentialUniversalStructure,
  inlineSwarmProofsImportsForCompat,
  loadCatalog,
  loadMainnetPolicy,
  loadProofRegistry,
  MainnetPolicyManifest,
  negateStatementProp,
  normalizedStatementSha256,
  parseLeanPolicyArg,
  parseNetworkArg,
  ProofRegistryManifest,
  sha256Hex,
  UNFOLD_DEFS_BY_MODULE,
  WRAPPER_DEFS_BY_MODULE,
  REPO_ROOT,
  resolveBridgeLemmasForStatement,
  runFullMathlibSearchProbes,
  stripLeanComments,
  verifyNonVacuousWitness,
  verifyStatementElaborates,
} from "./seed-lean-catalog";
import {
  checkAxiomOutputIsStandard,
  compileRedTeamCandidate,
  extractLeanCodeBlock,
} from "./redteam-lean-statement";

function mockJsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

async function runUnitTests(): Promise<void> {
  console.log("=== Running Lean catalog gate offline unit tests ===");

  // 0. S1 + N1: Verify PROBE_MATHLIB_UMBRELLAS and swarm_fold_primorial are gone and ALLOWED_SWARM_LIBRARY_IMPORTS has ONLY the 8 frozen Defs.V1 modules
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
    "SwarmProofs.Chains.BoolFourier.Defs.V1",
    "SwarmProofs.Chains.Circuits.Defs.V1",
    "SwarmProofs.Chains.NavierStokes.Defs.V1",
    "SwarmProofs.Chains.ProtocolMath.Defs.V1",
    "SwarmProofs.Chains.QuantumInfo.Defs.V1",
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
  for (const modName of [
    "SwarmProofs.Chains.BoolFourier.Defs.V1",
    "SwarmProofs.Chains.QuantumInfo.Defs.V1",
    "SwarmProofs.Chains.NavierStokes.Defs.V1",
    "SwarmProofs.Chains.ProtocolMath.Defs.V1",
  ]) {
    assert.ok(
      (UNFOLD_DEFS_BY_MODULE[modName]?.length ?? 0) > 0,
      `expected non-empty UNFOLD_DEFS_BY_MODULE for ${modName}`
    );
    assert.ok(
      (WRAPPER_DEFS_BY_MODULE[modName]?.length ?? 0) > 0,
      `expected non-empty WRAPPER_DEFS_BY_MODULE for ${modName}`
    );
    assert.deepEqual(BRIDGE_LEMMAS_BY_DEFS_MODULE[modName], []);
  }
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
    33,
    "expected all 33 open chain-*.lean entries (18 existing + 15 CS/Physics/ProtocolMath)"
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
      !stripLeanComments(inlined).includes("#guard"),
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
    "  [PASS] 33 chain-*.lean headers, byte-for-byte parity, composability, CTA notes & inline compat"
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

  // 8. Unbaked Defs.V1 runner revision guard: fail closed on real seeding, inline on composable v4.33.0-rc2 for --check-only
  {
    const baseRevRes = spawnSync("git", ["rev-parse", "origin/main"], {
      cwd: REPO_ROOT,
      encoding: "utf8",
    });
    const baseRev = baseRevRes.status === 0 ? baseRevRes.stdout.trim() : "";
    const f1 = chainEntries.find(
      (e) => e.name === "chain-bool-fourier-1-parseval-poincare-bonami"
    )!;
    const pm1 = chainEntries.find(
      (e) => e.name === "chain-protocol-math-1-advance-recoupment-safety"
    )!;
    assert.ok(
      f1 && pm1,
      "expected new BoolFourier & ProtocolMath Rung 1 entries"
    );
    // Multi-line #guard stripping check on ProtocolMath & QuantumInfo inlined payloads
    const pm1Inlined = inlineSwarmProofsImportsForCompat(pm1.statement);
    assert.ok(
      !pm1Inlined.includes("some { to_backer := 2600000"),
      "multi-line #guard continuation must be stripped from ProtocolMath inline compat payload"
    );

    if (baseRev && /^[0-9a-f]{40}$/i.test(baseRev)) {
      const f1UnbakedOnBase = findUnbakedSwarmDefsImports(
        f1.statement,
        baseRev
      );
      const bsd1Entry = chainEntries.find(
        (e) => e.name === "chain-bsd-1-weierstrass-bijection"
      )!;
      assert.deepEqual(
        findUnbakedSwarmDefsImports(bsd1Entry.statement, baseRev),
        [],
        "BSD Defs.V1 already exists on origin/main"
      );
      // Only assert unbaked if origin/main does not yet contain BoolFourier (pre-merge)
      if (f1UnbakedOnBase.length > 0) {
        assert.deepEqual(f1UnbakedOnBase, [
          "SwarmProofs.Chains.BoolFourier.Defs.V1",
        ]);
        const capturedPayloads: Array<Record<string, unknown>> = [];
        const mockOldRevFetch: typeof fetch = async (input, init) => {
          const url = String(input);
          if (url.endsWith("/health")) {
            return mockJsonResponse(200, {
              status: "ok",
              composable: true,
              swarm_proofs_rev: baseRev,
            });
          }
          const payload = JSON.parse(String(init?.body ?? "{}")) as Record<
            string,
            unknown
          >;
          capturedPayloads.push(payload);
          return mockJsonResponse(200, {
            verdict: "fail",
            detail: "unsolved goals",
          });
        };
        // Real seeding (!checkOnly) MUST fail closed!
        await assert.rejects(
          () =>
            verifyStatementElaborates(f1, {
              fetchImpl: mockOldRevFetch,
              runnerUrl: "https://runner.mock",
              runnerToken: "tok",
              checkOnly: false,
              leanPolicy: 4,
              skipSanityScript: true,
              retryDelayBaseMs: 0,
            }),
          /does not yet include SwarmProofs\.Chains\.BoolFourier\.Defs\.V1/
        );
        // Pre-rollout --check-only inlines locally while STAYING on mode="composable" (v4.33.0-rc2)!
        await verifyStatementElaborates(f1, {
          fetchImpl: mockOldRevFetch,
          runnerUrl: "https://runner.mock",
          runnerToken: "tok",
          checkOnly: true,
          leanPolicy: 4,
          skipSanityScript: true,
          retryDelayBaseMs: 0,
        });
        assert.ok(capturedPayloads.length > 0);
        for (const p of capturedPayloads) {
          assert.equal(
            p.mode,
            "composable",
            "pre-rollout inline check must stay on composable v4.33.0-rc2 toolchain"
          );
          assert.ok(
            !/\bimport\s+SwarmProofs\b/.test(String(p.statement ?? "")),
            "pre-rollout inline check must expand unbaked SwarmProofs import"
          );
        }
      }
    }
  }
  console.log(
    "  [PASS] Multi-line #guard stripping & unbaked Defs.V1 fail-closed seeding / composable check-only guard"
  );

  // 9. Mainnet Open-Only & Novel-Unpublished Guardrails (Guardrails 1-4)
  {
    const mainnetPolicy = loadMainnetPolicy();
    assertMainnetPolicyCoverage(openEntries, mainnetPolicy);

    const openByName = new Map(openEntries.map((e) => [e.name, e]));
    const fixedNow = new Date("2026-11-28T12:00:00Z");

    // 9a. Published theorems, warmups, Rung 1-3, Aaronson-Ambainis (proved Nov 6, 2026), NLTS (2022), proof-golf, and toy invariants MUST be blocked on mainnet
    for (const blockedSlug of [
      "chain-bool-fourier-1-parseval-poincare-bonami",
      "chain-bool-fourier-2-kkl-influential-variable",
      "chain-bool-fourier-3-hastad-ac0-parity",
      "chain-bool-fourier-4-aaronson-ambainis-conjecture",
      "chain-quantum-info-4-nlts-quantum-pcp-gap",
      "chain-navier-stokes-1-dyadic-energy-subcritical",
      "chain-protocol-math-1-advance-recoupment-safety",
      "golf-erdos-szekeres-monotone-subseq",
      "invariant-shillbot-recoupment-conservation",
      "post-rh-robin-criterion-equiv",
    ]) {
      const entry = openByName.get(blockedSlug);
      assert.ok(entry, `missing expected catalog file ${blockedSlug}`);
      assert.throws(
        () =>
          assertMainnetEntryAllowed(entry, mainnetPolicy, {
            now: fixedNow,
            requireRedteam: false,
          }),
        /\[MAINNET POLICY GUARD\].*published-theorem/,
        `${blockedSlug} must be hard-blocked on mainnet`
      );
    }

    // 9b. Unclassified slugs fail closed on mainnet
    const unclassifiedEntry: CatalogEntry = {
      name: "unclassified-frontier-test",
      kind: "open",
      program: "general",
      statement: "def statementProp : Prop := True\n",
    };
    assert.throws(
      () =>
        assertMainnetEntryAllowed(unclassifiedEntry, mainnetPolicy, {
          now: fixedNow,
        }),
      /not classified in mainnet-policy\.json/
    );

    // 9c. Open problems without red-team attestation fail closed when requireRedteam=true
    const ns4Entry = openByName.get(
      "chain-navier-stokes-4-leray-3d-uniform-enstrophy-millennium"
    )!;
    assert.equal(
      ns4Entry.dependsOn,
      undefined,
      "3D Navier-Stokes Millennium frontier must be decoupled from paid Rung 1-3 prerequisites"
    );

    const syntheticPolicy: MainnetPolicyManifest = JSON.parse(
      JSON.stringify(mainnetPolicy)
    );
    delete syntheticPolicy.entries[ns4Entry.name].redteam;
    assert.throws(
      () =>
        assertMainnetEntryAllowed(ns4Entry, syntheticPolicy, {
          now: fixedNow,
          requireRedteam: true,
        }),
      /\[RED-TEAM GUARD\]/
    );

    // 9d. Offline LLM attestation with web_literature_checked=false MUST fail closed (prevents missing newly published proofs like Nov 6, 2026 Aaronson-Ambainis)
    const ns4Sha = sha256Hex(ns4Entry.statement);
    syntheticPolicy.entries[ns4Entry.name].redteam = {
      passed: true,
      statement_sha256: ns4Sha,
      provider: "xai",
      model: "grok-4",
      iterations: 6,
      duration_secs: 180,
      tested_at: "2026-11-28T09:00:00Z",
      attacks: ["cheat", "disprove", "prove"],
      web_literature_checked: false,
    };
    assert.throws(
      () =>
        assertMainnetEntryAllowed(ns4Entry, syntheticPolicy, {
          now: fixedNow,
          requireRedteam: true,
        }),
      /\[RED-TEAM GUARD\]/
    );

    // Valid web-literature-checked Antigravity red-team attestation bound to statement_sha256 passes
    syntheticPolicy.entries[ns4Entry.name].redteam = {
      passed: true,
      statement_sha256: ns4Sha,
      provider: "antigravity",
      model: "antigravity-subagent",
      iterations: 6,
      duration_secs: 180,
      tested_at: "2026-11-28T09:00:00Z",
      attacks: ["cheat", "disprove", "prove"],
      web_literature_checked: true,
    };
    assertMainnetEntryAllowed(ns4Entry, syntheticPolicy, {
      now: fixedNow,
      requireRedteam: true,
    });

    // Missing 'disprove' attack fails
    syntheticPolicy.entries[ns4Entry.name].redteam = {
      passed: true,
      statement_sha256: ns4Sha,
      provider: "xai",
      model: "grok-4",
      iterations: 6,
      duration_secs: 180,
      tested_at: "2026-11-28T09:00:00Z",
      attacks: ["cheat", "prove"],
      web_literature_checked: true,
    };
    assert.throws(
      () =>
        assertMainnetEntryAllowed(ns4Entry, syntheticPolicy, {
          now: fixedNow,
          requireRedteam: true,
        }),
      /\[RED-TEAM GUARD\]/
    );

    // Stale literature_checked (>30d) fails
    syntheticPolicy.entries[ns4Entry.name].redteam = {
      passed: true,
      statement_sha256: ns4Sha,
      provider: "antigravity",
      model: "antigravity-subagent",
      iterations: 6,
      duration_secs: 180,
      tested_at: "2026-11-28T09:00:00Z",
      attacks: ["cheat", "disprove", "prove"],
      web_literature_checked: true,
    };
    syntheticPolicy.entries[ns4Entry.name].literature_checked = "2026-10-01";
    assert.throws(
      () =>
        assertMainnetEntryAllowed(ns4Entry, syntheticPolicy, {
          now: fixedNow,
          requireRedteam: true,
        }),
      /literature_checked .* is stale/
    );
    syntheticPolicy.entries[ns4Entry.name].literature_checked = "2026-11-28";

    // Statement SHA-256 drift fails closed
    const modifiedNs4: CatalogEntry = {
      ...ns4Entry,
      statement: `${ns4Entry.statement}\n-- edited\n`,
    };
    assert.throws(
      () =>
        assertMainnetEntryAllowed(modifiedNs4, syntheticPolicy, {
          now: fixedNow,
          requireRedteam: true,
        }),
      /statement_sha256 drift/
    );

    // 9e. novel-unpublished requires target_frontier, novelty_rationale, witness, web_literature_checked, and >= 10 redteam iterations
    const novelEntry: CatalogEntry = {
      name: "novel-hyperviscous-ns-6-5",
      kind: "open",
      program: "general",
      statement: "def statementProp : Prop := ∀ n : Nat, n = n\n",
    };
    const novelSha = sha256Hex(novelEntry.statement);
    syntheticPolicy.entries[novelEntry.name] = {
      status: "novel-unpublished",
      statement_sha256: novelSha,
      target_frontier: "3D Navier-Stokes regularity",
      novelty_rationale:
        "Unpublished intermediate regularity threshold gamma=6/5 strictly below Lions 5/4.",
      literature_checked: "2026-11-28",
      nonvacuous_witness: "nonvacuous/goldbach.nonvacuous.lean",
      redteam: {
        passed: true,
        statement_sha256: novelSha,
        provider: "antigravity",
        model: "antigravity-subagent",
        iterations: 5, // too low for novel-unpublished (requires >= 10)
        duration_secs: 90,
        tested_at: "2026-11-28T09:00:00Z",
        attacks: ["cheat", "disprove", "prove"],
        web_literature_checked: true,
      },
    };
    assert.throws(
      () =>
        assertMainnetEntryAllowed(novelEntry, syntheticPolicy, {
          now: fixedNow,
          requireRedteam: true,
        }),
      /\[RED-TEAM GUARD\].*min_iterations=10/
    );
    (
      syntheticPolicy.entries[novelEntry.name].redteam as { iterations: number }
    ).iterations = 12;
    assertMainnetEntryAllowed(novelEntry, syntheticPolicy, {
      now: fixedNow,
      requireRedteam: true,
    });

    // 9f. Guardrail 4: Settled-duplicate lock (slug, exact sha, normalized sha, and active registry alias)
    const mockSettledFetch: typeof fetch = async () =>
      mockJsonResponse(200, {
        network: "mainnet",
        total_verified: 1,
        proofs: [
          {
            task_id: "task-settled-1",
            slug: "chain-pnp-circuit-1-de-morgan-parity-lower-bound",
            state: "finalized",
            statement_sha256: sha256Hex(
              openByName.get(
                "chain-pnp-circuit-1-de-morgan-parity-lower-bound"
              )!.statement
            ),
            reward_lamports: 18_000_000,
          },
          {
            task_id: "task-settled-norm",
            slug: "old-alias-name",
            state: "verified",
            statement_sha256: normalizedStatementSha256(novelEntry.statement),
            reward_lamports: 10_000_000,
          },
        ],
      });
    const settledIndex = await fetchSettledMainnetIndex(
      "mainnet",
      mockSettledFetch,
      "https://api.example.test"
    );
    const emptyRegistry: ProofRegistryManifest = {
      schema: "swarm.proof-registry/v1",
      modules: [],
      aliases: {},
    };
    assert.throws(
      () =>
        assertNotPreviouslySettled(
          openByName.get("chain-pnp-circuit-1-de-morgan-parity-lower-bound")!,
          settledIndex,
          emptyRegistry
        ),
      /\[SETTLED-DUPLICATE GUARD\].*ALREADY proved and settled/
    );
    // Re-formatted statement with extra comment still matches normalizedStatementSha256
    assert.throws(
      () =>
        assertNotPreviouslySettled(
          {
            ...novelEntry,
            name: "renamed-duplicate-slug",
            statement: `-- sneak comment\n${novelEntry.statement}   \n`,
          },
          settledIndex,
          emptyRegistry
        ),
      /\[SETTLED-DUPLICATE GUARD\].*normalized statement_sha256/
    );
    // Failing /proofs endpoint fails closed
    await assert.rejects(
      () =>
        fetchSettledMainnetIndex(
          "mainnet",
          async () => mockJsonResponse(503, { error: "down" }),
          "https://api.example.test"
        ),
      /\[SETTLED-DUPLICATE GUARD\].*HTTP 503/
    );

    // 9g. Red-team helper checks (code extraction & axiom audit)
    assert.equal(
      extractLeanCodeBlock(
        "Here:\n```lean\ntheorem proof : statementProp := rfl\n```"
      ),
      "theorem proof : statementProp := rfl"
    );
    assert.equal(
      checkAxiomOutputIsStandard(
        "'proof' depends on axioms: [propext, Classical.choice, Quot.sound]"
      ),
      true
    );
    assert.equal(
      checkAxiomOutputIsStandard("'proof' does not depend on any axioms"),
      true
    );
    assert.equal(
      checkAxiomOutputIsStandard(
        "'proof' depends on axioms: [propext, sorryAx]"
      ),
      false
    );
  }
  console.log(
    "  [PASS] Mainnet Guardrails 1-4 (open-only/novel-unpublished lock, red-team attestation, non-vacuity check, settled-duplicate lock)"
  );
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

async function runNonVacuousWitnessGate(): Promise<void> {
  console.log(
    "=== Compiling machine-checked non-vacuity witnesses & red-team compiler positive controls ==="
  );
  const policy = loadMainnetPolicy();
  const openEntries = loadCatalog(false);
  const byName = new Map(openEntries.map((e) => [e.name, e]));

  let checked = 0;
  for (const [slug, pol] of Object.entries(policy.entries)) {
    if (pol.status !== "open-problem" && pol.status !== "novel-unpublished") {
      continue;
    }
    const entry = byName.get(slug);
    assert.ok(entry, `missing catalog entry for '${slug}'`);
    verifyNonVacuousWitness(entry, pol);
    checked++;
  }
  assert.ok(
    checked >= 9,
    `expected at least 9 non-vacuity witnesses compiled, got ${checked}`
  );

  // Positive control 1: red-team compiler catches a loophole proof of a trivial statement
  const trivialStmt =
    "import Mathlib.Data.Nat.Basic\ndef statementProp : Prop := ∀ (n : ℕ), n + 0 = n\n";
  const loopholeRes = compileRedTeamCandidate(
    trivialStmt,
    "theorem proof : statementProp := fun _ => rfl",
    "cheat"
  );
  assert.equal(
    loopholeRes.passed,
    true,
    `red-team compiler must detect valid cheat proof: ${loopholeRes.stderr}`
  );

  // Positive control 2: red-team compiler catches a disproof of a missing-precondition bug
  const buggyStmt =
    "import Mathlib.Data.Nat.Basic\ndef statementProp : Prop := ∀ (n : ℕ), 0 < n\n";
  const disproveRes = compileRedTeamCandidate(
    buggyStmt,
    "theorem disproof : ¬ statementProp := fun h => Nat.not_lt_zero 0 (h 0)",
    "disprove"
  );
  assert.equal(
    disproveRes.passed,
    true,
    `red-team compiler must detect valid counterexample disproof: ${disproveRes.stderr}`
  );

  // Negative control: sorry proof is rejected even if it typechecks
  const sorryRes = compileRedTeamCandidate(
    trivialStmt,
    "theorem proof : statementProp := by sorry",
    "prove"
  );
  assert.equal(
    sorryRes.passed,
    false,
    "red-team compiler must reject sorry proofs"
  );

  console.log(
    `=== All ${checked} non-vacuity witnesses + red-team compiler positive controls PASSED ===\n`
  );
}

async function main(): Promise<void> {
  const unitOnly = process.argv.includes("--unit-only");
  const nonvacuousOnly = process.argv.includes("--nonvacuous-only");
  const mathlibControls = process.argv.includes("--mathlib-controls");
  const live = process.argv.includes("--live");
  const runnerOnly = process.argv.includes("--runner-only");

  if (!unitOnly && !nonvacuousOnly && !mathlibControls && !live) {
    throw new Error(
      "Specify --unit-only, --nonvacuous-only, --mathlib-controls, or --live when running test-lean-catalog-gate.ts"
    );
  }

  if (nonvacuousOnly && !unitOnly && !mathlibControls && !live) {
    await runNonVacuousWitnessGate();
    return;
  }

  await runUnitTests();
  if (nonvacuousOnly || mathlibControls || (live && !runnerOnly)) {
    await runNonVacuousWitnessGate();
  }
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
