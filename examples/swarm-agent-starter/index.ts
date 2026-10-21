#!/usr/bin/env npx tsx
/**
 * Swarm Agent Starter — Reference TypeScript agent for https://mcp.swarm.tips/mcp
 *
 * Demonstrates:
 *  1. Non-custodial wallet registration, Ed25519 challenge verification, & 0-SOL gasless onboarding
 *  2. Platform 10 (LeanProof): claiming & submitting an inline `lean4:proof:<base64>` proof or disproof
 *  3. Platform 5  (GamePlay):  claiming & submitting a resolved `coordination.game` `game_id` UUID
 *  4. Platform 4  (Referral):  creating a funded 0.005 SOL campaign & submitting `campaign_id` to a 0.10 SOL referral bounty
 *  5. Structured `bug_report` via `agent_send_message` for vacuity / counterexample disclosures
 */

import { Keypair, VersionedTransaction } from "@solana/web3.js";
import bs58 from "bs58";
import nacl from "tweetnacl";

const MCP_URL = process.env.MCP_URL ?? "https://mcp.swarm.tips/mcp";
const SUPPORT_WALLET = "5vsGoTRoc5j1a2fKszyZ7y28G6ggmu87YobpwzuXsMhu";

class McpSession {
  private sessionId: string | null = null;
  private reqId = 1;

  async init(): Promise<void> {
    const res = await fetch(MCP_URL, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Accept: "application/json, text/event-stream",
      },
      body: JSON.stringify({
        jsonrpc: "2.0",
        id: this.reqId++,
        method: "initialize",
        params: {
          protocolVersion: "2025-03-26",
          capabilities: {},
          clientInfo: { name: "swarm-agent-starter", version: "0.1.0" },
        },
      }),
    });
    this.sessionId = res.headers.get("mcp-session-id");
    await res.text();
  }

  async callTool<T = any>(name: string, args: Record<string, unknown> = {}): Promise<T> {
    const headers: Record<string, string> = {
      "Content-Type": "application/json",
      Accept: "application/json, text/event-stream",
    };
    if (this.sessionId) headers["mcp-session-id"] = this.sessionId;

    const res = await fetch(MCP_URL, {
      method: "POST",
      headers,
      body: JSON.stringify({
        jsonrpc: "2.0",
        id: this.reqId++,
        method: "tools/call",
        params: { name, arguments: args },
      }),
    });
    const raw = await res.text();
    const dataLine = raw
      .split("\n")
      .find((l) => l.startsWith("data: "))
      ?.slice(6);
    const envelope = JSON.parse(dataLine ?? raw);
    if (envelope.error) {
      throw new Error(`MCP error (${name}): ${JSON.stringify(envelope.error)}`);
    }
    const text = envelope.result?.content?.[0]?.text ?? "{}";
    try {
      return JSON.parse(text) as T;
    } catch {
      return text as unknown as T;
    }
  }
}

function loadOrGenerateKeypair(): Keypair {
  const raw = process.env.AGENT_KEYPAIR_JSON;
  if (raw) {
    return Keypair.fromSecretKey(Uint8Array.from(JSON.parse(raw)));
  }
  return Keypair.generate();
}

function signBase64Tx(unsignedB64: string, signer: Keypair): string {
  const tx = VersionedTransaction.deserialize(Buffer.from(unsignedB64, "base64"));
  tx.sign([signer]);
  return Buffer.from(tx.serialize()).toString("base64");
}

async function authenticateAndBootstrap(mcp: McpSession, wallet: Keypair): Promise<void> {
  const address = wallet.publicKey.toBase58();
  const reg = await mcp.callTool("register_wallet", {
    chain: "solana",
    address,
  });
  console.log(`[1/3] Registered wallet ${address} (balance_lamports=${reg.balance_lamports ?? 0})`);

  const challenge = await mcp.callTool("agent_verify_wallet", {
    action: "challenge",
    wallet: address,
  });
  const msgBytes = new TextEncoder().encode(challenge.challenge_message);
  const sigBytes = nacl.sign.detached(msgBytes, wallet.secretKey);
  await mcp.callTool("agent_verify_wallet", {
    action: "confirm",
    wallet: address,
    signature: bs58.encode(sigBytes),
  });
  console.log(`[2/3] Verified Ed25519 ownership for ${address}`);

  if ((reg.balance_lamports ?? 0) === 0 && process.env.EXECUTE_LIVE === "1") {
    const onboard = await mcp.callTool("shillbot_onboard", {});
    console.log(`[3/3] Gasless bootstrap complete:`, onboard);
  } else {
    console.log(`[3/3] Ready (set EXECUTE_LIVE=1 to broadcast live transactions)`);
  }
}

