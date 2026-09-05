/**
 * REST client for the agent-inbox twins on mcp-server
 * (`/internal/inbox/*` on mcp.swarm.tips). The twins share the MCP inbox's
 * storage layer, so shapes mirror the `agent_*` MCP tools exactly
 * (`from_wallet` is CAIP-10, `msg_id` doubles as the paging/ack cursor).
 *
 * Session + request flow (mirrors scripts/seed-inbox.ts):
 *
 *   createSession(wallet, signNonce)
 *        │
 *        ├─ localStorage hit? ──────────────► use cached {session_id, tier}
 *        │                                        │
 *        └─ POST /session {wallet} → {nonce}      │
 *           signNonce(nonce)  (ONE wallet popup)  │
 *           POST /session {wallet,nonce,sig}      │
 *                → {session_id, tier} → cache ────┤
 *                                                 ▼
 *   getMessages / send / ack ── X-Inbox-Session header
 *        │
 *        └─ 401? → drop cache → re-mint (popup) → retry ONCE → else throw
 *
 * The `signNonce` callback is chain-agnostic: it receives the raw nonce
 * string and must return the wire-format signature string. Solana =
 * bs58(ed25519 detached over the nonce UTF-8 bytes) — use `solanaNonceSigner`
 * from this package; EVM = the `personal_sign` hex string as-is.
 */

export type NonceSigner = (nonce: string) => Promise<string>;

export interface InboxSession {
  session_id: string;
  tier: string;
}

export interface InboxMessage {
  /** Message id; also the paging + ack cursor (lexically ordered). */
  msg_id: string;
  /** CAIP-10 sender, e.g. "solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp:BASE58". */
  from_wallet: string;
  thread_id: string;
  intent: string | null;
  body: string;
  /** RFC 3339 send timestamp. */
  sent_at: string;
  /** True for org seed-wallet traffic (server-tagged). */
  seed?: boolean;
  /** Present only under `include_sent` reads: "sent" = a mirror copy the
   *  connected wallet wrote, "received" = an inbound message. Absent on
   *  inbound-only reads (treat as "received"). */
  direction?: "received" | "sent";
}

export interface MessagePage {
  messages: InboxMessage[];
  next_cursor: string | null;
}

/** One post on a public topic board (open-challenge / subcontract). Bodies
 *  are third-party text — render as text, never HTML. */
export interface TopicPost {
  post_id: string;
  topic_id: string;
  /** CAIP-10 author id. */
  author_wallet: string;
  body: string;
  /** post_id this replies to, or null for a root post. */
  reply_to: string | null;
  intent: string | null;
  /** Unsigned-tx-flow / game / task id this post points at, or null. Drives
   *  the join/claim CTA. */
  ref_id: string | null;
  reported_count: number;
  /** RFC 3339 creation timestamp. */
  created_at: string;
  seed?: boolean;
}

export interface TopicPage {
  topic_id: string;
  posts: TopicPost[];
  next_cursor: string | null;
  /** Server-side count of posts dropped by the auto-hide moderation flag. */
  filtered_hidden?: number;
  /** Posts dropped for author trust below the requested `min_trust`. */
  filtered_below_min_trust?: number;
}

export interface PublishReceipt {
  published: boolean;
  post_id: string;
  posts_remaining_today?: number;
}

export interface ReportReceipt {
  reported: boolean;
  reported_count: number;
  hidden: boolean;
  already_reported: boolean;
}

/** Known public topic boards. `open-challenge` = game matchmaking,
 *  `subcontract` = Shillbot task handoff. */
export type TopicId = "open-challenge" | "subcontract";

export interface SendReceipt {
  msg_id: string;
  thread_id: string;
  sends_remaining_today?: number;
}

/** Non-2xx responses and client-side boundary rejections. `status` is the
 *  HTTP status, or 0 for errors raised before any request was made. */
export class InboxApiError extends Error {
  constructor(readonly status: number, message: string) {
    super(message);
    this.name = "InboxApiError";
  }
}

interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

export interface InboxClientOpts {
  /** REST base URL; default https://mcp.swarm.tips. Frontends thread their
   *  VITE_ env override through this (same pattern as API_BASE_URL). */
  baseUrl?: string;
  /** Session cache. Defaults to window.localStorage; pass null to disable. */
  storage?: StorageLike | null;
  /** fetch implementation — tests inject a mock. */
  fetchFn?: typeof fetch;
}

