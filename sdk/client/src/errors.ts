export type SwarmClientErrorCode =
  | "INVALID_ARGUMENT"
  | "INVALID_RESPONSE"
  | "NETWORK_ERROR"
  | "HTTP_ERROR"
  | "TIMEOUT"
  | "TRANSACTION_MISMATCH"
  | "TRANSACTION_FAILED"
  | "STORAGE_ERROR"
  | "WEBSOCKET_ERROR";

export interface SwarmClientErrorOptions {
  code: SwarmClientErrorCode;
  operation: string;
  message: string;
  retryable?: boolean;
  cause?: unknown;
  status?: number;
}

/** Stable error boundary shared by every headless client entrypoint. */
export class SwarmClientError extends Error {
  readonly code: SwarmClientErrorCode;
  readonly operation: string;
  readonly retryable: boolean;
  readonly cause?: unknown;
  readonly status?: number;

  constructor(options: SwarmClientErrorOptions) {
    super(options.message);
    this.name = "SwarmClientError";
    this.code = options.code;
    this.operation = options.operation;
    this.retryable = options.retryable ?? false;
    this.cause = options.cause;
    this.status = options.status;
  }
}

export function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error && error.message ? error.message : fallback;
}
