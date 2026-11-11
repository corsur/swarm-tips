import { Buffer } from "buffer";
import { Secp256k1Program } from "@solana/web3.js";
import { SwarmClientError } from "../errors.js";
import { redactSigner, type EvmCallRequest, type EvmSwarmSigner } from "./types.js";

// Compact pure-TS Keccak-256 for EIP-55 address checksumming with zero optional peer dependency at load time.
const KECCAK_ROUNDS = 24;
const RC_LOW = [
  0x00000001, 0x00008082, 0x0000808a, 0x80008000, 0x0000808b, 0x80000001,
  0x80008081, 0x00008009, 0x0000008a, 0x00000088, 0x80008009, 0x8000000a,
  0x8000808b, 0x0000008b, 0x00008089, 0x00008003, 0x00008002, 0x00000080,
  0x0000800a, 0x8000000a, 0x80008081, 0x00008080, 0x80000001, 0x80008008,
];
const RC_HIGH = [
  0x00000000, 0x00000000, 0x80000000, 0x80000000, 0x00000000, 0x00000000,
  0x80000000, 0x80000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
  0x00000000, 0x80000000, 0x80000000, 0x80000000, 0x80000000, 0x80000000,
  0x00000000, 0x80000000, 0x80000000, 0x80000000, 0x00000000, 0x80000000,
];
const ROTC = [
  1, 3, 6, 10, 15, 21, 28, 36, 45, 55, 2, 14,
  27, 41, 56, 8, 25, 43, 62, 18, 39, 61, 20, 44,
];
const PILN = [
  10, 7, 11, 17, 18, 3, 5, 16, 8, 21, 24, 4,
  15, 23, 19, 13, 12, 2, 20, 14, 22, 9, 6, 1,
];

function keccakF1600(s: Uint32Array): void {
  const bc = new Uint32Array(10);
  for (let round = 0; round < KECCAK_ROUNDS; round++) {
    for (let i = 0; i < 10; i += 2) {
      bc[i] = s[i] ^ s[i + 10] ^ s[i + 20] ^ s[i + 30] ^ s[i + 40];
      bc[i + 1] = s[i + 1] ^ s[i + 11] ^ s[i + 21] ^ s[i + 31] ^ s[i + 41];
    }
    for (let i = 0; i < 10; i += 2) {
      const prev = (i + 8) % 10;
      const next = (i + 2) % 10;
      const dLow = bc[prev] ^ ((bc[next] << 1) | (bc[next + 1] >>> 31));
      const dHigh = bc[prev + 1] ^ ((bc[next + 1] << 1) | (bc[next] >>> 31));
      for (let j = 0; j < 50; j += 10) {
        s[j + i] ^= dLow;
        s[j + i + 1] ^= dHigh;
      }
    }
    let tLow = s[2];
    let tHigh = s[3];
    for (let i = 0; i < 24; i++) {
      const j = PILN[i] * 2;
      const r = ROTC[i];
      const nLow = s[j];
      const nHigh = s[j + 1];
      if (r < 32) {
        s[j] = (tLow << r) | (tHigh >>> (32 - r));
        s[j + 1] = (tHigh << r) | (tLow >>> (32 - r));
      } else {
        const sShift = r - 32;
        s[j] = (tHigh << sShift) | (tLow >>> (32 - sShift));
        s[j + 1] = (tLow << sShift) | (tHigh >>> (32 - sShift));
      }
      tLow = nLow;
      tHigh = nHigh;
    }
    for (let j = 0; j < 50; j += 10) {
      for (let i = 0; i < 10; i++) bc[i] = s[j + i];
      for (let i = 0; i < 10; i += 2) {
        s[j + i] ^= ~bc[(i + 2) % 10] & bc[(i + 4) % 10];
        s[j + i + 1] ^= ~bc[(i + 3) % 10] & bc[(i + 5) % 10];
      }
    }
    s[0] ^= RC_LOW[round];
    s[1] ^= RC_HIGH[round];
  }
}

