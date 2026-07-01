/**
 * Live CROSS-CHAIN settle e2e: Solana devnet leg + Base Sepolia EVM leg, bound
 * by ONE co-signed MatchLiveCert + OutcomeCert, settled on BOTH chains.
 *
 * This is the completion-gate proof that the two deployed programs + the
 * operator key + the shared certificate encoding all agree across chains:
 *
 *   Solana: create_xmatch (matchmaker-cosigned, id.json player) -> lock_xtranche
 *   EVM:    createMatch (0x9962 player)                          -> lockTranche
 *     -> build ONE canonical cert (legA = Solana state, legB = EVM state)
 *     -> co-sign liveDigest + ocDigest with legA + legB + operator
 *        (Solana wants v=recid 0/1; EVM/OZ wants 27/28 — same digest, both encodings)
 *     -> settle (EVM)  +  settle_xmatch (Solana, legA rebuilt on-chain)
 *     -> assert Settled + conservation on both legs.
 *
 * MANUAL-ONLY (live network, real testnet funds) — self-skips without creds so
 * the `anchor test` glob never runs it. The cert encoders come from the same
 * golden-vector-verified helper the anchor tests use, so the digest each chain
 * recomputes is byte-identical to what was signed.
 *
 * Run:
 *   export OPERATOR_SK=$(gcloud secrets versions access latest --secret=xchain-operator-signer --project=coordination-game-prod)
 *   export EVM_KEY=$(cat ~/.foundry/keystores/xchain-testnet.key)
 *   export MATCHMAKER_KEY=$(gcloud secrets versions access latest --secret=matchmaker-keypair --project=coordination-game-prod)
 *   npx ts-mocha -p ./tsconfig.json tests/live/xchain-cross-settle-testnet.ts --timeout 600000
 */
import { execFileSync } from "child_process";
import { readFileSync } from "fs";
import { homedir } from "os";
import { expect } from "chai";
import { Connection, Keypair, PublicKey, SystemProgram } from "@solana/web3.js";
import {
  AnchorProvider,
  Program,
  Wallet,
  BN,
  type Idl,
} from "@coral-xyz/anchor";
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

// ---- Config (all verified live 2026-06-12) --------------------------------
const SOLANA_RPC =
  process.env.SOLANA_RPC_URL ?? "https://api.devnet.solana.com";
const EVM_RPC = process.env.RPC_URL ?? "https://sepolia.base.org";
const PROGRAM_ID = new PublicKey(
  "2qqVk7kUqffnahiJpcQJCsSd8ErbEUgKTgCn1zYsw64P"
);
const CONTRACT = (process.env.CONTRACT ??
  "0xd38b1fB07Bf64801bCBc3721937D6e2Ba6E5feb4") as `0x${string}`;
const SOLANA_CHAIN_ID = "solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1";
const IDL_PATH =
  process.env.IDL_PATH ??
  `${homedir()}/swarm/swarm-tips-repo/target/idl/coordination_game.json`;
// Tournament 2 on devnet is current-layout (122 bytes); tournament 1 is a stale
// 98-byte account from an older program version that the current program can't
// deserialize (AccountDidNotDeserialize). settle_xmatch derives the tournament
// from xmatch.tournament_id, so the xmatch must be funded under a current one.
const TOURNAMENT_ID = BigInt(process.env.TOURNAMENT_ID ?? "2");
const HOMOG_BOTH_CORRECT = 0;
const TERMINAL_STEP_COUNT = 4;
const SOL_STAKE = 50_000_000n; // 0.05 SOL (FIXED_STAKE_LAMPORTS)

const OPERATOR_SK = process.env.OPERATOR_SK ?? "";
const EVM_KEY = (process.env.EVM_KEY ?? "").trim();
const MATCHMAKER_KEY = process.env.MATCHMAKER_KEY ?? "";
// Live, manual-only: needs the operator key, the EVM owner key, and the
// matchmaker key (create_xmatch cosign). Self-skips otherwise.
const LIVE = Boolean(OPERATOR_SK && EVM_KEY && MATCHMAKER_KEY);

