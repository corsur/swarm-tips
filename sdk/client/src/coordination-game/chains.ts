/** Canonical Coordination Game deployments, generated from the public chain registry. */
export const COORDINATION_GAME_DEPLOYMENTS = {
  "solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1": {
    displayName: "Solana Devnet",
    network: "testnet",
    namespace: "solana",
    coordinationGame: "2qqVk7kUqffnahiJpcQJCsSd8ErbEUgKTgCn1zYsw64P",
    crossChainGame: "2qqVk7kUqffnahiJpcQJCsSd8ErbEUgKTgCn1zYsw64P",
    stakeBaseUnits: 50_000_000n,
    maxTrancheBaseUnits: 100_000_000n,
  },
  "solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp": {
    displayName: "Solana Mainnet",
    network: "mainnet",
    namespace: "solana",
    coordinationGame: "2qqVk7kUqffnahiJpcQJCsSd8ErbEUgKTgCn1zYsw64P",
    crossChainGame: "2qqVk7kUqffnahiJpcQJCsSd8ErbEUgKTgCn1zYsw64P",
    stakeBaseUnits: 68_482_585n,
    maxTrancheBaseUnits: 100_000_000n,
  },
  "eip155:84532": {
    displayName: "Base Sepolia",
    network: "testnet",
    namespace: "eip155",
    coordinationGame: "0x4FBBceb96D2814b5d4ac26089Eb7E43471533253",
    crossChainGame: "0x7Cc7667f4B71c8eb1D4d3fADE088BFD8d01290AB",
    stakeBaseUnits: 3_200_000_000_000_000n,
    maxTrancheBaseUnits: 6_400_000_000_000_000n,
  },
  "eip155:8453": {
    displayName: "Base",
    network: "mainnet",
    namespace: "eip155",
    coordinationGame: "0xd585baE48901513202dAEb7d4feE4Af508a96234",
    crossChainGame: "0x44aC6Eb44692Bcf724d7975B91B299E2c553Ca12",
    stakeBaseUnits: 2_700_000_000_000_000n,
    maxTrancheBaseUnits: 5_400_000_000_000_000n,
  },
  "eip155:1": {
    displayName: "Ethereum",
    network: "mainnet",
    namespace: "eip155",
    coordinationGame: "0x265818b054E8413Bab870e0Ce0D8aB68400CF0F9",
    crossChainGame: "0xb84638E5d03AE68c5dC58408e05C001918A23fe9",
    stakeBaseUnits: 2_700_000_000_000_000n,
    maxTrancheBaseUnits: 5_400_000_000_000_000n,
  },
} as const;

export type CoordinationGameChainId = keyof typeof COORDINATION_GAME_DEPLOYMENTS;
export type CoordinationGameDeployment =
  (typeof COORDINATION_GAME_DEPLOYMENTS)[CoordinationGameChainId];

/** Return the pinned deployment for a supported CAIP-2 chain ID. */
export function coordinationGameDeployment(
  chainId: string,
): CoordinationGameDeployment | undefined {
  return COORDINATION_GAME_DEPLOYMENTS[
    chainId as CoordinationGameChainId
  ];
}
