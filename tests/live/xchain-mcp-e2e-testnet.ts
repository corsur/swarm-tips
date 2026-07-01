/**
 * Live MCP / agent-path cross-chain e2e: drives the DEPLOYED game-api's
 * cross-chain matchmaking + cosign + checkpoint relay + outcome-cosign (the
 * backend the MCP tools proxy), then settles ON-CHAIN on both legs.
 *
 * Unlike the protocol e2e (xchain-cross-settle-testnet.ts), which builds the
 * cert locally, this proves the DEPLOYED agent path end-to-end:
 *
 *   register-free xqueue join (Solana + EVM) -> game-api pairs + operator-cosigns
 *     the MatchLiveCert -> build-sol-fund (matchmaker-cosigned create_xmatch) +
 *     createMatch (EVM) -> lock both tranches -> relay a co-signed terminal
 *     Checkpoint -> outcome-cosign (operator derives + signs the OutcomeCert) ->
 *     players co-sign live+outcome digests -> settle (EVM) + settle_xmatch
 *     (Solana) -> assert both Settled.
 *
 * Cross-checks that game-api's cert encoding (chain-core) is byte-identical to
 * the TS helper and to what both chains verify: we reconstruct the cert from
 * the relay payload and assert matchLiveDigest(cert) == payload.match_live_digest.
 *
 * MANUAL-ONLY (live network + real testnet funds). Self-skips without creds.
 * Run:
 *   export OPERATOR_SK=$(gcloud secrets versions access latest --secret=xchain-operator-signer --project=coordination-game-prod)
 *   export EVM_KEY=$(cat ~/.foundry/keystores/xchain-testnet.key)
 *   npx ts-mocha -p ./tsconfig.json tests/live/xchain-mcp-e2e-testnet.ts --timeout 600000
 */
import { execFileSync } from "child_process";
import { readFileSync } from "fs";
import { homedir } from "os";
import { createHash, randomBytes } from "crypto";
import { expect } from "chai";
import { Connection, Keypair, PublicKey, SystemProgram } from "@solana/web3.js";
import {
  AnchorProvider,
  Program,
  Wallet,
  BN,
  type Idl,
} from "@coral-xyz/anchor";
import { encodeFunctionData, toBytes, toHex, bytesToHex, fromHex } from "viem";
import { privateKeyToAccount } from "viem/accounts";
import {
  newSessionSigner,
  signDigest,
  keccak256,
  matchLiveDigest,
  checkpointDigest,
  outcomeDigest,
  type SessionSigner,
  type MatchLiveCert,
  type Checkpoint,
  type OutcomeCert,
} from "../helpers/xchain-cert";
import { deriveTerminalOutcome } from "../helpers/outcome-oracle";

const GAME_API = process.env.GAME_API ?? "https://api.coordination.game";
const SOLANA_RPC =
  process.env.SOLANA_RPC_URL ?? "https://api.devnet.solana.com";
const EVM_RPC = process.env.RPC_URL ?? "https://sepolia.base.org";
const PROGRAM_ID = new PublicKey(
  "2qqVk7kUqffnahiJpcQJCsSd8ErbEUgKTgCn1zYsw64P"
);
const CONTRACT = (process.env.CONTRACT ??
  "0xd38b1fB07Bf64801bCBc3721937D6e2Ba6E5feb4") as `0x${string}`;
const SOLANA_CHAIN = "solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1";
const EVM_CHAIN = "eip155:84532";
const IDL_PATH =
  process.env.IDL_PATH ??
  `${homedir()}/swarm/swarm-tips-repo/target/idl/coordination_game.json`;
const TOURNAMENT_ID = Number(process.env.TOURNAMENT_ID ?? "2");
const TERMINAL_STEP_COUNT = 4;

const OPERATOR_SK = process.env.OPERATOR_SK ?? "";
const EVM_KEY = (process.env.EVM_KEY ?? "").trim();
const LIVE = Boolean(OPERATOR_SK && EVM_KEY);

// ---- helpers ----
function signerFromHex(hex: string): SessionSigner {
  const clean = hex.startsWith("0x") ? hex.slice(2) : hex;
  const acct = privateKeyToAccount(`0x${clean}`);
  return { privateKey: toBytes(`0x${clean}`), address: toBytes(acct.address) };
}
const signEvm = (s: SessionSigner, d: Uint8Array): `0x${string}` => {
  const out = Uint8Array.from(signDigest(s, d));
  out[64] += 27;
  return bytesToHex(out);
};
const signSol = (s: SessionSigner, d: Uint8Array): number[] =>
  Array.from(signDigest(s, d));
