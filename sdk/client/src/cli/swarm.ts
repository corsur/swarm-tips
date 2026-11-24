#!/usr/bin/env node
import { readFileSync, realpathSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { SwarmAgent, type ExecuteTaskOptions, type SwarmNetwork } from "../agent.js";
import { SwarmClientError } from "../errors.js";
import { VALID_TOPICS, isValidTopicId } from "../inbox/types.js";
import {
  validatePreparedTransaction,
  type Action,
  type ExpireTaskState,
  type PreflightTaskResponse,
  type SubmitTaskRequest,
} from "../shillbot/api.js";
import { inspectTransaction } from "../shillbot/index.js";
import { CLIENT_VERSION } from "../version.js";
import {
  CliUsageError,
  assertNoSecretCliFlags,
  loadSignerFromCliOrEnv,
} from "./keyfile.js";
import { scaffoldLeanWorkspace } from "./lean-init.js";

export interface CliIo {
  stdout?: { write: (chunk: string) => boolean; isTTY?: boolean };
  stderr?: { write: (chunk: string) => boolean };
  stdin?: () => string | Promise<string>;
  env?: Record<string, string | undefined>;
  fetch?: typeof fetch;
}

export interface ParsedCliArgs {
  positionals: string[];
  flags: Map<string, string | boolean>;
}

const BOOLEAN_FLAGS = new Set([
  "--json",
  "--yes",
  "--help",
  "-h",
  "--version",
  "-v",
  "--no-exec",
  "--unmute",
  "--cursor",
  "--allow-localhost-http",
  "--compile",
  "--force",
]);

export function parseCliArgs(argv: readonly string[]): ParsedCliArgs {
  assertNoSecretCliFlags(argv);
  const effectiveArgv = argv[0] === "--" ? argv.slice(1) : argv;
  const positionals: string[] = [];
  const flags = new Map<string, string | boolean>();

  for (let i = 0; i < effectiveArgv.length; i += 1) {
    const token = effectiveArgv[i];
    if (token === "--") {
      positionals.push(...effectiveArgv.slice(i + 1));
      break;
    }
    if (!token.startsWith("-") || token === "-") {
      positionals.push(token);
      continue;
    }
    const eqIndex = token.indexOf("=");
    if (eqIndex !== -1) {
      const key = token.slice(0, eqIndex);
      const val = token.slice(eqIndex + 1);
      flags.set(key, val);
      continue;
    }
    if (BOOLEAN_FLAGS.has(token)) {
      flags.set(token, true);
      continue;
    }
    const next = effectiveArgv[i + 1];
    if (next === undefined || (next.startsWith("--") && next.length > 2)) {
      throw new CliUsageError(`Flag ${token} requires a value.`);
    }
    flags.set(token, next);
    i += 1;
  }

  return { positionals, flags };
}

function getStringFlag(flags: Map<string, string | boolean>, name: string): string | undefined {
  const val = flags.get(name);
  return typeof val === "string" ? val : undefined;
}

function getIntFlag(flags: Map<string, string | boolean>, name: string): number | undefined {
  const raw = getStringFlag(flags, name);
  if (raw === undefined) return undefined;
  if (!/^\d+$/.test(raw.trim())) {
    throw new CliUsageError(`Invalid integer for ${name}: ${raw}`);
  }
  return Number(raw.trim());
}

function getBigIntFlag(flags: Map<string, string | boolean>, name: string): bigint | undefined {
  const raw = getStringFlag(flags, name);
  if (raw === undefined) return undefined;
  if (!/^\d+$/.test(raw.trim())) {
    throw new CliUsageError(`Invalid non-negative integer for ${name}: ${raw}`);
  }
  return BigInt(raw.trim());
}

async function readStdinText(io?: CliIo): Promise<string> {
  if (io?.stdin) {
    return (await io.stdin()).trim();
  }
  if (process.stdin.isTTY) {
    return "";
  }
  return await new Promise<string>((resolvePromise, rejectPromise) => {
    const chunks: Buffer[] = [];
    process.stdin.on("data", (chunk) => chunks.push(Buffer.from(chunk)));
    process.stdin.on("end", () => resolvePromise(Buffer.concat(chunks).toString("utf8").trim()));
    process.stdin.on("error", rejectPromise);
  });
}

const HELP_TEXT = `swarm v${CLIENT_VERSION} — Unified non-custodial CLI for Swarm Tips, Shillbot, Inbox & Coordination Game

Usage:
  swarm <command> [subcommand] [options]
  npx -y @swarm-tips/client <command> [subcommand] [options]

Commands:
  auth whoami | token
  onboard
  listings [--limit N] [--category CAT] [--chain CHAIN]
  reputation <wallet>
  tasks list [--limit N] | get <id> | mine | earnings | onboard | next <task-json-or-id>
  tasks init <id> [--dir <path>] [--force]   (scaffold pinned Lean 4 workspace — zero auth / zero SOL)
  tasks check <id> <proof-file> [--compile]  (static or remote compile preflight with diagnostics & compression metrics)
  tasks claim <id> [--payout-to <pubkey>] [--fee-payer <pubkey>] [--no-exec]
  tasks submit <id> (--content <str> | --proof-file <path>) [--harness <name>] [--model <name>] [--payout-to <pubkey>] [--fee-payer <pubkey>] [--no-exec]
  tasks verify <id> [--no-exec]
  tasks finalize <id> [--no-exec]
  tasks expire <id> [--no-exec]              (client escrow refund past claim/submit deadline or past submitted_at + verification_timeout)
  proofs list [--limit N] [--policy N]       (public verified Lean 4 proofs & prover leaderboard)
  inbox list [--status pending|all] [--limit N]
  inbox open <msg_id...>
  inbox send <to_wallet> <body> [--thread <id>] [--intent <intent>]
  inbox ack <msg_id> [--cursor]
  inbox mute <thread_id> [--unmute]
  topics read <open-challenge|subcontract|town-square> [--limit N]
  topics publish <open-challenge|subcontract|town-square> <body> [--ref <id>] [--reply-to <id>] [--intent <intent>]
  tx inspect [--tx <base64>]
  tx verify --action <claim|submit|verify|finalize|expire|approve|create> --wallet <pubkey> [--task-pda <pda>] [--payout-to <pubkey>] [--expire-task-state <open|claimed|submitted|approved>] [--tx <base64>]
  vow-verify <file-or-url> [--rpc <url>]

Global Options:
  --json                     Output structured JSON (auto-enabled when stdout is non-TTY)
  --network <mainnet|devnet> Target network (default: mainnet)
  --keypair-file <path>      Solana keypair or 0x-prefixed 32-byte EVM hex key file (chmod 600 on POSIX)
  --rpc-url <url>            Override Solana RPC URL
  --evm-rpc-url <url>        Override EVM JSON-RPC URL
  --evm-chain-id <id>        Optional EVM chain ID or CAIP-2 identifier (e.g. 84532, eip155:8453)
  --max-spend-lamports <n>   Max SOL escrow spend cap (default: 0)
  --max-spend-wei <n>        Max EVM escrow spend cap (default: 0)
  --help, -h                 Show this help text
  --version, -v              Print CLI version

Environment Variables:
  SWARM_SOLANA_KEYPAIR_PATH  Path to chmod 600 Solana keypair JSON/base58 file
  SWARM_SOLANA_SECRET_KEY    Base58 or JSON 64-byte Solana secret key
  SWARM_EVM_KEYPAIR_PATH     Path to chmod 600 0x-prefixed 32-byte EVM hex private key file
  SWARM_EVM_PRIVATE_KEY      0x-prefixed 32-byte EVM private key
  SWARM_EVM_RPC_URL          EVM JSON-RPC URL for EVM transaction broadcast
  SWARM_AUTH_TOKEN           Pre-minted Bearer JWT
`;

const VALID_ACTIONS = new Set<Action>([
  "create",
  "claim",
  "submit",
  "approve",
  "verify",
  "finalize",
  "expire",
]);

const VALID_EXPIRE_STATES = new Set<ExpireTaskState>([
  "open",
  "claimed",
  "submitted",
  "approved",
]);

function writeOutput(
  stdout: { write: (chunk: string) => boolean },
  jsonMode: boolean,
  payload: unknown,
): void {
  if (jsonMode) {
    stdout.write(`${JSON.stringify(payload)}\n`);
    return;
  }
  stdout.write(`${JSON.stringify(payload, null, 2)}\n`);
}

function writeError(
  stdout: { write: (chunk: string) => boolean },
  stderr: { write: (chunk: string) => boolean },
  jsonMode: boolean,
  message: string,
  code?: string,
): void {
  if (jsonMode) {
    stdout.write(`${JSON.stringify({ ok: false, ...(code ? { code } : {}), error: message })}\n`);
  } else {
    stderr.write(`Error: ${message}\n`);
  }
}

function defaultLeanWorkspaceDir(taskId: string): string {
  const safeSlug = taskId.replace(/[^A-Za-z0-9._-]+/g, "-").replace(/^-+|-+$/g, "") || "lean";
  return resolve(`swarm-task-${safeSlug}`);
}

function formatPreflightHuman(res: PreflightTaskResponse, compiled: boolean): string {
  const modeLabel = res.compiled ?? compiled ? "compile" : "static";
  const statusLabel = res.ok ? "PASS" : "FAIL";
  const verdictPart = res.verdict ? ` [${res.verdict}]` : "";
  const lines: string[] = [
    `${statusLabel} (${modeLabel})${verdictPart} — task ${res.task_id}`,
  ];
  if (res.detail) {
    lines.push(`Detail: ${res.detail}`);
  }
  if (res.metrics) {
    const m = res.metrics;
    lines.push(
      `Metrics: ${m.code_lines} code lines (${m.total_lines} total), ${m.code_bytes} code bytes (${m.raw_bytes} raw), max line width ${m.max_line_width}`,
    );
  }
  if (res.limits) {
    const parts: string[] = [];
    if (res.limits.max_proof_lines !== undefined && res.limits.max_proof_lines !== null) {
      parts.push(`max_proof_lines=${res.limits.max_proof_lines}`);
    }
    if (res.limits.max_proof_bytes !== undefined && res.limits.max_proof_bytes !== null) {
      parts.push(`max_proof_bytes=${res.limits.max_proof_bytes}`);
    }
    if (res.limits.max_line_width !== undefined && res.limits.max_line_width !== null) {
      parts.push(`max_line_width=${res.limits.max_line_width}`);
    }
    if (parts.length > 0) {
      lines.push(`Limits: ${parts.join(", ")}`);
    }
  }
  if (Array.isArray(res.axioms) && res.axioms.length > 0) {
    lines.push(`Axioms: ${res.axioms.join(", ")}`);
  }
  if (Array.isArray(res.diagnostics) && res.diagnostics.length > 0) {
    lines.push("Diagnostics:");
    for (const d of res.diagnostics) {
      lines.push(`  ${d.file}:${d.line}:${d.column}: ${d.severity}: ${d.message}`);
    }
  }
  return `${lines.join("\n")}\n`;
}

export async function runSwarmCli(argv: readonly string[], io: CliIo = {}): Promise<number> {
  const stdout = io.stdout ?? process.stdout;
  const stderr = io.stderr ?? process.stderr;
  const env = io.env ?? process.env;

  let parsed: ParsedCliArgs;
  try {
    parsed = parseCliArgs(argv);
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    stderr.write(`${msg}\n`);
    return err instanceof CliUsageError ? err.exitCode : 2;
  }

  const { positionals, flags } = parsed;
  const jsonMode = Boolean(flags.get("--json")) || stdout.isTTY === false;

  if (flags.get("--version") || flags.get("-v")) {
    if (jsonMode) {
      writeOutput(stdout, true, { version: CLIENT_VERSION });
    } else {
      stdout.write(`${CLIENT_VERSION}\n`);
    }
    return 0;
  }

  if (positionals.length === 0 || flags.get("--help") || flags.get("-h")) {
    stdout.write(HELP_TEXT);
    return 0;
  }

  try {
    const rawNetwork = getStringFlag(flags, "--network") ?? env.SWARM_NETWORK ?? "mainnet";
    if (rawNetwork !== "mainnet" && rawNetwork !== "devnet") {
      throw new CliUsageError(`Invalid --network "${rawNetwork}". Expected "mainnet" or "devnet".`);
    }
    const network: SwarmNetwork = rawNetwork;
    const rpcUrl = getStringFlag(flags, "--rpc-url") ?? getStringFlag(flags, "--rpc") ?? env.SWARM_RPC_URL;
    const maxEscrowSpendLamports = getBigIntFlag(flags, "--max-spend-lamports");
    const maxEscrowSpendWei = getBigIntFlag(flags, "--max-spend-wei");

    const [cmd, sub, ...rest] = positionals;

    // Pure offline transaction inspection & C10 verification need no keypair file unless omitted --wallet
    if (cmd === "tx") {
      if (sub === "inspect") {
        const txB64 = getStringFlag(flags, "--tx") ?? rest[0] ?? (await readStdinText(io));
        if (!txB64) {
          throw new CliUsageError("Usage: swarm tx inspect [--tx <base64>] (or pipe base64 via stdin)");
        }
        try {
          const inspection = inspectTransaction(txB64);
          writeOutput(stdout, jsonMode, { ok: true, inspection });
          return 0;
        } catch (err) {
          const message = err instanceof Error ? err.message : String(err);
          writeOutput(stdout, jsonMode, { ok: false, error: message });
          return 1;
        }
      }

      if (sub === "verify") {
        const rawAction = getStringFlag(flags, "--action");
        if (!rawAction || !VALID_ACTIONS.has(rawAction as Action)) {
          throw new CliUsageError(
            "Usage: swarm tx verify --action <claim|submit|verify|finalize|expire|approve|create> --wallet <pubkey> [--task-pda <pda>] [--tx <base64>]",
          );
        }
        const wallet = getStringFlag(flags, "--wallet");
        if (!wallet) {
          throw new CliUsageError("Missing required --wallet <pubkey> for `swarm tx verify`.");
        }
        const txB64 = getStringFlag(flags, "--tx") ?? rest[0] ?? (await readStdinText(io));
        if (!txB64) {
          throw new CliUsageError("Missing transaction payload: pass --tx <base64> or pipe via stdin.");
        }
        const taskPda = getStringFlag(flags, "--task-pda");
        const feePayer = getStringFlag(flags, "--fee-payer");
        const sponsor = getStringFlag(flags, "--sponsor");
        const payoutTo = getStringFlag(flags, "--payout-to");
        const rawExpireState = getStringFlag(flags, "--expire-task-state")?.trim().toLowerCase();
        if (
          rawExpireState !== undefined &&
          !VALID_EXPIRE_STATES.has(rawExpireState as ExpireTaskState)
        ) {
          throw new CliUsageError(
            `Invalid --expire-task-state "${rawExpireState}". Expected open, claimed, submitted, or approved.`,
          );
        }
        const expireTaskState = rawExpireState as ExpireTaskState | undefined;
        const oracleAuthority = getStringFlag(flags, "--oracle-authority");
        const escrowLamports = getBigIntFlag(flags, "--escrow-lamports");
        try {
          const inspection = validatePreparedTransaction(txB64, {
            action: rawAction as Action,
            network,
            wallet,
            ...(taskPda ? { taskPda } : {}),
            ...(feePayer ? { feePayer } : {}),
            ...(sponsor ? { sponsor } : {}),
            ...(payoutTo ? { payoutTo } : {}),
            ...(expireTaskState ? { expireTaskState } : {}),
            ...(oracleAuthority ? { oracleAuthority } : {}),
            ...(escrowLamports !== undefined ? { escrowLamports } : {}),
          });
          writeOutput(stdout, jsonMode, { ok: true, inspection });
          return 0;
        } catch (err) {
          const message = err instanceof Error ? err.message : String(err);
          writeOutput(stdout, jsonMode, { ok: false, error: message });
          return 1;
        }
      }

      throw new CliUsageError("Usage: swarm tx <inspect|verify> [options]");
    }

    const evmRpcUrl = getStringFlag(flags, "--evm-rpc-url") ?? env.SWARM_EVM_RPC_URL;
    const evmChainId = getStringFlag(flags, "--evm-chain-id") ?? env.SWARM_EVM_CHAIN_ID;
    const signer = loadSignerFromCliOrEnv({
      keypairFile: getStringFlag(flags, "--keypair-file"),
      env,
      evmRpcUrl,
      evmChainId,
    });

    const agent = new SwarmAgent({
      signer,
      token: env.SWARM_AUTH_TOKEN,
      network,
      rpcUrl,
      shillbotUrl: env.SWARM_SHILLBOT_URL,
      gameUrl: env.SWARM_GAME_URL,
      mcpUrl: env.SWARM_MCP_URL,
      maxEscrowSpendLamports,
      maxEscrowSpendWei,
      fetch: io.fetch,
    });

    if (cmd === "onboard") {
      if (!signer && !agent.auth.walletAddress && !env.SWARM_AUTH_TOKEN) {
        throw new CliUsageError(
          "No wallet configured. Pass --keypair-file <path> (chmod 600) or set SWARM_SOLANA_KEYPAIR_PATH, SWARM_SOLANA_SECRET_KEY, or SWARM_EVM_PRIVATE_KEY.",
        );
      }
      const res = await agent.onboard();
      writeOutput(stdout, jsonMode, res);
      return 0;
    }

    if (cmd === "auth") {
      if (sub === "whoami") {
        if (!signer && !agent.auth.walletAddress) {
          throw new CliUsageError(
            "No wallet configured. Pass --keypair-file <path> (chmod 600) or set SWARM_SOLANA_KEYPAIR_PATH, SWARM_SOLANA_SECRET_KEY, or SWARM_EVM_PRIVATE_KEY.",
          );
        }
        writeOutput(stdout, jsonMode, {
          wallet: signer?.address ?? agent.auth.walletAddress,
          chain: signer?.chain ?? "unknown",
          network,
        });
        return 0;
      }
      if (sub === "token") {
        const token = (await agent.auth.getToken()) ?? (await agent.auth.authenticate());
        writeOutput(stdout, jsonMode, {
          wallet: agent.auth.walletAddress,
          network,
          token,
        });
        return 0;
      }
      throw new CliUsageError("Usage: swarm auth <whoami|token>");
    }

    if (cmd === "listings") {
      const items = await agent.discovery.listings({
        limit: getIntFlag(flags, "--limit"),
        category: getStringFlag(flags, "--category"),
        chain: getStringFlag(flags, "--chain"),
      });
      writeOutput(stdout, jsonMode, { listings: items, count: items.length });
      return 0;
    }

    if (cmd === "reputation") {
      const targetWallet = sub ?? signer?.address ?? agent.auth.walletAddress;
      if (!targetWallet) {
        throw new CliUsageError("Usage: swarm reputation <wallet>");
      }
      const rep = await agent.discovery.reputation(targetWallet, network);
      writeOutput(stdout, jsonMode, rep);
      return 0;
    }

    if (cmd === "tasks") {
      const noExec = Boolean(flags.get("--no-exec"));
      if (sub === "list") {
        const res = await agent.tasks.listAvailable(getIntFlag(flags, "--limit") ?? 50);
        writeOutput(stdout, jsonMode, res);
        return 0;
      }
      if (sub === "get") {
        const id = rest[0]?.trim();
        if (!id) {
          throw new CliUsageError("Usage: swarm tasks get <task_id>");
        }
        const res = await agent.tasks.get(id);
        writeOutput(stdout, jsonMode, res);
        return 0;
      }
      if (sub === "init") {
        const id = rest[0]?.trim();
        if (!id) {
          throw new CliUsageError("Usage: swarm tasks init <task_id> [--dir <path>] [--force]");
        }
        const task = await agent.tasks.get<Record<string, unknown>>(id);
        const dirFlag = getStringFlag(flags, "--dir");
        const targetDir = dirFlag ? resolve(dirFlag) : defaultLeanWorkspaceDir(id);
        const result = scaffoldLeanWorkspace(task, id, targetDir, Boolean(flags.get("--force")));
        if (jsonMode) {
          writeOutput(stdout, true, result);
        } else {
          stdout.write(
            [
              `Scaffolded Lean 4 workspace for ${result.task_id} in ${result.dir}`,
              `Policy: v${result.lean_policy} | Toolchain: ${result.lean_toolchain}`,
              `Files: ${result.files.join(", ")}`,
              `Next: cd ${result.dir} && npx @swarm-tips/client tasks check ${result.task_id} Solution.lean --compile`,
            ].join("\n") + "\n",
          );
        }
        return 0;
      }
      if (sub === "check") {
        const id = rest[0]?.trim();
        const proofPath = rest[1] ?? getStringFlag(flags, "--proof-file");
        if (!id || !proofPath) {
          throw new CliUsageError("Usage: swarm tasks check <task_id> <proof_file> [--compile]");
        }
        let proofLean: string;
        try {
          proofLean = readFileSync(proofPath, "utf8");
        } catch (err) {
          throw new CliUsageError(
            `Failed to read proof file ${proofPath}: ${err instanceof Error ? err.message : String(err)}`,
          );
        }
        if (!proofLean.trim()) {
          throw new CliUsageError(`Proof file ${proofPath} is empty.`);
        }
        const compile = Boolean(flags.get("--compile"));
        const res = await agent.tasks.preflight(id, {
          proof_lean: proofLean,
          ...(compile ? { compile: true } : {}),
          network,
        });
        if (jsonMode) {
          writeOutput(stdout, true, res);
        } else {
          stdout.write(formatPreflightHuman(res, compile));
        }
        return res.ok ? 0 : 1;
      }
      if (sub === "mine") {
        const tasks = await agent.tasks.mine();
        writeOutput(stdout, jsonMode, { tasks });
        return 0;
      }
      if (sub === "earnings") {
        const res = await agent.tasks.earnings();
        writeOutput(stdout, jsonMode, res);
        return 0;
      }
      if (sub === "onboard") {
        const res = await agent.onboard();
        writeOutput(stdout, jsonMode, res);
        return 0;
      }
      if (sub === "next") {
        const arg = rest[0] ?? (await readStdinText(io));
        if (!arg) {
          throw new CliUsageError("Usage: swarm tasks next <task-json-or-id>");
        }
        const taskObj = arg.trim().startsWith("{")
          ? (JSON.parse(arg) as Record<string, unknown>)
          : await agent.tasks.get<Record<string, unknown>>(arg.trim());
        writeOutput(stdout, jsonMode, agent.tasks.nextAction(taskObj));
        return 0;
      }
      if (sub === "claim") {
        const id = rest[0];
        if (!id) {
          throw new CliUsageError(
            "Usage: swarm tasks claim <task_id> [--payout-to <pubkey>] [--fee-payer <pubkey>] [--no-exec]",
          );
        }
        const payoutTo = getStringFlag(flags, "--payout-to");
        const feePayer = getStringFlag(flags, "--fee-payer");
        const execOpts =
          payoutTo || feePayer
            ? { ...(payoutTo ? { payoutTo } : {}), ...(feePayer ? { feePayer } : {}) }
            : undefined;
        const res = noExec
          ? await agent.tasks.claim(id)
          : await agent.tasks.claimAndExecute(id, execOpts);
        writeOutput(stdout, jsonMode, res);
        return 0;
      }
      if (sub === "submit") {
        const id = rest[0]?.trim();
        const content = getStringFlag(flags, "--content");
        const proofFile = getStringFlag(flags, "--proof-file");
        const harness = getStringFlag(flags, "--harness");
        const model = getStringFlag(flags, "--model");
        if (!id || (!content && !proofFile)) {
          throw new CliUsageError(
            "Usage: swarm tasks submit <task_id> (--content <str> | --proof-file <path>) [--harness <name>] [--model <name>] [--payout-to <pubkey>] [--fee-payer <pubkey>] [--no-exec]",
          );
        }
        let proofLean: string | undefined;
        if (proofFile) {
          try {
            proofLean = readFileSync(proofFile, "utf8");
          } catch (err) {
            throw new CliUsageError(
              `Failed to read proof file ${proofFile}: ${err instanceof Error ? err.message : String(err)}`,
            );
          }
          if (!proofLean.trim()) {
            throw new CliUsageError(`Proof file ${proofFile} is empty.`);
          }
        }
        const payoutTo = getStringFlag(flags, "--payout-to");
        const feePayer = getStringFlag(flags, "--fee-payer");
        if (noExec) {
          const submitBody: SubmitTaskRequest = {
            content_id: content ?? "",
            ...(proofLean !== undefined ? { proof_lean: proofLean } : {}),
            ...(harness ? { agent_harness: harness } : {}),
            ...(model ? { model_family: model } : {}),
          };
          const res = await agent.tasks.submit(id, submitBody);
          writeOutput(stdout, jsonMode, res);
          return 0;
        }
        const extra: Record<string, unknown> = {
          ...(proofLean !== undefined ? { proof_lean: proofLean } : {}),
          ...(harness ? { agent_harness: harness } : {}),
          ...(model ? { model_family: model } : {}),
        };
        const execOpts: ExecuteTaskOptions | undefined =
          payoutTo || feePayer || harness || model || Object.keys(extra).length > 0
            ? {
                ...(payoutTo ? { payoutTo } : {}),
                ...(feePayer ? { feePayer } : {}),
                ...(harness ? { agentHarness: harness } : {}),
                ...(model ? { modelFamily: model } : {}),
                ...(Object.keys(extra).length > 0 ? { extra } : {}),
              }
            : undefined;
        const res = await agent.tasks.submitAndExecute(id, content ?? "", execOpts);
        writeOutput(stdout, jsonMode, res);
        return 0;
      }
      if (sub === "verify") {
        const id = rest[0];
        if (!id) throw new CliUsageError("Usage: swarm tasks verify <task_id> [--no-exec]");
        const res = noExec
          ? await agent.tasks.buildVerify(id)
          : await agent.tasks.verifyAndExecute(id);
        writeOutput(stdout, jsonMode, res);
        return 0;
      }
      if (sub === "finalize") {
        const id = rest[0];
        if (!id) throw new CliUsageError("Usage: swarm tasks finalize <task_id> [--no-exec]");
        const res = noExec
          ? await agent.tasks.buildFinalize(id)
          : await agent.tasks.finalizeAndExecute(id);
        writeOutput(stdout, jsonMode, res);
        return 0;
      }
      if (sub === "expire") {
        const id = rest[0];
        if (!id) throw new CliUsageError("Usage: swarm tasks expire <task_id> [--no-exec]");
        const res = noExec
          ? await agent.tasks.buildExpire(id)
          : await agent.tasks.expireAndExecute(id);
        writeOutput(stdout, jsonMode, res);
        return 0;
      }
      throw new CliUsageError(
        "Usage: swarm tasks <list|get|init|check|mine|earnings|onboard|next|claim|submit|verify|finalize|expire> ...",
      );
    }

    if (cmd === "proofs") {
      if (sub === "list" || !sub) {
        const res = await agent.proofs.list({
          network,
          limit: getIntFlag(flags, "--limit"),
          policy: getIntFlag(flags, "--policy"),
        });
        writeOutput(stdout, jsonMode, res);
        return 0;
      }
      throw new CliUsageError("Usage: swarm proofs list [--limit N] [--policy N]");
    }

    if (cmd === "inbox") {
      if (sub === "list") {
        const rawStatus = getStringFlag(flags, "--status");
        if (rawStatus && rawStatus !== "pending" && rawStatus !== "all") {
          throw new CliUsageError('Invalid --status: expected "pending" or "all".');
        }
        const res = await agent.inbox.list({
          status: (rawStatus as "pending" | "all" | undefined) ?? "pending",
          limit: getIntFlag(flags, "--limit"),
        });
        writeOutput(stdout, jsonMode, res);
        return 0;
      }
      if (sub === "open") {
        if (rest.length === 0) throw new CliUsageError("Usage: swarm inbox open <msg_id...>");
        const res = await agent.inbox.open(rest);
        writeOutput(stdout, jsonMode, res);
        return 0;
      }
      if (sub === "send") {
        const [toWallet, body] = rest;
        if (!toWallet || !body) {
          throw new CliUsageError(
            "Usage: swarm inbox send <to_wallet> <body> [--thread <id>] [--intent <intent>]",
          );
        }
        const res = await agent.inbox.send(toWallet, body, {
          threadId: getStringFlag(flags, "--thread"),
          intent: getStringFlag(flags, "--intent"),
          deliveryKey: getStringFlag(flags, "--delivery-key"),
        });
        writeOutput(stdout, jsonMode, res);
        return 0;
      }
      if (sub === "ack") {
        const target = rest[0];
        if (!target) throw new CliUsageError("Usage: swarm inbox ack <msg_id>");
        if (flags.get("--cursor")) {
          await agent.inbox.ack(target);
          writeOutput(stdout, jsonMode, { ok: true, up_to: target });
        } else {
          const res = await agent.inbox.ackIds([target]);
          writeOutput(stdout, jsonMode, res);
        }
        return 0;
      }
      if (sub === "mute") {
        const threadId = rest[0];
        if (!threadId) throw new CliUsageError("Usage: swarm inbox mute <thread_id> [--unmute]");
        const muted = !flags.get("--unmute");
        const res = await agent.inbox.mute(threadId, muted);
        writeOutput(stdout, jsonMode, res);
        return 0;
      }
      throw new CliUsageError("Usage: swarm inbox <list|open|send|ack|mute> ...");
    }

    if (cmd === "topics") {
      if (sub === "read") {
        const topic = rest[0];
        if (!isValidTopicId(topic)) {
          throw new CliUsageError(`Usage: swarm topics read <${VALID_TOPICS.join("|")}>`);
        }
        const res = await agent.topics.read(topic, {
          limit: getIntFlag(flags, "--limit"),
        });
        writeOutput(stdout, jsonMode, res);
        return 0;
      }
      if (sub === "publish") {
        const [topic, body] = rest;
        if (!isValidTopicId(topic) || !body) {
          throw new CliUsageError(
            `Usage: swarm topics publish <${VALID_TOPICS.join("|")}> <body> [--ref <id>]`,
          );
        }
        const res = await agent.topics.publish(topic, body, {
          refId: getStringFlag(flags, "--ref"),
          replyTo: getStringFlag(flags, "--reply-to"),
          intent: getStringFlag(flags, "--intent"),
        });
        writeOutput(stdout, jsonMode, res);
        return 0;
      }
      throw new CliUsageError("Usage: swarm topics <read|publish> ...");
    }

    if (cmd === "vow-verify") {
      const target = sub;
      if (!target) {
        throw new CliUsageError("Usage: swarm vow-verify <file-or-url> [--rpc <url>]");
      }
      let payload: string | Record<string, unknown>;
      if (target === "-") {
        payload = await readStdinText(io);
      } else if (/^[a-zA-Z][a-zA-Z0-9+.-]*:\/\//.test(target)) {
        payload = target;
      } else {
        try {
          payload = JSON.parse(readFileSync(target, "utf8")) as Record<string, unknown>;
        } catch (err) {
          throw new CliUsageError(
            `Failed to read VOW JSON file ${target}: ${err instanceof Error ? err.message : String(err)}`,
          );
        }
      }
      const verdict = await agent.tasks.verifyVow(payload, {
        rpcUrl,
        allowLocalhostHttp: Boolean(flags.get("--allow-localhost-http")),
      });
      writeOutput(stdout, jsonMode, verdict);
      return verdict.valid ? 0 : 1;
    }

    throw new CliUsageError(`Unknown command "${cmd}". Run \`swarm --help\` for usage.`);
  } catch (err) {
    if (err instanceof CliUsageError) {
      stderr.write(`${err.message}\n`);
      return err.exitCode;
    }
    if (err instanceof SwarmClientError) {
      const exitCode = err.code === "AUTH_REQUIRED" ? 2 : 1;
      writeError(stdout, stderr, jsonMode, err.message, err.code);
      return exitCode;
    }
    const message = err instanceof Error ? err.message : String(err);
    writeError(stdout, stderr, jsonMode, message);
    return 1;
  }
}

function isDirectExecution(): boolean {
  const entry = process.argv[1];
  if (!entry) return false;
  try {
    return realpathSync(entry) === realpathSync(fileURLToPath(import.meta.url));
  } catch {
    return false;
  }
}

function exitAfterFlush(code: number): void {
  const finish = () => process.exit(code);
  if (process.stdout.writableNeedDrain) {
    process.stdout.once("drain", finish);
    return;
  }
  finish();
}

if (isDirectExecution()) {
  runSwarmCli(process.argv.slice(2)).then(
    (code) => {
      exitAfterFlush(code);
    },
    (err: unknown) => {
      process.stderr.write(`${err instanceof Error ? err.message : String(err)}\n`);
      exitAfterFlush(2);
    },
  );
}
