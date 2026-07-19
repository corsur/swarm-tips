/**
 * Live same-chain SOLANA human-vs-GROK e2e — the proof that the flagship
 * coordination game still resolves on the just-upgraded MAINNET program
 * (the build-flag change to `solana_chain_tag`).
 *
 * One funded human wallet (id.json) authenticates, connects the matchmaking
 * WebSocket, and joins the tournament queue ALONE. game-api sees a human in an
 * empty queue and fires the AI fallback (~20-40s), which spawns grok-agent;
 * grok joins the SAME queue from its own pool wallet and the two are paired.
 *
 * The matchmaker CREATES the game on-chain implicitly by co-signing the
 * creator's `create_game` (players never call `initialize`/`create` alone).
 * Roles: the pre-existing waiter (our human) is P1/creator (role 0); grok, the
 * arriving player, is P2/joiner (role 1) — grok-agent only ever joins, never
 * creates, so the human is always the creator in the fallback. The human drives
 * deposit_stake -> create_game (matchmaker co-sign) -> commit_guess ->
 * reveal_guess; grok drives deposit_stake -> join_game -> commit -> reveal
 * autonomously (running its ~3-4min persona chat loop before committing). We
 * assert the on-chain Game PDA reaches `Resolved` with both guesses set and
 * player_two is grok's wallet (i.e. the opponent was the AI, not another human).
 *
 * NOT CI (real ~0.05 SOL ante + a live grok spawn). Self-skips (exit 0) when
 * SOLANA_RPC_URL or the keyfile are absent. Run:
 *   GAME_API=https://api.coordination.game \
 *   SOLANA_RPC_URL=https://mainnet.helius-rpc.com/?api-key=... \
 *   A_KEYFILE=~/.config/solana/id.json TOURNAMENT_ID=2 \
 *   npx ts-mocha -p tsconfig.json tests/live/solanagame-grok-mainnet.ts --timeout 480000
 */
import { readFileSync, existsSync } from "fs";
import { createHash, randomBytes } from "crypto";
import { Program, AnchorProvider, Wallet, BN } from "@coral-xyz/anchor";
import {
  Connection,
  Keypair,
  PublicKey,
  Transaction,
  SystemProgram,
} from "@solana/web3.js";
import { assert } from "chai";
import type { CoordinationGame } from "../../target/types/coordination_game";

// The runtime IDL (JSON) drives Program construction; the `.ts` sibling is a
// type-only helper. `require` avoids needing `resolveJsonModule` in tsconfig.
// eslint-disable-next-line @typescript-eslint/no-var-requires
const IDL =
  require("../../target/idl/coordination_game.json") as CoordinationGame;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

const GAME_API = process.env.GAME_API ?? "https://api.coordination.game";
const RPC = process.env.SOLANA_RPC_URL ?? "";
const KEYFILE = (process.env.A_KEYFILE ?? "~/.config/solana/id.json").replace(
  "~",
  process.env.HOME ?? ""
);
// Mainnet default: tournament 2 (tournament 1 ended; #2 created 2026-05-08,
// 90-day window) — the value the game frontend defaults to on mainnet
// (frontend constants.ts `TOURNAMENT_ID`). Devnet uses 1003.
const TOURNAMENT_ID = Number(process.env.TOURNAMENT_ID ?? 2);
// 0.05 SOL — FIXED_STAKE_LAMPORTS in the coordination-game program.
const STAKE_LAMPORTS = 50_000_000;
const HUMAN_GUESS: 0 | 1 = 1; // "different" — the safe default guess

const TID = new BN(TOURNAMENT_ID);
const STAKE = new BN(STAKE_LAMPORTS);

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

const hexToBytes = (hex: string): number[] =>
  Array.from(Buffer.from(hex.replace(/^0x/, ""), "hex"));

function generateCommit(guess: 0 | 1): { commitment: number[]; r: number[] } {
  const r = randomBytes(32);
  r[31] = (r[31] & 0xfe) | guess; // encode guess in the last bit
  const commitment = createHash("sha256").update(r).digest();
  return { commitment: Array.from(commitment), r: Array.from(r) };
}

