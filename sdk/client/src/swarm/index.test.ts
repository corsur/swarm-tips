import { describe, expect, it, vi } from "vitest";
import { SwarmClient } from "./index.js";

describe("SwarmClient", () => {
  it("fetches listings, discovery, reputation, and leaderboard through injected fetch", async () => {
    const fetch = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.includes("listings")) return new Response("[]", { status: 200 });
      if (url.includes("mcp/search")) return new Response(JSON.stringify({ results: [], corpus_size: 16_000 }), { status: 200 });
      if (url.includes("leaderboard")) return new Response(JSON.stringify({ agents: [{ wallet: "a" }] }), { status: 200 });
      return new Response(JSON.stringify({ wallet: "a", web_position: 1, extensions_received: 1, has_standing: true }), { status: 200 });
    });
    const client = new SwarmClient({ baseUrl: "https://mcp.test/", fetch });
    await expect(client.listings()).resolves.toEqual([]);
    await expect(client.search("solana", 10)).resolves.toMatchObject({ corpus_size: 16_000 });
    await expect(client.reputation("a", "devnet")).resolves.toMatchObject({ wallet: "a" });
    await expect(client.leaderboard()).resolves.toEqual([{ wallet: "a" }]);
  });

  it("performs the MCP initialize/session/list handshake", async () => {
    const headers = new Headers({ "mcp-session-id": "sid" });
    const fetch = vi.fn()
      .mockResolvedValueOnce(new Response("{}", { status: 200, headers }))
      .mockResolvedValueOnce(new Response("", { status: 200 }))
      .mockResolvedValueOnce(new Response(`data: ${JSON.stringify({ result: { tools: [{}, {}] } })}\n\n`, { status: 200 }));
    const client = new SwarmClient({ baseUrl: "https://mcp.test", fetch });
    await expect(client.toolCount()).resolves.toBe(2);
    expect((fetch.mock.calls[1][1].headers as Record<string, string>)["mcp-session-id"]).toBe("sid");
  });
});
