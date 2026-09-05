import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  InboxApiError,
  InboxClient,
  type InboxSession,
  type MessagePage,
} from "./client.js";
import { solanaNonceSigner } from "./solana.js";

const BASE = "https://mcp.test";
const WALLET = "WaLLet1111111111111111111111111111111111111";
const CACHE_KEY = `swarm-inbox-session:${WALLET}`;

/** Minimal in-memory Storage. */
function fakeStorage(seed: Record<string, string> = {}) {
  const map = new Map(Object.entries(seed));
  return {
    getItem: (k: string) => map.get(k) ?? null,
    setItem: (k: string, v: string) => void map.set(k, v),
    removeItem: (k: string) => void map.delete(k),
    dump: () => Object.fromEntries(map),
  };
}

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

const mockFetch = vi.fn();

function client(
  storage: ReturnType<typeof fakeStorage> | null = fakeStorage()
) {
  return new InboxClient({
    baseUrl: BASE,
    storage,
    fetchFn: mockFetch as unknown as typeof fetch,
  });
}

/** Queue the two-step session mint (challenge → verify). */
function queueMint(sessionId = "sess-1", tier = "session") {
  mockFetch
    .mockResolvedValueOnce(jsonResponse(200, { nonce: "nonce-abc" }))
    .mockResolvedValueOnce(jsonResponse(200, { session_id: sessionId, tier }));
}

const signer = vi.fn(async (nonce: string) => `sig(${nonce})`);

beforeEach(() => {
  vi.resetAllMocks();
  signer.mockImplementation(async (nonce: string) => `sig(${nonce})`);
});

describe("createSession", () => {
  it("mints via challenge → sign → verify and caches the session by wallet", async () => {
    const storage = fakeStorage();
    const c = client(storage);
    queueMint("sess-42", "wallet_verified");

    const session = await c.createSession(WALLET, signer);

    expect(session).toEqual<InboxSession>({
      session_id: "sess-42",
      tier: "wallet_verified",
    });
    expect(signer).toHaveBeenCalledTimes(1);
    expect(signer).toHaveBeenCalledWith("nonce-abc");

    // Phase 1: {wallet} only. Phase 2: {wallet, nonce, signature}.
    const [url1, init1] = mockFetch.mock.calls[0];
    const [url2, init2] = mockFetch.mock.calls[1];
    expect(url1).toBe(`${BASE}/internal/inbox/session`);
    expect(url2).toBe(`${BASE}/internal/inbox/session`);
    expect(JSON.parse(String(init1?.body))).toEqual({ wallet: WALLET });
    expect(JSON.parse(String(init2?.body))).toEqual({
      wallet: WALLET,
      nonce: "nonce-abc",
      signature: "sig(nonce-abc)",
    });

    expect(JSON.parse(storage.dump()[CACHE_KEY] ?? "{}")).toEqual({
      session_id: "sess-42",
      tier: "wallet_verified",
    });
  });

  it("returns the cached session without fetching or signing", async () => {
    const storage = fakeStorage({
      [CACHE_KEY]: JSON.stringify({ session_id: "cached-1", tier: "session" }),
    });
    const c = client(storage);

    const session = await c.createSession(WALLET, signer);

    expect(session.session_id).toBe("cached-1");
    expect(mockFetch).not.toHaveBeenCalled();
    expect(signer).not.toHaveBeenCalled();
    expect(c.hasCachedSession(WALLET)).toBe(true);
  });

  it("ignores a corrupt cache entry and mints fresh", async () => {
    const storage = fakeStorage({ [CACHE_KEY]: "{not json" });
    const c = client(storage);
    queueMint();

    const session = await c.createSession(WALLET, signer);
    expect(session.session_id).toBe("sess-1");
    expect(mockFetch).toHaveBeenCalledTimes(2);
  });

  it("propagates a challenge rejection with the server's error message", async () => {
    const c = client();
    mockFetch.mockResolvedValueOnce(
      jsonResponse(429, { error: "rate limited" })
    );

    await expect(c.createSession(WALLET, signer)).rejects.toMatchObject({
      name: "InboxApiError",
      status: 429,
      message: "rate limited",
    });
    expect(signer).not.toHaveBeenCalled();
  });

  it("rejects a verify response missing session_id", async () => {
    const c = client();
    mockFetch
      .mockResolvedValueOnce(jsonResponse(200, { nonce: "n" }))
      .mockResolvedValueOnce(jsonResponse(200, { ok: true }));

    await expect(c.createSession(WALLET, signer)).rejects.toThrow(
      /no session_id/
    );
  });
});

