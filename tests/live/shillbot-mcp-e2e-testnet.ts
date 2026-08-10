/**
 * Live MCP earn-lifecycle e2e — drives the DEPLOYED shillbot_* MCP tools exactly
 * as an autonomous agent would (mcp.swarm.tips streamable-http), NON-CUSTODIALLY:
 * every state-changing tool returns an unsigned base64 tx, which this script
 * signs locally with test.json and broadcasts via shillbot_submit_tx. The MCP
 * server never sees a private key.
 *
 *   register_wallet → list_available_tasks → get_task_details → claim → submit_work
 *   → verify_task → finalize_task → check_earnings
 *
 * The tool PATH (correct unsigned-tx construction, non-custodial signing, on-chain
 * confirmation, earnings update) is what this proves. The exact payout↔oracle
 * relation is pinned on-chain by the devnet harness (attested-lifecycle-devnet.ts)
 * and the bankrun matrix; here we cross-check it best-effort from get_task_details.
 *
 * SELF-SKIPS (exit 0) when it can't run: no test.json, MCP unreachable, or no
 * claimable devnet task in the marketplace. The read path (register + list) still
 * runs and is asserted before any skip.
 *
 * NOT CI (real devnet, spends SOL, mutates marketplace state). Run manually:
 *   MCP_URL=https://mcp.swarm.tips npx tsx tests/live/shillbot-mcp-e2e-testnet.ts
 */
import { Keypair, Transaction, VersionedTransaction } from "@solana/web3.js";
import { readFileSync } from "fs";
import { homedir } from "os";
import { join } from "path";
import { McpClient } from "./mcp-client";
import { computePayment } from "../../sdk/task-outcome-oracle";

const MCP_URL = process.env.MCP_URL ?? "https://mcp.swarm.tips";
// Devnet global defaults the harness + program initialize with.
const QUALITY_THRESHOLD = 200_000;
const PROTOCOL_FEE_BPS = 1000;

let failures = 0;
function check(cond: boolean, msg: string): void {
  console.log(`  ${cond ? "✓" : "✗"} ${msg}`);
  if (!cond) failures++;
}
function skip(msg: string): never {
  console.log(`\nSKIP — ${msg}`);
  process.exit(failures === 0 ? 0 : 1);
}
function loadKeypairOrNull(name: string): Keypair | null {
  try {
    return Keypair.fromSecretKey(
      Uint8Array.from(
        JSON.parse(
          readFileSync(join(homedir(), ".config/solana", name), "utf8")
        ) as number[]
      )
    );
  } catch {
    return null;
  }
}

/** Sign a base64 unsigned tx (legacy or versioned) with `signer`, return base64. */
function signBase64(unsigned: string, signer: Keypair): string {
  const raw = Buffer.from(unsigned, "base64");
  try {
    const vtx = VersionedTransaction.deserialize(Uint8Array.from(raw));
    vtx.sign([signer]);
    return Buffer.from(vtx.serialize()).toString("base64");
  } catch {
    const tx = Transaction.from(raw);
    tx.partialSign(signer);
    return tx.serialize({ requireAllSignatures: false }).toString("base64");
  }
}

interface TaskListItem {
  task_id: string;
  platform?: number;
  payment_amount?: number;
}

/**
 * A syntactically valid content_id for the task's platform.
 *
 * The orchestrator validates shape per platform at submit time
 * (shillbot-api `is_valid_content_id`): YouTube (0) is exactly 11
 * alphanumeric/-/_ chars, X (3) is up to 20 digits, Website (9) must be an
 * http(s) URL with a host, and everything else accepts any non-empty string.
 * We claim whichever task is first claimable, so a hardcoded YouTube id is
 * rejected outright whenever that task happens to be another platform.
 *
 * Shape only — verification is async and out of scope for this test, which
 * asserts the submit broadcast confirms.
 */
function sampleContentId(platform: number | undefined): string {
  switch (platform ?? 0) {
    case 0:
      return "dQw4w9WgXcQ";
    case 3:
      return "1234567890123456789";
    case 9:
      return "https://swarm.tips/e2e-mcp-lifecycle";
    default:
      return `e2e-mcp-${platform ?? 0}`;
  }
}

