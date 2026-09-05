import { spawnSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repositoryRoot = resolve(packageRoot, "../..");
const packageJson = JSON.parse(
  readFileSync(resolve(packageRoot, "package.json"), "utf8")
);
const result = spawnSync(
  "pnpm",
  ["licenses", "list", "--filter", packageJson.name, "--prod", "--json"],
  { cwd: repositoryRoot, encoding: "utf8", maxBuffer: 32 * 1024 * 1024 }
);
if (result.status !== 0) {
  process.stderr.write(result.stderr || result.stdout);
  process.exit(result.status ?? 1);
}

const licenses = JSON.parse(result.stdout);
const dependencies = [];
const seen = new Set();
for (const [license, entries] of Object.entries(licenses)) {
  for (const entry of entries) {
    for (const version of entry.versions) {
      const key = `${entry.name}@${version}`;
      if (seen.has(key)) continue;
      seen.add(key);
      dependencies.push({ name: entry.name, version, license });
    }
  }
}
dependencies.sort((a, b) =>
  `${a.name}@${a.version}`.localeCompare(`${b.name}@${b.version}`)
);

const dependencyPackages = dependencies.map((dependency) => ({
  name: dependency.name,
  SPDXID: `SPDXRef-Dependency-${createHash("sha256")
    .update(`${dependency.name}@${dependency.version}`)
    .digest("hex")
    .slice(0, 16)}`,
  versionInfo: dependency.version,
  downloadLocation: "NOASSERTION",
  filesAnalyzed: false,
  licenseConcluded:
    dependency.license === "Unknown" ? "NOASSERTION" : dependency.license,
  licenseDeclared:
    dependency.license === "Unknown" ? "NOASSERTION" : dependency.license,
}));

const packageSpdxId = "SPDXRef-Package";
const document = {
  spdxVersion: "SPDX-2.3",
  dataLicense: "CC0-1.0",
  SPDXID: "SPDXRef-DOCUMENT",
  name: `${packageJson.name}-${packageJson.version}`,
  documentNamespace: `https://swarm.tips/sbom/client/${packageJson.version}`,
  creationInfo: {
    created: new Date().toISOString(),
    creators: [
      "Tool: sdk/client/scripts/generate-sbom.mjs",
      "Organization: Swarm Tips",
    ],
  },
  packages: [
    {
      name: packageJson.name,
      SPDXID: packageSpdxId,
      versionInfo: packageJson.version,
      downloadLocation: `https://www.npmjs.com/package/${packageJson.name}/v/${packageJson.version}`,
      filesAnalyzed: false,
      licenseConcluded: packageJson.license,
      licenseDeclared: packageJson.license,
    },
    ...dependencyPackages,
  ],
  relationships: [
    {
      spdxElementId: "SPDXRef-DOCUMENT",
      relationshipType: "DESCRIBES",
      relatedSpdxElement: packageSpdxId,
    },
    ...dependencyPackages.map((dependency) => ({
      spdxElementId: packageSpdxId,
      relationshipType: "DEPENDS_ON",
      relatedSpdxElement: dependency.SPDXID,
    })),
  ],
};

writeFileSync(
  resolve(packageRoot, "SBOM.spdx.json"),
  `${JSON.stringify(document, null, 2)}\n`
);
console.log(
  `generated SPDX SBOM with ${dependencyPackages.length} resolved dependencies`
);
