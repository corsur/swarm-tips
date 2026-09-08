import { createPublicClient, encodeFunctionData, http, type Hex } from "viem";
import type { CoordinationGameApiClient, XMatchPayload } from "../api.js";
import { CROSS_CHAIN_GAME_ABI } from "./abi.js";

export function unpadAddress(word: string): Hex {
  const hex = word.startsWith("0x") ? word.slice(2) : word;
  return `0x${hex.slice(-40)}`;
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

export function buildEvmFundCall(match: XMatchPayload, pin: EvmFundPin): EvmFundCall {
  const to = unpadAddress(match.leg_b.contract);
  if (to.toLowerCase() !== pin.contract.toLowerCase()) {
    throw new Error(`relay leg_b.contract ${to} != pinned CrossChainGame ${pin.contract}`);
  }
  const value = BigInt(match.leg_b.stake_base_units);
  if (value !== pin.stakeWei) {
    throw new Error(`relay leg_b.stake ${value} != pinned stake ${pin.stakeWei}`);
  }
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
  return { to, data, value };
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
