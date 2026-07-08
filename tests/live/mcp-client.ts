// Minimal MCP streamable-http client — the agent's transport, extracted from
// scripts/e2e/mcp-reputation-devnet.ts so every MCP e2e (reputation, shillbot
// earn lifecycle, …) shares ONE client instead of re-implementing the
// initialize handshake + SSE frame parsing. Lives in tests/live (tsc-excluded,
// run via tsx) because it only ever talks to a live/running MCP server.

const HEADERS = {
  "Content-Type": "application/json",
  Accept: "application/json, text/event-stream",
};

/** Pull the JSON-RPC frame out of a text/event-stream response body. */
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
  // Some servers answer application/json directly.
  if (body.trim().startsWith("{")) {
    return JSON.parse(body) as Record<string, unknown>;
  }
  throw new Error(`no JSON-RPC data in response: ${body.slice(0, 200)}`);
}

export class McpClient {
  private sid = "";
  private rpcId = 2;

  constructor(private readonly url: string) {}

  /** initialize + notifications/initialized; returns the session id. */
  async connect(clientName = "shillbot-mcp-e2e"): Promise<string> {
    const res = await fetch(`${this.url}/mcp`, {
      method: "POST",
      headers: HEADERS,
      body: JSON.stringify({
        jsonrpc: "2.0",
        id: 1,
        method: "initialize",
        params: {
          protocolVersion: "2025-03-26",
          capabilities: {},
          clientInfo: { name: clientName, version: "1.0" },
        },
      }),
    });
    const sid = res.headers.get("mcp-session-id");
    await res.text();
    if (!sid) throw new Error("MCP initialize returned no mcp-session-id");
    this.sid = sid;
    await fetch(`${this.url}/mcp`, {
      method: "POST",
      headers: { ...HEADERS, "mcp-session-id": sid },
      body: JSON.stringify({
        jsonrpc: "2.0",
        method: "notifications/initialized",
      }),
    });
    return sid;
  }

  /** tools/call; unwraps the tool's text content into a parsed object. Retries
   *  transient socket drops (the live server closes idle keep-alives). */
  async call(
    name: string,
    args: Record<string, unknown>
  ): Promise<Record<string, unknown>> {
    if (!this.sid) throw new Error("McpClient.call before connect()");
    const body = JSON.stringify({
      jsonrpc: "2.0",
      id: this.rpcId++,
      method: "tools/call",
      params: { name, arguments: args },
    });
    let lastErr: unknown;
    for (let attempt = 0; attempt < 3; attempt++) {
      try {
        const res = await fetch(`${this.url}/mcp`, {
          method: "POST",
          headers: { ...HEADERS, "mcp-session-id": this.sid },
          body,
        });
        const rpc = parseSse(await res.text());
        if (rpc.error) {
          throw new Error(
            `MCP tool ${name} error: ${JSON.stringify(rpc.error)}`
          );
        }
        const result = rpc.result as { content?: { text?: string }[] };
        const text = result?.content?.[0]?.text;
        return text
          ? (JSON.parse(text) as Record<string, unknown>)
          : (rpc.result as Record<string, unknown>);
      } catch (e) {
        // A program/tool error is terminal; only retry transient transport drops.
        if (/MCP tool .* error:/.test(String(e))) throw e;
        lastErr = e;
        await new Promise((r) => setTimeout(r, 500 * (attempt + 1)));
      }
    }
    throw lastErr;
  }
}
