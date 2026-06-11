// Cross-chain certificate helpers for the TypeScript test harness.
//
// This is the THIRD implementation of the canonical certificate byte layout
// (after crates/chain-core cert_schema.rs and evm CertLib.sol). It is pinned
// to the SAME golden-vector fixture (tests/fixtures/cert-vectors.json), so if
// it drifts from the on-chain encoders a test fails. Every field is one
// 32-byte big-endian word — abi.encode-equivalent.

import { keccak_256 } from "@noble/hashes/sha3.js";
import { secp256k1 } from "@noble/curves/secp256k1.js";

export const MATCH_LIVE_MAGIC = keccak_256(utf8("SWARM_XCHAIN_MATCH_LIVE"));
export const CHECKPOINT_MAGIC = keccak_256(utf8("SWARM_XCHAIN_CHECKPOINT"));
export const OUTCOME_MAGIC = keccak_256(utf8("SWARM_XCHAIN_OUTCOME"));
export const SCHEMA_VERSION = 1;
export const TERMINAL_STEP_COUNT = 4;

function utf8(s: string): Uint8Array {
  return new TextEncoder().encode(s);
}

export function keccak256(bytes: Uint8Array): Uint8Array {
  return keccak_256(bytes);
}

/** A 32-byte big-endian word from a bigint/number. */
function word(value: bigint | number): Uint8Array {
  let v = BigInt(value);
  const out = new Uint8Array(32);
  for (let i = 31; i >= 0 && v > 0n; i--) {
    out[i] = Number(v & 0xffn);
    v >>= 8n;
  }
  return out;
}

/** A 20-byte address left-padded (right-aligned) into a 32-byte word. */
function addrWord(addr: Uint8Array): Uint8Array {
  const out = new Uint8Array(32);
  out.set(addr.slice(0, 20), 12);
  return out;
}

function bytes32(b: Uint8Array): Uint8Array {
  const out = new Uint8Array(32);
  out.set(b.slice(0, 32));
  return out;
}

function concat(words: Uint8Array[]): Uint8Array {
  const out = new Uint8Array(words.length * 32);
  words.forEach((w, i) => out.set(w, i * 32));
  return out;
}

export interface CertLeg {
  chainTag: Uint8Array; // 32
  contract: Uint8Array; // 32
  player: Uint8Array; // 32
  sessionKey: Uint8Array; // 20 (eth address)
  stake: bigint;
  tranche: bigint;
}

function legWords(leg: CertLeg): Uint8Array[] {
  return [
    bytes32(leg.chainTag),
    bytes32(leg.contract),
    bytes32(leg.player),
    addrWord(leg.sessionKey),
    word(leg.stake),
    word(leg.tranche),
  ];
}

export interface MatchLiveCert {
  matchId: Uint8Array; // 32
  tournamentId: bigint;
  matchupCommitment: Uint8Array; // 32
  legA: CertLeg;
  legB: CertLeg;
  quoteTimestamp: bigint;
  quoteMaxAgeSecs: number;
  matchDeadline: bigint;
  claimWindowSecs: number;
  aIsP1: number;
}

export function encodeMatchLive(c: MatchLiveCert): Uint8Array {
  return concat([
    bytes32(MATCH_LIVE_MAGIC),
    word(SCHEMA_VERSION),
    bytes32(c.matchId),
    word(c.tournamentId),
    bytes32(c.matchupCommitment),
    ...legWords(c.legA),
    ...legWords(c.legB),
    word(c.quoteTimestamp),
    word(c.quoteMaxAgeSecs),
    word(c.matchDeadline),
    word(c.claimWindowSecs),
    word(c.aIsP1),
  ]);
}

export function matchLiveDigest(c: MatchLiveCert): Uint8Array {
  return keccak256(encodeMatchLive(c));
}

export interface Checkpoint {
  matchLiveDigest: Uint8Array; // 32
  stepCount: number;
  p1Commit: Uint8Array; // 32
  p2Commit: Uint8Array; // 32
  p1Guess: number;
  p2Guess: number;
  firstCommitter: number;
  matchupType: number;
  transcriptHash: Uint8Array; // 32
}

export function encodeCheckpoint(c: Checkpoint): Uint8Array {
  return concat([
    bytes32(CHECKPOINT_MAGIC),
    word(SCHEMA_VERSION),
    bytes32(c.matchLiveDigest),
    word(c.stepCount),
    bytes32(c.p1Commit),
    bytes32(c.p2Commit),
    word(c.p1Guess),
    word(c.p2Guess),
    word(c.firstCommitter),
    word(c.matchupType),
    bytes32(c.transcriptHash),
  ]);
}

export function checkpointDigest(c: Checkpoint): Uint8Array {
  return keccak256(encodeCheckpoint(c));
}

export interface OutcomeCert {
  matchId: Uint8Array; // 32
  matchLiveDigest: Uint8Array; // 32
  outcomeKind: number;
  stepCount: number;
  p1Guess: number;
  p2Guess: number;
  firstCommitter: number;
  matchupType: number;
  transcriptHash: Uint8Array; // 32
}

export function encodeOutcome(c: OutcomeCert): Uint8Array {
  return concat([
    bytes32(OUTCOME_MAGIC),
    word(SCHEMA_VERSION),
    bytes32(c.matchId),
    bytes32(c.matchLiveDigest),
    word(c.outcomeKind),
    word(c.stepCount),
    word(c.p1Guess),
    word(c.p2Guess),
    word(c.firstCommitter),
    word(c.matchupType),
    bytes32(c.transcriptHash),
  ]);
}

export function outcomeDigest(c: OutcomeCert): Uint8Array {
  return keccak256(encodeOutcome(c));
}

// --- secp256k1 session keys ---------------------------------------------

export interface SessionSigner {
  privateKey: Uint8Array;
  address: Uint8Array; // 20-byte eth address
}

export function newSessionSigner(seed?: number): SessionSigner {
  const privateKey =
    seed === undefined
      ? secp256k1.utils.randomSecretKey()
      : word(seed).slice(0, 32);
  const pub = secp256k1.getPublicKey(privateKey, false); // 0x04 || x || y
  const address = keccak256(pub.slice(1)).slice(12); // last 20 bytes
  return { privateKey, address };
}

/** Sign a 32-byte digest, returning the program's [r(32) || s(32) || v(1)]
 *  with v = recovery id (0 or 1). */
export function signDigest(
  signer: SessionSigner,
  digest: Uint8Array,
): number[] {
  const sig = secp256k1.sign(digest, signer.privateKey, {
    prehash: false,
    format: "recovered",
  });
  // recovered format = [recid(1) || r(32) || s(32)]
  const recid = sig[0];
  const out = new Uint8Array(65);
  out.set(sig.slice(1, 65), 0); // r || s
  out[64] = recid; // v = recid (program accepts 0/1)
  return Array.from(out);
}

export function toArray(b: Uint8Array): number[] {
  return Array.from(b);
}
