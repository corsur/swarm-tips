// Verdict classification for the lean-worker live e2e.
//
// The orchestrator's /tasks/:id mirror updates `state` BEFORE it backfills
// composite_score / payment_amount. On 2026-08-14 the attester ACCEPTED the
// worker's proof (score=1000000 on-chain, "proof checked; axioms: []") while
// the e2e's single read saw {state: "verified", score: 0, payment: 0} and
// declared the proof rejected. A rejected proof and a lagging mirror are
// indistinguishable on one read — only a settled read (fields populated) or a
// bounded deadline separates them, so the poll loop must keep going on
// "unsettled" and fail only when the deadline expires.

export interface LeanTaskRead {
  state?: string;
  composite_score?: number | null;
  payment_amount?: number | null;
}

export type LeanVerdict = "pending" | "unsettled" | "accepted";

const TERMINAL = new Set(["verified", "finalized"]);

export function leanVerdict(task: LeanTaskRead): LeanVerdict {
  if (!TERMINAL.has(String(task.state ?? ""))) return "pending";
  const score = Number(task.composite_score ?? 0);
  const payment = Number(task.payment_amount ?? 0);
  return score > 0 && payment > 0 ? "accepted" : "unsettled";
}
