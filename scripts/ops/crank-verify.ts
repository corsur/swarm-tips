import {
  Connection,
  Keypair,
  PublicKey,
  VersionedTransaction,
} from "@solana/web3.js";
import { readFileSync } from "fs";
import { homedir } from "os";
import { join } from "path";
import { createHash } from "crypto";
const RPC = "https://api.mainnet-beta.solana.com",
  MCP = "https://mcp.swarm.tips";
const TASK =
  "3534bc4d-644e-4b95-b520-9a038261ce18:b9216590-eda0-4400-a1fc-a24ab836acb3";
const FEED = "En9CNFh7p1VDJ5CAvRiKZbTFL8dH5u9s2shC5TS4T6qQ",
  GS = "FV6v93WcTB1G8xS8Z1kATR12oRKyapQ74uuRwmfTqQud",
  PDA = "6SLwu3pKvpnWMFrciN6nwZ9anie1C7LtUynaDNyP9utS";
const kp = Keypair.fromSecretKey(
  Uint8Array.from(
    JSON.parse(readFileSync(join(homedir(), ".config/solana/id.json"), "utf8"))
  )
);
(async () => {
  const conn = new Connection(RPC, "confirmed");
  const before = (await conn.getAccountInfo(new PublicKey(PDA)))!.data[80];
  console.log("task state before:", before, "(2=Submitted,3=Verified)");
  const score = 1000000;
  const sb = Buffer.alloc(8);
  sb.writeBigUInt64LE(BigInt(score));
  const hash = createHash("sha256")
    .update(Buffer.from(TASK, "utf8"))
    .update(sb)
    .digest("hex");
  const body = {
    task_id: TASK,
    payer: kp.publicKey.toBase58(),
    score,
    hash,
    task_pda: PDA,
    feed: FEED,
    global_state: GS,
    network: "mainnet",
  };
  const r = await fetch(`${MCP}/internal/build-verify-tx`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!r.ok) {
    console.log("BUILD FAIL", r.status, (await r.text()).slice(0, 300));
    process.exit(1);
  }
  const vtx = VersionedTransaction.deserialize(
    Buffer.from(JSON.parse(await r.text()).transaction, "base64")
  );
  vtx.sign([kp]);
  const sig = await conn.sendRawTransaction(vtx.serialize(), {
    skipPreflight: false,
    maxRetries: 5,
  });
  console.log("verify tx sent:", sig);
  await conn.confirmTransaction(sig, "confirmed");
  const after = (await conn.getAccountInfo(new PublicKey(PDA)))!.data[80];
  console.log("task state after:", after, "(3=Verified)");
})().catch((e) => {
  console.log("ERR", String(e).slice(0, 400));
  process.exit(1);
});
