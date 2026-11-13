import { chmodSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { inspect } from "node:util";
import { spawnSync } from "node:child_process";
import bs58 from "bs58";
import {
  ComputeBudgetProgram,
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
  Transaction,
  TransactionInstruction,
} from "@solana/web3.js";
import { privateKeyToAccount } from "viem/accounts";
import { describe, expect, it, vi } from "vitest";
import {
  CLIENT_VERSION,
  DEFAULT_MAX_NETWORK_FEE_WEI,
  DEFAULT_MAX_VOW_BYTES,
  MAX_COMPUTE_UNIT_PRICE_MICROLAMPORTS,
  SwarmAgent,
  SwarmClientError,
  VALID_TOPICS,
  buildOnChainActionProofRef,
  createSwarmAgent,
  estimateSolanaNetworkFeeLamports,
  extractCreateEscrowLamports,
  fetchVowAttestationJson,
  formatOnChainActionAttributionMessage,
  fromEvmPrivateKey,
  fromEvmWalletClient,
  fromSolanaKeypair,
  fromSolanaSecretKey,
  fromSolanaWalletAdapter,
  isValidTopicId,
  validateComputeBudgetInstructions,
  validatePreparedEvmCall,
  validatePreparedTransaction,
  validateVowUrl,
  verifyBuiltTransaction,
  verifyVow,
  type EvmSwarmSigner,
} from "../src/index.js";
import {
  SHILLBOT_ESCROW_DEPLOYMENTS,
  SHILLBOT_EVM_SELECTORS,
  ShillbotApiClient,
} from "../src/shillbot/api.js";
import {
  SHILLBOT_PROGRAM_ID,
  buildClaim,
  buildCreate,
  buildFinalize,
  buildInstruction,
  buildShillbotInstruction,
  buildVerify,
  inspectTransaction,
  shillbotInstructionDiscriminator,
  verifyIntent,
} from "../src/shillbot/index.js";
import { InboxClient } from "../src/inbox/client.js";
import { parseCliArgs, runSwarmCli } from "../src/cli/swarm.js";

const testDir = dirname(fileURLToPath(import.meta.url));
const clientRoot = resolve(testDir, "..");
const repoRoot = resolve(clientRoot, "../..");

function makeJwt(wallet: string, expSeconds: number): string {
  const b64url = (obj: unknown) =>
    Buffer.from(JSON.stringify(obj), "utf8")
      .toString("base64")
      .replace(/\+/g, "-")
      .replace(/\//g, "_")
      .replace(/=+$/g, "");
  return `${b64url({ alg: "HS256", typ: "JWT" })}.${b64url({ wallet, exp: expSeconds })}.sig`;
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

function mockConnection(txSig = "5MockSig11111111111111111111111111111111111111"): Connection {
  return {
    sendRawTransaction: vi.fn(async () => txSig),
    confirmTransaction: vi.fn(async () => ({ value: { err: null } })),
  } as unknown as Connection;
}

describe("Version sync & multi-bin package contract", () => {
  it("exports CLIENT_VERSION matching package.json 0.4.2 and registers both swarm and client bins", () => {
    const pkg = JSON.parse(readFileSync(resolve(clientRoot, "package.json"), "utf8")) as {
      version: string;
      bin: Record<string, string>;
    };
    expect(pkg.version).toBe("0.4.2");
    expect(CLIENT_VERSION).toBe(pkg.version);
    expect(pkg.bin.swarm).toBe("./dist/cli/swarm.js");
    expect(pkg.bin.client).toBe("./dist/cli/swarm.js");
  });

  it("exits the CLI child process immediately even when an active RPC/WebSocket handle is open", async () => {
    const { buildSync } = await import("esbuild");
    const cliBundle = resolve(clientRoot, `.tmp-swarm-cli-exit-${process.pid}.mjs`);
    try {
      buildSync({
        entryPoints: [resolve(clientRoot, "src/cli/swarm.ts")],
        bundle: true,
        platform: "node",
        format: "esm",
        packages: "external",
        outfile: cliBundle,
      });
      const res = spawnSync(
        process.execPath,
        [
          "--import",
          "data:text/javascript,setInterval(()=>{},60000);",
          cliBundle,
          "--version",
          "--json",
        ],
        { cwd: clientRoot, encoding: "utf8", timeout: 4000 },
      );
      expect(res.error).toBeUndefined();
      expect(res.status, res.stderr).toBe(0);
      expect(JSON.parse(res.stdout)).toEqual({ version: "0.4.2" });
    } finally {
      rmSync(cliBundle, { force: true });
    }
  });

  it("queries GET /leaderboard?tournament_id=...&limit=... on SwarmAgent.games.leaderboard", async () => {
    let seenUrl = "";
    const agent = createSwarmAgent({
      network: "devnet",
      gameUrl: "https://api.coordination.game",
      fetch: (async (input: RequestInfo | URL) => {
        seenUrl = String(input);
        return new Response(JSON.stringify([{ wallet: "solana:x:abc", score: 1 }]), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }) as typeof fetch,
    });
    const rows = await agent.games.leaderboard("devnet", { limit: 5 });
    expect(seenUrl).toBe(
      "https://api.coordination.game/leaderboard?tournament_id=1003&limit=5&network=devnet",
    );
    expect(Array.isArray(rows)).toBe(true);
  });
});

describe("Pluggable signers & C6 redaction", () => {
  it("loads Solana signers from Keypair, Uint8Array, base58, JSON array, and wallet adapter with C6 redaction", async () => {
    const kp = Keypair.generate();
    const b58Secret = bs58.encode(kp.secretKey);
    const jsonSecret = JSON.stringify(Array.from(kp.secretKey));

    const s1 = fromSolanaKeypair(kp);
    const s2 = fromSolanaSecretKey(kp.secretKey);
    const s3 = fromSolanaSecretKey(b58Secret);
    const s4 = fromSolanaSecretKey(jsonSecret);

    expect(s1.address).toBe(kp.publicKey.toBase58());
    expect(s2.address).toBe(s1.address);
    expect(s3.address).toBe(s1.address);
    expect(s4.address).toBe(s1.address);

    const msg = new TextEncoder().encode("swarm-auth-nonce-123");
    const sig1 = await s1.signMessage(msg);
    const sig3 = await s3.signMessage(msg);
    expect(sig1).toHaveLength(64);
    expect(Buffer.from(sig1).equals(Buffer.from(sig3))).toBe(true);

    // Verify signature with WebCrypto Ed25519
    const pubKey = await globalThis.crypto.subtle.importKey(
      "raw",
      new Uint8Array(kp.publicKey.toBytes()),
      { name: "Ed25519" },
      false,
      ["verify"],
    );
    expect(
      await globalThis.crypto.subtle.verify(
        "Ed25519",
        pubKey,
        new Uint8Array(sig1),
        new Uint8Array(msg),
      ),
    ).toBe(true);

    // Wallet adapter
    const adapterSigner = fromSolanaWalletAdapter({
      publicKey: kp.publicKey,
      signMessage: async (m) => s1.signMessage(m),
      signTransaction: async (tx) => s1.signTransaction(tx),
    });
    expect(adapterSigner.address).toBe(s1.address);

    // C6 Redaction: never serialize or inspect secret key material
    for (const signer of [s1, s2, s3, s4, adapterSigner]) {
      expect(JSON.parse(JSON.stringify(signer))).toEqual({
        chain: "solana",
        address: kp.publicKey.toBase58(),
      });
      expect(String(signer)).not.toContain(b58Secret);
      expect(inspect(signer)).not.toContain(b58Secret);
      expect(Object.getOwnPropertyNames(signer)).not.toContain("secretKey");
    }
  });

  it("loads EVM signers matching viem EIP-55 address and EIP-191 signature with C6 redaction", async () => {
    const privKey =
      "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80" as const;
    const viemAccount = privateKeyToAccount(privKey);
    const evmSigner = fromEvmPrivateKey(privKey, { chainId: 84532 });

    expect(evmSigner.address).toBe(viemAccount.address);
    const sig = await evmSigner.signMessage("evm-challenge-nonce");
    const expectedSig = await viemAccount.signMessage({ message: "evm-challenge-nonce" });
    expect(sig).toBe(expectedSig);

    // C6 Redaction
    expect(JSON.parse(JSON.stringify(evmSigner))).toEqual({
      chain: "evm",
      address: viemAccount.address,
    });
    expect(String(evmSigner)).not.toContain(privKey.slice(2));
    expect(inspect(evmSigner)).not.toContain(privKey.slice(2));

    // WalletClient adapter
    const clientSigner = fromEvmWalletClient({
      account: { address: viemAccount.address },
      chain: { id: 84532 },
      signMessage: async ({ message }: { message: string | { raw: Uint8Array } }) =>
        viemAccount.signMessage({ message }),
      sendTransaction: async () => `0x${"ab".repeat(32)}` as const,
    });
    expect(clientSigner.address).toBe(viemAccount.address);
    expect(await clientSigner.signMessage("hello")).toBe(
      await viemAccount.signMessage({ message: "hello" }),
    );
  });
});

describe("SwarmAgent auth caching, deduplication, 401 refresh & Inbox C1 Bearer mode", () => {
  it("caches JWT, deduplicates concurrent auth, and refreshes once on 401", async () => {
    const kp = Keypair.generate();
    const signer = fromSolanaKeypair(kp);
    const futureExp = Math.floor(Date.now() / 1000) + 3600;
    const token1 = makeJwt(signer.address, futureExp);
    const token2 = makeJwt(signer.address, futureExp + 3600);

    let authVerifyCount = 0;
    let agentTasksAttempt = 0;

    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      const headers = (init?.headers ?? {}) as Record<string, string>;
      if (url === "https://api.coordination.game/auth/challenge") {
        return jsonResponse({ nonce: `nonce-${authVerifyCount + 1}` });
      }
      if (url === "https://api.coordination.game/auth/verify") {
        authVerifyCount += 1;
        return jsonResponse({ token: authVerifyCount === 1 ? token1 : token2 });
      }
      if (url === "https://api.shillbot.org/agent/earnings") {
        expect(headers.Authorization).toBe(`Bearer ${token1}`);
        return jsonResponse({ total_earned_lamports: 12345 });
      }
      if (url === "https://api.shillbot.org/agent/tasks") {
        agentTasksAttempt += 1;
        if (agentTasksAttempt === 1) {
          expect(headers.Authorization).toBe(`Bearer ${token1}`);
          return jsonResponse({ error: "token revoked" }, 401);
        }
        expect(headers.Authorization).toBe(`Bearer ${token2}`);
        return jsonResponse({ tasks: [{ task_id: "t-1", state: "claimed" }] });
      }
      throw new Error(`Unexpected URL: ${url}`);
    });

    const agent = createSwarmAgent({
      signer,
      fetch: fetchMock as unknown as typeof fetch,
    });

    // Concurrent calls share one challenge+verify handshake
    const [tokA, tokB, earnings] = await Promise.all([
      agent.auth.getToken(),
      agent.auth.getToken(),
      agent.tasks.earnings(),
    ]);
    expect(tokA).toBe(token1);
    expect(tokB).toBe(token1);
    expect(earnings).toEqual({ total_earned_lamports: 12345 });
    expect(authVerifyCount).toBe(1);

    // 401 triggers invalidate() + re-auth + single retry
    const mine = await agent.tasks.mine();
    expect(mine).toEqual([{ task_id: "t-1", state: "claimed" }]);
    expect(authVerifyCount).toBe(2);
    expect(agentTasksAttempt).toBe(2);
  });

  it("enforces C1 in InboxClient Bearer mode: sends Authorization Bearer, never X-Inbox-Session or /internal/inbox/session", async () => {
    const kp = Keypair.generate();
    const signer = fromSolanaKeypair(kp);
    const jwt = makeJwt(signer.address, Math.floor(Date.now() / 1000) + 3600);

    const seenCalls: Array<{ url: string; headers: Record<string, string>; body?: string }> = [];
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      const headers = { ...((init?.headers ?? {}) as Record<string, string>) };
      seenCalls.push({ url, headers, body: init?.body ? String(init.body) : undefined });
      if (url.includes("/internal/inbox/list")) {
        return jsonResponse({
          messages: [],
          count: 0,
          next_cursor: null,
          filtered_acknowledged: 0,
          filtered_muted: 0,
          filtered_below_min_trust: 0,
        });
      }
      if (url.includes("/internal/inbox/mute")) {
        return jsonResponse({ thread_id: "task:abc", muted: true });
      }
      throw new Error(`Unexpected URL: ${url}`);
    });

    const agent = new SwarmAgent({
      signer,
      token: jwt,
      fetch: fetchMock as unknown as typeof fetch,
    });

    await agent.inbox.list({ status: "pending" });
    const muteRes = await agent.inbox.mute("task:abc", true);
    expect(muteRes).toEqual({ thread_id: "task:abc", muted: true });

    expect(seenCalls).toHaveLength(2);
    for (const call of seenCalls) {
      expect(call.url).not.toContain("/internal/inbox/session");
      expect(call.headers.Authorization).toBe(`Bearer ${jwt}`);
      expect(call.headers["X-Inbox-Session"]).toBeUndefined();
      expect(call.headers["x-inbox-session"]).toBeUndefined();
    }

    // Also verify legacy InboxClient session mode remains intact when getToken is omitted
    const legacyFetch = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse({ nonce: "n-1" }))
      .mockResolvedValueOnce(jsonResponse({ session_id: "sess-legacy", tier: "session" }))
      .mockResolvedValueOnce(jsonResponse({ messages: [], next_cursor: null }));
    const legacy = new InboxClient({
      baseUrl: "https://mcp.swarm.tips",
      storage: null,
      fetchFn: legacyFetch as unknown as typeof fetch,
    });
    await legacy.createSession(signer.address, async (n) => `sig:${n}`);
    await legacy.getMessages();
    const lastHeaders = legacyFetch.mock.calls[2][1]?.headers as Record<string, string>;
    expect(lastHeaders["X-Inbox-Session"]).toBe("sess-legacy");
    expect(lastHeaders.Authorization).toBeUndefined();
  });
});

