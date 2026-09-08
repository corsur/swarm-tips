// Single source of truth: the canonical task-payout oracle ships in the unified
// client package and is reusable cross-repo. This shim keeps in-repo test imports
// (`./helpers/task-outcome-oracle`) working.
export * from "@swarm-tips/client/shillbot";
