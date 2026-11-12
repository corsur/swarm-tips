/**
 * Seed flagship permissionless `PlatformType::OnChainAction` (`platform=11`)
 * reference campaigns onto Devnet or Mainnet (`ONCHAIN_ACTION_CONSENSUS_2026-11-10.md`).
 *
 * Showcases both supported target namespaces with strictly Sybil-safe
 * (`Cost_to_Trigger > Net_Payout`) live contracts:
 *   1. Cross-chain EVM (`eip155:8453` Base Mainnet `CoordinationGameV4`
 *      `GameStarted(bytes32,address,address)` -> Solana settlement, `0.012 SOL`).
 *   2. Solana-native (`solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp` `coordination-game`
 *      `reveal_guess` `InstructionData` + `AccountIndex`, `0.008 SOL`).
 *
 * Usage:
 *   npx tsx scripts/e2e/seed-onchain-action-showcase.ts --check-only
 *   npx tsx scripts/e2e/seed-onchain-action-showcase.ts --mainnet
 */
import * as crypto from "crypto";
import { execSync } from "child_process";
import {
  Connection,
  Keypair,
  LAMPORTS_PER_SOL,
  Transaction,
  VersionedTransaction,
} from "@solana/web3.js";
import { loadKeypair } from "./devnet-harness";

const API_BASE = process.env.SHILLBOT_API ?? "https://api.shillbot.org";
const ONCHAIN_ACTION_PLATFORM = 11;
const MIN_CAMPAIGN_ESCROW_LAMPORTS = 5_000_000;

export interface OnChainShowcaseEntry {
  topic: string;
  cta: string;
  utmLink: string;
  escrowLamports: number;
  spec: Record<string, unknown>;
}

export function anchorInstructionDiscriminatorHex(ixName: string): string {
  const digest = crypto
    .createHash("sha256")
    .update(`global:${ixName}`)
    .digest();
  return `0x${digest.subarray(0, 8).toString("hex")}`;
}

export function buildShowcaseEntries(
  isMainnet: boolean
): OnChainShowcaseEntry[] {
  const revealGuessDiscHex = anchorInstructionDiscriminatorHex("reveal_guess");
  const solanaChain = isMainnet
    ? "solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp"
    : "solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1";
  const baseChain = isMainnet ? "eip155:8453" : "eip155:84532";

  return [
    {
      topic: "onchain-action:base-coordination-game-match",
      cta: "Start a Base CoordinationGameV4 match as Player 1 against an independent Player 2 (`GameStarted(bytes32,address,address)`, submit `eip155:8453:<tx_hash>:<log_index>:sig:<eip191_sig>`)",
      utmLink: "https://coordination.game",
      escrowLamports: 12_000_000,
      spec: {
        schema_version: 1,
        target_chain: baseChain,
        action_mode: "affiliate_event",
        predicate: {
          kind: "evm_log",
          target_contract: "0xd585bae48901513202daeb7d4fee4af508a96234",
          topic0: "",
          event_signature: "GameStarted(bytes32,address,address)",
          agent_attribution: { source: "topic", index: 2 },
          actor_field: { field: "topic", index: 3 },
          require_direct_call: false,
          field_predicates: [],
        },
      },
    },
    {
      topic: "onchain-action:solana-coordination-game-reveal",
      cta: "Complete a Solana coordination.game match via `reveal_guess` as Player 1 against an independent Player 2 (`solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp:<tx_sig>:<ix_index>`)",
      utmLink: "https://coordination.game",
      escrowLamports: 8_000_000,
      spec: {
        schema_version: 1,
        target_chain: solanaChain,
        action_mode: "affiliate_event",
        predicate: {
          kind: "solana_ix",
          program_id: "2qqVk7kUqffnahiJpcQJCsSd8ErbEUgKTgCn1zYsw64P",
          matcher: {
            mode: "instruction_data",
            ix_discriminator_hex: revealGuessDiscHex,
          },
          agent_attribution: { source: "account_index", index: 7 },
          actor_account_index: 8,
          min_lamports: 0,
          field_predicates: [],
        },
      },
    },
  ];
}

function getSecret(name: string): string {
  try {
    return execSync(
      `gcloud secrets versions access latest --secret=${name} --project=coordination-game-prod`,
      { stdio: ["ignore", "pipe", "ignore"] }
    )
      .toString()
      .trim();
  } catch {
    return "";
  }
}

