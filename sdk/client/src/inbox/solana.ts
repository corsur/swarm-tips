import bs58 from "bs58";
import type { NonceSigner } from "./client.js";

/**
 * Adapt a Solana wallet-adapter `signMessage` (bytes → detached ed25519
 * signature bytes) into the NonceSigner the REST session expects: the raw
 * nonce UTF-8 signed, bs58-encoded — exactly what scripts/seed-inbox.ts and
 * the MCP `agent_verify_wallet` phase-2 send.
 *
 * EVM wallets do not use this adapter: `personal_sign(nonce)` already returns
 * the hex string the server verifies, so pass it through unchanged.
 */
export function solanaNonceSigner(
  signMessage: (message: Uint8Array) => Promise<Uint8Array>
): NonceSigner {
  return async (nonce: string) =>
    bs58.encode(await signMessage(new TextEncoder().encode(nonce)));
}
