/**
 * Devnet e2e — the FULL $0-agent capital recoupment loop, end to end, with REAL
 * protocol settlement (no simulated transfer). This is the unified test that
 * stitches together the three links previously proven only in isolation:
 *   - shillbot bankrun: finalize_task routes a non-default payout_to to a vault
 *   - credit-flow-devnet.ts: open_advance + route_and_recoup + conservation,
 *     but with the vault deposit SIMULATED by a direct transfer (verify_task was
 *     oracle-gated on devnet)
 *   - verify-crank-devnet.ts: the protocol cranks verify_task + finalizes a task
 *     the agent never touches, but with payout_to = default (paid the agent)
 *
 * Here the advance vault is funded by a REAL finalized task:
 *   open_advance (backer fronts capital)
 *   -> claim a game-play task via the deployed api.shillbot.org (agent)
 *   -> set_payout_to(advance vault)  [agent-signed, while Claimed]
 *   -> submit -> the protocol cranks verify_task + finalizes (agent does nothing)
 *   -> finalize_task routes the escrow INTO the advance vault (payout_to)
 *   -> route_and_recoup splits the vault backer-first, agent keeps the surplus
 * Proves the whole mund-creanc-witer capital loop against live devnet programs.
 *
 * Roles: id.json = root (backer); test.json = agent (claims the task, receives
 * the recouped surplus). The agent pays its own claim/submit gas — the gasless/$0
 * entry path is covered by sponsored-claim-devnet.ts + zero-funds-onboarding-
 * devnet.ts; this test isolates the capital recoupment loop with real settlement.
 *
 * Requires an OPEN game-play (platform 5) task: the protocol crank only fires for
 * API-seeded tasks with an off-chain verification workflow, and game-play is the
 * binary fast-lane that scores from a resolved game_id (~5-min verify delay). If
 * none is open, the script exits 2 (SKIP) WITHOUT opening an advance — seed one
 * via the coordination game first.
 *
 * Run: npx tsx scripts/e2e/recoupment-loop-devnet.ts   (0 = pass, 1 = fail, 2 = skip)
 */
import * as anchor from "@coral-xyz/anchor";
import { BN } from "@coral-xyz/anchor";
import {
  Connection,
  Keypair,
  PublicKey,
  VersionedTransaction,
} from "@solana/web3.js";
import { readFileSync } from "fs";
import { homedir } from "os";
import { join } from "path";
import type { ExtensionCredit } from "../../target/types/extension_credit";
import type { Shillbot } from "../../target/types/shillbot";

const API = "https://api.shillbot.org";
const DEVNET = "https://api.devnet.solana.com";
const COORD_GAME = new PublicKey(
  "2qqVk7kUqffnahiJpcQJCsSd8ErbEUgKTgCn1zYsw64P"
);
const SHILLBOT = new PublicKey("2tR37nqMpwdV4DVUHjzUmL1rH2DtkA8zrRA4EAhT7KMi");
// Keep the advance well under a game-play escrow so the backer fully recoups and
// the agent is left a visible surplus (the interesting, fully-conserved case).
const ADVANCE = new BN(300_000); // 0.0003 SOL
const ADVANCE_SPACE = 113;

let failures = 0;
function check(cond: boolean, msg: string): void {
  console.log(`  ${cond ? "✓" : "✗"} ${msg}`);
  if (!cond) failures++;
}

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

