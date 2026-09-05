import { createHash } from "node:crypto";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repositoryRoot = resolve(packageRoot, "../..");
const check = process.argv.includes("--check");
const manifestPath = resolve(packageRoot, "scripts/generated-bindings.json");

const files = [
  [
    "target/idl/coordination_game.json",
    "src/contracts/idl/coordination_game.json",
  ],
  ["target/idl/shillbot.json", "src/contracts/idl/shillbot.json"],
  [
    "target/types/coordination_game.ts",
    "src/contracts/generated/coordination_game.ts",
  ],
  ["target/types/shillbot.ts", "src/contracts/generated/shillbot.ts"],
];

const digest = (contents) =>
  createHash("sha256").update(contents).digest("hex");
const expectedDigests = check
  ? JSON.parse(readFileSync(manifestPath, "utf8"))
  : {};
const generatedDigests = {};
let stale = false;
for (const [sourceName, destinationName] of files) {
  const sourcePath = resolve(repositoryRoot, sourceName);
  const destinationPath = resolve(packageRoot, destinationName);
  if (check) {
    let destination = Buffer.alloc(0);
    try {
      destination = readFileSync(destinationPath);
    } catch {
      // Report the same actionable stale message below.
    }
    const expected = expectedDigests[destinationName];
    if (!expected || digest(destination) !== expected) {
      console.error(
        `${destinationName} does not match scripts/generated-bindings.json; ` +
          "run pnpm sync:generated after building both programs"
      );
      stale = true;
    }
    if (existsSync(sourcePath)) {
      const source = readFileSync(sourcePath);
      if (!source.equals(destination)) {
        console.error(
          `${destinationName} is stale against ${sourceName}; ` +
            "run pnpm sync:generated after building both programs"
        );
        stale = true;
      }
    }
  } else {
    const source = readFileSync(sourcePath);
    writeFileSync(destinationPath, source);
    generatedDigests[destinationName] = digest(source);
    console.log(`updated ${destinationName}`);
  }
}

if (!check) {
  writeFileSync(
    manifestPath,
    `${JSON.stringify(generatedDigests, null, 2)}\n`
  );
  console.log("updated scripts/generated-bindings.json");
}

if (stale) process.exit(1);