async function api<T = any>(
  path: string,
  opts: { method?: string; body?: unknown; token?: string } = {}
): Promise<T> {
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
  };
  if (opts.token) headers.Authorization = `Bearer ${opts.token}`;
  const res = await fetch(`${GAME_API}${path}`, {
    method: opts.method ?? (opts.body ? "POST" : "GET"),
    headers,
    body: opts.body ? JSON.stringify(opts.body) : undefined,
  });
  const text = await res.text();
  if (!res.ok) throw new Error(`${path} -> HTTP ${res.status}: ${text}`);
  return (text ? JSON.parse(text) : {}) as T;
}

// ---------------------------------------------------------------------------
// PDA derivation (seeds mirror the on-chain program exactly)
// ---------------------------------------------------------------------------

function pda(seeds: (Buffer | Uint8Array)[]): PublicKey {
  return PublicKey.findProgramAddressSync(seeds, new PublicKey(IDL.address))[0];
}
const le8 = (n: BN) => n.toArrayLike(Buffer, "le", 8);
const gamePda = (id: BN) => pda([Buffer.from("game"), le8(id)]);
const tournamentPda = (id: BN) => pda([Buffer.from("tournament"), le8(id)]);
const escrowPda = (id: BN, w: PublicKey) =>
  pda([Buffer.from("escrow"), le8(id), w.toBuffer()]);
const playerProfilePda = (id: BN, w: PublicKey) =>
  pda([Buffer.from("player"), le8(id), w.toBuffer()]);
const globalConfigPda = () => pda([Buffer.from("global_config")]);
const gameCounterPda = () => pda([Buffer.from("game_counter")]);

// The name of the active GameState enum variant, e.g. "pending" | "active" |
// "committing" | "revealing" | "resolved". Anchor decodes the enum as a
// single-key object (`{ resolved: {} }`).
const stateName = (game: any): string => Object.keys(game.state)[0];
const MATCHUP_TYPE_UNSET = 255;

// ---------------------------------------------------------------------------
// Minimal WebSocket wrapper over the Node global `WebSocket` (no `ws` dep).
// Buffers every parsed server message so the flow can poll for the one it
// needs (match_found, game_ready, reveal_data) without a listener race.
// ---------------------------------------------------------------------------

interface WsLike {
  send(data: string): void;
  close(): void;
  onopen: ((ev: unknown) => void) | null;
  onmessage: ((ev: { data: unknown }) => void) | null;
  onerror: ((ev: unknown) => void) | null;
  onclose: ((ev: unknown) => void) | null;
}
type WsCtor = new (url: string) => WsLike;

class MatchWs {
  private ws: WsLike;
  private msgs: any[] = [];
  private keepalive: ReturnType<typeof setInterval> | null = null;

  private constructor(ws: WsLike) {
    this.ws = ws;
    ws.onmessage = (ev) => {
      try {
        this.msgs.push(JSON.parse(String(ev.data)));
      } catch {
        /* ignore non-JSON frames */
      }
    };
    this.keepalive = setInterval(() => {
      try {
        ws.send(JSON.stringify({ type: "ping" }));
      } catch {
        /* connection gone — polls will surface it */
      }
    }, 25_000);
  }

  static connect(gameApi: string, jwt: string): Promise<MatchWs> {
    const Ctor = (globalThis as unknown as { WebSocket: WsCtor }).WebSocket;
    assert(
      Ctor,
      "global WebSocket unavailable — need Node >= 20 with WebSocket"
    );
    const url = `${gameApi
      .replace(/^http:\/\//, "ws://")
      .replace(/^https:\/\//, "wss://")
      .replace(/\/$/, "")}/ws?token=${jwt}`;
    const ws = new Ctor(url);
    return new Promise((resolve, reject) => {
      ws.onopen = () => resolve(new MatchWs(ws));
      ws.onerror = (e) =>
        reject(new Error(`WebSocket error: ${JSON.stringify(e)}`));
    });
  }

