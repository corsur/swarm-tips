#!/usr/bin/env tsx
/**
 * account-survey.ts — pre-extension audit tool.
 *
 * Generalized from `coordination-app/scripts/agent-state-mainnet-survey.ts`
 * (Sprint 0-FU-2). Surveys all accounts of a given Anchor account type
 * owned by a deployed program on a target Solana cluster, grouped by
 * on-chain `dataLen`. Use this BEFORE any struct-extension PR's
 * "bytewise compatible, no realloc needed" claim is accepted, to confirm
 * the assumed pre-extension size matches every account on chain.
 *
 * Why this script exists: Sprint 0 of the v5 roadmap-iteration surfaced
 * (2026-05-06) that v4 task #12 claimed bytewise compatibility for an
 * AgentState extension on the assumption that all on-chain accounts
 * were already at the post-v2 size (90 bytes). The claim was correct
 * for v2-onward accounts but missed v1-era accounts at 42 bytes. The
 * founder wallet's devnet AgentState was 42 bytes, undeserializable
 * under the v4 struct. Filed as 0a-FINDING; mitigated by the
 * `migrate_agent_state` instruction in PR #1.
 *
 * Going forward: any struct-extension PR for a deployed program ships
 * with the output of this script for the relevant account type, proving
 * the assumed size matches every on-chain account. See the cross-repo
 * rule in `swarm-tips-repo/CLAUDE.md` "Pre-extension account survey."
 *
 * Usage:
 *   npx tsx scripts/account-survey.ts \
 *     --program <program_pubkey> \
 *     --account-name <e.g., AgentState> \
 *     --cluster <devnet|mainnet>
 *
 * Required args:
 *   --program       Solana program pubkey (the deployed program ID)
 *   --account-name  Anchor account name (used to derive the
 *                   discriminator: sha256("account:<Name>")[0..8])
 *
 * Optional args:
 *   --cluster       devnet (default) or mainnet
 *   --rpc-url       Override the RPC URL (takes precedence over --cluster)
 *
 * Output: per-size buckets with sample wallet pubkeys + total count.
 * Exit 0 always (non-blocking diagnostic, not a CI gate).
 */

import { Connection, PublicKey } from "@solana/web3.js";
import { createHash } from "crypto";

// Inline base58 encoder so this script doesn't need a workspace
// dep on `bs58`. Discriminators are 8 bytes; encode is fast and
// allocation-free for that size.
const BASE58_ALPHABET =
  "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
function base58Encode(bytes: Uint8Array): string {
  let zeros = 0;
  while (zeros < bytes.length && bytes[zeros] === 0) zeros++;
  // Convert byte array to base58 via repeated division.
  const digits: number[] = [];
  for (let i = zeros; i < bytes.length; i++) {
    let carry = bytes[i];
    for (let j = 0; j < digits.length; j++) {
      carry += digits[j] << 8;
      digits[j] = carry % 58;
      carry = (carry / 58) | 0;
    }
    while (carry > 0) {
      digits.push(carry % 58);
      carry = (carry / 58) | 0;
    }
  }
  let s = "";
  for (let i = 0; i < zeros; i++) s += "1";
  for (let i = digits.length - 1; i >= 0; i--) s += BASE58_ALPHABET[digits[i]];
  return s;
}

interface CliArgs {
  program: PublicKey;
  accountName: string;
  rpcUrl: string;
}

function parseArgs(): CliArgs {
  const argv = process.argv.slice(2);
  const get = (flag: string): string | undefined => {
    const idx = argv.indexOf(flag);
    return idx >= 0 ? argv[idx + 1] : undefined;
  };

  const programArg = get("--program");
  if (!programArg) {
    throw new Error("--program <pubkey> is required");
  }
  const accountName = get("--account-name");
  if (!accountName) {
    throw new Error("--account-name <Name> is required");
  }
  const rpcOverride = get("--rpc-url") ?? process.env["E2E_RPC_URL"];
  const cluster = get("--cluster") ?? "devnet";
  const rpcUrl =
    rpcOverride ??
    (cluster === "mainnet" || cluster === "mainnet-beta"
      ? "https://api.mainnet-beta.solana.com"
      : "https://api.devnet.solana.com");

  return {
    program: new PublicKey(programArg),
    accountName,
    rpcUrl,
  };
}

function discriminator(accountName: string): string {
  // Anchor: sha256("account:<Name>")[0..8], encoded base58 for the
  // memcmp filter (Solana JSON-RPC expects base58-encoded bytes).
  const disc = createHash("sha256")
    .update(`account:${accountName}`)
    .digest()
    .subarray(0, 8);
  return base58Encode(new Uint8Array(disc));
}

interface BucketRow {
  size: number;
  count: number;
  samples: string[];
}

async function main(): Promise<void> {
  const args = parseArgs();
  console.log(`\n=== Anchor account survey ===`);
  console.log(`RPC:          ${args.rpcUrl}`);
  console.log(`Program:      ${args.program.toBase58()}`);
  console.log(`Account name: ${args.accountName}`);

  const conn = new Connection(args.rpcUrl, "confirmed");
  const disc = discriminator(args.accountName);
  console.log(
    `Disc:         ${disc} (base58 of sha256("account:${args.accountName}")[..8])`
  );

  console.log(`\nFetching all matching accounts via getProgramAccounts…`);
  const accounts = await conn.getProgramAccounts(args.program, {
    commitment: "confirmed",
    filters: [{ memcmp: { offset: 0, bytes: disc } }],
  });
  console.log(`  Found ${accounts.length} account(s).`);

  if (accounts.length === 0) {
    console.log(`\nNo matching accounts on this cluster — survey complete.`);
    return;
  }

  const buckets = new Map<number, BucketRow>();
  for (const { pubkey, account } of accounts) {
    const size = account.data.length;
    const row = buckets.get(size) ?? { size, count: 0, samples: [] };
    row.count += 1;
    if (row.samples.length < 5) row.samples.push(pubkey.toBase58());
    buckets.set(size, row);
  }

  console.log(`\n--- Buckets by dataLen ---`);
  const sizes = [...buckets.keys()].sort((a, b) => a - b);
  for (const size of sizes) {
    const row = buckets.get(size)!;
    const pct = ((row.count / accounts.length) * 100).toFixed(1);
    console.log(
      `  size=${String(size).padStart(4)}  count=${String(row.count).padStart(
        5
      )}  (${pct}%)`
    );
    for (const sample of row.samples) {
      console.log(`    sample: ${sample}`);
    }
  }

  console.log(`\n--- Summary ---`);
  console.log(`  ${accounts.length} total ${args.accountName} account(s)`);
  console.log(
    `  ${sizes.length} distinct size bucket(s): [${sizes.join(", ")}]`
  );
  if (sizes.length > 1) {
    console.log(
      `\n  ⚠ Multiple sizes detected — any struct-extension PR claiming`
    );
    console.log(
      `  "bytewise compatible, no realloc" must explicitly handle each`
    );
    console.log(`  size in this list, OR ship a migration instruction.`);
  } else {
    console.log(`\n  All accounts at the same size — bytewise extension is`);
    console.log(
      `  safe IFF the new struct keeps the existing size as a prefix.`
    );
  }
}

main().catch((e) => {
  console.error("\n\x1b[31m✗\x1b[0m fatal:", e);
  process.exit(1);
});
