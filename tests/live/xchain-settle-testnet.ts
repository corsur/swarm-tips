/**
 * Live cross-chain settle e2e against the DEPLOYED Base Sepolia CrossChainGame.
 *
 * This is MANUAL-ONLY (live network, real testnet ETH) — never a CI gate.
 * It proves the deployed bytecode + the operator key in Secret Manager + the
 * shared certificate encoding all agree on-chain by running a full EVM-leg
 * match lifecycle with real signatures:
 *
 *     poolDeposit (already funded) -> createMatch -> lockTranche
 *       -> co-sign MatchLiveCert + OutcomeCert (legA + legB session keys +
 *          operator, 3+3 secp256k1 sigs, v bumped to 27/28 for OZ ECDSA)
 *       -> settle -> assert Settled + on-chain payoff conservation
 *
 * The certificate encoders come from the same golden-vector-verified helper
 * the anchor tests use (tests/helpers/xchain-cert.ts), so the digest the EVM
 * contract recomputes is byte-identical to what was signed. Transactions are
 * signed by `cast` via the Foundry keystore — no raw private key touches this
 * process except the per-match session keys (which we generate) and the
 * operator key (read from GCP Secret Manager into OPERATOR_SK).
 *
 * Run:
 *   export OPERATOR_SK=$(gcloud secrets versions access latest --secret=xchain-operator-signer)
 *   export KEYSTORE_PW=$(cat ~/.foundry/keystores/xchain-testnet.password)
 *   npx ts-mocha -p ./tsconfig.json tests/live/xchain-settle-testnet.ts --timeout 600000
 */
import { execFileSync } from "child_process";
import { expect } from "chai";
import {
  encodeFunctionData,
  keccak256 as viemKeccak,
  toBytes,
  toHex,
  pad,
  bytesToHex,
} from "viem";
import { privateKeyToAccount } from "viem/accounts";
import {
  newSessionSigner,
  signDigest,
  matchLiveDigest,
  outcomeDigest,
  type SessionSigner,
  type MatchLiveCert,
  type OutcomeCert,
} from "../helpers/xchain-cert";

const RPC = process.env.RPC_URL ?? "https://sepolia.base.org";
const CONTRACT = (process.env.CONTRACT ??
  "0xd585baE48901513202dAEb7d4feE4Af508a96234") as `0x${string}`;
// Live-network, manual-only. The Anchor.toml test glob (tests/**/*.ts) and a
// bare `anchor test` would otherwise sweep this in; skip unless the live
// credentials are present so it never runs (or fails) in those contexts.
const LIVE = Boolean(process.env.OPERATOR_SK && process.env.KEYSTORE_PW);
const OPERATOR_SK = process.env.OPERATOR_SK ?? "";
const KEYSTORE_PW = process.env.KEYSTORE_PW ?? "";
const DEPLOYER = "0x996213ed4099707059b8b5d7489ffF23dAC9770d" as `0x${string}`;
const OPERATOR_ADDR = "0x54a600649C2906f7Bb0D52E74818Ce025afd9A30";

const SOLANA_CHAIN_ID = "solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1";
const HOMOG_BOTH_CORRECT = 0;
const TERMINAL_STEP_COUNT = 4;

/** A SessionSigner from a raw 32-byte hex key (for the operator). */
function signerFromHex(hex: string): SessionSigner {
  const clean = hex.startsWith("0x") ? hex.slice(2) : hex;
  const privateKey = toBytes(`0x${clean}`);
  const acct = privateKeyToAccount(`0x${clean}`);
  return { privateKey, address: toBytes(acct.address) };
}

/** EVM signature: helper emits v=recid (0/1); OZ ECDSA wants 27/28. */
function signEvm(signer: SessionSigner, digest: Uint8Array): `0x${string}` {
  const sig = signDigest(signer, digest); // [r..32, s..32, v(0|1)]
  const out = Uint8Array.from(sig);
  out[64] = out[64] + 27;
  return bytesToHex(out);
}

function cast(args: string[]): string {
  return execFileSync("cast", args, { encoding: "utf8" }).trim();
}

