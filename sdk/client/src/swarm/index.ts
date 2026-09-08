import { HttpClient, type FetchLike } from "../http.js";

export interface AgentJob {
  source: string;
  source_id: string;
  source_url: string;
  title: string;
  description: string;
  category: string;
  vertical?: string;
  tags: string[];
  reward_amount: string;
  reward_token: string;
  reward_chain: string;
  reward_usd_estimate?: number;
  payment_model: string;
  escrow: boolean;
  posted_at: string;
  deadline?: string | null;
  status: string;
  claims_remaining?: number | null;
  required_capabilities?: string[];
  client_address?: string;
  how_to_start?: string;
  indexed_at: string;
  slots_open?: number;
}

export interface RankingSignals {
  bm25_score: number;
  final_score: number;
  quality_prior: number;
  source_count: number;
  github_stars: number | null;
  npm_weekly_downloads: number | null;
  upstream_quality_score: number | null;
  llm_confidence: number | null;
}
export interface SearchHit {
  name: string;
  title: string | null;
  description: string | null;
  endpoint: string | null;
  transport: string | null;
  github_repo: string | null;
  npm_package: string | null;
  category: string | null;
  currencies: string[];
  provenance: string;
  ranking_signals: RankingSignals;
}
export interface EigenTrustRecord {
  wallet: string;
  eigentrust_score: number;
  rank: number;
  rank_normalized: number;
  settlements_received: number;
  settlements_paid: number;
  counterparty_count: number;
  computed_at: string;
}
export interface AgentReputation {
  wallet: string;
  web_position: number | null;
  extensions_received: number;
  has_standing: boolean;
  eigentrust?: EigenTrustRecord | null;
}

export interface SwarmClientOptions {
  baseUrl: string;
  fetch?: FetchLike;
}

type RpcEnvelope = { result?: { tools?: unknown[] } };
function parseRpc(body: string): RpcEnvelope | null {
  const trimmed = body.trimStart();
  if (trimmed.startsWith("{")) {
    try { return JSON.parse(trimmed) as RpcEnvelope; } catch { return null; }
  }
  for (const line of body.split("\n")) {
    const value = line.trim();
    if (!value.startsWith("data:")) continue;
    try { return JSON.parse(value.slice(5).trim()) as RpcEnvelope; } catch { /* continue */ }
  }
  return null;
}

export class SwarmClient {
  readonly http: HttpClient;
  private readonly fetchImpl: FetchLike;
  readonly baseUrl: string;

  constructor(options: SwarmClientOptions) {
    this.baseUrl = options.baseUrl.replace(/\/$/, "");
    this.fetchImpl = (options.fetch ?? globalThis.fetch).bind(globalThis);
    this.http = new HttpClient(options);
  }

  async listings(): Promise<AgentJob[]> { return this.http.get<AgentJob[]>("/internal/listings"); }
  async search(query: string, limit = 30): Promise<{ results: SearchHit[]; corpus_size: number }> {
    const params = new URLSearchParams();
    if (query) params.set("query", query);
    params.set("limit", String(limit));
    return this.http.get(`/internal/mcp/search?${params.toString()}`);
  }
  reputation(wallet: string, network = "mainnet") {
    if (!wallet) return Promise.reject(new TypeError("wallet is required"));
    return this.http.get<AgentReputation>(`/internal/agent-reputation?wallet=${encodeURIComponent(wallet)}&network=${encodeURIComponent(network)}`);
  }
  async leaderboard(limit = 25): Promise<EigenTrustRecord[]> {
    const body = await this.http.get<{ agents?: EigenTrustRecord[] }>(`/internal/reputation/leaderboard?limit=${encodeURIComponent(String(limit))}`);
    return body.agents ?? [];
  }

  async toolCount(): Promise<number | null> {
    const headers = { "Content-Type": "application/json", Accept: "application/json, text/event-stream" };
    try {
      const init = await this.fetchImpl(`${this.baseUrl}/mcp`, { method: "POST", headers, body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-03-26", capabilities: {}, clientInfo: { name: "swarm-client", version: "0.2.0" } } }) });
      if (!init.ok) return null;
      const session = init.headers.get("mcp-session-id");
      await init.text();
      if (!session) return null;
      const sessionHeaders = { ...headers, "mcp-session-id": session };
      await this.fetchImpl(`${this.baseUrl}/mcp`, { method: "POST", headers: sessionHeaders, body: JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized" }) });
      const listed = await this.fetchImpl(`${this.baseUrl}/mcp`, { method: "POST", headers: sessionHeaders, body: JSON.stringify({ jsonrpc: "2.0", id: 2, method: "tools/list", params: {} }) });
      if (!listed.ok) return null;
      const tools = parseRpc(await listed.text())?.result?.tools;
      return Array.isArray(tools) && tools.length ? tools.length : null;
    } catch { return null; }
  }
}