// ---- helpers ---------------------------------------------------------------
function signerFromHex(hex: string): SessionSigner {
  const clean = hex.startsWith("0x") ? hex.slice(2) : hex;
  const acct = privateKeyToAccount(`0x${clean}`);
  return { privateKey: toBytes(`0x${clean}`), address: toBytes(acct.address) };
}
/** EVM/OZ ECDSA wants v=27/28; the helper emits v=recid (0/1). */
function signEvm(s: SessionSigner, d: Uint8Array): `0x${string}` {
  const out = Uint8Array.from(signDigest(s, d));
  out[64] += 27;
  return bytesToHex(out);
}
/** Solana secp256k1_recover wants the raw recid (0/1) — helper's native form. */
function signSol(s: SessionSigner, d: Uint8Array): number[] {
  return Array.from(signDigest(s, d));
}
function loadKeypair(json: string): Keypair {
  return Keypair.fromSecretKey(Uint8Array.from(JSON.parse(json) as number[]));
}
const b32 = (u: Uint8Array): `0x${string}` => toHex(u);
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

function cast(args: string[]): string {
  return execFileSync("cast", args, { encoding: "utf8" }).trim();
}
function castSend(extra: string[]): void {
  cast([
    "send",
    CONTRACT,
    ...extra,
    "--private-key",
    EVM_KEY,
    "--rpc-url",
    EVM_RPC,
  ]);
}
const castUint = (sig: string): bigint =>
  BigInt(
    cast(["call", CONTRACT, sig, "--rpc-url", EVM_RPC])
      .split("\n")[0]
      .split(" ")[0]
  );
async function evmStatus(id: `0x${string}`): Promise<number> {
  const out = cast([
    "call",
    CONTRACT,
    "matches(bytes32)(uint8,address,address,address,bool,bool,bool,uint128,uint128,uint64,uint64,uint64,uint64,bytes32,uint8,uint8)",
    id,
    "--rpc-url",
    EVM_RPC,
  ]);
  return Number(out.split("\n")[0].trim());
}
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
function randBytes32(): Uint8Array {
  randCounter += 1;
  const seed = BigInt(Date.now()) * 1_000_000n + BigInt(randCounter);
  return toBytes(viemKeccak(toBytes(`0x${seed.toString(16)}`)));
}

// ---- EVM settle ABI (nested tuples) ---------------------------------------
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
// Permissionless lockTranche(cert, operatorSig) — operator's match-live sig authorizes.
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
// Full cert → Anchor MatchLiveCertArg (lock takes the full cert; leg A's tranche
// isn't on-chain pre-lock so it can't be rebuilt).
function toCertArg(c: MatchLiveCert) {
  const leg = (l: MatchLiveCert["legA"]) => ({
    chainTag: Array.from(l.chainTag),
    contract: Array.from(l.contract),
    player: Array.from(l.player),
    sessionKey: Array.from(l.sessionKey),
    stake: new BN(l.stake.toString()),
    tranche: new BN(l.tranche.toString()),
  });
  return {
    matchId: Array.from(c.matchId),
    tournamentId: new BN(c.tournamentId.toString()),
    matchupCommitment: Array.from(c.matchupCommitment),
    legA: leg(c.legA),
    legB: leg(c.legB),
    quoteTimestamp: new BN(c.quoteTimestamp.toString()),
    quoteMaxAgeSecs: Number(c.quoteMaxAgeSecs),
    matchDeadline: new BN(c.matchDeadline.toString()),
    claimWindowSecs: Number(c.claimWindowSecs),
    aIsP1: Number(c.aIsP1),
  };
}
function legForAbi(l: MatchLiveCert["legA"]) {
  return {
    chainTag: b32(l.chainTag),
    contractId: b32(l.contract),
    player: b32(l.player),
    sessionKey: toHex(l.sessionKey),
    stake: l.stake,
    tranche: l.tranche,
  };
}
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

