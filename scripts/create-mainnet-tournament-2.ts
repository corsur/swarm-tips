/**
 * One-shot: create tournament_id=2 on mainnet coordination-game.
 *
 * Tournament 1 ended (page shows "Awaiting finalization", 79 games played),
 * so deposit_stake calls fail with OutsideTournamentWindow. This blocks the
 * gameplay E2E tests.
 *
 * create_tournament is permissionless — anyone can create one. Sets
 * end_time 90 days out so this doesn't recur soon.
 *
 * After running, also bump TOURNAMENT_ID = 2n in
 * coordination-app/frontend/coordination-game/src/lib/constants.ts and redeploy.
 *
 * Usage: ANCHOR_WALLET=~/.config/solana/id.json \
 *        ANCHOR_PROVIDER_URL=https://api.mainnet-beta.solana.com \
 *        npx ts-node scripts/create-mainnet-tournament-2.ts
 */

import * as anchor from "@coral-xyz/anchor";
import { PublicKey } from "@solana/web3.js";
import * as fs from "fs";
import * as path from "path";

const idl = JSON.parse(
  fs.readFileSync(
    path.join(__dirname, "../target/idl/coordination_game.json"),
    "utf8",
  ),
);

const TOURNAMENT_ID = 2n;
const NOW_SECS = Math.floor(Date.now() / 1000);
const START_TIME = NOW_SECS - 60; // start 1 min ago to ensure now >= start
const END_TIME = NOW_SECS + 90 * 24 * 60 * 60; // 90 days

async function main() {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = new anchor.Program(idl as anchor.Idl, provider);
  const wallet = provider.wallet.publicKey;

  console.log(`Wallet:  ${wallet.toBase58()}`);
  console.log(`RPC:     ${provider.connection.rpcEndpoint}`);
  console.log(`Program: ${program.programId.toBase58()}`);
  console.log(`Tournament ID: ${TOURNAMENT_ID}`);
  console.log(`Start:   ${new Date(START_TIME * 1000).toISOString()}`);
  console.log(`End:     ${new Date(END_TIME * 1000).toISOString()} (90d out)`);
  console.log();

  const idBuf = Buffer.alloc(8);
  idBuf.writeBigUInt64LE(TOURNAMENT_ID);
  const [tournamentPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("tournament"), idBuf],
    program.programId,
  );
  console.log(`Tournament PDA: ${tournamentPda.toBase58()}`);

  const existing = await provider.connection.getAccountInfo(tournamentPda);
  if (existing !== null) {
    console.log("Tournament already exists. Aborting.");
    return;
  }

  const sig = await (program.methods as any)
    .createTournament(
      new anchor.BN(TOURNAMENT_ID.toString()),
      new anchor.BN(START_TIME),
      new anchor.BN(END_TIME),
    )
    .accountsPartial({
      tournament: tournamentPda,
      authority: wallet,
    })
    .rpc();

  console.log(`OK: signature=${sig}`);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