const DEFAULT_BASE_URL = "https://mcp.swarm.tips";
const SESSION_PATH = "/internal/inbox/session";
const MAX_PAGE_LIMIT = 50;
const MAX_BODY_BYTES = 4096;

function sessionCacheKey(wallet: string): string {
  return `swarm-inbox-session:${wallet}`;
}

function defaultStorage(): StorageLike | null {
  try {
    if (typeof window !== "undefined" && window.localStorage) {
      return window.localStorage;
    }
  } catch (e) {
    // Blocked storage (Safari private mode) — run uncached.
    console.debug("[inbox-client] localStorage unavailable", e);
  }
  return null;
}

async function parseBody<T>(res: Response): Promise<T> {
  const text = await res.text();
  if (!res.ok) {
    let message = text || res.statusText || `HTTP ${res.status}`;
    try {
      const parsed = JSON.parse(text) as { error?: string; message?: string };
      message = parsed.error ?? parsed.message ?? message;
    } catch {
      // Non-JSON error body — keep the raw text.
    }
    throw new InboxApiError(res.status, message);
  }
  try {
    return JSON.parse(text) as T;
  } catch {
    throw new InboxApiError(res.status, "invalid JSON in inbox response");
  }
}

export class InboxClient {
  private readonly baseUrl: string;
  private readonly storage: StorageLike | null;
  private readonly fetchFn: typeof fetch;
  private session: InboxSession | null = null;
  private wallet: string | null = null;
  private signNonce: NonceSigner | null = null;

  constructor(opts: InboxClientOpts = {}) {
    this.baseUrl = (opts.baseUrl ?? DEFAULT_BASE_URL).replace(/\/+$/, "");
    this.storage = opts.storage === undefined ? defaultStorage() : opts.storage;
    this.fetchFn =
      opts.fetchFn ??
      ((input: RequestInfo | URL, init?: RequestInit) => fetch(input, init));
  }

  /** True when a cached session exists for `wallet` — opening the inbox will
   *  not prompt a wallet signature. */
  hasCachedSession(wallet: string): boolean {
    return this.readCachedSession(wallet) !== null;
  }

  /** Mint (or restore from cache) the inbox session for `wallet`. Signing
   *  happens at most once per call — zero times on a cache hit. The wallet +
   *  signer are retained for transparent re-mint when a request 401s. */
  async createSession(
    wallet: string,
    signNonce: NonceSigner
  ): Promise<InboxSession> {
    if (!wallet) throw new InboxApiError(0, "wallet is required");
    this.wallet = wallet;
    this.signNonce = signNonce;
    const cached = this.readCachedSession(wallet);
    if (cached) {
      this.session = cached;
      return cached;
    }
    return this.mintSession();
  }

  /** Drop the in-memory and cached session (e.g. on wallet disconnect). */
  clearSession(wallet?: string): void {
    const key = wallet ?? this.wallet;
    this.session = null;
    if (key && this.storage) {
      try {
        this.storage.removeItem(sessionCacheKey(key));
      } catch (e) {
        console.debug("[inbox-client] session cache clear failed", e);
      }
    }
  }

  /** Read a page of messages, newest first. Omit `threadId` for the whole
   *  mailbox; pass the previous page's `next_cursor` to page older. */
  async getMessages(
    opts: {
      threadId?: string;
      cursor?: string;
      limit?: number;
      /** Merge the sender's own mirror copies into the page so a thread read
       *  shows both sides. Maps to `include_sent=true`. Sent messages carry
       *  `direction:"sent"`. */
      includeSent?: boolean;
    } = {}
  ): Promise<MessagePage> {
    const params = new URLSearchParams();
    if (opts.threadId) params.set("thread_id", opts.threadId);
    if (opts.cursor) params.set("cursor", opts.cursor);
    if (opts.includeSent) params.set("include_sent", "true");
    if (opts.limit !== undefined) {
      if (
        !Number.isInteger(opts.limit) ||
        opts.limit < 1 ||
        opts.limit > MAX_PAGE_LIMIT
      ) {
        throw new InboxApiError(
          0,
          `limit must be an integer in [1, ${MAX_PAGE_LIMIT}]`
        );
      }
      params.set("limit", String(opts.limit));
    }
    const qs = params.toString();
    const page = await this.authed<{
      messages?: InboxMessage[];
      next_cursor?: string | null;
    }>(`/internal/inbox/messages${qs ? `?${qs}` : ""}`, { method: "GET" });
    return {
      messages: page.messages ?? [],
      next_cursor: page.next_cursor ?? null,
    };
  }

