import path from "node:path";
import { BN, Program } from "@coral-xyz/anchor";
import { BankrunProvider } from "anchor-bankrun";
import { type ProgramTestContext, start } from "solana-bankrun";
import {
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  SystemProgram,
  Transaction,
} from "@solana/web3.js";
import { assert } from "chai";
import type { ExtensionCredit } from "../target/types/extension_credit";
import type { ExtensionRegistry } from "../target/types/extension_registry";
import CREDIT_IDL from "../target/idl/extension_credit.json";
import REGISTRY_IDL from "../target/idl/extension_registry.json";

const MIN_ADVANCE_LAMPORTS = 1_000_000;
const ADVANCE = new BN(2_000_000); // 0.002 SOL fronted
const ADVANCE_SPACE = 113;
const BOND = new BN(5_000_000); // 0.005 SOL backing vouch

function advancePda(
  backer: PublicKey,
  recipient: PublicKey,
  programId: PublicKey
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("advance"), backer.toBuffer(), recipient.toBuffer()],
    programId
  );
}

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

describe("extension-credit", () => {
  let context: ProgramTestContext;
  let provider: BankrunProvider;
  let program: Program<ExtensionCredit>;
  let registryProgram: Program<ExtensionRegistry>;
  let backerKp: Keypair;
  let funderPayer: PublicKey;
  let rentFloor: number;

  async function bal(pk: PublicKey): Promise<number> {
    const acct = await context.banksClient.getAccount(pk);
    return acct ? Number(acct.lamports) : 0;
  }

  async function transferFromPayer(
    pk: PublicKey,
    lamports: number
  ): Promise<void> {
    const tx = new Transaction().add(
      SystemProgram.transfer({
        fromPubkey: funderPayer,
        toPubkey: pk,
        lamports,
      })
    );
    await provider.sendAndConfirm!(tx, []);
  }

  async function vouchFor(recipient: PublicKey): Promise<PublicKey> {
    const [extension] = extensionPda(
      backerKp.publicKey,
      recipient,
      registryProgram.programId
    );
    await registryProgram.methods
      .submitExtension(0, BOND)
      .accountsPartial({
        extension,
        extender: backerKp.publicKey,
        recipient,
        systemProgram: SystemProgram.programId,
      })
      .signers([backerKp])
      .rpc();
    return extension;
  }

  before(async () => {
    process.env.SBF_OUT_DIR ??= path.resolve(process.cwd(), "target/deploy");
    const creditProgramId = new PublicKey(CREDIT_IDL.address);
    const registryProgramId = new PublicKey(REGISTRY_IDL.address);
    context = await start(
      [
        { name: "extension_credit", programId: creditProgramId },
        { name: "extension_registry", programId: registryProgramId },
      ],
      []
    );
    provider = new BankrunProvider(context);
    funderPayer = provider.wallet.publicKey;
    program = new Program<ExtensionCredit>(CREDIT_IDL as never, provider);
    registryProgram = new Program<ExtensionRegistry>(
      REGISTRY_IDL as never,
      provider
    );
    backerKp = Keypair.generate();
    await transferFromPayer(backerKp.publicKey, 20 * LAMPORTS_PER_SOL);

    const rent = await context.banksClient.getRent();
    rentFloor = Number(rent.minimumBalance(BigInt(ADVANCE_SPACE)));

    const [globalState] = PublicKey.findProgramAddressSync(
      [Buffer.from("extension_global")],
      registryProgramId
    );
    await registryProgram.methods
      .initialize(backerKp.publicKey, backerKp.publicKey)
      .accountsPartial({
        globalState,
        payer: backerKp.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([backerKp])
      .rpc();
  });

  it("open -> route_and_recoup: backer recouped first, recipient gets the surplus (conservation)", async () => {
    const recipient = Keypair.generate();
    const extension = await vouchFor(recipient.publicKey);
    const [advance] = advancePda(
      backerKp.publicKey,
      recipient.publicKey,
      program.programId
    );

    await program.methods
      .openAdvance(ADVANCE)
      .accountsPartial({
        advance,
        extension,
        backer: backerKp.publicKey,
        recipient: recipient.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([backerKp])
      .rpc();

    assert.equal(
      await bal(recipient.publicKey),
      ADVANCE.toNumber(),
      "recipient received the fronted capital"
    );

    // Simulate Shillbot routing earnings to the vault (paid by separate funderPayer so backer pays 0 fee).
    await transferFromPayer(advance, 3_000_000);

    const vaultBefore = await bal(advance);
    const available = vaultBefore - rentFloor;
    const toBacker = Math.min(available, ADVANCE.toNumber());
    const toRecipient = available - toBacker;
    assert.isAbove(toRecipient, 0, "test setup: earnings exceed the advance");

    const backerBefore = await bal(backerKp.publicKey);
    const recipientBefore = await bal(recipient.publicKey);

    // Permissionless crank paid by provider.wallet (funderPayer), so backerKp & recipient pay 0 tx fee!
    await program.methods
      .routeAndRecoup()
      .accountsPartial({
        advance,
        backer: backerKp.publicKey,
        recipient: recipient.publicKey,
      })
      .rpc();

    assert.equal(
      (await bal(recipient.publicKey)) - recipientBefore,
      toRecipient,
      "recipient receives exact surplus"
    );
    assert.equal(
      (await bal(backerKp.publicKey)) - backerBefore,
      toBacker,
      "backer recouped exact principal"
    );
    assert.equal(await bal(advance), rentFloor, "vault drained to exact rent");

    await program.methods
      .closeAdvance()
      .accountsPartial({
        advance,
        backer: backerKp.publicKey,
        recipient: recipient.publicKey,
      })
      .signers([backerKp])
      .rpc();
    assert.isNull(
      await context.banksClient.getAccount(advance),
      "advance closed after full recoupment"
    );
  });

  it("open -> mark_default sweeps the vault to the backer and closes", async () => {
    const recipient = Keypair.generate();
    const extension = await vouchFor(recipient.publicKey);
    const [advance] = advancePda(
      backerKp.publicKey,
      recipient.publicKey,
      program.programId
    );

    await program.methods
      .openAdvance(ADVANCE)
      .accountsPartial({
        advance,
        extension,
        backer: backerKp.publicKey,
        recipient: recipient.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([backerKp])
      .rpc();

    await transferFromPayer(advance, 1_000_000); // partial earnings (< advance)
    const vaultBefore = await bal(advance);
    const available = vaultBefore - rentFloor;
    const backerBefore = await bal(backerKp.publicKey);

    // provider.wallet pays fee while backerKp co-signs markDefault -> exact lamport check!
    await program.methods
      .markDefault()
      .accountsPartial({
        advance,
        backer: backerKp.publicKey,
        recipient: recipient.publicKey,
      })
      .signers([backerKp])
      .rpc();

    assert.isNull(
      await context.banksClient.getAccount(advance),
      "advance closed on default"
    );
    assert.equal(
      (await bal(backerKp.publicKey)) - backerBefore,
      available + rentFloor,
      "backer swept exact earnings + reclaimed rent"
    );
  });

  it("mark_default caps the backer at the outstanding principal; the excess goes to the recipient", async () => {
    const recipient = Keypair.generate();
    const extension = await vouchFor(recipient.publicKey);
    const [advance] = advancePda(
      backerKp.publicKey,
      recipient.publicKey,
      program.programId
    );

    await program.methods
      .openAdvance(ADVANCE)
      .accountsPartial({
        advance,
        extension,
        backer: backerKp.publicKey,
        recipient: recipient.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([backerKp])
      .rpc();

    await transferFromPayer(advance, 3_000_000);
    const available = (await bal(advance)) - rentFloor;
    const toBacker = Math.min(available, ADVANCE.toNumber());
    const toRecipient = available - toBacker;
    assert.isAbove(toRecipient, 0);

    const backerBefore = await bal(backerKp.publicKey);
    const recipientBefore = await bal(recipient.publicKey);
    await program.methods
      .markDefault()
      .accountsPartial({
        advance,
        backer: backerKp.publicKey,
        recipient: recipient.publicKey,
      })
      .signers([backerKp])
      .rpc();

    assert.isNull(await context.banksClient.getAccount(advance));
    assert.equal(
      (await bal(recipient.publicKey)) - recipientBefore,
      toRecipient,
      "recipient receives exact excess over principal"
    );
    assert.equal(
      (await bal(backerKp.publicKey)) - backerBefore,
      toBacker + rentFloor,
      "backer receives exact outstanding principal + rent"
    );
  });

  it("mark_default is rejected once the advance is fully recouped (use close_advance)", async () => {
    const recipient = Keypair.generate();
    const extension = await vouchFor(recipient.publicKey);
    const [advance] = advancePda(
      backerKp.publicKey,
      recipient.publicKey,
      program.programId
    );
    await program.methods
      .openAdvance(ADVANCE)
      .accountsPartial({
        advance,
        extension,
        backer: backerKp.publicKey,
        recipient: recipient.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([backerKp])
      .rpc();
    await transferFromPayer(advance, ADVANCE.toNumber());
    await program.methods
      .routeAndRecoup()
      .accountsPartial({
        advance,
        backer: backerKp.publicKey,
        recipient: recipient.publicKey,
      })
      .rpc();

    try {
      await program.methods
        .markDefault()
        .accountsPartial({
          advance,
          backer: backerKp.publicKey,
          recipient: recipient.publicKey,
        })
        .signers([backerKp])
        .rpc();
      assert.fail("expected AlreadyFullyRecouped");
    } catch (e) {
      if ((e as Error)?.name === "AssertionError") throw e;
      assert.include(`${e}`, "AlreadyFullyRecouped");
    }
    await program.methods
      .closeAdvance()
      .accountsPartial({
        advance,
        backer: backerKp.publicKey,
        recipient: recipient.publicKey,
      })
      .signers([backerKp])
      .rpc();
    assert.isNull(await context.banksClient.getAccount(advance));
  });

  it("rejects open_advance without an active Extension vouch or while Extension is unbonding (HIGH-16)", async () => {
    const unbacked = Keypair.generate();
    const [unbackedExt] = extensionPda(
      backerKp.publicKey,
      unbacked.publicKey,
      registryProgram.programId
    );
    const [unbackedAdv] = advancePda(
      backerKp.publicKey,
      unbacked.publicKey,
      program.programId
    );
    try {
      await program.methods
        .openAdvance(ADVANCE)
        .accountsPartial({
          advance: unbackedAdv,
          extension: unbackedExt,
          backer: backerKp.publicKey,
          recipient: unbacked.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([backerKp])
        .rpc();
      assert.fail("expected AccountNotInitialized");
    } catch (e) {
      assert.include(`${e}`, "AccountNotInitialized");
    }

    // Backed extension that entered unbonding must be rejected with ExtensionUnbonding.
    const unbondingRecipient = Keypair.generate();
    const unbondingExt = await vouchFor(unbondingRecipient.publicKey);
    await registryProgram.methods
      .requestUnbond()
      .accountsPartial({
        extension: unbondingExt,
        extender: backerKp.publicKey,
        recipient: unbondingRecipient.publicKey,
      })
      .signers([backerKp])
      .rpc();
    const [unbondingAdv] = advancePda(
      backerKp.publicKey,
      unbondingRecipient.publicKey,
      program.programId
    );
    try {
      await program.methods
        .openAdvance(ADVANCE)
        .accountsPartial({
          advance: unbondingAdv,
          extension: unbondingExt,
          backer: backerKp.publicKey,
          recipient: unbondingRecipient.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([backerKp])
        .rpc();
      assert.fail("expected ExtensionUnbonding");
    } catch (e) {
      assert.include(`${e}`, "ExtensionUnbonding");
    }
  });

  it("rejects an advance below the minimum", async () => {
    const recipient = Keypair.generate();
    const extension = await vouchFor(recipient.publicKey);
    const [advance] = advancePda(
      backerKp.publicKey,
      recipient.publicKey,
      program.programId
    );
    try {
      await program.methods
        .openAdvance(new BN(MIN_ADVANCE_LAMPORTS - 1))
        .accountsPartial({
          advance,
          extension,
          backer: backerKp.publicKey,
          recipient: recipient.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([backerKp])
        .rpc();
      assert.fail("expected AdvanceTooLow");
    } catch (e) {
      assert.include(`${e}`, "AdvanceTooLow");
    }
  });

  it("rejects closing before fully recouped", async () => {
    const recipient = Keypair.generate();
    const extension = await vouchFor(recipient.publicKey);
    const [advance] = advancePda(
      backerKp.publicKey,
      recipient.publicKey,
      program.programId
    );
    await program.methods
      .openAdvance(ADVANCE)
      .accountsPartial({
        advance,
        extension,
        backer: backerKp.publicKey,
        recipient: recipient.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([backerKp])
      .rpc();
    try {
      await program.methods
        .closeAdvance()
        .accountsPartial({
          advance,
          backer: backerKp.publicKey,
          recipient: recipient.publicKey,
        })
        .signers([backerKp])
        .rpc();
      assert.fail("expected NotFullyRecouped");
    } catch (e) {
      assert.include(`${e}`, "NotFullyRecouped");
    }
  });
});
