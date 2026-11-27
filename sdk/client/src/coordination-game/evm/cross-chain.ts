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
  EvmMatchPayload,
  XLegPayload,
  XMatchPayload,
  XOutcomeCertPayload,
} from "../api.js";
import { CROSS_CHAIN_GAME_ABI } from "./abi.js";

const ZERO_ADDRESS_HEX = "0000000000000000000000000000000000000000";

/** Maximum horizon (7 days) allowed for relay-supplied `fund_deadline` / `match_deadline`. */
export const MAX_XCHAIN_DEADLINE_HORIZON_SECS = 7n * 86_400n;

export function unpadAddress(word: string): Hex {
  if (typeof word !== "string") {
    throw new Error("invalid EVM address: expected hex string");
  }
  const hex = word.startsWith("0x") || word.startsWith("0X") ? word.slice(2) : word;
  let addr: string;
  if (/^[0-9a-fA-F]{40}$/.test(hex)) {
    addr = hex.toLowerCase();
  } else if (/^[0-9a-fA-F]{64}$/.test(hex)) {
    if (!hex.startsWith("000000000000000000000000")) {
      throw new Error(`invalid 32-byte padded EVM address: non-zero high 12 bytes in ${word}`);
    }
    addr = hex.slice(24).toLowerCase();
  } else {
    throw new Error(`invalid EVM address: expected 20-byte or zero-padded 32-byte hex, got ${word}`);
  }
  if (addr === ZERO_ADDRESS_HEX) {
    throw new Error("invalid EVM address: zero address is not allowed");
  }
  return `0x${addr}`;
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
  const v = parseInt(body.slice(128, 130), 16);
  if (v !== 0 && v !== 1 && v !== 27 && v !== 28) {
    throw new Error(`invalid secp256k1 signature recovery id v=${v}; expected 0, 1, 27, or 28`);
  }
  const normalizedV = v < 27 ? v + 27 : v;
  return `0x${rs}${normalizedV.toString(16).padStart(2, "0")}`;
}

export interface CrossChainMatchResult {
  match: XMatchPayload;
  handle: string | undefined;
}

export interface FindCrossChainMatchInput {
  wallet: string;
  sessionKeyAddress: string;
  tournamentId: number;
  chain: string;
  isAi?: boolean;
  is_ai?: boolean;
  token?: string;
}

export interface FindMatchPollOptions {
  pollMs?: number;
  maxPolls?: number;
  sleep?: (ms: number) => Promise<void>;
  token?: string;
  isAi?: boolean;
}

export async function findCrossChainMatch(
  api: CoordinationGameApiClient,
  input: FindCrossChainMatchInput,
  options: FindMatchPollOptions = {},
): Promise<CrossChainMatchResult> {
  const resolvedToken = options.token ?? input.token;
  const resolvedIsAi = options.isAi ?? input.isAi ?? input.is_ai;
  const joined = await api.joinCrossChainQueue(
    {
      wallet: input.wallet,
      chain: input.chain,
      session_key: input.sessionKeyAddress,
      tournament_id: input.tournamentId,
      ...(resolvedIsAi !== undefined ? { is_ai: resolvedIsAi } : {}),
    },
    resolvedToken,
  );
  const handle = joined.poll_handle;
  if (joined.status === "matched" && joined.match) return { match: joined.match, handle };
  const sleep =
    options.sleep ?? ((ms: number) => new Promise<void>((resolve) => globalThis.setTimeout(resolve, ms)));
  for (let index = 0; index < (options.maxPolls ?? 60); index += 1) {
    await sleep(options.pollMs ?? 4_000);
    const next = await api.crossChainStatus(
      handle ? { handle } : { wallet: input.wallet },
      resolvedToken,
    );
    if (next.status === "matched" && next.match) return { match: next.match, handle };
  }
  throw new Error("no opponent found before timeout");
}

export interface FindEvmMatchInput {
  wallet: string;
  playerWallet?: string;
  tournamentId: number;
  chain: string;
  isAi?: boolean;
  is_ai?: boolean;
  token?: string;
}

export interface EvmMatchResult {
  match: EvmMatchPayload;
  sessionId: string | undefined;
}

