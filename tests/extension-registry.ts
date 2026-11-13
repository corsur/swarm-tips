import path from "node:path";
import { BN, Program } from "@coral-xyz/anchor";
import { BankrunProvider } from "anchor-bankrun";
import { Clock, type ProgramTestContext, start } from "solana-bankrun";
import {
  ComputeBudgetProgram,
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  SystemProgram,
  Transaction,
} from "@solana/web3.js";
import { assert } from "chai";
import type { ExtensionRegistry } from "../target/types/extension_registry";
import IDL from "../target/idl/extension_registry.json";

// Constants matching the on-chain program (constants.rs).
const MIN_BOND_LAMPORTS = 1_000_000;
const BOND = new BN(5_000_000); // 0.005 SOL, above the floor
const UNBOND_COOLDOWN_SECS = 7 * 86_400;
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

describe("extension-registry", () => {
  let context: ProgramTestContext;
  let provider: BankrunProvider;
  let program: Program<ExtensionRegistry>;
  let authorityKp: Keypair;
  let treasuryKp: Keypair;
  let globalState: PublicKey;

  async function bal(pk: PublicKey): Promise<number> {
    const acct = await context.banksClient.getAccount(pk);
    return acct ? Number(acct.lamports) : 0;
  }

  async function fund(pubkey: PublicKey, sol: number): Promise<void> {
    const tx = new Transaction().add(
      SystemProgram.transfer({
        fromPubkey: provider.wallet.publicKey,
        toPubkey: pubkey,
        lamports: Math.round(sol * LAMPORTS_PER_SOL),
      })
    );
    await provider.sendAndConfirm!(tx, []);
  }

  async function now(): Promise<number> {
    const c = await context.banksClient.getClock();
    return Number(c.unixTimestamp);
  }

  async function warpTo(ts: number): Promise<void> {
    const c = await context.banksClient.getClock();
    context.setClock(
      new Clock(
        c.slot + 1n,
        c.epochStartTimestamp,
        c.epoch,
        c.leaderScheduleEpoch,
        BigInt(ts)
      )
    );
  }

  before(async () => {
    process.env.SBF_OUT_DIR ??= path.resolve(process.cwd(), "target/deploy");
    const programId = new PublicKey(IDL.address);
    context = await start([{ name: "extension_registry", programId }], []);
    provider = new BankrunProvider(context);
    program = new Program<ExtensionRegistry>(IDL as never, provider);
    authorityKp = Keypair.generate();
    treasuryKp = Keypair.generate();
    await fund(authorityKp.publicKey, 10);
    await fund(treasuryKp.publicKey, 1);

    [globalState] = PublicKey.findProgramAddressSync(
      [Buffer.from("extension_global")],
      program.programId
    );
    await program.methods
      .initialize(authorityKp.publicKey, treasuryKp.publicKey)
      .accountsPartial({
        globalState,
        payer: authorityKp.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([authorityKp])
      .rpc();
  });

  it("submit -> request_unbond -> 7d cooldown -> withdraw returns full bond + rent (and rejects premature withdraw)", async () => {
    const extender = Keypair.generate();
    const recipient = Keypair.generate();
    await fund(extender.publicKey, 1);
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
      .rpc();

    const pdaBal = await bal(extension);
    assert.isAbove(pdaBal, BOND.toNumber(), "PDA holds bond + rent");

    // Premature withdraw before request_unbond must fail with UnbondNotRequested.
    try {
      await program.methods
        .withdrawExtension()
        .preInstructions([
          ComputeBudgetProgram.setComputeUnitLimit({ units: 200_001 }),
        ])
        .accountsPartial({
          extension,
          extender: extender.publicKey,
          recipient: recipient.publicKey,
        })
        .signers([extender])
        .rpc();
      assert.fail("expected UnbondNotRequested");
    } catch (e) {
      assert.include(`${e}`, "UnbondNotRequested");
    }

    await program.methods
      .requestUnbond()
      .accountsPartial({
        extension,
        extender: extender.publicKey,
        recipient: recipient.publicKey,
      })
      .signers([extender])
      .rpc();

    const tReq = await now();
    // 1 second before cooldown expires must fail with UnbondCooldownActive.
    await warpTo(tReq + UNBOND_COOLDOWN_SECS - 1);
    try {
      await program.methods
        .withdrawExtension()
        .preInstructions([
          ComputeBudgetProgram.setComputeUnitLimit({ units: 200_002 }),
        ])
        .accountsPartial({
          extension,
          extender: extender.publicKey,
          recipient: recipient.publicKey,
        })
        .signers([extender])
        .rpc();
      assert.fail("expected UnbondCooldownActive");
    } catch (e) {
      assert.include(`${e}`, "UnbondCooldownActive");
    }

    // At exact cooldown boundary, withdraw succeeds and returns bond + rent.
    await warpTo(tReq + UNBOND_COOLDOWN_SECS);
    await program.methods
      .withdrawExtension()
      .preInstructions([
        ComputeBudgetProgram.setComputeUnitLimit({ units: 200_003 }),
      ])
      .accountsPartial({
        extension,
        extender: extender.publicKey,
        recipient: recipient.publicKey,
      })
      .signers([extender])
      .rpc();

    const closed = await context.banksClient.getAccount(extension);
    assert.isNull(closed, "extension account is closed after withdraw");

    const after = await bal(extender.publicKey);
    const netLoss = before - after;
    assert.isAtLeast(netLoss, 0, "extender cannot gain lamports");
    assert.isBelow(
      netLoss,
      1_000_000,
      "extender net loss is only tx fees — full bond + rent returned"
    );
  });

  it("submit -> (even during unbonding) default slashes bond to treasury, returns rent to extender (conservation)", async () => {
    const extender = Keypair.generate();
    const recipient = Keypair.generate();
    await fund(extender.publicKey, 1);
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
      .rpc();

    // Extender tries to escape slashing by requesting unbond; default_extension still slashes!
    await program.methods
      .requestUnbond()
      .accountsPartial({
        extension,
        extender: extender.publicKey,
        recipient: recipient.publicKey,
      })
      .signers([extender])
      .rpc();

    const pdaBefore = await bal(extension);
    const treasuryBefore = await bal(treasuryKp.publicKey);
    const extenderBefore = await bal(extender.publicKey);

    await program.methods
      .defaultExtension()
      .accountsPartial({
        extension,
        globalState,
        authority: authorityKp.publicKey,
        treasury: treasuryKp.publicKey,
        extender: extender.publicKey,
        recipient: recipient.publicKey,
      })
      .signers([authorityKp])
      .rpc();

    const closed = await context.banksClient.getAccount(extension);
    assert.isNull(closed, "extension account is closed after default");

    const treasuryDelta = (await bal(treasuryKp.publicKey)) - treasuryBefore;
    const extenderDelta = (await bal(extender.publicKey)) - extenderBefore;
    const rent = pdaBefore - BOND.toNumber();

    assert.equal(extenderDelta, rent, "extender receives exactly the rent");
    assert.equal(
      treasuryDelta,
      BOND.toNumber(),
      "treasury receives exact slashed bond"
    );
    assert.equal(
      treasuryDelta + extenderDelta,
      pdaBefore,
      "PDA lamports 100% conserved"
    );
  });

  it("rejects an unsupported extension type", async () => {
    const extender = Keypair.generate();
    const recipient = Keypair.generate();
    await fund(extender.publicKey, 1);
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
        .rpc();
      assert.fail("expected UnsupportedExtensionType");
    } catch (e) {
      assert.include(`${e}`, "UnsupportedExtensionType");
    }
  });

  it("rejects a bond below the minimum", async () => {
    const extender = Keypair.generate();
    const recipient = Keypair.generate();
    await fund(extender.publicKey, 1);
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
        .rpc();
      assert.fail("expected BondTooLow");
    } catch (e) {
      assert.include(`${e}`, "BondTooLow");
    }
  });

  it("rejects self-extension", async () => {
    const extender = Keypair.generate();
    await fund(extender.publicKey, 1);
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
        .rpc();
      assert.fail("expected SelfExtension");
    } catch (e) {
      assert.include(`${e}`, "SelfExtension");
    }
  });

  it("rejects default from a non-authority signer", async () => {
    const extender = Keypair.generate();
    const recipient = Keypair.generate();
    const intruder = Keypair.generate();
    await fund(extender.publicKey, 1);
    await fund(intruder.publicKey, 1);
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
      .rpc();

    try {
      await program.methods
        .defaultExtension()
        .accountsPartial({
          extension,
          globalState,
          authority: intruder.publicKey,
          treasury: treasuryKp.publicKey,
          extender: extender.publicKey,
          recipient: recipient.publicKey,
        })
        .signers([intruder])
        .rpc();
      assert.fail("expected NotAuthority");
    } catch (e) {
      assert.include(`${e}`, "NotAuthority");
    }
  });
});