function keccak256Bytes(input: Uint8Array): Uint8Array {
  const rate = 136;
  const state = new Uint32Array(50);
  const block = new Uint8Array(rate);
  let offset = 0;
  while (offset + rate <= input.length) {
    for (let i = 0; i < rate; i += 4) {
      state[i >> 2] ^=
        input[offset + i] |
        (input[offset + i + 1] << 8) |
        (input[offset + i + 2] << 16) |
        (input[offset + i + 3] << 24);
    }
    keccakF1600(state);
    offset += rate;
  }
  const rem = input.length - offset;
  block.fill(0);
  block.set(input.subarray(offset), 0);
  block[rem] ^= 0x01;
  block[rate - 1] ^= 0x80;
  for (let i = 0; i < rate; i += 4) {
    state[i >> 2] ^=
      block[i] | (block[i + 1] << 8) | (block[i + 2] << 16) | (block[i + 3] << 24);
  }
  keccakF1600(state);
  const out = new Uint8Array(32);
  for (let i = 0; i < 8; i++) {
    const w = state[i];
    out[i * 4] = w & 0xff;
    out[i * 4 + 1] = (w >>> 8) & 0xff;
    out[i * 4 + 2] = (w >>> 16) & 0xff;
    out[i * 4 + 3] = (w >>> 24) & 0xff;
  }
  return out;
}

function toEip55Address(lowerHexNoPrefix: string): `0x${string}` {
  const clean = lowerHexNoPrefix.toLowerCase();
  const hash = keccak256Bytes(new TextEncoder().encode(clean));
  let out = "0x";
  for (let i = 0; i < 40; i++) {
    const nibble = (hash[i >> 1] >> (i % 2 === 0 ? 4 : 0)) & 0x0f;
    out += nibble >= 8 ? clean[i].toUpperCase() : clean[i];
  }
  return out as `0x${string}`;
}

export interface EvmPrivateKeyOptions {
  chainId?: number;
  rpcUrl?: string;
}

export interface EvmWalletClientLike {
  account?: { address: `0x${string}` } | `0x${string}`;
  chain?: { id: number };
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  signMessage: (args: any) => Promise<`0x${string}`>;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  sendTransaction?: (args: any) => Promise<`0x${string}`>;
}

/**
 * Create a browser-clean `EvmSwarmSigner` from a `0x`-prefixed 32-byte hex private key.
 * Derives the EIP-55 address and EIP-191 `personal_sign` signatures without requiring
 * optional peer `viem` at module load, and uses `viem` dynamically if `sendTransaction` is called.
 */