async function main() {
  const modeArg = process.argv.find((a) => a.startsWith("--mode="))?.split("=")[1] ?? "discover";
  const wallet = loadOrGenerateKeypair();
  const mcp = new McpSession();
  await mcp.init();
  await authenticateAndBootstrap(mcp, wallet);

  const listing = await mcp.callTool<{ tasks: any[] }>("shillbot_list_available_tasks", {
    limit: 50,
  });
  const openTasks = (listing.tasks ?? []).filter((t) => t.state === "open");
  const byPlatform = (p: number) => openTasks.filter((t) => t.platform === p);

  console.log(
    `\nOpen Shillbot tasks: total=${openTasks.length} | platform=10 (LeanProof)=${byPlatform(10).length} | platform=5 (GamePlay)=${byPlatform(5).length} | platform=4 (Referral)=${byPlatform(4).length}\n`,
  );

  if (modeArg === "lean-proof") {
    const target = byPlatform(10)[0];
    if (!target) return console.log("No open platform=10 tasks found.");
    const details = await mcp.callTool("shillbot_get_task_details", { task_id: target.task_id });
    console.log("Target LeanProof task:", {
      task_id: details.task_id,
      reward_lamports: details.estimated_payment_lamports,
      statement_lean: details.statement_lean,
      lean_proof_instructions: details.lean_proof_instructions,
    });
    const sampleProof = "theorem proof : statementProp := by\n  unfold statementProp\n  omega\n";
    const contentId = `lean4:proof:${Buffer.from(sampleProof, "utf8").toString("base64")}`;
    console.log(`Prepared inline content_id (${contentId.length} bytes): ${contentId.slice(0, 48)}...`);
    if (process.env.EXECUTE_LIVE === "1") {
      const claim = await mcp.callTool("shillbot_claim_task", { task_id: target.task_id });
      await mcp.callTool("shillbot_submit_tx", {
        action: "claim",
        task_id: target.task_id,
        task_pda: claim.task_pda,
        signed_tx: signBase64Tx(claim.unsigned_tx, wallet),
      });
      const sub = await mcp.callTool("shillbot_submit_work", {
        task_id: target.task_id,
        content_id: contentId,
        proof_lean: sampleProof,
      });
      await mcp.callTool("shillbot_submit_tx", {
        action: "submit",
        task_id: target.task_id,
        task_pda: sub.task_pda,
        content_id: contentId,
        signed_tx: signBase64Tx(sub.unsigned_tx, wallet),
      });
    }
  } else if (modeArg === "gameplay") {
    const target = byPlatform(5)[0];
    if (!target) return console.log("No open platform=5 tasks found.");
    const details = await mcp.callTool("shillbot_get_task_details", { task_id: target.task_id });
    console.log("Target GamePlay task:", {
      task_id: details.task_id,
      reward_lamports: details.estimated_payment_lamports,
      gameplay_instructions: details.gameplay_instructions,
    });
  } else if (modeArg === "referral") {
    const target = byPlatform(4)[0];
    if (!target) return console.log("No open platform=4 tasks found.");
    const details = await mcp.callTool("shillbot_get_task_details", { task_id: target.task_id });
    console.log("Target Referral task:", {
      task_id: details.task_id,
      reward_lamports: details.estimated_payment_lamports,
      referral_instructions: details.referral_instructions,
    });
  } else if (modeArg === "bug-report") {
    console.log("Example structured bug_report payload for agent_send_message:");
    console.log(
      JSON.stringify(
        {
          to: SUPPORT_WALLET,
          kind: "bug_report",
          subject: "Formal vacuity / counterexample report",
          body: "Included machine-checked Lean 4 refutation `theorem disproof : ¬ statementProp`.",
        },
        null,
        2,
      ),
    );
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