describe("ShillbotApiClient.buildVerify 1-hop flow & fallback", () => {
  it("calls POST /internal/build-verify-tx with Bearer token in 1 hop", async () => {
    const wallet = Keypair.generate().publicKey.toBase58();
    const taskOwner = Keypair.generate().publicKey;
    const feed = Keypair.generate().publicKey.toBase58();
    const built = buildVerify({
      wallet,
      network: "mainnet",
      recentBlockhash: "11111111111111111111111111111111",
      taskPda: Keypair.generate().publicKey.toBase58(),
      switchboardFeed: feed,
      compositeScore: "1000000",
      verificationHash: "aa".repeat(32),
      crankInstructions: [],
    });

    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      const headers = (init?.headers ?? {}) as Record<string, string>;
      expect(url).toBe("https://mcp.swarm.tips/internal/build-verify-tx");
      expect(headers.Authorization).toBe("Bearer my-jwt");
      const body = JSON.parse(String(init?.body));
      expect(body).toMatchObject({
        task_id: "task-verify-1",
        payer: wallet,
        network: "mainnet",
      });
      return jsonResponse({
        transaction: built.unsigned_tx,
        task_pda: built.transaction_intent.task_pda,
        transaction_intent: built.transaction_intent,
      });
    });

    const api = new ShillbotApiClient({
      baseUrl: "https://api.shillbot.org",
      fetch: fetchMock as unknown as typeof fetch,
      getToken: () => "my-jwt",
    });
    void taskOwner;
    const res = await api.buildVerify("task-verify-1", { payer: wallet, network: "mainnet" });
    expect(res.transaction).toBe(built.unsigned_tx);
    expect(res.task_pda).toBe(built.transaction_intent.task_pda);
    expect(res.transaction_intent).toEqual(built.transaction_intent);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });
});

