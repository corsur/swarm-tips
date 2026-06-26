// EVM same-chain target: StateViews over the deployed CoordinationGame (Base
// Sepolia). Pure — the on-chain reads are INJECTED, so the adapter logic is
// unit-testable without viem/network, and the live script (tests/live/*) supplies
// viem-backed readers. The EVM game shares the same status vocabulary and the
// same outcome oracle as the Solana same-chain game, which is exactly what makes
// the metamorphic (cross-runtime) battery check meaningful.

import {
  Transcript as OracleInput,
  deriveClaimOutcome,
  TERMINAL_STEP_COUNT,
} from "../helpers/outcome-oracle";
import { StateView } from "./assertions";

/** CoordinationGame.sol GameStatus codes -> canonical labels (== Solana labels). */
export const EVM_GAME_STATUS: Record<number, string> = {
  0: "None",
  1: "Pending",
  2: "Active",
  3: "Committing",
  4: "Revealing",
  5: "Resolved",
};

export function evmGameStatusLabel(code: number): string {
  const label = EVM_GAME_STATUS[code];
  if (label === undefined) throw new Error(`unknown EVM game status ${code}`);
  return label;
}

/** Coarse cross-layer phase BOTH the chain and the backend can observe. */
export type Phase = "Unmatched" | "Live" | "BothCommitted" | "Resolved";

/** On-chain game status -> cross-layer phase. */
export function evmGamePhase(status: number): Phase {
  if (status <= 0) return "Unmatched";
  if (status <= 3) return "Live"; // Pending/Active/Committing
  if (status === 4) return "BothCommitted"; // Revealing
  return "Resolved";
}

/** The terminal-transcript fields read off an EVM game account. */
export interface EvmGameFields {
  matchupType: number;
  p1Guess: number;
  p2Guess: number;
  firstCommitter: number;
}

export interface EvmGameReader {
  status(): Promise<number>;
  fields(): Promise<EvmGameFields>;
}

/** StateView for legality + oracle checks (raw game label + derived outcome). */
export function evmGameView(read: EvmGameReader): StateView {
  return {
    async readStatus() {
      return evmGameStatusLabel(await read.status());
    },
    async readOutcomeKind() {
      const status = await read.status();
      if (status < 5) return -1;
      const f = await read.fields();
      const input: OracleInput = {
        stepCount: TERMINAL_STEP_COUNT,
        matchupType: f.matchupType,
        p1Guess: f.p1Guess,
        p2Guess: f.p2Guess,
        firstCommitter: f.firstCommitter,
      };
      return deriveClaimOutcome(input);
    },
  };
}

/** Phase StateView for cross-layer agreement (readStatus returns the phase). */
export function evmPhaseView(read: Pick<EvmGameReader, "status">): StateView {
  return {
    async readStatus() {
      return evmGamePhase(await read.status());
    },
    async readOutcomeKind() {
      return -1; // phase views are for status agreement, not outcome
    },
  };
}
