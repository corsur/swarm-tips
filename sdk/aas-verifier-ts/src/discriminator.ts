import { sha256 } from "js-sha256";

/**
 * Anchor account discriminator: sha256("account:" + name)[0..8].
 *
 * Used by step 4 of the verification protocol — the first 8 bytes of
 * the on-chain account data MUST equal this for the named
 * `account_kind` under the named `program_id`. Anchor accounts derive
 * their discriminator deterministically from the struct name; the
 * verifier reproduces it without needing the protocol's IDL.
 */
export function anchorDiscriminator(accountKind: string): Uint8Array {
  const preimage = `account:${accountKind}`;
  const hash = sha256.array(preimage);
  return new Uint8Array(hash.slice(0, 8));
}

/** Constant-time byte equality. */
export function bytesEqual(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) {
    diff |= a[i] ^ b[i];
  }
  return diff === 0;
}
