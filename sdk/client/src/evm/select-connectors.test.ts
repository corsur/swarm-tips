import { describe, it, expect } from "vitest";
import { selectConnectors } from "./wallet.js";

/**
 * Precedence guard for the connector set.
 *
 * The E2E test connector must outrank app-supplied connectors: a live testnet
 * sweep drives real value, and letting RainbowKit's modal (or a real wallet)
 * take over mid-cell would strand stake in a wallet the harness cannot reach.
 * The app-supplied path exists so a RainbowKit UI and the wagmi config agree on
 * the wallet list — registering a connector wagmi-side alone leaves RainbowKit's
 * modal empty.
 */
describe("selectConnectors", () => {
  it("gives the E2E test connector absolute priority", () => {
    expect(selectConnectors("test", ["app1", "app2"], ["builtin"])).toEqual([
      "test",
    ]);
  });

  it("uses app-supplied connectors when not in test mode", () => {
    expect(selectConnectors(null, ["app1", "app2"], ["builtin"])).toEqual([
      "app1",
      "app2",
    ]);
  });

  it("falls back to built-ins when the app supplies none", () => {
    expect(selectConnectors(null, undefined, ["builtin"])).toEqual(["builtin"]);
  });

  it("treats an EMPTY app list as 'supplied nothing', not as 'no wallets'", () => {
    // Reachable in a real build: the frontend skips connectorsForWallets when no
    // projectId is configured. Returning [] would leave wagmi with no connector
    // at all, so nobody could connect — worse than the bug being fixed.
    expect(selectConnectors(null, [], ["builtin"])).toEqual(["builtin"]);
  });
});
