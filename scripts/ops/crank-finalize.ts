import * as anchor from "@coral-xyz/anchor";
import { Connection, Keypair, PublicKey } from "@solana/web3.js";
import { readFileSync } from "fs";
import { homedir } from "os";
import { join } from "path";
const RPC = "https://api.mainnet-beta.solana.com";
const PDA = new PublicKey("6SLwu3pKvpnWMFrciN6nwZ9anie1C7LtUynaDNyP9utS");
const kp = Keypair.fromSecretKey(
  Uint8Array.from(
    JSON.parse(readFileSync(join(homedir(), ".config/solana/id.json"), "utf8"))
  )
);
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
(async () => {
  const conn = new Connection(RPC, "confirmed");
  const provider = new anchor.AnchorProvider(conn, new anchor.Wallet(kp), {
    commitment: "confirmed",
  });
  anchor.setProvider(provider);
  const idl = JSON.parse(
    readFileSync(
      join(__dirname, "..", "..", "target", "idl", "shillbot.json"),
      "utf8"
    )
  );
  const program = new anchor.Program(idl as anchor.Idl, provider);
  const gPda = PublicKey.findProgramAddressSync(
    [Buffer.from("shillbot_global")],
    program.programId
  )[0];
  const t: any = await (program.account as any).task.fetch(PDA);
  const g: any = await (program.account as any).globalState.fetch(gPda);
  const ZERO = PublicKey.default;
  const payee: PublicKey =
    t.payoutTo && !t.payoutTo.equals(ZERO) ? t.payoutTo : t.agent;
  console.log(
    "state:",
    Object.keys(t.state || {})[0],
    "| payment_amount:",
    t.paymentAmount.toString(),
    "| payee:",
    payee.toBase58()
  );
  const deadline = Number(t.challengeDeadline.toString());
  let now = Math.floor(Date.now() / 1000);
  console.log(
    "challenge_deadline:",
    deadline,
    "now:",
    now,
    "| wait:",
    Math.max(0, deadline - now + 2),
    "s"
  );
  while (now <= deadline + 1) {
    await sleep(2000);
    now = Math.floor(Date.now() / 1000);
  }
  const agentStatePda = PublicKey.findProgramAddressSync(
    [Buffer.from("agent_state"), t.agent.toBuffer()],
    program.programId
  )[0];
  const asInfo = await conn.getAccountInfo(agentStatePda);
  const remaining =
    asInfo && asInfo.owner.equals(program.programId)
      ? [{ pubkey: agentStatePda, isWritable: true, isSigner: false }]
      : [];
  console.log("AgentState passed:", remaining.length > 0);
  const balBefore = await conn.getBalance(payee, "confirmed");
  const sig = await (program.methods as any)
    .finalizeTask()
    .accountsPartial({
      task: PDA,
      globalState: gPda,
      agent: payee,
      client: t.client,
      treasury: g.treasury,
    })
    .remainingAccounts(remaining)
    .rpc();
  console.log("finalize tx:", sig);
  await conn.confirmTransaction(sig, "confirmed");
  const closed = await conn.getAccountInfo(PDA, "confirmed");
  const balAfter = await conn.getBalance(payee, "confirmed");
  console.log("task account closed?", closed === null || closed.lamports === 0);
  console.log(
    "payee balance delta:",
    balAfter - balBefore,
    "lamports (",
    (balAfter - balBefore) / 1e9,
    "SOL )"
  );
})().catch((e) => {
  console.log("ERR", String(e).slice(0, 400));
  process.exit(1);
});
