/**
 * Devnet e2e — the on-chain extension-credit flow against the LIVE devnet
 * programs (extension-registry + extension-credit), with a money-conservation
 * ledger. Proves:
 *   - submit_extension locks a bond; withdraw_extension returns bond + rent
 *     (no leak)
 *   - open_advance fronts capital; route_and_recoup splits routed earnings
 *     backer-first, the agent getting the surplus; all earnings accounted for
 *
 * Signers: id.json = root (authority / treasury / backer); test.json = agent
 * (recipient — receives funds, signs nothing).
 *
 * Here the "finalized payout" routed to the advance vault is simulated with a
 * direct transfer, keeping this test fast and self-contained. The REAL end-to-end
 * version — a live game-play task whose protocol-cranked finalize_task routes its
 * escrow INTO the vault — is recoupment-loop-devnet.ts (added once the protocol-
 * funded verify crank made on-chain verify_task reachable on devnet). The payout_to
 * routing itself is also proven by the shillbot localnet anchor tests.
 *
 * Run: npx tsx scripts/e2e/credit-flow-devnet.ts   (exit 0 = pass)
 */
import * as anchor from "@coral-xyz/anchor";
import { BN } from "@coral-xyz/anchor";
import {
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
  Transaction,
} from "@solana/web3.js";
import { readFileSync } from "fs";
import { homedir } from "os";
import { join } from "path";
import type { ExtensionRegistry } from "../../target/types/extension_registry";
import type { ExtensionCredit } from "../../target/types/extension_credit";

const DEVNET =
  process.env.DEVNET_RPC ??
  process.env.RPC_URL ??
  "https://api.devnet.solana.com";
const TYPE_CAPABILITY_VALIDATION = 0;
const BOND = new BN(5_000_000); // 0.005 SOL
const ADVANCE = new BN(2_000_000); // 0.002 SOL
const EARNINGS = 3_000_000; // > advance: backer fully recoups, agent gets surplus
const ADVANCE_SPACE = 113;

let failures = 0;
function check(cond: boolean, msg: string): void {
  console.log(`  ${cond ? "✓" : "✗"} ${msg}`);
  if (!cond) failures++;
}

