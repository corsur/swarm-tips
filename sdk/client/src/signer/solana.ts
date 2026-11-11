import bs58 from "bs58";
import {
  Ed25519Program,
  Keypair,
  VersionedTransaction,
  type PublicKey,
  type Transaction,
} from "@solana/web3.js";
import { SwarmClientError } from "../errors.js";
import { redactSigner, type SolanaSwarmSigner } from "./types.js";

const ED25519_PKCS8_PREFIX = new Uint8Array([
  0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06,
  0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
]);

async function signEd25519Detached(
  secretKey64: Uint8Array,
  message: Uint8Array,
): Promise<Uint8Array> {
  if (globalThis.crypto?.subtle) {
    try {
      const pkcs8 = new Uint8Array(48);
      pkcs8.set(ED25519_PKCS8_PREFIX, 0);
      pkcs8.set(secretKey64.subarray(0, 32), 16);
      const key = await globalThis.crypto.subtle.importKey(
        "pkcs8",
        pkcs8,
        { name: "Ed25519" },
        false,
        ["sign"],
      );
      pkcs8.fill(0);
      const payload = new Uint8Array(message.byteLength);
      payload.set(message);
      const sig = await globalThis.crypto.subtle.sign("Ed25519", key, payload);
      return new Uint8Array(sig);
    } catch {
      // Fall back to @solana/web3.js Ed25519Program below on runtimes without WebCrypto Ed25519.
    }
  }
  const ix = Ed25519Program.createInstructionWithPrivateKey({
    privateKey: secretKey64,
    message,
  });
  const sigOffset = ix.data[2] | (ix.data[3] << 8);
  return new Uint8Array(ix.data.subarray(sigOffset, sigOffset + 64));
}

export interface SolanaWalletAdapterLike {
  publicKey: PublicKey | { toBase58(): string } | null | undefined;
  signMessage?: (message: Uint8Array) => Promise<Uint8Array>;
  signTransaction?: <T extends Transaction | VersionedTransaction>(tx: T) => Promise<T>;
}

/**
 * Create a browser-clean `SolanaSwarmSigner` from a `@solana/web3.js` `Keypair`.
 * Secret key bytes stay trapped inside a private closure and are redacted from logs/JSON.
 */
export function fromSolanaKeypair(keypair: Keypair): SolanaSwarmSigner {
  if (!keypair?.secretKey || keypair.secretKey.length !== 64) {
    throw new SwarmClientError({
      code: "INVALID_ARGUMENT",
      operation: "signer.fromSolanaKeypair",
      message: "Expected a valid 64-byte Solana Keypair",
    });
  }
  const secretBytes = new Uint8Array(keypair.secretKey);
  const address = keypair.publicKey.toBase58();

  return redactSigner<SolanaSwarmSigner>({
    chain: "solana",
    address,
    async signMessage(message: Uint8Array): Promise<Uint8Array> {
      return signEd25519Detached(secretBytes, message);
    },
    async signTransaction<T extends Transaction | VersionedTransaction>(tx: T): Promise<T> {
      const kp = Keypair.fromSecretKey(secretBytes);
      if (tx instanceof VersionedTransaction) {
        tx.sign([kp]);
      } else {
        (tx as Transaction).partialSign(kp);
      }
      return tx;
    },
  });
}

/**
 * Create a `SolanaSwarmSigner` from a 64-byte `Uint8Array`, base58-encoded secret key,
 * or JSON `[u8, ...]` array string (matching `solana-keygen` format).
 */
export function fromSolanaSecretKey(secret: Uint8Array | string): SolanaSwarmSigner {
  let bytes: Uint8Array;
  if (secret instanceof Uint8Array) {
    bytes = new Uint8Array(secret);
  } else if (typeof secret === "string") {
    const trimmed = secret.trim();
    if (!trimmed) {
      throw new SwarmClientError({
        code: "INVALID_ARGUMENT",
        operation: "signer.fromSolanaSecretKey",
        message: "Solana secret key string must not be empty",
      });
    }
    if (trimmed.startsWith("[")) {
      let parsed: unknown;
      try {
        parsed = JSON.parse(trimmed);
      } catch {
        throw new SwarmClientError({
          code: "INVALID_ARGUMENT",
          operation: "signer.fromSolanaSecretKey",
          message: "Invalid JSON array for Solana secret key",
        });
      }
      if (
        !Array.isArray(parsed) ||
        parsed.length !== 64 ||
        parsed.some((n) => typeof n !== "number" || !Number.isInteger(n) || n < 0 || n > 255)
      ) {
        throw new SwarmClientError({
          code: "INVALID_ARGUMENT",
          operation: "signer.fromSolanaSecretKey",
          message: "Solana JSON secret key must be an array of 64 byte integers (0..255)",
        });
      }
      bytes = Uint8Array.from(parsed);
    } else {
      try {
        bytes = bs58.decode(trimmed);
      } catch {
        throw new SwarmClientError({
          code: "INVALID_ARGUMENT",
          operation: "signer.fromSolanaSecretKey",
          message: "Invalid base58 Solana secret key",
        });
      }
    }
  } else {
    throw new SwarmClientError({
      code: "INVALID_ARGUMENT",
      operation: "signer.fromSolanaSecretKey",
      message: "Expected a 64-byte Uint8Array, base58 string, or JSON byte array string",
    });
  }

  if (bytes.length !== 64) {
    throw new SwarmClientError({
      code: "INVALID_ARGUMENT",
      operation: "signer.fromSolanaSecretKey",
      message: `Expected 64-byte Solana secret key, received ${bytes.length} bytes`,
    });
  }

  let kp: Keypair;
  try {
    kp = Keypair.fromSecretKey(bytes);
  } catch {
    throw new SwarmClientError({
      code: "INVALID_ARGUMENT",
      operation: "signer.fromSolanaSecretKey",
      message: "Invalid Solana Ed25519 secret key bytes",
    });
  }
  return fromSolanaKeypair(kp);
}

/**
 * Adapt a browser Solana Wallet Adapter (`@solana/wallet-adapter-base` compatible)
 * into a `SolanaSwarmSigner`.
 */
export function fromSolanaWalletAdapter(adapter: SolanaWalletAdapterLike): SolanaSwarmSigner {
  if (!adapter?.publicKey) {
    throw new SwarmClientError({
      code: "INVALID_ARGUMENT",
      operation: "signer.fromSolanaWalletAdapter",
      message: "Wallet adapter has no connected publicKey",
    });
  }
  const address = adapter.publicKey.toBase58();
  return redactSigner<SolanaSwarmSigner>({
    chain: "solana",
    address,
    async signMessage(message: Uint8Array): Promise<Uint8Array> {
      if (!adapter.signMessage) {
        throw new SwarmClientError({
          code: "INVALID_ARGUMENT",
          operation: "signer.signMessage",
          message: "Connected Solana wallet adapter does not support signMessage",
        });
      }
      return adapter.signMessage(message);
    },
    async signTransaction<T extends Transaction | VersionedTransaction>(tx: T): Promise<T> {
      if (!adapter.signTransaction) {
        throw new SwarmClientError({
          code: "INVALID_ARGUMENT",
          operation: "signer.signTransaction",
          message: "Connected Solana wallet adapter does not support signTransaction",
        });
      }
      return adapter.signTransaction(tx);
    },
  });
}
