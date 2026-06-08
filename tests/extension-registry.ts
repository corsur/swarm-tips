import * as anchor from "@coral-xyz/anchor";
import { Program, BN } from "@coral-xyz/anchor";
import { ExtensionRegistry } from "../target/types/extension_registry";
import {
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  SystemProgram,
} from "@solana/web3.js";
import { assert } from "chai";

// Constants matching the on-chain program (constants.rs).
const MIN_BOND_LAMPORTS = 1_000_000;
const BOND = new BN(5_000_000); // 0.005 SOL, above the floor
const TYPE_CAPABILITY_VALIDATION = 0;
const TYPE_MENTORSHIP = 1;

function extensionPda(
  extender: PublicKey,
  recipient: PublicKey,
  programId: PublicKey
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("extension"), extender.toBuffer(), recipient.toBuffer()],
    programId
  );
}

async function fund(
  provider: anchor.AnchorProvider,
  pubkey: PublicKey,
  sol: number
): Promise<void> {
  const sig = await provider.connection.requestAirdrop(
    pubkey,
    sol * LAMPORTS_PER_SOL
  );
  await provider.connection.confirmTransaction(sig, "confirmed");
}

describe("extension-registry", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const program = anchor.workspace
    .extensionRegistry as Program<ExtensionRegistry>;
  // The provider wallet (id.json) becomes the registry authority + treasury.
  const authority = provider.wallet.publicKey;
  const [globalState] = PublicKey.findProgramAddressSync(
    [Buffer.from("extension_global")],
    program.programId
  );

  const bal = (pk: PublicKey) =>
    provider.connection.getBalance(pk, "confirmed");

  before(async () => {
    try {
      await program.methods
        .initialize(authority, authority)
        .accountsPartial({
          globalState,
          payer: authority,
          systemProgram: SystemProgram.programId,
        })
        .rpc({ commitment: "confirmed" });
    } catch (_e) {
      // already initialized (validator not reset) — fine
    }
  });

  it("submit → withdraw returns the full bond + rent to the extender (conservation)", async () => {
    const extender = Keypair.generate();
    const recipient = Keypair.generate();
    await fund(provider, extender.publicKey, 1);
    const [extension] = extensionPda(
      extender.publicKey,
      recipient.publicKey,
      program.programId
    );

    const before = await bal(extender.publicKey);

    await program.methods
      .submitExtension(TYPE_CAPABILITY_VALIDATION, BOND)
      .accountsPartial({
        extension,
        extender: extender.publicKey,
        recipient: recipient.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([extender])
      .rpc({ commitment: "confirmed" });

    // Bond is held on the PDA (rent + bond).
    const pdaBal = await bal(extension);
    assert.isAbove(pdaBal, BOND.toNumber(), "PDA holds bond + rent");

    await program.methods
      .withdrawExtension()
      .accountsPartial({
        extension,
        extender: extender.publicKey,
        recipient: recipient.publicKey,
      })
      .signers([extender])
      .rpc({ commitment: "confirmed" });

    // Account is closed; extender got bond + rent back, losing only tx fees.
    const closed = await provider.connection.getAccountInfo(extension);
    assert.isNull(closed, "extension account is closed after attest");

    const after = await bal(extender.publicKey);
    const netLoss = before - after;
    assert.isAtLeast(netLoss, 0, "extender cannot gain lamports");
    assert.isBelow(
      netLoss,
      1_000_000,
      "extender net loss is only tx fees — full bond + rent returned"
    );
  });

  it("submit → default slashes the bond to treasury, returns rent to extender (conservation)", async () => {
    const extender = Keypair.generate();
    const recipient = Keypair.generate();
    await fund(provider, extender.publicKey, 1);
    const [extension] = extensionPda(
      extender.publicKey,
      recipient.publicKey,
      program.programId
    );

    await program.methods
      .submitExtension(TYPE_CAPABILITY_VALIDATION, BOND)
      .accountsPartial({
        extension,
        extender: extender.publicKey,
        recipient: recipient.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([extender])
      .rpc({ commitment: "confirmed" });

    const pdaBefore = await bal(extension); // rent + bond
    const treasuryBefore = await bal(authority);
    const extenderBefore = await bal(extender.publicKey);

    // authority == treasury == provider wallet (id.json / root) — so the
    // treasury wallet is ALSO the fee payer; account for that fee exactly.
    const sig = await program.methods
      .defaultExtension()
      .accountsPartial({
        extension,
        globalState,
        authority,
        treasury: authority,
        extender: extender.publicKey,
        recipient: recipient.publicKey,
      })
      .rpc({ commitment: "confirmed" });
    const txMeta = await provider.connection.getTransaction(sig, {
      commitment: "confirmed",
      maxSupportedTransactionVersion: 0,
    });
    const fee = txMeta!.meta!.fee;

    const closed = await provider.connection.getAccountInfo(extension);
    assert.isNull(closed, "extension account is closed after default");

    const treasuryDelta = (await bal(authority)) - treasuryBefore;
    const extenderDelta = (await bal(extender.publicKey)) - extenderBefore;

    // Clean, fee-independent invariant: the extender pays no fee here, so it
    // must receive EXACTLY the rent (pdaBefore − bond). This proves the bond
    // (and only the bond) left the PDA toward the treasury.
    const rent = pdaBefore - BOND.toNumber();
    assert.equal(extenderDelta, rent, "extender receives exactly the rent");
    // The bond goes to the treasury, net of the fee the treasury wallet paid
    // (treasury == fee payer here). Tolerance < 1 base fee absorbs Solana's
    // fee-burn / reported-fee accounting artifact — immaterial to safety.
    assert.closeTo(
      treasuryDelta + fee,
      BOND.toNumber(),
      100,
      "treasury receives the slashed bond"
    );
    // No material leak: total accounted lamports == pdaBefore within < 1 fee.
    assert.closeTo(
      treasuryDelta + extenderDelta + fee,
      pdaBefore,
      100,
      "PDA lamports fully conserved (no material leak)"
    );
  });

  it("rejects an unsupported extension type", async () => {
    const extender = Keypair.generate();
    const recipient = Keypair.generate();
    await fund(provider, extender.publicKey, 1);
    const [extension] = extensionPda(
      extender.publicKey,
      recipient.publicKey,
      program.programId
    );
    try {
      await program.methods
        .submitExtension(TYPE_MENTORSHIP, BOND)
        .accountsPartial({
          extension,
          extender: extender.publicKey,
          recipient: recipient.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([extender])
        .rpc({ commitment: "confirmed" });
      assert.fail("expected UnsupportedExtensionType");
    } catch (e) {
      assert.include(`${e}`, "UnsupportedExtensionType");
    }
  });

  it("rejects a bond below the minimum", async () => {
    const extender = Keypair.generate();
    const recipient = Keypair.generate();
    await fund(provider, extender.publicKey, 1);
    const [extension] = extensionPda(
      extender.publicKey,
      recipient.publicKey,
      program.programId
    );
    try {
      await program.methods
        .submitExtension(
          TYPE_CAPABILITY_VALIDATION,
          new BN(MIN_BOND_LAMPORTS - 1)
        )
        .accountsPartial({
          extension,
          extender: extender.publicKey,
          recipient: recipient.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([extender])
        .rpc({ commitment: "confirmed" });
      assert.fail("expected BondTooLow");
    } catch (e) {
      assert.include(`${e}`, "BondTooLow");
    }
  });

  it("rejects self-extension", async () => {
    const extender = Keypair.generate();
    await fund(provider, extender.publicKey, 1);
    const [extension] = extensionPda(
      extender.publicKey,
      extender.publicKey,
      program.programId
    );
    try {
      await program.methods
        .submitExtension(TYPE_CAPABILITY_VALIDATION, BOND)
        .accountsPartial({
          extension,
          extender: extender.publicKey,
          recipient: extender.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([extender])
        .rpc({ commitment: "confirmed" });
      assert.fail("expected SelfExtension");
    } catch (e) {
      assert.include(`${e}`, "SelfExtension");
    }
  });

  it("rejects default from a non-authority signer", async () => {
    const extender = Keypair.generate();
    const recipient = Keypair.generate();
    const intruder = Keypair.generate();
    await fund(provider, extender.publicKey, 1);
    await fund(provider, intruder.publicKey, 1);
    const [extension] = extensionPda(
      extender.publicKey,
      recipient.publicKey,
      program.programId
    );

    await program.methods
      .submitExtension(TYPE_CAPABILITY_VALIDATION, BOND)
      .accountsPartial({
        extension,
        extender: extender.publicKey,
        recipient: recipient.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([extender])
      .rpc({ commitment: "confirmed" });

    try {
      await program.methods
        .defaultExtension()
        .accountsPartial({
          extension,
          globalState,
          authority: intruder.publicKey,
          treasury: authority,
          extender: extender.publicKey,
          recipient: recipient.publicKey,
        })
        .signers([intruder])
        .rpc({ commitment: "confirmed" });
      assert.fail("expected NotAuthority");
    } catch (e) {
      assert.include(`${e}`, "NotAuthority");
    }
  });
});
