/** Thin CLI wrapper around @swarm-tips/client's canonical Switchboard builder. */
import { Connection, PublicKey } from "@solana/web3.js";
import { buildSwitchboardCrankAndVerify } from "@swarm-tips/client/shillbot";

function parseArgs(): Record<string, string> {
  const parsed: Record<string, string> = {};
  const argv = process.argv.slice(2);
  for (let i = 0; i < argv.length; i += 2) {
    parsed[argv[i].replace(/^--/, "")] = argv[i + 1];
  }
  return parsed;
}

async function main(): Promise<void> {
  const args = parseArgs();
  const required = [
    "task-id",
    "payer",
    "score",
    "hash",
    "task-pda",
    "feed",
    "rpc",
  ];
  for (const name of required) {
    if (!args[name]) throw new Error(`missing required --${name}`);
  }
  const network =
    args.network === "devnet" || args.rpc.includes("devnet")
      ? "devnet"
      : "mainnet";
  const transaction = await buildSwitchboardCrankAndVerify({
    connection: new Connection(args.rpc, "confirmed"),
    taskId: args["task-id"],
    payer: new PublicKey(args.payer),
    taskPda: new PublicKey(args["task-pda"]),
    feed: new PublicKey(args.feed),
    compositeScore: BigInt(args.score),
    verificationHash: Buffer.from(args.hash, "hex"),
    network,
  });
  process.stdout.write(Buffer.from(transaction.serialize()).toString("base64"));
}

main().catch((error: unknown) => {
  process.stderr.write(
    `build-verify-tx failed: ${
      error instanceof Error ? error.message : String(error)
    }\n`
  );
  process.exitCode = 1;
});
