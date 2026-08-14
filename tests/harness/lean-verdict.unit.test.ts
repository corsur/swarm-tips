// Verdict classification for the lean-worker live e2e. The orchestrator's
// /tasks/:id mirror updates `state` BEFORE it backfills composite_score /
// payment_amount: on 2026-08-14 the attester accepted the worker's proof
// (score=1000000 on-chain, "proof checked; axioms: []") while the e2e read
// {state: "verified", composite_score: 0, payment_amount: 0} and declared the
// proof REJECTED. A single read cannot distinguish "rejected" from "mirror
// lagging" — only a settled read (fields populated) or a deadline can.

import { assert } from "chai";
import { leanVerdict } from "./lean-verdict";

describe("harness/lean-verdict", () => {
  it("accepted: terminal state with positive payment", () => {
    assert.equal(
      leanVerdict({
        state: "verified",
        composite_score: 1_000_000,
        payment_amount: 1_800_000,
      }),
      "accepted"
    );
    assert.equal(
      leanVerdict({
        state: "finalized",
        composite_score: 1_000_000,
        payment_amount: 1_800_000,
      }),
      "accepted"
    );
  });

  it("accepted: payment present even when the mirror never backfills a score", () => {
    // Live 2026-08-14 (final sweep): the worker's proof was ACCEPTED and paid
    // 18_000_000 lamports (state=finalized), but the orchestrator mirror
    // never populates composite_score for attested-path tasks — it stayed
    // null even after finalization. The payment is the pass condition (the
    // cell's own doctrine); requiring a mirrored score fails a PAID run.
    assert.equal(
      leanVerdict({
        state: "finalized",
        composite_score: 0,
        payment_amount: 18_000_000,
      }),
      "accepted"
    );
    assert.equal(
      leanVerdict({ state: "finalized", payment_amount: 18_000_000 }),
      "accepted"
    );
  });

  it("pending: not yet terminal", () => {
    assert.equal(leanVerdict({ state: "submitted" }), "pending");
    assert.equal(leanVerdict({ state: "open" }), "pending");
  });

  it("unsettled: terminal but score/payment still zero — the exact 2026-08-14 false-REJECTED read", () => {
    // Must NOT be classified as rejected on a single read: the attester had
    // already accepted on-chain; the mirror simply hadn't backfilled yet.
    assert.equal(
      leanVerdict({ state: "verified", composite_score: 0, payment_amount: 0 }),
      "unsettled"
    );
    assert.equal(leanVerdict({ state: "verified" }), "unsettled");
  });

  it("unsettled: score present but payment not yet backfilled — payment is the truth", () => {
    assert.equal(
      leanVerdict({
        state: "verified",
        composite_score: 1_000_000,
        payment_amount: 0,
      }),
      "unsettled"
    );
  });
});
