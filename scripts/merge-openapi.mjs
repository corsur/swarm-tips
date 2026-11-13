#!/usr/bin/env node
/**
 * Deterministically merges the three typed Rust `utoipa` OpenAPI 3.1 documents
 * (`mcp-server`, `shillbot-api`, `game-api`) into the canonical multi-host
 * Swarm Tips OpenAPI 3.1 specification (`services/mcp-server/openapi.merged.json`
 * and `../coordination-app/frontend/swarm-tips/public/.well-known/openapi.json`).
 *
 * Every path item and operation carries explicit per-host `servers`
 * (`https://mcp.swarm.tips`, `https://api.shillbot.org`,
 * `https://api.coordination.game`) so OpenAPI 3.1 SDK generators and HTTP
 * agents call the three certed hosts directly with unified `bearer_jwt` auth.
 */

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const swarmTipsRoot = resolve(__dirname, "..");
const defaultCoordRoot = resolve(swarmTipsRoot, "../coordination-app");

function parseArgs(argv) {
  const args = {};
  for (let i = 2; i < argv.length; i += 1) {
    const key = argv[i];
    if (key.startsWith("--")) {
      args[key.slice(2)] = argv[i + 1];
      i += 1;
    }
  }
  return args;
}

function deepClone(v) {
  return JSON.parse(JSON.stringify(v));
}

function sortDeep(value) {
  if (Array.isArray(value)) {
    return value.map(sortDeep);
  }
  if (value && typeof value === "object") {
    const out = {};
    for (const key of Object.keys(value).sort()) {
      out[key] = sortDeep(value[key]);
    }
    return out;
  }
  return value;
}

function rewriteRefsAndSecurity(node, schemaRenameMap) {
  if (Array.isArray(node)) {
    return node.map((item) => rewriteRefsAndSecurity(item, schemaRenameMap));
  }
  if (!node || typeof node !== "object") {
    return node;
  }
  const out = {};
  for (const [k, v] of Object.entries(node)) {
    if (k === "$ref" && typeof v === "string") {
      const prefix = "#/components/schemas/";
      if (v.startsWith(prefix)) {
        const name = v.slice(prefix.length);
        out[k] = `${prefix}${schemaRenameMap.get(name) ?? name}`;
        continue;
      }
    }
    if (k === "security" && Array.isArray(v)) {
      out[k] = v.map((secObj) => {
        if (!secObj || typeof secObj !== "object") return secObj;
        const mapped = {};
        for (const [scheme, scopes] of Object.entries(secObj)) {
          mapped[scheme === "bearerAuth" ? "bearer_jwt" : scheme] = scopes;
        }
        return mapped;
      });
      continue;
    }
    out[k] = rewriteRefsAndSecurity(v, schemaRenameMap);
  }
  return out;
}

const args = parseArgs(process.argv);
const mcpSpecPath = args.mcp;
const shillbotSpecPath = args.shillbot;
const gameSpecPath = args.game;

if (!mcpSpecPath || !shillbotSpecPath || !gameSpecPath) {
  console.error(
    "Usage: node scripts/merge-openapi.mjs --mcp <mcp.json> --shillbot <shillbot.json> --game <game.json>"
  );
  process.exit(1);
}

const sources = [
  {
    prefix: "Mcp",
    serverUrl: "https://mcp.swarm.tips",
    serverDescription: "Swarm Tips Discovery, Inbox, Topic Boards, A2A & Verify Builder",
    spec: JSON.parse(readFileSync(mcpSpecPath, "utf8")),
  },
  {
    prefix: "Shillbot",
    serverUrl: "https://api.shillbot.org",
    serverDescription: "Shillbot Non-Custodial Bounty, Campaign & Shorts Orchestrator",
    spec: JSON.parse(readFileSync(shillbotSpecPath, "utf8")),
  },
  {
    prefix: "Game",
    serverUrl: "https://api.coordination.game",
    serverDescription: "Coordination Game Auth, Solana/EVM & Cross-Chain Matchmaking API",
    spec: JSON.parse(readFileSync(gameSpecPath, "utf8")),
  },
];

const mergedSchemas = new Map();
const mergedPaths = {};
const mergedTags = new Map();
const seenOperationIds = new Set();

