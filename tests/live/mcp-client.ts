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
  // Wallet registered on the session. `register_wallet` auth is SESSION-scoped
  // on the server, so a reconnect (new session) must re-register before any
  // authenticated tool call — remembered here so `call()` can do it transparently.
  private authWallet = "";

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

  /** One tools/call round-trip on the current session (no retry). */
  private async once(
    name: string,
    args: Record<string, unknown>
  ): Promise<Record<string, unknown>> {
    const res = await fetch(`${this.url}/mcp`, {
      method: "POST",
      headers: { ...HEADERS, "mcp-session-id": this.sid },
      body: JSON.stringify({
        jsonrpc: "2.0",
        id: this.rpcId++,
        method: "tools/call",
        params: { name, arguments: args },
      }),
    });
    const rpc = parseSse(await res.text());
    if (rpc.error) {
      throw new Error(`MCP tool ${name} error: ${JSON.stringify(rpc.error)}`);
    }
    const result = rpc.result as { content?: { text?: string }[] };
    const text = result?.content?.[0]?.text;
    return text
      ? (JSON.parse(text) as Record<string, unknown>)
      : (rpc.result as Record<string, unknown>);
  }

  /** tools/call with resilience. The live server drops idle streamable-http
   *  SESSIONS (and their session-scoped `register_wallet` auth) between calls —
   *  e.g. while a claim's on-chain tx confirms. On a session/auth loss we
   *  re-establish the session, re-register the wallet, and retry. A real agent
   *  must do exactly this; only a genuine program/tool error is terminal. */
  async call(
    name: string,
    args: Record<string, unknown>
  ): Promise<Record<string, unknown>> {
    if (!this.sid) throw new Error("McpClient.call before connect()");
    if (name === "register_wallet" && typeof args.pubkey === "string") {
      this.authWallet = args.pubkey;
    }
    let lastErr: unknown;
    for (let attempt = 0; attempt < 3; attempt++) {
      try {
        return await this.once(name, args);
      } catch (e) {
        const s = String(e);
        const recoverable =
          /[Ss]ession not found|authentication required|register_wallet first|no valid session|Mcp-Session-Id/.test(
            s
          );
        // A real program/tool error (not a session/auth loss) is terminal.
        if (/MCP tool .* error:/.test(s) && !recoverable) throw e;
        lastErr = e;
        if (recoverable) {
          try {
            await this.connect();
            if (this.authWallet && name !== "register_wallet") {
              await this.once("register_wallet", { pubkey: this.authWallet });
            }
          } catch {
            /* re-auth failed too; fall through to backoff + retry */
          }
        }
        await new Promise((r) => setTimeout(r, 500 * (attempt + 1)));
      }
    }
    throw lastErr;
  }
}
