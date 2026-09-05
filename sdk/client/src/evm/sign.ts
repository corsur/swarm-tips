import { sign } from "viem/accounts";
import type { Hex } from "viem";

/**
 * Sign a `0x` 32-byte digest with an EVM private key, returning a `0x` 65-byte
 * `[r||s||v]` secp256k1 signature with `v = 0 | 1`. viem's `sign` emits the EVM
 * `ecrecover` convention `v = 27 | 28`; this normalizes it to the k256
 * `recover_address` form the game-api relay (and the Solana program) verify.
 *
 * NOTE: this is the OFF-CHAIN relay direction (27/28 → 0/1). For a signature an
 * EVM CONTRACT verifies, do NOT normalize — viem's 27/28 is already correct
 * (the inverse axis; see chain-core `sign_digest_eth` / `evm_operator_sig_v_convention`).
 */
export async function signDigestForRelay(
  privateKey: Hex,
  digestHex: Hex
): Promise<Hex> {
  const serialized = await sign({ hash: digestHex, privateKey, to: "hex" });
  const body = serialized.slice(2); // 130 hex chars: r(64) || s(64) || v(2)
  if (body.length !== 130) {
    throw new Error(`unexpected signature length: ${serialized}`);
  }
  const rs = body.slice(0, 128);
  let v = parseInt(body.slice(128, 130), 16);
  if (v >= 27) v -= 27; // 27|28 -> 0|1 for the relay's k256 recover_address
  return `0x${rs}${v.toString(16).padStart(2, "0")}`;
}
