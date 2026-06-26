// Realized value-movement + game-transcript recorders for the e2e harness.
//
// The harness verifies COMPOSED scenarios whose end state is variable — it
// depends on which steps were combined. So we never hardcode "expect outcome X".
// Instead each step records what ACTUALLY moved (Ledger) and what ACTUALLY
// happened (Transcript), and the property battery (assertions.ts) checks
// path-independent relations over those realized records. See the harness README
// for the verification model.

import {
  Transcript as OracleInput,
  UNREVEALED,
  TERMINAL_STEP_COUNT,
} from "../helpers/outcome-oracle";

/**
 * A balance-bearing account a scenario touches. `protocol` accounts are
 * program-controlled (the game/match PDA, xpool, treasury, prize pool) and must
 * conserve value EXACTLY. `player` accounts also pay network gas, so they
 * reconcile only modulo a measured fee.
 */
export type AccountKind = "protocol" | "player";

interface LedgerEntry {
  kind: AccountKind;
  before: bigint;
  after: bigint | null;
}

/** Accumulates opening/closing balances of every account a scenario touches so
 *  the battery can assert conservation regardless of the path taken. */
export class Ledger {
  private entries = new Map<string, LedgerEntry>();

  /** Snapshot an account's opening balance. Idempotent per label. */
  open(label: string, kind: AccountKind, balance: bigint): void {
    this.entries.set(label, { kind, before: balance, after: null });
  }

  /** Snapshot an account's closing balance. */
  close(label: string, balance: bigint): void {
    const e = this.entries.get(label);
    if (!e) throw new Error(`Ledger.close: '${label}' was never opened`);
    e.after = balance;
  }

  /** Net change for one account (after - before). */
  delta(label: string): bigint {
    const e = this.entries.get(label);
    if (!e) throw new Error(`Ledger.delta: '${label}' was never opened`);
    if (e.after === null)
      throw new Error(`Ledger.delta: '${label}' not closed`);
    return e.after - e.before;
  }

  /** Sum of deltas over all accounts of a given kind. Throws if any are unclosed. */
  netByKind(kind: AccountKind): bigint {
    let sum = 0n;
    for (const [label, e] of this.entries) {
      if (e.kind !== kind) continue;
      if (e.after === null) throw new Error(`Ledger: '${label}' not closed`);
      sum += e.after - e.before;
    }
    return sum;
  }

  labels(): string[] {
    return [...this.entries.keys()];
  }
}

/**
 * The realized game transcript — what actually happened, in order — from which
 * the shared oracle DERIVES the expected outcome (never hardcoded). Produces the
 * `outcome-oracle` `Transcript` input shape via {@link toOracleInput}.
 */
export class Transcript {
  private commitOrder: (1 | 2)[] = [];
  private guesses = new Map<1 | 2, number>();
  private matchupType = UNREVEALED;

  /** Record a commit; the first commit fixes `firstCommitter`. */
  commit(player: 1 | 2): void {
    if (!this.commitOrder.includes(player)) this.commitOrder.push(player);
  }

  /** Record a reveal. The first reveal also fixes the matchup type. */
  reveal(player: 1 | 2, guess: number, matchupType?: number): void {
    this.guesses.set(player, guess);
    if (matchupType !== undefined && this.matchupType === UNREVEALED) {
      this.matchupType = matchupType;
    }
  }

  get firstCommitter(): number {
    return this.commitOrder[0] ?? UNREVEALED;
  }

  /** Protocol step count: 0 none, 1 one commit, 2 both commit, 3 both+one
   *  reveal, 4 terminal (both revealed). Mirrors cert_schema semantics. */
  get stepCount(): number {
    const commits = this.commitOrder.length;
    const reveals = this.guesses.size;
    if (commits === 0) return 0;
    if (commits === 1) return 1;
    if (reveals === 0) return 2;
    if (reveals === 1) return 3;
    return TERMINAL_STEP_COUNT;
  }

  toOracleInput(): OracleInput {
    return {
      stepCount: this.stepCount,
      matchupType: this.matchupType,
      p1Guess: this.guesses.get(1) ?? UNREVEALED,
      p2Guess: this.guesses.get(2) ?? UNREVEALED,
      firstCommitter: this.firstCommitter,
    };
  }
}