async function main(): Promise<void> {
  const agent = loadKeypairOrNull("test.json");
  if (!agent) skip("no test.json — cannot sign non-custodially");
  const wallet = agent.publicKey.toBase58();

  console.log(`MCP:    ${MCP_URL}`);
  console.log(`wallet: ${wallet}`);

  const mcp = new McpClient(MCP_URL);
  try {
    await mcp.connect();
  } catch (e) {
    skip(`MCP unreachable at ${MCP_URL}: ${String(e).slice(0, 120)}`);
  }
  check(true, "MCP session established over /mcp");

  // --- read path (always runs) ---
  const reg = await mcp.call("register_wallet", { pubkey: wallet });
  check(reg.status === "registered", "register_wallet: wallet registered");

  const listed = (await mcp.call("shillbot_list_available_tasks", {
    network: "devnet",
    limit: 25,
  })) as { tasks?: TaskListItem[] };
  const tasks = listed.tasks ?? [];
  check(Array.isArray(tasks), "list_available_tasks returned a task array");
  console.log(`  (${tasks.length} devnet task(s) listed)`);

  if (tasks.length === 0) {
    skip("no claimable devnet task in the marketplace — read path verified");
  }

  // The marketplace mixes fresh + stale-relic tasks (old layouts fail claim with
  // AccountDidNotDeserialize 3003; expired ones with ClaimBufferInsufficient).
  // Iterate past those to the first genuinely claimable task; failed claims are
  // simulation-only (no SOL spent). If none claim, self-skip — a fresh task must
  // be created via the orchestrator to exercise the write lifecycle.
  let task: TaskListItem | null = null;
  for (const candidate of tasks) {
    try {
      await signSubmit(mcp, agent, "claim", "shillbot_claim_task", {
        task_id: candidate.task_id,
        network: "devnet",
      });
      task = candidate;
      break;
    } catch (e) {
      const msg = String(e);
      if (
        /3003|AccountDidNotDeserialize|ClaimBuffer|InvalidTaskState/.test(msg)
      ) {
        console.log(`  · task ${candidate.task_id} not claimable (skipping)`);
        continue;
      }
      throw e;
    }
  }
  if (!task) {
    skip(
      "all listed devnet tasks are stale/unclaimable — read + claim path verified; " +
        "create a fresh task via the orchestrator for the full write lifecycle"
    );
  }
  check(true, `claim broadcast + confirmed (task ${task.task_id})`);

  await mcp.call("shillbot_get_task_details", {
    task_id: task.task_id,
    network: "devnet",
  });

  await signSubmit(mcp, agent, "submit", "shillbot_submit_work", {
    task_id: task.task_id,
    content_id: sampleContentId(task.platform),
    network: "devnet",
  });
  check(true, "submit_work broadcast + confirmed");

  // verify → finalize are gated on the ASYNC verifier: a kind-0 (marketing) task
  // is only verifiable once the verifier has produced a scoring_result (Switchboard
  // scoring at T+delay). Immediately after submit_work that hasn't happened, so a
  // 409 "no scoring_result" is the EXPECTED async boundary — the claim+submit
  // non-custodial writes are what this run proves live. The verify→finalize payout
  // path is proven end-to-end (no async wait) by the attested devnet harness.
  try {
    await signSubmit(mcp, agent, "verify", "shillbot_verify_task", {
      task_id: task.task_id,
      network: "devnet",
    });
  } catch (e) {
    const msg = String(e);
    if (/scoring_result|has not scored|409|Conflict/.test(msg)) {
      console.log(
        "  · verify/finalize gated on async verifier scoring (T+delay) — claim+submit proven live"
      );
      // SKIP, not PASS. This path exits having proven claim+submit only; it
      // never reaches a payout, so calling it PASS reports settlement coverage
      // the run does not have.
      console.log(
        `\nSKIP — verify/finalize not reached; ${failures} failed check(s)`
      );
      process.exit(failures === 0 ? 0 : 1);
    }
    throw e;
  }
  check(true, "verify_task broadcast + confirmed");

  // Best-effort metamorphic cross-check: the verified task's payment matches the
  // oracle for its realized score (exact on-chain assertion is in the devnet
  // harness; get_task_details may not expose the raw score).
  const details = (await mcp.call("shillbot_get_task_details", {
    task_id: task.task_id,
    network: "devnet",
  })) as {
    composite_score?: number;
    escrow_lamports?: number;
    payment_amount?: number;
  };
  if (
    typeof details.composite_score === "number" &&
    typeof details.escrow_lamports === "number" &&
    typeof details.payment_amount === "number"
  ) {
    const expected = computePayment(
      details.composite_score,
      QUALITY_THRESHOLD,
      BigInt(details.escrow_lamports),
      PROTOCOL_FEE_BPS
    );
    check(
      BigInt(details.payment_amount) === expected.payment,
      `payment == oracle computePayment for score ${details.composite_score}`
    );
    // The equality above is satisfied by 0 == computePayment(0, ...): a
    // REJECTED task scores 0, refunds the client, and matches the oracle
    // perfectly. Without this line the whole block passes on a non-payout.
    check(
      BigInt(details.payment_amount) > 0n,
      `payment_amount > 0 (got ${details.payment_amount}, score ${details.composite_score})`
    );
  } else {
    // Not a skip. If the details response stops exposing the score, this test
    // silently stops checking settlement while still reporting PASS — the
    // failure mode is indistinguishable from success, so fail loudly instead.
    check(
      false,
      "get_task_details must expose composite_score/escrow_lamports/payment_amount " +
        "— without them nothing here verifies the payout"
    );
  }

  const earnBefore = await earnings(mcp, wallet);
  await signSubmit(mcp, agent, "finalize", "shillbot_finalize_task", {
    task_id: task.task_id,
    network: "devnet",
  });
  check(true, "finalize_task broadcast + confirmed");
  const earnAfter = await earnings(mcp, wallet);
  check(
    earnAfter > earnBefore,
    `check_earnings INCREASED (${earnBefore} → ${earnAfter}) — ` +
      `non-decreasing was satisfied by being paid nothing at all`
  );

  console.log(
    `\n${failures === 0 ? "PASS" : "FAIL"} — ${failures} failed check(s)`
  );
  process.exit(failures === 0 ? 0 : 1);
}

/** Call a state-changing tool, sign its unsigned_tx locally, broadcast via submit_tx. */
async function signSubmit(
  mcp: McpClient,
  signer: Keypair,
  action: string,
  tool: string,
  args: Record<string, unknown>
): Promise<void> {
  const built = (await mcp.call(tool, args)) as { unsigned_tx?: string };
  if (!built.unsigned_tx) throw new Error(`${tool} returned no unsigned_tx`);
  const signed = signBase64(built.unsigned_tx, signer);
  await mcp.call("shillbot_submit_tx", {
    task_id: args.task_id,
    action,
    signed_transaction: signed,
    network: "devnet",
  });
}

async function earnings(mcp: McpClient, wallet: string): Promise<number> {
  const e = (await mcp.call("shillbot_check_earnings", {
    wallet,
    network: "devnet",
  })) as { total_earned?: number; total_earned_lamports?: number };
  return Number(e.total_earned ?? e.total_earned_lamports ?? 0);
}

main().catch((e: unknown) => {
  console.error(e);
  process.exit(1);
});