(LIVE ? describe : describe.skip)(
  "live cross-chain settle (Solana devnet + Base Sepolia)",
  function () {
    this.timeout(600_000);

    it("funds + locks + settles ONE cert across both chains with conservation", async () => {
      // ---- keys ----
      const operator = signerFromHex(OPERATOR_SK);
      const player = loadKeypair(
        readFileSync(`${homedir()}/.config/solana/id.json`, "utf8")
      );
      const matchmaker = loadKeypair(MATCHMAKER_KEY);
      const evmPlayer = privateKeyToAccount(
        (EVM_KEY.startsWith("0x") ? EVM_KEY : `0x${EVM_KEY}`) as `0x${string}`
      );

      const legA = newSessionSigner(); // Solana leg session key
      const legB = newSessionSigner(); // EVM leg session key

      // ---- Anchor program (Solana) ----
      const connection = new Connection(SOLANA_RPC, "confirmed");
      const provider = new AnchorProvider(connection, new Wallet(player), {
        commitment: "confirmed",
      });
      const idl = JSON.parse(readFileSync(IDL_PATH, "utf8")) as Idl;
      const program = new Program(idl, provider);
      const m = program.methods as unknown as Record<
        string,
        (...a: unknown[]) => {
          accounts: (a: Record<string, PublicKey>) => {
            signers: (s: Keypair[]) => { rpc: () => Promise<string> };
            rpc: () => Promise<string>;
          };
        }
      >;

      const matchId = randBytes32();
      const matchIdHex = b32(matchId);
      const matchIdArr = Array.from(matchId);
      const [xmatchPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("xmatch"), Buffer.from(matchId)],
        PROGRAM_ID
      );
      const [xpoolPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("xpool")],
        PROGRAM_ID
      );
      const [globalConfigPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("global_config")],
        PROGRAM_ID
      );
      const [tournamentPda] = PublicKey.findProgramAddressSync(
        [
          Buffer.from("tournament"),
          new BN(TOURNAMENT_ID.toString()).toArrayLike(Buffer, "le", 8),
        ],
        PROGRAM_ID
      );
      const [playerProfilePda] = PublicKey.findProgramAddressSync(
        [
          Buffer.from("player"),
          new BN(TOURNAMENT_ID.toString()).toArrayLike(Buffer, "le", 8),
          player.publicKey.toBuffer(),
        ],
        PROGRAM_ID
      );

      const evmStake = castUint("stakeWei()(uint256)");
      const evmTranche = evmStake; // <= maxTranche
      const solTranche = SOL_STAKE; // <= xpool max_tranche (0.1 SOL)
      const evmChainTag = cast([
        "call",
        CONTRACT,
        "CHAIN_TAG()(bytes32)",
        "--rpc-url",
        EVM_RPC,
      ]) as `0x${string}`;
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const gcAccount: any = await (program.account as any).globalConfig.fetch(
        globalConfigPda
      );
      const treasury = gcAccount.treasury as PublicKey;

      const nowSol = Math.floor(Date.now() / 1000);
      const fundDeadline = nowSol + 3600;
      const matchDeadline = nowSol + 7200;

      // a_is_p1 = 1 (Solana player is P1). EVM playerIsP1 must be the opposite
      // ((aIsP1==1) != playerIsP1), so the EVM leg funds with playerIsP1=false.
      // ---- Solana leg: create_xmatch (matchmaker-cosigned) ----
      await m
        .createXmatch(matchIdArr, {
          tournamentId: new BN(TOURNAMENT_ID.toString()),
          playerIsP1: true,
          sessionKey: Array.from(legA.address),
          counterSessionKey: Array.from(legB.address),
          stakeLamports: new BN(SOL_STAKE.toString()),
          fundDeadline: new BN(fundDeadline),
          matchDeadline: new BN(matchDeadline),
        })
        .accounts({
          xmatch: xmatchPda,
          globalConfig: globalConfigPda,
          matchmaker: matchmaker.publicKey,
          player: player.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([matchmaker])
        .rpc();

      // ---- EVM leg: createMatch (playerIsP1=false) ----
      castSend([
        "createMatch(bytes32,address,address,bool,uint64,uint64)",
        matchIdHex,
        toHex(legB.address),
        toHex(legA.address),
        "false",
        String(fundDeadline),
        String(matchDeadline),
        "--value",
        evmStake.toString(),
      ]);
      await poll(
        () => evmStatus(matchIdHex),
        (s) => s === 1,
        "EVM Funded"
      );

      // ---- ONE canonical cert: legA = Solana state, legB = EVM state.
      // Built BEFORE the lock now: the operator's match-live signature over this
      // cert is what authorizes the permissionless lock on each leg. ----
      const cert: MatchLiveCert = {
        matchId,
        tournamentId: TOURNAMENT_ID,
        matchupCommitment: randBytes32(),
        legA: {
          chainTag: toBytes(
            viemKeccak(new TextEncoder().encode(SOLANA_CHAIN_ID))
          ),
          contract: PROGRAM_ID.toBytes(),
          player: player.publicKey.toBytes(),
          sessionKey: legA.address,
          stake: SOL_STAKE,
          tranche: solTranche,
        },
        legB: {
          chainTag: toBytes(evmChainTag),
          contract: toBytes(pad(CONTRACT, { size: 32 })),
          player: toBytes(pad(evmPlayer.address, { size: 32 })),
          sessionKey: legB.address,
          stake: evmStake,
          tranche: evmTranche,
        },
        quoteTimestamp: BigInt(nowSol - 60),
        quoteMaxAgeSecs: 3600,
        matchDeadline: BigInt(matchDeadline),
        claimWindowSecs: 3600,
        aIsP1: 1,
      };
      const liveDigest = matchLiveDigest(cert);

      // ---- lock both tranches (PERMISSIONLESS: the operator's match-live sig
      // authorizes; a non-operator cranker submits + pays the fee) ----
      // Solana: cranker = player, NOT the operator — proves the lock is permissionless.
      await m
        .lockXtranche(toCertArg(cert), signSol(operator, liveDigest))
        .accounts({
          xmatch: xmatchPda,
          pool: xpoolPda,
          cranker: player.publicKey,
        })
        .signers([player])
        .rpc();
      // EVM: lockTranche(cert, operatorSig) — submitted by the cranker (cast key).
      const lockData = encodeFunctionData({
        abi: LOCK_ABI,
        functionName: "lockTranche",
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        args: [certForAbi(cert) as any, signEvm(operator, liveDigest) as any],
      });
      castSend([lockData]);
      await poll(
        () => evmStatus(matchIdHex),
        (s) => s === 2,
        "EVM Locked"
      );

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

      // ---- EVM settle (v=27/28) ----
      const evmLive = [
        signEvm(legA, liveDigest),
        signEvm(legB, liveDigest),
        signEvm(operator, liveDigest),
      ];
      const evmOc = [
        signEvm(legA, ocDigest),
        signEvm(legB, ocDigest),
        signEvm(operator, ocDigest),
      ];
      const data = encodeFunctionData({
        abi: SETTLE_ABI,
        functionName: "settle",
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        args: [
          certForAbi(cert) as any,
          ocForAbi(oc) as any,
          evmLive as any,
          evmOc as any,
        ],
      });
      castSend([data]);
      await poll(
        () => evmStatus(matchIdHex),
        (s) => s === 4,
        "EVM Settled"
      );

      // ---- Solana settle_xmatch (v=recid 0/1; legA rebuilt on-chain) ----
      const solLive = [
        signSol(legA, liveDigest),
        signSol(legB, liveDigest),
        signSol(operator, liveDigest),
      ];
      const solOc = [
        signSol(legA, ocDigest),
        signSol(legB, ocDigest),
        signSol(operator, ocDigest),
      ];
      const certNoA = {
        matchId: matchIdArr,
        tournamentId: new BN(TOURNAMENT_ID.toString()),
        matchupCommitment: Array.from(cert.matchupCommitment),
        legB: {
          chainTag: Array.from(cert.legB.chainTag),
          contract: Array.from(cert.legB.contract),
          player: Array.from(cert.legB.player),
          sessionKey: Array.from(cert.legB.sessionKey),
          stake: new BN(cert.legB.stake.toString()),
          tranche: new BN(cert.legB.tranche.toString()),
        },
        quoteTimestamp: new BN(cert.quoteTimestamp.toString()),
        quoteMaxAgeSecs: cert.quoteMaxAgeSecs,
        matchDeadline: new BN(cert.matchDeadline.toString()),
        claimWindowSecs: cert.claimWindowSecs,
        aIsP1: cert.aIsP1,
      };
      const outcomeArg = {
        matchId: matchIdArr,
        matchLiveDigest: Array.from(liveDigest),
        outcomeKind: oc.outcomeKind,
        stepCount: oc.stepCount,
        p1Guess: oc.p1Guess,
        p2Guess: oc.p2Guess,
        firstCommitter: oc.firstCommitter,
        matchupType: oc.matchupType,
        transcriptHash: Array.from(oc.transcriptHash),
      };
      await m
        .settleXmatch(certNoA, outcomeArg, solLive, solOc)
        .accounts({
          xmatch: xmatchPda,
          pool: xpoolPda,
          tournament: tournamentPda,
          playerProfile: playerProfilePda,
          globalConfig: globalConfigPda,
          treasury,
          player: player.publicKey,
          cranker: player.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .rpc();

      // ---- assert both legs settled ----
      expect(await evmStatus(matchIdHex)).to.equal(4); // EVM Settled
      const xmatch = await (program.account as any).xChainMatch.fetch(
        xmatchPda
      );
      // XChainStatus::Settled — the enum's Settled variant.
      expect(Object.keys(xmatch.status)[0].toLowerCase()).to.contain("settled");
    });
  }
);
