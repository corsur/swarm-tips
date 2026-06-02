/**
 * extension-registry `Extension` decoder tests.
 *
 * The primary fixture is a REAL devnet Extension account (the permanent dogfood
 * extension root CKsZ… -> agent B9H6…, PDA 4gAYnHAChB6PMroRa6Rv7C2MTkf86R2f6qqe6kbWwvSZ),
 * captured verbatim as base64 — so this decodes a genuine on-chain record,
 * network-free.
 */
import { describe, expect, it } from "vitest";
import {
  anchorDiscriminator,
  decodeExtension,
  resolveExtensionType,
  EXTENSION_ACCOUNT_KIND,
  EXTENSION_MIN_BODY_LEN,
} from "../src/index.js";

// Real devnet account data (full 106 bytes incl. the 8-byte discriminator).
const REAL_DEVNET_EXTENSION_B64 =
  "jbd+KhftCwWoRiwGfwms/EriH89VTKrVNRnA02kf0yLikuVftJPCQ5azt3iMmNnp+2hGXMiDkChIC/TXJfvXpZ6D8YT57eObAEBLTAAAAAAAyR4fagAAAAD/AAAAAAAAAAAAAAAAAAAAAA==";

function realAccount(): Uint8Array {
  return new Uint8Array(Buffer.from(REAL_DEVNET_EXTENSION_B64, "base64"));
}

describe("decodeExtension (real devnet record)", () => {
  it("decodes the on-chain Extension body to the exact fields", () => {
    const account = realAccount();
    expect(account.length).toBe(106); // Extension::SPACE

    // Step 4 parity: the account's first 8 bytes are the Anchor discriminator.
    expect(account.slice(0, 8)).toEqual(
      anchorDiscriminator(EXTENSION_ACCOUNT_KIND)
    );

    const ext = decodeExtension(account.slice(8), EXTENSION_ACCOUNT_KIND);
    expect(ext.extender).toBe("CKsZ7ZMLLUzbHUeu2Vm5mjuB8QQi3vfvqvXFdFxT7xmY");
    expect(ext.recipient).toBe("B9H6dLnZNrYa6Gkho8TsKo7nRKgpuK7SSYBbHNC4qiY2");
    expect(ext.extension_type).toBe(0);
    expect(resolveExtensionType(ext.extension_type)).toBe(
      "capability_validation"
    );
    expect(ext.bond_lamports).toBe(5_000_000n);
    expect(ext.created_at).toBe(1_780_424_393n);
    expect(ext.bump).toBe(255);
  });
});

describe("decodeExtension (guards)", () => {
  it("rejects a non-Extension account_kind", () => {
    expect(() =>
      decodeExtension(new Uint8Array(EXTENSION_MIN_BODY_LEN), "Task")
    ).toThrow(/only handles account_kind="Extension"/);
  });

  it("rejects a body shorter than the decoded fields", () => {
    expect(() =>
      decodeExtension(
        new Uint8Array(EXTENSION_MIN_BODY_LEN - 1),
        EXTENSION_ACCOUNT_KIND
      )
    ).toThrow(/too short/);
  });
});

describe("resolveExtensionType", () => {
  it("maps the full taxonomy and rejects unknown discriminants", () => {
    expect(resolveExtensionType(0)).toBe("capability_validation");
    expect(resolveExtensionType(1)).toBe("mentorship");
    expect(resolveExtensionType(2)).toBe("sponsorship");
    expect(resolveExtensionType(3)).toBe("validation");
    expect(resolveExtensionType(4)).toBeNull();
  });
});
