/**
 * Devnet extension fixture for the human reputation-UI e2e: open or close a
 * live root -> agent extension so the served /reputation page has something to
 * render. Idempotent.
 *
 *   npx tsx scripts/e2e/extension-fixture.ts open    # submit_extension (if absent)
 *   npx tsx scripts/e2e/extension-fixture.ts close   # withdraw_extension (if present)
 *
 * Signers: id.json = root (= web-position root). The recipient never signs
 * (only the extender attests), so it can be any pubkey: defaults to test.json,
 * override with RECIPIENT_PUBKEY=<base58> (used to open the permanent dogfood
 * extension the human reputation-UI CI reads). Prints the agent pubkey on `open`
 * so the runner can pass it to Playwright.
 */
import * as anchor from "@coral-xyz/anchor";
import { BN } from "@coral-xyz/anchor";
import { Connection, Keypair, PublicKey, SystemProgram } from "@solana/web3.js";
import { readFileSync } from "fs";
import { homedir } from "os";
import { join } from "path";
import type { ExtensionRegistry } from "../../target/types/extension_registry";

const DEVNET = "https://api.devnet.solana.com";
const TYPE_CAPABILITY_VALIDATION = 0;
const BOND = new BN(5_000_000);

function loadKeypair(path: string): Keypair {
  return Keypair.fromSecretKey(
    Uint8Array.from(JSON.parse(readFileSync(path, "utf8")) as number[])
  );
}
function loadIdl(name: string): anchor.Idl {
  return JSON.parse(
    readFileSync(
      join(__dirname, "..", "..", "target", "idl", `${name}.json`),
      "utf8"
    )
  ) as anchor.Idl;
}

async function main(): Promise<void> {
  const mode = process.argv[2];
  if (mode !== "open" && mode !== "close") {
    throw new Error("usage: extension-fixture.ts <open|close>");
  }
  const root = loadKeypair(join(homedir(), ".config/solana/id.json"));
  // Recipient is account-only (never a signer): prefer RECIPIENT_PUBKEY, else
  // fall back to the test.json keypair's pubkey.
  const recipientPubkey = process.env.RECIPIENT_PUBKEY
    ? new PublicKey(process.env.RECIPIENT_PUBKEY)
    : loadKeypair(join(homedir(), ".config/solana/test.json")).publicKey;
  const connection = new Connection(DEVNET, "confirmed");
  const provider = new anchor.AnchorProvider(
    connection,
    new anchor.Wallet(root),
    {
      commitment: "confirmed",
    }
  );
  anchor.setProvider(provider);
  const registry = new anchor.Program(
    loadIdl("extension_registry"),
    provider
  ) as unknown as anchor.Program<ExtensionRegistry>;

  const [globalState] = PublicKey.findProgramAddressSync(
    [Buffer.from("extension_global")],
    registry.programId
  );
  try {
    await registry.methods
      .initialize(root.publicKey, root.publicKey)
      .accountsPartial({
        globalState,
        payer: root.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc({ commitment: "confirmed" });
  } catch {
    /* already initialized */
  }

  const [extension] = PublicKey.findProgramAddressSync(
    [
      Buffer.from("extension"),
      root.publicKey.toBuffer(),
      recipientPubkey.toBuffer(),
    ],
    registry.programId
  );
  const exists = (await connection.getAccountInfo(extension)) !== null;

  if (mode === "open") {
    if (!exists) {
      await registry.methods
        .submitExtension(TYPE_CAPABILITY_VALIDATION, BOND)
        .accountsPartial({
          extension,
          extender: root.publicKey,
          recipient: recipientPubkey,
          systemProgram: SystemProgram.programId,
        })
        .rpc({ commitment: "confirmed" });
    }
    // Stdout contract: the agent pubkey, for the runner to export.
    console.log(recipientPubkey.toBase58());
    return;
  }

  if (exists) {
    await registry.methods
      .withdrawExtension()
      .accountsPartial({
        extension,
        extender: root.publicKey,
        recipient: recipientPubkey,
      })
      .rpc({ commitment: "confirmed" });
  }
  console.error("extension closed");
}

main().catch((e: unknown) => {
  console.error(e);
  process.exit(1);
});
