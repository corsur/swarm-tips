import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import {
  COORDINATION_GAME_DEPLOYMENTS,
  coordinationGameDeployment,
  type CoordinationGameChainId,
} from "./chains.js";

const repositoryRoot = resolve(import.meta.dirname, "../../../..");
const registry = readFileSync(
  resolve(repositoryRoot, "crates/chain-registry/src/lib.rs"),
  "utf8",
);

const registryEntry = (constant: string): string => {
  const match = new RegExp(
    `\\n    ChainEntry \\{\\n        chain_id: ${constant},[\\s\\S]*?\\n    \\},`,
  ).exec(registry);
  if (!match) throw new Error(`chain registry entry ${constant} not found`);
  return match[0];
};

const CASES: Array<[CoordinationGameChainId, string]> = [
  ["solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1", "SOLANA_DEVNET_CAIP2"],
  ["solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp", "SOLANA_MAINNET_CAIP2"],
  ["eip155:84532", "BASE_SEPOLIA_CAIP2"],
  ["eip155:8453", "BASE_MAINNET_CAIP2"],
  ["eip155:1", "ETHEREUM_MAINNET_CAIP2"],
];

describe("canonical Coordination Game deployments", () => {
  it.each(CASES)("keeps %s in lockstep with chain-registry", (chainId, constant) => {
    const deployment = COORDINATION_GAME_DEPLOYMENTS[chainId];
    const entry = registryEntry(constant);
    const rustInteger = (value: bigint) =>
      value.toString().replace(/\B(?=(\d{3})+(?!\d))/g, "_");
    expect(entry).toContain(`stake_base_units: ${rustInteger(deployment.stakeBaseUnits)}`);
    expect(entry).toContain(`max_tranche_base_units: ${rustInteger(deployment.maxTrancheBaseUnits)}`);
    if (deployment.namespace === "eip155") {
      expect(entry.toLowerCase()).toContain(
        `game_contract: some("${deployment.crossChainGame.toLowerCase()}")`,
      );
      expect(entry.toLowerCase()).toContain(
        `coordination_game_v4_proxy: some("${deployment.coordinationGame.toLowerCase()}")`,
      );
    } else {
      expect(entry).toContain(`game_contract: Some("${deployment.crossChainGame}")`);
    }
  });

  it("returns undefined for unsupported chains", () => {
    expect(coordinationGameDeployment("eip155:999999")).toBeUndefined();
  });
});
