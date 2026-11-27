import {
  ComputeBudgetProgram,
  Keypair,
  PublicKey,
  Secp256k1Program,
  Transaction,
  TransactionInstruction,
} from "@solana/web3.js";
import { readFileSync } from "node:fs";
import { sha256 } from "js-sha256";
import { describe, expect, it } from "vitest";
import { BN } from "../contracts/index.js";
import {
  EXTENSION_CREDIT_PROGRAM_ID,
  SHILLBOT_PROGRAM_ID,
  advancePda,
  agentStatePda,
  buildExpire,
  buildFinalize,
  buildShillbotInstruction,
  buildTransaction,
  globalStatePda,
  inspectTransaction,
  verifyBuiltTransaction,
  verifyIntent,
  type BuildRequest,
} from "./index.js";
import { buildSecp256k1Instruction } from "./switchboard.js";

const wallet = Keypair.fromSeed(
  Uint8Array.from({ length: 32 }, (_, i) => i + 1)
).publicKey;
const sponsor = Keypair.fromSeed(
  Uint8Array.from({ length: 32 }, (_, i) => 32 - i)
).publicKey;
const task = new PublicKey(Uint8Array.from({ length: 32 }, () => 7));
const feed = new PublicKey(Uint8Array.from({ length: 32 }, () => 8));
const recentBlockhash = PublicKey.default.toBase58();
const golden = JSON.parse(
  readFileSync(
    new URL("../../test/transaction-vectors.json", import.meta.url),
    "utf8"
  )
) as {
  vectors: Array<{ action: string; accounts: string[]; data_base64: string }>;
  sponsored: Array<{ action: string; fee_payer: string; signers: string[] }>;
};

function base(action: BuildRequest["action"]) {
  return {
    action,
    wallet: wallet.toBase58(),
    network: "devnet" as const,
    recentBlockhash,
  };
}

function fixtures(): BuildRequest[] {
  return [
    {
      ...base("create"),
      action: "create",
      nonce: "42",
      escrowLamports: "1000000",
      contentHash: "11".repeat(32),
      deadline: "1900000000",
      submitMargin: "14400",
      claimBuffer: "14400",
      platform: 0,
      attestationDelayOverride: 0,
      challengeWindowOverride: 0,
      verificationTimeoutOverride: 0,
      requiresApproval: true,
      verificationKind: 0,
    },
    { ...base("claim"), action: "claim", taskPda: task.toBase58() },
    {
      ...base("submit"),
      action: "submit",
      taskPda: task.toBase58(),
      contentId: "video-123",
    },
    { ...base("approve"), action: "approve", taskPda: task.toBase58() },
    {
      ...base("verify"),
      action: "verify",
      taskPda: task.toBase58(),
      switchboardFeed: feed.toBase58(),
      compositeScore: "900000",
      verificationHash: "22".repeat(32),
      crankInstructions: [
        ComputeBudgetProgram.setComputeUnitLimit({ units: 400_000 }),
      ],
    },
    {
      ...base("finalize"),
      action: "finalize",
      taskPda: task.toBase58(),
      agent: wallet.toBase58(),
      client: sponsor.toBase58(),
      treasury: feed.toBase58(),
    },
  ];
}

