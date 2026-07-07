#!/usr/bin/env tsx
/**
 * update-params-post-upgrade.ts — one-shot migration helper.
 *
 * After the 2026-05-07 program redeploy, on-chain GlobalState accounts
 * that pre-date the D2/D3 commit (which carved fields out of `_reserved`)
 * now have `rate_limit_window_seconds = 0` and
 * `max_tasks_per_rate_window = 0` — the bytes are zero-initialized from
 * the old `_reserved` block, but the new program reads them as the live
 * rate-limit config. With max=0, every `create_task` fails immediately
 * with `RateLimitExceeded`.
 *
 * This script calls `update_params` once with the canonical defaults
 * (1-hour window, 10 tasks/hour cap) so the gate operates correctly
 * post-upgrade.
 *
 * Usage:
 *   npx tsx scripts/update-params-post-upgrade.ts --cluster devnet
 *   npx tsx scripts/update-params-post-upgrade.ts --cluster mainnet
 *
 * Authority: uses ~/.config/solana/id.json (the wallet that initialized
 * GlobalState on the target cluster). Will fail with NotAuthority if the
 * keypair doesn't match `global.authority`.
 */

import { AnchorProvider, Program, Wallet, BN } from "@coral-xyz/anchor";
import { Connection, Keypair, PublicKey } from "@solana/web3.js";
import { readFileSync } from "fs";
import { homedir } from "os";
import { Shillbot } from "../target/types/shillbot";
import { readFileSync as readFileSyncIdl } from "fs";
import { resolve as resolvePath } from "path";
const idl = JSON.parse(
  readFileSyncIdl(
    resolvePath(__dirname, "../target/idl/shillbot.json"),
    "utf-8"
  )
);

function parseArgs(): { cluster: "devnet" | "mainnet" } {
  const args = process.argv.slice(2);
  let cluster: "devnet" | "mainnet" = "devnet";
  for (let i = 0; i < args.length; i++) {
    if (args[i] === "--cluster") {
      const v = args[i + 1];
      if (v !== "devnet" && v !== "mainnet") {
        throw new Error(`--cluster must be devnet or mainnet, got: ${v}`);
      }
      cluster = v;
      i++;
    }
  }
  return { cluster };
}

async function main() {
  const { cluster } = parseArgs();
  const rpcUrl =
    cluster === "mainnet"
      ? "https://api.mainnet-beta.solana.com"
      : "https://api.devnet.solana.com";
  const connection = new Connection(rpcUrl, "confirmed");

  const keypairPath = `${homedir()}/.config/solana/id.json`;
  const secret = JSON.parse(readFileSync(keypairPath, "utf-8")) as number[];
  const keypair = Keypair.fromSecretKey(Uint8Array.from(secret));
  const wallet = new Wallet(keypair);

  const provider = new AnchorProvider(connection, wallet, {
    commitment: "confirmed",
  });
  const program = new Program<Shillbot>(idl as Shillbot, provider);

  const [globalPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("shillbot_global")],
    program.programId
  );

  console.log(`cluster: ${cluster}`);
  console.log(`authority: ${keypair.publicKey.toBase58()}`);
  console.log(`globalPda: ${globalPda.toBase58()}`);

  const before = await program.account.globalState.fetch(globalPda);
  console.log("\nBefore:");
  console.log(
    `  rate_limit_window_seconds: ${before.rateLimitWindowSeconds.toString()}`
  );
  console.log(`  max_tasks_per_rate_window: ${before.maxTasksPerRateWindow}`);
  console.log(`  protocol_fee_bps: ${before.protocolFeeBps}`);
  console.log(`  paused: ${before.paused}`);

  const sig = await program.methods
    .updateParams(
      before.protocolFeeBps,
      before.qualityThreshold,
      before.challengeWindowSeconds,
      before.verificationTimeoutSeconds,
      before.attestationDelaySeconds,
      before.stalenessWindowSeconds,
      before.maxConcurrentClaims,
      before.challengeBondMultiplierBps as unknown as number,
      before.bondSlashTreasuryBps,
      before.paused,
      before.pausedPlatforms,
      // Canonical post-upgrade defaults from constants.rs.
      new BN(3600), // rate_limit_window_seconds: 1 hour
      10, // max_tasks_per_rate_window
      new BN(604_800) // dispute_resolution_window_seconds: 7 days
    )
    .accountsPartial({
      globalState: globalPda,
      authority: keypair.publicKey,
    })
    .signers([keypair])
    .rpc();

  console.log(`\nupdate_params signature: ${sig}`);

  const after = await program.account.globalState.fetch(globalPda);
  console.log("\nAfter:");
  console.log(
    `  rate_limit_window_seconds: ${after.rateLimitWindowSeconds.toString()}`
  );
  console.log(`  max_tasks_per_rate_window: ${after.maxTasksPerRateWindow}`);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