  /** Poll the buffer for the first message matching `type`, up to `timeoutMs`. */
  async waitFor(type: string, timeoutMs: number): Promise<any> {
    const deadline = Date.now() + timeoutMs;
    for (;;) {
      const found = this.msgs.find((m) => m.type === type);
      if (found) return found;
      if (Date.now() > deadline)
        throw new Error(
          `timed out waiting for WS "${type}" after ${timeoutMs}ms`
        );
      await sleep(1000);
    }
  }

  close() {
    if (this.keepalive) clearInterval(this.keepalive);
    try {
      this.ws.close();
    } catch {
      /* already closed */
    }
  }
}

// ---------------------------------------------------------------------------
// On-chain polling
// ---------------------------------------------------------------------------

async function pollGameState(
  program: Program<CoordinationGame>,
  gameKey: PublicKey,
  wanted: string[],
  label: string,
  tries: number
): Promise<any> {
  for (let i = 0; i < tries; i++) {
    try {
      const game = await program.account.game.fetch(gameKey);
      if (wanted.includes(stateName(game))) return game;
    } catch {
      /* account may not be visible yet */
    }
    await sleep(3000);
  }
  throw new Error(
    `timed out waiting for game state ${wanted.join("|")} (${label})`
  );
}

async function pollRMatchup(
  token: string,
  sessionId: string,
  tries: number
): Promise<string> {
  for (let i = 0; i < tries; i++) {
    const s = await api(
      `/games/session-status?session_id=${encodeURIComponent(sessionId)}`,
      { token }
    );
    if (s.r_matchup) return s.r_matchup as string;
    await sleep(3000);
  }
  throw new Error("timed out waiting for r_matchup delivery (session-status)");
}

// ---------------------------------------------------------------------------
// Auth: non-custodial session auth. The deposit_stake tx (needed anyway) is
// signed by the human wallet, and its signature doubles as proof of ownership
// for POST /auth/session — no separate nonce-signing popup.
// ---------------------------------------------------------------------------

async function sessionAuth(
  wallet: string,
  txSignature: string
): Promise<string> {
  let lastErr: unknown;
  for (let i = 0; i < 12; i++) {
    try {
      const r = await api<{ token: string }>("/auth/session", {
        body: { wallet, tx_signature: txSignature },
      });
      if (r.token) return r.token;
    } catch (e) {
      lastErr = e; // tx may not be RPC-visible yet — retry
    }
    await sleep(3000);
  }
  throw new Error(`session auth failed: ${String(lastErr)}`);
}

// ---------------------------------------------------------------------------
// create_game with matchmaker co-sign (creator/P1 path). Mirrors the frontend
// game-tx.ts `buildCosignedCreateGameTx`: player slot blank + matchmaker slot
// filled from /games/cosign, then the player signs and submits.
// ---------------------------------------------------------------------------

async function createGameCosigned(
  program: Program<CoordinationGame>,
  connection: Connection,
  human: Keypair,
  matchmaker: PublicKey,
  token: string,
  matchupCommitment: number[],
  gameId: BN
): Promise<void> {
  const ix = await program.methods
    .createGame(STAKE, matchupCommitment as any)
    .accountsPartial({
      game: gamePda(gameId),
      gameCounter: gameCounterPda(),
      playerProfile: playerProfilePda(TID, human.publicKey),
      escrow: escrowPda(TID, human.publicKey),
      tournament: tournamentPda(TID),
      globalConfig: globalConfigPda(),
      matchmaker,
      player: human.publicKey,
      systemProgram: SystemProgram.programId,
    })
    .instruction();

  const tx = new Transaction().add(ix);
  const { blockhash } = await connection.getLatestBlockhash("confirmed");
  tx.recentBlockhash = blockhash;
  tx.feePayer = human.publicKey;
  // Signer order matches the compiled message: fee payer (human) first,
  // matchmaker (readonly signer) second.
  tx.signatures = [
    { publicKey: human.publicKey, signature: null },
    { publicKey: matchmaker, signature: null },
  ];

  const messageB64 = Buffer.from(tx.serializeMessage()).toString("base64");
  const { signature: mmSigB64 } = await api<{ signature: string }>(
    "/games/cosign",
    { body: { message: messageB64 }, token }
  );
  tx.signatures[1] = {
    publicKey: matchmaker,
    signature: Buffer.from(mmSigB64, "base64"),
  };
  tx.partialSign(human);

  const sig = await connection.sendRawTransaction(tx.serialize());
  await connection.confirmTransaction(sig, "confirmed");
  console.log(`[ok] create_game landed: ${sig}`);
}

