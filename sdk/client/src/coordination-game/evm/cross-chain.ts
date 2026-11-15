import {
  createPublicClient,
  encodeFunctionData,
  http,
  keccak256,
  toBytes,
  type Hex,
} from "viem";
import type {
  CoordinationGameApiClient,
  XLegPayload,
  XMatchPayload,
  XOutcomeCertPayload,
} from "../api.js";
import { CROSS_CHAIN_GAME_ABI } from "./abi.js";

export function unpadAddress(word: string): Hex {
  const hex = word.startsWith("0x") ? word.slice(2) : word;
  return `0x${hex.slice(-40)}`;
}

/**
 * Normalize a 65-byte `0x` secp256k1 `[r||s||v]` signature so `v` is in the
 * EVM `ecrecover` / OpenZeppelin `ECDSA.recover` domain (`27 | 28`). The
 * off-chain relay stores signatures with `v = 0 | 1`; passing `v < 27` directly
 * to `CrossChainGame` reverts with `ECDSAInvalidSignature`.
 */
export function normalizeEvmSigV(sig: string): Hex {
  if (!/^0x[0-9a-fA-F]{130}$/.test(sig)) {
    throw new Error(`expected 65-byte 0x signature (130 hex chars), got length ${sig.length}`);
  }
  const body = sig.slice(2).toLowerCase();
  const rs = body.slice(0, 128);
  let v = parseInt(body.slice(128, 130), 16);
  if (v < 27) v += 27;
  return `0x${rs}${v.toString(16).padStart(2, "0")}`;
}

export interface CrossChainMatchResult {
  match: XMatchPayload;
  handle: string | undefined;
}

export async function findCrossChainMatch(
  api: CoordinationGameApiClient,
  input: { wallet: string; sessionKeyAddress: string; tournamentId: number; chain: string },
  options: { pollMs?: number; maxPolls?: number; sleep?: (ms: number) => Promise<void> } = {},
): Promise<CrossChainMatchResult> {
  const joined = await api.joinCrossChainQueue({
    wallet: input.wallet,
    chain: input.chain,
    session_key: input.sessionKeyAddress,
    tournament_id: input.tournamentId,
  });
  const handle = joined.poll_handle;
  if (joined.status === "matched" && joined.match) return { match: joined.match, handle };
  const sleep = options.sleep ?? ((ms: number) => new Promise<void>((resolve) => globalThis.setTimeout(resolve, ms)));
  for (let index = 0; index < (options.maxPolls ?? 60); index += 1) {
    await sleep(options.pollMs ?? 4_000);
    const next = await api.crossChainStatus(handle ? { handle } : { wallet: input.wallet });
    if (next.status === "matched" && next.match) return { match: next.match, handle };
  }
  throw new Error("no opponent found before timeout");
}

export interface EvmFundCall { to: Hex; data: Hex; value: bigint }
export interface EvmFundPin { contract: Hex; stakeWei: bigint }

function verifyPin(match: XMatchPayload, pin: EvmFundPin): { to: Hex; stakeWei: bigint } {
  const to = unpadAddress(match.leg_b.contract);
  if (to.toLowerCase() !== pin.contract.toLowerCase()) {
    throw new Error(`relay leg_b.contract ${to} != pinned CrossChainGame ${pin.contract}`);
  }
  const stakeWei = BigInt(match.leg_b.stake_base_units);
  if (stakeWei !== pin.stakeWei) {
    throw new Error(`relay leg_b.stake ${stakeWei} != pinned stake ${pin.stakeWei}`);
  }
  return { to, stakeWei };
}

function legTupleFromPayload(leg: XLegPayload) {
  return {
    chainTag: keccak256(toBytes(leg.chain)),
    contractId: leg.contract as Hex,
    player: leg.player as Hex,
    sessionKey: unpadAddress(leg.session_key),
    stake: BigInt(leg.stake_base_units),
    tranche: BigInt(leg.tranche_base_units),
  } as const;
}

