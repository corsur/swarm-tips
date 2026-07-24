/**
 * MAINNET LeanProof e2e via mcp.swarm.tips (non-custodial): register → claim →
 * submit a real proof. Agent = worker wallet (NOT the attester/client, clears
 * both on-chain guards). verify+finalize run server-side (attestation-pipeline).
 *   AGENT_KEYPAIR=… TASK_ID=… PROOF_URL=… npx tsx tests/live/shillbot-mcp-lean-mainnet.ts
 */
import { Keypair, Transaction, VersionedTransaction } from "@solana/web3.js";
import { readFileSync } from "fs";
import { McpClient } from "./mcp-client";

const MCP_URL = process.env.MCP_URL ?? "https://mcp.swarm.tips";
const NETWORK = "mainnet";
const TASK_ID = process.env.TASK_ID!;
const PROOF_URL = process.env.PROOF_URL!;
const agent = Keypair.fromSecretKey(
  Uint8Array.from(JSON.parse(readFileSync(process.env.AGENT_KEYPAIR!, "utf8")))
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
  console.log(
    "→ attestation-pipeline now runs server-side (attest → challenge → finalize). Poll task state / agent balance."
  );
}

main().catch((e) => {
  console.error("FAIL:", e instanceof Error ? e.message : e);
  process.exit(1);
});
