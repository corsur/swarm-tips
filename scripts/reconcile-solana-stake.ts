/**
 * Reconcile the on-chain Solana stake to the value declared in chain-registry.
 *
 * The Solana counterpart of the `Reconcile EVM Stake` workflow. Same contract:
 * the registry is the desired state, this converges the chain to it, and it is
 * a no-op when they already agree.
 *
 * Two steps, both idempotent:
 *   1. `migrate_global_config` — grows the singleton GlobalConfig from the
 *      107-byte layout to 115 so it can carry `stake_lamports`. Skipped once
 *      the account is already 115 bytes.
 *   2. `set_stake_lamports` — writes the registry's value. Skipped when the
 *      on-chain value already matches.
 *
 * Run:
 *   npx tsx scripts/reconcile-solana-stake.ts --cluster devnet [--apply]
 *
 * Without `--apply` it only reports the diff, so CI can run it read-only.
 */
import {
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
  Transaction,
} from "@solana/web3.js";
import { AnchorProvider, Program, Wallet, type Idl } from "@coral-xyz/anchor";
import { readFileSync } from "fs";
import { homedir } from "os";

const PROGRAM_ID = new PublicKey(
  "2qqVk7kUqffnahiJpcQJCsSd8ErbEUgKTgCn1zYsw64P"
);
const LEGACY_SPACE = 107;
const CURRENT_SPACE = 115;

const args = process.argv.slice(2);
const apply = args.includes("--apply");
const cluster =
  args.find((a) => a.startsWith("--cluster="))?.split("=")[1] ??
  (args.includes("--cluster") ? args[args.indexOf("--cluster") + 1] : "devnet");

/** Desired stake for this cluster, parsed from the Rust registry — the single
 *  source of truth. Reading the source beats re-encoding it in TS, which would
 *  just be one more copy to drift. */
function registryStake(cluster: string): bigint {
  const src = readFileSync(
    `${__dirname}/../crates/chain-registry/src/lib.rs`,
    "utf8"
  );
  const wantMainnet = cluster === "mainnet" || cluster === "mainnet-beta";
  for (const block of src.split("ChainEntry {").slice(1)) {
    if (!/chain_id:\s*SOLANA_/.test(block)) continue;
    const isMainnet = /is_mainnet:\s*true/.test(block);
    if (isMainnet !== wantMainnet) continue;
    const m = block.match(/stake_base_units:\s*([0-9_]+)/);
    if (m) return BigInt(m[1].replace(/_/g, ""));
  }
  throw new Error(`no Solana ${cluster} entry with a stake in chain-registry`);
}

async function main(): Promise<void> {
  const rpc =
    process.env["SOLANA_RPC_URL"] ??
    (cluster.startsWith("main")
      ? "https://api.mainnet-beta.solana.com"
      : "https://api.devnet.solana.com");
  const connection = new Connection(rpc, "confirmed");

  const authority = Keypair.fromSecretKey(
    new Uint8Array(
      JSON.parse(
        readFileSync(
          process.env["ANCHOR_WALLET"] ?? `${homedir()}/.config/solana/id.json`,
          "utf8"
        )
      ) as number[]
    )
  );
  const provider = new AnchorProvider(connection, new Wallet(authority), {
    commitment: "confirmed",
  });
  const idl = JSON.parse(
    readFileSync(`${__dirname}/../target/idl/coordination_game.json`, "utf8")
  ) as Idl;
  const program = new Program(idl, provider) as never as {
    methods: Record<
      string,
      (...a: unknown[]) => {
        accounts: (a: unknown) => { rpc: () => Promise<string> };
      }
    >;
  };

  const [globalConfig] = PublicKey.findProgramAddressSync(
    [Buffer.from("global_config")],
    PROGRAM_ID
  );
  const want = registryStake(cluster);
  const info = await connection.getAccountInfo(globalConfig);
  if (!info)
    throw new Error(
      `GlobalConfig ${globalConfig.toBase58()} not found on ${cluster}`
    );

  console.log(`cluster=${cluster} globalConfig=${globalConfig.toBase58()}`);
  console.log(
    `  account size : ${info.data.length} (legacy ${LEGACY_SPACE}, current ${CURRENT_SPACE})`
  );
  console.log(`  registry wants stake_lamports = ${want}`);

  // --- step 1: migrate ------------------------------------------------------
  if (info.data.length === LEGACY_SPACE) {
    console.log("  MIGRATION NEEDED (account is still the legacy layout)");
    if (apply) {
      // The handler is pure-compute and the caller pre-funds the rent delta,
      // matching migrate_agent_state. Send both in one tx.
      const rentDelta =
        (await connection.getMinimumBalanceForRentExemption(CURRENT_SPACE)) -
        info.lamports;
      const tx = new Transaction();
      if (rentDelta > 0) {
        tx.add(
          SystemProgram.transfer({
            fromPubkey: authority.publicKey,
            toPubkey: globalConfig,
            lamports: rentDelta,
          })
        );
      }
      const ix = await (
        program as never as {
          methods: {
            migrateGlobalConfig: () => {
              accounts: (a: unknown) => { instruction: () => Promise<never> };
            };
          };
        }
      ).methods
        .migrateGlobalConfig()
        .accounts({ globalConfig, authority: authority.publicKey })
        .instruction();
      tx.add(ix as never);
      const sig = await provider.sendAndConfirm(tx, []);
      console.log(`  migrated (${sig.slice(0, 16)}…)`);
    }
  } else if (info.data.length !== CURRENT_SPACE) {
    throw new Error(
      `unexpected GlobalConfig size ${info.data.length}; expected ${LEGACY_SPACE} or ${CURRENT_SPACE}`
    );
  } else {
    console.log("  migration: already applied");
  }

  // --- step 2: set the stake ------------------------------------------------
  const after = await connection.getAccountInfo(globalConfig);
  if (!after || after.data.length !== CURRENT_SPACE) {
    console.log(
      "  stake: cannot read until the migration is applied — rerun with --apply"
    );
    process.exit(apply ? 1 : 0);
  }
  // stake_lamports is the last 8 bytes of the 115-byte layout.
  const onchain = after.data.readBigUInt64LE(CURRENT_SPACE - 8);
  console.log(`  on-chain stake_lamports = ${onchain}`);
  if (onchain === want) {
    console.log("  PASS: chain already matches the registry");
    return;
  }
  console.log(`  DRIFT: on-chain ${onchain} != registry ${want}`);
  if (!apply) {
    console.log("  (read-only run — pass --apply to converge)");
    process.exit(1);
  }
  const sig = await (
    program as never as {
      methods: {
        setStakeLamports: (n: unknown) => {
          accounts: (a: unknown) => { rpc: () => Promise<string> };
        };
      };
    }
  ).methods
    .setStakeLamports(want)
    .accounts({ globalConfig, authority: authority.publicKey })
    .rpc();
  const verify = await connection.getAccountInfo(globalConfig);
  const now = verify!.data.readBigUInt64LE(CURRENT_SPACE - 8);
  // A confirmed signature is not proof the value moved.
  if (now !== want)
    throw new Error(
      `set_stake_lamports did not take effect: ${now} != ${want}`
    );
  console.log(`  set stake_lamports = ${now} (${sig.slice(0, 16)}…)`);
}

main().catch((e) => {
  console.error(e instanceof Error ? e.message : e);
  process.exit(1);
});
