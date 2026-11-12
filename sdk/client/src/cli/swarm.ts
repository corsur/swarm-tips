#!/usr/bin/env node
import { readFileSync, realpathSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { SwarmAgent, type SwarmNetwork } from "../agent.js";
import { SwarmClientError } from "../errors.js";
import type { TopicId } from "../inbox/client.js";
import { validatePreparedTransaction, type Action } from "../shillbot/api.js";
import { inspectTransaction } from "../shillbot/index.js";
import { CLIENT_VERSION } from "../version.js";
import {
  CliUsageError,
  assertNoSecretCliFlags,
  loadSignerFromCliOrEnv,
} from "./keyfile.js";

export interface CliIo {
  stdout?: { write: (chunk: string) => boolean; isTTY?: boolean };
  stderr?: { write: (chunk: string) => boolean };
  stdin?: () => string | Promise<string>;
  env?: Record<string, string | undefined>;
  fetch?: typeof fetch;
}

interface ParsedCliArgs {
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
]);

function parseCliArgs(argv: readonly string[]): ParsedCliArgs {
  assertNoSecretCliFlags(argv);
  const positionals: string[] = [];
  const flags = new Map<string, string | boolean>();

  for (let i = 0; i < argv.length; i += 1) {
    const token = argv[i];
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
    const next = argv[i + 1];
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
  listings [--limit N] [--category CAT] [--chain CHAIN]
  reputation <wallet>
  tasks list [--limit N] | mine | earnings | next <task-json-or-id>
  tasks claim <id> [--no-exec]
  tasks submit <id> --content <str> [--no-exec]
  tasks verify <id> [--no-exec]
  tasks finalize <id> [--no-exec]
  inbox list [--status pending|all] [--limit N]
  inbox open <msg_id...>
  inbox send <to_wallet> <body> [--thread <id>] [--intent <intent>]
  inbox ack <msg_id> [--cursor]
  inbox mute <thread_id> [--unmute]
  topics read <open-challenge|subcontract> [--limit N]
  topics publish <topic> <body> [--ref <id>] [--reply-to <id>] [--intent <intent>]
  tx inspect [--tx <base64>]
  tx verify --action <claim|submit|verify|finalize|approve|create> --wallet <pubkey> [--task-pda <pda>] [--tx <base64>]
  vow-verify <file-or-url> [--rpc <url>]

Global Options:
  --json                     Output structured JSON (auto-enabled when stdout is non-TTY)
  --network <mainnet|devnet> Target network (default: mainnet)
  --keypair-file <path>      Solana keypair file (must be chmod 600 on POSIX)
  --rpc-url <url>            Override Solana RPC URL
  --max-spend-lamports <n>   Max SOL escrow spend cap (default: 0)
  --max-spend-wei <n>        Max EVM escrow spend cap (default: 0)
  --help, -h                 Show this help text
  --version, -v              Print CLI version

Environment Variables:
  SWARM_SOLANA_KEYPAIR_PATH  Path to chmod 600 Solana keypair JSON/base58 file
  SWARM_SOLANA_SECRET_KEY    Base58 or JSON 64-byte Solana secret key
  SWARM_EVM_PRIVATE_KEY      0x-prefixed 32-byte EVM private key
  SWARM_AUTH_TOKEN           Pre-minted Bearer JWT
`;

const VALID_ACTIONS = new Set<Action>([
  "create",
  "claim",
  "submit",
  "approve",
  "verify",
  "finalize",
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
            "Usage: swarm tx verify --action <claim|submit|verify|finalize|approve|create> --wallet <pubkey> [--task-pda <pda>] [--tx <base64>]",
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
        const escrowLamports = getBigIntFlag(flags, "--escrow-lamports");
        try {
          const inspection = validatePreparedTransaction(txB64, {
            action: rawAction as Action,
            network,
            wallet,
            ...(taskPda ? { taskPda } : {}),
            ...(feePayer ? { feePayer } : {}),
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

    const signer = loadSignerFromCliOrEnv({
      keypairFile: getStringFlag(flags, "--keypair-file"),
      env,
      evmRpcUrl: env.SWARM_EVM_RPC_URL,
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
        if (!id) throw new CliUsageError("Usage: swarm tasks claim <task_id> [--no-exec]");
        const res = noExec ? await agent.tasks.claim(id) : await agent.tasks.claimAndExecute(id);
        writeOutput(stdout, jsonMode, res);
        return 0;
      }
      if (sub === "submit") {
        const id = rest[0];
        const content = getStringFlag(flags, "--content");
        if (!id || !content) {
          throw new CliUsageError("Usage: swarm tasks submit <task_id> --content <str> [--no-exec]");
        }
        const res = noExec
          ? await agent.tasks.submit(id, { content_id: content })
          : await agent.tasks.submitAndExecute(id, content);
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
      throw new CliUsageError(
        "Usage: swarm tasks <list|mine|earnings|next|claim|submit|verify|finalize> ...",
      );
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
        const topic = rest[0] as TopicId | undefined;
        if (topic !== "open-challenge" && topic !== "subcontract") {
          throw new CliUsageError("Usage: swarm topics read <open-challenge|subcontract>");
        }
        const res = await agent.topics.read(topic, {
          limit: getIntFlag(flags, "--limit"),
        });
        writeOutput(stdout, jsonMode, res);
        return 0;
      }
      if (sub === "publish") {
        const [topic, body] = rest;
        if ((topic !== "open-challenge" && topic !== "subcontract") || !body) {
          throw new CliUsageError(
            "Usage: swarm topics publish <open-challenge|subcontract> <body> [--ref <id>]",
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
      } else if (/^https?:\/\//i.test(target)) {
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
      const verdict = await agent.tasks.verifyVow(payload, { rpcUrl });
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
