/**
 * Live cross-chain GAMEPLAY-coordination e2e — no funds, no on-chain txs.
 *
 * Validates the commit → sign → reveal → sign coordination against the DEPLOYED
 * game-api, the half that the protocol e2e fabricated before finding #5:
 *
 *   join (both legs) → commit (both) → co-sign step-2 (both) → r_matchup released
 *     → reveal (both) → co-sign terminal (both) → terminal checkpoint stored
 *
 * Proves the relay's matchup binding, the r_matchup reveal gate, the canonical
 * wire / digest matching (the digest game-api returns is what each player signs),
 * and the two-leg signature assembly — all without staking or settling on-chain.
 * The operator co-signs the match-live cert server-side, so this needs NO creds,
 * only a reachable game-api.
 *
 * MANUAL-ONLY. Self-skips unless RUN_GAMEPLAY_E2E is set.
 * Run:
 *   GAME_API=https://api.coordination.game RUN_GAMEPLAY_E2E=1 \
 *     npx ts-mocha -p ./tsconfig.json tests/live/xchain-gameplay-testnet.ts --timeout 120000
 */
import { createHash, randomBytes } from "crypto";
import { expect } from "chai";
import { Keypair } from "@solana/web3.js";
import { bytesToHex } from "viem";
import { privateKeyToAccount, generatePrivateKey } from "viem/accounts";
import {
  newSessionSigner,
  signDigest,
  type SessionSigner,
} from "../helpers/xchain-cert";

const GAME_API = process.env.GAME_API ?? "https://api.coordination.game";
// An isolated tournament queue so this never cross-pollinates other matchmaker
// e2es (ephemeral wallets also make each run's queue/pending docs unique).
const TOURNAMENT_ID = Number(process.env.GAMEPLAY_TOURNAMENT_ID ?? "990001");
const SOLANA_CHAIN = "solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1";
const EVM_CHAIN = "eip155:84532";
const RUN = Boolean(process.env.RUN_GAMEPLAY_E2E);

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

const sha256_0x = (b: Uint8Array): string =>
  "0x" + createHash("sha256").update(b).digest("hex");

/** A 32-byte guess preimage whose last bit encodes the guess. */
function preimage(guess: number): Uint8Array {
  const r = randomBytes(32);
  r[31] = (r[31] & 0xfe) | (guess & 1);
  return r;
}

/** Sign a `0x` 32-byte digest with a session key → `0x` 65-byte [r||s||v=0|1]
 *  signature (the form the relay's k256 recover_address verifies). */
function sig0x(s: SessionSigner, digestHex: string): string {
  const digest = Uint8Array.from(Buffer.from(digestHex.slice(2), "hex"));
  return bytesToHex(Uint8Array.from(signDigest(s, digest)));
}

describe("cross-chain gameplay coordination (live, no funds)", function () {
  this.timeout(120000);
  before(function () {
    if (!RUN) this.skip();
  });

  it("plays commit → sign → reveal → sign with a bound matchup", async () => {
    const sessA = newSessionSigner(); // Solana-leg per-match session key
    const sessB = newSessionSigner(); // EVM-leg per-match session key
    const solWallet = Keypair.generate().publicKey.toBase58();
    const evmWallet = privateKeyToAccount(generatePrivateKey()).address;

    // 1) Join both legs → the matchmaker pairs them and co-signs the cert.
    await api("/internal/xqueue/join", {
      wallet: solWallet,
      chain: SOLANA_CHAIN,
      session_key: bytesToHex(sessA.address),
      tournament_id: TOURNAMENT_ID,
    });
    const matched = await api("/internal/xqueue/join", {
      wallet: evmWallet,
      chain: EVM_CHAIN,
      session_key: bytesToHex(sessB.address),
      tournament_id: TOURNAMENT_ID,
    });
    expect(matched.status).to.equal("matched");

    // 2) Commit both guesses (preimages kept client-side).
    const preA = preimage(1);
    const preB = preimage(0);
    await api("/internal/xqueue/commit", {
      wallet: solWallet,
      commit: sha256_0x(preA),
    });
    const c2 = await api("/internal/xqueue/commit", {
      wallet: evmWallet,
      commit: sha256_0x(preB),
    });
    expect(c2.both_committed).to.equal(true);

    // 3) Co-sign the canonical step-2 checkpoint (both legs).
    const gp1 = await api(`/internal/xqueue/gameplay?wallet=${solWallet}`);
    expect(
      gp1.step2_checkpoint_digest,
      "step-2 digest present once both commit"
    ).to.be.a("string");
    await api("/internal/xqueue/sign", {
      wallet: solWallet,
      step: 2,
      signature: sig0x(sessA, gp1.step2_checkpoint_digest),
    });
    const signed2 = await api("/internal/xqueue/sign", {
      wallet: evmWallet,
      step: 2,
      signature: sig0x(sessB, gp1.step2_checkpoint_digest),
    });
    expect(
      signed2.relayed,
      "step-2 checkpoint relayed once both signed"
    ).to.equal(true);
    expect(signed2.r_matchup, "r_matchup released after step-2 stored").to.be.a(
      "string"
    );

    // 4) Reveal both guesses (verified against the commits server-side).
    await api("/internal/xqueue/reveal", {
      wallet: solWallet,
      preimage: bytesToHex(preA),
    });
    const r2 = await api("/internal/xqueue/reveal", {
      wallet: evmWallet,
      preimage: bytesToHex(preB),
    });
    expect(r2.both_revealed).to.equal(true);

    // 5) Co-sign the canonical terminal checkpoint (both legs) — the matchup
    //    binding is enforced by the relay here.
    const gp2 = await api(`/internal/xqueue/gameplay?wallet=${solWallet}`);
    expect(
      gp2.terminal_checkpoint_digest,
      "terminal digest present once both reveal"
    ).to.be.a("string");
    await api("/internal/xqueue/sign", {
      wallet: solWallet,
      step: 4,
      signature: sig0x(sessA, gp2.terminal_checkpoint_digest),
    });
    const settled = await api("/internal/xqueue/sign", {
      wallet: evmWallet,
      step: 4,
      signature: sig0x(sessB, gp2.terminal_checkpoint_digest),
    });
    expect(
      settled.relayed,
      "terminal checkpoint relayed (matchup bound)"
    ).to.equal(true);
  });
});
