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

  it("posts cross-chain settle signatures, outcome cosign, and Solana settle builds", async () => {
    const calls: Array<{ url: string; method?: string; body: unknown }> = [];
    const api = new CoordinationGameApiClient({
      baseUrl: "https://game.test",
      fetch: async (input, init) => {
        calls.push({
          url: String(input),
          method: init?.method,
          body: init?.body ? JSON.parse(String(init.body)) : undefined,
        });
        return new Response(JSON.stringify({ ready: true, match_id: "0x01", live_sigs: ["0xa", "0xb", "0xc"], oc_sigs: ["0xd", "0xe", "0xf"], unsigned_tx: "AQID", blockhash: "Bh", action: "settle_xmatch" }), { status: 200 });
      },
    });
    await api.submitCrossChainSettleSigs({ handle: "ph-1", live_signature: "0x11", outcome_signature: "0x22" });
    await api.crossChainOutcomeCosign({ handle: "ph-1", live_signature: "0x11", outcome_signature: "0x22" });
    await api.buildCrossChainSolSettle({ handle: "ph-1", wallet: "Sol111" });

    expect(calls[0]).toEqual({
      url: "https://game.test/internal/xqueue/settle-sigs",
      method: "POST",
      body: { handle: "ph-1", live_signature: "0x11", outcome_signature: "0x22" },
    });
    expect(calls[1]).toEqual({
      url: "https://game.test/internal/xqueue/outcome-cosign",
      method: "POST",
      body: { handle: "ph-1", live_signature: "0x11", outcome_signature: "0x22" },
    });
    expect(calls[2]).toEqual({
      url: "https://game.test/internal/xqueue/build-sol-settle",
      method: "POST",
      body: { handle: "ph-1", wallet: "Sol111" },
    });
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

// Signed nonce authentication is shared by Shillbot and Game clients.
it("submits the ownership proof without using a wallet address as bearer", async () => {
  const requests: Array<{url: string, init?: RequestInit}> = [];
  const api = new CoordinationGameApiClient({ baseUrl: "https://identity.example", fetch: async (input, init) => {
    requests.push({url: String(input), init});
    return new Response(JSON.stringify({token:"signed-session"}));
  }});
  expect(await api.authVerify({ wallet:"wallet", nonce:"nonce", signature:"signature" })).toEqual({token:"signed-session"});
  expect(requests[0]?.url).toBe("https://identity.example/auth/verify");
  expect(new Headers(requests[0]?.init?.headers).has("Authorization")).toBe(false);
});

it("P2-8: forwards optional token and isAi -> is_ai across EVM and cross-chain endpoints", async () => {
  const seen: Array<{ url: string; method?: string; auth: string | null; body: unknown }> = [];
  const api = new CoordinationGameApiClient({
    baseUrl: "https://game.test",
    getToken: () => "default-bearer",
    fetch: async (input, init) => {
      seen.push({
        url: String(input),
        method: init?.method,
        auth: new Headers(init?.headers).get("Authorization"),
        body: init?.body ? JSON.parse(String(init.body)) : undefined,
      });
      return new Response(
        JSON.stringify({
          status: "waiting",
          both_committed: false,
          both_revealed: false,
          relayed: true,
          r_matchup: null,
          match_id: "0x01",
          matched: false,
        }),
        { status: 200 },
      );
    },
  });

  await api.joinQueue({ tournament_id: 3, isAi: true });
  await api.joinCrossChainQueue(
    { wallet: "0xabc", chain: "eip155:8453", session_key: "0xdef", tournament_id: 3, isAi: true },
    "xchain-jwt",
  );
  await api.crossChainCommit({ wallet: "0xabc", commit: "0x11", handle: "h1" }, "xchain-jwt");
  await api.crossChainSign({ wallet: "0xabc", step: 1, signature: "0x22", handle: "h1" }, "xchain-jwt");
  await api.crossChainReveal({ wallet: "0xabc", preimage: "0x33", handle: "h1" }, "xchain-jwt");
  await api.crossChainGameplay({ handle: "h1" }, "xchain-jwt");
  await api.joinEvmGame(
    { wallet: "0xdef", player_wallet: "0xabc", chain: "eip155:8453", tournament_id: 3, isAi: true },
    "evm-jwt",
  );
  await api.evmGameCommitted({ game_id: "0x99", wallet: "0xdef" }, "evm-jwt");
  await api.evmGameStaked({ game_id: "0x99", wallet: "0xdef" }, "evm-jwt");

  expect(seen[0]).toEqual({
    url: "https://game.test/queue/join",
    method: "POST",
    auth: "Bearer default-bearer",
    body: { tournament_id: 3, is_ai: true },
  });
  expect(seen[1]).toEqual({
    url: "https://game.test/internal/xqueue/join",
    method: "POST",
    auth: "Bearer xchain-jwt",
    body: {
      wallet: "0xabc",
      chain: "eip155:8453",
      session_key: "0xdef",
      tournament_id: 3,
      is_ai: true,
    },
  });
  expect(seen[2]?.auth).toBe("Bearer xchain-jwt");
  expect(seen[3]?.auth).toBe("Bearer xchain-jwt");
  expect(seen[4]?.auth).toBe("Bearer xchain-jwt");
  expect(seen[5]).toEqual({
    url: "https://game.test/internal/xqueue/gameplay?handle=h1",
    method: "GET",
    auth: "Bearer xchain-jwt",
    body: undefined,
  });
  expect(seen[6]).toEqual({
    url: "https://game.test/internal/evmgame/join",
    method: "POST",
    auth: "Bearer evm-jwt",
    body: {
      wallet: "0xdef",
      player_wallet: "0xabc",
      chain: "eip155:8453",
      tournament_id: 3,
      is_ai: true,
    },
  });
  expect(seen[7]?.auth).toBe("Bearer evm-jwt");
  expect(seen[8]?.auth).toBe("Bearer evm-jwt");
});