function getJwtSecret(): string {
  return process.env.JWT_SECRET || getSecret("jwt-secret");
}

function getRpcUrl(isMainnet: boolean): string {
  if (isMainnet) {
    return (
      process.env.SOLANA_RPC_URL ||
      process.env.MAINNET_RPC ||
      getSecret("solana-rpc-url-mainnet") ||
      "https://api.mainnet-beta.solana.com"
    );
  }
  return (
    process.env.DEVNET_RPC ||
    process.env.SOLANA_RPC_URL ||
    getSecret("solana-rpc-url-devnet") ||
    "https://api.devnet.solana.com"
  );
}

function mintJwt(wallet: string, secret: string): string {
  if (!secret) return wallet;
  const header = Buffer.from(
    JSON.stringify({ alg: "HS256", typ: "JWT" })
  ).toString("base64url");
  const now = Math.floor(Date.now() / 1000);
  const payload = Buffer.from(
    JSON.stringify({
      wallet,
      exp: now + 86400,
      typ: "user",
      aud: ["shillbot", "game", "mcp"],
    })
  ).toString("base64url");
  const sig = crypto
    .createHmac("sha256", secret)
    .update(`${header}.${payload}`)
    .digest("base64url");
  return `${header}.${payload}.${sig}`;
}

async function api(
  method: string,
  path_: string,
  bearer: string,
  body?: unknown
): Promise<Record<string, unknown>> {
  const res = await fetch(API_BASE + path_, {
    method,
    headers: {
      Authorization: `Bearer ${bearer}`,
      "Content-Type": "application/json",
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await res.text();
  if (!res.ok) throw new Error(`${method} ${path_} -> ${res.status}: ${text}`);
  return text ? (JSON.parse(text) as Record<string, unknown>) : {};
}

function signBase64(unsigned: string, signer: Keypair): string {
  const raw = Buffer.from(unsigned, "base64");
  try {
    const vtx = VersionedTransaction.deserialize(raw);
    vtx.sign([signer]);
    return Buffer.from(vtx.serialize()).toString("base64");
  } catch {
    const tx = Transaction.from(raw);
    tx.partialSign(signer);
    return tx.serialize({ requireAllSignatures: false }).toString("base64");
  }
}

async function broadcast(conn: Connection, signedB64: string): Promise<string> {
  const raw = Buffer.from(signedB64, "base64");
  const sig = await conn.sendRawTransaction(raw, {
    skipPreflight: false,
    maxRetries: 5,
  });
  const deadline = Date.now() + 90_000;
  let lastResend = Date.now();
  while (Date.now() < deadline) {
    const { value } = await conn.getSignatureStatuses([sig], {
      searchTransactionHistory: true,
    });
    const st = value[0];
    if (st?.err) {
      throw new Error(`tx ${sig} failed on-chain: ${JSON.stringify(st.err)}`);
    }
    if (
      st?.confirmationStatus === "confirmed" ||
      st?.confirmationStatus === "finalized"
    ) {
      return sig;
    }
    if (Date.now() - lastResend > 4_000) {
      lastResend = Date.now();
      await conn
        .sendRawTransaction(raw, { skipPreflight: true, maxRetries: 2 })
        .catch(() => {});
    }
    await new Promise((r) => setTimeout(r, 1_500));
  }
  throw new Error(`Transaction ${sig} was not confirmed within 90.00 seconds`);
}

async function existingOpenTopics(
  bearer: string,
  network: string
): Promise<Set<string>> {
  const res = (await api(
    "GET",
    `/tasks?network=${network}&limit=100`,
    bearer
  )) as { tasks?: Array<{ state?: string; campaign_topic?: string }> };
  const topics = new Set<string>();
  for (const t of res.tasks ?? []) {
    const st = String(t.state ?? "").toLowerCase();
    if (st === "open" && t.campaign_topic) {
      topics.add(t.campaign_topic);
    }
  }
  return topics;
}

function verifyShowcaseEntriesOffline(entries: OnChainShowcaseEntry[]): void {
  for (const entry of entries) {
    if (entry.escrowLamports < MIN_CAMPAIGN_ESCROW_LAMPORTS) {
      throw new Error(
        `${entry.topic}: escrow ${entry.escrowLamports} < MIN_CAMPAIGN_ESCROW_LAMPORTS`
      );
    }
    const specJson = JSON.stringify(entry.spec);
    if (specJson.length > 4096) {
      throw new Error(`${entry.topic}: spec JSON exceeds 4096 bytes`);
    }
    const sha = crypto.createHash("sha256").update(specJson).digest("hex");
    console.log(
      `  [showcase precheck PASS] ${entry.topic} (${(
        entry.escrowLamports / LAMPORTS_PER_SOL
      ).toFixed(4)} SOL, raw-json-sha256=${sha.slice(0, 16)}...)`
    );
  }
}

async function main(): Promise<void> {
  const isMainnet =
    process.argv.includes("--mainnet") ||
    process.env.SOLANA_NETWORK === "mainnet";
  const checkOnly = process.argv.includes("--check-only");
  const forceReseed = process.argv.includes("--force");
  const network = isMainnet ? "mainnet" : "devnet";

  const entries = buildShowcaseEntries(true);
  verifyShowcaseEntriesOffline(entries);

  if (checkOnly) {
    console.log(
      `\n[check-only PASS] All ${entries.length} OnChainAction showcase specs validated.`
    );
    return;
  }

  const targetEntries = buildShowcaseEntries(isMainnet);
  const rpcUrl = getRpcUrl(isMainnet);
  const conn = new Connection(rpcUrl, "confirmed");
  const authority = loadKeypair("id.json");
  const clientPk = authority.publicKey.toBase58();
  const bearer = mintJwt(clientPk, getJwtSecret());

  const bal = await conn.getBalance(authority.publicKey, "confirmed");
  const totalEscrow = targetEntries.reduce(
    (acc, e) => acc + e.escrowLamports,
    0
  );
  if (bal < totalEscrow + 5_000_000) {
    throw new Error(
      `Insufficient ${network} balance: ${(bal / LAMPORTS_PER_SOL).toFixed(
        4
      )} SOL`
    );
  }

  console.log(`network: ${network}`);
  console.log(
    `wallet:  ${clientPk} (${(bal / LAMPORTS_PER_SOL).toFixed(4)} SOL)`
  );

  const already = forceReseed
    ? new Set<string>()
    : await existingOpenTopics(bearer, network);
  let created = 0;
  let skipped = 0;

  for (const entry of targetEntries) {
    if (already.has(entry.topic)) {
      console.log(`  skip (open task exists): ${entry.topic}`);
      skipped++;
      continue;
    }

    const camp = await api("POST", `/campaigns?network=${network}`, bearer, {
      brief: {
        topic: entry.topic,
        brand_voice: "technical",
        cta: entry.cta,
        utm_link: entry.utmLink,
      },
      budget_lamports: entry.escrowLamports,
      platform: ONCHAIN_ACTION_PLATFORM,
      onchain_action: entry.spec,
    });
    const campaignId = camp.campaign_id as string;

    const fund = await api(
      "POST",
      `/campaigns/${campaignId}/fund?network=${network}`,
      bearer,
      { amount_lamports: entry.escrowLamports }
    );
    const unsigned = fund.transaction as string;
    const sig = await broadcast(conn, signBase64(unsigned, authority));
    await api(
      "POST",
      `/tasks/${fund.task_id}/confirm?network=${network}`,
      bearer,
      {
        tx_signature: sig,
        action: "create",
        task_pda: fund.task_pda,
      }
    );
    console.log(
      `  [${network}] seeded ${entry.topic} (${(
        entry.escrowLamports / LAMPORTS_PER_SOL
      ).toFixed(4)} SOL) -> campaign ${campaignId} task ${fund.task_id} pda ${
        fund.task_pda
      } (tx ${sig})`
    );
    created++;
  }

  console.log(
    `\nonchain-action showcase seed complete: ${created} created, ${skipped} skipped`
  );
}

const isDirectRun =
  process.argv[1]?.endsWith("seed-onchain-action-showcase.ts") ||
  process.argv[1]?.endsWith("seed-onchain-action-showcase");

if (isDirectRun) {
  main().catch((e) => {
    console.error(e);
    process.exit(1);
  });
}