describe("getThread / getMessages", () => {
  it("sends X-Inbox-Session and thread/cursor/limit params, and parses the page", async () => {
    const c = client();
    queueMint("sess-1");
    await c.createSession(WALLET, signer);

    const messages = [
      {
        msg_id: "02_b",
        from_wallet: `solana:mainnet:${WALLET}`,
        thread_id: "task:t-1",
        intent: "task_clarification",
        body: "newest",
        sent_at: "2026-08-24T00:00:00Z",
      },
    ];
    mockFetch.mockResolvedValueOnce(
      jsonResponse(200, { messages, next_cursor: "01_a" })
    );

    const page = await c.getThread("task:t-1", { cursor: "03_c", limit: 25 });

    expect(page).toEqual<MessagePage>({
      messages: messages as MessagePage["messages"],
      next_cursor: "01_a",
    });
    const [url, init] = mockFetch.mock.calls[2];
    const parsed = new URL(String(url));
    expect(parsed.pathname).toBe("/internal/inbox/messages");
    expect(parsed.searchParams.get("thread_id")).toBe("task:t-1");
    expect(parsed.searchParams.get("cursor")).toBe("03_c");
    expect(parsed.searchParams.get("limit")).toBe("25");
    expect(init?.method).toBe("GET");
    expect((init?.headers as Record<string, string>)["X-Inbox-Session"]).toBe(
      "sess-1"
    );
  });

  it("pages: passing the previous next_cursor fetches the older page", async () => {
    const c = client();
    queueMint();
    await c.createSession(WALLET, signer);

    mockFetch
      .mockResolvedValueOnce(
        jsonResponse(200, { messages: [], next_cursor: "cur-2" })
      )
      .mockResolvedValueOnce(
        jsonResponse(200, { messages: [], next_cursor: null })
      );

    const p1 = await c.getThread("task:t-1");
    const p2 = await c.getThread("task:t-1", {
      cursor: p1.next_cursor ?? undefined,
    });

    expect(p1.next_cursor).toBe("cur-2");
    expect(p2.next_cursor).toBeNull();
    const lastUrl = new URL(String(mockFetch.mock.calls[3][0]));
    expect(lastUrl.searchParams.get("cursor")).toBe("cur-2");
  });

  it("rejects an out-of-bounds limit before any request", async () => {
    const c = client();
    queueMint();
    await c.createSession(WALLET, signer);
    await expect(c.getMessages({ limit: 51 })).rejects.toThrow(/limit/);
    await expect(c.getMessages({ limit: 0 })).rejects.toThrow(/limit/);
    expect(mockFetch).toHaveBeenCalledTimes(2); // mint only
  });

  it("sends include_sent=true and parses the direction marker under includeSent", async () => {
    const c = client();
    queueMint("sess-1");
    await c.createSession(WALLET, signer);

    const messages = [
      {
        msg_id: "02_b",
        from_wallet: `solana:mainnet:${WALLET}`,
        thread_id: "task:t-1",
        intent: "task_clarification",
        body: "my own reply",
        sent_at: "2026-08-24T00:00:00Z",
        direction: "sent" as const,
      },
      {
        msg_id: "01_a",
        from_wallet: "solana:mainnet:Other",
        thread_id: "task:t-1",
        intent: "task_clarification",
        body: "their message",
        sent_at: "2026-08-23T00:00:00Z",
        direction: "received" as const,
      },
    ];
    mockFetch.mockResolvedValueOnce(
      jsonResponse(200, { messages, next_cursor: null })
    );

    const page = await c.getThread("task:t-1", {
      includeSent: true,
      limit: 50,
    });

    expect(page.messages).toHaveLength(2);
    expect(page.messages[0].direction).toBe("sent");
    expect(page.messages[1].direction).toBe("received");
    const url = new URL(String(mockFetch.mock.calls[2][0]));
    expect(url.searchParams.get("include_sent")).toBe("true");
    expect(url.searchParams.get("thread_id")).toBe("task:t-1");
  });

  it("omits include_sent when not requested", async () => {
    const c = client();
    queueMint();
    await c.createSession(WALLET, signer);
    mockFetch.mockResolvedValueOnce(
      jsonResponse(200, { messages: [], next_cursor: null })
    );
    await c.getThread("task:t-1");
    const url = new URL(String(mockFetch.mock.calls[2][0]));
    expect(url.searchParams.has("include_sent")).toBe(false);
  });

  it("requires createSession first", async () => {
    await expect(client().getThread("task:t-1")).rejects.toThrow(
      /createSession/
    );
    expect(mockFetch).not.toHaveBeenCalled();
  });
});