for (const src of sources) {
  const rawSchemas = src.spec.components?.schemas ?? {};
  const renameMap = new Map();

  for (const [name, schema] of Object.entries(rawSchemas)) {
    const normalizedCandidate = JSON.stringify(sortDeep(schema));
    if (mergedSchemas.has(name)) {
      const existing = JSON.stringify(sortDeep(mergedSchemas.get(name)));
      if (existing !== normalizedCandidate) {
        const prefixed = `${src.prefix}${name}`;
        renameMap.set(name, prefixed);
      }
    }
  }

  for (const [name, schema] of Object.entries(rawSchemas)) {
    const finalName = renameMap.get(name) ?? name;
    const rewritten = rewriteRefsAndSecurity(deepClone(schema), renameMap);
    mergedSchemas.set(finalName, rewritten);
  }

  for (const tag of src.spec.tags ?? []) {
    if (tag?.name && !mergedTags.has(tag.name)) {
      mergedTags.set(tag.name, deepClone(tag));
    }
  }

  const paths = src.spec.paths ?? {};
  for (const [pathKey, rawItem] of Object.entries(paths)) {
    if (mergedPaths[pathKey]) {
      throw new Error(
        `Path collision across hosts on ${pathKey} (${src.serverUrl}); disambiguate before merging`
      );
    }
    const item = rewriteRefsAndSecurity(deepClone(rawItem), renameMap);
    item.servers = [
      {
        url: src.serverUrl,
        description: src.serverDescription,
      },
    ];
    for (const method of ["get", "post", "put", "patch", "delete", "options", "head"]) {
      if (item[method] && typeof item[method] === "object") {
        const op = item[method];
        if (!op.operationId) {
          throw new Error(`Missing operationId on ${method.toUpperCase()} ${pathKey}`);
        }
        if (seenOperationIds.has(op.operationId)) {
          throw new Error(`Duplicate operationId across services: ${op.operationId}`);
        }
        seenOperationIds.add(op.operationId);
        op.servers = [
          {
            url: src.serverUrl,
            description: src.serverDescription,
          },
        ];
      }
    }
    mergedPaths[pathKey] = item;
  }
}

const sortedPaths = {};
for (const pathKey of Object.keys(mergedPaths).sort()) {
  sortedPaths[pathKey] = mergedPaths[pathKey];
}

const sortedSchemas = {};
for (const schemaKey of [...mergedSchemas.keys()].sort()) {
  sortedSchemas[schemaKey] = sortDeep(mergedSchemas.get(schemaKey));
}

const sdkPkgPath = resolve(swarmTipsRoot, "sdk/client/package.json");
const sdkVersion = JSON.parse(readFileSync(sdkPkgPath, "utf8")).version;

const mergedDoc = {
  openapi: "3.1.0",
  info: {
    title: "Swarm Tips Unified Agent API",
    version: sdkVersion,
    description:
      "Canonical OpenAPI 3.1 specification spanning all three Swarm Tips production API hosts (`https://mcp.swarm.tips`, `https://api.shillbot.org`, `https://api.coordination.game`) with unified stateless wallet Bearer JWT authentication and non-custodial transaction intent inspection.",
    contact: {
      name: "Swarm Tips",
      url: "https://swarm.tips/developers",
    },
  },
  servers: sources.map((s) => ({
    url: s.serverUrl,
    description: s.serverDescription,
  })),
  tags: [...mergedTags.values()],
  paths: sortedPaths,
  components: {
    securitySchemes: {
      bearer_jwt: {
        type: "http",
        scheme: "bearer",
        bearerFormat: "JWT",
        description:
          "Unified stateless wallet JWT minted via POST https://api.coordination.game/auth/challenge + /auth/verify (Solana Ed25519) or /auth/evm/challenge + /auth/evm/verify (EVM EIP-191). Accepted across api.shillbot.org, api.coordination.game, and mcp.swarm.tips.",
      },
      inbox_session: {
        type: "apiKey",
        in: "header",
        name: "X-Inbox-Session",
        description:
          "Optional opaque inbox session header from POST https://mcp.swarm.tips/internal/inbox/session. Never send Authorization and X-Inbox-Session on the same request (returns 400 conflicting_auth_headers).",
      },
    },
    schemas: sortedSchemas,
  },
};

const formatted = `${JSON.stringify(mergedDoc, null, 2)}\n`;

const mcpOutPath = resolve(swarmTipsRoot, "services/mcp-server/openapi.merged.json");
writeFileSync(mcpOutPath, formatted);
console.log(
  `wrote ${mcpOutPath} (${Object.keys(sortedPaths).length} paths, ${seenOperationIds.size} operations, ${Object.keys(sortedSchemas).length} schemas)`
);

const coordOutDir = resolve(defaultCoordRoot, "frontend/swarm-tips/public/.well-known");
if (existsSync(resolve(defaultCoordRoot, "frontend/swarm-tips"))) {
  mkdirSync(coordOutDir, { recursive: true });
  const coordOutPath = resolve(coordOutDir, "openapi.json");
  writeFileSync(coordOutPath, formatted);
  console.log(`wrote ${coordOutPath}`);
}
