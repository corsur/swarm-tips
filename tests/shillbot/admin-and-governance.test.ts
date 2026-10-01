import * as anchor from "@coral-xyz/anchor";
import { Program, BN } from "@coral-xyz/anchor";
import type { Shillbot } from "../../target/types/shillbot";
import {
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  SystemProgram,
  SYSVAR_SLOT_HASHES_PUBKEY,
} from "@solana/web3.js";
import { assert } from "chai";
import {
  MAX_SCORE,
  CHALLENGE_WINDOW_SECONDS,
  PROTOCOL_FEE_BPS,
  QUALITY_THRESHOLD,
  ESCROW_LAMPORTS,
  contentHash,
  globalStatePda,
  taskPda,
  newTaskNonce,
  challengePda,
  agentStatePda,
  sessionPda,
  airdrop,
  getClockTimestamp,
  ensureShillbotInitialized,
} from "./common.ts";

describe("shillbot - admin-and-governance", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const program = anchor.workspace.shillbot as Program<Shillbot>;

  const authority = (provider.wallet as anchor.Wallet).payer;
  let client = Keypair.generate();
  const agent = Keypair.generate();
  const challenger = Keypair.generate();
  const treasury = Keypair.generate();

  let globalPda: PublicKey;

  before(async () => {
    globalPda = await ensureShillbotInitialized(
      program,
      provider,
      authority,
      treasury
    );

    for (const kp of [agent, challenger, treasury]) {
      await airdrop(provider.connection, kp.publicKey, 5 * LAMPORTS_PER_SOL);
    }
  });

  beforeEach(async () => {
    client = Keypair.generate();
    await airdrop(provider.connection, client.publicKey, 5 * LAMPORTS_PER_SOL);
  });
  describe("update_params", () => {
    it("updates fee and threshold", async () => {
      const newFee = 500; // 5%
      const newThreshold = new BN(300_000);

      await program.methods
        .updateParams(
          newFee,
          newThreshold,
          new BN(86_400),
          new BN(1_209_600),
          new BN(604_800),
          new BN(86_400),
          5,
          2,
          5_000,
          false,
          0,
          new BN(3600),
          10,
          new BN(604_800) // dispute_resolution_window_seconds (7d)
        )
        .accountsPartial({
          globalState: globalPda,
          authority: authority.publicKey,
        })
        .rpc();

      const global = await program.account.globalState.fetch(globalPda);
      assert.equal(global.protocolFeeBps, newFee);
      assert.equal(global.qualityThreshold.toString(), newThreshold.toString());

      // Restore original values for subsequent tests
      await program.methods
        .updateParams(
          PROTOCOL_FEE_BPS,
          QUALITY_THRESHOLD,
          new BN(86_400),
          new BN(1_209_600),
          new BN(604_800),
          new BN(86_400),
          5,
          2,
          5_000,
          false,
          0,
          new BN(3600),
          10,
          new BN(604_800) // dispute_resolution_window_seconds (7d)
        )
        .accountsPartial({
          globalState: globalPda,
          authority: authority.publicKey,
        })
        .rpc();

      const restored = await program.account.globalState.fetch(globalPda);
      assert.equal(restored.protocolFeeBps, PROTOCOL_FEE_BPS);
      assert.equal(
        restored.qualityThreshold.toString(),
        QUALITY_THRESHOLD.toString()
      );
    });

    it("rejects non-authority caller", async () => {
      const impostor = Keypair.generate();
      await airdrop(provider.connection, impostor.publicKey, LAMPORTS_PER_SOL);

      try {
        await program.methods
          .updateParams(
            500,
            new BN(300_000),
            new BN(86_400),
            new BN(1_209_600),
            new BN(604_800),
            new BN(86_400),
            5,
            2,
            5_000,
            false,
            0,
            new BN(3600),
            10,
            new BN(604_800) // dispute_resolution_window_seconds (7d)
          )
          .accountsPartial({
            globalState: globalPda,
            authority: impostor.publicKey,
          })
          .signers([impostor])
          .rpc();
        assert.fail("Expected NotAuthority error");
      } catch (e: any) {
        // assert.fail above throws AssertionError — re-throw it so the
        // no-rejection path cannot satisfy this catch's substring assert.
        if (e?.name === "AssertionError") throw e;
        assert.include(e.toString(), "NotAuthority");
      }
    });

    it("rejects fee below minimum (100 bps)", async () => {
      try {
        await program.methods
          .updateParams(
            50,
            QUALITY_THRESHOLD,
            new BN(86_400),
            new BN(1_209_600),
            new BN(604_800),
            new BN(86_400),
            5,
            2,
            5_000,
            false,
            0,
            new BN(3600),
            10,
            new BN(604_800) // dispute_resolution_window_seconds (7d)
          ) // 50 bps < 100 bps minimum
          .accountsPartial({
            globalState: globalPda,
            authority: authority.publicKey,
          })
          .rpc();
        assert.fail("Expected ProtocolFeeBoundsExceeded error");
      } catch (e: any) {
        // assert.fail above throws AssertionError — re-throw it so the
        // no-rejection path cannot satisfy this catch's substring assert.
        if (e?.name === "AssertionError") throw e;
        assert.include(e.toString(), "ProtocolFeeBoundsExceeded");
      }
    });

    it("rejects fee above maximum (2500 bps)", async () => {
      try {
        await program.methods
          .updateParams(
            3000,
            QUALITY_THRESHOLD,
            new BN(86_400),
            new BN(1_209_600),
            new BN(604_800),
            new BN(86_400),
            5,
            2,
            5_000,
            false,
            0,
            new BN(3600),
            10,
            new BN(604_800) // dispute_resolution_window_seconds (7d)
          ) // 3000 bps > 2500 bps maximum
          .accountsPartial({
            globalState: globalPda,
            authority: authority.publicKey,
          })
          .rpc();
        assert.fail("Expected ProtocolFeeBoundsExceeded error");
      } catch (e: any) {
        // assert.fail above throws AssertionError — re-throw it so the
        // no-rejection path cannot satisfy this catch's substring assert.
        if (e?.name === "AssertionError") throw e;
        assert.include(e.toString(), "ProtocolFeeBoundsExceeded");
      }
    });

    it("rejects threshold above MAX_SCORE", async () => {
      try {
        await program.methods
          .updateParams(
            PROTOCOL_FEE_BPS,
            new BN(MAX_SCORE + 1),
            new BN(86_400),
            new BN(1_209_600),
            new BN(604_800),
            new BN(86_400),
            5,
            2,
            5_000,
            false,
            0,
            new BN(3600),
            10,
            new BN(604_800) // dispute_resolution_window_seconds (7d)
          )
          .accountsPartial({
            globalState: globalPda,
            authority: authority.publicKey,
          })
          .rpc();
        assert.fail("Expected QualityThresholdBoundsExceeded error");
      } catch (e: any) {
        // assert.fail above throws AssertionError — re-throw it so the
        // no-rejection path cannot satisfy this catch's substring assert.
        if (e?.name === "AssertionError") throw e;
        assert.include(e.toString(), "QualityThresholdBoundsExceeded");
      }
    });
  });

  // ---------------------------------------------------------------------------
  // Additional error path tests
  // ---------------------------------------------------------------------------

  // ---------------------------------------------------------------------------
  // Authority rotation handlers (transfer_authority / update_oracle_authority /
  // update_treasury). Each happy-path rotates and restores so the rest of the
  // suite continues to see the original (authority, oracle_authority, treasury)
  // triple — the suite's other tests assume `authority.publicKey` is the
  describe("transfer_authority", () => {
    it("rotates authority and restores it", async () => {
      const newAuthority = Keypair.generate();
      await airdrop(
        provider.connection,
        newAuthority.publicKey,
        LAMPORTS_PER_SOL
      );

      await program.methods
        .transferAuthority(newAuthority.publicKey)
        .accountsPartial({
          globalState: globalPda,
          authority: authority.publicKey,
        })
        .rpc();

      const rotated = await program.account.globalState.fetch(globalPda);
      assert.equal(
        rotated.authority.toString(),
        newAuthority.publicKey.toString()
      );

      // Restore — the new authority signs the put-back so it actually rotates.
      await program.methods
        .transferAuthority(authority.publicKey)
        .accountsPartial({
          globalState: globalPda,
          authority: newAuthority.publicKey,
        })
        .signers([newAuthority])
        .rpc();

      const restored = await program.account.globalState.fetch(globalPda);
      assert.equal(
        restored.authority.toString(),
        authority.publicKey.toString()
      );
    });

    it("rejects non-authority caller", async () => {
      const imposter = Keypair.generate();
      await airdrop(provider.connection, imposter.publicKey, LAMPORTS_PER_SOL);
      const newAuthority = Keypair.generate();

      try {
        await program.methods
          .transferAuthority(newAuthority.publicKey)
          .accountsPartial({
            globalState: globalPda,
            authority: imposter.publicKey,
          })
          .signers([imposter])
          .rpc();
        assert.fail("Expected NotAuthority");
      } catch (e: any) {
        // assert.fail above throws AssertionError — re-throw it so the
        // no-rejection path cannot satisfy this catch's substring assert.
        if (e?.name === "AssertionError") throw e;
        assert.include(e.toString(), "NotAuthority");
      }
    });

    it("rejects zero pubkey", async () => {
      try {
        await program.methods
          .transferAuthority(PublicKey.default)
          .accountsPartial({
            globalState: globalPda,
            authority: authority.publicKey,
          })
          .rpc();
        assert.fail("Expected ZeroPubkey");
      } catch (e: any) {
        // assert.fail above throws AssertionError — re-throw it so the
        // no-rejection path cannot satisfy this catch's substring assert.
        if (e?.name === "AssertionError") throw e;
        assert.include(e.toString(), "ZeroPubkey");
      }
    });
  });
  describe("update_oracle_authority", () => {
    it("rotates oracle authority and restores it", async () => {
      const newOracle = Keypair.generate();

      const before = await program.account.globalState.fetch(globalPda);
      const originalOracle = before.oracleAuthority;

      await program.methods
        .updateOracleAuthority(newOracle.publicKey)
        .accountsPartial({
          globalState: globalPda,
          authority: authority.publicKey,
        })
        .rpc();

      const rotated = await program.account.globalState.fetch(globalPda);
      assert.equal(
        rotated.oracleAuthority.toString(),
        newOracle.publicKey.toString()
      );

      await program.methods
        .updateOracleAuthority(originalOracle)
        .accountsPartial({
          globalState: globalPda,
          authority: authority.publicKey,
        })
        .rpc();

      const restored = await program.account.globalState.fetch(globalPda);
      assert.equal(
        restored.oracleAuthority.toString(),
        originalOracle.toString()
      );
    });

    it("rejects non-authority caller", async () => {
      const imposter = Keypair.generate();
      await airdrop(provider.connection, imposter.publicKey, LAMPORTS_PER_SOL);

      try {
        await program.methods
          .updateOracleAuthority(Keypair.generate().publicKey)
          .accountsPartial({
            globalState: globalPda,
            authority: imposter.publicKey,
          })
          .signers([imposter])
          .rpc();
        assert.fail("Expected NotAuthority");
      } catch (e: any) {
        // assert.fail above throws AssertionError — re-throw it so the
        // no-rejection path cannot satisfy this catch's substring assert.
        if (e?.name === "AssertionError") throw e;
        assert.include(e.toString(), "NotAuthority");
      }
    });

    it("rejects zero pubkey", async () => {
      try {
        await program.methods
          .updateOracleAuthority(PublicKey.default)
          .accountsPartial({
            globalState: globalPda,
            authority: authority.publicKey,
          })
          .rpc();
        assert.fail("Expected ZeroPubkey");
      } catch (e: any) {
        // assert.fail above throws AssertionError — re-throw it so the
        // no-rejection path cannot satisfy this catch's substring assert.
        if (e?.name === "AssertionError") throw e;
        assert.include(e.toString(), "ZeroPubkey");
      }
    });
  });
  describe("update_treasury", () => {
    it("rotates treasury and restores it", async () => {
      const newTreasury = Keypair.generate();

      const before = await program.account.globalState.fetch(globalPda);
      const originalTreasury = before.treasury;

      await program.methods
        .updateTreasury(newTreasury.publicKey)
        .accountsPartial({
          globalState: globalPda,
          authority: authority.publicKey,
        })
        .rpc();

      const rotated = await program.account.globalState.fetch(globalPda);
      assert.equal(
        rotated.treasury.toString(),
        newTreasury.publicKey.toString()
      );

      await program.methods
        .updateTreasury(originalTreasury)
        .accountsPartial({
          globalState: globalPda,
          authority: authority.publicKey,
        })
        .rpc();

      const restored = await program.account.globalState.fetch(globalPda);
      assert.equal(restored.treasury.toString(), originalTreasury.toString());
    });

    it("rejects non-authority caller", async () => {
      const imposter = Keypair.generate();
      await airdrop(provider.connection, imposter.publicKey, LAMPORTS_PER_SOL);

      try {
        await program.methods
          .updateTreasury(Keypair.generate().publicKey)
          .accountsPartial({
            globalState: globalPda,
            authority: imposter.publicKey,
          })
          .signers([imposter])
          .rpc();
        assert.fail("Expected NotAuthority");
      } catch (e: any) {
        // assert.fail above throws AssertionError — re-throw it so the
        // no-rejection path cannot satisfy this catch's substring assert.
        if (e?.name === "AssertionError") throw e;
        assert.include(e.toString(), "NotAuthority");
      }
    });

    it("rejects zero pubkey", async () => {
      try {
        await program.methods
          .updateTreasury(PublicKey.default)
          .accountsPartial({
            globalState: globalPda,
            authority: authority.publicKey,
          })
          .rpc();
        assert.fail("Expected ZeroPubkey");
      } catch (e: any) {
        // assert.fail above throws AssertionError — re-throw it so the
        // no-rejection path cannot satisfy this catch's substring assert.
        if (e?.name === "AssertionError") throw e;
        assert.include(e.toString(), "ZeroPubkey");
      }
    });
  });

  // ---------------------------------------------------------------------------
  // Session-delegated handler coverage (claim_task_session / submit_work_session).
  // The session-delegate creation/revocation paths are covered in "session
  // delegate" above; these tests exercise the actual on-chain ix the delegate
  // is authorized to call. Pre-mainnet scope — adds coverage on the privileged
});
