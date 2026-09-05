// Single source of truth: the canonical task-payout oracle lives in
// `sdk/task-outcome-oracle.ts` so it ships in the published SDK and is reusable
// cross-repo (coordination-app's Playwright/MCP harness imports it). This shim
// keeps in-repo test imports (`./helpers/task-outcome-oracle`) working — mirrors
// the `outcome-oracle` shim for the coordination game.
export * from "@swarm-tips/client/shillbot";