/** Reveal, recomputing r_matchup from the live matchup_type each attempt so a
 * reveal race with grok (whoever reveals first supplies r_matchup) can't wedge
 * on RMatchupMismatch (6032). Returns once this player's guess is recorded. */
async function revealWithRace(
  program: Program<CoordinationGame>,
  human: Keypair,
  gameId: BN,
  preimage: number[],
  rMatchupHex: string,
  playerOne: PublicKey,
  playerTwo: PublicKey,
  treasury: PublicKey
): Promise<void> {
  const gameKey = gamePda(gameId);
  const revealAccounts = {
    game: gameKey,
    p1Profile: playerProfilePda(TID, playerOne),
    p2Profile: playerProfilePda(TID, playerTwo),
    tournament: tournamentPda(TID),
    playerOneWallet: playerOne,
    playerTwoWallet: playerTwo,
    globalConfig: globalConfigPda(),
    treasury,
    systemProgram: SystemProgram.programId,
  };
  const humanIsP1 = playerOne.equals(human.publicKey);

  for (let attempt = 1; attempt <= 4; attempt++) {
    const game = await program.account.game.fetch(gameKey);
    // Already recorded our guess (we or a retry landed) — done.
    const myGuess = humanIsP1 ? game.p1Guess : game.p2Guess;
    if (myGuess !== 255 || stateName(game) === "resolved") return;

    // First revealer supplies r_matchup; once matchup_type is set, send null.
    const rMatchup =
      game.matchupType === MATCHUP_TYPE_UNSET
        ? (hexToBytes(rMatchupHex) as any)
        : null;
    try {
      await program.methods
        .revealGuess(preimage as any, rMatchup)
        .accountsPartial({ ...revealAccounts, player: human.publicKey })
        .rpc();
      return;
    } catch (e) {
      const msg = String(e);
      if (attempt === 4) throw e;
      console.log(
        `[warn] reveal attempt ${attempt} failed, re-reading state: ${msg}`
      );
      await sleep(2500);
    }
  }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

async function main() {
  // Self-skip guard: no RPC or keyfile -> exit 0 (like the EVM harness).
  if (!RPC) {
    console.log("SKIP — SOLANA_RPC_URL not set (mainnet RPC required).");
    return;
  }
  if (!existsSync(KEYFILE)) {
    console.log(`SKIP — keyfile not found: ${KEYFILE}`);
    return;
  }
  let human: Keypair;
  try {
    human = Keypair.fromSecretKey(
      Uint8Array.from(JSON.parse(readFileSync(KEYFILE, "utf8")))
    );
  } catch (e) {
    console.log(`SKIP — could not load keyfile ${KEYFILE}: ${String(e)}`);
    return;
  }

  const connection = new Connection(RPC, "confirmed");
  const provider = new AnchorProvider(connection, new Wallet(human), {
    commitment: "confirmed",
    preflightCommitment: "confirmed",
  });
  const program = new Program<CoordinationGame>(IDL, provider);
  const wallet = human.publicKey.toBase58();
  console.log(`human (P1/creator) ${wallet}`);
  console.log(
    `game-api ${GAME_API} | tournament ${TOURNAMENT_ID} | program ${IDL.address}`
  );

  // Balance gate — need stake + a fee buffer, or bail before touching anything.
  const balance = await connection.getBalance(human.publicKey);
  assert(
    balance >= STAKE_LAMPORTS + 5_000_000,
    `insufficient balance ${balance} lamports (need >= ${
      STAKE_LAMPORTS + 5_000_000
    })`
  );
  console.log(`[ok] balance ${(balance / 1e9).toFixed(4)} SOL`);

  // 1) deposit_stake — funds the per-tournament escrow the game will consume.
  //    Tolerate a leftover escrow from a prior aborted run.
  let depositSig: string;
  try {
    depositSig = await program.methods
      .depositStake()
      .accountsPartial({
        escrow: escrowPda(TID, human.publicKey),
        tournament: tournamentPda(TID),
        player: human.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();
    console.log(`[ok] deposit_stake: ${depositSig}`);
  } catch (e) {
    if (String(e).includes("already in use")) {
      console.log("[ok] escrow already funded from a prior run — continuing");
      // Fall back to a lightweight self-transfer signature for auth below.
      depositSig = "";
    } else {
      throw e;
    }
  }

  // 2) Authenticate. Prefer the deposit signature (non-custodial session auth).
  if (!depositSig) {
    // No fresh deposit tx to authenticate with; make a 0-lamport self-transfer
    // purely to obtain a signed, wallet-owned tx for /auth/session.
    const { blockhash } = await connection.getLatestBlockhash("confirmed");
    const t = new Transaction({
      feePayer: human.publicKey,
      recentBlockhash: blockhash,
    }).add(
      SystemProgram.transfer({
        fromPubkey: human.publicKey,
        toPubkey: human.publicKey,
        lamports: 0,
      })
    );
    t.sign(human);
    depositSig = await connection.sendRawTransaction(t.serialize());
    await connection.confirmTransaction(depositSig, "confirmed");
  }
  const token = await sessionAuth(wallet, depositSig);
  console.log("[ok] authenticated (session auth)");

  // 3) Connect the matchmaking WebSocket (required before /queue/join, else 428).
  const ws = await MatchWs.connect(GAME_API, token);
  console.log("[ok] WebSocket connected");
  await sleep(1500); // let game-api register the connection

  try {
    // 4) Join the queue ALONE as a human -> "waiting" -> schedules AI fallback.
    const joinRes = await api("/queue/join", {
      body: { tournament_id: TOURNAMENT_ID, is_ai: false, is_test: true },
      token,
    });
    console.log(
      `[ok] queue join -> ${JSON.stringify(
        joinRes
      )} (awaiting grok fallback ~20-40s)`
    );

    // 5) grok fills the match. `match_found` carries our role + (for the
    //    creator) the matchup_commitment.
    const mf = await ws.waitFor("match_found", 120_000);
    const sessionId: string = mf.session_id;
    const role: number = mf.role;
    console.log(`[ok] match_found: session=${sessionId} role=${role}`);
    assert(
      role === 0,
      `expected human=role 0 (creator); got ${role}. grok only joins, never creates — a role-1 human cannot be paired by the AI fallback.`
    );
    const matchupCommitment = hexToBytes(String(mf.matchup_commitment));
    assert.equal(
      matchupCommitment.length,
      32,
      "matchup_commitment must be 32 bytes"
    );

    // 6) Read the game counter, create the game (matchmaker co-sign), and tell
    //    game-api so it emits game_ready to grok (who then joins on-chain).
    const cfg = await program.account.globalConfig.fetch(globalConfigPda());
    const counter = await program.account.gameCounter.fetch(gameCounterPda());
    const gameId = counter.count as BN;
    const gameKey = gamePda(gameId);
    console.log(
      `[..] creating game_id=${gameId.toString()} matchmaker=${cfg.matchmaker.toBase58()}`
    );
    await createGameCosigned(
      program,
      connection,
      human,
      cfg.matchmaker,
      token,
      matchupCommitment,
      gameId
    );
    await api("/games/started", {
      body: { game_id: gameId.toNumber(), session_id: sessionId },
      token,
    });
    console.log(`[ok] /games/started sent — grok will receive game_ready`);

    // 7) Wait for grok to join on-chain (game -> Active, player_two set).
    const active = await pollGameState(
      program,
      gameKey,
      ["active"],
      "grok joins",
      30
    );
    const playerOne: PublicKey = active.playerOne;
    const playerTwo: PublicKey = active.playerTwo;
    assert(
      !playerTwo.equals(PublicKey.default) &&
        !playerTwo.equals(human.publicKey),
      "player_two must be grok's wallet, distinct from the human"
    );
    console.log(`[ok] grok joined as P2: ${playerTwo.toBase58()}`);

    // 8) Human commits. State -> Committing. Notify game-api.
    const c = generateCommit(HUMAN_GUESS);
    await program.methods
      .commitGuess(c.commitment as any)
      .accountsPartial({ game: gameKey, player: human.publicKey })
      .rpc();
    await api("/games/committed", { body: { session_id: sessionId }, token });
    console.log(
      `[ok] human committed (guess=${HUMAN_GUESS}); waiting on grok to commit...`
    );

    // 9) grok runs its ~3-4min persona chat loop, then commits -> Revealing.
    await pollGameState(
      program,
      gameKey,
      ["revealing", "resolved"],
      "grok commits",
      120
    );
    console.log("[ok] both committed (state=revealing)");

    // 10) r_matchup is delivered once both commit — needed by the first revealer.
    const rMatchupHex = await pollRMatchup(token, sessionId, 30);
    console.log("[ok] r_matchup delivered");

    // 11) Human reveals (race-safe), then grok reveals -> Resolved.
    await revealWithRace(
      program,
      human,
      gameId,
      c.r,
      rMatchupHex,
      playerOne,
      playerTwo,
      cfg.treasury
    );
    console.log("[ok] human revealed; waiting for grok to reveal + resolve...");

    const resolved = await pollGameState(
      program,
      gameKey,
      ["resolved"],
      "resolution",
      40
    );
    assert.notEqual(
      resolved.p1Guess,
      255,
      "p1 guess must be set on resolution"
    );
    assert.notEqual(
      resolved.p2Guess,
      255,
      "p2 guess must be set on resolution"
    );
    assert.notEqual(
      resolved.resolvedAt.toString(),
      "0",
      "resolved_at must be set"
    );
    assert.notEqual(
      resolved.matchupType,
      MATCHUP_TYPE_UNSET,
      "matchup_type must be revealed on resolution"
    );

    // Best-effort resolution report (telemetry only; never fails the run).
    try {
      await api("/games/resolved", {
        token,
        body: {
          game_id: gameId.toNumber(),
          p1_guess: resolved.p1Guess,
          p2_guess: resolved.p2Guess,
          p1_return: 0,
          p2_return: 0,
          matchup_type: resolved.matchupType,
          first_committer: resolved.firstCommitter,
        },
      });
    } catch (e) {
      console.log(
        `[warn] /games/resolved report failed (non-fatal): ${String(e)}`
      );
    }

    console.log(
      `\nRESOLVED on-chain: game_id=${gameId.toString()} ` +
        `p1Guess=${resolved.p1Guess} p2Guess=${resolved.p2Guess} ` +
        `matchupType=${resolved.matchupType} firstCommitter=${resolved.firstCommitter}`
    );
    console.log(
      `\nPASS — Solana same-chain mainnet game resolved on the upgraded program; ` +
        `opponent = grok (${playerTwo.toBase58()}), game_id=${gameId.toString()}`
    );
  } finally {
    // Leave the queue (idempotent) and close the socket.
    try {
      await api("/queue/leave", {
        body: { tournament_id: TOURNAMENT_ID },
        token,
      });
    } catch {
      /* already out of queue */
    }
    ws.close();
  }
}

main()
  .then(() => process.exit(0))
  .catch((e) => {
    console.error("FAIL:", e?.message ?? e);
    process.exit(1);
  });