/** A read-only `cast call <contract> "<sig>"` returning the first output line. */
function castCall(sig: string, args: string[] = []): string {
  return cast(["call", CONTRACT, sig, ...args, "--rpc-url", RPC])
    .split("\n")[0]
    .trim();
}

const castUint = (sig: string): bigint => BigInt(castCall(sig).split(" ")[0]);

function castSend(to: string, extra: string[]): string {
  return cast([
    "send",
    to,
    ...extra,
    "--account",
    "xchain-testnet",
    "--password",
    KEYSTORE_PW,
    "--rpc-url",
    RPC,
  ]);
}

const SETTLE_ABI = [
  {
    type: "function",
    name: "settle",
    stateMutability: "nonpayable",
    inputs: [
      {
        name: "cert",
        type: "tuple",
        components: [
          { name: "matchId", type: "bytes32" },
          { name: "tournamentId", type: "uint64" },
          { name: "matchupCommitment", type: "bytes32" },
          { name: "legA", type: "tuple", components: legComponents() },
          { name: "legB", type: "tuple", components: legComponents() },
          { name: "quoteTimestamp", type: "uint64" },
          { name: "quoteMaxAgeSecs", type: "uint32" },
          { name: "matchDeadline", type: "uint64" },
          { name: "claimWindowSecs", type: "uint32" },
          { name: "aIsP1", type: "uint8" },
        ],
      },
      {
        name: "oc",
        type: "tuple",
        components: [
          { name: "matchId", type: "bytes32" },
          { name: "matchLiveDigest", type: "bytes32" },
          { name: "outcomeKind", type: "uint8" },
          { name: "stepCount", type: "uint8" },
          { name: "p1Guess", type: "uint8" },
          { name: "p2Guess", type: "uint8" },
          { name: "firstCommitter", type: "uint8" },
          { name: "matchupType", type: "uint8" },
          { name: "transcriptHash", type: "bytes32" },
        ],
      },
      { name: "liveSigs", type: "bytes[3]" },
      { name: "ocSigs", type: "bytes[3]" },
    ],
    outputs: [],
  },
] as const;
// Permissionless lockTranche(cert, operatorSig).
const LOCK_ABI = [
  {
    type: "function",
    name: "lockTranche",
    stateMutability: "nonpayable",
    inputs: [
      {
        name: "cert",
        type: "tuple",
        components: [
          { name: "matchId", type: "bytes32" },
          { name: "tournamentId", type: "uint64" },
          { name: "matchupCommitment", type: "bytes32" },
          { name: "legA", type: "tuple", components: legComponents() },
          { name: "legB", type: "tuple", components: legComponents() },
          { name: "quoteTimestamp", type: "uint64" },
          { name: "quoteMaxAgeSecs", type: "uint32" },
          { name: "matchDeadline", type: "uint64" },
          { name: "claimWindowSecs", type: "uint32" },
          { name: "aIsP1", type: "uint8" },
        ],
      },
      { name: "operatorSig", type: "bytes" },
    ],
    outputs: [],
  },
] as const;

function legComponents() {
  return [
    { name: "chainTag", type: "bytes32" },
    { name: "contractId", type: "bytes32" },
    { name: "player", type: "bytes32" },
    { name: "sessionKey", type: "address" },
    { name: "stake", type: "uint128" },
    { name: "tranche", type: "uint128" },
  ] as const;
}

const b32 = (u: Uint8Array): `0x${string}` => toHex(u);
const addr = (u: Uint8Array): `0x${string}` => toHex(u); // 20-byte

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Public Base Sepolia RPC is load-balanced; reads can hit a node a block
 *  behind a just-confirmed send. Poll until `ok(value)` or exhaust tries. */
async function poll<T>(
  read: () => T | Promise<T>,
  ok: (v: T) => boolean,
  label: string
): Promise<T> {
  for (let i = 0; i < 20; i += 1) {
    const v = await read();
    if (ok(v)) return v;
    await sleep(1500);
  }
  throw new Error(`poll timed out: ${label}`);
}
let randCounter = 0;
const randBytes32 = (): Uint8Array => {
  randCounter += 1;
  const seed = BigInt(Date.now()) * 1_000_000n + BigInt(randCounter);
  return toBytes(viemKeccak(toBytes(`0x${seed.toString(16)}`)));
};

