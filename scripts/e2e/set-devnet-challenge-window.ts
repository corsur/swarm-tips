/**
 * One-shot: set the DEVNET GlobalState.challenge_window_seconds to a compressed
 * value PERSISTENTLY (no restore), so every devnet task's on-chain finalize gate
 * (challenge_deadline = verified_at + global window) is reachable inside the E2E
 * budget. The per-task override can't do this (create_task bounds it to
 * [3600,604800]); the global only needs > 0. Admin = id.json (GlobalState authority).
 *
 * Run: npx tsx scripts/e2e/set-devnet-challenge-window.ts
 */
import { BN } from "@coral-xyz/anchor";
import { connectDevnet, snapshotParams, applyParams } from "./devnet-harness";

const TARGET_SECS = 30;

async function main(): Promise<void> {
  const h = connectDevnet();
  const before = await snapshotParams(h);
  console.log(
    `authority: ${h.authority.publicKey.toBase58()}\n` +
      `challenge_window_seconds BEFORE: ${before.challengeWindowSeconds.toString()}`
  );
  if (before.challengeWindowSeconds.eq(new BN(TARGET_SECS))) {
    console.log(`already ${TARGET_SECS}s — no-op`);
    return;
  }
  await applyParams(h, {
    ...before,
    challengeWindowSeconds: new BN(TARGET_SECS),
  });
  const after = await snapshotParams(h);
  console.log(
    `challenge_window_seconds AFTER:  ${after.challengeWindowSeconds.toString()}`
  );
  if (!after.challengeWindowSeconds.eq(new BN(TARGET_SECS))) {
    throw new Error("verification failed: window not set");
  }
  console.log("OK");
}

main().catch((e: unknown) => {
  console.error(e);
  process.exit(1);
});
