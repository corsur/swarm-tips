/**
 * Create a new mainnet tournament. Permissionless — anyone with gas can
 * call. Defaults to a 90-day window starting 1 minute ago (the negative
 * `start_time` offset ensures `now >= start` for the first deposit_stake).
 *
 * Used at tournament rollover (current ends → create the next one with
 * `TOURNAMENT_ID` set to the next free integer). Bumping
 * coordination-app/frontend/coordination-game/src/lib/constants.ts to
 * point at the new id is a separate manual step until that mapping moves
 * to a dynamic config.
 *
 * Usage: ANCHOR_WALLET=~/.config/solana/id.json \
 *        ANCHOR_PROVIDER_URL=https://api.mainnet-beta.solana.com \
 *        TOURNAMENT_ID=3 \
 *        END_TIME_DAYS=90 \
 *          npx ts-node scripts/create-mainnet-tournament.ts
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

const TOURNAMENT_ID = BigInt(process.env.TOURNAMENT_ID ?? "0");
if (TOURNAMENT_ID === 0n) {
  console.error("TOURNAMENT_ID env var is required (e.g. 3, 4, 5).");
  process.exit(1);
}

const END_TIME_DAYS = parseInt(process.env.END_TIME_DAYS ?? "90", 10);
const NOW_SECS = Math.floor(Date.now() / 1000);
const START_TIME = NOW_SECS - 60;
const END_TIME = NOW_SECS + END_TIME_DAYS * 24 * 60 * 60;

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
  console.log(
    `End:     ${new Date(
      END_TIME * 1000
    ).toISOString()} (${END_TIME_DAYS}d out)`
  );

  const idBuf = Buffer.alloc(8);
  idBuf.writeBigUInt64LE(TOURNAMENT_ID);
  const [tournamentPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("tournament"), idBuf],
    program.programId
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
      new anchor.BN(END_TIME)
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