/**
 * Reconstruct the `CertLib.MatchLiveCert` tuple expected by `CrossChainGame`
 * (`lockTranche`, `settle`, `openClaim`, `supersedeClaim`,
 * `submitEquivocationProof`) from a matched `XMatchPayload`. `chainTag` is
 * recomputed as `keccak256(utf8(leg.chain))`, matching `game-api`.
 */
export function certTupleFromMatchPayload(match: XMatchPayload) {
  return {
    matchId: match.match_id as Hex,
    tournamentId: BigInt(match.tournament_id),
    matchupCommitment: match.matchup_commitment as Hex,
    legA: legTupleFromPayload(match.leg_a),
    legB: legTupleFromPayload(match.leg_b),
    quoteTimestamp: BigInt(match.quote_timestamp),
    quoteMaxAgeSecs: Number(match.quote_max_age_secs),
    matchDeadline: BigInt(match.match_deadline),
    claimWindowSecs: Number(match.claim_window_secs),
    aIsP1: Number(match.a_is_p1),
  } as const;
}

export function buildEvmFundCall(match: XMatchPayload, pin: EvmFundPin): EvmFundCall {
  const { to, stakeWei } = verifyPin(match, pin);
  const data = encodeFunctionData({
    abi: CROSS_CHAIN_GAME_ABI,
    functionName: "createMatch",
    args: [
      match.match_id as Hex,
      unpadAddress(match.leg_b.session_key),
      unpadAddress(match.leg_a.session_key),
      match.a_is_p1 === 0,
      BigInt(match.fund_deadline),
      BigInt(match.match_deadline),
      match.create_match_operator_sig as Hex,
    ],
  });
  return { to, data, value: stakeWei };
}

/**
 * Build the unsigned permissionless `CrossChainGame.lockTranche(cert, operatorSig)`
 * call (`value === 0n`) from the matched `XMatchPayload`, verifying `leg_b`
 * against `pin` and normalizing `operator_signature` `v` to `27 | 28`.
 */
export function buildEvmLockTrancheCall(match: XMatchPayload, pin: EvmFundPin): EvmFundCall {
  const { to } = verifyPin(match, pin);
  const data = encodeFunctionData({
    abi: CROSS_CHAIN_GAME_ABI,
    functionName: "lockTranche",
    args: [certTupleFromMatchPayload(match), normalizeEvmSigV(match.operator_signature)],
  });
  return { to, data, value: 0n };
}

export interface XSettleBundle {
  outcome: Pick<
    XOutcomeCertPayload,
    | "match_id"
    | "match_live_digest"
    | "outcome_kind"
    | "step_count"
    | "p1_guess"
    | "p2_guess"
    | "first_committer"
    | "matchup_type"
    | "transcript_hash"
  >;
  live_sigs: readonly [string, string, string];
  oc_sigs: readonly [string, string, string];
}

/**
 * Build the unsigned permissionless `CrossChainGame.settle(cert, oc, liveSigs, ocSigs)`
 * call (`value === 0n`), verifying `leg_b` against `pin`, checking `match_id`
 * and `match_live_digest` consistency, and normalizing all 6 signatures to
 * EVM `v = 27 | 28`.
 */