export async function findEvmMatch(
  api: CoordinationGameApiClient,
  input: FindEvmMatchInput,
  options: FindMatchPollOptions = {},
): Promise<EvmMatchResult> {
  const resolvedToken = options.token ?? input.token;
  const resolvedIsAi = options.isAi ?? input.isAi ?? input.is_ai;
  const joined = await api.joinEvmGame(
    {
      wallet: input.wallet,
      ...(input.playerWallet ? { player_wallet: input.playerWallet } : {}),
      chain: input.chain,
      tournament_id: input.tournamentId,
      ...(resolvedIsAi !== undefined ? { is_ai: resolvedIsAi } : {}),
    },
    resolvedToken,
  );
  if (joined.status === "matched" && joined.match) {
    return { match: joined.match, sessionId: joined.session_id };
  }
  const sleep =
    options.sleep ?? ((ms: number) => new Promise<void>((resolve) => globalThis.setTimeout(resolve, ms)));
  for (let index = 0; index < (options.maxPolls ?? 60); index += 1) {
    await sleep(options.pollMs ?? 4_000);
    const next = await api.evmGameStatus(input.wallet, resolvedToken);
    if (next.status === "matched" && next.match) {
      return { match: next.match, sessionId: next.session_id ?? joined.session_id };
    }
  }
  throw new Error("no opponent found before timeout");
}

export interface EvmFundCall { to: Hex; data: Hex; value: bigint }
export interface EvmFundPin {
  contract: Hex;
  stakeWei: bigint;
  /** Expected local EVM session key address (`leg_b.session_key`). Verified whenever provided. */
  sessionKey?: Hex;
  /** Optional Unix-seconds clock override for deadline horizon validation. */
  nowSec?: number;
}

function verifyPin(
  match: XMatchPayload,
  pin: EvmFundPin,
): { to: Hex; stakeWei: bigint; legBSessionKey: Hex; legASessionKey: Hex } {
  const pinnedContract = unpadAddress(pin.contract);
  const to = unpadAddress(match.leg_b.contract);
  if (to !== pinnedContract) {
    throw new Error(`relay leg_b.contract ${to} != pinned CrossChainGame ${pinnedContract}`);
  }
  const stakeWei = BigInt(match.leg_b.stake_base_units);
  if (stakeWei !== pin.stakeWei) {
    throw new Error(`relay leg_b.stake ${stakeWei} != pinned stake ${pin.stakeWei}`);
  }
  const legBSessionKey = unpadAddress(match.leg_b.session_key);
  const legASessionKey = unpadAddress(match.leg_a.session_key);
  if (pin.sessionKey !== undefined) {
    const pinnedSessionKey = unpadAddress(pin.sessionKey);
    if (legBSessionKey !== pinnedSessionKey) {
      throw new Error(
        `relay leg_b.session_key ${legBSessionKey} != pinned sessionKey ${pinnedSessionKey}`,
      );
    }
  }
  return { to, stakeWei, legBSessionKey, legASessionKey };
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
  const { to, stakeWei, legBSessionKey, legASessionKey } = verifyPin(match, pin);
  if (match.a_is_p1 !== 0 && match.a_is_p1 !== 1) {
    throw new Error(`invalid a_is_p1=${String(match.a_is_p1)}; expected 0 or 1`);
  }
  const fundDeadline = BigInt(match.fund_deadline);
  const matchDeadline = BigInt(match.match_deadline);
  const nowSec = BigInt(
    pin.nowSec !== undefined ? Math.floor(pin.nowSec) : Math.floor(Date.now() / 1000),
  );
  const maxDeadline = nowSec + MAX_XCHAIN_DEADLINE_HORIZON_SECS;
  if (fundDeadline <= 0n || matchDeadline <= 0n) {
    throw new Error(
      `invalid deadlines (fund_deadline=${fundDeadline}, match_deadline=${matchDeadline}): must be > 0`,
    );
  }
  if (matchDeadline < fundDeadline) {
    throw new Error(
      `invalid deadlines: match_deadline (${matchDeadline}) < fund_deadline (${fundDeadline})`,
    );
  }
  if (fundDeadline > maxDeadline || matchDeadline > maxDeadline) {
    throw new Error(
      `relay deadline exceeds 7-day horizon (fund_deadline=${fundDeadline}, match_deadline=${matchDeadline}, max=${maxDeadline})`,
    );
  }
  const operatorSig = normalizeEvmSigV(match.create_match_operator_sig);
  const data = encodeFunctionData({
    abi: CROSS_CHAIN_GAME_ABI,
    functionName: "createMatch",
    args: [
      match.match_id as Hex,
      legBSessionKey,
      legASessionKey,
      match.a_is_p1 === 0,
      fundDeadline,
      matchDeadline,
      operatorSig,
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