(LIVE ? describe : describe.skip)(
  "live cross-chain settle (Base Sepolia)",
  function () {
    this.timeout(600_000);
    const balanceOf = (): bigint =>
      BigInt(cast(["balance", CONTRACT, "--rpc-url", RPC]).split(" ")[0]);

    it("runs a full match lifecycle and settles with conservation", async () => {
      // Sanity: operator key matches the on-chain operatorSigner.
      const operator = signerFromHex(OPERATOR_SK);
      expect(toHex(operator.address).toLowerCase()).to.equal(
        OPERATOR_ADDR.toLowerCase()
      );

      const stake = castUint("stakeWei()(uint256)");
      const chainTagB = castCall("CHAIN_TAG()(bytes32)") as `0x${string}`;
      const poolFreeStart = readUint("poolFree");
      expect(
        poolFreeStart >= stake,
        "pool must be funded; run poolDeposit first"
      ).to.be.true;

      const now = Number(
        cast(["block", "latest", "--rpc-url", RPC, "-f", "timestamp"])
      );
      const fundDeadline = now + 3600;
      const matchDeadline = now + 7200;
      const tranche = stake; // <= maxTranche; winner counter-value
      const matchId = randBytes32();
      const matchIdHex = b32(matchId);

      const legA = newSessionSigner(); // Solana-side counterparty session key
      const legB = newSessionSigner(); // local EVM player's session key

      // createMatch: local player (deployer) funds stake; playerIsP1 = true.
      castSend(CONTRACT, [
        "createMatch(bytes32,address,address,bool,uint64,uint64)",
        matchIdHex,
        addr(legB.address),
        addr(legA.address),
        "true",
        String(fundDeadline),
        String(matchDeadline),
        "--value",
        stake.toString(),
      ]);
      await poll(
        () => matchStatus(matchIdHex),
        (s) => s === 1,
        "Funded"
      ); // Funded

      // Build the co-signed cert BEFORE the lock — the operator's match-live
      // signature over it authorizes the permissionless lock. Leg A is Solana,
      // leg B is EVM; the EVM player is P1 (createMatch playerIsP1=true) so leg
      // A's player is NOT P1 → aIsP1=0 (the contract enforces (aIsP1==1) != playerIsP1).
      const cert: MatchLiveCert = {
        matchId,
        tournamentId: 1n,
        matchupCommitment: randBytes32(),
        legA: {
          chainTag: toBytes(
            viemKeccak(new TextEncoder().encode(SOLANA_CHAIN_ID))
          ),
          contract: randBytes32(),
          player: randBytes32(),
          sessionKey: legA.address,
          stake: 50_000_000n,
          tranche: 50_000_000n,
        },
        legB: {
          chainTag: toBytes(chainTagB),
          contract: toBytes(pad(CONTRACT, { size: 32 })),
          player: toBytes(pad(DEPLOYER, { size: 32 })),
          sessionKey: legB.address,
          stake,
          tranche,
        },
        quoteTimestamp: BigInt(now - 60),
        quoteMaxAgeSecs: 3600,
        matchDeadline: BigInt(matchDeadline),
        claimWindowSecs: 3600,
        aIsP1: 0,
      };
      const liveDigest = matchLiveDigest(cert);

      // lockTranche: PERMISSIONLESS — the operator's match-live sig authorizes;
      // the cranker (deployer) submits + pays. Locks cert.legB.tranche.
      castSend(CONTRACT, [
        encodeFunctionData({
          abi: LOCK_ABI,
          functionName: "lockTranche",
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          args: [certForAbi(cert) as any, signEvm(operator, liveDigest) as any],
        }),
      ]);
      await poll(
        () => matchStatus(matchIdHex),
        (s) => s === 2,
        "Locked"
      ); // Locked

      // Snapshot the pool + contract balance AFTER the lock, right before
      // settle, so the conservation asserts are deltas (robust to any other
      // matches locked in the same shared pool).
      const poolFreePreSettle = await poll(
        () => readUint("poolFree"),
        (v) => v === poolFreeStart - tranche,
        "poolFree debited by tranche"
      );
      const poolLockedPreSettle = readUint("poolLocked");
      const balPreSettle = balanceOf();

      const oc: OutcomeCert = {
        matchId,
        matchLiveDigest: liveDigest,
        outcomeKind: HOMOG_BOTH_CORRECT,
        stepCount: TERMINAL_STEP_COUNT,
        p1Guess: 0,
        p2Guess: 0,
        firstCommitter: 1,
        matchupType: 0,
        transcriptHash: randBytes32(),
      };
      const ocDigest = outcomeDigest(oc);

      const liveSigs = [
        signEvm(legA, liveDigest),
        signEvm(legB, liveDigest),
        signEvm(operator, liveDigest),
      ];
      const ocSigs = [
        signEvm(legA, ocDigest),
        signEvm(legB, ocDigest),
        signEvm(operator, ocDigest),
      ];

      const data = encodeFunctionData({
        abi: SETTLE_ABI,
        functionName: "settle",
        args: [certForAbi(cert), ocForAbi(oc), liveSigs as any, ocSigs as any],
      });

      castSend(CONTRACT, [data]);

      // HOMOG_BOTH_CORRECT -> KeepStake: player gets stake back, the locked
      // tranche releases back to the pool, treasury untouched. Conservation,
      // all measured as deltas around the settle:
      //   poolLocked -= tranche, poolFree += tranche, balance -= stake (payout).
      await poll(
        () => matchStatus(matchIdHex),
        (s) => s === 4,
        "Settled"
      ); // Settled
      await poll(
        () => readUint("poolLocked"),
        (v) => v === poolLockedPreSettle - tranche,
        "poolLocked released by tranche"
      );
      expect(readUint("poolFree")).to.equal(poolFreePreSettle + tranche);
      const balAfter = balanceOf();
      expect(balPreSettle - balAfter).to.equal(stake);
    });

    function readUint(fn: "poolFree" | "poolLocked"): bigint {
      return castUint(`${fn}()(uint256)`);
    }

    async function matchStatus(id: `0x${string}`): Promise<number> {
      const out = cast([
        "call",
        CONTRACT,
        "matches(bytes32)(uint8,address,address,address,bool,bool,bool,uint128,uint128,uint64,uint64,uint64,uint64,bytes32,uint8,uint8)",
        id,
        "--rpc-url",
        RPC,
      ]);
      return Number(out.split("\n")[0].trim());
    }
  }
);

