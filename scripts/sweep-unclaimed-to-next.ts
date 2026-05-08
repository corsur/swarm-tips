/**
 * One-shot: sweep unclaimed lamports from a finalized tournament into the
 * destination tournament's prize pool. Authority-gated; requires the
 * GlobalConfig.authority keypair (id.json on mainnet).
 *
 * The on-chain handler enforces: src is finalized, src.end_time + 90 days
 * has elapsed, dest is not finalized, dest is within its active window.
 *
 * Usage:
 *   ANCHOR_WALLET=~/.config/solana/id.json \
 *   ANCHOR_PROVIDER_URL=https://api.mainnet-beta.solana.com \
 *   SRC_TOURNAMENT_ID=1 DEST_TOURNAMENT_ID=2 \
 *     npx ts-node scripts/sweep-unclaimed-to-next.ts
 */

import * as anchor from "@coral-xyz/anchor";
import { PublicKey } from "@solana/web3.js";
import * as fs from "fs";
import * as path from "path";

const idl = JSON.parse(
  fs.readFileSync(
    path.join(__dirname, "../target/idl/coordination_game.json"),
    "utf8"
  )
);

const SRC_ID = BigInt(process.env.SRC_TOURNAMENT_ID ?? "1");
const DEST_ID = BigInt(process.env.DEST_TOURNAMENT_ID ?? "2");

function pdaForTournament(programId: PublicKey, id: bigint): PublicKey {
  const buf = Buffer.alloc(8);
  buf.writeBigUInt64LE(id);
  const [pda] = PublicKey.findProgramAddressSync(
    [Buffer.from("tournament"), buf],
    programId
  );
  return pda;
}

async function main() {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = new anchor.Program(idl as anchor.Idl, provider);
  const wallet = provider.wallet.publicKey;

  console.log(`Wallet:  ${wallet.toBase58()}`);
  console.log(`RPC:     ${provider.connection.rpcEndpoint}`);
  console.log(`Sweep T#${SRC_ID} → T#${DEST_ID}`);
  console.log();

  const srcPda = pdaForTournament(program.programId, SRC_ID);
  const destPda = pdaForTournament(program.programId, DEST_ID);
  const [globalConfigPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("global_config")],
    program.programId
  );

  const src = await (program.account as any).tournament.fetch(srcPda);
  const dest = await (program.account as any).tournament.fetch(destPda);
  const srcLamports = await provider.connection.getBalance(srcPda);
  const destLamportsBefore = await provider.connection.getBalance(destPda);

  console.log(`src finalized:           ${src.finalized}`);
  console.log(
    `src end_time:            ${new Date(
      Number(src.endTime) * 1000
    ).toISOString()}`
  );
  console.log(
    `src lamports on-chain:   ${srcLamports} (${srcLamports / 1e9} SOL)`
  );
  console.log(`dest finalized:          ${dest.finalized}`);
  console.log(
    `dest active window:      ${new Date(
      Number(dest.startTime) * 1000
    ).toISOString()} → ${new Date(Number(dest.endTime) * 1000).toISOString()}`
  );
  console.log(`dest prize_lamports:     ${dest.prizeLamports.toString()}`);
  console.log(`dest lamports on-chain:  ${destLamportsBefore}`);

  const sig = await (program.methods as any)
    .sweepUnclaimedToNext()
    .accountsPartial({
      srcTournament: srcPda,
      destTournament: destPda,
      globalConfig: globalConfigPda,
      authority: wallet,
    })
    .rpc();

  console.log(`\nOK: signature=${sig}`);

  const srcAfter = await provider.connection.getBalance(srcPda);
  const destAfter = await (program.account as any).tournament.fetch(destPda);
  const destLamportsAfter = await provider.connection.getBalance(destPda);
  console.log(`src lamports after:      ${srcAfter} (rent floor only)`);
  console.log(`dest prize_lamports:     ${destAfter.prizeLamports.toString()}`);
  console.log(`dest lamports after:     ${destLamportsAfter}`);
  console.log(
    `swept:                   ${
      destLamportsAfter - destLamportsBefore
    } lamports`
  );
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