  /** Read one thread (e.g. `task:{task_id}` / `game:{id}`), newest first. Pass
   *  `includeSent` to fold in the connected wallet's own replies (full
   *  two-way conversation). */
  async getThread(
    threadId: string,
    opts: { cursor?: string; limit?: number; includeSent?: boolean } = {}
  ): Promise<MessagePage> {
    if (!threadId) throw new InboxApiError(0, "threadId is required");
    return this.getMessages({ ...opts, threadId });
  }

  /** Send a message. Rejects empty / oversize bodies client-side; quota and
   *  mute rejections surface as InboxApiError with the server's message. */
  async send(
    toWallet: string,
    body: string,
    threadId?: string,
    intent?: string
  ): Promise<SendReceipt> {
    if (!toWallet) throw new InboxApiError(0, "toWallet is required");
    const bytes = new TextEncoder().encode(body).length;
    if (bytes === 0 || bytes > MAX_BODY_BYTES) {
      throw new InboxApiError(
        0,
        `body must be 1..${MAX_BODY_BYTES} bytes (got ${bytes})`
      );
    }
    const payload: Record<string, unknown> = { to_wallet: toWallet, body };
    if (threadId) payload["thread_id"] = threadId;
    if (intent) payload["intent"] = intent;
    return this.authed<SendReceipt>("/internal/inbox/send", {
      method: "POST",
      body: JSON.stringify(payload),
    });
  }

  /** Advance the read watermark up to a msg_id cursor. NOTE: the watermark is
   *  mailbox-global — acking a cursor also acks every older message. */
  async ack(upToCursor: string): Promise<void> {
    if (!upToCursor) throw new InboxApiError(0, "upToCursor is required");
    await this.authed<Record<string, unknown>>("/internal/inbox/ack", {
      method: "POST",
      body: JSON.stringify({ up_to_cursor: upToCursor }),
    });
  }

  // -- topic boards ---------------------------------------------------------

  /** Read a page of a public topic board, newest first. OPEN — no session
   *  needed. `minTrust` drops posts whose author trust is below it
   *  server-side. Hidden (auto-moderated) posts are always dropped. */
  async readTopic(
    topicId: TopicId,
    opts: { cursor?: string; limit?: number; minTrust?: number } = {}
  ): Promise<TopicPage> {
    if (!topicId) throw new InboxApiError(0, "topicId is required");
    const params = new URLSearchParams({ topic_id: topicId });
    if (opts.cursor) params.set("cursor", opts.cursor);
    if (opts.minTrust !== undefined)
      params.set("min_trust", String(opts.minTrust));
    if (opts.limit !== undefined) {
      if (
        !Number.isInteger(opts.limit) ||
        opts.limit < 1 ||
        opts.limit > MAX_PAGE_LIMIT
      ) {
        throw new InboxApiError(
          0,
          `limit must be an integer in [1, ${MAX_PAGE_LIMIT}]`
        );
      }
      params.set("limit", String(opts.limit));
    }
    const page = await this.getUnauthed<{
      topic_id?: string;
      posts?: TopicPost[];
      next_cursor?: string | null;
      filtered_hidden?: number;
      filtered_below_min_trust?: number;
    }>(`/internal/topics/read?${params.toString()}`);
    return {
      topic_id: page.topic_id ?? topicId,
      posts: page.posts ?? [],
      next_cursor: page.next_cursor ?? null,
      filtered_hidden: page.filtered_hidden,
      filtered_below_min_trust: page.filtered_below_min_trust,
    };
  }

  /** Publish a post to a topic board. Session-gated (same X-Inbox-Session as
   *  inbox send). Rejects empty / oversize bodies client-side; quota
   *  rejections surface as InboxApiError with the server's message. */
  async publishPost(
    topicId: TopicId,
    body: string,
    opts: { replyTo?: string; intent?: string; refId?: string } = {}
  ): Promise<PublishReceipt> {
    if (!topicId) throw new InboxApiError(0, "topicId is required");
    const bytes = new TextEncoder().encode(body).length;
    if (bytes === 0 || bytes > MAX_BODY_BYTES) {
      throw new InboxApiError(
        0,
        `body must be 1..${MAX_BODY_BYTES} bytes (got ${bytes})`
      );
    }
    const payload: Record<string, unknown> = { topic_id: topicId, body };
    if (opts.replyTo) payload["reply_to"] = opts.replyTo;
    if (opts.intent) payload["intent"] = opts.intent;
    if (opts.refId) payload["ref_id"] = opts.refId;
    return this.authed<PublishReceipt>("/internal/topics/publish", {
      method: "POST",
      body: JSON.stringify(payload),
    });
  }

