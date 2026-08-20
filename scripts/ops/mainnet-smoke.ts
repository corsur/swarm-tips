/**
 * Shillbot mainnet earn→verify→pay smoke.
 *
 * DRY-RUN by default: runs the cheap, no-spend PREFLIGHT that catches the
 * regressions which actually break the lifecycle (RPC down, Switchboard feed
 * unconfigured, a too-tight staleness window that locks agents out, a paused
 * platform, MCP build-verify unreachable), then prints the per-platform
 * create→claim→submit→verify→finalize plan with minimal escrow + the
 * emergency_return recovery each task would use. Spends nothing.
 *
 * --execute would run the real minimal-escrow lifecycle on mainnet; it is
 * intentionally GATED: it refuses to spend and tells you to get sign-off,
 * because real SOL leaves the authority wallet. Wire the real steps in only
 * after that sign-off (reuse scripts/ops/{crank-verify,crank-finalize}.ts and
 * scripts/e2e/recoupment-loop-devnet.ts).
 *
 * Run (deps in scripts/ops via `npm i` there, or from a dir with anchor+web3):
 *   npx tsx scripts/ops/mainnet-smoke.ts            # preflight (no spend)
 *   npx tsx scripts/ops/mainnet-smoke.ts --execute  # refuses; asks for sign-off
 */
import * as anchor from "@coral-xyz/anchor";
import { BN } from "@coral-xyz/anchor";
import { Connection, Keypair, PublicKey } from "@solana/web3.js";
import { readFileSync } from "fs";
import { homedir } from "os";
import { join } from "path";

const RPC = process.env.MAINNET_RPC ?? "https://api.mainnet-beta.solana.com";
const MCP = process.env.MCP_URL ?? "https://mcp.swarm.tips";
const EXECUTE = process.argv.includes("--execute");

// In-scope launched platforms (YouTube/X are not launched — out of scope).
const PLATFORMS: { id: number; name: string; content: string }[] = [
  { id: 4, name: "Referral", content: "a funded shillbot campaign UUID" },
  { id: 5, name: "GamePlay", content: "a resolved coordination.game game_id" },
  { id: 9, name: "Website", content: "a URL whose footer links swarm.tips + task nonce" },
  { id: 10, name: "LeanProof", content: "a Lean statement (attester path)" },
];

function loadKeypair(name: string): Keypair {
  return Keypair.fromSecretKey(
    Uint8Array.from(JSON.parse(readFileSync(join(homedir(), ".config/solana", name), "utf8"))),
  );
}

class Checks {
  failures = 0;
  check(ok: boolean, label: string, detail = ""): void {
    console.log(`  ${ok ? "✓" : "✗"} ${label}${detail ? ` — ${detail}` : ""}`);
    if (!ok) this.failures++;
  }
}

async function rpcHealthy(conn: Connection): Promise<boolean> {
  try {
    const r = await fetch(RPC, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "getHealth" }),
    });
    const j = (await r.json()) as { result?: string };
    return j.result === "ok";
  } catch {
    return false;
  }
}

async function mcpReachable(): Promise<boolean> {
  try {
    const r = await fetch(MCP, { method: "GET" });
    return r.status < 500;
  } catch {
    return false;
  }
}

(async () => {
  const authority = loadKeypair("id.json");
  const conn = new Connection(RPC, "confirmed");
  const provider = new anchor.AnchorProvider(conn, new anchor.Wallet(authority), {
    commitment: "confirmed",
  });
  anchor.setProvider(provider);
  const idl = JSON.parse(
    readFileSync(join(__dirname, "..", "..", "target", "idl", "shillbot.json"), "utf8"),
  );
  const program = new anchor.Program(idl as anchor.Idl, provider);
  const globalPda = PublicKey.findProgramAddressSync(
    [Buffer.from("shillbot_global")],
    program.programId,
  )[0];
  const g: any = await (program.account as any).globalState.fetch(globalPda);
  const ZERO = PublicKey.default;

  console.log("=== Shillbot mainnet smoke — PREFLIGHT (no spend) ===");
  console.log(`program ${program.programId.toBase58()}  global ${globalPda.toBase58()}\n`);

  const c = new Checks();
  c.check(await rpcHealthy(conn), "RPC healthy", RPC);
  c.check(await mcpReachable(), "MCP reachable (build-verify host)", MCP);
  c.check(!g.authority.equals(ZERO), "GlobalState.authority configured", g.authority.toBase58());
  c.check(
    !g.switchboardFeed.equals(ZERO),
    "Switchboard feed configured (oracle verify fails closed if unset)",
    g.switchboardFeed.toBase58(),
  );
  // The verify window is [submitted_at + delay ± staleness]. If staleness is
  // shorter than the verification timeout, verify CLOSES before the task expires
  // — an agent who doesn't verify in that window is locked out with escrow still
  // live (the exact footgun that stranded the referral task at a 24h window).
  // Sane = staleness covers the whole task lifetime.
  const stale = (g.stalenessWindowSeconds as BN).toNumber();
  const vtimeout = (g.verificationTimeoutSeconds as BN).toNumber();
  c.check(
    stale >= vtimeout,
    "verify window spans task lifetime (no staleness lockout)",
    `staleness ${(stale / 86400).toFixed(1)}d vs timeout ${(vtimeout / 86400).toFixed(1)}d`,
  );
  c.check(!g.paused, "protocol not paused");
  const minEscrow = (g.minEscrowLamports as BN).toNumber();
  console.log(`  · min_escrow_lamports = ${minEscrow} (${minEscrow / 1e9} SOL)`);
  console.log(`  · attestation_delay = ${(g.attestationDelaySeconds as BN).toString()}s\n`);

  console.log("=== per-platform lifecycle plan (create→claim→submit→verify→finalize) ===");
  const escrow = Math.max(minEscrow, 1_000_000); // >= min, ~0.001 SOL floor
  for (const p of PLATFORMS) {
    const paused = ((g.pausedPlatforms as number) & (1 << p.id)) !== 0;
    console.log(
      `  [${p.id}] ${p.name}: escrow ${escrow / 1e9} SOL | content = ${p.content}` +
        (paused ? "  ⚠ PLATFORM PAUSED" : ""),
    );
    console.log(
      `        recovery: emergency_return (Open/Claimed) or expire_task returns escrow to client`,
    );
  }
  const total = (escrow * PLATFORMS.length) / 1e9;
  console.log(`\n  estimated max escrow at risk (all recoverable): ~${total} SOL + gas`);

  console.log(`\nPREFLIGHT ${c.failures === 0 ? "PASS" : `FAIL (${c.failures})`}`);

  if (EXECUTE) {
    console.log(
      "\n--execute requested — REFUSED. The real lifecycle spends mainnet SOL from the\n" +
        "authority wallet. Get explicit spend sign-off, then wire the real steps here\n" +
        "(create_task → claim_task → submit_work → verify_task → finalize_task, with an\n" +
        "emergency_return sweep) reusing scripts/ops/crank-verify.ts + crank-finalize.ts.",
    );
  }
  process.exit(c.failures === 0 ? 0 : 1);
})().catch((e) => {
  console.error("ERR", String(e).slice(0, 400));
  process.exit(1);
});
