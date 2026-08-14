/**
 * MAINNET LeanProof e2e via mcp.swarm.tips (non-custodial): register → claim →
 * submit a real proof → WAIT FOR SETTLEMENT AND ASSERT THE AGENT WAS PAID.
 * Agent = worker wallet (NOT the attester/client, clears both on-chain guards).
 * verify+finalize run server-side (attestation-pipeline).
 *   AGENT_KEYPAIR=… TASK_ID=… PROOF_URL=… npx tsx tests/live/shillbot-mcp-lean-mainnet.ts
 *
 * This is the ONLY mainnet LeanProof path — the attester cells are devnet — so
 * it is worth keeping, but it previously ended at "submit_work broadcast" and
 * printed "Poll task state / agent balance". It asserted NOTHING. A proof the
 * runner rejects scores 0, refunds the client, and still reaches `finalized`;
 * this script exited 0 either way, on mainnet, having spent real SOL on the
 * claim. The settlement wait below is the whole point of the file.
 */
import { Keypair, Transaction, VersionedTransaction } from "@solana/web3.js";
import { readFileSync } from "fs";
import { McpClient } from "./mcp-client";

const MCP_URL = process.env.MCP_URL ?? "https://mcp.swarm.tips";
const NETWORK = "mainnet";
/** Required env, rejected at the boundary. Non-null assertions turned an unset
 *  var into the string "undefined" reaching the MCP server, or a
 *  readFileSync(undefined) stack — both after connecting, neither naming the
 *  actual mistake. */
function required(name: string): string {
  const v = process.env[name];
  if (!v)
    throw new Error(
      `${name} is required. Usage: AGENT_KEYPAIR=<path> TASK_ID=<id> ` +
        `PROOF_URL=<https url> npx tsx tests/live/shillbot-mcp-lean-mainnet.ts`
    );
  return v;
}

const TASK_ID = required("TASK_ID");
const PROOF_URL = required("PROOF_URL");
const agent = Keypair.fromSecretKey(
  Uint8Array.from(JSON.parse(readFileSync(required("AGENT_KEYPAIR"), "utf8")))
);
const wallet = agent.publicKey.toBase58();

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

async function signSubmit(
  mcp: McpClient,
  action: string,
  tool: string,
  args: Record<string, unknown>
): Promise<unknown> {
  const built = (await mcp.call(tool, args)) as {
    unsigned_tx?: string;
    transaction?: string;
  };
  const unsigned = built.unsigned_tx ?? built.transaction;
  if (!unsigned)
    throw new Error(
      `${tool} returned no tx: ${JSON.stringify(built).slice(0, 200)}`
    );
  const signed = signBase64(unsigned, agent);
  return mcp.call("shillbot_submit_tx", {
    task_id: args.task_id,
    action,
    signed_transaction: signed,
    network: NETWORK,
  });
}

async function main(): Promise<void> {
  console.log(`MCP: ${MCP_URL}\nagent: ${wallet}\ntask: ${TASK_ID}`);
  const mcp = new McpClient(MCP_URL);
  await mcp.connect();
  const reg = (await mcp.call("register_wallet", { pubkey: wallet })) as {
    status?: string;
  };
  console.log(`register_wallet → ${reg.status}`);
  const details = (await mcp.call("shillbot_get_task_details", {
    task_id: TASK_ID,
    network: NETWORK,
  })) as { platform?: number; state?: string; lean_policy?: number };
  console.log(
    `task → platform ${details.platform} state ${details.state} policy ${details.lean_policy}`
  );
  await signSubmit(mcp, "claim", "shillbot_claim_task", {
    task_id: TASK_ID,
    network: NETWORK,
  });
  console.log("✓ claim broadcast + confirmed");
  await signSubmit(mcp, "submit", "shillbot_submit_work", {
    task_id: TASK_ID,
    content_id: PROOF_URL,
    network: NETWORK,
  });
  console.log("✓ submit_work broadcast + confirmed (proof submitted)");

  // The attestation pipeline runs server-side (attest → challenge → finalize).
  // Wait for it and assert the SETTLEMENT, not the submission.
  console.log("waiting for settlement (attest → challenge → finalize)…");
  const deadline = Date.now() + SETTLE_TIMEOUT_MS;
  let last = "";
  let settled: LeanTaskDetails = {};
  while (Date.now() < deadline) {
    settled = (await mcp.call("shillbot_get_task_details", {
      task_id: TASK_ID,
      network: NETWORK,
    })) as LeanTaskDetails;
    const state = String(settled.state ?? "?");
    if (state !== last) {
      console.log(`  state → ${state}`);
      last = state;
    }
    if (state === "finalized" || state === "expired") break;
    await new Promise((r) => setTimeout(r, 20_000));
  }

  const state = String(settled.state ?? "?");
  if (state !== "finalized") {
    throw new Error(
      `task did not finalize within ${
        SETTLE_TIMEOUT_MS / 60_000
      }m (state=${state}) — ` +
        `the proof was submitted but the attestation pipeline never settled it`
    );
  }

  const score = Number(settled.composite_score ?? 0);
  const payment = Number(settled.payment_amount ?? 0);
  // The PAYMENT is the pass condition. The orchestrator mirror never
  // backfills composite_score for attested-path tasks (verified live
  // 2026-08-14 on BOTH networks: paid tasks finalize with score=null/0 while
  // payment_amount carries the real payout — the mainnet spot-check paid
  // 1_800_000 lamports, balance-verified, yet the old score>0 gate FAILed it).
  // A rejected proof pays 0, so payment>0 is the discriminator; score stays
  // in the message as a diagnostic.
  if (!(payment > 0)) {
    throw new Error(
      `proof REJECTED: score=${score} payment=${payment}. The task still reached ` +
        `\`finalized\` and the client was refunded — reaching a terminal state is ` +
        `never evidence of payment. Check the artifact contains the THEOREM ONLY ` +
        `(the runner prepends the campaign statement).`
    );
  }

  console.log(
    `PASS — mainnet LeanProof settled and PAID: state=${state} score=${score} ` +
      `payment=${payment} lamports to ${wallet}`
  );
}

/** Server-side attest + 1h challenge window + finalize. Generous: a cold
 *  lean-runner plus a mathlib elaboration is minutes on its own. */
const SETTLE_TIMEOUT_MS = 90 * 60 * 1000;

interface LeanTaskDetails {
  state?: string;
  composite_score?: number | null;
  payment_amount?: number | null;
}

main().catch((e) => {
  console.error("FAIL:", e instanceof Error ? e.message : e);
  process.exit(1);
});