describe("C9 Spend & Fee Caps + C10 Transaction & EVM Call Validation", () => {
  it("enforces default-zero SPEND_CAP_EXCEEDED on fundAndExecute while allowing bounded network fees on verifyAndExecute / finalizeAndExecute", async () => {
    const kp = Keypair.generate();
    const signer = fromSolanaKeypair(kp);
    const jwt = makeJwt(signer.address, Math.floor(Date.now() / 1000) + 3600);
    const taskKey = Keypair.generate().publicKey.toBase58();
    const clientKey = Keypair.generate().publicKey.toBase58();
    const treasuryKey = Keypair.generate().publicKey.toBase58();
    const feedKey = Keypair.generate().publicKey.toBase58();

    const createdTx = buildCreate({
      wallet: signer.address,
      network: "mainnet",
      recentBlockhash: "11111111111111111111111111111111",
      nonce: "1",
      escrowLamports: "5000000",
      contentHash: "22".repeat(32),
      deadline: "1900000000",
      submitMargin: "60",
      claimBuffer: "60",
      platform: 10,
      attestationDelayOverride: 0,
      challengeWindowOverride: 0,
      verificationTimeoutOverride: 0,
      requiresApproval: false,
      verificationKind: 1,
    });

    const verifyTx = buildVerify({
      wallet: signer.address,
      network: "mainnet",
      recentBlockhash: "11111111111111111111111111111111",
      taskPda: taskKey,
      switchboardFeed: feedKey,
      compositeScore: "1000000",
      verificationHash: "33".repeat(32),
      crankInstructions: [],
    });

    const finalizeTx = buildFinalize({
      wallet: signer.address,
      network: "mainnet",
      recentBlockhash: "11111111111111111111111111111111",
      taskPda: taskKey,
      agent: signer.address,
      client: clientKey,
      treasury: treasuryKey,
    });

    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.endsWith("/campaigns/camp-1/fund")) {
        return jsonResponse({
          transaction: createdTx.unsigned_tx,
          task_pda: createdTx.transaction_intent.task_pda,
        });
      }
      if (url.endsWith("/internal/build-verify-tx")) {
        return jsonResponse({
          transaction: verifyTx.unsigned_tx,
          task_pda: taskKey,
          transaction_intent: verifyTx.transaction_intent,
        });
      }
      if (url.endsWith("/tasks/t-fin/build-finalize")) {
        return jsonResponse({
          transaction: finalizeTx.unsigned_tx,
          task_pda: taskKey,
        });
      }
      if (url.includes("/confirm")) {
        return jsonResponse({ task_id: "ok", action: "confirmed", message: "ok" });
      }
      throw new Error(`Unexpected URL: ${url}`);
    });

    const defaultAgent = createSwarmAgent({
      signer,
      token: jwt,
      fetch: fetchMock as unknown as typeof fetch,
    });

    // Default maxEscrowSpendLamports === 0n MUST block fundAndExecute with SPEND_CAP_EXCEEDED
    await expect(
      defaultAgent.campaigns.fundAndExecute("camp-1", { amount_lamports: 5_000_000 }),
    ).rejects.toMatchObject({
      name: "SwarmClientError",
      code: "SPEND_CAP_EXCEEDED",
    });

    // Even when body omits amount_lamports, inspecting create_task instruction rejects > 0n
    await expect(defaultAgent.campaigns.fundAndExecute("camp-1", {})).rejects.toMatchObject({
      name: "SwarmClientError",
      code: "SPEND_CAP_EXCEEDED",
    });

    // Bounded network fees (5_000 lamports <= 100_000 lamports) on verifyAndExecute & finalizeAndExecute SUCCEED!
    const conn = mockConnection("5SigAllowed111111111111111111111111111111111");
    const verRes = await defaultAgent.tasks.verifyAndExecute("t-ver", { connection: conn });
    expect(verRes.signature).toBe("5SigAllowed111111111111111111111111111111111");

    const finRes = await defaultAgent.tasks.finalizeAndExecute("t-fin", { connection: conn });
    expect(finRes.signature).toBe("5SigAllowed111111111111111111111111111111111");

    // Excessive priority fee (> 100_000 lamports) on finalizeAndExecute MUST be blocked by C9!
    const finalizeIx = buildInstruction({
      action: "finalize",
      wallet: signer.address,
      network: "mainnet",
      recentBlockhash: "11111111111111111111111111111111",
      taskPda: taskKey,
      agent: signer.address,
      client: clientKey,
      treasury: treasuryKey,
    });
    const highFeeTx = new Transaction({
      feePayer: kp.publicKey,
      recentBlockhash: "11111111111111111111111111111111",
    }).add(
      ComputeBudgetProgram.setComputeUnitLimit({ units: 400_000 }),
      ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 1_000_000 }), // 0.4 SOL priority fee!
      finalizeIx,
    );
    const highFeeB64 = highFeeTx
      .serialize({ requireAllSignatures: false, verifySignatures: false })
      .toString("base64");

    const highFeeAgent = createSwarmAgent({
      signer,
      token: jwt,
      fetch: vi.fn(async () =>
        jsonResponse({ transaction: highFeeB64, task_pda: taskKey }),
      ) as unknown as typeof fetch,
    });
    await expect(
      highFeeAgent.tasks.finalizeAndExecute("t-drain", { connection: conn }),
    ).rejects.toMatchObject({
      name: "SwarmClientError",
      code: "SPEND_CAP_EXCEEDED",
    });
  });

  it("auto-sponsors claimAndExecute via /tasks/{id}/sponsor and advises lifecycle via nextAction", async () => {
    const agentKp = Keypair.generate();
    const sponsorKp = Keypair.generate();
    const taskKey = Keypair.generate().publicKey.toBase58();
    const signer = fromSolanaKeypair(agentKp);
    const jwt = makeJwt(signer.address, Math.floor(Date.now() / 1000) + 3600);

    // Build unsigned template where sponsor is fee_payer
    const claimTemplate = buildClaim({
      wallet: signer.address,
      sponsor: sponsorKp.publicKey.toBase58(),
      network: "mainnet",
      recentBlockhash: "11111111111111111111111111111111",
      taskPda: taskKey,
    });

    // Sponsor co-signs the template
    const sponsorSignedTx = Transaction.from(Buffer.from(claimTemplate.unsigned_tx, "base64"));
    sponsorSignedTx.partialSign(sponsorKp);
    const sponsorSignedB64 = sponsorSignedTx
      .serialize({ requireAllSignatures: false, verifySignatures: false })
      .toString("base64");

    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      if (url.includes("/tasks/t-claim/claim")) {
        return jsonResponse({ transaction: claimTemplate.unsigned_tx, task_pda: taskKey });
      }
      if (url.endsWith("/tasks/t-claim/sponsor")) {
        const body = JSON.parse(String(init?.body));
        expect(body.action).toBe("claim");
        expect(body.unsigned_transaction).toBe(claimTemplate.unsigned_tx);
        return jsonResponse({ transaction: sponsorSignedB64, task_pda: taskKey });
      }
      if (url.endsWith("/tasks/t-claim/confirm")) {
        return jsonResponse({ task_id: "t-claim", action: "claim", message: "confirmed" });
      }
      throw new Error(`Unexpected URL: ${url}`);
    });

    const agent = createSwarmAgent({
      signer,
      token: jwt,
      fetch: fetchMock as unknown as typeof fetch,
    });
    const conn = mockConnection("5ClaimSponsoredSig11111111111111111111111111");
    const claimed = await agent.tasks.claimAndExecute("t-claim", { connection: conn });
    expect(claimed.signature).toBe("5ClaimSponsoredSig11111111111111111111111111");

    // Stateless nextAction advisor
    expect(agent.tasks.nextAction({ state: "open" }).step).toBe("claim");
    expect(agent.tasks.nextAction({ state: "claimed" }).step).toBe("submit");
    const futureIso = new Date(Date.now() + 120_000).toISOString();
    expect(
      agent.tasks.nextAction({ state: "submitted", attestation_ready_at: futureIso }),
    ).toEqual({
      step: "wait_attestation",
      not_before: futureIso,
    });
    expect(agent.tasks.nextAction({ state: "submitted", composite_score: 1_000_000 }).step).toBe(
      "verify",
    );
    expect(
      agent.tasks.nextAction({ state: "verified", challenge_deadline: futureIso }),
    ).toEqual({
      step: "wait_challenge",
      not_before: futureIso,
    });
    expect(
      agent.tasks.nextAction({
        state: "verified",
        challenge_deadline: new Date(Date.now() - 5_000).toISOString(),
      }).step,
    ).toBe("finalize");
    expect(agent.tasks.nextAction({ state: "finalized" }).step).toBe("done");
  });

  it("rejects tampered Solana transactions and tampered EVM calls (C10)", () => {
    const kp = Keypair.generate();
    const taskKey = Keypair.generate().publicKey.toBase58();
    const claimIx = buildInstruction({
      action: "claim",
      wallet: kp.publicKey.toBase58(),
      network: "mainnet",
      recentBlockhash: "11111111111111111111111111111111",
      taskPda: taskKey,
    });
    const drainTx = new Transaction({
      feePayer: kp.publicKey,
      recentBlockhash: "11111111111111111111111111111111",
    }).add(
      claimIx,
      SystemProgram.transfer({
        fromPubkey: kp.publicKey,
        toPubkey: Keypair.generate().publicKey,
        lamports: 1_000_000,
      }),
    );
    const drainB64 = drainTx
      .serialize({ requireAllSignatures: false, verifySignatures: false })
      .toString("base64");

    expect(() =>
      validatePreparedTransaction(drainB64, {
        action: "claim",
        network: "mainnet",
        wallet: kp.publicKey.toBase58(),
        taskPda: taskKey,
      }),
    ).toThrow(SwarmClientError);

    // EVM call validation (C10)
    const canonicalEscrow = SHILLBOT_ESCROW_DEPLOYMENTS["eip155:84532"];
    const validClaimData = `${SHILLBOT_EVM_SELECTORS.claim}${"00".repeat(32)}` as const;

    expect(
      validatePreparedEvmCall(
        { to: canonicalEscrow, data: validClaimData, value_wei: "0" },
        { action: "claim", chain: "eip155:84532" },
      ),
    ).toMatchObject({
      to: canonicalEscrow,
      value: 0n,
    });

    // Wrong contract address
    expect(() =>
      validatePreparedEvmCall(
        { to: "0x000000000000000000000000000000000000dead", data: validClaimData, value_wei: "0" },
        { action: "claim", chain: "eip155:84532" },
      ),
    ).toThrow(/contract/i);

    // Wrong 4-byte selector (createTask selector on a claim expectation!)
    expect(() =>
      validatePreparedEvmCall(
        {
          to: canonicalEscrow,
          data: `${SHILLBOT_EVM_SELECTORS.create}${"00".repeat(160)}`,
          value_wei: "0",
        },
        { action: "claim", chain: "eip155:84532" },
      ),
    ).toThrow(/selector/i);

    // Nonzero value_wei on earn action (claim/submit/verify/finalize)
    expect(() =>
      validatePreparedEvmCall(
        { to: canonicalEscrow, data: validClaimData, value_wei: "1" },
        { action: "claim", chain: "eip155:84532", maxValueWei: 1_000n },
      ),
    ).toThrow(/value/i);
  });
});

