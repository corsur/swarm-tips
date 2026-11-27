#!/usr/bin/env node
/**
 * `vow-verify <attestation.json> [--rpc <url>]`
 *
 * MVP CLI entrypoint. Reads a VOW v1 attestation from a file path or
 * stdin, runs the full 7-step verifier against the named network's
 * default RPC (or the `--rpc` override), prints the verdict as JSON.
 *
 * Exits with code 0 on `valid: true`, code 1 on `valid: false`, code
 * 2 on usage error / unreadable file.
 */
import { readFileSync } from "node:fs";
import { Connection } from "@solana/web3.js";
import { verifyV1 } from "../vow/verify.js";
import { shillbotProtocol } from "../vow/decoders/shillbot.js";

const DEFAULT_RPC: Record<string, string> = {
  mainnet: "https://api.mainnet-beta.solana.com",
  devnet: "https://api.devnet.solana.com",
};

async function main(): Promise<number> {
  const args = process.argv.slice(2);
  if (args.length === 0 || args[0] === "--help" || args[0] === "-h") {
    process.stderr.write(
      "Usage: vow-verify <attestation.json> [--network mainnet|devnet] [--rpc <url>]\n" +
        "       vow-verify - [--network mainnet|devnet] [--rpc <url>]   (read from stdin)\n"
    );
    return 2;
  }

  const path = args[0];
  let rpcUrlOverride: string | undefined;
  let expectedNetwork: "mainnet" | "devnet" = "mainnet";
  for (let i = 1; i < args.length; i++) {
    if (args[i] === "--rpc" && i + 1 < args.length) {
      rpcUrlOverride = args[i + 1];
      i++;
    } else if (args[i] === "--network" && i + 1 < args.length) {
      const candidate = args[i + 1];
      if (candidate !== "mainnet" && candidate !== "devnet") {
        process.stderr.write(
          `invalid --network "${candidate}" (expected "mainnet" or "devnet")\n`
        );
        return 2;
      }
      expectedNetwork = candidate;
      i++;
    }
  }

  let raw: string;
  try {
    raw = path === "-" ? readStdinSync() : readFileSync(path, "utf8");
  } catch (e) {
    process.stderr.write(`failed to read ${path}: ${(e as Error).message}\n`);
    return 2;
  }

  let attestation: unknown;
  try {
    attestation = JSON.parse(raw);
  } catch (e) {
    process.stderr.write(`invalid JSON in ${path}: ${(e as Error).message}\n`);
    return 2;
  }

  const rpcUrl = rpcUrlOverride ?? DEFAULT_RPC[expectedNetwork];

  const rpc = new Connection(rpcUrl, "confirmed");
  const verdict = await verifyV1(
    attestation,
    shillbotProtocol,
    rpc,
    undefined,
    expectedNetwork
  );

  process.stdout.write(JSON.stringify(verdict, null, 2) + "\n");
  return verdict.valid ? 0 : 1;
}

function readStdinSync(): string {
  // Sync read works on Node 16+ for piped input (small files).
  return readFileSync(0, "utf8");
}

main().then(
  (code) => process.exit(code),
  (err) => {
    process.stderr.write(`unexpected error: ${(err as Error).message}\n`);
    process.exit(2);
  }
);