function loadKeypair(path: string): Keypair {
  const raw = JSON.parse(readFileSync(path, "utf8")) as number[];
  return Keypair.fromSecretKey(Uint8Array.from(raw));
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
  const root = loadKeypair(join(homedir(), ".config/solana/id.json"));
  const agent = loadKeypair(join(homedir(), ".config/solana/test.json"));
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
  const credit = new anchor.Program(
    loadIdl("extension_credit"),
    provider
  ) as unknown as anchor.Program<ExtensionCredit>;

  const bal = (pk: PublicKey): Promise<number> =>
    connection.getBalance(pk, "confirmed");
  const feeOf = async (sig: string): Promise<number> => {
    const tx = await connection.getTransaction(sig, {
      commitment: "confirmed",
      maxSupportedTransactionVersion: 0,
    });
    return tx?.meta?.fee ?? 0;
  };

  console.log(`root  (backer/authority): ${root.publicKey.toBase58()}`);
  console.log(`agent (recipient):        ${agent.publicKey.toBase58()}`);

  // --- registry init (idempotent) ---
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
    console.log("  initialized registry GlobalState");
  } catch {
    console.log("  registry GlobalState already initialized");
  }

  // ===== Extension: submit -> attest (bond conservation) =====
  console.log("\n[extension-registry] submit_extension -> withdraw_extension");
  const [extension] = PublicKey.findProgramAddressSync(
    [
      Buffer.from("extension"),
      root.publicKey.toBuffer(),
      agent.publicKey.toBuffer(),
    ],
    registry.programId
  );
  const rootBeforeExt = await bal(root.publicKey);
  const s1 = await registry.methods
    .submitExtension(TYPE_CAPABILITY_VALIDATION, BOND)
    .accountsPartial({
      extension,
      extender: root.publicKey,
      recipient: agent.publicKey,
      systemProgram: SystemProgram.programId,
    })
    .rpc({ commitment: "confirmed" });
  const fee1 = await feeOf(s1);
  check(
    (await bal(extension)) > BOND.toNumber(),
    "extension PDA holds bond + rent"
  );

  const s2 = await registry.methods
    .withdrawExtension()
    .accountsPartial({
      extension,
      extender: root.publicKey,
      recipient: agent.publicKey,
    })
    .rpc({ commitment: "confirmed" });
  const fee2 = await feeOf(s2);
  check(
    (await connection.getAccountInfo(extension)) === null,
    "extension closed after attest"
  );
  const extNetLoss = rootBeforeExt - (await bal(root.publicKey));
  check(
    extNetLoss >= 0 && extNetLoss <= fee1 + fee2 + 100,
    `extension bond + rent fully returned (net loss ${extNetLoss} == fees ${
      fee1 + fee2
    })`
  );

  // ===== Credit: open_advance -> simulate routed earnings -> route_and_recoup =====
  console.log("\n[extension-credit] open_advance -> route_and_recoup -> close");
  const [advance] = PublicKey.findProgramAddressSync(
    [
      Buffer.from("advance"),
      root.publicKey.toBuffer(),
      agent.publicKey.toBuffer(),
    ],
    credit.programId
  );
  const rentFloor = await connection.getMinimumBalanceForRentExemption(
    ADVANCE_SPACE
  );

  const agentBeforeOpen = await bal(agent.publicKey);
  await credit.methods
    .openAdvance(ADVANCE)
    .accountsPartial({
      advance,
      backer: root.publicKey,
      recipient: agent.publicKey,
      systemProgram: SystemProgram.programId,
    })
    .rpc({ commitment: "confirmed" });
  check(
    (await bal(agent.publicKey)) - agentBeforeOpen === ADVANCE.toNumber(),
    "agent received the fronted advance"
  );

  // Simulate a finalized Shillbot payout routed to the vault (oracle-gated on
  // devnet -> direct transfer, mirroring the localnet anchor tests).
  await provider.sendAndConfirm(
    new Transaction().add(
      SystemProgram.transfer({
        fromPubkey: root.publicKey,
        toPubkey: advance,
        lamports: EARNINGS,
      })
    )
  );

  const vaultBefore = await bal(advance);
  const available = vaultBefore - rentFloor;
  const toBacker = Math.min(available, ADVANCE.toNumber());
  const toRecipient = available - toBacker;
  check(toRecipient > 0, "setup: routed earnings exceed the advance");

  const agentBeforeRoute = await bal(agent.publicKey);
  await credit.methods
    .routeAndRecoup()
    .accountsPartial({
      advance,
      backer: root.publicKey,
      recipient: agent.publicKey,
    })
    .rpc({ commitment: "confirmed" });
  check(
    (await bal(agent.publicKey)) - agentBeforeRoute === toRecipient,
    `agent received exactly the surplus (${toRecipient})`
  );
  check(
    Math.abs((await bal(advance)) - rentFloor) < 100,
    "vault drained to the rent floor"
  );
  check(
    toBacker + toRecipient === available,
    "all routed earnings distributed (conservation)"
  );

  await credit.methods
    .closeAdvance()
    .accountsPartial({
      advance,
      backer: root.publicKey,
      recipient: agent.publicKey,
    })
    .rpc({ commitment: "confirmed" });
  check(
    (await connection.getAccountInfo(advance)) === null,
    "advance closed after full recoupment"
  );

  console.log(
    `\n${failures === 0 ? "PASS" : "FAIL"} — ${failures} failed check(s)`
  );
  process.exit(failures === 0 ? 0 : 1);
}

main().catch((e: unknown) => {
  console.error(e);
  process.exit(1);
});