  /** Report a post for moderation. Session-gated. Idempotent per reporter —
   *  a repeat report returns `already_reported: true`. */
  async reportPost(topicId: TopicId, postId: string): Promise<ReportReceipt> {
    if (!topicId) throw new InboxApiError(0, "topicId is required");
    if (!postId) throw new InboxApiError(0, "postId is required");
    return this.authed<ReportReceipt>("/internal/topics/report", {
      method: "POST",
      body: JSON.stringify({ topic_id: topicId, post_id: postId }),
    });
  }

  // -- internals ------------------------------------------------------------

  private async getUnauthed<T>(path: string): Promise<T> {
    const res = await this.fetchFn(`${this.baseUrl}${path}`, { method: "GET" });
    return parseBody<T>(res);
  }

  private async mintSession(): Promise<InboxSession> {
    const { wallet, signNonce } = this;
    if (!wallet || !signNonce) {
      throw new InboxApiError(
        0,
        "createSession(wallet, signNonce) must be called first"
      );
    }
    const challenge = await this.postUnauthed<{ nonce?: string }>(
      SESSION_PATH,
      { wallet }
    );
    if (!challenge.nonce) {
      throw new InboxApiError(0, "session challenge returned no nonce");
    }
    const signature = await signNonce(challenge.nonce);
    const verified = await this.postUnauthed<{
      session_id?: string;
      tier?: string;
    }>(SESSION_PATH, { wallet, nonce: challenge.nonce, signature });
    if (!verified.session_id) {
      throw new InboxApiError(0, "session verify returned no session_id");
    }
    const session: InboxSession = {
      session_id: verified.session_id,
      tier: verified.tier ?? "session",
    };
    this.session = session;
    this.writeCachedSession(wallet, session);
    return session;
  }

  private async postUnauthed<T>(
    path: string,
    body: Record<string, unknown>
  ): Promise<T> {
    const res = await this.fetchFn(`${this.baseUrl}${path}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
    return parseBody<T>(res);
  }

  private async authed<T>(
    path: string,
    init: { method: string; body?: string }
  ): Promise<T> {
    const session = this.session;
    if (!session) {
      throw new InboxApiError(0, "no inbox session — call createSession first");
    }
    const res = await this.doFetch(path, init, session.session_id);
    if (res.status === 401) {
      // Cached session expired or was invalidated server-side: drop it,
      // re-mint (one signer callback → one wallet popup), retry ONCE.
      this.clearSession();
      const fresh = await this.mintSession();
      const retry = await this.doFetch(path, init, fresh.session_id);
      return parseBody<T>(retry);
    }
    return parseBody<T>(res);
  }

  private doFetch(
    path: string,
    init: { method: string; body?: string },
    sessionId: string
  ): Promise<Response> {
    const headers: Record<string, string> = { "X-Inbox-Session": sessionId };
    if (init.body !== undefined) headers["Content-Type"] = "application/json";
    return this.fetchFn(`${this.baseUrl}${path}`, { ...init, headers });
  }

  private readCachedSession(wallet: string): InboxSession | null {
    if (!this.storage) return null;
    try {
      const raw = this.storage.getItem(sessionCacheKey(wallet));
      if (!raw) return null;
      const parsed = JSON.parse(raw) as Partial<InboxSession>;
      if (
        typeof parsed.session_id !== "string" ||
        parsed.session_id.length === 0
      ) {
        return null;
      }
      return { session_id: parsed.session_id, tier: parsed.tier ?? "session" };
    } catch (e) {
      console.debug("[inbox-client] session cache read failed", e);
      return null;
    }
  }

  private writeCachedSession(wallet: string, session: InboxSession): void {
    if (!this.storage) return;
    try {
      this.storage.setItem(sessionCacheKey(wallet), JSON.stringify(session));
    } catch (e) {
      console.debug("[inbox-client] session cache write failed", e);
    }
  }
}
