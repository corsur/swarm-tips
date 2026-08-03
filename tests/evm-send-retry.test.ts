import { expect } from "chai";
import { classifySendFailure, planRetry } from "./live/evm-send-retry";

describe("classifySendFailure", () => {
  it("classifies the verbatim nonce rejection that killed escrow-matrix cells", () => {
    expect(
      classifySendFailure(
        "ContractFunctionExecutionError: Nonce provided for the transaction is lower than the current nonce of the account."
      )
    ).to.equal("nonce");
  });

  it("classifies the two other transport shapes seen in the same run", () => {
    expect(
      classifySendFailure("ContractFunctionExecutionError: RPC Request failed.")
    ).to.equal("transient");
    expect(
      classifySendFailure(
        "ContractFunctionExecutionError: The request took too long to respond."
      )
    ).to.equal("transient");
  });

  it("classifies replacement/underpriced as its own kind", () => {
    expect(classifySendFailure("replacement transaction underpriced")).to.equal(
      "underpriced"
    );
    expect(classifySendFailure("transaction underpriced")).to.equal(
      "underpriced"
    );
  });

  it("never retries a revert", () => {
    expect(classifySendFailure("execution reverted: NotTaskClient")).to.equal(
      "fatal"
    );
  });

  it("treats a revert mentioning nonce as fatal, not as a nonce failure", () => {
    // Ordering matters: a resend cannot fix a revert, and resending a
    // value-moving call because its revert string contained "nonce" is exactly
    // the bug this ordering prevents.
    expect(classifySendFailure("execution reverted: BadNonce()")).to.equal(
      "fatal"
    );
  });

  it("defaults an unrecognized failure to transient", () => {
    expect(classifySendFailure("socket hang up")).to.equal("transient");
  });
});

describe("planRetry", () => {
  it("refreshes the nonce for nonce and underpriced failures", () => {
    expect(planRetry(1, 4, "nonce").refreshNonce).to.equal(true);
    expect(planRetry(1, 4, "underpriced").refreshNonce).to.equal(true);
  });

  it("retries a transient failure WITHOUT re-reading the nonce", () => {
    // The nonce was fine; re-reading it mid-flight can pick up a pending tx
    // from another sender path and skip a slot.
    const step = planRetry(1, 4, "transient");
    expect(step.retry).to.equal(true);
    expect(step.refreshNonce).to.equal(false);
  });

  it("stops immediately on a fatal failure even with attempts left", () => {
    expect(planRetry(1, 4, "fatal").retry).to.equal(false);
  });

  it("stops at the attempt ceiling", () => {
    expect(planRetry(4, 4, "nonce").retry).to.equal(false);
    expect(planRetry(3, 4, "nonce").retry).to.equal(true);
  });

  it("backs off exponentially and caps at 8s", () => {
    expect(planRetry(1, 9, "transient").backoffMs).to.equal(1000);
    expect(planRetry(2, 9, "transient").backoffMs).to.equal(2000);
    expect(planRetry(3, 9, "transient").backoffMs).to.equal(4000);
    expect(planRetry(4, 9, "transient").backoffMs).to.equal(8000);
    expect(planRetry(8, 9, "transient").backoffMs).to.equal(8000);
  });

  it("rejects a non-positive attempt number", () => {
    expect(() => planRetry(0, 4, "nonce")).to.throw(/attempt must be >= 1/);
  });
});
