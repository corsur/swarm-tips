import { HttpClient, type HttpClientOptions } from "../http.js";
import { SwarmClientError } from "../errors.js";

export interface ChallengeRequest { wallet: string }
export interface ChallengeResponse { nonce: string }
export interface VerifyRequest { wallet: string; nonce: string; signature: string }
export interface VerifyResponse { token: string }
export interface SessionAuthRequest { wallet: string; tx_signature: string; nonce: string }
export interface SessionAuthResponse { token: string }
export interface QueueRequest { tournament_id: number; is_ai?: boolean }
export type QueueJoinResponse = { matched: true; session_id: string } | { matched: false };
export interface OkResponse { ok: boolean }

export interface XLegPayload {
  chain: string;
  contract: string;
  player: string;
  session_key: string;
  stake_base_units: string;
  tranche_base_units: string;
}

export interface XMatchPayload {
  match_id: string;
  tournament_id: number;
  match_live_digest: string;
  operator_signature: string;
  create_match_operator_sig: string;
  fund_deadline: number;
  match_deadline: number;
  claim_window_secs: number;
  a_is_p1: number;
  leg_a: XLegPayload;
  leg_b: XLegPayload;
}

export interface XQueueResponse {
  status: "waiting" | "matched";
  match?: XMatchPayload;
  poll_handle?: string;
}

export interface XGameplayView {
  match_id: string;
  step2_checkpoint?: unknown;
  step2_checkpoint_digest?: string;
  r_matchup?: string;
  terminal_checkpoint?: unknown;
  terminal_checkpoint_digest?: string;
}

export interface EvmCall { player: string; to: string; data: string; value_wei: string }
export interface EvmMatchPayload {
  game_id: string;
  matchup_commitment: string;
  chain: string;
  contract: string;
  stake_base_units: string;
  create_call: EvmCall;
  join_call: EvmCall;
}
export interface EvmJoinResponse {
  status: string;
  match?: EvmMatchPayload;
  session_id?: string;
}

export class CoordinationGameApiClient {
  readonly http: HttpClient;

  constructor(options: HttpClientOptions) {
    this.http = new HttpClient(options);
  }

  authChallenge(body: ChallengeRequest) { return this.http.post<ChallengeResponse>("/auth/challenge", body); }
  authSession(body: SessionAuthRequest) { return this.http.post<SessionAuthResponse>("/auth/session", body); }
  evmAuthChallenge(body: ChallengeRequest) { return this.http.post<ChallengeResponse>("/auth/evm/challenge", body); }
  evmAuthVerify(body: VerifyRequest) { return this.http.post<VerifyResponse>("/auth/evm/verify", body); }
  joinQueue(body: QueueRequest, token: string) { return this.http.post<QueueJoinResponse>("/queue/join", body, token); }
  leaveQueue(body: QueueRequest, token: string) { return this.http.post<OkResponse>("/queue/leave", body, token); }
  gameStarted(body: { game_id: number; session_id: string }, token: string) { return this.http.post<OkResponse>("/games/started", body, token); }
  gameJoined(body: { game_id: number; session_id: string }, token: string) { return this.http.post<OkResponse>("/games/joined", body, token); }
  cosign(body: { session_id: string; message: string }, token: string) { return this.http.post<{ signature: string }>("/games/cosign", body, token); }
  cosignJoin(body: { session_id: string; message: string }, token: string) { return this.http.post<{ signature: string }>("/games/cosign-join", body, token); }
  committed(body: { session_id: string }, token: string) { return this.http.post<{ ok: boolean; both_committed: boolean }>("/games/committed", body, token); }
  bothCommitted(body: { session_id: string }, token: string) { return this.http.post<{ r_matchup: string }>("/games/both-committed", body, token); }
  resolved(body: Record<string, number>, token: string) { return this.http.post<OkResponse>("/games/resolved", body, token); }

  joinCrossChainQueue(body: { wallet: string; chain: string; session_key: string; tournament_id: number }) {
    return this.http.post<XQueueResponse>("/internal/xqueue/join", body);
  }
  crossChainStatus(by: { handle: string } | { wallet: string }) {
    const query = "handle" in by
      ? `handle=${encodeURIComponent(by.handle)}`
      : `wallet=${encodeURIComponent(by.wallet)}`;
    return this.http.get<XQueueResponse>(`/internal/xqueue/status?${query}`);
  }
  crossChainCommit(body: { wallet: string; commit: string; handle?: string }) {
    return this.http.post<{ status: string; both_committed: boolean }>("/internal/xqueue/commit", body);
  }
  crossChainSign(body: { wallet: string; step: number; signature: string; handle?: string }) {
    return this.http.post<{ status: string; relayed: boolean; r_matchup: string | null }>("/internal/xqueue/sign", body);
  }
  crossChainReveal(body: { wallet: string; preimage: string; handle?: string }) {
    return this.http.post<{ status: string; both_revealed: boolean }>("/internal/xqueue/reveal", body);
  }
  crossChainGameplay(by: { handle: string } | { wallet: string }) {
    const query = "handle" in by
      ? `handle=${encodeURIComponent(by.handle)}`
      : `wallet=${encodeURIComponent(by.wallet)}`;
    return this.http.get<XGameplayView>(`/internal/xqueue/gameplay?${query}`);
  }
  joinEvmGame(body: { wallet: string; player_wallet?: string; chain: string; tournament_id: number }) {
    return this.http.post<EvmJoinResponse>("/internal/evmgame/join", body);
  }
  evmGameStatus(wallet: string) {
    return this.http.get<EvmJoinResponse>(`/internal/evmgame/status?wallet=${encodeURIComponent(wallet)}`);
  }
  evmGameCommitted(body: { game_id: string; wallet: string }) {
    return this.http.post<{ status: string; both_committed: boolean; r_matchup: string | null }>("/internal/evmgame/committed", body);
  }
  evmGameStaked(body: { game_id: string }) {
    return this.http.post<{ status: string }>("/internal/evmgame/staked", body);
  }
}

