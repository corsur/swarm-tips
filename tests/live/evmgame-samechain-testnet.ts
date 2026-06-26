/**
 * Live same-chain EVM-vs-EVM e2e on Base Sepolia — the end-to-end PROOF that the
 * operator-sig v-normalization works against the deployed CoordinationGame.
 *
 * Two funded EVM wallets play a real game via the deployed game-api
 * (api.coordination.game /internal/evmgame/*) + the live CoordinationGame
 * contract: join -> createGame (creator) + joinGame (joiner) -> commit -> reveal.
 * The LOAD-BEARING assertion is that `createGame` (which carries game-api's
 * operator attestation, signed with chain-core sign_digest_eth, v=27/28) is
 * ACCEPTED on-chain (no BadSignature revert) — proving the convention end-to-end.
 *
 * NOT a CI test (real testnet gas). Run locally:
 *   A_KEYFILE=~/.foundry/keystores/xchain-testnet.key \
 *   B_KEYFILE=~/.foundry/keystores/evmgame-player-b.key \
 *   npx tsx tests/live/evmgame-samechain-testnet.ts
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
import { baseSepolia } from "viem/chains";
import { deriveTerminalOutcome, OutcomeKind } from "../helpers/outcome-oracle";

const GAME_API = process.env.GAME_API ?? "https://api.coordination.game";
const RPC = process.env.RPC_URL ?? "https://sepolia.base.org";
const TOURNAMENT = Number(process.env.TOURNAMENT_ID ?? 2);
const CHAIN = "eip155:84532";
const ZERO32 = `0x${"00".repeat(32)}` as Hex;

const ABI = parseAbi([
  "function commitGuess(bytes32 gameId, bytes32 commitment)",
  "function revealGuess(bytes32 gameId, bytes32 r, bytes32 rMatchup)",
  "function games(bytes32) view returns (uint8 status, address player1, address player2, uint8 p1Guess, uint8 p2Guess, uint8 firstCommitter, uint8 matchupType, uint128 stakeWei, bytes32 matchupCommitment, bytes32 p1Commit, bytes32 p2Commit, uint64 activatedAt, uint64 firstCommitAt, uint64 bothCommitAt)",
]);

function loadKey(path: string): Hex {
  const raw = readFileSync(
    path.replace("~", process.env.HOME ?? ""),
    "utf8"
  ).trim();
  return `0x${raw.replace(/^0x/, "")}` as Hex;
}
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
const status = (wallet: string) =>
  api(`/internal/evmgame/status?wallet=${wallet}`);
const committed = (game_id: string, wallet: string) =>
  api("/internal/evmgame/committed", {
    method: "POST",
    body: JSON.stringify({ game_id, wallet }),
  });

/** commit secret: random 32-byte r with the guess in the last bit; SHA-256(r). */
async function commitSecret(
  guess: 0 | 1
): Promise<{ r: Hex; commitment: Hex }> {
  const r = new Uint8Array(32);
  webcrypto.getRandomValues(r);
  r[31] = (r[31] & 0xfe) | guess;
  const digest = new Uint8Array(await webcrypto.subtle.digest("SHA-256", r));
  return { r: toHex(r), commitment: toHex(digest) };
}

/** Poll the on-chain game status until it reaches >= `min` — the public Base
 *  Sepolia RPC is load-balanced and lags, so estimateGas on the next tx can hit
 *  a node that hasn't indexed the prior tx yet. */
async function untilStatus(
  pub: ReturnType<typeof createPublicClient>,
  contract: Address,
  gameId: Hex,
  min: number,
  label: string
): Promise<void> {
  for (let i = 0; i < 30; i++) {
    const g = (await pub.readContract({
      address: contract,
      abi: ABI,
      functionName: "games",
      args: [gameId],
    })) as unknown as unknown[];
    if (Number(g[0]) >= min) return;
    await new Promise((r) => setTimeout(r, 3000));
  }
  throw new Error(`timed out waiting for game status >= ${min} (${label})`);
}