function certForAbi(c: MatchLiveCert) {
  return {
    matchId: b32(c.matchId),
    tournamentId: c.tournamentId,
    matchupCommitment: b32(c.matchupCommitment),
    legA: legForAbi(c.legA),
    legB: legForAbi(c.legB),
    quoteTimestamp: c.quoteTimestamp,
    quoteMaxAgeSecs: c.quoteMaxAgeSecs,
    matchDeadline: c.matchDeadline,
    claimWindowSecs: c.claimWindowSecs,
    aIsP1: c.aIsP1,
  };
}

function legForAbi(l: MatchLiveCert["legA"]) {
  return {
    chainTag: b32(l.chainTag),
    contractId: b32(l.contract),
    player: b32(l.player),
    sessionKey: addr(l.sessionKey),
    stake: l.stake,
    tranche: l.tranche,
  };
}

function ocForAbi(o: OutcomeCert) {
  return {
    matchId: b32(o.matchId),
    matchLiveDigest: b32(o.matchLiveDigest),
    outcomeKind: o.outcomeKind,
    stepCount: o.stepCount,
    p1Guess: o.p1Guess,
    p2Guess: o.p2Guess,
    firstCommitter: o.firstCommitter,
    matchupType: o.matchupType,
    transcriptHash: b32(o.transcriptHash),
  };
}
