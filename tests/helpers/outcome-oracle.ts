// Single source of truth: the canonical oracle now lives in `sdk/outcome-oracle.ts`
// so it ships in the published `@corsur/coordination-game-sdk` package and is
// reusable cross-repo (coordination-app's Playwright/MCP harness imports it).
// This shim keeps the in-repo test imports (`./helpers/outcome-oracle`) working.
export * from "@swarm-tips/client/coordination-game";
