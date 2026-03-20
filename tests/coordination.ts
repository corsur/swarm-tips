import * as anchor from "@coral-xyz/anchor";
import { Program, BN } from "@coral-xyz/anchor";
import { Coordination } from "../target/types/coordination";
import { createHash, randomBytes } from "crypto";
import {
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  SystemProgram,
} from "@solana/web3.js";
import { assert } from "chai";

// ---------------------------------------------------------------------------
// Commit helpers
//
// Commit scheme: client generates a random 32-byte value R, encodes the guess
// in the last bit (R[31] & 1), and commits SHA-256(R). At reveal, the player
// sends R and the chain derives guess = R[31] & 1 and verifies the hash.
// ---------------------------------------------------------------------------

interface Commit {
  commitment: number[]; // SHA-256(R), 32 bytes
  r: number[]; // random preimage, 32 bytes
}

function generateCommit(guess: 0 | 1): Commit {
  const r = randomBytes(32);
  r[31] = (r[31] & 0xfe) | guess; // encode guess in the last bit
  const commitment = createHash("sha256").update(r).digest();
  return { commitment: Array.from(commitment), r: Array.from(r) };
}

// ---------------------------------------------------------------------------
// Test suite
// ---------------------------------------------------------------------------

describe("coordination", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const program = anchor.workspace.coordination as Program<Coordination>;

  const player1 = Keypair.generate();
  const player2 = Keypair.generate();

  const TOURNAMENT_ID = new BN(1);
  const STAKE = new BN(10_000_000); // 0.01 SOL
  const GUESS_HUMAN = 0;

  let gameCounterPda: PublicKey;
  let tournamentPda: PublicKey;
  let gamePda: PublicKey;
  let p1ProfilePda: PublicKey;
  let p2ProfilePda: PublicKey;

  // Precomputed commits used across the commit and reveal tests
  const p1Commit = generateCommit(GUESS_HUMAN);
  const p2Commit = generateCommit(GUESS_HUMAN);

  function tournamentIdBuf(): Buffer {
    return TOURNAMENT_ID.toArrayLike(Buffer, "le", 8);
  }

  // ---------------------------------------------------------------------------
  // Setup
  // ---------------------------------------------------------------------------

  before(async () => {
    for (const player of [player1, player2]) {
      const sig = await provider.connection.requestAirdrop(
        player.publicKey,
        2 * LAMPORTS_PER_SOL
      );
      await provider.connection.confirmTransaction(sig);
    }

    [gameCounterPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("game_counter")],
      program.programId
    );

    [tournamentPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("tournament"), tournamentIdBuf()],
      program.programId
    );
  });

  // ---------------------------------------------------------------------------
  // Initialize
  // ---------------------------------------------------------------------------

  it("initializes the program", async () => {
    await program.methods
      .initialize()
      .accountsPartial({
        gameCounter: gameCounterPda,
        authority: provider.wallet.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    const counter = await program.account.gameCounter.fetch(gameCounterPda);
    assert.equal(counter.count.toString(), "0");
  });

  // ---------------------------------------------------------------------------
  // Tournament
  // ---------------------------------------------------------------------------

  it("creates a tournament", async () => {
    const now = Math.floor(Date.now() / 1000);
    await program.methods
      .createTournament(TOURNAMENT_ID, new BN(now - 60), new BN(now + 3600))
      .accountsPartial({
        tournament: tournamentPda,
        authority: provider.wallet.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    const t = await program.account.tournament.fetch(tournamentPda);
    assert.equal(t.tournamentId.toString(), TOURNAMENT_ID.toString());
    assert.isFalse(t.finalized);
  });

  // ---------------------------------------------------------------------------
  // Game setup
  // ---------------------------------------------------------------------------

  it("player 1 creates a game", async () => {
    const counter = await program.account.gameCounter.fetch(gameCounterPda);
    const gameId = counter.count;

    [gamePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("game"), (gameId as BN).toArrayLike(Buffer, "le", 8)],
      program.programId
    );

    [p1ProfilePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("player"), tournamentIdBuf(), player1.publicKey.toBuffer()],
      program.programId
    );

    await program.methods
      .createGame(STAKE)
      .accountsPartial({
        game: gamePda,
        gameCounter: gameCounterPda,
        playerProfile: p1ProfilePda,
        tournament: tournamentPda,
        player: player1.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([player1])
      .rpc();

    const game = await program.account.game.fetch(gamePda);
    assert.equal(game.playerOne.toString(), player1.publicKey.toString());
    assert.equal(game.stakeLamports.toString(), STAKE.toString());
  });

  it("player 2 joins the game", async () => {
    [p2ProfilePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("player"), tournamentIdBuf(), player2.publicKey.toBuffer()],
      program.programId
    );

    await program.methods
      .joinGame()
      .accountsPartial({
        game: gamePda,
        playerProfile: p2ProfilePda,
        tournament: tournamentPda,
        player: player2.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([player2])
      .rpc();

    const game = await program.account.game.fetch(gamePda);
    assert.equal(game.playerTwo.toString(), player2.publicKey.toString());
  });

  // ---------------------------------------------------------------------------
  // Commit
  // ---------------------------------------------------------------------------

  it("player 1 commits", async () => {
    await program.methods
      .commitGuess(p1Commit.commitment as any)
      .accountsPartial({ game: gamePda, player: player1.publicKey })
      .signers([player1])
      .rpc();

    const game = await program.account.game.fetch(gamePda);
    assert.notDeepEqual(
      Array.from(game.p1Commit as any),
      Array(32).fill(0),
      "p1 commitment should be stored"
    );
  });

  it("rejects double commit from player 1", async () => {
    const { commitment } = generateCommit(GUESS_HUMAN);
    try {
      await program.methods
        .commitGuess(commitment as any)
        .accountsPartial({ game: gamePda, player: player1.publicKey })
        .signers([player1])
        .rpc();
      assert.fail("Expected AlreadyCommitted error");
    } catch (e: any) {
      assert.include(e.toString(), "AlreadyCommitted");
    }
  });

  it("player 2 commits", async () => {
    await program.methods
      .commitGuess(p2Commit.commitment as any)
      .accountsPartial({ game: gamePda, player: player2.publicKey })
      .signers([player2])
      .rpc();

    const game = await program.account.game.fetch(gamePda);
    assert.notDeepEqual(
      Array.from(game.p2Commit as any),
      Array(32).fill(0),
      "p2 commitment should be stored"
    );
  });

  // ---------------------------------------------------------------------------
  // Reveal + resolution
  // ---------------------------------------------------------------------------

  it("rejects reveal with wrong preimage", async () => {
    const wrongR = Array(32).fill(0xff); // SHA-256([0xff * 32]) != p1Commit.commitment
    const revealAccounts = {
      game: gamePda,
      p1Profile: p1ProfilePda,
      p2Profile: p2ProfilePda,
      tournament: tournamentPda,
      playerOneWallet: player1.publicKey,
      playerTwoWallet: player2.publicKey,
      systemProgram: SystemProgram.programId,
    };
    try {
      await program.methods
        .revealGuess(wrongR as any)
        .accountsPartial({ ...revealAccounts, player: player1.publicKey })
        .signers([player1])
        .rpc();
      assert.fail("Expected CommitmentMismatch error");
    } catch (e: any) {
      assert.include(e.toString(), "CommitmentMismatch");
    }
  });

  it("both players reveal and the game resolves", async () => {
    const revealAccounts = {
      game: gamePda,
      p1Profile: p1ProfilePda,
      p2Profile: p2ProfilePda,
      tournament: tournamentPda,
      playerOneWallet: player1.publicKey,
      playerTwoWallet: player2.publicKey,
      systemProgram: SystemProgram.programId,
    };

    await program.methods
      .revealGuess(p1Commit.r as any)
      .accountsPartial({ ...revealAccounts, player: player1.publicKey })
      .signers([player1])
      .rpc();

    await program.methods
      .revealGuess(p2Commit.r as any)
      .accountsPartial({ ...revealAccounts, player: player2.publicKey })
      .signers([player2])
      .rpc();

    const game = await program.account.game.fetch(gamePda);
    assert.equal(game.p1Guess, GUESS_HUMAN, "p1 should have guessed Human");
    assert.equal(game.p2Guess, GUESS_HUMAN, "p2 should have guessed Human");
    assert.notEqual(game.resolvedAt.toString(), "0", "game should be resolved");

    // Both correct in homogenous match → each gets 90% back, tournament gets 10% from each
    const tournament = await program.account.tournament.fetch(tournamentPda);
    const expectedPrize = STAKE.muln(2).divn(10);
    assert.equal(
      tournament.prizeLamports.toString(),
      expectedPrize.toString(),
      "tournament should have received 10% from each player"
    );
  });

  // ---------------------------------------------------------------------------
  // close_game
  // ---------------------------------------------------------------------------

  it("closes a resolved game", async () => {
    await program.methods
      .closeGame()
      .accountsPartial({
        game: gamePda,
        caller: provider.wallet.publicKey,
      })
      .rpc();

    try {
      await program.account.game.fetch(gamePda);
      assert.fail("Expected game account to be closed");
    } catch (e: any) {
      assert.include(e.toString(), "Account does not exist");
    }
  });

  // ---------------------------------------------------------------------------
  // Error path tests
  // ---------------------------------------------------------------------------

  it("rejects joining own game", async () => {
    const counter = await program.account.gameCounter.fetch(gameCounterPda);
    const soloGameId = counter.count as BN;
    const [soloGamePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("game"), soloGameId.toArrayLike(Buffer, "le", 8)],
      program.programId
    );
    const [soloProfilePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("player"), tournamentIdBuf(), player1.publicKey.toBuffer()],
      program.programId
    );

    await program.methods
      .createGame(STAKE)
      .accountsPartial({
        game: soloGamePda,
        gameCounter: gameCounterPda,
        playerProfile: soloProfilePda,
        tournament: tournamentPda,
        player: player1.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([player1])
      .rpc();

    try {
      await program.methods
        .joinGame()
        .accountsPartial({
          game: soloGamePda,
          playerProfile: soloProfilePda,
          tournament: tournamentPda,
          player: player1.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([player1])
        .rpc();
      assert.fail("Expected CannotJoinOwnGame error");
    } catch (e: any) {
      assert.include(e.toString(), "CannotJoinOwnGame");
    }
  });

  it("rejects finalize_tournament before end time", async () => {
    try {
      await program.methods
        .finalizeTournament()
        .accountsPartial({
          tournament: tournamentPda,
          caller: provider.wallet.publicKey,
        })
        .remainingAccounts([])
        .rpc();
      assert.fail("Expected TournamentNotEnded error");
    } catch (e: any) {
      assert.include(e.toString(), "TournamentNotEnded");
    }
  });

  it("rejects claim_reward on unfinalized tournament", async () => {
    try {
      await program.methods
        .claimReward()
        .accountsPartial({
          tournament: tournamentPda,
          playerProfile: p1ProfilePda,
          player: player1.publicKey,
        })
        .signers([player1])
        .rpc();
      assert.fail("Expected TournamentNotFinalized error");
    } catch (e: any) {
      assert.include(e.toString(), "TournamentNotFinalized");
    }
  });

  // ---------------------------------------------------------------------------
  // finalize_tournament (short-lived tournament)
  // ---------------------------------------------------------------------------

  it("finalizes an ended tournament", async () => {
    const now = Math.floor(Date.now() / 1000);
    const shortId = new BN(999);
    const [shortTournamentPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("tournament"), shortId.toArrayLike(Buffer, "le", 8)],
      program.programId
    );

    await program.methods
      .createTournament(shortId, new BN(now - 60), new BN(now + 2))
      .accountsPartial({
        tournament: shortTournamentPda,
        authority: provider.wallet.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    // Wait for tournament window to close — local validator clock can lag wall time
    await new Promise((r) => setTimeout(r, 10000));

    await program.methods
      .finalizeTournament()
      .accountsPartial({
        tournament: shortTournamentPda,
        caller: provider.wallet.publicKey,
      })
      .remainingAccounts([])
      .rpc();

    const t = await program.account.tournament.fetch(shortTournamentPda);
    assert.isTrue(t.finalized, "tournament should be finalized");
    assert.equal(t.prizeSnapshot.toString(), "0", "prize should be zero");
    assert.equal(
      t.totalScoreSnapshot.toString(),
      "0",
      "score snapshot should be zero"
    );
  });
});