async function api(
  path: string,
  bearer: string,
  body?: unknown
): Promise<Response> {
  const sep = path.includes("?") ? "&" : "?";
  return fetch(`${API}${path}${sep}network=devnet`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${bearer}`,
      "Content-Type": "application/json",
    },
    body: body ? JSON.stringify(body) : undefined,
  });
}

async function signSendConfirm(
  connection: Connection,
  txB64: string,
  agent: Keypair
): Promise<string> {
  const vtx = VersionedTransaction.deserialize(Buffer.from(txB64, "base64"));
  vtx.sign([agent]);
  const sig = await connection.sendRawTransaction(vtx.serialize(), {
    maxRetries: 5,
  });
  await connection.confirmTransaction(sig, "confirmed");
  return sig;
}

// The API does not expose the on-chain task PDA, so extract it from the claim tx:
// claim_task's first account is the Task PDA. Handles both v0 and legacy messages.
function extractTaskPda(txB64: string): PublicKey {
  const vtx = VersionedTransaction.deserialize(Buffer.from(txB64, "base64"));
  const msg = vtx.message as unknown as {
    staticAccountKeys?: PublicKey[];
    accountKeys?: PublicKey[];
    compiledInstructions?: {
      programIdIndex: number;
      accountKeyIndexes: number[];
    }[];
    instructions?: { programIdIndex: number; accounts: number[] }[];
  };
  const keys = msg.staticAccountKeys ?? msg.accountKeys ?? [];
  const v0 = msg.compiledInstructions ?? [];
  const legacy = msg.instructions ?? [];
  for (const ix of v0) {
    if (keys[ix.programIdIndex]?.equals(SHILLBOT))
      return keys[ix.accountKeyIndexes[0]];
  }
  for (const ix of legacy) {
    if (keys[ix.programIdIndex]?.equals(SHILLBOT)) return keys[ix.accounts[0]];
  }
  throw new Error("no shillbot instruction found in claim tx");
}

async function pickFreshGameId(connection: Connection): Promise<string | null> {
  const [counterPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("game_counter")],
    COORD_GAME
  );
  const info = await connection.getAccountInfo(counterPda);
  if (!info || info.data.length < 16) return null;
  const count = info.data.readBigUInt64LE(8);
  const MAX_SCAN = 48;
  const RESOLVED = 4;
  const STATE_OFFSET = 88;
  for (let i = 1n; i <= BigInt(MAX_SCAN); i++) {
    if (count <= i) break;
    const candidate = count - i;
    const [gamePda] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("game"),
        Buffer.from(new BigUint64Array([candidate]).buffer),
      ],
      COORD_GAME
    );
    const g = await connection.getAccountInfo(gamePda);
    if (!g || g.data.length < STATE_OFFSET + 1) continue;
    if (g.data[STATE_OFFSET] === RESOLVED) return candidate.toString();
  }
  return null;
}

async function main(): Promise<void> {
  const root = loadKeypair(join(homedir(), ".config/solana/id.json"));
  const agent = loadKeypair(join(homedir(), ".config/solana/test.json"));
  const bearer = agent.publicKey.toBase58();
  const connection = new Connection(DEVNET, "confirmed");
  const provider = new anchor.AnchorProvider(
    connection,
    new anchor.Wallet(root),
    {
      commitment: "confirmed",
    }
  );
  anchor.setProvider(provider);
  const credit = new anchor.Program(
    loadIdl("extension_credit"),
    provider
  ) as unknown as anchor.Program<ExtensionCredit>;
  const shillbot = new anchor.Program(
    loadIdl("shillbot"),
    provider
  ) as unknown as anchor.Program<Shillbot>;
  const bal = (pk: PublicKey): Promise<number> =>
    connection.getBalance(pk, "confirmed");

  console.log(`root  (backer):    ${root.publicKey.toBase58()}`);
  console.log(`agent (recipient): ${bearer}`);

  // --- 0. Need an open game-play task + a resolved game_id, else SKIP cleanly. ---
  const gameId = await pickFreshGameId(connection);
  if (!gameId) {
    console.log(
      "\nSKIP — no Resolved game found to submit; seed the coordination game."
    );
    process.exit(2);
  }
  const open = (
    (await (await fetch(`${API}/tasks?network=devnet&limit=100`)).json()) as {
      tasks: Array<{ task_id: string; platform: number; state: string }>;
    }
  ).tasks.filter((t) => t.platform === 5 && (t.state === "open" || !t.state));
  console.log(
    `  ${open.length} open game-play task(s); picked game_id=${gameId}`
  );
  if (open.length === 0) {
    console.log(
      "\nSKIP — no open game-play (platform 5) task; seed one via the coordination game."
    );
    process.exit(2);
  }

  // --- 1. Claim a game-play task via the API (agent pays its own gas). ---
  let taskId = "";
  let taskPda: PublicKey | null = null;
  for (const t of open) {
    const claimResp = await api(`/tasks/${t.task_id}/claim`, bearer);
    if (!claimResp.ok) continue;
    const { transaction } = (await claimResp.json()) as {
      transaction?: string;
    };
    if (!transaction) continue;
    try {
      taskPda = extractTaskPda(transaction);
      const sig = await signSendConfirm(connection, transaction, agent);
      const cf = await api(`/tasks/${t.task_id}/confirm`, bearer, {
        tx_signature: sig,
        action: "claim",
      });
      if (!cf.ok) continue;
      taskId = t.task_id;
      break;
    } catch (e) {
      console.log(
        `  claim ${t.task_id} threw: ${e instanceof Error ? e.message : e}`
      );
    }
  }
  if (!taskId || !taskPda)
    throw new Error("could not claim any open game-play task");
  console.log(`  claimed ${taskId}  (task PDA ${taskPda.toBase58()})`);

  // --- 2. open_advance: backer fronts capital to the agent's vault. ---
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
      systemProgram: anchor.web3.SystemProgram.programId,
    })
    .rpc({ commitment: "confirmed" });
  check(
    (await bal(agent.publicKey)) - agentBeforeOpen === ADVANCE.toNumber(),
    "agent received the fronted advance"
  );
  const vaultAfterOpen = await bal(advance);

  // --- 3. set_payout_to(advance vault) — agent-signed, while Claimed. ---
  await shillbot.methods
    .setPayoutTo(advance)
    .accountsPartial({ task: taskPda, agent: agent.publicKey })
    .signers([agent])
    .rpc({ commitment: "confirmed" });
  const taskAcct = await shillbot.account.task.fetch(taskPda);
  check(
    (taskAcct.payoutTo as PublicKey).equals(advance),
    "task.payout_to locked to the advance vault"
  );

  // --- 4. submit -> the protocol cranks + finalizes; the agent does NOTHING. ---
  const sResp = await api(`/tasks/${taskId}/submit`, bearer, {
    content_id: gameId,
  });
  if (!sResp.ok) throw new Error(`submit failed: ${await sResp.text()}`);
  const { transaction: sTx } = (await sResp.json()) as { transaction?: string };
  if (!sTx) throw new Error("submit returned no tx");
  const sSig = await signSendConfirm(connection, sTx, agent);
  let submitted = false;
  for (let i = 0; i < 5 && !submitted; i++) {
    if (i > 0) await new Promise((r) => setTimeout(r, 3000));
    const cf = await api(`/tasks/${taskId}/confirm`, bearer, {
      tx_signature: sSig,
      action: "submit",
    });
    if (cf.ok) submitted = true;
  }
  if (!submitted) throw new Error("submit-confirm failed");
  console.log(
    "  submitted; agent now does NOTHING — protocol verifies + finalizes"
  );

  // --- 5. Poll until finalized (no verify/finalize action from us). Budget 16m. ---
  const start = Date.now();
  let state = "";
  while (Date.now() - start < 16 * 60 * 1000) {
    const td = (await (
      await fetch(`${API}/tasks/${taskId}?network=devnet`)
    ).json()) as { state: string };
    if (td.state !== state) {
      state = td.state;
      console.log(
        `  [${Math.round((Date.now() - start) / 1000)}s] state=${state}`
      );
    }
    if (state === "finalized") break;
    await new Promise((r) => setTimeout(r, 4000));
  }
  check(
    state === "finalized",
    "task finalized by the protocol crank (agent idle)"
  );

  // --- 6. The REAL finalize routed the escrow INTO the vault (not simulated). ---
  const vaultAfterFinalize = await bal(advance);
  const routedIn = vaultAfterFinalize - vaultAfterOpen;
  check(
    routedIn > 0,
    `finalize routed the escrow into the advance vault (+${routedIn} lamports)`
  );

  // --- 7. route_and_recoup: backer-first split, agent keeps the surplus. ---
  const available = vaultAfterFinalize - rentFloor;
  const toBacker = Math.min(available, ADVANCE.toNumber());
  const toRecipient = available - toBacker;
  const agentBeforeRoute = await bal(agent.publicKey);
  const backerBeforeRoute = await bal(root.publicKey);
  const rrSig = await credit.methods
    .routeAndRecoup()
    .accountsPartial({
      advance,
      backer: root.publicKey,
      recipient: agent.publicKey,
    })
    .rpc({ commitment: "confirmed" });
  const rrFee =
    (
      await connection.getTransaction(rrSig, {
        commitment: "confirmed",
        maxSupportedTransactionVersion: 0,
      })
    )?.meta?.fee ?? 0;
  check(
    (await bal(agent.publicKey)) - agentBeforeRoute === toRecipient,
    `agent received exactly the surplus (${toRecipient})`
  );
  check(
    (await bal(root.publicKey)) - backerBeforeRoute === toBacker - rrFee,
    `backer recouped ${toBacker} (net of the ${rrFee} route fee it paid)`
  );
  check(
    toBacker + toRecipient === available,
    "all routed earnings distributed (conservation)"
  );
  check(
    Math.abs((await bal(advance)) - rentFloor) < 100,
    "vault drained to the rent floor"
  );

  // --- 8. close the fully-recouped advance (rent back to the backer). ---
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
    `\n${
      failures === 0 ? "PASS" : "FAIL"
    } — full $0 recoupment loop with REAL protocol settlement; ${failures} failed check(s)`
  );
  process.exit(failures === 0 ? 0 : 1);
}

main().catch((e: unknown) => {
  console.error(e instanceof Error ? e.message : e);
  process.exit(1);
});
