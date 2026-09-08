import { afterEach, describe, expect, it, vi } from "vitest";
import { CoordinationGameApiClient, CoordinationGameWebSocketClient, type WebSocketLike } from "./api.js";
import { SwarmClientError } from "../errors.js";

afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); });

describe("CoordinationGameApiClient", () => {
  it("injects fetch, network, auth, and secret-handle routing", async () => {
    const fetch = vi.fn(async () => new Response(JSON.stringify({ status: "waiting" }), { status: 200 }));
    const api = new CoordinationGameApiClient({ baseUrl: "https://game.test/", fetch, network: "devnet" });
    await api.joinQueue({ tournament_id: 1 }, "token");
    const calls = fetch.mock.calls as unknown as Array<[RequestInfo | URL, RequestInit]>;
    expect(calls[0][0]).toBe("https://game.test/queue/join?network=devnet");
    expect((calls[0][1].headers as Record<string, string>).Authorization).toBe("Bearer token");
    await api.crossChainStatus({ handle: "h/1+" });
    expect(calls[1][0]).toContain("handle=h%2F1%2B");
    expect(calls[1][0]).not.toContain("wallet=");
  });

  it("uses the shared stable error model", async () => {
    const api = new CoordinationGameApiClient({ baseUrl: "https://game.test", fetch: async () => new Response(JSON.stringify({ message: "down" }), { status: 503 }) });
    await expect(api.authChallenge({ wallet: "w" })).rejects.toMatchObject({ code: "HTTP_ERROR", retryable: true, status: 503 } satisfies Partial<SwarmClientError>);
  });
});

describe("CoordinationGameWebSocketClient", () => {
  it("queues application messages and flushes them after connection", () => {
    const sockets: WebSocketLike[] = [];
    const factory = vi.fn((_url: string) => {
      const socket: WebSocketLike & { sent: string[] } = { readyState: 0, sent: [], onopen: null, onclose: null, onmessage: null, onerror: null, send(value) { this.sent.push(value); }, close() { this.readyState = 3; } };
      sockets.push(socket);
      return socket;
    });
    const client = new CoordinationGameWebSocketClient({ baseUrl: "https://game.test", token: "t", network: "eip155:84532", webSocket: factory });
    client.connect();
    client.send({ type: "chat", text: "hello" });
    client.send({ type: "ping" });
    sockets[0].readyState = 1;
    sockets[0].onopen?.();
    expect((sockets[0] as WebSocketLike & { sent: string[] }).sent.map((value) => JSON.parse(value))).toEqual([{ type: "chat", text: "hello" }]);
    expect(factory.mock.calls[0][0]).toContain("network=eip155:84532");
    client.close();
  });
});
