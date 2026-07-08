/**
 * Agent-facing e2e — the reputation/credit-web path THROUGH THE MCP SERVER
 * (the agent's "frontend"), against the LIVE devnet extension graph. Agents
 * consume swarm.tips via MCP tools, so this drives the real `/mcp`
 * streamable-http transport exactly as an agent would.
 *
 * READ-ONLY — it asserts against the permanent dogfood extension (root CKsZ… ->
 * agent B9H6…, kept open on devnet) and spends NO SOL, so it is safe to run in
 * automated CI. The write path (submit_extension / attest / default + bond
 * conservation) is covered by the program's free localnet anchor tests.
 *
 * Flow:
 *   1. MCP initialize handshake against the running server (devnet-pointed)
 *   2. tools/call list_extensions{recipient:dogfood} -> the live extension appears
 *   3. tools/call query_agent_credit_web_score{wallet:dogfood} -> has_standing
 *   4. GET /internal/agent-reputation -> parity with the MCP tool
 *   5. negative control: a random wallet has_standing == false
 *   6. settlement graph: agent_trust_score carries the eigentrust record;
 *      agent_reputation_leaderboard is rank-ordered and matches the HTTP
 *      endpoint (Firestore agent_reputation — mainnet settlements,
 *      network-independent)
 *   7. search_mcp_servers: BM25 over the ingested catalog returns ranked
 *      hits with per-hit signal disclosure
 *
 * Assumes the MCP server is running and pointed at devnet (run-mcp-reputation-devnet.sh
 * / e2e.yml start it). Override its URL with MCP_URL (default http://localhost:8090).
 * Run: npx tsx scripts/e2e/mcp-reputation-devnet.ts
 */
import { PublicKey } from "@solana/web3.js";

const MCP_URL = process.env.MCP_URL ?? "http://localhost:8090";
// The trusted root (web_position.rs WEB_POSITION_ROOT) and the permanent
// dogfood agent it vouches for on devnet.
const ROOT = "CKsZ7ZMLLUzbHUeu2Vm5mjuB8QQi3vfvqvXFdFxT7xmY";
const DOGFOOD_AGENT = "B9H6dLnZNrYa6Gkho8TsKo7nRKgpuK7SSYBbHNC4qiY2";

let failures = 0;
function check(cond: boolean, msg: string): void {
  console.log(`  ${cond ? "✓" : "✗"} ${msg}`);
  if (!cond) failures++;
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
    body: JSON.stringify({
      jsonrpc: "2.0",
      method: "notifications/initialized",
    }),
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
  if (rpc.error)
    throw new Error(`MCP tool ${name} error: ${JSON.stringify(rpc.error)}`);
  const result = rpc.result as { content?: { text?: string }[] };
  const text = result?.content?.[0]?.text;
  return text
    ? (JSON.parse(text) as Record<string, unknown>)
    : (rpc.result as Record<string, unknown>);
}

