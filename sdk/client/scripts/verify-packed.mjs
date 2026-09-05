import { mkdtempSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const tarball = process.argv[2];
if (!tarball) throw new Error("usage: verify-packed.mjs <client.tgz>");
const fixture = mkdtempSync(resolve(tmpdir(), "swarm-client-packed-"));

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: fixture,
    encoding: "utf8",
    stdio: "pipe",
    ...options,
  });
  if (result.status !== 0) {
    process.stderr.write(result.stdout ?? "");
    process.stderr.write(result.stderr ?? "");
    throw new Error(
      `${command} ${args.join(" ")} failed with ${result.status}`
    );
  }
  return result;
}

run("npm", ["init", "--yes"]);
run("npm", [
  "install",
  "--ignore-scripts",
  resolve(tarball),
  "viem@2.52.2",
  "wagmi@2.19.5",
  "esbuild@0.25.9",
]);

const imports = [
  "@swarm-tips/client",
  "@swarm-tips/client/shillbot",
  "@swarm-tips/client/coordination-game",
  "@swarm-tips/client/evm",
  "@swarm-tips/client/evm/testing",
  "@swarm-tips/client/inbox",
  "@swarm-tips/client/vow",
];
run("node", [
  "--input-type=module",
  "--eval",
  `await Promise.all(${JSON.stringify(
    imports
  )}.map((specifier) => import(specifier))); await import("@swarm-tips/client/idl/shillbot", { with: { type: "json" } }); await import("@swarm-tips/client/idl/coordination-game", { with: { type: "json" } });`,
]);

const browserImports = {
  root: "@swarm-tips/client",
  shillbot: "@swarm-tips/client/shillbot",
  coordination: "@swarm-tips/client/coordination-game",
  evm: "@swarm-tips/client/evm",
  evmTesting: "@swarm-tips/client/evm/testing",
  inbox: "@swarm-tips/client/inbox",
  vow: "@swarm-tips/client/vow",
};
mkdirSync(resolve(fixture, "browser"));
for (const [name, specifier] of Object.entries(browserImports)) {
  const entry = resolve(fixture, "browser", `${name}.ts`);
  writeFileSync(entry, `export * from ${JSON.stringify(specifier)};\n`);
  const args = [
    entry,
    "--bundle",
    "--platform=browser",
    "--format=esm",
    `--outfile=${resolve(fixture, "browser", `${name}.js`)}`,
  ];
  run(resolve(fixture, "node_modules", ".bin", "esbuild"), args);
}

const installed = resolve(fixture, "node_modules", "@swarm-tips", "client");
JSON.parse(readFileSync(resolve(installed, "SBOM.spdx.json"), "utf8"));
const binPath = `${resolve(fixture, "node_modules", ".bin")}${delimiter}${
  process.env.PATH ?? ""
}`;
const help = spawnSync("vow-verify", ["--help"], {
  cwd: fixture,
  encoding: "utf8",
  env: { ...process.env, PATH: binPath },
});
if (help.status !== 2 || !help.stderr.includes("Usage: vow-verify"))
  throw new Error("vow-verify command contract failed");
const swarmTx = spawnSync(
  "swarm-tx",
  ["unknown", resolve(installed, "package.json")],
  { cwd: fixture, encoding: "utf8", env: { ...process.env, PATH: binPath } }
);
if (swarmTx.status !== 1 || !swarmTx.stderr.includes("usage: swarm-tx"))
  throw new Error("swarm-tx command contract failed");

console.log(`verified packed client in ${fixture}`);
