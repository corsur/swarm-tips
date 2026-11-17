import { readFileSync, statSync } from "node:fs";
import { resolve } from "node:path";
import { fromEvmPrivateKey } from "../signer/evm.js";
import { fromSolanaSecretKey } from "../signer/solana.js";
import type { SwarmSigner } from "../signer/types.js";

export class CliUsageError extends Error {
  readonly exitCode = 2;

  constructor(message: string) {
    super(message);
    this.name = "CliUsageError";
  }
}

const FORBIDDEN_SECRET_FLAGS = [
  "--private-key",
  "--secret-key",
  "--mnemonic",
  "--seed",
] as const;

/**
 * C6 CLI security invariant: reject raw secret material passed on argv
 * (visible to `ps` and shell history) without ever echoing the secret value.
 */
export function assertNoSecretCliFlags(argv: readonly string[]): void {
  for (const arg of argv) {
    const lower = arg.toLowerCase();
    for (const forbidden of FORBIDDEN_SECRET_FLAGS) {
      if (lower === forbidden || lower.startsWith(`${forbidden}=`)) {
        throw new CliUsageError(
          `Refusing ${forbidden} on the command line (argv is visible in process listings and shell history). ` +
            `Pass --keypair-file <path> (chmod 600) or set SWARM_SOLANA_KEYPAIR_PATH, SWARM_SOLANA_SECRET_KEY, or SWARM_EVM_PRIVATE_KEY.`,
        );
      }
    }
  }
}

/**
 * Verify keyfile exists and is owner-only (`0o600` / `0o400`) on POSIX before reading.
 */
export function readSecureKeypairFile(filePath: string): string {
  const resolved = resolve(filePath);
  let stat;
  try {
    stat = statSync(resolved);
  } catch (err) {
    throw new CliUsageError(
      `Cannot access keypair file ${resolved}: ${err instanceof Error ? err.message : String(err)}`,
    );
  }
  if (!stat.isFile()) {
    throw new CliUsageError(`Keypair path ${resolved} is not a regular file.`);
  }
  if (process.platform !== "win32" && (stat.mode & 0o077) !== 0) {
    const modeOctal = (stat.mode & 0o777).toString(8).padStart(3, "0");
    throw new CliUsageError(
      `Refusing keypair file ${resolved} with insecure permissions 0${modeOctal} (group/world accessible). Run: chmod 600 ${resolved}`,
    );
  }
  try {
    return readFileSync(resolved, "utf8").trim();
  } catch (err) {
    throw new CliUsageError(
      `Failed to read keypair file ${resolved}: ${err instanceof Error ? err.message : String(err)}`,
    );
  }
}

export interface LoadCliSignerOptions {
  keypairFile?: string;
  env?: Record<string, string | undefined>;
  evmChainId?: number | string;
  evmRpcUrl?: string;
}

/**
 * Load a non-custodial `SwarmSigner` from `--keypair-file` (Solana JSON/base58 or
 * `0x`-prefixed 32-byte hex EVM private key) or `SWARM_*` environment variables.
 */
export function loadSignerFromCliOrEnv(options: LoadCliSignerOptions = {}): SwarmSigner | undefined {
  const env = options.env ?? process.env;
  const resolvedEvmChainId = options.evmChainId ?? env.SWARM_EVM_CHAIN_ID?.trim();
  const resolvedEvmRpcUrl = options.evmRpcUrl ?? env.SWARM_EVM_RPC_URL?.trim();

  const keyfilePath =
    options.keypairFile ?? env.SWARM_SOLANA_KEYPAIR_PATH ?? env.SWARM_EVM_KEYPAIR_PATH;
  if (keyfilePath) {
    const raw = readSecureKeypairFile(keyfilePath);
    if (/^0x/i.test(raw)) {
      if (!/^0x[0-9a-fA-F]{64}$/.test(raw)) {
        throw new CliUsageError(
          `Invalid EVM private key in ${resolve(keyfilePath)}: Expected a 0x-prefixed 32-byte hex EVM private key`,
        );
      }
      try {
        return fromEvmPrivateKey(raw as `0x${string}`, {
          chainId: resolvedEvmChainId,
          rpcUrl: resolvedEvmRpcUrl,
        });
      } catch (err) {
        throw new CliUsageError(
          `Invalid EVM private key in ${resolve(keyfilePath)}: ${err instanceof Error ? err.message : String(err)}`,
        );
      }
    }
    try {
      return fromSolanaSecretKey(raw);
    } catch (err) {
      throw new CliUsageError(
        `Invalid Solana keypair in ${resolve(keyfilePath)}: ${err instanceof Error ? err.message : String(err)}`,
      );
    }
  }

  const solSecret = env.SWARM_SOLANA_SECRET_KEY?.trim();
  if (solSecret) {
    try {
      return fromSolanaSecretKey(solSecret);
    } catch (err) {
      throw new CliUsageError(
        `Invalid SWARM_SOLANA_SECRET_KEY: ${err instanceof Error ? err.message : String(err)}`,
      );
    }
  }

  const evmPriv = env.SWARM_EVM_PRIVATE_KEY?.trim();
  if (evmPriv) {
    const normalized = (evmPriv.startsWith("0x") ? evmPriv : `0x${evmPriv}`) as `0x${string}`;
    try {
      return fromEvmPrivateKey(normalized, {
        chainId: resolvedEvmChainId,
        rpcUrl: resolvedEvmRpcUrl,
      });
    } catch (err) {
      throw new CliUsageError(
        `Invalid SWARM_EVM_PRIVATE_KEY: ${err instanceof Error ? err.message : String(err)}`,
      );
    }
  }

  return undefined;
}
