// Backend target: a phase StateView over the game-api off-chain view of an EVM
// same-chain match (/internal/evmgame/status + /committed). Pure — the HTTP reads
// are INJECTED so the mapping is unit-testable without a live backend; the live
// script supplies fetch-backed readers.
//
// The backend tracks MATCHMAKING, not on-chain game progress, so it maps onto the
// SAME coarse cross-layer phase the chain does — that shared phase is what lets
// assertCrossLayer catch backend/chain drift (the Step-5 Firestore-index bug
// class: backend says "still matching/waiting" while the chain has moved on).

import { Phase } from "./evm-target";
import { StateView } from "./assertions";

export interface BackendReader {
  /** /internal/evmgame/status -> "waiting" | "matched" (matchmaking state). */
  matchStatus(): Promise<string>;
  /** /internal/evmgame/committed -> both players committed on-chain yet. */
  bothCommitted(): Promise<boolean>;
}

/** Backend matchmaking state -> cross-layer phase. */
export function backendPhase(
  matchStatus: string,
  bothCommitted: boolean
): Phase {
  if (matchStatus === "waiting") return "Unmatched";
  if (matchStatus === "matched")
    return bothCommitted ? "BothCommitted" : "Live";
  throw new Error(`unknown backend match status '${matchStatus}'`);
}

/** Phase StateView for cross-layer agreement against an EVM phase view. */
export function backendPhaseView(read: BackendReader): StateView {
  return {
    async readStatus() {
      return backendPhase(await read.matchStatus(), await read.bothCommitted());
    },
    async readOutcomeKind() {
      return -1; // the backend does not adjudicate the on-chain outcome
    },
  };
}
