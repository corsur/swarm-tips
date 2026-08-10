/**
 * Live same-chain EVM human-vs-GROK e2e — the marketing-push proof that a person
 * can play the AI opponent for real on Base MAINNET.
 *
 * One funded human wallet joins the EVM queue ALONE; the game-api AI-fallback
 * fires (~20-25s), spawns grok-agent, and grok joins as P2 from its own funded
 * wallet. The human (creator) drives createGame -> commit -> reveal; grok drives
 * joinGame -> commit -> reveal autonomously. We assert the game RESOLVES on-chain
 * and P2 is grok's wallet.
 *
 * NOT CI (real gas + a live grok spawn). Run:
 *   GAME_API=https://api.coordination.game CHAIN=eip155:8453 \
 *   RPC_URL=https://base-rpc.publicnode.com TOURNAMENT_ID=3 \
 *   A_KEYFILE=~/.foundry/keystores/evmgame-player-a.key \
 *   npx ts-mocha -p tsconfig.json tests/live/evmgame-grok-mainnet.ts --timeout 300000
 */
import { readFileSync } from "fs";
import { webcrypto } from "crypto";
import {
  createPublicClient,
  createWalletClient,
  http,
  encodeFunctionData,
  toHex,
  parseAbi,
  type Hex,
  type Address,
} from "viem";
import { privateKeyToAccount } from "viem/accounts";
import { base, baseSepolia } from "viem/chains";
import { assert } from "chai";

const GAME_API = process.env.GAME_API ?? "https://api.coordination.game";
const RPC = process.env.RPC_URL ?? "https://base-rpc.publicnode.com";
// T3 (ends 2027-08-05). T2 expired 2026-08-06 and on-chain end_time is
// immutable, so rollover replaces the id rather than extending it — a default of
// 2 silently joined a DEAD tournament. Mirrors tests/e2e/harness/network.ts.
const TOURNAMENT = Number(process.env.TOURNAMENT_ID ?? 3);
const CHAIN = process.env.CHAIN ?? "eip155:8453";
const VIEM_CHAIN = CHAIN === "eip155:8453" ? base : baseSepolia;
const GROK_EVM = "0xf70D421719cd3843415Ed57C6F7AB6F671089D15".toLowerCase();

const ABI = parseAbi([
  "function commitGuess(bytes32 gameId, bytes32 commitment)",
  "function revealGuess(bytes32 gameId, bytes32 r, bytes32 rMatchup)",
  "function games(bytes32) view returns (uint8 status, address player1, address player2, uint8 p1Guess, uint8 p2Guess, uint8 firstCommitter, uint8 matchupType, uint128 stakeWei, bytes32 matchupCommitment, bytes32 p1Commit, bytes32 p2Commit, uint64 activatedAt, uint64 firstCommitAt, uint64 bothCommitAt)",
]);

const loadKey = (p: string): Hex => {
  const raw = readFileSync(
    p.replace("~", process.env.HOME ?? ""),
    "utf8"
  ).trim();
  return `0x${raw.replace(/^0x/, "")}` as Hex;
};
const api = async (path: string, init?: RequestInit) => {
  const res = await fetch(`${GAME_API}${path}`, {
    headers: { "Content-Type": "application/json" },
    ...init,
  });
  if (!res.ok) throw new Error(`${path} -> ${res.status} ${await res.text()}`);
  return res.json();
};
const join = (wallet: string) =>
  api("/internal/evmgame/join", {
    method: "POST",
    body: JSON.stringify({ wallet, chain: CHAIN, tournament_id: TOURNAMENT }),
  });
const evmStatus = (wallet: string) =>
  api(`/internal/evmgame/status?wallet=${wallet}`);
const committed = (game_id: string, wallet: string) =>
  api("/internal/evmgame/committed", {
    method: "POST",
    body: JSON.stringify({ game_id, wallet }),
  });
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

async function commitSecret(
  guess: 0 | 1
): Promise<{ r: Hex; commitment: Hex }> {
  const r = new Uint8Array(32);
  webcrypto.getRandomValues(r);
  r[31] = (r[31] & 0xfe) | guess;
  const digest = new Uint8Array(await webcrypto.subtle.digest("SHA-256", r));
  return { r: toHex(r), commitment: toHex(digest) };
}

async function untilStatus(
  pub: ReturnType<typeof createPublicClient>,
  contract: Address,
  gameId: Hex,
  min: number,
  label: string,
  tries = 40
): Promise<number[]> {
  for (let i = 0; i < tries; i++) {
    const g = (await pub.readContract({
      address: contract,
      abi: ABI,
      functionName: "games",
      args: [gameId],
    })) as unknown as unknown[];
    if (Number((g as number[])[0]) >= min)
      return (g as unknown[]).map(Number) as number[];
    await sleep(3000);
  }
  throw new Error(`timed out waiting for status >= ${min} (${label})`);
}