describe("C6 CLI security, keyfile permissions, swarm tx verify --json, and Python safe verifier", () => {
  it("rejects --private-key / --secret-key / --mnemonic / --seed flags with exit code 2 without echoing the secret", async () => {
    for (const flag of [
      ["--private-key", "SUPER_SECRET_HEX_VALUE"],
      ["--secret-key=SUPER_SECRET_B58_VALUE"],
      ["--mnemonic", "abandon abandon abandon"],
      ["--seed", "deadbeef"],
    ]) {
      let out = "";
      let err = "";
      const code = await runSwarmCli(["auth", "whoami", ...flag], {
        stdout: { write: (s) => ((out += s), true), isTTY: true },
        stderr: { write: (s) => ((err += s), true) },
        env: {},
      });
      expect(code).toBe(2);
      expect(out + err).not.toContain("SUPER_SECRET");
      expect(out + err).not.toContain("abandon");
      expect(out + err).toMatch(/--keypair-file|SWARM_SOLANA_KEYPAIR_PATH/);
    }
  });

  it("refuses group/world-readable keyfiles (0o644) with exit 2 and accepts 0o600 keyfiles", async () => {
    const dir = mkdtempSync(resolve(tmpdir(), "swarm-keyfile-test-"));
    try {
      const kp = Keypair.generate();
      const keyPath = resolve(dir, "id.json");
      writeFileSync(keyPath, JSON.stringify(Array.from(kp.secretKey)));

      if (process.platform !== "win32") {
        chmodSync(keyPath, 0o644);
        let err644 = "";
        const code644 = await runSwarmCli(
          ["auth", "whoami", "--keypair-file", keyPath, "--json"],
          {
            stdout: { write: (s) => ((err644 += s), true), isTTY: false },
            stderr: { write: (s) => ((err644 += s), true) },
            env: {},
          },
        );
        expect(code644).toBe(2);
        expect(err644).toMatch(/chmod 600/);
      }

      chmodSync(keyPath, 0o600);
      let stdout600 = "";
      const code600 = await runSwarmCli(
        ["auth", "whoami", "--keypair-file", keyPath, "--json"],
        {
          stdout: { write: (s) => ((stdout600 += s), true), isTTY: false },
          stderr: { write: () => true },
          env: {},
        },
      );
      expect(code600).toBe(0);
      expect(JSON.parse(stdout600)).toMatchObject({
        wallet: kp.publicKey.toBase58(),
        chain: "solana",
      });
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("validates base64 transactions via `swarm tx verify --json` and standalone Python `verify_shillbot_tx.py`", async () => {
    const kp = Keypair.generate();
    const wallet = kp.publicKey.toBase58();
    const taskKey = Keypair.generate().publicKey.toBase58();

    const validClaim = buildClaim({
      wallet,
      network: "mainnet",
      recentBlockhash: "11111111111111111111111111111111",
      taskPda: taskKey,
    });

    // 1. Valid tx in `swarm tx verify --json` -> exit 0, { ok: true, inspection }
    let okOut = "";
    const okCode = await runSwarmCli(
      [
        "tx",
        "verify",
        "--action",
        "claim",
        "--wallet",
        wallet,
        "--task-pda",
        taskKey,
        "--tx",
        validClaim.unsigned_tx,
        "--json",
      ],
      {
        stdout: { write: (s) => ((okOut += s), true), isTTY: false },
        stderr: { write: () => true },
        env: {},
      },
    );
    expect(okCode).toBe(0);
    const parsedOk = JSON.parse(okOut);
    expect(parsedOk.ok).toBe(true);
    expect(parsedOk.inspection.fee_payer).toBe(wallet);

    // 2. Tampered tx with rogue instruction -> exit 1, { ok: false, error }
    const rogueIx = new TransactionInstruction({
      programId: SystemProgram.programId,
      keys: [{ pubkey: kp.publicKey, isSigner: true, isWritable: true }],
      data: Buffer.from([2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0]),
    });
    const claimIx = buildInstruction({
      action: "claim",
      wallet,
      network: "mainnet",
      recentBlockhash: "11111111111111111111111111111111",
      taskPda: taskKey,
    });
    const tamperedB64 = new Transaction({
      feePayer: kp.publicKey,
      recentBlockhash: "11111111111111111111111111111111",
    })
      .add(claimIx, rogueIx)
      .serialize({ requireAllSignatures: false, verifySignatures: false })
      .toString("base64");

    let badOut = "";
    const badCode = await runSwarmCli(
      [
        "tx",
        "verify",
        "--action",
        "claim",
        "--wallet",
        wallet,
        "--task-pda",
        taskKey,
        "--json",
      ],
      {
        stdout: { write: (s) => ((badOut += s), true), isTTY: false },
        stderr: { write: () => true },
        stdin: () => tamperedB64,
        env: {},
      },
    );
    expect(badCode).toBe(1);
    const parsedBad = JSON.parse(badOut);
    expect(parsedBad.ok).toBe(false);
    expect(parsedBad.error).toMatch(/Unexpected program/i);

    // 3. Standalone Python non-custodial verifier fixture (`examples/python-safe-recipe/verify_shillbot_tx.py`)
    const pyScript = resolve(repoRoot, "examples/python-safe-recipe/verify_shillbot_tx.py");
    const pyGood = spawnSync(
      "python3",
      [pyScript, "--action", "claim", "--wallet", wallet, "--fee-payer", wallet],
      { input: validClaim.unsigned_tx, encoding: "utf8" },
    );
    expect(pyGood.status).toBe(0);
    expect(JSON.parse(pyGood.stdout)).toMatchObject({ ok: true, fee_payer: wallet });

    const pyBad = spawnSync(
      "python3",
      [pyScript, "--action", "claim", "--wallet", wallet, "--fee-payer", wallet],
      { input: tamperedB64, encoding: "utf8" },
    );
    expect(pyBad.status).toBe(1);

    // Unexpected create_task passed when expecting claim -> Python verifier rejects!
    const unexpectedCreate = buildCreate({
      wallet,
      network: "mainnet",
      recentBlockhash: "11111111111111111111111111111111",
      nonce: "9",
      escrowLamports: "5000000",
      contentHash: "44".repeat(32),
      deadline: "1900000000",
      submitMargin: "60",
      claimBuffer: "60",
      platform: 10,
      attestationDelayOverride: 0,
      challengeWindowOverride: 0,
      verificationTimeoutOverride: 0,
      requiresApproval: false,
      verificationKind: 1,
    });
    const pyCreateAsClaim = spawnSync(
      "python3",
      [pyScript, "--action", "claim", "--wallet", wallet],
      { input: unexpectedCreate.unsigned_tx, encoding: "utf8" },
    );
    expect(pyCreateAsClaim.status).toBe(1);
  });

  it("validates OnChainAction atomic [claim_task, (set_payout_to?), submit_work], proof refs, and adviseNextAction", async () => {
    const kp = Keypair.generate();
    const wallet = kp.publicKey.toBase58();
    const taskKey = Keypair.generate().publicKey.toBase58();
    const otherTaskKey = Keypair.generate().publicKey.toBase58();
    const payoutKey = Keypair.generate().publicKey;
    const blockhash = "11111111111111111111111111111111";

    const evmPrivKey =
      "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80" as const;
    const evmSigner = fromEvmPrivateKey(evmPrivKey, { chainId: 56 });
    const attrMsg = formatOnChainActionAttributionMessage("eip155:56", wallet);
    expect(attrMsg).toBe(`swarm-attr-v1:eip155:56:${wallet}`);
    const attrSig = await evmSigner.signMessage(attrMsg);
    const proofRef = buildOnChainActionProofRef({
      targetChainId: "eip155:56",
      txId: `0x${"AB".repeat(32)}`,
      index: 0,
      attributionSignature: attrSig,
    });
    expect(proofRef).toMatch(/^eip155:56:0x[0-9a-f]{64}:0:sig:[A-Za-z0-9_-]{87}$/);

    // adviseNextAction routes open onchain_action directly to submit
    expect(SwarmAgent.adviseNextAction({ state: "open", platform: "onchain_action" })).toEqual({
      step: "submit",
    });
    expect(SwarmAgent.adviseNextAction({ state: "open", platform: 11 })).toEqual({
      step: "submit",
    });
    expect(SwarmAgent.adviseNextAction({ state: "open", platform: "lean_proof" })).toEqual({
      step: "claim",
    });

    const claimIx = buildInstruction({
      action: "claim",
      wallet,
      network: "mainnet",
      recentBlockhash: blockhash,
      taskPda: taskKey,
    });
    const payoutIx = buildShillbotInstruction({
      name: "set_payout_to",
      accounts: { task: taskKey, agent: wallet },
      args: { payout_to: payoutKey },
    });
    const submitIx = buildInstruction({
      action: "submit",
      wallet,
      network: "mainnet",
      recentBlockhash: blockhash,
      taskPda: taskKey,
      contentId: proofRef,
    });

    const atomicSubmitB64 = new Transaction({ feePayer: kp.publicKey, recentBlockhash: blockhash })
      .add(claimIx, submitIx)
      .serialize({ requireAllSignatures: false, verifySignatures: false })
      .toString("base64");
    expect(
      validatePreparedTransaction(atomicSubmitB64, {
        action: "submit",
        network: "mainnet",
        wallet,
        taskPda: taskKey,
      }).fee_payer,
    ).toBe(wallet);

    const atomicSubmitWithPayoutB64 = new Transaction({
      feePayer: kp.publicKey,
      recentBlockhash: blockhash,
    })
      .add(claimIx, payoutIx, submitIx)
      .serialize({ requireAllSignatures: false, verifySignatures: false })
      .toString("base64");
    // Without explicit expected.payoutTo, set_payout_to companion MUST fail closed (HIGH-02)
    expect(() =>
      validatePreparedTransaction(atomicSubmitWithPayoutB64, {
        action: "submit",
        network: "mainnet",
        wallet,
        taskPda: taskKey,
      }),
    ).toThrow(/Unexpected set_payout_to instruction/);
    expect(() =>
      validatePreparedTransaction(atomicSubmitWithPayoutB64, {
        action: "submit",
        network: "mainnet",
        wallet,
        taskPda: taskKey,
        payoutTo: Keypair.generate().publicKey.toBase58(),
      }),
    ).toThrow(/destination pubkey/);
    expect(
      validatePreparedTransaction(atomicSubmitWithPayoutB64, {
        action: "submit",
        network: "mainnet",
        wallet,
        taskPda: taskKey,
        payoutTo: payoutKey.toBase58(),
      }).fee_payer,
    ).toBe(wallet);

    // Reversed [submit_work, claim_task] MUST fail closed
    const reversedB64 = new Transaction({ feePayer: kp.publicKey, recentBlockhash: blockhash })
      .add(submitIx, claimIx)
      .serialize({ requireAllSignatures: false, verifySignatures: false })
      .toString("base64");
    expect(() =>
      validatePreparedTransaction(reversedB64, {
        action: "submit",
        network: "mainnet",
        wallet,
        taskPda: taskKey,
      }),
    ).toThrow(/Expected single Shillbot|Expected exactly one matching/);

    // Mismatched claim_task taskPda MUST fail closed
    const wrongClaimIx = buildInstruction({
      action: "claim",
      wallet,
      network: "mainnet",
      recentBlockhash: blockhash,
      taskPda: otherTaskKey,
    });
    const mismatchedTaskB64 = new Transaction({
      feePayer: kp.publicKey,
      recentBlockhash: blockhash,
    })
      .add(wrongClaimIx, submitIx)
      .serialize({ requireAllSignatures: false, verifySignatures: false })
      .toString("base64");
    expect(() =>
      validatePreparedTransaction(mismatchedTaskB64, {
        action: "submit",
        network: "mainnet",
        wallet,
        taskPda: taskKey,
      }),
    ).toThrow(/Expected single Shillbot|Expected exactly one matching/);

    // Standalone Python verifier also accepts atomic [claim_task, submit_work] and enforces --payout-to
    const pyScript = resolve(repoRoot, "examples/python-safe-recipe/verify_shillbot_tx.py");
    const pyAtomicOk = spawnSync(
      "python3",
      [pyScript, "--action", "submit", "--wallet", wallet, "--task-pda", taskKey],
      { input: atomicSubmitB64, encoding: "utf8" },
    );
    expect(pyAtomicOk.status).toBe(0);
    const pyPayoutNoFlag = spawnSync(
      "python3",
      [pyScript, "--action", "submit", "--wallet", wallet, "--task-pda", taskKey],
      { input: atomicSubmitWithPayoutB64, encoding: "utf8" },
    );
    expect(pyPayoutNoFlag.status).toBe(1);
    const pyPayoutWithFlag = spawnSync(
      "python3",
      [
        pyScript,
        "--action",
        "submit",
        "--wallet",
        wallet,
        "--task-pda",
        taskKey,
        "--payout-to",
        payoutKey.toBase58(),
      ],
      { input: atomicSubmitWithPayoutB64, encoding: "utf8" },
    );
    expect(pyPayoutWithFlag.status).toBe(0);
    const pyReversedBad = spawnSync(
      "python3",
      [pyScript, "--action", "submit", "--wallet", wallet, "--task-pda", taskKey],
      { input: reversedB64, encoding: "utf8" },
    );
    expect(pyReversedBad.status).toBe(1);
  });
});

describe("Phase 5.1 Security Hardening (HIGH-02, HIGH-03, HIGH-37, MED-09)", () => {
  it("HIGH-02: rejects set_payout_to hijack unless expected.payoutTo matches exact accounts and data[8..40]", () => {
    const agentKp = Keypair.generate();
    const attackerKp = Keypair.generate();
    const payoutKp = Keypair.generate();
    const taskPubkey = Keypair.generate().publicKey;
    const wallet = agentKp.publicKey.toBase58();
    const taskPda = taskPubkey.toBase58();
    const payoutTo = payoutKp.publicKey.toBase58();
    const blockhash = "11111111111111111111111111111111";

    const claimIx = buildInstruction({
      action: "claim",
      wallet,
      network: "mainnet",
      recentBlockhash: blockhash,
      taskPda,
    });
    const hijackPayoutIx = buildShillbotInstruction({
      name: "set_payout_to",
      accounts: { task: taskPda, agent: wallet },
      args: { payout_to: attackerKp.publicKey },
    });
    const legitPayoutIx = buildShillbotInstruction({
      name: "set_payout_to",
      accounts: { task: taskPda, agent: wallet },
      args: { payout_to: payoutKp.publicKey },
    });

    const serializeTx = (...ixs: TransactionInstruction[]) =>
      new Transaction({ feePayer: agentKp.publicKey, recentBlockhash: blockhash })
        .add(...ixs)
        .serialize({ requireAllSignatures: false, verifySignatures: false })
        .toString("base64");

    // 1. Unexpected set_payout_to in claim without expected.payoutTo -> rejected
    const claimWithHijackB64 = serializeTx(claimIx, hijackPayoutIx);
    expect(() =>
      verifyBuiltTransaction(claimWithHijackB64, {
        action: "claim",
        network: "mainnet",
        wallet,
        taskPda,
      }),
    ).toThrow(/Unexpected set_payout_to instruction/);

    // 2. expected.payoutTo provided, but ix.data[8..40] routes to attacker -> rejected
    expect(() =>
      verifyBuiltTransaction(claimWithHijackB64, {
        action: "claim",
        network: "mainnet",
        wallet,
        taskPda,
        payoutTo,
      }),
    ).toThrow(/destination pubkey/);

    // 3. Valid [claim_task, set_payout_to] with matching expected.payoutTo -> accepted
    const claimWithLegitPayoutB64 = serializeTx(claimIx, legitPayoutIx);
    expect(
      verifyBuiltTransaction(claimWithLegitPayoutB64, {
        action: "claim",
        network: "mainnet",
        wallet,
        taskPda,
        payoutTo,
      }).fee_payer,
    ).toBe(wallet);

    // 4. Caller requested expected.payoutTo on claim, but server omitted set_payout_to -> rejected
    const plainClaimB64 = serializeTx(claimIx);
    expect(() =>
      verifyBuiltTransaction(plainClaimB64, {
        action: "claim",
        network: "mainnet",
        wallet,
        taskPda,
        payoutTo,
      }),
    ).toThrow(/Expected set_payout_to instruction matching expected\.payoutTo/);

    // 5. set_payout_to with wrong authority slot [1] (attacker signs [1], wallet is feePayer) -> rejected
    const wrongAuthPayoutIx = new TransactionInstruction({
      programId: SHILLBOT_PROGRAM_ID,
      keys: [
        { pubkey: taskPubkey, isSigner: false, isWritable: true },
        { pubkey: attackerKp.publicKey, isSigner: true, isWritable: false },
      ],
      data: Buffer.concat([
        shillbotInstructionDiscriminator("set_payout_to"),
        payoutKp.publicKey.toBuffer(),
      ]),
    });
    expect(() =>
      verifyBuiltTransaction(serializeTx(claimIx, wrongAuthPayoutIx), {
        action: "claim",
        network: "mainnet",
        wallet,
        taskPda,
        payoutTo,
      }),
    ).toThrow(/authority slot \[1\]/);

    // 6. set_payout_to with extra trailing account (3 accounts instead of exact 2) -> rejected
    const extraAcctPayoutIx = new TransactionInstruction({
      programId: SHILLBOT_PROGRAM_ID,
      keys: [
        { pubkey: taskPubkey, isSigner: false, isWritable: true },
        { pubkey: agentKp.publicKey, isSigner: true, isWritable: false },
        { pubkey: attackerKp.publicKey, isSigner: false, isWritable: false },
      ],
      data: Buffer.concat([
        shillbotInstructionDiscriminator("set_payout_to"),
        payoutKp.publicKey.toBuffer(),
      ]),
    });
    expect(() =>
      verifyBuiltTransaction(serializeTx(claimIx, extraAcctPayoutIx), {
        action: "claim",
        network: "mainnet",
        wallet,
        taskPda,
        payoutTo,
      }),
    ).toThrow(/exact 2 accounts/);

    // 7. Zero pubkey or taskPda as payoutTo -> rejected
    expect(() =>
      verifyBuiltTransaction(claimWithLegitPayoutB64, {
        action: "claim",
        network: "mainnet",
        wallet,
        taskPda,
        payoutTo: SystemProgram.programId.toBase58(),
      }),
    ).toThrow(/zero pubkey/);
    expect(() =>
      verifyBuiltTransaction(claimWithLegitPayoutB64, {
        action: "claim",
        network: "mainnet",
        wallet,
        taskPda,
        payoutTo: taskPda,
      }),
    ).toThrow(/cannot equal the task PDA/);

    // 8. set_payout_to sidecar injected into finalize_task -> always rejected
    const finalizeBuilt = buildFinalize({
      wallet,
      network: "mainnet",
      recentBlockhash: blockhash,
      taskPda,
      agent: payoutTo,
      client: Keypair.generate().publicKey.toBase58(),
      treasury: Keypair.generate().publicKey.toBase58(),
    });
    const finalizeTx = Transaction.from(Buffer.from(finalizeBuilt.unsigned_tx, "base64"));
    finalizeTx.add(legitPayoutIx);
    const finalizeWithSidecarB64 = finalizeTx
      .serialize({ requireAllSignatures: false, verifySignatures: false })
      .toString("base64");
    expect(() =>
      verifyBuiltTransaction(finalizeWithSidecarB64, {
        action: "finalize",
        network: "mainnet",
        wallet,
        taskPda,
        payoutTo,
      }),
    ).toThrow(/Unexpected set_payout_to/);
  });

  it("HIGH-03: validates all ComputeBudget instructions without RequestHeapFrame (tag 1) early-return bypass", () => {
    const kp = Keypair.generate();
    const wallet = kp.publicKey.toBase58();
    const taskPda = Keypair.generate().publicKey.toBase58();
    const blockhash = "11111111111111111111111111111111";

    const claimIx = buildInstruction({
      action: "claim",
      wallet,
      network: "mainnet",
      recentBlockhash: blockhash,
      taskPda,
    });

    // RequestHeapFrame (tag 1) followed by excessive SetComputeUnitPrice (500,000 > 100,000 cap)
    const bypassB64 = new Transaction({ feePayer: kp.publicKey, recentBlockhash: blockhash })
      .add(
        ComputeBudgetProgram.requestHeapFrame({ bytes: 65_536 }),
        ComputeBudgetProgram.setComputeUnitLimit({ units: 200_000 }),
        ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 500_000 }),
        claimIx,
      )
      .serialize({ requireAllSignatures: false, verifySignatures: false })
      .toString("base64");

    expect(MAX_COMPUTE_UNIT_PRICE_MICROLAMPORTS).toBe(100_000n);
    expect(() =>
      verifyBuiltTransaction(bypassB64, {
        action: "claim",
        network: "mainnet",
        wallet,
        taskPda,
      }),
    ).toThrow(/SetComputeUnitPrice 500000.*exceeds maximum allowed 100000/);

    expect(() =>
      validatePreparedTransaction(bypassB64, {
        action: "claim",
        network: "mainnet",
        wallet,
        taskPda,
      }),
    ).toThrow(/SetComputeUnitPrice 500000.*exceeds maximum allowed 100000/);

    const claimBuilt = buildClaim({
      wallet,
      network: "mainnet",
      recentBlockhash: blockhash,
      taskPda,
    });
    expect(() => verifyIntent(bypassB64, claimBuilt.transaction_intent)).toThrow(
      /SetComputeUnitPrice 500000.*exceeds maximum allowed 100000/,
    );

    // Fee estimator MUST NOT short-circuit on RequestHeapFrame (tag 1):
    // base fee (1 sig * 5000) + priority fee (200_000 CU * 500_000 microLamports / 1_000_000 = 100_000) = 105_000
    expect(estimateSolanaNetworkFeeLamports(inspectTransaction(bypassB64), wallet)).toBe(105_000n);

    // Even if tag 1 is malformed with 9 bytes followed by tag 3, fee estimator still processes tag 3
    // and validator rejects malformed tag 1
    const malformedHeapData = Buffer.alloc(9);
    malformedHeapData[0] = 1;
    const malformedHeapIx = new TransactionInstruction({
      programId: ComputeBudgetProgram.programId,
      keys: [],
      data: malformedHeapData,
    });
    const malformedHeapTxB64 = new Transaction({
      feePayer: kp.publicKey,
      recentBlockhash: blockhash,
    })
      .add(
        malformedHeapIx,
        ComputeBudgetProgram.setComputeUnitLimit({ units: 200_000 }),
        ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 200_000 }),
        claimIx,
      )
      .serialize({ requireAllSignatures: false, verifySignatures: false })
      .toString("base64");
    expect(estimateSolanaNetworkFeeLamports(inspectTransaction(malformedHeapTxB64), wallet)).toBe(45_000n);
    expect(() => validateComputeBudgetInstructions(inspectTransaction(malformedHeapTxB64).instructions)).toThrow(
      /RequestHeapFrame length/,
    );

    // Valid RequestHeapFrame + bounded SetComputeUnitLimit + bounded SetComputeUnitPrice succeeds
    const validCbB64 = new Transaction({ feePayer: kp.publicKey, recentBlockhash: blockhash })
      .add(
        ComputeBudgetProgram.requestHeapFrame({ bytes: 65_536 }),
        ComputeBudgetProgram.setComputeUnitLimit({ units: 250_000 }),
        ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 80_000 }),
        claimIx,
      )
      .serialize({ requireAllSignatures: false, verifySignatures: false })
      .toString("base64");
    expect(
      verifyBuiltTransaction(validCbB64, {
        action: "claim",
        network: "mainnet",
        wallet,
        taskPda,
      }).fee_payer,
    ).toBe(wallet);
  });

  it("HIGH-03: enforces exact Anchor authority account slot indices even when wallet is in signers", () => {
    const victimKp = Keypair.generate();
    const attackerKp = Keypair.generate();
    const sponsorKp = Keypair.generate();
    const victim = victimKp.publicKey.toBase58();
    const attacker = attackerKp.publicKey.toBase58();
    const sponsor = sponsorKp.publicKey.toBase58();
    const taskPubkey = Keypair.generate().publicKey;
    const taskPda = taskPubkey.toBase58();
    const dummy1 = Keypair.generate().publicKey;
    const dummy2 = Keypair.generate().publicKey;
    const dummy3 = Keypair.generate().publicKey;
    const blockhash = "11111111111111111111111111111111";

    const serializeWithVictimFeePayer = (ix: TransactionInstruction) =>
      new Transaction({ feePayer: victimKp.publicKey, recentBlockhash: blockhash })
        .add(ix)
        .serialize({ requireAllSignatures: false, verifySignatures: false })
        .toString("base64");

    // 1. claim_task: attacker in [3], victim is feePayer (so victim IS in signers) -> MUST reject!
    const badClaimIx = buildInstruction({
      action: "claim",
      wallet: attacker,
      network: "mainnet",
      recentBlockhash: blockhash,
      taskPda,
    });
    expect(() =>
      verifyBuiltTransaction(serializeWithVictimFeePayer(badClaimIx), {
        action: "claim",
        network: "mainnet",
        wallet: victim,
        taskPda,
      }),
    ).toThrow(/claim_task authority slot \[3\]/);

    // 2. claim_task_session: attacker at delegate [4], victim at payer [5] & feePayer -> MUST reject!
    const badClaimSessionIx = new TransactionInstruction({
      programId: SHILLBOT_PROGRAM_ID,
      keys: [
        { pubkey: taskPubkey, isSigner: false, isWritable: true },
        { pubkey: dummy1, isSigner: false, isWritable: false },
        { pubkey: dummy2, isSigner: false, isWritable: true },
        { pubkey: dummy3, isSigner: false, isWritable: false }, // [3] agent_wallet
        { pubkey: attackerKp.publicKey, isSigner: true, isWritable: false }, // [4] delegate
        { pubkey: victimKp.publicKey, isSigner: true, isWritable: true }, // [5] payer
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      ],
      data: shillbotInstructionDiscriminator("claim_task_session"),
    });
    expect(() =>
      verifyBuiltTransaction(serializeWithVictimFeePayer(badClaimSessionIx), {
        action: "claim",
        network: "mainnet",
        wallet: victim,
        taskPda,
      }),
    ).toThrow(/claim_task_session authority slot \[4\]/);

    // Positive claim_task_session: victim is delegate [4], sponsor is feePayer & payer [5]
    const goodClaimSessionIx = new TransactionInstruction({
      programId: SHILLBOT_PROGRAM_ID,
      keys: [
        { pubkey: taskPubkey, isSigner: false, isWritable: true },
        { pubkey: dummy1, isSigner: false, isWritable: false },
        { pubkey: dummy2, isSigner: false, isWritable: true },
        { pubkey: dummy3, isSigner: false, isWritable: false },
        { pubkey: victimKp.publicKey, isSigner: true, isWritable: false }, // [4] delegate = victim
        { pubkey: sponsorKp.publicKey, isSigner: true, isWritable: true }, // [5] payer = sponsor
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      ],
      data: shillbotInstructionDiscriminator("claim_task_session"),
    });
    const goodSessionTxB64 = new Transaction({
      feePayer: sponsorKp.publicKey,
      recentBlockhash: blockhash,
    })
      .add(goodClaimSessionIx)
      .serialize({ requireAllSignatures: false, verifySignatures: false })
      .toString("base64");
    expect(
      verifyBuiltTransaction(goodSessionTxB64, {
        action: "claim",
        network: "mainnet",
        wallet: victim,
        feePayer: sponsor,
        taskPda,
      }).fee_payer,
    ).toBe(sponsor);

    // 3. submit_work: attacker in [3], victim is feePayer -> MUST reject!
    const badSubmitIx = buildInstruction({
      action: "submit",
      wallet: attacker,
      network: "mainnet",
      recentBlockhash: blockhash,
      taskPda,
      contentId: "proof-123",
    });
    expect(() =>
      verifyBuiltTransaction(serializeWithVictimFeePayer(badSubmitIx), {
        action: "submit",
        network: "mainnet",
        wallet: victim,
        taskPda,
      }),
    ).toThrow(/submit_work authority slot \[3\]/);

    // 4. submit_work_session: attacker in [4], victim is feePayer -> MUST reject!
    const contentBuf = Buffer.from("proof-123", "utf8");
    const lenBuf = Buffer.alloc(4);
    lenBuf.writeUInt32LE(contentBuf.length, 0);
    const badSubmitSessionIx = new TransactionInstruction({
      programId: SHILLBOT_PROGRAM_ID,
      keys: [
        { pubkey: taskPubkey, isSigner: false, isWritable: true },
        { pubkey: dummy1, isSigner: false, isWritable: false },
        { pubkey: dummy2, isSigner: false, isWritable: true },
        { pubkey: dummy3, isSigner: false, isWritable: false },
        { pubkey: attackerKp.publicKey, isSigner: true, isWritable: false }, // [4] delegate = attacker
      ],
      data: Buffer.concat([
        shillbotInstructionDiscriminator("submit_work_session"),
        lenBuf,
        contentBuf,
      ]),
    });
    expect(() =>
      verifyBuiltTransaction(serializeWithVictimFeePayer(badSubmitSessionIx), {
        action: "submit",
        network: "mainnet",
        wallet: victim,
        taskPda,
      }),
    ).toThrow(/submit_work_session authority slot \[4\]/);

    // 5. create_task: attacker in client slot [3], victim is feePayer -> MUST reject!
    const badCreateBuilt = buildCreate({
      wallet: attacker,
      network: "mainnet",
      recentBlockhash: blockhash,
      nonce: "1",
      escrowLamports: "10000000",
      contentHash: "aa".repeat(32),
      deadline: "1900000000",
      submitMargin: "0",
      claimBuffer: "0",
      platform: 1,
      attestationDelayOverride: 0,
      challengeWindowOverride: 0,
      verificationTimeoutOverride: 0,
      requiresApproval: false,
      verificationKind: 1,
    });
    const badCreateOrigTx = Transaction.from(Buffer.from(badCreateBuilt.unsigned_tx, "base64"));
    const badCreateWithVictimPayer = new Transaction({
      feePayer: victimKp.publicKey,
      recentBlockhash: blockhash,
    })
      .add(...badCreateOrigTx.instructions)
      .serialize({ requireAllSignatures: false, verifySignatures: false })
      .toString("base64");
    expect(() =>
      verifyBuiltTransaction(badCreateWithVictimPayer, {
        action: "create",
        network: "mainnet",
        wallet: victim,
      }),
    ).toThrow(/create_task authority slot \[3\]/);

    // 6. approve_task: attacker in client slot [1], victim is feePayer -> MUST reject!
    const badApproveIx = buildInstruction({
      action: "approve",
      wallet: attacker,
      network: "mainnet",
      recentBlockhash: blockhash,
      taskPda,
    });
    expect(() =>
      verifyBuiltTransaction(serializeWithVictimFeePayer(badApproveIx), {
        action: "approve",
        network: "mainnet",
        wallet: victim,
        taskPda,
      }),
    ).toThrow(/approve_task authority slot \[1\]/);

    // 7. verify_task_attested: unsigned attester [2] or wrong oracleAuthority -> MUST reject!
    const attestedData = Buffer.alloc(48);
    shillbotInstructionDiscriminator("verify_task_attested").copy(attestedData, 0);
    attestedData.writeBigUInt64LE(9_500n, 8);
    const unsignedAttestedIx = new TransactionInstruction({
      programId: SHILLBOT_PROGRAM_ID,
      keys: [
        { pubkey: taskPubkey, isSigner: false, isWritable: true },
        { pubkey: dummy1, isSigner: false, isWritable: false },
        { pubkey: attackerKp.publicKey, isSigner: false, isWritable: false }, // not a signer!
      ],
      data: attestedData,
    });
    expect(() =>
      verifyBuiltTransaction(serializeWithVictimFeePayer(unsignedAttestedIx), {
        action: "verify",
        network: "mainnet",
        wallet: victim,
        taskPda,
      }),
    ).toThrow(/attester slot \[2\].*must be a required signer/);
  });

  it("HIGH-03: extractCreateEscrowLamports filters strictly by SHILLBOT_PROGRAM_ID and CREATE_TASK_DISCRIMINATOR", () => {
    const kp = Keypair.generate();
    const wallet = kp.publicKey.toBase58();
    const taskPda = Keypair.generate().publicKey.toBase58();
    const blockhash = "11111111111111111111111111111111";

    // A submit_work instruction with long contentId (>= 24 bytes) has NO create_task discriminator
    const longSubmitIx = buildInstruction({
      action: "submit",
      wallet,
      network: "mainnet",
      recentBlockhash: blockhash,
      taskPda,
      contentId: "https://example.com/very-long-submission-content-id-over-24-bytes",
    });
    const submitOnlyB64 = new Transaction({ feePayer: kp.publicKey, recentBlockhash: blockhash })
      .add(longSubmitIx)
      .serialize({ requireAllSignatures: false, verifySignatures: false })
      .toString("base64");
    expect(extractCreateEscrowLamports(submitOnlyB64)).toBeUndefined();

    // Non-Shillbot instruction (>= 24 bytes) before create_task MUST be ignored so real escrow is read
    const decoyProgram = Keypair.generate().publicKey;
    const decoyData = Buffer.alloc(32);
    decoyData.writeBigUInt64LE(1n, 16); // decoy says "1 lamport"!
    const decoyIx = new TransactionInstruction({
      programId: decoyProgram,
      keys: [],
      data: decoyData,
    });

    const realCreate = buildCreate({
      wallet,
      network: "mainnet",
      recentBlockhash: blockhash,
      nonce: "77",
      escrowLamports: "250000000",
      contentHash: "bb".repeat(32),
      deadline: "1900000000",
      submitMargin: "0",
      claimBuffer: "0",
      platform: 1,
      attestationDelayOverride: 0,
      challengeWindowOverride: 0,
      verificationTimeoutOverride: 0,
      requiresApproval: false,
      verificationKind: 1,
    });
    const realCreateTx = Transaction.from(Buffer.from(realCreate.unsigned_tx, "base64"));
    const combinedB64 = new Transaction({ feePayer: kp.publicKey, recentBlockhash: blockhash })
      .add(decoyIx, ...realCreateTx.instructions)
      .serialize({ requireAllSignatures: false, verifySignatures: false })
      .toString("base64");

    expect(extractCreateEscrowLamports(combinedB64)).toBe(250_000_000n);
  });

  it("HIGH-37: executeEvmAction rejects chain mismatch on EvmSwarmSigner and forwards chain + maxFeePerGas ceiling", async () => {
    const evmPrivKey =
      "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80" as const;
    const baseAccount = privateKeyToAccount(evmPrivKey);
    const baseSepoliaContract = SHILLBOT_ESCROW_DEPLOYMENTS["eip155:84532"];
    const claimCalldata = `${SHILLBOT_EVM_SELECTORS.claim}${"0".repeat(63)}1` as const;

    const preparedClaimFetch = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.includes("/tasks/evm-1/claim")) {
        return jsonResponse({
          transaction: "",
          evm_call: {
            chain: "eip155:84532",
            to: baseSepoliaContract,
            data: claimCalldata,
            value_wei: "0",
          },
        });
      }
      if (url.includes("/tasks/evm-1/confirm")) {
        return jsonResponse({ ok: true });
      }
      throw new Error(`Unexpected fetch URL: ${url}`);
    });

    // 1. Signer pinned to Base Mainnet (8453) MUST fail closed against Base Sepolia (84532) call
    const sendSpyWrongChain = vi.fn(async () => `0x${"11".repeat(32)}` as `0x${string}`);
    const wrongChainSigner: EvmSwarmSigner = {
      chain: "evm",
      address: baseAccount.address,
      chainId: 8453,
      signMessage: async (m) => await baseAccount.signMessage({ message: typeof m === "string" ? m : { raw: m } }),
      sendTransaction: sendSpyWrongChain,
    };
    const wrongChainAgent = new SwarmAgent({
      signer: wrongChainSigner,
      token: makeJwt(baseAccount.address, Math.floor(Date.now() / 1000) + 3600),
      fetch: preparedClaimFetch as unknown as typeof fetch,
    });
    await expect(wrongChainAgent.tasks.claimAndExecute("evm-1")).rejects.toMatchObject({
      code: "TRANSACTION_MISMATCH",
    });
    expect(sendSpyWrongChain).not.toHaveBeenCalled();

    // 2. Signer pinned to CAIP-2 "eip155:84532" succeeds and receives chain, chainId, and maxFeePerGas ceiling
    const sendSpyMatching = vi.fn(async () => `0x${"22".repeat(32)}` as `0x${string}`);
    const matchingSigner: EvmSwarmSigner = {
      chain: "evm",
      address: baseAccount.address,
      chainId: "eip155:84532",
      signMessage: async (m) => await baseAccount.signMessage({ message: typeof m === "string" ? m : { raw: m } }),
      sendTransaction: sendSpyMatching,
    };
    const customFeeCap = 12_345_000_000n;
    const okAgent = new SwarmAgent({
      signer: matchingSigner,
      token: makeJwt(baseAccount.address, Math.floor(Date.now() / 1000) + 3600),
      maxNetworkFeeWei: customFeeCap,
      fetch: preparedClaimFetch as unknown as typeof fetch,
    });
    expect(DEFAULT_MAX_NETWORK_FEE_WEI).toBe(50_000_000_000n);
    const res = await okAgent.tasks.claimAndExecute("evm-1");
    expect(res.signature).toBe(`0x${"22".repeat(32)}`);
    expect(sendSpyMatching).toHaveBeenCalledTimes(1);
    expect(sendSpyMatching).toHaveBeenCalledWith({
      to: baseSepoliaContract,
      data: claimCalldata,
      value: 0n,
      chain: "eip155:84532",
      chainId: 84532,
      maxFeePerGas: customFeeCap,
    });
  });

  it("MED-09: verifyVow blocks SSRF hosts/IPs, trailing-dot FQDNs, redirects, and streams with a 2 MiB cap", async () => {
    expect(DEFAULT_MAX_VOW_BYTES).toBe(2 * 1024 * 1024);

    // 1. SSRF URL blocklist assertions
    const blockedUrls = [
      "http://example.com/vow.json",
      "ftp://example.com/vow.json",
      "https://user:pass@example.com/vow.json",
      "https://example.com./vow.json",
      "https://metadata.google.internal/computeMetadata/v1/",
      "https://metadata.google.internal./computeMetadata/v1/",
      "https://svc.cluster.local/vow.json",
      "https://my-app-xyz.a.run.app/vow.json",
      "https://169.254.169.254.nip.io/vow.json",
      "https://10.0.0.1.sslip.io/vow.json",
      "https://127.0.0.1.xip.io/vow.json",
      "https://localhost/vow.json",
      "https://sub.localhost/vow.json",
      "https://169.254.169.254/latest/meta-data/",
      "https://10.0.0.42/vow.json",
      "https://172.16.5.4/vow.json",
      "https://192.168.1.10/vow.json",
      "https://100.64.0.1/vow.json",
      "https://127.0.0.1/vow.json",
      "https://0.0.0.0/vow.json",
      "https://[::1]/vow.json",
      "https://[fc00::1]/vow.json",
      "https://[fd12:3456::1]/vow.json",
      "https://[fe80::1]/vow.json",
      "https://[::ffff:169.254.169.254]/vow.json",
      "https://[64:ff9b::10.0.0.1]/vow.json",
    ];
    for (const badUrl of blockedUrls) {
      expect(() => validateVowUrl(badUrl), `Expected ${badUrl} to be rejected`).toThrow(
        SwarmClientError,
      );
    }

    // Explicit localhost HTTP opt-in works ONLY for loopback when allowLocalhostHttp: true
    expect(
      validateVowUrl("http://127.0.0.1:8787/vow.json", { allowLocalhostHttp: true }).href,
    ).toBe("http://127.0.0.1:8787/vow.json");
    expect(() =>
      validateVowUrl("http://169.254.169.254/vow.json", { allowLocalhostHttp: true }),
    ).toThrow(SwarmClientError);

    // 2. Redirects are disabled (`redirect: "error"`) and 3xx responses fail closed
    const redirectFetch = vi.fn(async (_url: RequestInfo | URL, init?: RequestInit) => {
      expect(init?.redirect).toBe("error");
      return new Response("", {
        status: 302,
        headers: { Location: "http://169.254.169.254/latest/meta-data/" },
      });
    });
    await expect(
      verifyVow("https://attestations.swarm.tips/vow-1.json", {
        fetch: redirectFetch as unknown as typeof fetch,
      }),
    ).rejects.toMatchObject({ code: "INVALID_RESPONSE" });
    expect(redirectFetch).toHaveBeenCalledTimes(1);

    // 3. Streaming reader enforces 2 MiB cap mid-stream even without Content-Length header
    let chunksPulled = 0;
    let streamCancelled = false;
    const infiniteChunkStream = new ReadableStream<Uint8Array>({
      pull(controller) {
        chunksPulled += 1;
        // 512 KiB per chunk -> exceeds 2 MiB on chunk 5!
        controller.enqueue(new Uint8Array(512 * 1024).fill(0x61));
      },
      cancel() {
        streamCancelled = true;
      },
    });
    const oversizedFetch = vi.fn(
      async () =>
        new Response(infiniteChunkStream, {
          status: 200,
          headers: { "Content-Type": "application/json" },
        }),
    );
    await expect(
      fetchVowAttestationJson("https://attestations.swarm.tips/oversized.json", {
        fetch: oversizedFetch as unknown as typeof fetch,
      }),
    ).rejects.toThrow(/exceeded maxBytes \(2097152 bytes\)/);
    expect(chunksPulled).toBeLessThanOrEqual(6);
    expect(streamCancelled).toBe(true);
  });

  it("MED-09: supports 'town-square' in TopicId / VALID_TOPICS and strips only leading '--' in parseCliArgs", async () => {
    expect(VALID_TOPICS).toEqual(["open-challenge", "subcontract", "town-square"]);
    expect(isValidTopicId("town-square")).toBe(true);
    expect(isValidTopicId("invalid-room")).toBe(false);

    // parseCliArgs strips leading standalone "--" (e.g. `pnpm swarm -- --version --json`)
    const leadingDash = parseCliArgs(["--", "--version", "--json"]);
    expect(leadingDash.positionals).toEqual([]);
    expect(leadingDash.flags.get("--version")).toBe(true);
    expect(leadingDash.flags.get("--json")).toBe(true);

    const topicCmd = parseCliArgs(["--", "topics", "read", "town-square", "--limit", "5"]);
    expect(topicCmd.positionals).toEqual(["topics", "read", "town-square"]);
    expect(topicCmd.flags.get("--limit")).toBe("5");

    // Flag whose value is "--" is preserved intact, and end-of-options "--" after positionals stops flag parsing
    const literalValue = parseCliArgs(["tasks", "submit", "t-1", "--content", "--"]);
    expect(literalValue.positionals).toEqual(["tasks", "submit", "t-1"]);
    expect(literalValue.flags.get("--content")).toBe("--");

    const endOfOpts = parseCliArgs(["inbox", "send", "wallet1", "--", "--not-a-flag"]);
    expect(endOfOpts.positionals).toEqual(["inbox", "send", "wallet1", "--not-a-flag"]);

    // Secret CLI flags after leading "--" are STILL rejected without echoing secret material
    expect(() => parseCliArgs(["--", "--private-key", "TOP_SECRET_VALUE_999"])).toThrow(
      /--private-key/,
    );
    try {
      parseCliArgs(["--", "--private-key", "TOP_SECRET_VALUE_999"]);
    } catch (err) {
      expect(String(err)).not.toContain("TOP_SECRET_VALUE_999");
    }

    // CLI `swarm -- topics read town-square` and `swarm -- topics publish town-square "gm"` succeed end-to-end
    const kp = Keypair.generate();
    const wallet = kp.publicKey.toBase58();
    const jwt = makeJwt(wallet, Math.floor(Date.now() / 1000) + 3600);
    const seenUrls: string[] = [];
    const mockFetch = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      seenUrls.push(`${init?.method ?? "GET"} ${url}`);
      if (url.includes("/internal/topics/publish")) {
        const parsedBody = JSON.parse(String(init?.body ?? "{}")) as { topic_id?: string };
        expect(parsedBody.topic_id).toBe("town-square");
        return jsonResponse({ published: true, post_id: "post-ts-1" });
      }
      if (url.includes("/internal/topics/read")) {
        expect(url).toContain("topic_id=town-square");
        return jsonResponse({ topic_id: "town-square", posts: [], next_cursor: null });
      }
      throw new Error(`Unexpected URL: ${url}`);
    });

    const outRead: string[] = [];
    const codeRead = await runSwarmCli(["--", "topics", "read", "town-square", "--limit", "10", "--json"], {
      stdout: { write: (c) => (outRead.push(c), true), isTTY: false },
      stderr: { write: () => true },
      env: { SWARM_AUTH_TOKEN: jwt },
      fetch: mockFetch as unknown as typeof fetch,
    });
    expect(codeRead).toBe(0);
    expect(JSON.parse(outRead.join(""))).toMatchObject({ topic_id: "town-square" });

    const outPub: string[] = [];
    const codePub = await runSwarmCli(["--", "topics", "publish", "town-square", "hello swarm", "--json"], {
      stdout: { write: (c) => (outPub.push(c), true), isTTY: false },
      stderr: { write: () => true },
      env: { SWARM_AUTH_TOKEN: jwt },
      fetch: mockFetch as unknown as typeof fetch,
    });
    expect(codePub).toBe(0);
    expect(JSON.parse(outPub.join(""))).toMatchObject({ published: true, post_id: "post-ts-1" });
    expect(seenUrls.some((u) => u.startsWith("GET ") && u.includes("/internal/topics/read"))).toBe(true);
    expect(seenUrls.some((u) => u.startsWith("POST ") && u.includes("/internal/topics/publish"))).toBe(true);

    // InboxClient direct call also accepts "town-square"
    const inbox = new InboxClient({
      baseUrl: "https://mcp.swarm.tips",
      getToken: () => jwt,
      fetchFn: mockFetch as unknown as typeof fetch,
    });
    const tsFeed = await inbox.readTopic("town-square");
    expect(tsFeed.topic_id).toBe("town-square");
  });
});
