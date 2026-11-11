import type { Transaction, VersionedTransaction } from "@solana/web3.js";

export interface SolanaSwarmSigner {
  readonly chain: "solana";
  readonly address: string;
  signMessage(message: Uint8Array): Promise<Uint8Array>;
  signTransaction<T extends Transaction | VersionedTransaction>(tx: T): Promise<T>;
}

export interface EvmCallRequest {
  to: `0x${string}`;
  data: `0x${string}`;
  value?: bigint;
  chainId?: number;
}

export interface EvmSwarmSigner {
  readonly chain: "evm";
  readonly address: `0x${string}`;
  readonly chainId?: number;
  signMessage(message: string | Uint8Array): Promise<`0x${string}`>;
  sendTransaction?(call: EvmCallRequest): Promise<`0x${string}`>;
}

export type SwarmSigner = SolanaSwarmSigner | EvmSwarmSigner;

const NODE_INSPECT_CUSTOM = Symbol.for("nodejs.util.inspect.custom");

/**
 * C6 Redaction: attach safe serialization and inspection hooks while freezing
 * the public signer surface so secret key material can never leak into JSON,
 * string formatting, or `util.inspect` logs.
 */
export function redactSigner<T extends { readonly chain: string; readonly address: string }>(
  signer: T,
): T {
  const { chain, address } = signer;
  const label = `SwarmSigner(${chain}:${address})`;
  Object.defineProperties(signer, {
    toJSON: {
      value: () => ({ chain, address }),
      enumerable: false,
      configurable: false,
      writable: false,
    },
    toString: {
      value: () => label,
      enumerable: false,
      configurable: false,
      writable: false,
    },
    [NODE_INSPECT_CUSTOM]: {
      value: () => label,
      enumerable: false,
      configurable: false,
      writable: false,
    },
  });
  return Object.freeze(signer);
}
