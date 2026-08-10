/**
 * Read-only: list every LeanProof (platform 10) task on mainnet with its
 * on-chain state and deadline, decoded via the IDL.
 *
 * Written to answer "is the trivial bounty claimed by the oracle authority
 * stuck forever?". It is not. Every platform-10 `deadline` is ~year 2126, so
 * expire_task's Open/Claimed branch can never fire — but that task is
 * SUBMITTED, and the Submitted branch keys off `submitted_at +
 * verification_timeout_seconds` (14d), not the deadline. It therefore releases
 * itself, and scripts/crank-stuck-shillbot-tasks.ts already cranks exactly that
 * case. emergency_return is not an alternative: it accepts Open/Claimed only.
 *
 * The claim that can never complete is the PAYOUT, not the lifecycle — the
 * agent is the oracle authority, so attestation trips the arms-length guard,
 * scores 0, and refunds the client. Which is the correct outcome.
 *
 * Decoded with `program.account.task.all()` rather than byte offsets — a
 * hand-computed offset already produced an implausible value once in this
 * codebase, and the IDL is the only thing that stays correct across an upgrade.
 */
import * as anchor from "@coral-xyz/anchor";
import * as fs from "fs";
import * as path from "path";

const idl = JSON.parse(
  fs.readFileSync(path.join(__dirname, "../target/idl/shillbot.json"), "utf8")
);
// Anchor renders a Rust enum as `{ variantName: {} }`, NOT as its discriminant.
// Reading it as a number yields NaN — which prints as a plausible-looking "?"
// rather than an error, exactly the class of silent mis-decode that a
// hand-computed byte offset already caused here once.
const stateName = (v: unknown): string =>
  v && typeof v === "object" ? Object.keys(v as object)[0] ?? "?" : String(v);

async function main() {
  const provider = anchor.AnchorProvider.env();
  const program = new anchor.Program(idl, provider);
  const now = Math.floor(Date.now() / 1000);
  const gs = await (program.account as any).globalState.all();
  const verifTimeout = gs.length
    ? Number(gs[0].account.verificationTimeoutSeconds)
    : NaN;
  console.log(`global verification_timeout_seconds ${verifTimeout}`);
  const all = await (program.account as any).task.all();
  const lean = all.filter((t: any) => Number(t.account.platform) === 10);
  console.log(
    `total tasks ${all.length} | platform-10 ${lean.length} | now ${now}`
  );
  for (const t of lean) {
    const a = t.account;
    const st = stateName(a.state);
    const dl = Number(a.deadline);
    console.log(
      [
        `task ${a.taskId.toString()}`,
        `state ${st}`,
        `agent ${a.agent?.toBase58?.() ?? "-"}`,
        `escrow ${Number(a.escrowLamports) / 1e9}`,
        `deadline ${dl} (${dl - now}s away)`,
        `submitted_at ${Number(a.submittedAt)}`,
        `verif_override ${Number(a.verificationTimeoutOverride)}`,
        dl < now ? "EXPIRABLE" : "live",
        `pda ${t.publicKey.toBase58()}`,
      ].join(" | ")
    );
  }
}
main().catch((e) => {
  console.error(e);
  process.exit(1);
});
