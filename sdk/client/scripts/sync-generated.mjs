import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repositoryRoot = resolve(packageRoot, "../..");
const check = process.argv.includes("--check");

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

let stale = false;
for (const [sourceName, destinationName] of files) {
  const source = readFileSync(resolve(repositoryRoot, sourceName));
  const destinationPath = resolve(packageRoot, destinationName);
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

if (stale) process.exit(1);
