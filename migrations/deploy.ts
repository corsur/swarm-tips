// Post-deploy migration script. Anchor runs this automatically after
// `anchor deploy`. All operations here are idempotent — safe to run on
// upgrades as well as fresh deployments.
//
// Two one-time bootstrapping steps:
//   1. initialize  — creates the GameCounter singleton PDA.
//   2. create_tournament (ID 1) — creates a long-lived devnet tournament for
//      e2e tests and initial game play. End time is set 10 years out so it
//      never needs to be recreated.

import * as anchor from "@coral-xyz/anchor";
import { BN, Program } from "@coral-xyz/anchor";
import { PublicKey, SystemProgram } from "@solana/web3.js";
import { Coordination } from "../target/types/coordination";

const TOURNAMENT_ID = new BN(1);
// 10 years out so the tournament never needs to be recreated between deploys.
const TOURNAMENT_END = new BN(
  Math.floor(Date.now() / 1000) + 10 * 365 * 24 * 3600
);

module.exports = async function (provider: anchor.AnchorProvider) {
  anchor.setProvider(provider);
  const program = anchor.workspace.coordination as Program<Coordination>;

  // ------------------------------------------------------------------
  // Step 1: initialize (GameCounter PDA)
  // ------------------------------------------------------------------

  const [gameCounterPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("game_counter")],
    program.programId
  );

  const counterInfo = await provider.connection.getAccountInfo(gameCounterPda);
  if (counterInfo === null) {
    console.log("GameCounter not found — calling initialize");
    await program.methods
      .initialize()
      .accountsPartial({
        gameCounter: gameCounterPda,
        authority: provider.wallet.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();
    console.log("initialize done");
  } else {
    console.log("GameCounter already exists — skipping initialize");
  }

  // ------------------------------------------------------------------
  // Step 2: create_tournament (ID 1)
  // ------------------------------------------------------------------

  const [tournamentPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("tournament"), TOURNAMENT_ID.toArrayLike(Buffer, "le", 8)],
    program.programId
  );

  const tournamentInfo = await provider.connection.getAccountInfo(
    tournamentPda
  );
  if (tournamentInfo === null) {
    const now = Math.floor(Date.now() / 1000);
    console.log("Tournament 1 not found — calling create_tournament");
    await program.methods
      .createTournament(
        TOURNAMENT_ID,
        new BN(now - 60), // start slightly in the past so it's immediately active
        TOURNAMENT_END
      )
      .accountsPartial({
        tournament: tournamentPda,
        authority: provider.wallet.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();
    console.log(
      "create_tournament done — tournament 1 active until",
      new Date(TOURNAMENT_END.toNumber() * 1000).toISOString()
    );
  } else {
    console.log("Tournament 1 already exists — skipping create_tournament");
  }
};
