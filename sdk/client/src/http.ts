import { SwarmClientError, errorMessage } from "./errors.js";

export type FetchLike = (input: RequestInfo | URL, init?: RequestInit) => Promise<Response>;

export interface HttpClientOptions {
  baseUrl: string;
  fetch?: FetchLike;
  network?: string;
  getToken?: () => Promise<string | null | undefined> | string | null | undefined;
  onUnauthorized?: () => Promise<void> | void;
}

export class HttpClient {
  readonly baseUrl: string;
  readonly network?: string;
  readonly fetchImpl: FetchLike;
  readonly getToken?: () => Promise<string | null | undefined> | string | null | undefined;
  private readonly onUnauthorized?: () => Promise<void> | void;

  constructor(options: HttpClientOptions) {
    this.baseUrl = options.baseUrl.replace(/\/$/, "");
    this.network = options.network;
    const candidate = options.fetch ?? globalThis.fetch;
    if (!candidate) throw new SwarmClientError({
      code: "INVALID_ARGUMENT",
      operation: "http.constructor",
      message: "A fetch implementation is required",
    });
    this.fetchImpl = options.fetch
      ? (input, init) => options.fetch!(input, init)
      : candidate.bind(globalThis);
    this.getToken = options.getToken;
    this.onUnauthorized = options.onUnauthorized;
  }

  withNetwork(path: string): string {
    if (!this.network || this.network === "mainnet" || /[?&]network=/.test(path)) return path;
    const separator = path.includes("?") ? "&" : "?";
    return `${path}${separator}network=${encodeURIComponent(this.network)}`;
  }

  async request<T>(
    path: string,
    init: RequestInit & { token?: string | null; _retried?: boolean } = {},
  ): Promise<T> {
    const operation = `${init.method ?? "GET"} ${path}`;
    const token = init.token ?? (await this.getToken?.());
    const { token: _token, _retried, ...fetchInit } = init;
    const headers: Record<string, string> = {
      ...(fetchInit.headers as Record<string, string> | undefined),
    };
    // A Content-Type header on a bodyless GET turns an otherwise simple
    // cross-origin read into a CORS preflight. Public browser reads should not
    // require OPTIONS for a body that does not exist.
    if (fetchInit.body !== undefined && !headers["Content-Type"]) {
      headers["Content-Type"] = "application/json";
    }
    if (token) headers.Authorization = `Bearer ${token}`;
    let response: Response;
    try {
      response = await this.fetchImpl(`${this.baseUrl}${this.withNetwork(path)}`, {
        ...fetchInit,
        headers,
      });
    } catch (cause) {
      throw new SwarmClientError({
        code: "NETWORK_ERROR",
        operation,
        message: errorMessage(cause, "Network request failed"),
        retryable: true,
        cause,
      });
    }
    if (
      response.status === 401 &&
      !_retried &&
      init.token === undefined &&
      this.onUnauthorized &&
      this.getToken
    ) {
      await this.onUnauthorized();
      const freshToken = await this.getToken();
      if (freshToken && freshToken !== token) {
        return this.request<T>(path, { ...init, _retried: true });
      }
    }
    if (!response.ok) {
      const raw = await response.text().catch(() => "");
      let message = `HTTP ${response.status}`;
      try {
        const body = JSON.parse(raw) as { message?: string; error?: string };
        message = body.message ?? body.error ?? message;
      } catch {
        if (raw.trim()) message = raw.slice(0, 500);
      }
      throw new SwarmClientError({
        code: "HTTP_ERROR",
        operation,
        message,
        retryable: response.status === 408 || response.status === 429 || response.status >= 500,
        status: response.status,
      });
    }
    if (response.status === 204) return undefined as T;
    try {
      return (await response.json()) as T;
    } catch (cause) {
      throw new SwarmClientError({
        code: "INVALID_RESPONSE",
        operation,
        message: "Response was not valid JSON",
        cause,
      });
    }
  }

  get<T>(path: string, token?: string | null): Promise<T> {
    return this.request(path, { method: "GET", token });
  }

  post<T>(path: string, body?: unknown, token?: string | null): Promise<T> {
    return this.request(path, {
      method: "POST",
      body: body === undefined ? undefined : JSON.stringify(body),
      token,
    });
  }

  patch<T>(path: string, body?: unknown, token?: string | null): Promise<T> {
    return this.request(path, {
      method: "PATCH",
      body: body === undefined ? undefined : JSON.stringify(body),
      token,
    });
  }
}