describe("401 re-mint", () => {
  it("drops the cached session, re-mints once, and retries the request", async () => {
    const storage = fakeStorage({
      [CACHE_KEY]: JSON.stringify({ session_id: "stale", tier: "session" }),
    });
    const c = client(storage);
    await c.createSession(WALLET, signer); // cache hit — no fetch, no popup

    mockFetch.mockResolvedValueOnce(
      jsonResponse(401, { error: "session expired" })
    );
    queueMint("sess-fresh");
    mockFetch.mockResolvedValueOnce(
      jsonResponse(200, { messages: [], next_cursor: null })
    );

    const page = await c.getThread("task:t-1");

    expect(page.messages).toEqual([]);
    expect(signer).toHaveBeenCalledTimes(1); // exactly one re-mint popup
    // Stale cache replaced with the fresh session.
    expect(JSON.parse(storage.dump()[CACHE_KEY] ?? "{}").session_id).toBe(
      "sess-fresh"
    );
    // The retry carries the fresh session id.
    const retryInit = mockFetch.mock.calls[3][1];
    expect(
      (retryInit?.headers as Record<string, string>)["X-Inbox-Session"]
    ).toBe("sess-fresh");
  });

  it("surfaces a still-401 retry instead of looping", async () => {
    const storage = fakeStorage({
      [CACHE_KEY]: JSON.stringify({ session_id: "stale", tier: "session" }),
    });
    const c = client(storage);
    await c.createSession(WALLET, signer);

    mockFetch.mockResolvedValueOnce(jsonResponse(401, { error: "nope" }));
    queueMint("sess-fresh");
    mockFetch.mockResolvedValueOnce(jsonResponse(401, { error: "still nope" }));

    await expect(c.getThread("task:t-1")).rejects.toMatchObject({
      status: 401,
      message: "still nope",
    });
    expect(mockFetch).toHaveBeenCalledTimes(4); // 401 + mint(2) + retry, no loop
  });
});

describe("send", () => {
  async function mintedClient() {
    const c = client();
    queueMint();
    await c.createSession(WALLET, signer);
    return c;
  }

  it("posts to_wallet/body/thread_id/intent and returns the receipt", async () => {
    const c = await mintedClient();
    mockFetch.mockResolvedValueOnce(
      jsonResponse(200, {
        msg_id: "05_x",
        thread_id: "task:t-1",
        sends_remaining_today: 4,
      })
    );

    const receipt = await c.send(
      "DestWallet",
      "hello",
      "task:t-1",
      "task_clarification"
    );

    expect(receipt.msg_id).toBe("05_x");
    const [url, init] = mockFetch.mock.calls[2];
    expect(String(url)).toBe(`${BASE}/internal/inbox/send`);
    expect(JSON.parse(String(init?.body))).toEqual({
      to_wallet: "DestWallet",
      body: "hello",
      thread_id: "task:t-1",
      intent: "task_clarification",
    });
  });

  it("rejects empty and oversize bodies client-side", async () => {
    const c = await mintedClient();
    await expect(c.send("DestWallet", "")).rejects.toThrow(/body/);
    // 4096-BYTE cap, not chars: 1400 three-byte chars = 4200 bytes.
    await expect(c.send("DestWallet", "€".repeat(1400))).rejects.toThrow(
      /body/
    );
    expect(mockFetch).toHaveBeenCalledTimes(2); // mint only
  });

  it("surfaces a quota rejection with status and message", async () => {
    const c = await mintedClient();
    mockFetch.mockResolvedValueOnce(
      jsonResponse(429, { error: "daily send quota exhausted" })
    );
    await expect(c.send("DestWallet", "hi")).rejects.toMatchObject({
      status: 429,
      message: "daily send quota exhausted",
    });
  });

  it("propagates network failures", async () => {
    const c = await mintedClient();
    mockFetch.mockRejectedValueOnce(new TypeError("fetch failed"));
    await expect(c.send("DestWallet", "hi")).rejects.toThrow("fetch failed");
  });
});

