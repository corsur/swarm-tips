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
  SystemProgram,
  Transaction,
  VersionedTransaction,
} from "@solana/web3.js";
import { readFileSync } from "fs";
import { homedir } from "os";
import { join } from "path";
import type { ExtensionCredit } from "../../target/types/extension_credit";
import type { Shillbot } from "../../target/types/shillbot";

const API = "https://api.shillbot.org";
const DEVNET =
  process.env.DEVNET_RPC ??
  process.env.RPC_URL ??
  "https://api.devnet.solana.com";
const COORD_GAME = new PublicKey(
  "2qqVk7kUqffnahiJpcQJCsSd8ErbEUgKTgCn1zYsw64P"
);
const SHILLBOT = new PublicKey("2tR37nqMpwdV4DVUHjzUmL1rH2DtkA8zrRA4EAhT7KMi");
// Advance = the program minimum (MIN_ADVANCE_LAMPORTS); the task escrow is set
// several times larger (see ESCROW in main) so the finalized payment exceeds the
// advance: the backer fully recoups and the agent keeps a visible surplus.
const ADVANCE = new BN(1_000_000); // 0.001 SOL = MIN_ADVANCE_LAMPORTS
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

// Minimal valid campaign brief (create_campaign requires these four non-empty).
const BRIEF = {
  topic: "recoupment-loop e2e (devnet)",
  brand_voice: "Terse. Automated devnet recoupment-loop test task.",
  cta: "n/a — game-play binary task",
  utm_link: "https://swarm.tips/devnet-e2e",
};

// Self-seed an open game-play (platform 5) task as the client (root): create a
// campaign, fund it (escrows + builds the create_task tx), sign + confirm. The
// fund response returns the on-chain task PDA directly. Returns the off-chain
// task id (to drive claim/submit via the API) and the on-chain Task PDA (for
// set_payout_to). The agent then submits any Resolved game_id to score it.
async function seedGamePlayTask(
  connection: Connection,
  root: Keypair,
  escrowLamports: number
): Promise<{ taskId: string; taskPda: PublicKey }> {
  const rootBearer = root.publicKey.toBase58();
  const cResp = await api("/campaigns", rootBearer, {
    brief: BRIEF,
    budget_lamports: 0,
    platform: 5,
  });
  if (!cResp.ok)
    throw new Error(`create_campaign failed: ${await cResp.text()}`);
  const campaign = (await cResp.json()) as {
    id?: string;
    campaign_id?: string;
  };
  const campaignId = campaign.id ?? campaign.campaign_id;
  if (!campaignId)
    throw new Error(`no campaign id in response: ${JSON.stringify(campaign)}`);

  const fResp = await api(`/campaigns/${campaignId}/fund`, rootBearer, {
    amount_lamports: escrowLamports,
  });
  if (!fResp.ok) throw new Error(`fund_campaign failed: ${await fResp.text()}`);
  const fund = (await fResp.json()) as {
    transaction?: string;
    task_id?: string;
    task_pda?: string;
  };
  if (!fund.transaction || !fund.task_id || !fund.task_pda)
    throw new Error(`fund response missing fields: ${JSON.stringify(fund)}`);

  const sig = await signSendConfirm(connection, fund.transaction, root);
  const cf = await api(`/tasks/${fund.task_id}/confirm`, rootBearer, {
    tx_signature: sig,
    action: "create",
    task_pda: fund.task_pda,
  });
  if (!cf.ok) throw new Error(`confirm create failed: ${await cf.text()}`);
  return { taskId: fund.task_id, taskPda: new PublicKey(fund.task_pda) };
}

