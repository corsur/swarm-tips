import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repositoryRoot = resolve(packageRoot, "../..");
const check = process.argv.includes("--check");

const files = [
  ["target/idl/coordination_game.json", "src/idl/coordination_game.json"],
  ["target/idl/shillbot.json", "src/idl/shillbot.json"],
  ["target/types/coordination_game.ts", "src/generated/coordination_game.ts"],
  ["target/types/shillbot.ts", "src/generated/shillbot.ts"],
];

const runtimeIdls = [
  [
    "target/idl/coordination_game.json",
    "src/generated/coordination_game_idl.ts",
  ],
  ["target/idl/shillbot.json", "src/generated/shillbot_idl.ts"],
];

let stale = false;
for (const [sourceName, destinationName] of files) {
  const sourcePath = resolve(repositoryRoot, sourceName);
  const destinationPath = resolve(packageRoot, destinationName);
  const source = readFileSync(sourcePath);
  if (check) {
    let destination;
    try {
      destination = readFileSync(destinationPath);
    } catch {
      destination = Buffer.alloc(0);
    }
    if (!source.equals(destination)) {
      console.error(
        `${destinationName} is stale; run pnpm sync:generated after building both programs`
      );
      stale = true;
    }
  } else {
    writeFileSync(destinationPath, source);
    console.log(`updated ${destinationName}`);
  }
}

for (const [sourceName, destinationName] of runtimeIdls) {
  const sourcePath = resolve(repositoryRoot, sourceName);
  const destinationPath = resolve(packageRoot, destinationName);
  const parsed = JSON.parse(readFileSync(sourcePath, "utf8"));
  const generated = Buffer.from(
    `// Generated from ${sourceName}; do not edit.\nexport default ${JSON.stringify(
      parsed,
      null,
      2
    )} as const;\n`
  );
  if (check) {
    let destination;
    try {
      destination = readFileSync(destinationPath);
    } catch {
      destination = Buffer.alloc(0);
    }
    if (!generated.equals(destination)) {
      console.error(
        `${destinationName} is stale; run pnpm sync:generated after building both programs`
      );
      stale = true;
    }
  } else {
    writeFileSync(destinationPath, generated);
    console.log(`updated ${destinationName}`);
  }
}

if (stale) process.exit(1);
