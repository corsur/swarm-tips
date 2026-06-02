/**
 * Agent-facing e2e — the reputation/credit-web path THROUGH THE MCP SERVER
 * (the agent's "frontend"), against the LIVE devnet extension-registry program.
 * This is deliberately NOT a browser/Playwright test: agents consume swarm.tips
 * via MCP tools, so the agent e2e drives the real `/mcp` streamable-http
 * transport exactly as an agent would.
 *
 * Flow (with a money-conservation ledger):
 *   1. submit_extension root -> agent on devnet (leave the obligation OPEN)
 *   2. MCP initialize handshake against the running server (devnet-pointed)
 *   3. tools/call list_extensions{recipient:agent} -> the live extension appears
 *   4. tools/call query_agent_credit_web_score{wallet:agent} -> has_standing,
 *      position ~1.0 (agent is directly vouched by the trusted root)
 *   5. GET /internal/agent-reputation (the human-frontend backend) -> parity
 *      with the MCP tool (same shared web_position compute)
 *   6. negative control: a random wallet has_standing == false
 *   7. cleanup: attest_return_substance closes the extension + returns bond;
 *      assert the root only lost gas (no leaked bond/rent)
 *
 * Assumes the MCP server is already running and pointed at devnet (the
 * run-mcp-reputation-devnet.sh wrapper / e2e.yml CI job starts it). Override its
 * URL with MCP_URL (default http://localhost:8090).
 *
 * Signers: id.json = root (= the trusted web-position root CKsZ…); test.json =
 * agent (recipient, signs nothing). Run: npx tsx scripts/e2e/mcp-reputation-devnet.ts
 */
import * as anchor from "@coral-xyz/anchor";
import { BN } from "@coral-xyz/anchor";
import { Connection, Keypair, PublicKey, SystemProgram } from "@solana/web3.js";
import { readFileSync } from "fs";
import { homedir } from "os";
import { join } from "path";
import type { ExtensionRegistry } from "../../target/types/extension_registry";

const DEVNET = "https://api.devnet.solana.com";
const MCP_URL = process.env.MCP_URL ?? "http://localhost:8090";
const TYPE_CAPABILITY_VALIDATION = 0;
const BOND = new BN(5_000_000); // 0.005 SOL
// The trusted root the MCP server anchors web-position to (web_position.rs
// WEB_POSITION_ROOT). id.json must equal this for the agent to score.
const WEB_POSITION_ROOT = "CKsZ7ZMLLUzbHUeu2Vm5mjuB8QQi3vfvqvXFdFxT7xmY";

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

// --- minimal MCP streamable-http client (the agent's transport) ---
function parseSse(body: string): Record<string, unknown> {
  for (const line of body.split("\n")) {
    const t = line.trim();
    if (t.startsWith("data:")) {
      const payload = t.slice(5).trim();
      if (payload.startsWith("{")) {
        try {
          return JSON.parse(payload) as Record<string, unknown>;
        } catch {
          /* not the JSON-RPC frame; keep scanning */
        }
      }
    }
  }
  throw new Error(`no JSON-RPC data in SSE response: ${body.slice(0, 200)}`);
}

const MCP_HEADERS = {
  "Content-Type": "application/json",
  Accept: "application/json, text/event-stream",
};

async function mcpInitialize(): Promise<string> {
  const res = await fetch(`${MCP_URL}/mcp`, {
    method: "POST",
    headers: MCP_HEADERS,
    body: JSON.stringify({
      jsonrpc: "2.0",
      id: 1,
      method: "initialize",
      params: {
        protocolVersion: "2025-03-26",
        capabilities: {},
        clientInfo: { name: "mcp-reputation-e2e", version: "1.0" },
      },
    }),
  });
  const sid = res.headers.get("mcp-session-id");
  await res.text();
  if (!sid) throw new Error("MCP initialize returned no mcp-session-id");
  await fetch(`${MCP_URL}/mcp`, {
    method: "POST",
    headers: { ...MCP_HEADERS, "mcp-session-id": sid },
    body: JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized" }),
  });
  return sid;
}

let rpcId = 2;
async function mcpCall(
  sid: string,
  name: string,
  args: Record<string, unknown>
): Promise<Record<string, unknown>> {
  const res = await fetch(`${MCP_URL}/mcp`, {
    method: "POST",
    headers: { ...MCP_HEADERS, "mcp-session-id": sid },
    body: JSON.stringify({
      jsonrpc: "2.0",
      id: rpcId++,
      method: "tools/call",
      params: { name, arguments: args },
    }),
  });
  const rpc = parseSse(await res.text());
  if (rpc.error) throw new Error(`MCP tool ${name} error: ${JSON.stringify(rpc.error)}`);
  const result = rpc.result as { content?: { text?: string }[] };
  const text = result?.content?.[0]?.text;
  return text ? (JSON.parse(text) as Record<string, unknown>) : (rpc.result as Record<string, unknown>);
}

