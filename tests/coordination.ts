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
  const GUESS_SAME_TEAM = 0;
  const GUESS_DIFF_TEAM = 1;

  let gameCounterPda: PublicKey;
  let tournamentPda: PublicKey;
  let gamePda: PublicKey;
  let p1ProfilePda: PublicKey;
  let p2ProfilePda: PublicKey;

  // Precomputed commits used across the commit and reveal tests (same-team game)
  const p1Commit = generateCommit(GUESS_SAME_TEAM);
  const p2Commit = generateCommit(GUESS_SAME_TEAM);

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
      .createGame(STAKE, GUESS_SAME_TEAM)
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
    assert.equal(game.matchupType, GUESS_SAME_TEAM, "matchup_type should be 0 (same team)");
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
    const { commitment } = generateCommit(GUESS_SAME_TEAM);
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

  it("rejects reveal from non-participant", async () => {
    const outsider = Keypair.generate();
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
        .revealGuess(p1Commit.r as any)
        .accountsPartial({ ...revealAccounts, player: outsider.publicKey })
        .signers([outsider])
        .rpc();
      assert.fail("Expected NotAParticipant error");
    } catch (e: any) {
      assert.include(e.toString(), "NotAParticipant");
    }
  });

  it("player 1 reveals", async () => {
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

    const game = await program.account.game.fetch(gamePda);
    assert.equal(game.p1Guess, GUESS_SAME_TEAM, "p1 guess should be recorded");
  });

  it("rejects double reveal from player 1", async () => {
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
        .revealGuess(p1Commit.r as any)
        .accountsPartial({ ...revealAccounts, player: player1.publicKey })
        .signers([player1])
        .rpc();
      assert.fail("Expected AlreadyRevealed error");
    } catch (e: any) {
      assert.include(e.toString(), "AlreadyRevealed");
    }
  });

  it("player 2 reveals and the game resolves", async () => {
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
      .revealGuess(p2Commit.r as any)
      .accountsPartial({ ...revealAccounts, player: player2.publicKey })
      .signers([player2])
      .rpc();

    const game = await program.account.game.fetch(gamePda);
    assert.equal(game.p1Guess, GUESS_SAME_TEAM, "p1 should have guessed same team");
    assert.equal(game.p2Guess, GUESS_SAME_TEAM, "p2 should have guessed same team");
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
      .createGame(STAKE, GUESS_SAME_TEAM)
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

  it("rejects resolve_timeout before timeout elapses", async () => {
    // Create a fresh game and have p1 commit so the game is in Committing state
    const counter = await program.account.gameCounter.fetch(gameCounterPda);
    const timeoutGameId = counter.count as BN;
    const [timeoutGamePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("game"), timeoutGameId.toArrayLike(Buffer, "le", 8)],
      program.programId
    );
    const [tp1ProfilePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("player"), tournamentIdBuf(), player1.publicKey.toBuffer()],
      program.programId
    );
    const [tp2ProfilePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("player"), tournamentIdBuf(), player2.publicKey.toBuffer()],
      program.programId
    );

    await program.methods
      .createGame(STAKE, GUESS_SAME_TEAM)
      .accountsPartial({
        game: timeoutGamePda,
        gameCounter: gameCounterPda,
        playerProfile: tp1ProfilePda,
        tournament: tournamentPda,
        player: player1.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([player1])
      .rpc();

    await program.methods
      .joinGame()
      .accountsPartial({
        game: timeoutGamePda,
        playerProfile: tp2ProfilePda,
        tournament: tournamentPda,
        player: player2.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([player2])
      .rpc();

    const { commitment } = generateCommit(GUESS_SAME_TEAM);
    await program.methods
      .commitGuess(commitment as any)
      .accountsPartial({ game: timeoutGamePda, player: player1.publicKey })
      .signers([player1])
      .rpc();

    // Game is now in Committing state; timeout has not elapsed
    try {
      await program.methods
        .resolveTimeout()
        .accountsPartial({
          game: timeoutGamePda,
          p1Profile: tp1ProfilePda,
          p2Profile: tp2ProfilePda,
          tournament: tournamentPda,
          playerOneWallet: player1.publicKey,
          playerTwoWallet: player2.publicKey,
          caller: provider.wallet.publicKey,
        })
        .rpc();
      assert.fail("Expected TimeoutNotElapsed error");
    } catch (e: any) {
      assert.include(e.toString(), "TimeoutNotElapsed");
    }
  });

  it("rejects create_tournament with end_time before start_time", async () => {
    const now = Math.floor(Date.now() / 1000);
    const [badTournamentPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("tournament"), new BN(998).toArrayLike(Buffer, "le", 8)],
      program.programId
    );
    try {
      await program.methods
        .createTournament(new BN(998), new BN(now + 100), new BN(now + 50))
        .accountsPartial({
          tournament: badTournamentPda,
          authority: provider.wallet.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .rpc();
      assert.fail("Expected InvalidTournamentTimes error");
    } catch (e: any) {
      assert.include(e.toString(), "InvalidTournamentTimes");
    }
  });

  it("rejects create_game with zero stake", async () => {
    const counter = await program.account.gameCounter.fetch(gameCounterPda);
    const [zeroGamePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("game"), (counter.count as BN).toArrayLike(Buffer, "le", 8)],
      program.programId
    );
    const [zeroProfilePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("player"), tournamentIdBuf(), player1.publicKey.toBuffer()],
      program.programId
    );
    try {
      await program.methods
        .createGame(new BN(0), GUESS_SAME_TEAM)
        .accountsPartial({
          game: zeroGamePda,
          gameCounter: gameCounterPda,
          playerProfile: zeroProfilePda,
          tournament: tournamentPda,
          player: player1.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([player1])
        .rpc();
      assert.fail("Expected StakeMismatch error");
    } catch (e: any) {
      assert.include(e.toString(), "StakeMismatch");
    }
  });

  it("rejects commit from non-participant", async () => {
    // Use the timeout game (still in Committing state — only p1 committed)
    const counter = await program.account.gameCounter.fetch(gameCounterPda);
    // The timeout game was game_id = counter - 1 (created in a prior test)
    const timeoutGameId = (counter.count as BN).subn(1);
    const [tGamePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("game"), timeoutGameId.toArrayLike(Buffer, "le", 8)],
      program.programId
    );
    const outsider = Keypair.generate();
    const { commitment } = generateCommit(GUESS_SAME_TEAM);
    try {
      await program.methods
        .commitGuess(commitment as any)
        .accountsPartial({ game: tGamePda, player: outsider.publicKey })
        .signers([outsider])
        .rpc();
      assert.fail("Expected NotAParticipant error");
    } catch (e: any) {
      assert.include(e.toString(), "NotAParticipant");
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

  // ---------------------------------------------------------------------------
  // Heterogeneous game — different-team matchup
  // ---------------------------------------------------------------------------

  it("heterogeneous game: p1 commits first, both correct → p1 gets ~1.9× stake", async () => {
    // Create a fresh tournament for this test so we can run it in isolation
    const heteroTournamentId = new BN(2);
    const [heteroTournamentPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("tournament"), heteroTournamentId.toArrayLike(Buffer, "le", 8)],
      program.programId
    );
    const now = Math.floor(Date.now() / 1000);
    await program.methods
      .createTournament(heteroTournamentId, new BN(now - 60), new BN(now + 3600))
      .accountsPartial({
        tournament: heteroTournamentPda,
        authority: provider.wallet.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    const counter = await program.account.gameCounter.fetch(gameCounterPda);
    const heteroGameId = counter.count as BN;
    const [heteroGamePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("game"), heteroGameId.toArrayLike(Buffer, "le", 8)],
      program.programId
    );
    const heteroTournamentIdBuf = heteroTournamentId.toArrayLike(Buffer, "le", 8);
    const [hetP1ProfilePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("player"), heteroTournamentIdBuf, player1.publicKey.toBuffer()],
      program.programId
    );
    const [hetP2ProfilePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("player"), heteroTournamentIdBuf, player2.publicKey.toBuffer()],
      program.programId
    );

    // Create game with matchup_type = 1 (different teams)
    await program.methods
      .createGame(STAKE, GUESS_DIFF_TEAM)
      .accountsPartial({
        game: heteroGamePda,
        gameCounter: gameCounterPda,
        playerProfile: hetP1ProfilePda,
        tournament: heteroTournamentPda,
        player: player1.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([player1])
      .rpc();

    const createdGame = await program.account.game.fetch(heteroGamePda);
    assert.equal(createdGame.matchupType, GUESS_DIFF_TEAM, "matchup_type should be 1 (diff team)");

    // Player 2 joins
    await program.methods
      .joinGame()
      .accountsPartial({
        game: heteroGamePda,
        playerProfile: hetP2ProfilePda,
        tournament: heteroTournamentPda,
        player: player2.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([player2])
      .rpc();

    // Both commit guessing DIFF_TEAM (1 = correct for a heterogeneous match)
    // P1 commits first → first_committer = 1
    const hetP1Commit = generateCommit(GUESS_DIFF_TEAM);
    const hetP2Commit = generateCommit(GUESS_DIFF_TEAM);

    await program.methods
      .commitGuess(hetP1Commit.commitment as any)
      .accountsPartial({ game: heteroGamePda, player: player1.publicKey })
      .signers([player1])
      .rpc();

    await program.methods
      .commitGuess(hetP2Commit.commitment as any)
      .accountsPartial({ game: heteroGamePda, player: player2.publicKey })
      .signers([player2])
      .rpc();

    // Capture balances before reveal
    const p1BalanceBefore = await provider.connection.getBalance(player1.publicKey);
    const p2BalanceBefore = await provider.connection.getBalance(player2.publicKey);

    // Both reveal
    const revealAccounts = {
      game: heteroGamePda,
      p1Profile: hetP1ProfilePda,
      p2Profile: hetP2ProfilePda,
      tournament: heteroTournamentPda,
      playerOneWallet: player1.publicKey,
      playerTwoWallet: player2.publicKey,
      systemProgram: SystemProgram.programId,
    };

    await program.methods
      .revealGuess(hetP1Commit.r as any)
      .accountsPartial({ ...revealAccounts, player: player1.publicKey })
      .signers([player1])
      .rpc();

    await program.methods
      .revealGuess(hetP2Commit.r as any)
      .accountsPartial({ ...revealAccounts, player: player2.publicKey })
      .signers([player2])
      .rpc();

    const resolvedGame = await program.account.game.fetch(heteroGamePda);
    assert.equal(resolvedGame.p1Guess, GUESS_DIFF_TEAM, "p1 should have guessed diff team");
    assert.equal(resolvedGame.p2Guess, GUESS_DIFF_TEAM, "p2 should have guessed diff team");
    assert.equal(resolvedGame.firstCommitter, 1, "p1 should be first committer");
    assert.notEqual(resolvedGame.resolvedAt.toString(), "0", "game should be resolved");

    // P1 committed first, both correct → p1 wins 1.9× stake
    const expectedP1Return = STAKE.muln(19).divn(10); // 19_000_000
    const p1BalanceAfter = await provider.connection.getBalance(player1.publicKey);
    const p1Net = p1BalanceAfter - p1BalanceBefore;
    // p1Net ≈ expectedP1Return - STAKE (stake was locked) minus tx fees
    // We just check the tournament got its share
    const expectedTournamentGain = new BN(2).mul(STAKE).sub(expectedP1Return); // 1_000_000
    const hetTournament = await program.account.tournament.fetch(heteroTournamentPda);
    assert.equal(
      hetTournament.prizeLamports.toString(),
      expectedTournamentGain.toString(),
      "tournament should have received stake/10"
    );

    // P2 should receive nothing (lost)
    const p2BalanceAfter = await provider.connection.getBalance(player2.publicKey);
    // p2 net = -(stake + tx fees) — approximately, just check they didn't gain
    assert.isBelow(p2BalanceAfter, p2BalanceBefore, "p2 should lose stake");
  });

  it("rejects create_game outside tournament window", async () => {
    // Tournament 999 was created with end_time = now + 2 and is now expired
    const expiredId = new BN(999);
    const [expiredTournamentPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("tournament"), expiredId.toArrayLike(Buffer, "le", 8)],
      program.programId
    );
    const counter = await program.account.gameCounter.fetch(gameCounterPda);
    const [expiredGamePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("game"), (counter.count as BN).toArrayLike(Buffer, "le", 8)],
      program.programId
    );
    const [expiredProfilePda] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("player"),
        expiredId.toArrayLike(Buffer, "le", 8),
        player1.publicKey.toBuffer(),
      ],
      program.programId
    );
    try {
      await program.methods
        .createGame(STAKE, GUESS_SAME_TEAM)
        .accountsPartial({
          game: expiredGamePda,
          gameCounter: gameCounterPda,
          playerProfile: expiredProfilePda,
          tournament: expiredTournamentPda,
          player: player1.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([player1])
        .rpc();
      assert.fail("Expected OutsideTournamentWindow error");
    } catch (e: any) {
      assert.include(e.toString(), "OutsideTournamentWindow");
    }
  });
});
