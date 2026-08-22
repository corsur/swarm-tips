// Operational: set GlobalState.staleness_window_seconds on the shillbot program.
// DRY-RUN by default (simulate only). Pass --execute to send the authority-signed tx.
// Signer = ~/.config/solana/id.json (must be GlobalState.authority).
import * as anchor from "@coral-xyz/anchor";
import { BN } from "@coral-xyz/anchor";
import { Connection, Keypair, PublicKey } from "@solana/web3.js";
import { readFileSync } from "fs";
import { homedir } from "os";
import { join } from "path";

const RPC = process.env.MAINNET_RPC ?? "https://api.mainnet-beta.solana.com";
const TARGET_STALENESS = new BN(Number(process.env.STALENESS ?? 1_209_600)); // 14d default
const EXECUTE = process.argv.includes("--execute");

function loadKeypair(name: string): Keypair {
  return Keypair.fromSecretKey(
    Uint8Array.from(
      JSON.parse(readFileSync(join(homedir(), ".config/solana", name), "utf8"))
    )
  );
}

(async () => {
  const authority = loadKeypair("id.json");
  const connection = new Connection(RPC, "confirmed");
  const provider = new anchor.AnchorProvider(
    connection,
    new anchor.Wallet(authority),
    { commitment: "confirmed" }
  );
  anchor.setProvider(provider);
  const idl = JSON.parse(
    readFileSync(
      join(__dirname, "..", "..", "target", "idl", "shillbot.json"),
      "utf8"
    )
  );
  const program = new anchor.Program(idl as anchor.Idl, provider);
  const globalPda = PublicKey.findProgramAddressSync(
    [Buffer.from("shillbot_global")],
    program.programId
  )[0];

  const s: any = await (program.account as any).globalState.fetch(globalPda);
  console.log("program:", program.programId.toBase58());
  console.log("globalPda:", globalPda.toBase58());
  console.log(
    "authority (id.json):",
    authority.publicKey.toBase58(),
    "| on-chain authority:",
    s.authority.toBase58(),
    authority.publicKey.equals(s.authority)
      ? "✓ MATCH"
      : "✗ MISMATCH — cannot sign"
  );
  const cur = s.stalenessWindowSeconds as BN;
  console.log(
    `\nstaleness_window_seconds: current=${cur.toString()} (${(
      cur.toNumber() / 86400
    ).toFixed(2)}d)  ->  target=${TARGET_STALENESS.toString()} (${(
      TARGET_STALENESS.toNumber() / 86400
    ).toFixed(2)}d)`
  );
  console.log("(all other 13 params preserved unchanged)");

  const args = [
    s.protocolFeeBps,
    s.qualityThreshold,
    s.challengeWindowSeconds,
    s.verificationTimeoutSeconds,
    s.attestationDelaySeconds,
    TARGET_STALENESS,
    s.maxConcurrentClaims,
    s.challengeBondMultiplierBps,
    s.bondSlashTreasuryBps,
    s.paused,
    s.pausedPlatforms,
    s.rateLimitWindowSeconds,
    s.maxTasksPerRateWindow,
    s.disputeResolutionWindowSeconds,
  ];
  const builder = (program.methods as any)
    .updateParams(...args)
    .accountsPartial({
      globalState: globalPda,
      authority: authority.publicKey,
    });

  if (!EXECUTE) {
    const sim = await builder.simulate();
    console.log(
      "\nDRY RUN — simulate result:",
      JSON.stringify(sim?.raw?.slice?.(-3) ?? sim ?? "ok").slice(0, 300)
    );
    console.log("No tx sent. Re-run with --execute to apply.");
    return;
  }
  const sig = await builder.rpc();
  console.log("\nEXECUTED updateParams. sig:", sig);
  const after: any = await (program.account as any).globalState.fetch(
    globalPda
  );
  console.log(
    "staleness_window_seconds now:",
    (after.stalenessWindowSeconds as BN).toString()
  );
})().catch((e) => {
  console.error("ERR", String(e).slice(0, 400));
  process.exit(1);
});