const w32 = (hex: string): Uint8Array =>
  fromHex(hex as `0x${string}`, { to: "bytes", size: 32 });
const w20 = (hex: string): Uint8Array =>
  fromHex(hex as `0x${string}`, { to: "bytes", size: 20 });
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
/** A 32-byte guess preimage whose last bit encodes the guess. */
const preimage = (guess: number): Uint8Array => {
  const r = randomBytes(32);
  r[31] = (r[31] & 0xfe) | (guess & 1);
  return Uint8Array.from(r);
};
const sha256_0x = (b: Uint8Array): string =>
  "0x" + createHash("sha256").update(b).digest("hex");
/** Sign a `0x` 32-byte digest → `0x` 65-byte [r||s||v=0|1] (relay's recover form). */
const sig0x = (s: SessionSigner, digestHex: string): string =>
  bytesToHex(Uint8Array.from(signDigest(s, w32(digestHex))));

function cast(args: string[]): string {
  return execFileSync("cast", args, { encoding: "utf8" }).trim();
}
const castSend = (extra: string[]): void => {
  cast([
    "send",
    CONTRACT,
    ...extra,
    "--private-key",
    EVM_KEY,
    "--rpc-url",
    EVM_RPC,
  ]);
};
async function evmStatus(id: string): Promise<number> {
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
  for (let i = 0; i < 25; i += 1) {
    const v = await read();
    if (ok(v)) return v;
    await sleep(1500);
  }
  throw new Error(`poll timed out: ${label}`);
}
async function api(path: string, body?: unknown): Promise<any> {
  const res = await fetch(`${GAME_API}${path}`, {
    method: body ? "POST" : "GET",
    headers: { "Content-Type": "application/json" },
    body: body ? JSON.stringify(body) : undefined,
  });
  const text = await res.text();
  if (!res.ok) throw new Error(`${path} -> HTTP ${res.status}: ${text}`);
  return text ? JSON.parse(text) : {};
}

/** Clear any stale cross-chain queue/pending/checkpoint docs so a prior run's
 *  leftover pending match (with an expired fund deadline) can't be re-paired
 *  here — otherwise create_xmatch fails with XDeadlinePassed. Mirrors the
 *  Playwright xchain test's clearQueue. */
async function clearXQueue(): Promise<void> {
  const token = execFileSync("gcloud", ["auth", "print-access-token"], {
    encoding: "utf8",
  }).trim();
  const fs =
    "https://firestore.googleapis.com/v1/projects/coordination-game-prod/databases/(default)/documents";
  for (const col of [
    "xmatch_queue_entries",
    "xmatch_pending",
    "xmatch_checkpoint",
  ]) {
    const res = await fetch(`${fs}/${col}`, {
      headers: { Authorization: `Bearer ${token}` },
    });
    const j = (await res.json()) as { documents?: { name: string }[] };
    for (const d of j.documents ?? []) {
      const path = d.name.split("/documents/")[1];
      await fetch(`${fs}/${path}`, {
        method: "DELETE",
        headers: { Authorization: `Bearer ${token}` },
      });
    }
  }
}

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
const legAbi = (l: MatchLiveCert["legA"]) => ({
  chainTag: toHex(l.chainTag),
  contractId: toHex(l.contract),
  player: toHex(l.player),
  sessionKey: toHex(l.sessionKey),
  stake: l.stake,
  tranche: l.tranche,
});
const certAbi = (c: MatchLiveCert) => ({
  matchId: toHex(c.matchId),
  tournamentId: c.tournamentId,
  matchupCommitment: toHex(c.matchupCommitment),
  legA: legAbi(c.legA),
  legB: legAbi(c.legB),
  quoteTimestamp: c.quoteTimestamp,
  quoteMaxAgeSecs: c.quoteMaxAgeSecs,
  matchDeadline: c.matchDeadline,
  claimWindowSecs: c.claimWindowSecs,
  aIsP1: c.aIsP1,
});
const ocAbi = (o: OutcomeCert) => ({
  matchId: toHex(o.matchId),
  matchLiveDigest: toHex(o.matchLiveDigest),
  outcomeKind: o.outcomeKind,
  stepCount: o.stepCount,
  p1Guess: o.p1Guess,
  p2Guess: o.p2Guess,
  firstCommitter: o.firstCommitter,
  matchupType: o.matchupType,
  transcriptHash: toHex(o.transcriptHash),
});

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

