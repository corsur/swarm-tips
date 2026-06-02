import * as anchor from "@coral-xyz/anchor";
import { Program, BN } from "@coral-xyz/anchor";
import { ExtensionCredit } from "../target/types/extension_credit";
import {
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  SystemProgram,
} from "@solana/web3.js";
import { assert } from "chai";

const MIN_ADVANCE_LAMPORTS = 1_000_000;
const ADVANCE = new BN(2_000_000); // 0.002 SOL fronted
const ADVANCE_SPACE = 113;

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

describe("extension-credit", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const program = anchor.workspace.extensionCredit as Program<ExtensionCredit>;
  // The provider wallet (id.json) is the backer and pays tx fees.
  const backer = provider.wallet.publicKey;

  const bal = (pk: PublicKey) =>
    provider.connection.getBalance(pk, "confirmed");

  async function airdrop(pk: PublicKey, lamports: number): Promise<void> {
    const sig = await provider.connection.requestAirdrop(pk, lamports);
    await provider.connection.confirmTransaction(sig, "confirmed");
  }

  async function feeOf(sig: string): Promise<number> {
    const tx = await provider.connection.getTransaction(sig, {
      commitment: "confirmed",
      maxSupportedTransactionVersion: 0,
    });
    return tx!.meta!.fee;
  }

  it("open → route_and_recoup: backer recouped first, recipient gets the surplus (conservation)", async () => {
    const recipient = Keypair.generate();
    const [advance] = advancePda(
      backer,
      recipient.publicKey,
      program.programId
    );
    const rentFloor =
      await provider.connection.getMinimumBalanceForRentExemption(
        ADVANCE_SPACE
      );

    await program.methods
      .openAdvance(ADVANCE)
      .accountsPartial({
        advance,
        backer,
        recipient: recipient.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc({ commitment: "confirmed" });

    // The fronted capital lands in the recipient's wallet.
    assert.equal(
      await bal(recipient.publicKey),
      ADVANCE.toNumber(),
      "recipient received the fronted capital"
    );

    // Simulate Shillbot routing earnings to the vault (payout_to = Advance PDA).
    // Earnings > advance so the backer fully recoups and the recipient gets a surplus.
    await airdrop(advance, 3_000_000);

    const vaultBefore = await bal(advance);
    const available = vaultBefore - rentFloor;
    const toBacker = Math.min(available, ADVANCE.toNumber());
    const toRecipient = available - toBacker;
    assert.isAbove(toRecipient, 0, "test setup: earnings exceed the advance");

    const backerBefore = await bal(backer);
    const recipientBefore = await bal(recipient.publicKey);

    const sig = await program.methods
      .routeAndRecoup()
      .accountsPartial({ advance, backer, recipient: recipient.publicKey })
      .rpc({ commitment: "confirmed" });
    const fee = await feeOf(sig);

    // Recipient pays no fee → receives EXACTLY the surplus.
    assert.equal(
      (await bal(recipient.publicKey)) - recipientBefore,
      toRecipient,
      "recipient receives exactly the surplus"
    );
    // Backer recouped first (net of the fee it paid as fee payer).
    assert.closeTo(
      (await bal(backer)) - backerBefore + fee,
      toBacker,
      100,
      "backer is recouped first, up to the advance"
    );
    // Vault drained back to the rent floor; all routed earnings distributed.
    assert.closeTo(await bal(advance), rentFloor, 100, "vault drained to rent");
    assert.equal(
      toBacker + toRecipient,
      available,
      "all routed earnings distributed"
    );

    // Fully recouped → close, rent returns to backer.
    await program.methods
      .closeAdvance()
      .accountsPartial({ advance, backer, recipient: recipient.publicKey })
      .rpc({ commitment: "confirmed" });
    assert.isNull(
      await provider.connection.getAccountInfo(advance),
      "advance closed after full recoupment"
    );
  });

  it("open → mark_default sweeps the vault to the backer and closes", async () => {
    const recipient = Keypair.generate();
    const [advance] = advancePda(
      backer,
      recipient.publicKey,
      program.programId
    );
    const rentFloor =
      await provider.connection.getMinimumBalanceForRentExemption(
        ADVANCE_SPACE
      );

    await program.methods
      .openAdvance(ADVANCE)
      .accountsPartial({
        advance,
        backer,
        recipient: recipient.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc({ commitment: "confirmed" });

    await airdrop(advance, 1_000_000); // partial earnings (< advance)
    const vaultBefore = await bal(advance);
    const available = vaultBefore - rentFloor;
    const backerBefore = await bal(backer);

    const sig = await program.methods
      .markDefault()
      .accountsPartial({ advance, backer, recipient: recipient.publicKey })
      .rpc({ commitment: "confirmed" });
    const fee = await feeOf(sig);

    assert.isNull(
      await provider.connection.getAccountInfo(advance),
      "advance closed on default"
    );
    // Backer swept the available earnings AND reclaimed the rent (net of fee).
    assert.closeTo(
      (await bal(backer)) - backerBefore + fee,
      available + rentFloor,
      100,
      "backer swept earnings + reclaimed rent"
    );
  });

  it("rejects an advance below the minimum", async () => {
    const recipient = Keypair.generate();
    const [advance] = advancePda(
      backer,
      recipient.publicKey,
      program.programId
    );
    try {
      await program.methods
        .openAdvance(new BN(MIN_ADVANCE_LAMPORTS - 1))
        .accountsPartial({
          advance,
          backer,
          recipient: recipient.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .rpc({ commitment: "confirmed" });
      assert.fail("expected AdvanceTooLow");
    } catch (e) {
      assert.include(`${e}`, "AdvanceTooLow");
    }
  });

  it("rejects self-advance", async () => {
    const [advance] = advancePda(backer, backer, program.programId);
    try {
      await program.methods
        .openAdvance(ADVANCE)
        .accountsPartial({
          advance,
          backer,
          recipient: backer,
          systemProgram: SystemProgram.programId,
        })
        .rpc({ commitment: "confirmed" });
      assert.fail("expected SelfAdvance");
    } catch (e) {
      assert.include(`${e}`, "SelfAdvance");
    }
  });

  it("rejects closing before fully recouped", async () => {
    const recipient = Keypair.generate();
    const [advance] = advancePda(
      backer,
      recipient.publicKey,
      program.programId
    );
    await program.methods
      .openAdvance(ADVANCE)
      .accountsPartial({
        advance,
        backer,
        recipient: recipient.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc({ commitment: "confirmed" });
    try {
      await program.methods
        .closeAdvance()
        .accountsPartial({ advance, backer, recipient: recipient.publicKey })
        .rpc({ commitment: "confirmed" });
      assert.fail("expected NotFullyRecouped");
    } catch (e) {
      assert.include(`${e}`, "NotFullyRecouped");
    }
  });
});
