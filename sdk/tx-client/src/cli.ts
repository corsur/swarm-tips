#!/usr/bin/env node
import { readFile } from "node:fs/promises";
import { Connection } from "@solana/web3.js";
import {
  buildTransaction,
  inspectTransaction,
  signAndBroadcast,
  verifyIntent,
  type BuildRequest,
  type TransactionIntent,
} from "./index.js";

async function input(): Promise<Record<string, unknown>> {
  const path = process.argv[3];
  const raw = path
    ? await readFile(path, "utf8")
    : await new Promise<string>((resolve, reject) => {
        const chunks: Buffer[] = [];
        process.stdin.on("data", (chunk) => chunks.push(Buffer.from(chunk)));
        process.stdin.on("end", () =>
          resolve(Buffer.concat(chunks).toString("utf8"))
        );
        process.stdin.on("error", reject);
      });
  return JSON.parse(raw) as Record<string, unknown>;
}

async function main(): Promise<void> {
  const command = process.argv[2];
  const value = await input();
  if (command === "build") {
    process.stdout.write(
      `${JSON.stringify(buildTransaction(value as unknown as BuildRequest))}\n`
    );
    return;
  }
  if (command === "inspect") {
    process.stdout.write(
      `${JSON.stringify(inspectTransaction(String(value.transaction)))}\n`
    );
    return;
  }
  if (command === "verify") {
    const result = verifyIntent(
      String(value.transaction),
      value.intent as unknown as TransactionIntent
    );
    process.stdout.write(
      `${JSON.stringify({ valid: true, inspection: result })}\n`
    );
    return;
  }
  if (command === "broadcast") {
    const connection = new Connection(String(value.rpcUrl), "confirmed");
    // Broadcast accepts only an already-signed wire transaction. The callback
    // returns it unchanged; the SDK never reads private-key files.
    const signature = await signAndBroadcast(
      connection,
      String(value.transaction),
      async (transaction) => transaction
    );
    process.stdout.write(`${JSON.stringify({ signature })}\n`);
    return;
  }
  throw new Error(
    "usage: swarm-tx <build|inspect|verify|broadcast> [input.json]; otherwise JSON is read from stdin"
  );
}

main().catch((error: unknown) => {
  process.stderr.write(
    `${error instanceof Error ? error.message : String(error)}\n`
  );
  process.exitCode = 1;
});