export function fromEvmPrivateKey(
  privateKeyHex: `0x${string}`,
  opts: EvmPrivateKeyOptions = {},
): EvmSwarmSigner {
  if (typeof privateKeyHex !== "string" || !/^0x[0-9a-fA-F]{64}$/.test(privateKeyHex)) {
    throw new SwarmClientError({
      code: "INVALID_ARGUMENT",
      operation: "signer.fromEvmPrivateKey",
      message: "Expected a 0x-prefixed 32-byte hex EVM private key",
    });
  }
  const secretBytes = Buffer.from(privateKeyHex.slice(2), "hex");
  const normalizedKey = `0x${privateKeyHex.slice(2).toLowerCase()}` as `0x${string}`;

  let rawAddressHex: string;
  try {
    const probe = Secp256k1Program.createInstructionWithPrivateKey({
      privateKey: secretBytes,
      message: new Uint8Array(0),
    });
    const addrOffset = probe.data.readUInt16LE(4);
    rawAddressHex = Buffer.from(probe.data.subarray(addrOffset, addrOffset + 20)).toString("hex");
  } catch {
    throw new SwarmClientError({
      code: "INVALID_ARGUMENT",
      operation: "signer.fromEvmPrivateKey",
      message: "Invalid secp256k1 private key",
    });
  }

  const address = toEip55Address(rawAddressHex);

  return redactSigner<EvmSwarmSigner>({
    chain: "evm",
    address,
    ...(opts.chainId !== undefined ? { chainId: opts.chainId } : {}),
    async signMessage(message: string | Uint8Array): Promise<`0x${string}`> {
      const msgBytes =
        typeof message === "string" ? new TextEncoder().encode(message) : message;
      const prefix = new TextEncoder().encode(`\x19Ethereum Signed Message:\n${msgBytes.length}`);
      const payload = new Uint8Array(prefix.length + msgBytes.length);
      payload.set(prefix, 0);
      payload.set(msgBytes, prefix.length);
      const ix = Secp256k1Program.createInstructionWithPrivateKey({
        privateKey: secretBytes,
        message: payload,
      });
      const sigOffset = ix.data.readUInt16LE(1);
      const rsHex = Buffer.from(ix.data.subarray(sigOffset, sigOffset + 64)).toString("hex");
      const vHex = (ix.data[sigOffset + 64] + 27).toString(16).padStart(2, "0");
      return `0x${rsHex}${vHex}`;
    },
    async sendTransaction(call: EvmCallRequest): Promise<`0x${string}`> {
      if (!opts.rpcUrl) {
        throw new SwarmClientError({
          code: "INVALID_ARGUMENT",
          operation: "signer.sendTransaction",
          message: "rpcUrl is required in fromEvmPrivateKey options to broadcast EVM transactions",
        });
      }
      const [{ createWalletClient, http }, { privateKeyToAccount }] = await Promise.all([
        import("viem"),
        import("viem/accounts"),
      ]);
      const account = privateKeyToAccount(normalizedKey);
      const chainId = call.chainId ?? opts.chainId ?? 84532;
      const client = createWalletClient({
        account,
        transport: http(opts.rpcUrl),
      });
      return client.sendTransaction({
        account,
        chain: {
          id: chainId,
          name: `eip155:${chainId}`,
          nativeCurrency: { name: "Ether", symbol: "ETH", decimals: 18 },
          rpcUrls: { default: { http: [opts.rpcUrl] } },
        },
        to: call.to,
        data: call.data,
        value: call.value ?? 0n,
      });
    },
  });
}

/**
 * Adapt a `viem` / `wagmi` `WalletClient` into a browser-clean `EvmSwarmSigner`.
 */
export function fromEvmWalletClient(walletClient: EvmWalletClientLike): EvmSwarmSigner {
  const rawAccount = walletClient?.account;
  const rawAddress =
    typeof rawAccount === "string"
      ? rawAccount
      : rawAccount && typeof rawAccount === "object"
      ? rawAccount.address
      : undefined;
  if (!rawAddress || !/^0x[0-9a-fA-F]{40}$/.test(rawAddress)) {
    throw new SwarmClientError({
      code: "INVALID_ARGUMENT",
      operation: "signer.fromEvmWalletClient",
      message: "WalletClient has no connected account address",
    });
  }
  const address = toEip55Address(rawAddress.slice(2));
  const chainId = walletClient.chain?.id;

  const signer: EvmSwarmSigner = {
    chain: "evm",
    address,
    ...(chainId !== undefined ? { chainId } : {}),
    async signMessage(message: string | Uint8Array): Promise<`0x${string}`> {
      return walletClient.signMessage({
        account: rawAccount ?? address,
        message: typeof message === "string" ? message : { raw: message },
      });
    },
    ...(walletClient.sendTransaction
      ? {
          async sendTransaction(call: EvmCallRequest): Promise<`0x${string}`> {
            return walletClient.sendTransaction!({
              account: rawAccount ?? address,
              to: call.to,
              data: call.data,
              value: call.value ?? 0n,
              ...(call.chainId ? { chainId: call.chainId } : {}),
            });
          },
        }
      : {}),
  };

  return redactSigner(signer);
}
