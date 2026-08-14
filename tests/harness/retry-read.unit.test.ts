// Read-after-write consistency for the LIVE (devnet) path of the shillbot
// step helpers. Each step does `before = read(); rpc(); after = read()` —
// against a load-balanced devnet RPC the post-write read can hit a node that
// has not seen the tx yet, producing the two failure shapes the 2026-08-13
// outcome-matrix run hit on 4/6 cells:
//   - "state machine: illegal transition Claimed -> Claimed" (stale status),
//   - "Account does not exist or has no data <task PDA>" (account not yet
//     visible — it existed minutes later, owned by the program).
// Bankrun reads are authoritative on the first attempt, so these helpers must
// be zero-cost there (no delay when the first read is already fresh).

import { assert } from "chai";
import { readWhenVisible, readWhenAdvanced } from "./retry-read";

/** Reader that throws `missing` times, then yields values in order. */
function flakyReader<T>(missing: number, values: T[]): () => Promise<T> {
  let calls = 0;
  return async () => {
    calls += 1;
    if (calls <= missing) {
      throw new Error("Account does not exist or has no data FAKE111");
    }
    return values[Math.min(calls - missing, values.length) - 1] as T;
  };
}

describe("harness/retry-read (devnet read-after-write)", () => {
  it("readWhenVisible returns the first successful read", async () => {
    const value = await readWhenVisible(flakyReader(2, ["Open"]), {
      attempts: 5,
      delayMs: 1,
    });
    assert.equal(value, "Open");
  });

  it("readWhenVisible does not retry when the first read succeeds (bankrun path)", async () => {
    let calls = 0;
    const value = await readWhenVisible(
      async () => {
        calls += 1;
        return "Open";
      },
      { attempts: 5, delayMs: 1_000_000 } // a retry would hang the test
    );
    assert.equal(value, "Open");
    assert.equal(calls, 1);
  });

  it("readWhenVisible surfaces the underlying error once attempts are exhausted", async () => {
    try {
      await readWhenVisible(flakyReader(10, ["never"]), {
        attempts: 3,
        delayMs: 1,
      });
      assert.fail("expected the missing-account error to surface");
    } catch (e) {
      assert.match(String(e), /does not exist/);
    }
  });

  it("readWhenAdvanced retries past a stale pre-tx status", async () => {
    // The exact live failure: tx confirmed, first post-write read still says
    // the OLD status. A single read asserted 'Claimed -> Claimed'.
    const status = await readWhenAdvanced(
      flakyReader(0, ["Claimed", "Claimed", "Submitted"]),
      "Claimed",
      { attempts: 5, delayMs: 1 }
    );
    assert.equal(status, "Submitted");
  });

  it("readWhenAdvanced returns the stale status after bounded attempts (genuine stalls still fail loudly downstream)", async () => {
    const status = await readWhenAdvanced(
      flakyReader(0, ["Claimed"]),
      "Claimed",
      { attempts: 3, delayMs: 1 }
    );
    // NOT an error: the caller's assertLegalTransition("Claimed","Claimed")
    // is the loud failure, carrying the real evidence.
    assert.equal(status, "Claimed");
  });

  it("readWhenAdvanced also rides through a not-yet-visible account", async () => {
    const status = await readWhenAdvanced(
      flakyReader(2, ["Submitted"]),
      "Claimed",
      { attempts: 6, delayMs: 1 }
    );
    assert.equal(status, "Submitted");
  });

  it("readWhenAdvanced does not retry when the first read already advanced (bankrun path)", async () => {
    let calls = 0;
    const status = await readWhenAdvanced(
      async () => {
        calls += 1;
        return "Submitted";
      },
      "Claimed",
      { attempts: 5, delayMs: 1_000_000 }
    );
    assert.equal(status, "Submitted");
    assert.equal(calls, 1);
  });
});