describe("ack", () => {
  it("posts up_to_cursor with the session header", async () => {
    const c = client();
    queueMint("sess-9");
    await c.createSession(WALLET, signer);
    mockFetch.mockResolvedValueOnce(jsonResponse(200, { acked: "05_x" }));

    await c.ack("05_x");

    const [url, init] = mockFetch.mock.calls[2];
    expect(String(url)).toBe(`${BASE}/internal/inbox/ack`);
    expect(JSON.parse(String(init?.body))).toEqual({ up_to_cursor: "05_x" });
    expect((init?.headers as Record<string, string>)["X-Inbox-Session"]).toBe(
      "sess-9"
    );
  });

  it("rejects an empty cursor before any request", async () => {
    const c = client();
    queueMint();
    await c.createSession(WALLET, signer);
    await expect(c.ack("")).rejects.toBeInstanceOf(InboxApiError);
    expect(mockFetch).toHaveBeenCalledTimes(2);
  });
});

describe("clearSession", () => {
  it("removes the cache so the next createSession re-mints", async () => {
    const storage = fakeStorage();
    const c = client(storage);
    queueMint();
    await c.createSession(WALLET, signer);
    expect(c.hasCachedSession(WALLET)).toBe(true);

    c.clearSession(WALLET);
    expect(c.hasCachedSession(WALLET)).toBe(false);
    expect(storage.dump()[CACHE_KEY]).toBeUndefined();
  });
});

describe("readTopic (open, no session)", () => {
  it("reads without a session and passes topic_id/cursor/limit/min_trust", async () => {
    const c = client(); // never minted — reads are open
    const posts = [
      {
        post_id: "02",
        topic_id: "open-challenge",
        author_wallet: `solana:mainnet:${WALLET}`,
        body: "1v1 anyone? 0.05 SOL",
        reply_to: null,
        intent: "game_invite",
        ref_id: "game-77",
        reported_count: 0,
        created_at: "2026-08-24T00:00:00Z",
      },
    ];
    mockFetch.mockResolvedValueOnce(
      jsonResponse(200, {
        topic_id: "open-challenge",
        posts,
        next_cursor: "01",
        filtered_hidden: 1,
        filtered_below_min_trust: 2,
      })
    );

    const page = await c.readTopic("open-challenge", {
      cursor: "03",
      limit: 20,
      minTrust: 0.3,
    });

    expect(page.topic_id).toBe("open-challenge");
    expect(page.posts).toHaveLength(1);
    expect(page.posts[0].ref_id).toBe("game-77");
    expect(page.next_cursor).toBe("01");
    expect(page.filtered_hidden).toBe(1);
    expect(page.filtered_below_min_trust).toBe(2);
    // No session mint — exactly one request, and no X-Inbox-Session header.
    expect(mockFetch).toHaveBeenCalledTimes(1);
    const [url, init] = mockFetch.mock.calls[0];
    const parsed = new URL(String(url));
    expect(parsed.pathname).toBe("/internal/topics/read");
    expect(parsed.searchParams.get("topic_id")).toBe("open-challenge");
    expect(parsed.searchParams.get("cursor")).toBe("03");
    expect(parsed.searchParams.get("limit")).toBe("20");
    expect(parsed.searchParams.get("min_trust")).toBe("0.3");
    expect(
      (init?.headers as Record<string, string> | undefined)?.["X-Inbox-Session"]
    ).toBeUndefined();
  });

  it("rejects an out-of-bounds limit before any request", async () => {
    await expect(
      client().readTopic("subcontract", { limit: 51 })
    ).rejects.toThrow(/limit/);
    expect(mockFetch).not.toHaveBeenCalled();
  });

  it("surfaces a backend error with status and message", async () => {
    mockFetch.mockResolvedValueOnce(
      jsonResponse(503, { error: "topic store unavailable" })
    );
    await expect(client().readTopic("open-challenge")).rejects.toMatchObject({
      status: 503,
      message: "topic store unavailable",
    });
  });
});

