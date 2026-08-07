/**
 * The chain-agnostic half of season finalization.
 *
 * WHY THIS FILE EXISTS SEPARATELY
 * ------------------------------
 * Finalizing a season is: read every player's (wins, games) -> drop the
 * ineligible -> score the rest -> split the pot by score -> build a merkle tree
 * -> publish the root. Only the FIRST and LAST steps are chain-specific
 * (reading PlayerProfile PDAs vs a Solidity mapping; calling
 * finalize_tournament vs finalizeSeason). Everything between is one set of
 * rules that both chains must apply identically.
 *
 * Splitting it this way is the same seam the org already locked for chain
 * logic: shared business rules, per-chain read/write.
 *
 * THE FORMAT TRAP THIS FILE MUST NOT REPEAT
 * ----------------------------------------
 * The tree uses Solana's node format:
 *     leaf     = keccak256(0x00 || account || amount)
 *     internal = keccak256(0x01 || min || max)
 * OpenZeppelin's MerkleProof hashes `keccak256(min || max)` with NO
 * domain-separation byte. Those are DIFFERENT TREES. An earlier version of the
 * EVM contract used the library and would have rejected every proof this
 * script produces. `assertVectorParity` below re-checks the format against the
 * same fixture both chains are held to, so a drift fails here rather than at a
 * player's claim.
 */

import { keccak_256 } from "@noble/hashes/sha3.js";
import { readFileSync } from "node:fs";
import { join } from "node:path";

/** A player's record for one season, as read from whichever chain. */
export interface PlayerRecord {
  /** Base58 pubkey (Solana) or 0x address (EVM). Used verbatim in the leaf. */
  account: string;
  wins: bigint;
  games: bigint;
}

export interface Entitlement {
  account: string;
  score: bigint;
  amount: bigint;
  proof: string[];
}

export interface SeasonResult {
  root: string;
  totalDistributed: bigint;
  entitlements: Entitlement[];
  /** Eligible players who scored 0 — they get no leaf, so no proof exists. */
  zeroScored: string[];
}

const FIXTURE = join(__dirname, "..", "tests", "fixtures", "game-payout-vectors.json");

function hex(b: Uint8Array): string {
  return "0x" + Buffer.from(b).toString("hex");
}

/** `wins² / games`, integer division. Mirrors `chain_core::game::compute_score`. */
export function computeScore(wins: bigint, games: bigint): bigint {
  if (games === 0n) throw new Error("score is undefined with zero games");
  return (wins * wins) / games;
}

/** leaf = keccak256(0x00 ‖ account ‖ amount). */
export function leafFor(account: string, amount: bigint, accountBytes: (a: string) => Uint8Array): Uint8Array {
  const amt = Buffer.alloc(32);
  Buffer.from(amount.toString(16).padStart(64, "0"), "hex").copy(amt);
  return keccak_256(Buffer.concat([Buffer.from([0x00]), Buffer.from(accountBytes(account)), amt]));
}

/** internal = keccak256(0x01 ‖ min ‖ max) — sorted, so proofs are order-free. */
export function hashNode(a: Uint8Array, b: Uint8Array): Uint8Array {
  const [lo, hi] = Buffer.compare(Buffer.from(a), Buffer.from(b)) <= 0 ? [a, b] : [b, a];
  return keccak_256(Buffer.concat([Buffer.from([0x01]), Buffer.from(lo), Buffer.from(hi)]));
}

/**
 * Read the shared fixture and confirm this file still produces the same
 * leaf/node/root and the same eligibility gate. Called before any finalize.
 *
 * A finalizer that silently drifts from the contract publishes a root nobody
 * can prove against, and the failure surfaces as a player's claim reverting —
 * far from the cause.
 */
export function assertVectorParity(): { minGames: bigint } {
  const fx = JSON.parse(readFileSync(FIXTURE, "utf8"));
  const m = fx.merkle;
  const addrBytes = (a: string) => Buffer.from(a.replace(/^0x/, ""), "hex");

  const la = leafFor(m.addrA, BigInt(m.amountA), addrBytes);
  const lb = leafFor(m.addrB, BigInt(m.amountB), addrBytes);
  if (hex(la) !== m.leafA) throw new Error(`leaf format drift: ${hex(la)} != ${m.leafA}`);
  if (hex(lb) !== m.leafB) throw new Error(`leaf format drift: ${hex(lb)} != ${m.leafB}`);
  const root = hashNode(la, lb);
  if (hex(root) !== m.root) {
    throw new Error(`node format drift: ${hex(root)} != ${m.root} — check the 0x01 domain byte`);
  }

  for (const c of fx.constants.scoreCases) {
    const got = computeScore(BigInt(c.wins), BigInt(c.games));
    if (got !== BigInt(c.score)) {
      throw new Error(`score drift: ${c.wins}/${c.games} gave ${got}, fixture says ${c.score}`);
    }
  }
  return { minGames: BigInt(fx.constants.minGamesForPayout) };
}

/**
 * Split `potTotal` across eligible players in proportion to score, and build
 * the tree.
 *
 * Truncation is deliberate and the remainder is simply not promised: a season
 * must never promise more than it holds, so the dust stays in the contract and
 * is swept with the unclaimed remainder.
 */
export function buildSeason(
  records: PlayerRecord[],
  potTotal: bigint,
  accountBytes: (a: string) => Uint8Array,
): SeasonResult {
  const { minGames } = assertVectorParity();

  const eligible = records
    .filter((r) => r.games >= minGames)
    .map((r) => ({ ...r, score: computeScore(r.wins, r.games) }));

  const totalScore = eligible.reduce((s, e) => s + e.score, 0n);
  if (totalScore === 0n) {
    return { root: hex(new Uint8Array(32)), totalDistributed: 0n, entitlements: [], zeroScored: eligible.map((e) => e.account) };
  }

  // A zero-scored player gets NO leaf. `claim` would reject them anyway, and a
  // zero-amount leaf is indistinguishable from an absent one to a verifier.
  const scored = eligible.filter((e) => e.score > 0n);
  const zeroScored = eligible.filter((e) => e.score === 0n).map((e) => e.account);

  const entries = scored.map((e) => ({
    account: e.account,
    score: e.score,
    amount: (potTotal * e.score) / totalScore,
  }));

  let leaves = entries.map((e) => leafFor(e.account, e.amount, accountBytes));
  const levels: Uint8Array[][] = [leaves];
  while (levels[levels.length - 1].length > 1) {
    const cur = levels[levels.length - 1];
    const next: Uint8Array[] = [];
    for (let i = 0; i < cur.length; i += 2) {
      // Odd node carries up unchanged — it has no sibling to pair with.
      next.push(i + 1 < cur.length ? hashNode(cur[i], cur[i + 1]) : cur[i]);
    }
    levels.push(next);
  }

  const proofs = entries.map((_, idx) => {
    const proof: string[] = [];
    let i = idx;
    for (let d = 0; d < levels.length - 1; d++) {
      const sib = i % 2 === 0 ? i + 1 : i - 1;
      if (sib < levels[d].length) proof.push(hex(levels[d][sib]));
      i = Math.floor(i / 2);
    }
    return proof;
  });

  return {
    root: hex(levels[levels.length - 1][0]),
    totalDistributed: entries.reduce((s, e) => s + e.amount, 0n),
    entitlements: entries.map((e, i) => ({ ...e, proof: proofs[i] })),
    zeroScored,
  };
}