async function main(): Promise<void> {
  // Validate the constants are real pubkeys (fail fast on a typo).
  new PublicKey(ROOT);
  new PublicKey(DOGFOOD_AGENT);
  console.log(`MCP server: ${MCP_URL}`);
  console.log(`dogfood agent (recipient): ${DOGFOOD_AGENT}`);

  console.log(
    "\n[mcp] initialize -> list_extensions -> query_agent_credit_web_score"
  );
  const sid = await mcpInitialize();
  check(sid.length > 0, "MCP session established over /mcp");

  const listed = (await mcpCall(sid, "list_extensions", {
    recipient: DOGFOOD_AGENT,
  })) as {
    count: number;
    extensions: {
      extender: string;
      recipient: string;
      bond_lamports: number;
    }[];
  };
  const mine = listed.extensions.find(
    (e) => e.extender === ROOT && e.recipient === DOGFOOD_AGENT
  );
  check(
    listed.count >= 1 && mine !== undefined,
    "list_extensions surfaces the live dogfood extension"
  );

  const score = (await mcpCall(sid, "query_agent_credit_web_score", {
    wallet: DOGFOOD_AGENT,
  })) as {
    wallet: string;
    position: number | null;
    extensions_received: number;
    has_standing: boolean;
  };
  check(
    score.has_standing === true,
    "query_agent_credit_web_score: dogfood agent has standing"
  );
  check(
    typeof score.position === "number" && score.position > 0,
    `web-position computed (${score.position})`
  );
  check(score.extensions_received >= 1, "extensions_received >= 1 via MCP");

  const stranger = "11111111111111111111111111111112";
  const none = (await mcpCall(sid, "query_agent_credit_web_score", {
    wallet: stranger,
  })) as {
    has_standing: boolean;
  };
  check(
    none.has_standing === false,
    "unvouched wallet has no standing (negative control)"
  );

  console.log("\n[http] /internal/agent-reputation parity with the MCP tool");
  const rep = (await (
    await fetch(
      `${MCP_URL}/internal/agent-reputation?wallet=${DOGFOOD_AGENT}&network=devnet`
    )
  ).json()) as {
    web_position: number | null;
    extensions_received: number;
    has_standing: boolean;
  };
  check(
    rep.has_standing === true,
    "reputation endpoint: dogfood agent has standing"
  );
  check(
    rep.web_position === score.position &&
      rep.extensions_received === score.extensions_received,
    "reputation endpoint matches the MCP tool (shared compute)"
  );

  console.log(
    "\n[mcp] settlement graph: agent_trust_score -> agent_reputation_leaderboard"
  );
  const trust = (await mcpCall(sid, "agent_trust_score", {
    wallet: ROOT,
  })) as {
    trust_score: number;
    confidence: number;
    eigentrust: { rank: number; rank_normalized: number } | null;
    inputs_present: { eigentrust: boolean };
  };
  check(
    typeof trust.trust_score === "number" &&
      trust.trust_score > 0 &&
      trust.trust_score <= 1,
    `agent_trust_score returns a composite in (0, 1] (${trust.trust_score})`
  );
  check(
    trust.eigentrust !== null && trust.eigentrust.rank >= 1,
    "agent_trust_score carries the eigentrust settlement record"
  );
  check(
    trust.inputs_present.eigentrust === true,
    "inputs_present flags the eigentrust signal"
  );

  const board = (await mcpCall(sid, "agent_reputation_leaderboard", {
    limit: 10,
  })) as {
    count: number;
    agents: { wallet: string; rank: number; settlements_received: number }[];
  };
  check(board.count >= 1, `leaderboard has settlement-anchored agents (${board.count})`);
  const ranksAscending = board.agents.every(
    (a, i) => i === 0 || a.rank >= board.agents[i - 1].rank
  );
  check(
    board.agents[0]?.rank === 1 && ranksAscending,
    "leaderboard is best-rank-first"
  );

  const httpBoard = (await (
    await fetch(`${MCP_URL}/internal/reputation/leaderboard?limit=10`)
  ).json()) as { count: number; agents: { wallet: string }[] };
  check(
    httpBoard.count === board.count &&
      httpBoard.agents[0]?.wallet === board.agents[0]?.wallet,
    "leaderboard endpoint matches the MCP tool (shared read)"
  );

  console.log("\n[mcp] search_mcp_servers over the ingested catalog");
  const search = (await mcpCall(sid, "search_mcp_servers", {
    query: "solana",
    limit: 5,
  })) as {
    corpus_size: number;
    returned: number;
    results: {
      name: string;
      ranking_signals: { bm25_score: number; final_score: number };
    }[];
  };
  check(
    search.corpus_size > 1000,
    `ingested catalog behind search (corpus ${search.corpus_size})`
  );
  check(search.returned >= 1, "capability query returns hits");
  check(
    search.results.every(
      (r) =>
        r.ranking_signals.bm25_score > 0 && r.ranking_signals.final_score > 0
    ),
    "every hit discloses its ranking signals"
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