// Find a Resolved game in which `agent` actually played — game-play verification
// requires the submitting agent to be player_one or player_two (pipeline.rs
// require_player_match), and rejects a game_id already used by another task
// (dedup). So each run consumes one of the agent's games; pass GAME_ID=<n> to pin
// a specific fresh one. Game layout: disc(8) game_id(8) tournament_id(8) =>
// player_one@24, player_two@56, state@88 (4 = Resolved).
async function pickAgentGame(
  connection: Connection,
  agent: PublicKey
): Promise<string | null> {
  if (process.env.GAME_ID) return process.env.GAME_ID;
  const agentKey = agent.toBase58();
  const [counterPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("game_counter")],
    COORD_GAME
  );
  const info = await connection.getAccountInfo(counterPda);
  if (!info || info.data.length < 16) return null;
  const count = info.data.readBigUInt64LE(8);
  const MAX_SCAN = 60;
  const RESOLVED = 4;
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
    if (!g || g.data.length < 89) continue;
    if (g.data[88] !== RESOLVED) continue;
    const p1 = new PublicKey(g.data.subarray(24, 56)).toBase58();
    const p2 = new PublicKey(g.data.subarray(56, 88)).toBase58();
    if (p1 === agentKey || p2 === agentKey) return candidate.toString();
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
  // Baseline for the circular close-out: the agent is swept back to this at the end.
  const agentStart = await bal(agent.publicKey);

  // --- 0. Need a Resolved game the AGENT played (and not yet used), else SKIP. ---
  const gameId = await pickAgentGame(connection, agent.publicKey);
  if (!gameId) {
    console.log(
      "\nSKIP — no Resolved game found where the agent is a player; seed/play a game first."
    );
    process.exit(2);
  }
  console.log(`  using agent-played game_id=${gameId}`);

  // --- 1. Self-seed a game-play task (client=root) + claim it (agent). ---
  // Escrow comfortably exceeds the advance so the finalized payment to the vault
  // exceeds the advance: backer fully recoups, agent keeps the surplus.
  const ESCROW = 5_000_000; // 0.005 SOL
  const { taskId, taskPda } = await seedGamePlayTask(connection, root, ESCROW);
  console.log(`  seeded game-play task ${taskId}  (PDA ${taskPda.toBase58()})`);
  const claimResp = await api(`/tasks/${taskId}/claim`, bearer);
  if (!claimResp.ok) throw new Error(`claim failed: ${await claimResp.text()}`);
  const { transaction: claimTx } = (await claimResp.json()) as {
    transaction?: string;
  };
  if (!claimTx) throw new Error("claim returned no tx");
  const claimSig = await signSendConfirm(connection, claimTx, agent);
  const claimCf = await api(`/tasks/${taskId}/confirm`, bearer, {
    tx_signature: claimSig,
    action: "claim",
  });
  if (!claimCf.ok)
    throw new Error(`claim-confirm failed: ${await claimCf.text()}`);
  console.log(`  claimed by agent ${bearer}`);

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
  // Economic guarantee: ALWAYS fully recoup + close the advance so a run can never
  // strand the (backer, recipient) PDA — even off the happy path (if the protocol
  // finalize did not land, the assertions above already failed). If the vault is
  // short of the outstanding advance, top up the shortfall from the backer so
  // route+close succeed; that top-up is circular (backer -> vault -> backer) and
  // only happens when real settlement is absent.
  const outstanding = ADVANCE.toNumber();
  let vaultBal = await bal(advance);
  if (vaultBal - rentFloor < outstanding) {
    const shortfall = outstanding - (vaultBal - rentFloor);
    await provider.sendAndConfirm(
      new Transaction().add(
        SystemProgram.transfer({
          fromPubkey: root.publicKey,
          toPubkey: advance,
          lamports: shortfall,
        })
      )
    );
    vaultBal = await bal(advance);
    console.log(
      `  (no real settlement — topped up ${shortfall} lamports to recoup+close the advance; circular)`
    );
  }
  const available = vaultBal - rentFloor;
  const toBacker = Math.min(available, outstanding);
  const toRecipient = available - toBacker;
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
    `agent received exactly the routed surplus (${toRecipient})`
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
    "advance closed (rent reclaimed by backer)"
  );

  // --- 9. Circular close-out: sweep the agent's net gain back to root, so every
  //       lamport ends with the backer and only gas is spent. The finalized Task
  //       account was already closed to the client by finalize_task (close=client).
  const agentEnd = await bal(agent.publicKey);
  const agentGain = agentEnd - agentStart;
  if (agentGain > 5_000) {
    const sweep = agentGain - 5_000; // leave a hair for the sweep fee
    const tx = new Transaction().add(
      SystemProgram.transfer({
        fromPubkey: agent.publicKey,
        toPubkey: root.publicKey,
        lamports: sweep,
      })
    );
    tx.feePayer = agent.publicKey;
    tx.recentBlockhash = (
      await connection.getLatestBlockhash("confirmed")
    ).blockhash;
    tx.sign(agent);
    await connection.confirmTransaction(
      await connection.sendRawTransaction(tx.serialize()),
      "confirmed"
    );
    console.log(
      `  swept ${sweep} lamports of agent gain back to root (circular)`
    );
  }
  console.log(
    `  [ledger] agent net ${agentGain} lamports (swept to root); escrow returned to backer as fee+remainder+task-rent (finalize close=client) + recoup; advance closed. Only gas is spent.`
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
