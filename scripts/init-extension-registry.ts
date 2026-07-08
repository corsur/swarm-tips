/**
 * Idempotent extension-registry GlobalState initialize — the post-deploy
 * step of the deploy-mainnet CI job (and reusable against any cluster).
 *
 *   RPC_URL=https://api.mainnet-beta.solana.com \
 *     npx tsx scripts/init-extension-registry.ts
 *
 * Signer/payer: ~/.config/solana/id.json (in CI this is
 * SOLANA_DEPLOY_KEYPAIR = the root wallet). Authority and treasury are both
 * set to that wallet — the same shape the devnet dogfood uses. If
 * GlobalState already exists the init throws and we verify the account is
 * present instead (`init` constraint — no resurrection, no overwrite).
 */
import * as anchor from "@coral-xyz/anchor";
import { Connection, Keypair, PublicKey, SystemProgram } from "@solana/web3.js";
import { readFileSync } from "fs";
import { homedir } from "os";
import { join } from "path";
import type { ExtensionRegistry } from "../target/types/extension_registry";

function loadKeypair(path: string): Keypair {
  return Keypair.fromSecretKey(
    Uint8Array.from(JSON.parse(readFileSync(path, "utf8")) as number[])
  );
}

async function main(): Promise<void> {
  const rpcUrl = process.env.RPC_URL;
  if (!rpcUrl) {
    throw new Error("RPC_URL is required (refusing to guess a cluster)");
  }
  const root = loadKeypair(join(homedir(), ".config/solana/id.json"));
  const connection = new Connection(rpcUrl, "confirmed");
  const provider = new anchor.AnchorProvider(connection, new anchor.Wallet(root), {
    commitment: "confirmed",
  });
  anchor.setProvider(provider);
  const idl = JSON.parse(
    readFileSync(
      join(__dirname, "..", "target", "idl", "extension_registry.json"),
      "utf8"
    )
  ) as anchor.Idl;
  const registry = new anchor.Program(
    idl,
    provider
  ) as unknown as anchor.Program<ExtensionRegistry>;

  const [globalState] = PublicKey.findProgramAddressSync(
    [Buffer.from("extension_global")],
    registry.programId
  );

  const existing = await connection.getAccountInfo(globalState);
  if (existing) {
    console.log(
      `global_state already initialized at ${globalState.toBase58()} (${existing.data.length} bytes) — nothing to do`
    );
    return;
  }

  await registry.methods
    .initialize(root.publicKey, root.publicKey)
    .accountsPartial({
      globalState,
      payer: root.publicKey,
      systemProgram: SystemProgram.programId,
    })
    .rpc({ commitment: "confirmed" });

  const after = await connection.getAccountInfo(globalState);
  if (!after) {
    throw new Error("initialize landed but global_state account not found");
  }
  console.log(
    `global_state initialized at ${globalState.toBase58()} — authority/treasury = ${root.publicKey.toBase58()}`
  );
}

main().catch((e: unknown) => {
  console.error(e);
  process.exit(1);
});
