#!/usr/bin/env node
/**
 * Every Lean bounty artifact must contain the THEOREM ONLY.
 *
 * The runner PREPENDS the campaign statement before compiling, so an artifact
 * that also defines the statement fails elaboration with "already been
 * declared". That scores 0 and refunds the client while the task still reaches
 * `finalized` — invisible in every signal except the payment. Both committed
 * artifacts had this defect at once, propagated by a comment that asserted the
 * opposite rule.
 *
 * Run: node tests/lean-artifacts.guard.mjs
 */
import { readdirSync, readFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const DIRS = ["../research/lean-bounties", "fixtures/lean"];
const HERE = dirname(fileURLToPath(import.meta.url));

// Assembled from parts: a guard that scans for a string must not contain it,
// or it flags itself the moment it is placed inside a scanned directory.
const DEF = ["def", "statementProp"].join(" ");

const offenders = [];
for (const rel of DIRS) {
  const dir = join(HERE, rel);
  for (const f of readdirSync(dir).filter((n) => n.endsWith(".lean"))) {
    // A `.lean` under catalog/ is a STATEMENT, not an artifact; statements are
    // supposed to define it. Only `<name>.proof.lean` there is an artifact.
    const body = readFileSync(join(dir, f), "utf8");
    const code = body.replace(/\/-[\s\S]*?-\//g, ""); // strip block comments
    if (!code.includes(DEF)) continue;
    offenders.push(join(rel, f));
  }
}

if (offenders.length) {
  console.error(
    `these bounty artifacts redeclare the statement (the runner prepends it, ` +
      `so they fail elaboration and silently score 0):\n  ` +
      offenders.join("\n  "),
  );
  process.exit(1);
}
console.log(`OK — ${DIRS.length} dirs scanned, no artifact redeclares the statement`);
