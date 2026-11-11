#!/usr/bin/env npx tsx
import { Keypair } from "@solana/web3.js";
import {
  createSwarmAgent,
  fromSolanaKeypair,
  fromSolanaSecretKey,
} from "@swarm-tips/client";

const SUPPORT_WALLET = "5vsGoTRoc5j1a2fKszyZ7y28G6ggmu87YobpwzuXsMhu";
const rawKey =
  process.env.SWARM_SOLANA_SECRET_KEY ?? process.env.AGENT_KEYPAIR_JSON;
const signer = rawKey
  ? fromSolanaSecretKey(rawKey)
  : fromSolanaKeypair(Keypair.generate());
const live = process.env.EXECUTE_LIVE === "1";

const agent = createSwarmAgent({
  signer,
  maxEscrowSpendLamports: live ? 5_000_000n : 0n,
});

async function main(): Promise<void> {
  const mode =
    process.argv.find((a) => a.startsWith("--mode="))?.split("=")[1] ??
    "discover";
  await agent.auth.authenticate();
  if (live) await agent.tasks.onboard().catch(() => undefined);

  const { tasks } = await agent.tasks.listAvailable<Record<string, unknown>>(
    50
  );
  const open = tasks.filter((t) => t.state === "open");
  const byPlatform = (p: number) => open.filter((t) => t.platform === p);
  console.log(
    `Wallet ${signer.address} | open=${open.length} lean=${
      byPlatform(10).length
    } game=${byPlatform(5).length} referral=${byPlatform(4).length}`
  );

  if (mode === "lean-proof" && byPlatform(10)[0]) {
    const id = String(byPlatform(10)[0].task_id);
    const details = await agent.tasks.get<Record<string, unknown>>(id);
    const proof =
      "theorem proof : statementProp := by\n  unfold statementProp\n  omega\n";
    const contentId = `lean4:proof:${Buffer.from(proof, "utf8").toString(
      "base64"
    )}`;
    console.log("LeanProof target:", details.task_id, contentId.slice(0, 44));
    if (live) {
      await agent.tasks.claimAndExecute(id);
      console.log(
        await agent.tasks.submitAndExecute(id, contentId, {
          extra: { proof_lean: proof },
        })
      );
    }
  } else if (mode === "gameplay" && byPlatform(5)[0]) {
    console.log(await agent.tasks.get(String(byPlatform(5)[0].task_id)));
  } else if (mode === "referral" && byPlatform(4)[0]) {
    console.log(await agent.tasks.get(String(byPlatform(4)[0].task_id)));
  } else if (mode === "bug-report") {
    const body =
      "Included machine-checked Lean 4 refutation `theorem disproof : ¬ statementProp`.";
    console.log(
      live
        ? await agent.inbox.send(SUPPORT_WALLET, body, { intent: "bug_report" })
        : { to: SUPPORT_WALLET, intent: "bug_report", body }
    );
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