describe("transaction construction", () => {
  it("matches Switchboard's secp256k1 instruction encoding", () => {
    const instruction = buildSecp256k1Instruction(
      [
        {
          ethAddress: Buffer.from("11".repeat(20), "hex"),
          signature: Buffer.from("22".repeat(64), "hex"),
          message: Buffer.from("hello"),
          recoveryId: 1,
          oracleIdx: 1,
        },
        {
          ethAddress: Buffer.from("33".repeat(20), "hex"),
          signature: Buffer.from("44".repeat(64), "hex"),
          message: Buffer.from("hello"),
          recoveryId: 0,
          oracleIdx: 0,
        },
      ],
      2
    );
    expect(instruction.data.toString("hex")).toBe(
      "02170002580002c1000500026c0002ad0002c100050002" +
        "44".repeat(64) +
        "00" +
        "33".repeat(20) +
        "22".repeat(64) +
        "01" +
        "11".repeat(20) +
        Buffer.from("hello").toString("hex")
    );
  });

  it("rejects malformed Switchboard secp256k1 inputs", () => {
    expect(() => buildSecp256k1Instruction([], 2)).toThrow(
      "no secp256k1 signatures"
    );
    expect(() =>
      buildSecp256k1Instruction(
        [
          {
            ethAddress: Buffer.alloc(20),
            signature: Buffer.alloc(64),
            message: Buffer.from("one"),
            recoveryId: 0,
            oracleIdx: 0,
          },
          {
            ethAddress: Buffer.alloc(20),
            signature: Buffer.alloc(64),
            message: Buffer.from("two"),
            recoveryId: 0,
            oracleIdx: 1,
          },
        ],
        2
      )
    ).toThrow("must share one message");
  });

  it("matches the Rust instruction golden vectors for every action", () => {
    for (const request of fixtures()) {
      const vector = golden.vectors.find(
        (candidate) => candidate.action === request.action
      )!;
      const instruction = buildTransaction(request).transaction_intent;
      const decoded = inspectTransaction(buildTransaction(request).unsigned_tx);
      const lifecycle = decoded.instructions.find(
        (ix) => ix.program_id === SHILLBOT_PROGRAM_ID.toBase58()
      )!;
      expect(instruction.accounts).toEqual(vector.accounts);
      expect(lifecycle.data_base64).toBe(vector.data_base64);
    }
  });

  it.each(
    fixtures().flatMap((request) => [
      { request, version: "legacy" as const },
      { request, version: "v0" as const },
    ])
  )("builds and inspects $request.action $version", ({ request, version }) => {
    const built = buildTransaction({ ...request, version } as BuildRequest);
    const inspection = inspectTransaction(built.unsigned_tx);
    expect(inspection.version).toBe(version);
    expect(inspection.fee_payer).toBe(wallet.toBase58());
    expect(
      inspection.instructions.some(
        (ix) => ix.program_id === SHILLBOT_PROGRAM_ID.toBase58()
      )
    ).toBe(true);
    expect(verifyIntent(built.unsigned_tx, built.transaction_intent)).toEqual(
      inspection
    );
  });

  it("constructs sponsored claim and submit messages without any private key", () => {
    for (const request of fixtures().filter(
      (candidate) =>
        candidate.action === "claim" || candidate.action === "submit"
    )) {
      const built = buildTransaction({
        ...request,
        sponsor: sponsor.toBase58(),
      } as BuildRequest);
      const inspection = inspectTransaction(built.unsigned_tx);
      expect(inspection.fee_payer).toBe(sponsor.toBase58());
      expect(inspection.signers).toEqual([
        sponsor.toBase58(),
        wallet.toBase58(),
      ]);
      const vector = golden.sponsored.find(
        (candidate) => candidate.action === request.action
      )!;
      expect(inspection.fee_payer).toBe(vector.fee_payer);
      expect(inspection.signers).toEqual(vector.signers);
    }
  });

  it("binds an optional sponsored-claim payout route to the task and agent", () => {
    const built = buildTransaction({
      ...fixtures()[1],
      action: "claim",
      sponsor: sponsor.toBase58(),
      payoutTo: feed.toBase58(),
    } as BuildRequest);
    const inspection = verifyIntent(
      built.unsigned_tx,
      built.transaction_intent
    );
    const shillbotInstructions = inspection.instructions.filter(
      (ix) => ix.program_id === SHILLBOT_PROGRAM_ID.toBase58()
    );
    expect(shillbotInstructions).toHaveLength(2);
    expect(shillbotInstructions[0].data_base64).toBe(
      golden.vectors.find((candidate) => candidate.action === "claim")!
        .data_base64
    );
    expect(shillbotInstructions[1].accounts).toEqual([
      task.toBase58(),
      wallet.toBase58(),
    ]);
  });

  it("rejects a payout route placed before claim establishes the agent", () => {
    const built = buildTransaction({
      ...fixtures()[1],
      action: "claim",
      sponsor: sponsor.toBase58(),
      payoutTo: feed.toBase58(),
    } as BuildRequest);
    const tx = Transaction.from(Buffer.from(built.unsigned_tx, "base64"));
    tx.instructions.reverse();
    const reordered = tx
      .serialize({ requireAllSignatures: false, verifySignatures: false })
      .toString("base64");
    expect(() => verifyIntent(reordered, built.transaction_intent)).toThrow(
      "payout route must follow claim"
    );
  });

  it("detects intent tampering", () => {
    const built = buildTransaction(fixtures()[1]);
    const altered = {
      ...built.transaction_intent,
      accounts: [
        PublicKey.default.toBase58(),
        ...built.transaction_intent.accounts.slice(1),
      ],
    };
    expect(() => verifyIntent(built.unsigned_tx, altered)).toThrow(
      "intent digest mismatch"
    );
  });

  it("does not accept arbitrary companion instructions as a second lifecycle", () => {
    const request = fixtures()[1];
    const extra = new TransactionInstruction({
      programId: SHILLBOT_PROGRAM_ID,
      keys: [],
      data: Buffer.alloc(8),
    });
    const verifyRequest = {
      ...base("verify"),
      action: "verify" as const,
      taskPda: task.toBase58(),
      switchboardFeed: feed.toBase58(),
      compositeScore: "1",
      verificationHash: "00".repeat(32),
      crankInstructions: [extra],
    };
    const built = buildTransaction(verifyRequest);
    expect(() =>
      verifyIntent(built.unsigned_tx, built.transaction_intent)
    ).toThrow("unexpected Shillbot companion");
    expect(request.action).toBe("claim");
  });

  it("builds and validates client-paid expire_task (Open/Submitted/Approved 3 accounts vs Claimed 4 accounts)", () => {
    const clientKp = Keypair.generate();
    const authorityPk = Keypair.generate().publicKey;
    const oraclePk = Keypair.generate().publicKey;
    const claimedAgent = Keypair.generate().publicKey;
    const expOpen = buildExpire({
      wallet: clientKp.publicKey.toBase58(),
      network: "mainnet",
      recentBlockhash,
      taskPda: task.toBase58(),
    });
    const inspOpen = verifyBuiltTransaction(expOpen.unsigned_tx, {
      action: "expire",
      network: "mainnet",
      wallet: clientKp.publicKey.toBase58(),
      taskPda: task.toBase58(),
      expireTaskState: "open",
    });
    expect(inspOpen.fee_payer).toBe(clientKp.publicKey.toBase58());
    expect(inspOpen.fee_payer).not.toBe(authorityPk.toBase58());
    expect(inspOpen.fee_payer).not.toBe(oraclePk.toBase58());
    expect(inspOpen.signers).toEqual([clientKp.publicKey.toBase58()]);
    expect(inspOpen.instructions).toHaveLength(1);
    expect(inspOpen.instructions[0].accounts).toEqual([
      task.toBase58(),
      globalStatePda().toBase58(),
      clientKp.publicKey.toBase58(),
    ]);

    const expClaimed = buildExpire({
      wallet: clientKp.publicKey.toBase58(),
      network: "mainnet",
      recentBlockhash,
      taskPda: task.toBase58(),
      claimedAgent: claimedAgent.toBase58(),
    });
    const inspClaimed = verifyBuiltTransaction(expClaimed.unsigned_tx, {
      action: "expire",
      network: "mainnet",
      wallet: clientKp.publicKey.toBase58(),
      taskPda: task.toBase58(),
      expireTaskState: "claimed",
    });
    expect(inspClaimed.instructions[0].accounts).toEqual([
      task.toBase58(),
      globalStatePda().toBase58(),
      clientKp.publicKey.toBase58(),
      agentStatePda(claimedAgent).toBase58(),
    ]);

    // Claimed task MUST reject 3-account expire_task
    expect(() =>
      verifyBuiltTransaction(expOpen.unsigned_tx, {
        action: "expire",
        network: "mainnet",
        wallet: clientKp.publicKey.toBase58(),
        taskPda: task.toBase58(),
        expireTaskState: "claimed",
      })
    ).toThrow(/claimed task requires exact 4 accounts/);

    // P1-1: Submitted / Approved tasks past `submitted_at + verification_timeout` MUST accept
    // the 3-account `[task, global_state, client]` form and REJECT the 4-account Claimed form!
    for (const expireTaskState of ["submitted", "approved"] as const) {
      const inspSub = verifyBuiltTransaction(expOpen.unsigned_tx, {
        action: "expire",
        network: "mainnet",
        wallet: clientKp.publicKey.toBase58(),
        taskPda: task.toBase58(),
        expireTaskState,
      });
      expect(inspSub.fee_payer).toBe(clientKp.publicKey.toBase58());
      expect(inspSub.fee_payer).not.toBe(authorityPk.toBase58());
      expect(inspSub.fee_payer).not.toBe(oraclePk.toBase58());
      expect(inspSub.instructions[0].accounts).toEqual([
        task.toBase58(),
        globalStatePda().toBase58(),
        clientKp.publicKey.toBase58(),
      ]);

      expect(() =>
        verifyBuiltTransaction(expClaimed.unsigned_tx, {
          action: "expire",
          network: "mainnet",
          wallet: clientKp.publicKey.toBase58(),
          taskPda: task.toBase58(),
          expireTaskState,
        })
      ).toThrow(/requires exact 3 accounts/);
    }
  });

  it("validates atomic [finalize_task(advance_pda), route_and_recoup] and rejects recipient hijack", () => {
    const agentKp = Keypair.generate();
    const backerKp = Keypair.generate();
    const clientPk = Keypair.generate().publicKey;
    const treasuryPk = Keypair.generate().publicKey;
    const advPk = advancePda(backerKp.publicKey, agentKp.publicKey);

    const built = buildFinalize({
      wallet: agentKp.publicKey.toBase58(),
      network: "mainnet",
      recentBlockhash,
      taskPda: task.toBase58(),
      agent: advPk.toBase58(),
      client: clientPk.toBase58(),
      treasury: treasuryPk.toBase58(),
      agentStateOwner: agentKp.publicKey.toBase58(),
      recoupBacker: backerKp.publicKey.toBase58(),
      recoupRecipient: agentKp.publicKey.toBase58(),
    });
    const insp = verifyIntent(built.unsigned_tx, built.transaction_intent);
    expect(insp.fee_payer).toBe(agentKp.publicKey.toBase58());
    expect(insp.instructions).toHaveLength(2);
    expect(insp.instructions[1].program_id).toBe(
      EXTENSION_CREDIT_PROGRAM_ID.toBase58()
    );
    expect(insp.instructions[1].accounts).toEqual([
      advPk.toBase58(),
      backerKp.publicKey.toBase58(),
      agentKp.publicKey.toBase58(),
    ]);

    expect(
      verifyBuiltTransaction(built.unsigned_tx, {
        action: "finalize",
        network: "mainnet",
        wallet: agentKp.publicKey.toBase58(),
        taskPda: task.toBase58(),
        payoutTo: advPk.toBase58(),
      }).fee_payer
    ).toBe(agentKp.publicKey.toBase58());

    // Hijack attempt: route_and_recoup recipient redirected to attacker -> MUST fail closed
    const attacker = Keypair.generate().publicKey;
    const badBuilt = buildFinalize({
      wallet: agentKp.publicKey.toBase58(),
      network: "mainnet",
      recentBlockhash,
      taskPda: task.toBase58(),
      agent: advPk.toBase58(),
      client: clientPk.toBase58(),
      treasury: treasuryPk.toBase58(),
      agentStateOwner: agentKp.publicKey.toBase58(),
      recoupBacker: backerKp.publicKey.toBase58(),
      recoupRecipient: attacker.toBase58(),
    });
    expect(() =>
      verifyBuiltTransaction(badBuilt.unsigned_tx, {
        action: "finalize",
        network: "mainnet",
        wallet: agentKp.publicKey.toBase58(),
        taskPda: task.toBase58(),
        payoutTo: advPk.toBase58(),
      })
    ).toThrow(/recipient.*must equal caller wallet/);
  });

  it("validates [set_payout_to, verify_task] on outstanding advance and competing-agent score-0 reopen bundle", () => {
    const agentKp = Keypair.generate();
    const oracleKp = Keypair.generate();
    const backerKp = Keypair.generate();
    const advPk = advancePda(backerKp.publicKey, agentKp.publicKey);

    const setPayoutIx = buildShillbotInstruction({
      name: "set_payout_to",
      accounts: { task: task.toBase58(), agent: agentKp.publicKey.toBase58() },
      args: { payout_to: advPk },
    });
    const verifyIx = buildShillbotInstruction({
      name: "verify_task",
      accounts: {
        task: task.toBase58(),
        global_state: globalStatePda(),
        switchboard_feed: oracleKp.publicKey,
      },
      args: {
        composite_score: new BN("900000"),
        verification_hash: Array.from(Buffer.alloc(32, 0x33)),
      },
    });
    const verifyWithPayoutB64 = new Transaction({
      feePayer: agentKp.publicKey,
      recentBlockhash,
    })
      .add(setPayoutIx, verifyIx)
      .serialize({ requireAllSignatures: false, verifySignatures: false })
      .toString("base64");

    const vInsp = verifyBuiltTransaction(verifyWithPayoutB64, {
      action: "verify",
      network: "mainnet",
      wallet: agentKp.publicKey.toBase58(),
      taskPda: task.toBase58(),
      payoutTo: advPk.toBase58(),
      oracleAuthority: oracleKp.publicKey.toBase58(),
    });
    expect(vInsp.fee_payer).toBe(agentKp.publicKey.toBase58());
    expect(vInsp.fee_payer).not.toBe(oracleKp.publicKey.toBase58());

    // Competing-agent score-0 reopen bundle: [verify_task_attested(0), claim_task, submit_work]
    const reopenZeroIx = buildShillbotInstruction({
      name: "verify_task_attested",
      accounts: {
        task: task.toBase58(),
        global_state: globalStatePda(),
        attester: oracleKp.publicKey,
      },
      args: {
        score: new BN("0"),
        verification_hash: Array.from(Buffer.alloc(32, 0x44)),
      },
    });
    const claimIx = buildShillbotInstruction({
      name: "claim_task",
      accounts: {
        task: task.toBase58(),
        global_state: globalStatePda(),
        agent_state: agentStatePda(agentKp.publicKey),
        agent: agentKp.publicKey,
      },
    });
    const submitIx = buildShillbotInstruction({
      name: "submit_work",
      accounts: {
        task: task.toBase58(),
        global_state: globalStatePda(),
        agent_state: agentStatePda(agentKp.publicKey),
        agent: agentKp.publicKey,
      },
      args: { content_id: Buffer.from("eip155:8453:0x" + "aa".repeat(32) + ":0") },
    });
    const reopenBundleB64 = new Transaction({
      feePayer: agentKp.publicKey,
      recentBlockhash,
    })
      .add(reopenZeroIx, claimIx, submitIx)
      .serialize({ requireAllSignatures: false, verifySignatures: false })
      .toString("base64");
    expect(
      verifyBuiltTransaction(reopenBundleB64, {
        action: "submit",
        network: "mainnet",
        wallet: agentKp.publicKey.toBase58(),
        taskPda: task.toBase58(),
        oracleAuthority: oracleKp.publicKey.toBase58(),
      }).fee_payer
    ).toBe(agentKp.publicKey.toBase58());

    // Non-zero score in reopen bundle MUST fail closed
    const reopenNonZeroIx = buildShillbotInstruction({
      name: "verify_task_attested",
      accounts: {
        task: task.toBase58(),
        global_state: globalStatePda(),
        attester: oracleKp.publicKey,
      },
      args: {
        score: new BN("1000000"),
        verification_hash: Array.from(Buffer.alloc(32, 0x44)),
      },
    });
    const badReopenB64 = new Transaction({
      feePayer: agentKp.publicKey,
      recentBlockhash,
    })
      .add(reopenNonZeroIx, claimIx, submitIx)
      .serialize({ requireAllSignatures: false, verifySignatures: false })
      .toString("base64");
    expect(() =>
      verifyBuiltTransaction(badReopenB64, {
        action: "submit",
        network: "mainnet",
        wallet: agentKp.publicKey.toBase58(),
        taskPda: task.toBase58(),
        oracleAuthority: oracleKp.publicKey.toBase58(),
      })
    ).toThrow(/composite_score === 0/);
  });

  it("P1-15: enforces Switchboard/Secp256k1 companion guards across mainnet, verify_task_attested, and devnet verify_task", () => {
    const agentKp = Keypair.generate();
    const oracleKp = Keypair.generate();
    const feedPk = Keypair.generate().publicKey;
    const otherFeedPk = Keypair.generate().publicKey;
    const devnetSwitchboard = new PublicKey("Aio4gaXjXzJNVLtzwtNVmSqGKpANtXhybbkhtAC94ji2");
    const mainnetSwitchboard = new PublicKey("SBondMDrcV3K4kxZR1HNVT7osZxAHVHgYXL5Ze1oMUv");
    const pullFeedDisc = Buffer.from(
      sha256.arrayBuffer("global:pull_feed_submit_response_consensus")
    ).subarray(0, 8);

    const makeSecpIx = (sigCount = 1) =>
      new TransactionInstruction({
        programId: Secp256k1Program.programId,
        keys: [],
        data: Buffer.from([sigCount, 0, 0, 0]),
      });
    const makeSbIx = (programId: PublicKey, disc: Buffer, targetFeed: PublicKey) =>
      new TransactionInstruction({
        programId,
        keys: [{ pubkey: targetFeed, isSigner: false, isWritable: true }],
        data: Buffer.concat([disc, Buffer.alloc(16)]),
      });

    const verifyTaskIx = buildShillbotInstruction({
      name: "verify_task",
      accounts: {
        task: task.toBase58(),
        global_state: globalStatePda(),
        switchboard_feed: feedPk,
      },
      args: {
        composite_score: new BN("900000"),
        verification_hash: Array.from(Buffer.alloc(32, 0x55)),
      },
    });
    const verifyAttestedIx = buildShillbotInstruction({
      name: "verify_task_attested",
      accounts: {
        task: task.toBase58(),
        global_state: globalStatePda(),
        attester: oracleKp.publicKey,
      },
      args: {
        score: new BN("900000"),
        verification_hash: Array.from(Buffer.alloc(32, 0x66)),
      },
    });
    const serialize = (ixs: TransactionInstruction[]) =>
      new Transaction({ feePayer: agentKp.publicKey, recentBlockhash })
        .add(...ixs)
        .serialize({ requireAllSignatures: false, verifySignatures: false })
        .toString("base64");

    // 1. Valid devnet verify_task crank [Secp256k1(1), Switchboard(pull_feed, feedPk), verify_task] PASSES both validators!
    const validDevnet = buildTransaction({
      action: "verify",
      wallet: agentKp.publicKey.toBase58(),
      network: "devnet",
      recentBlockhash,
      taskPda: task.toBase58(),
      switchboardFeed: feedPk.toBase58(),
      compositeScore: "900000",
      verificationHash: "55".repeat(32),
      crankInstructions: [makeSecpIx(1), makeSbIx(devnetSwitchboard, pullFeedDisc, feedPk)],
    });
    expect(
      verifyBuiltTransaction(validDevnet.unsigned_tx, {
        action: "verify",
        network: "devnet",
        wallet: agentKp.publicKey.toBase58(),
        taskPda: task.toBase58(),
        oracleAuthority: feedPk.toBase58(),
      }).fee_payer
    ).toBe(agentKp.publicKey.toBase58());
    expect(verifyIntent(validDevnet.unsigned_tx, validDevnet.transaction_intent).fee_payer).toBe(
      agentKp.publicKey.toBase58()
    );

    // 2. Mainnet Switchboard injection on verify_task MUST reject in both verifyBuiltTransaction and verifyIntent!
    const mainnetInjected = buildTransaction({
      action: "verify",
      wallet: agentKp.publicKey.toBase58(),
      network: "mainnet",
      recentBlockhash,
      taskPda: task.toBase58(),
      switchboardFeed: feedPk.toBase58(),
      compositeScore: "900000",
      verificationHash: "55".repeat(32),
      crankInstructions: [makeSecpIx(1), makeSbIx(mainnetSwitchboard, pullFeedDisc, feedPk)],
    });
    expect(() =>
      verifyBuiltTransaction(mainnetInjected.unsigned_tx, {
        action: "verify",
        network: "mainnet",
        wallet: agentKp.publicKey.toBase58(),
        taskPda: task.toBase58(),
      })
    ).toThrow(/Unexpected program|forbidden on mainnet/i);
    expect(() =>
      verifyIntent(mainnetInjected.unsigned_tx, mainnetInjected.transaction_intent)
    ).toThrow(/unexpected companion program|only permitted on devnet/i);

    // 3. Switchboard injection on verify_task_attested (even on devnet, standalone and with set_payout_to!) MUST reject!
    const attestedWithSb = serialize([
      makeSecpIx(1),
      makeSbIx(devnetSwitchboard, pullFeedDisc, oracleKp.publicKey),
      verifyAttestedIx,
    ]);
    expect(() =>
      verifyBuiltTransaction(attestedWithSb, {
        action: "verify",
        network: "devnet",
        wallet: agentKp.publicKey.toBase58(),
        taskPda: task.toBase58(),
        oracleAuthority: oracleKp.publicKey.toBase58(),
      })
    ).toThrow(/forbidden on verify_task_attested/);

    // 4. Devnet verify_task with arbitrary Switchboard discriminator MUST reject!
    const badDiscTx = serialize([
      makeSecpIx(1),
      makeSbIx(devnetSwitchboard, Buffer.alloc(8, 0xaa), feedPk),
      verifyTaskIx,
    ]);
    expect(() =>
      verifyBuiltTransaction(badDiscTx, {
        action: "verify",
        network: "devnet",
        wallet: agentKp.publicKey.toBase58(),
        taskPda: task.toBase58(),
        oracleAuthority: feedPk.toBase58(),
      })
    ).toThrow(/pull_feed_submit_response_consensus/);

    // 5. Devnet verify_task with wrong order (Switchboard before Secp256k1 or after verify_task) MUST reject!
    const reversedOrderTx = serialize([
      makeSbIx(devnetSwitchboard, pullFeedDisc, feedPk),
      makeSecpIx(1),
      verifyTaskIx,
    ]);
    expect(() =>
      verifyBuiltTransaction(reversedOrderTx, {
        action: "verify",
        network: "devnet",
        wallet: agentKp.publicKey.toBase58(),
        taskPda: task.toBase58(),
      })
    ).toThrow(/exact order/);

    // 6. Devnet verify_task with 0-signature Secp256k1 or mismatched feed account MUST reject!
    const zeroSigTx = serialize([
      makeSecpIx(0),
      makeSbIx(devnetSwitchboard, pullFeedDisc, feedPk),
      verifyTaskIx,
    ]);
    expect(() =>
      verifyBuiltTransaction(zeroSigTx, {
        action: "verify",
        network: "devnet",
        wallet: agentKp.publicKey.toBase58(),
        taskPda: task.toBase58(),
      })
    ).toThrow(/at least 1 signature/);

    const wrongFeedTx = serialize([
      makeSecpIx(1),
      makeSbIx(devnetSwitchboard, pullFeedDisc, otherFeedPk),
      verifyTaskIx,
    ]);
    expect(() =>
      verifyBuiltTransaction(wrongFeedTx, {
        action: "verify",
        network: "devnet",
        wallet: agentKp.publicKey.toBase58(),
        taskPda: task.toBase58(),
        oracleAuthority: feedPk.toBase58(),
      })
    ).toThrow(/expected feed/);
  });
});
