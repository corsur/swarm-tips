// The runtime seam. A SolanaRuntime is ONE Solana execution environment the
// harness steps run against; the same step code drives any implementation.
// Concrete runtimes: bankrun.ts (in-process, clock-warpable) and validator.ts
// (local validator, Slice 3). EVM + backend targets land in Slice 4.
//
// Steps (xchain-steps.ts, game-steps.ts) are written against this interface so a
// scenario is portable across runtimes — which is what makes the metamorphic
// (cross-target) verification in the property battery possible.

import { Program } from "@coral-xyz/anchor";
import { PublicKey } from "@solana/web3.js";
import { CoordinationGame } from "../../target/types/coordination_game";

export interface SolanaRuntime {
  readonly program: Program<CoordinationGame>;
  /** Default fee-payer / authority for setup transactions. */
  readonly payer: PublicKey;

  pda(seeds: (Buffer | Uint8Array)[]): PublicKey;
  getBalance(pk: PublicKey): Promise<bigint>;
  now(): Promise<number>;

  /**
   * Warp the clock to an absolute unix timestamp. Only runtimes that own the
   * clock (bankrun) support this; validator runtimes throw, so deadline tests
   * stay bankrun-only.
   */
  warpTo(ts: number): Promise<void>;

  /** Move lamports from the runtime payer to `recipient` (test funding helper). */
  fund(recipient: PublicKey, lamports: number | bigint): Promise<void>;
}