export type WebSocketLike = {
  readyState: number;
  onopen: (() => void) | null;
  onclose: (() => void) | null;
  onmessage: ((event: { data: unknown }) => void) | null;
  onerror: ((event: unknown) => void) | null;
  send(data: string): void;
  close(): void;
};

export type WebSocketFactory = (url: string) => WebSocketLike;
export type ServerMessage =
  | { type: "chat"; text: string; from: string }
  | { type: "match_found"; session_id: string; role: number; matchup_commitment?: string }
  | { type: "game_ready"; game_id: number }
  | { type: "both_staked" }
  | { type: "reveal_data"; r_matchup: string }
  | { type: "opponent_committed" }
  | { type: "game_resolved"; p1_guess: number; p2_guess: number; matchup_type: number; first_committer: number }
  | { type: "opponent_disconnected" | "opponent_reconnected" | "pong" }
  | { type: "error"; message: string };
export type ClientMessage = { type: "chat"; text: string } | { type: "ping" };

export interface CoordinationGameWebSocketOptions {
  baseUrl: string;
  token: string;
  network?: string;
  session?: string;
  webSocket?: WebSocketFactory;
  setTimeout?: typeof globalThis.setTimeout;
  clearTimeout?: typeof globalThis.clearTimeout;
  setInterval?: typeof globalThis.setInterval;
  clearInterval?: typeof globalThis.clearInterval;
  pingIntervalMs?: number;
  maxRetries?: number;
}

export class CoordinationGameWebSocketClient {
  private socket: WebSocketLike | null = null;
  private pingTimer: ReturnType<typeof globalThis.setInterval> | null = null;
  private retryCount = 0;
  private closed = false;
  private readonly outbox: ClientMessage[] = [];
  onMessage: ((message: ServerMessage) => void) | null = null;
  onOpen: (() => void) | null = null;
  onClose: (() => void) | null = null;

  constructor(private readonly options: CoordinationGameWebSocketOptions) {}

  connect(): void {
    if (this.closed) return;
    const factory = this.options.webSocket ?? ((url) => new globalThis.WebSocket(url) as unknown as WebSocketLike);
    if (!factory) throw new SwarmClientError({ code: "INVALID_ARGUMENT", operation: "websocket.connect", message: "A WebSocket factory is required" });
    const query = new URLSearchParams({ token: this.options.token });
    if (this.options.network && this.options.network !== "mainnet") query.set("network", this.options.network);
    if (this.options.session) query.set("session", this.options.session);
    const url = `${this.options.baseUrl.replace(/^http/, "ws").replace(/\/$/, "")}/ws?${query.toString().replace(/%3A/g, ":")}`;
    this.socket = factory(url);
    this.socket.onopen = () => {
      this.retryCount = 0;
      this.startPing();
      this.flush();
      this.onOpen?.();
    };
    this.socket.onmessage = (event) => {
      try { this.onMessage?.(JSON.parse(String(event.data)) as ServerMessage); }
      catch { /* malformed peer data is ignored */ }
    };
    this.socket.onclose = () => {
      this.stopPing();
      this.onClose?.();
      if (!this.closed && this.retryCount < (this.options.maxRetries ?? 10)) {
        const delay = Math.min(2 ** this.retryCount * 1_000, 10_000);
        this.retryCount += 1;
        (this.options.setTimeout ?? globalThis.setTimeout)(() => this.connect(), delay);
      }
    };
  }

  send(message: ClientMessage): void {
    if (this.socket?.readyState === 1) this.socket.send(JSON.stringify(message));
    else if (!this.closed && message.type !== "ping" && this.outbox.length < 100) this.outbox.push(message);
  }

  close(): void {
    this.closed = true;
    this.outbox.length = 0;
    this.stopPing();
    this.socket?.close();
    this.socket = null;
  }

  get connected(): boolean { return this.socket?.readyState === 1; }

  waitForConnection(timeoutMs = 30_000): Promise<void> {
    if (this.connected) return Promise.resolve();
    return new Promise((resolve, reject) => {
      const timeout = (this.options.setTimeout ?? globalThis.setTimeout)(() => reject(new SwarmClientError({ code: "TIMEOUT", operation: "websocket.connect", message: "WebSocket connection timeout", retryable: true })), timeoutMs);
      const previous = this.onOpen;
      this.onOpen = () => {
        (this.options.clearTimeout ?? globalThis.clearTimeout)(timeout);
        this.onOpen = previous;
        previous?.();
        resolve();
      };
    });
  }

  private flush(): void {
    while (this.outbox.length && this.socket?.readyState === 1) this.socket.send(JSON.stringify(this.outbox.shift()!));
  }
  private startPing(): void {
    this.pingTimer = (this.options.setInterval ?? globalThis.setInterval)(() => this.send({ type: "ping" }), this.options.pingIntervalMs ?? 10_000);
  }
  private stopPing(): void {
    if (this.pingTimer !== null) (this.options.clearInterval ?? globalThis.clearInterval)(this.pingTimer);
    this.pingTimer = null;
  }
}