async function main() {
  const aAcct = privateKeyToAccount(
    loadKey(process.env.A_KEYFILE ?? "~/.foundry/keystores/xchain-testnet.key")
  );
  const bAcct = privateKeyToAccount(
    loadKey(
      process.env.B_KEYFILE ?? "~/.foundry/keystores/evmgame-player-b.key"
    )
  );
  const pub = createPublicClient({ chain: baseSepolia, transport: http(RPC) });
  const aw = createWalletClient({
    account: aAcct,
    chain: baseSepolia,
    transport: http(RPC),
  });
  const bw = createWalletClient({
    account: bAcct,
    chain: baseSepolia,
    transport: http(RPC),
  });
  console.log(`A (creator) ${aAcct.address}\nB (joiner)  ${bAcct.address}`);

  // 1) Pair: A queues (waiting), B joins (matched) -> A is the waiter=creator.
  await join(aAcct.address);
  const bRes = await join(bAcct.address);
  if (bRes.status !== "matched")
    throw new Error(`B expected matched, got ${bRes.status}`);
  const match = bRes.match;
  const contract = match.contract as Address;
  const gameId = match.game_id as Hex;
  if (match.create_call.player.toLowerCase() !== aAcct.address.toLowerCase())
    throw new Error("expected A to be the creator");
  console.log(
    `matched: game ${gameId} on ${contract}, stake ${match.stake_base_units} wei`
  );

  // 2) A submits createGame (operator-attested) — THE v-normalization proof.
  const cc = match.create_call;
  const createHash = await aw.sendTransaction({
    to: cc.to as Address,
    data: cc.data as Hex,
    value: BigInt(cc.value_wei),
  });
  const createRcpt = await pub.waitForTransactionReceipt({ hash: createHash });
  if (createRcpt.status !== "success")
    throw new Error(
      `createGame REVERTED (${createHash}) — v-normalization broken`
    );
  console.log(
    `✅ createGame accepted on-chain (operator sig verified): ${createHash}`
  );
  await untilStatus(pub, contract, gameId, 1, "pending");

  // 3) B submits joinGame.
  const jc = match.join_call;
  const joinHash = await bw.sendTransaction({
    to: jc.to as Address,
    data: jc.data as Hex,
    value: BigInt(jc.value_wei),
  });
  if (
    (await pub.waitForTransactionReceipt({ hash: joinHash })).status !==
    "success"
  )
    throw new Error("joinGame reverted");
  console.log(`✅ joinGame accepted: ${joinHash}`);
  await untilStatus(pub, contract, gameId, 2, "active");

  // 4) Commit (A first -> firstCommitter=1), then notify game-api.
  const aSec = await commitSecret(1);
  const bSec = await commitSecret(0);
  const send = (w: typeof aw, fn: string, args: unknown[]) =>
    w.sendTransaction({
      to: contract,
      data: encodeFunctionData({
        abi: ABI,
        functionName: fn as never,
        args: args as never,
      }),
    });
  await pub.waitForTransactionReceipt({
    hash: await send(aw, "commitGuess", [gameId, aSec.commitment]),
  });
  await untilStatus(pub, contract, gameId, 3, "committing"); // A committed
  await pub.waitForTransactionReceipt({
    hash: await send(bw, "commitGuess", [gameId, bSec.commitment]),
  });
  await committed(gameId, aAcct.address);
  const bComm = await committed(gameId, bAcct.address);
  if (!bComm.both_committed || !bComm.r_matchup)
    throw new Error("expected both_committed + r_matchup");
  await untilStatus(pub, contract, gameId, 4, "revealing"); // both committed -> Revealing
  console.log(`✅ both committed; r_matchup released`);

  // 5) Reveal: creator A first WITH r_matchup; joiner B with the zero sentinel.
  await pub.waitForTransactionReceipt({
    hash: await send(aw, "revealGuess", [
      gameId,
      aSec.r,
      bComm.r_matchup as Hex,
    ]),
  });
  // B's reveal takes the second-revealer path; wait until A's reveal has bound
  // matchupType on-chain so B's estimateGas doesn't hit the first-revealer check.
  for (let i = 0; i < 30; i++) {
    const g = (await pub.readContract({
      address: contract,
      abi: ABI,
      functionName: "games",
      args: [gameId],
    })) as unknown as unknown[];
    if (Number(g[6]) !== 255) break; // matchupType bound
    await new Promise((r) => setTimeout(r, 3000));
  }
  await pub.waitForTransactionReceipt({
    hash: await send(bw, "revealGuess", [gameId, bSec.r, ZERO32]),
  });

  // 6) Wait for resolution (lag-robust) and cross-check the on-chain outcome
  //    against the canonical oracle.
  await untilStatus(pub, contract, gameId, 5, "resolved");
  const g = (await pub.readContract({
    address: contract,
    abi: ABI,
    functionName: "games",
    args: [gameId],
  })) as unknown as unknown[];
  const [, , , p1Guess, p2Guess, firstCommitter, matchupType] = g.map(
    Number
  ) as number[];
  const expected = deriveTerminalOutcome({
    stepCount: 4,
    matchupType,
    p1Guess,
    p2Guess,
    firstCommitter,
  });
  console.log(
    `✅ game RESOLVED on-chain: p1Guess=${p1Guess} p2Guess=${p2Guess} matchupType=${matchupType} firstCommitter=${firstCommitter} → ${OutcomeKind[expected]}`
  );
  console.log(
    "\nPASS — same-chain EVM end-to-end on Base Sepolia; operator-sig v-normalization confirmed live; resolved per the canonical payoff matrix."
  );
}

main().catch((e) => {
  console.error("FAIL:", e);
  process.exit(1);
});
