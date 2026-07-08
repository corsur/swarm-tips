// Shared deadline-boundary battery — the TypeScript/bankrun twin of
// evm/test/helpers/BoundaryBattery.sol. Every timed gate must be probed at
// deadline−1 / deadline / deadline+1, because the strict-vs-inclusive inequality
// at the boundary second is exactly where Solana↔EVM parity bugs hide. Replaces
// the copy-pasted three-`it` dead-second blocks that were triplicated across the
// hand-written shillbot suites.
//
// bankrun has no whole-VM snapshot/revert, and a successful gated action mutates
// or closes its subject — so each probe runs against a FRESH subject produced by
// `fresh()` (which also returns that subject's absolute deadline). This is the
// same independence the Solidity battery gets from vm.snapshotState().

import { assert } from "chai";

export interface BoundaryProbe<S> {
  /** Produce a fresh subject and its absolute unix-timestamp deadline. */
  fresh: () => Promise<{ subject: S; deadline: number }>;
  /** Warp the clock to an absolute unix timestamp (bankrun owns the clock). */
  warpTo: (ts: number) => Promise<void>;
  /** The ONE gated action under test; resolves on success, rejects when gated. */
  action: (subject: S) => Promise<void>;
  /** Pattern the rejection must match when the gate is closed. */
  deadError: RegExp;
}

async function expectOk(p: Promise<unknown>, when: string): Promise<void> {
  try {
    await p;
  } catch (e) {
    assert.fail(`expected action to succeed ${when}, got: ${String(e)}`);
  }
}

async function expectErr(
  p: Promise<unknown>,
  pattern: RegExp,
  when: string
): Promise<void> {
  try {
    await p;
    assert.fail(`expected ${pattern} ${when}, got success`);
  } catch (e: unknown) {
    assert.match(String(e), pattern, `wrong error ${when}`);
  }
}

/** Gate `t < deadline` (e.g. challenge_task): live at deadline−1, dead at the
 *  deadline second and after. */
export async function assertLiveStrictlyBefore<S>(
  p: BoundaryProbe<S>
): Promise<void> {
  {
    const { subject, deadline } = await p.fresh();
    await p.warpTo(deadline - 1);
    await expectOk(p.action(subject), "at deadline−1");
  }
  {
    const { subject, deadline } = await p.fresh();
    await p.warpTo(deadline);
    await expectErr(p.action(subject), p.deadError, "at the deadline second");
  }
  {
    const { subject, deadline } = await p.fresh();
    await p.warpTo(deadline + 1);
    await expectErr(p.action(subject), p.deadError, "at deadline+1");
  }
}

/** Gate `t <= deadline` (e.g. authority resolve_challenge inside its window):
 *  live at deadline−1 AND the deadline second, dead strictly after. */
export async function assertLiveThrough<S>(p: BoundaryProbe<S>): Promise<void> {
  {
    const { subject, deadline } = await p.fresh();
    await p.warpTo(deadline - 1);
    await expectOk(p.action(subject), "at deadline−1");
  }
  {
    const { subject, deadline } = await p.fresh();
    await p.warpTo(deadline);
    await expectOk(p.action(subject), "at the deadline second");
  }
  {
    const { subject, deadline } = await p.fresh();
    await p.warpTo(deadline + 1);
    await expectErr(p.action(subject), p.deadError, "at deadline+1");
  }
}

/** Gate `t > deadline` (e.g. finalize_task, resolve_challenge_default,
 *  expire_task): dead at deadline−1 and the deadline second, live strictly after. */
export async function assertLiveStrictlyAfter<S>(
  p: BoundaryProbe<S>
): Promise<void> {
  {
    const { subject, deadline } = await p.fresh();
    await p.warpTo(deadline - 1);
    await expectErr(p.action(subject), p.deadError, "at deadline−1");
  }
  {
    const { subject, deadline } = await p.fresh();
    await p.warpTo(deadline);
    await expectErr(p.action(subject), p.deadError, "at the deadline second");
  }
  {
    const { subject, deadline } = await p.fresh();
    await p.warpTo(deadline + 1);
    await expectOk(p.action(subject), "at deadline+1");
  }
}