describe("publishPost / reportPost (session-gated)", () => {
  async function mintedClient() {
    const c = client();
    queueMint();
    await c.createSession(WALLET, signer);
    return c;
  }

  it("posts topic_id/body/reply_to/intent/ref_id with the session header", async () => {
    const c = await mintedClient();
    mockFetch.mockResolvedValueOnce(
      jsonResponse(200, {
        published: true,
        post_id: "09",
        posts_remaining_today: 3,
      })
    );

    const receipt = await c.publishPost(
      "subcontract",
      "handoff: need a website task done",
      {
        replyTo: "07",
        intent: "task_offer",
        refId: "task-12",
      }
    );

    expect(receipt.published).toBe(true);
    expect(receipt.post_id).toBe("09");
    expect(receipt.posts_remaining_today).toBe(3);
    const [url, init] = mockFetch.mock.calls[2];
    expect(String(url)).toBe(`${BASE}/internal/topics/publish`);
    expect(JSON.parse(String(init?.body))).toEqual({
      topic_id: "subcontract",
      body: "handoff: need a website task done",
      reply_to: "07",
      intent: "task_offer",
      ref_id: "task-12",
    });
    expect(
      (init?.headers as Record<string, string>)["X-Inbox-Session"]
    ).toBeDefined();
  });

  it("rejects empty and oversize bodies client-side", async () => {
    const c = await mintedClient();
    await expect(c.publishPost("open-challenge", "")).rejects.toThrow(/body/);
    await expect(
      c.publishPost("open-challenge", "€".repeat(1400))
    ).rejects.toThrow(/body/);
    expect(mockFetch).toHaveBeenCalledTimes(2); // mint only
  });

  it("surfaces an over-quota publish rejection", async () => {
    const c = await mintedClient();
    mockFetch.mockResolvedValueOnce(
      jsonResponse(429, { error: "daily post quota exhausted" })
    );
    await expect(c.publishPost("open-challenge", "gm")).rejects.toMatchObject({
      status: 429,
      message: "daily post quota exhausted",
    });
  });

  it("reports a post and returns the moderation counters", async () => {
    const c = await mintedClient();
    mockFetch.mockResolvedValueOnce(
      jsonResponse(200, {
        reported: true,
        reported_count: 3,
        hidden: true,
        already_reported: false,
      })
    );

    const receipt = await c.reportPost("open-challenge", "09");

    expect(receipt).toEqual({
      reported: true,
      reported_count: 3,
      hidden: true,
      already_reported: false,
    });
    const [url, init] = mockFetch.mock.calls[2];
    expect(String(url)).toBe(`${BASE}/internal/topics/report`);
    expect(JSON.parse(String(init?.body))).toEqual({
      topic_id: "open-challenge",
      post_id: "09",
    });
  });

  it("rejects an empty postId before any request", async () => {
    const c = await mintedClient();
    await expect(c.reportPost("open-challenge", "")).rejects.toThrow(/postId/);
    expect(mockFetch).toHaveBeenCalledTimes(2); // mint only
  });
});

describe("solanaNonceSigner", () => {
  it("signs the nonce's UTF-8 bytes and bs58-encodes the raw signature", async () => {
    // Deterministic fake "signature": the message bytes themselves. bs58 of
    // "abc" (0x61 0x62 0x63) is "ZiCa" — pins both the encoding and that the
    // nonce reaches the wallet as UTF-8 bytes, not hex or base64.
    const signMessage = vi.fn(async (m: Uint8Array) => m);
    const sign = solanaNonceSigner(signMessage);
    await expect(sign("abc")).resolves.toBe("ZiCa");
    expect(signMessage).toHaveBeenCalledTimes(1);
    expect(signMessage).toHaveBeenCalledWith(new TextEncoder().encode("abc"));
  });
});
