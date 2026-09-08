// Single source of truth: the canonical oracle ships in the published unified
// client package and is reusable cross-repo. The package supports both ESM and
// the CommonJS loader used by the Anchor harness.
// This shim keeps the in-repo test imports (`./helpers/outcome-oracle`) working.
export * from "@swarm-tips/client/coordination-game";