async function main(): Promise<void> {
  const root = loadKeypair(join(homedir(), ".config/solana/id.json"));
  const agent = loadKeypair(join(homedir(), ".config/solana/test.json"));
  const connection = new Connection(DEVNET, "confirmed");
  const provider = new anchor.AnchorProvider(connection, new anchor.Wallet(root), {
    commitment: "confirmed",
  });
  anchor.setProvider(provider);
  const registry = new anchor.Program(
    loadIdl("extension_registry"),
    provider
  ) as unknown as anchor.Program<ExtensionRegistry>;

  const bal = (pk: PublicKey): Promise<number> => connection.getBalance(pk, "confirmed");
  const agentPk = agent.publicKey.toBase58();
  console.log(`MCP server: ${MCP_URL}`);
  console.log(`root  (web-position root): ${root.publicKey.toBase58()}`);
  console.log(`agent (recipient):         ${agentPk}`);
  check(
    root.publicKey.toBase58() === WEB_POSITION_ROOT,
    "id.json is the MCP web-position root (so the agent scores)"
  );

  // --- registry init (idempotent) ---
  const [globalState] = PublicKey.findProgramAddressSync(
    [Buffer.from("extension_global")],
    registry.programId
  );
  try {
    await registry.methods
      .initialize(root.publicKey, root.publicKey)
      .accountsPartial({ globalState, payer: root.publicKey, systemProgram: SystemProgram.programId })
      .rpc({ commitment: "confirmed" });
  } catch {
    /* already initialized */
  }

  // ===== 1. submit_extension root -> agent, LEAVE OPEN so the MCP tools see it =====
  console.log("\n[setup] submit_extension root -> agent (left open for the MCP reads)");
  const [extension] = PublicKey.findProgramAddressSync(
    [Buffer.from("extension"), root.publicKey.toBuffer(), agent.publicKey.toBuffer()],
    registry.programId
  );
  const rootBefore = await bal(root.publicKey);
  if ((await connection.getAccountInfo(extension)) === null) {
    await registry.methods
      .submitExtension(TYPE_CAPABILITY_VALIDATION, BOND)
      .accountsPartial({
        extension,
        extender: root.publicKey,
        recipient: agent.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc({ commitment: "confirmed" });
    console.log("  submitted (bond locked on devnet)");
  } else {
    console.log("  extension already open from a prior run — reusing");
  }
  check((await connection.getAccountInfo(extension)) !== null, "extension is live on devnet");

  // ===== 2-4. drive the MCP server (the agent's frontend) =====
  console.log("\n[mcp] initialize -> list_extensions -> query_agent_credit_web_score");
  const sid = await mcpInitialize();
  check(sid.length > 0, "MCP session established over /mcp");

  const listed = (await mcpCall(sid, "list_extensions", { recipient: agentPk })) as {
    count: number;
    extensions: { extender: string; recipient: string; bond_lamports: number }[];
  };
  const mine = listed.extensions.find(
    (e) => e.extender === root.publicKey.toBase58() && e.recipient === agentPk
  );
  check(listed.count >= 1 && mine !== undefined, "list_extensions surfaces the live extension");
  check(mine?.bond_lamports === BOND.toNumber(), "list_extensions reports the correct bond");

  const score = (await mcpCall(sid, "query_agent_credit_web_score", { wallet: agentPk })) as {
    wallet: string;
    position: number | null;
    extensions_received: number;
    has_standing: boolean;
  };
  check(score.has_standing === true, "query_agent_credit_web_score: agent has standing");
  check(
    typeof score.position === "number" && score.position > 0,
    `web-position computed (${score.position})`
  );
  check(score.extensions_received >= 1, "extensions_received >= 1 via MCP");

  // negative control: a fresh wallet is not in the graph
  const stranger = Keypair.generate().publicKey.toBase58();
  const none = (await mcpCall(sid, "query_agent_credit_web_score", { wallet: stranger })) as {
    has_standing: boolean;
  };
  check(none.has_standing === false, "unvouched wallet has no standing (negative control)");

  // ===== 5. /internal/agent-reputation (the human-frontend backend) parity =====
  console.log("\n[http] /internal/agent-reputation parity with the MCP tool");
  const repRes = await fetch(
    `${MCP_URL}/internal/agent-reputation?wallet=${agentPk}&network=devnet`
  );
  const rep = (await repRes.json()) as {
    wallet: string;
    web_position: number | null;
    extensions_received: number;
    has_standing: boolean;
  };
  check(rep.has_standing === true, "reputation endpoint: agent has standing");
  check(
    rep.web_position === score.position && rep.extensions_received === score.extensions_received,
    "reputation endpoint matches the MCP tool (shared compute)"
  );

  // ===== 7. cleanup: attest closes the extension + returns the bond =====
  console.log("\n[cleanup] attest_return_substance -> close extension, reclaim bond");
  await registry.methods
    .attestReturnSubstance()
    .accountsPartial({ extension, extender: root.publicKey, recipient: agent.publicKey })
    .rpc({ commitment: "confirmed" });
  check(
    (await connection.getAccountInfo(extension)) === null,
    "extension closed after attest (bond + rent reclaimed)"
  );
  const netLoss = rootBefore - (await bal(root.publicKey));
  check(netLoss >= 0 && netLoss < 100_000, `root lost only gas (net ${netLoss} lamports, bond returned)`);

  console.log(`\n${failures === 0 ? "PASS" : "FAIL"} — ${failures} failed check(s)`);
  process.exit(failures === 0 ? 0 : 1);
}

main().catch((e: unknown) => {
  console.error(e);
  process.exit(1);
});