async function main() {
  const aAcct = privateKeyToAccount(
    loadKey(
      process.env.A_KEYFILE ?? "~/.foundry/keystores/evmgame-player-a.key"
    )
  );
  const pub = createPublicClient({ chain: VIEM_CHAIN, transport: http(RPC) });
  const aw = createWalletClient({
    account: aAcct,
    chain: VIEM_CHAIN,
    transport: http(RPC),
  });
  console.log(`human (creator) ${aAcct.address}\ngrok (P2)       ${GROK_EVM}`);

  // 1) Human joins ALONE -> Waiting -> schedules the AI fallback.
  const jr = await join(aAcct.address);
  console.log(`join -> ${jr.status} (waiting on grok fallback ~20-25s)`);

  // 2) Poll until grok fills the match (fallback spawns grok as P2).
  let match: any = null;
  for (let i = 0; i < 40 && !match; i++) {
    await sleep(5000);
    const s = await evmStatus(aAcct.address);
    if (s.status === "matched" && s.match) match = s.match;
  }
  assert(match, "grok never joined — AI fallback did not fill P2 in time");
  const contract = match.contract as Address;
  const gameId = match.game_id as Hex;
  assert.equal(
    match.create_call.player.toLowerCase(),
    aAcct.address.toLowerCase(),
    "human should be the creator"
  );
  console.log(`✅ grok filled P2 — game ${gameId} on ${contract}`);

  // 3) Human createGame (operator-attested).
  const cc = match.create_call;
  const ch = await aw.sendTransaction({
    to: cc.to as Address,
    data: cc.data as Hex,
    value: BigInt(cc.value_wei),
  });
  assert.equal(
    (await pub.waitForTransactionReceipt({ hash: ch })).status,
    "success",
    "createGame reverted"
  );
  console.log(`✅ human createGame: ${ch}`);

  // 4) grok joinGame's on its own -> Active. Read player2 from the raw view
  // (untilStatus returns numeric-coerced fields, so re-read for the address).
  await untilStatus(pub, contract, gameId, 2, "active (grok joined)");
  const gRead = (await pub.readContract({
    address: contract,
    abi: ABI,
    functionName: "games",
    args: [gameId],
  })) as unknown as unknown[];
  assert.equal(
    (gRead[2] as string).toLowerCase(),
    GROK_EVM,
    "P2 on-chain must be grok's wallet"
  );
  console.log(`✅ grok joinGame'd as P2`);

  // 5) Human commits, notify game-api.
  const aSec = await commitSecret(1);
  const commitData = encodeFunctionData({
    abi: ABI,
    functionName: "commitGuess",
    args: [gameId, aSec.commitment],
  });
  await pub.waitForTransactionReceipt({
    hash: await aw.sendTransaction({ to: contract, data: commitData }),
  });
  await committed(gameId, aAcct.address);
  console.log(`✅ human committed; waiting for grok to commit...`);

  // 6) grok commits on its own -> both committed, r_matchup released. grok
  // runs a full ~3-min persona chat loop before deciding + committing, so
  // wait past that (this is the real product pace, not a hang).
  let bComm: any = null;
  for (let i = 0; i < 90; i++) {
    const r = await committed(gameId, aAcct.address);
    if (r.both_committed && r.r_matchup) {
      bComm = r;
      break;
    }
    await sleep(3000);
  }
  assert(bComm, "grok never committed (waited 4.5min past its chat loop)");
  await untilStatus(pub, contract, gameId, 4, "revealing");
  console.log(`✅ both committed; r_matchup released`);

  // 7) Human reveals first WITH r_matchup; grok reveals second on its own.
  const revealData = encodeFunctionData({
    abi: ABI,
    functionName: "revealGuess",
    args: [gameId, aSec.r, bComm.r_matchup as Hex],
  });
  await pub.waitForTransactionReceipt({
    hash: await aw.sendTransaction({ to: contract, data: revealData }),
  });
  console.log(`✅ human revealed; waiting for grok to reveal + resolve...`);

  // 8) grok reveals -> RESOLVED.
  const resolved = await untilStatus(pub, contract, gameId, 5, "resolved", 60);
  console.log(
    `✅ RESOLVED on-chain: p1Guess=${resolved[3]} p2Guess=${resolved[4]} matchupType=${resolved[6]} firstCommitter=${resolved[5]}`
  );
  console.log(
    `\nPASS — human vs grok, real game on ${CHAIN}; grok joined + played + resolved as P2 (${GROK_EVM}).`
  );
}

main()
  .then(() => process.exit(0))
  .catch((e) => {
    console.error("FAIL:", e.message ?? e);
    process.exit(1);
  });
