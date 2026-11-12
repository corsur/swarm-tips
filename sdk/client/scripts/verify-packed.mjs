import { mkdtempSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, resolve } from "node:path";
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
  "@swarm-tips/client/shillbot/api",
  "@swarm-tips/client/coordination-game",
  "@swarm-tips/client/coordination-game/solana",
  "@swarm-tips/client/coordination-game/evm",
  "@swarm-tips/client/coordination-game/api",
  "@swarm-tips/client/evm",
  "@swarm-tips/client/evm/testing",
  "@swarm-tips/client/inbox",
  "@swarm-tips/client/vow",
  "@swarm-tips/client/swarm",
];
run("node", [
  "--input-type=module",
  "--eval",
  `await Promise.all(${JSON.stringify(
    imports
  )}.map((specifier) => import(specifier)));
const root = await import("@swarm-tips/client");
if (typeof root.SwarmAgent !== "function" || typeof root.createSwarmAgent !== "function") {
  throw new Error("SwarmAgent missing from root export");
}
await import("@swarm-tips/client/idl/shillbot", { with: { type: "json" } });
await import("@swarm-tips/client/idl/coordination-game", { with: { type: "json" } });`,
]);
run("node", [
  "--eval",
  `for (const specifier of ${JSON.stringify(imports)}) require(specifier);`,
]);

const browserImports = {
  root: "@swarm-tips/client",
  shillbot: "@swarm-tips/client/shillbot",
  coordination: "@swarm-tips/client/coordination-game",
  coordinationSolana: "@swarm-tips/client/coordination-game/solana",
  coordinationEvm: "@swarm-tips/client/coordination-game/evm",
  coordinationApi: "@swarm-tips/client/coordination-game/api",
  evm: "@swarm-tips/client/evm",
  evmTesting: "@swarm-tips/client/evm/testing",
  inbox: "@swarm-tips/client/inbox",
  vow: "@swarm-tips/client/vow",
  shillbotApi: "@swarm-tips/client/shillbot/api",
  swarm: "@swarm-tips/client/swarm",
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
    "--define:Buffer=undefined",
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

for (const binName of ["swarm", "client"]) {
  const swarmHelp = spawnSync(binName, ["--help"], {
    cwd: fixture,
    encoding: "utf8",
    env: { ...process.env, PATH: binPath },
  });
  if (swarmHelp.status !== 0 || !swarmHelp.stdout.includes("Usage:"))
    throw new Error(`${binName} --help command contract failed`);
}

const activeHandleRes = spawnSync(
  process.execPath,
  [
    "--import",
    "data:text/javascript,setInterval(()=>{},60000);",
    resolve(installed, "dist", "cli", "swarm.js"),
    "--version",
    "--json",
  ],
  { cwd: fixture, encoding: "utf8", timeout: 4000 }
);
if (activeHandleRes.error || activeHandleRes.status !== 0) {
  throw new Error(
    `packed swarm CLI hung or failed with active handle: status=${activeHandleRes.status} err=${activeHandleRes.error}`
  );
}

const builtClaimJson = run("node", [
  "--input-type=module",
  "--eval",
  `const { buildClaim } = await import("@swarm-tips/client/shillbot");
const wallet = "11111111111111111111111111111111";
const taskPda = "SysvarRent111111111111111111111111111111111";
const built = buildClaim({
  wallet,
  network: "mainnet",
  recentBlockhash: "11111111111111111111111111111111",
  taskPda,
});
process.stdout.write(JSON.stringify({ wallet, taskPda, tx: built.unsigned_tx }));`,
]).stdout;
const { wallet, taskPda, tx } = JSON.parse(builtClaimJson);
const verifyRes = spawnSync(
  "swarm",
  [
    "tx",
    "verify",
    "--action",
    "claim",
    "--wallet",
    wallet,
    "--task-pda",
    taskPda,
    "--json",
  ],
  {
    cwd: fixture,
    input: tx,
    encoding: "utf8",
    env: { ...process.env, PATH: binPath },
  }
);
if (verifyRes.status !== 0 || !JSON.parse(verifyRes.stdout).ok) {
  throw new Error(`swarm tx verify contract failed: ${verifyRes.stdout} ${verifyRes.stderr}`);
}

console.log(`verified packed client in ${fixture}`);
