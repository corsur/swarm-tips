/**
 * Pins the two defects that made the Base Sepolia escrow battery fail with a
 * message pointing at the wrong layer entirely.
 *
 * Run: npx ts-mocha tests/rpc-endpoints.test.ts
 */

import { expect } from "chai";
import { resolveRpcEndpoints } from "./live/rpc-endpoints";

const CHAIN_RPCS = [
  "https://sepolia.base.org",
  "https://base-sepolia.publicnode.com",
];
const DEDICATED = "https://base-sepolia.g.alchemy.com/v2/KEY";

describe("resolveRpcEndpoints", () => {
  it("prefers an explicit override but KEEPS the defaults as failover", () => {
    // The original collapsed to a single endpoint here, silently trading
    // rate-limiting for a single point of failure.
    expect(resolveRpcEndpoints(DEDICATED, CHAIN_RPCS)).to.deep.equal([
      DEDICATED,
      ...CHAIN_RPCS,
    ]);
  });

  it("treats an empty override as unset, not as an endpoint", () => {
    // `?? ` returned "" here. A runner writing RPC_URL="${VAR:-}" with VAR unset
    // hands over an empty string, which is a configured-looking nothing.
    expect(resolveRpcEndpoints("", CHAIN_RPCS)).to.deep.equal(CHAIN_RPCS);
  });

  it("treats a whitespace-only override as unset", () => {
    expect(resolveRpcEndpoints("   ", CHAIN_RPCS)).to.deep.equal(CHAIN_RPCS);
  });

  it("uses the defaults when no override is given", () => {
    expect(resolveRpcEndpoints(undefined, CHAIN_RPCS)).to.deep.equal(
      CHAIN_RPCS
    );
  });

  it("does not list the same endpoint twice when the override duplicates a default", () => {
    // Otherwise viem retries the same rate-limited node twice before failing over.
    expect(resolveRpcEndpoints(CHAIN_RPCS[0], CHAIN_RPCS)).to.deep.equal(
      CHAIN_RPCS
    );
  });

  it("refuses to return an empty list rather than failing later as a connection error", () => {
    expect(() => resolveRpcEndpoints(undefined, [])).to.throw(
      /no RPC endpoints resolved/
    );
  });
});