export function buildEvmSettleCall(
  match: XMatchPayload,
  bundle: XSettleBundle,
  pin: EvmFundPin,
): EvmFundCall {
  const { to } = verifyPin(match, pin);
  if (bundle.outcome.match_id.toLowerCase() !== match.match_id.toLowerCase()) {
    throw new Error(
      `outcome.match_id ${bundle.outcome.match_id} != match.match_id ${match.match_id}`,
    );
  }
  if (
    match.match_live_digest &&
    bundle.outcome.match_live_digest.toLowerCase() !== match.match_live_digest.toLowerCase()
  ) {
    throw new Error(
      `outcome.match_live_digest ${bundle.outcome.match_live_digest} != match.match_live_digest ${match.match_live_digest}`,
    );
  }
  const oc = {
    matchId: bundle.outcome.match_id as Hex,
    matchLiveDigest: bundle.outcome.match_live_digest as Hex,
    outcomeKind: Number(bundle.outcome.outcome_kind),
    stepCount: Number(bundle.outcome.step_count),
    p1Guess: Number(bundle.outcome.p1_guess),
    p2Guess: Number(bundle.outcome.p2_guess),
    firstCommitter: Number(bundle.outcome.first_committer),
    matchupType: Number(bundle.outcome.matchup_type),
    transcriptHash: bundle.outcome.transcript_hash as Hex,
  } as const;
  const liveSigs: [Hex, Hex, Hex] = [
    normalizeEvmSigV(bundle.live_sigs[0]),
    normalizeEvmSigV(bundle.live_sigs[1]),
    normalizeEvmSigV(bundle.live_sigs[2]),
  ];
  const ocSigs: [Hex, Hex, Hex] = [
    normalizeEvmSigV(bundle.oc_sigs[0]),
    normalizeEvmSigV(bundle.oc_sigs[1]),
    normalizeEvmSigV(bundle.oc_sigs[2]),
  ];
  const data = encodeFunctionData({
    abi: CROSS_CHAIN_GAME_ABI,
    functionName: "settle",
    args: [certTupleFromMatchPayload(match), oc, liveSigs, ocSigs],
  });
  return { to, data, value: 0n };
}

/**
 * Read `CrossChainGame.matches(matchId).status` (`0 = None, 1 = Funded,
 * 2 = Locked, 3 = Claiming, 4 = Settled, 5 = ClaimSettled, 6 = RefundedNoCert,
 * 7 = RefundedTimeout`).
 */
export async function readEvmXMatchStatus(
  rpcUrl: string,
  contract: Hex,
  matchId: Hex,
): Promise<number> {
  const client = createPublicClient({ transport: http(rpcUrl) });
  const result = (await client.readContract({
    address: contract,
    abi: CROSS_CHAIN_GAME_ABI,
    functionName: "matches",
    args: [matchId],
  })) as readonly unknown[];
  return Number(result[0]);
}

export interface FundTargetReader {
  stakeWei(contract: Hex): Promise<bigint>;
  matchStatus(contract: Hex, matchId: Hex): Promise<number>;
}

export async function verifyEvmFundTarget(
  match: XMatchPayload,
  pin: EvmFundPin,
  readers: FundTargetReader[],
): Promise<void> {
  if (readers.length < 2) throw new Error(`need >=2 RPCs for a pre-fund quorum read, got ${readers.length}`);
  const matchId = match.match_id as Hex;
  const stakes = await Promise.all(readers.map((reader) => reader.stakeWei(pin.contract)));
  const statuses = await Promise.all(readers.map((reader) => reader.matchStatus(pin.contract, matchId)));
  if (!stakes.every((stake) => stake === stakes[0])) throw new Error(`RPCs disagree on stakeWei (${stakes.join(", ")}); refusing to fund`);
  if (!statuses.every((status) => status === statuses[0])) throw new Error(`RPCs disagree on match status (${statuses.join(", ")}); refusing to fund`);
  if (stakes[0] !== pin.stakeWei) throw new Error(`on-chain stakeWei ${stakes[0]} != pinned ${pin.stakeWei}; refusing to fund`);
  if (statuses[0] !== 0) throw new Error(`matchId ${matchId} already has status ${statuses[0]}; not a fundable slot`);
}

export function rpcFundTargetReaders(rpcUrls: string[]): FundTargetReader[] {
  return rpcUrls.map((url) => {
    const client = createPublicClient({ transport: http(url) });
    return {
      stakeWei: (contract: Hex) => client.readContract({ address: contract, abi: CROSS_CHAIN_GAME_ABI, functionName: "stakeWei" }) as Promise<bigint>,
      matchStatus: async (contract: Hex, matchId: Hex) => {
        const result = await client.readContract({ address: contract, abi: CROSS_CHAIN_GAME_ABI, functionName: "matches", args: [matchId] }) as readonly unknown[];
        return Number(result[0]);
      },
    };
  });
}