/** Reconstruct the canonical MatchLiveCert from game-api's relay payload. */
function certFromPayload(p: any): MatchLiveCert {
  const leg = (l: any) => ({
    // chain_tag = keccak256(CAIP-2 chain id) — game-api's xchain::chain_tag().
    chainTag: keccak256(new TextEncoder().encode(l.chain)),
    contract: w32(l.contract),
    player: w32(l.player),
    sessionKey: w20(l.session_key),
    stake: BigInt(l.stake_base_units),
    tranche: BigInt(l.tranche_base_units),
  });
  return {
    matchId: w32(p.match_id),
    tournamentId: BigInt(p.tournament_id),
    matchupCommitment: w32(p.matchup_commitment ?? `0x${"00".repeat(32)}`),
    legA: leg(p.leg_a),
    legB: leg(p.leg_b),
    quoteTimestamp: BigInt(p.quote_timestamp),
    quoteMaxAgeSecs: p.quote_max_age_secs,
    matchDeadline: BigInt(p.match_deadline),
    claimWindowSecs: p.claim_window_secs,
    aIsP1: p.a_is_p1,
  };
}

(LIVE ? describe : describe.skip)(
  "live MCP cross-chain e2e (deployed game-api)",
  function () {
    this.timeout(600_000);

    it("matchmaking -> fund -> relay -> outcome-cosign -> settle both legs", async () => {
      const operator = signerFromHex(OPERATOR_SK);
      const player = Keypair.fromSecretKey(
        Uint8Array.from(
          JSON.parse(
            readFileSync(`${homedir()}/.config/solana/id.json`, "utf8")
          ) as number[]
        )
      );
      const evmPlayer = privateKeyToAccount(
        (EVM_KEY.startsWith("0x") ? EVM_KEY : `0x${EVM_KEY}`) as `0x${string}`
      );
      const solWallet = player.publicKey.toBase58();
      const legA = newSessionSigner(); // Solana leg session key
      const legB = newSessionSigner(); // EVM leg session key

      // 0) Clear stale queue/pending state so a prior run's leftover (expired)
      //    pending match can't be re-paired here (→ XDeadlinePassed on fund).
      await clearXQueue();

      // 1) Both wallets join the cross-chain queue → game-api pairs + cosigns.
      await api("/internal/xqueue/join", {
        wallet: evmPlayer.address,
        chain: EVM_CHAIN,
        session_key: toHex(legB.address),
        tournament_id: TOURNAMENT_ID,
      });
      const joined = await api("/internal/xqueue/join", {
        wallet: solWallet,
        chain: SOLANA_CHAIN,
        session_key: toHex(legA.address),
        tournament_id: TOURNAMENT_ID,
      });
      expect(joined.status).to.equal("matched");
      const payload = joined.match;
      const matchIdHex = payload.match_id as string;

      // Cross-check: our TS cert encoding agrees with game-api's cosigned digest.
      const cert = certFromPayload(payload);
      expect(toHex(matchLiveDigest(cert))).to.equal(payload.match_live_digest);

      // 2) Fund Solana leg via the matchmaker-cosigned build-sol-fund.
      const solFund = await api("/internal/xqueue/build-sol-fund", {
        wallet: solWallet,
      });
      const connection = new Connection(SOLANA_RPC, "confirmed");
      const provider = new AnchorProvider(connection, new Wallet(player), {
        commitment: "confirmed",
      });
      const idl = JSON.parse(readFileSync(IDL_PATH, "utf8")) as Idl;
      const program = new Program(idl, provider);
      await submitMatchmakerCosigned(connection, player, solFund);

      // 3) Fund EVM leg (playerIsP1 = a_is_p1 == 0).
      const evmStake = BigInt(payload.leg_b.stake_base_units);
      castSend([
        "createMatch(bytes32,address,address,bool,uint64,uint64)",
        matchIdHex,
        toHex(legB.address),
        toHex(legA.address),
        payload.a_is_p1 === 0 ? "true" : "false",
        String(payload.match_deadline - 300),
        String(payload.match_deadline),
        "--value",
        evmStake.toString(),
      ]);
      await poll(
        () => evmStatus(matchIdHex),
        (s) => s === 1,
        "EVM Funded"
      );

      // 4) Lock both tranches (harness holds the operator/owner keys).
      const matchId = w32(matchIdHex);
      const matchIdArr = Array.from(matchId);
      const [xmatchPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("xmatch"), Buffer.from(matchId)],
        PROGRAM_ID
      );
      const [xpoolPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("xpool")],
        PROGRAM_ID
      );
      const m = program.methods as any;
      // PERMISSIONLESS lock: the operator's match-live signature is ALREADY in the
      // relay payload (game-api cosigned it at pairing) — the lock reuses that exact
      // sig, no new operator action. A non-operator cranker submits + pays.
      const opLiveBytes = fromHex(
        payload.operator_signature as `0x${string}`,
        "bytes"
      ); // recid (0/1)
      await m
        .lockXtranche(toCertArg(cert), Array.from(opLiveBytes))
        .accounts({
          xmatch: xmatchPda,
          pool: xpoolPda,
          cranker: player.publicKey,
        })
        .signers([player])
        .rpc();
      const opLiveEvm = Uint8Array.from(opLiveBytes);
      if (opLiveEvm[64] < 27) opLiveEvm[64] += 27; // OZ ECDSA wants v=27/28
      castSend([
        encodeFunctionData({
          abi: LOCK_ABI,
          functionName: "lockTranche",
          args: [certAbi(cert) as any, bytesToHex(opLiveEvm) as any],
        }),
      ]);
      await poll(
        () => evmStatus(matchIdHex),
        (s) => s === 2,
        "EVM Locked"
      );

      // 5) Play the REAL gameplay relay: commit → co-sign step-2 (releases the
      //    matchmaker's secret r_matchup) → reveal → co-sign the terminal
      //    checkpoint. The relay stores the matchup-bound terminal checkpoint
      //    server-side from the REAL transcript — no fabrication, so the binding
      //    holds by construction.
      const preA = preimage(1);
      const preB = preimage(0);
      await api("/internal/xqueue/commit", {
        wallet: solWallet,
        commit: sha256_0x(preA),
      });
      const committed = await api("/internal/xqueue/commit", {
        wallet: evmPlayer.address,
        commit: sha256_0x(preB),
      });
      expect(committed.both_committed).to.equal(true);

      const gpCommit = await api(
        `/internal/xqueue/gameplay?wallet=${solWallet}`
      );
      await api("/internal/xqueue/sign", {
        wallet: solWallet,
        step: 2,
        signature: sig0x(legA, gpCommit.step2_checkpoint_digest),
      });
      const signed2 = await api("/internal/xqueue/sign", {
        wallet: evmPlayer.address,
        step: 2,
        signature: sig0x(legB, gpCommit.step2_checkpoint_digest),
      });
      expect(signed2.r_matchup, "r_matchup released after step-2").to.be.a(
        "string"
      );

      await api("/internal/xqueue/reveal", {
        wallet: solWallet,
        preimage: bytesToHex(preA),
      });
      const revealed = await api("/internal/xqueue/reveal", {
        wallet: evmPlayer.address,
        preimage: bytesToHex(preB),
      });
      expect(revealed.both_revealed).to.equal(true);

      const gpTerm = await api(`/internal/xqueue/gameplay?wallet=${solWallet}`);
      await api("/internal/xqueue/sign", {
        wallet: solWallet,
        step: 4,
        signature: sig0x(legA, gpTerm.terminal_checkpoint_digest),
      });
      const relayed = await api("/internal/xqueue/sign", {
        wallet: evmPlayer.address,
        step: 4,
        signature: sig0x(legB, gpTerm.terminal_checkpoint_digest),
      });
      expect(
        relayed.relayed,
        "terminal checkpoint relayed (matchup bound)"
      ).to.equal(true);

      // 6) Operator-cosign the outcome (game-api derives it from the stored
      //    terminal checkpoint). Assert it against the oracle on the REALIZED
      //    transcript — derived, never hardcoded (matchup type is the
      //    matchmaker's, revealed only at step 2).
      const ocResp = await api("/internal/xqueue/outcome-cosign", {
        wallet: solWallet,
      });
      const expectedOutcome = deriveTerminalOutcome({
        stepCount: TERMINAL_STEP_COUNT,
        matchupType: ocResp.matchup_type,
        p1Guess: ocResp.p1_guess,
        p2Guess: ocResp.p2_guess,
        firstCommitter: ocResp.first_committer,
      });
      expect(ocResp.outcome_kind).to.equal(expectedOutcome);
      const oc: OutcomeCert = {
        matchId,
        matchLiveDigest: w32(payload.match_live_digest),
        outcomeKind: ocResp.outcome_kind,
        stepCount: ocResp.step_count,
        p1Guess: ocResp.p1_guess,
        p2Guess: ocResp.p2_guess,
        firstCommitter: ocResp.first_committer,
        matchupType: ocResp.matchup_type,
        transcriptHash: w32(ocResp.transcript_hash),
      };
      expect(toHex(outcomeDigest(oc))).to.equal(ocResp.outcome_digest);

      // 7) Settle both legs with the 6 sigs. Operator sigs come from game-api.
      const liveDigest = w32(payload.match_live_digest);
      const ocDigest = w32(ocResp.outcome_digest);
      const opLive = payload.operator_signature as string;
      const opOc = ocResp.operator_outcome_signature as string;
      const bump27 = (hex: string): `0x${string}` => {
        const b = fromHex(hex as `0x${string}`, "bytes");
        if (b[64] < 27) b[64] += 27;
        return bytesToHex(b);
      };
      const evmLive = [
        signEvm(legA, liveDigest),
        signEvm(legB, liveDigest),
        bump27(opLive),
      ];
      const evmOc = [
        signEvm(legA, ocDigest),
        signEvm(legB, ocDigest),
        bump27(opOc),
      ];
      castSend([
        encodeFunctionData({
          abi: SETTLE_ABI,
          functionName: "settle",
          args: [
            certAbi(cert) as any,
            ocAbi(oc) as any,
            evmLive as any,
            evmOc as any,
          ],
        }),
      ]);
      await poll(
        () => evmStatus(matchIdHex),
        (s) => s === 4,
        "EVM Settled"
      );

      // Solana settle_xmatch (recid 0/1; operator sigs from game-api, recid form).
      const opLiveSol = Array.from(fromHex(opLive as `0x${string}`, "bytes"));
      const opOcSol = Array.from(fromHex(opOc as `0x${string}`, "bytes"));
      const solLive = [
        signSol(legA, liveDigest),
        signSol(legB, liveDigest),
        opLiveSol,
      ];
      const solOc = [signSol(legA, ocDigest), signSol(legB, ocDigest), opOcSol];
      const [tournamentPda] = PublicKey.findProgramAddressSync(
        [
          Buffer.from("tournament"),
          new BN(TOURNAMENT_ID).toArrayLike(Buffer, "le", 8),
        ],
        PROGRAM_ID
      );
      const [playerProfilePda] = PublicKey.findProgramAddressSync(
        [
          Buffer.from("player"),
          new BN(TOURNAMENT_ID).toArrayLike(Buffer, "le", 8),
          player.publicKey.toBuffer(),
        ],
        PROGRAM_ID
      );
      const [globalConfigPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("global_config")],
        PROGRAM_ID
      );
      const gc: any = await (program.account as any).globalConfig.fetch(
        globalConfigPda
      );
      const certNoA = {
        matchId: matchIdArr,
        tournamentId: new BN(payload.tournament_id),
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
          treasury: gc.treasury,
          player: player.publicKey,
          cranker: player.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .rpc();

      expect(await evmStatus(matchIdHex)).to.equal(4);
      const xmatch: any = await (program.account as any).xChainMatch.fetch(
        xmatchPda
      );
      expect(Object.keys(xmatch.status)[0].toLowerCase()).to.contain("settled");
    });
  }
);

/** Assemble + submit the matchmaker-cosigned create_xmatch tx from build-sol-fund. */
async function submitMatchmakerCosigned(
  connection: Connection,
  player: Keypair,
  solFund: any
): Promise<void> {
  const { Transaction } = await import("@solana/web3.js");
  // build-sol-fund returns a full legacy Transaction with empty signature slots
  // (one per required signer: the matchmaker + the player) plus the matchmaker's
  // detached signature. Player signs its slot; the matchmaker slot is the other
  // signer — fill it with the server-provided sig.
  const tx = Transaction.from(Buffer.from(solFund.unsigned_tx, "base64"));
  tx.partialSign(player);
  const mmSlot = tx.signatures.find(
    (s) => !s.publicKey.equals(player.publicKey)
  );
  if (!mmSlot)
    throw new Error("build-sol-fund tx has no matchmaker signer slot");
  tx.addSignature(
    mmSlot.publicKey,
    Buffer.from(solFund.matchmaker_signature, "base64")
  );
  await connection.sendRawTransaction(tx.serialize(), { skipPreflight: false });
}
