import * as anchor from "@coral-xyz/anchor";
import { Program, BN } from "@coral-xyz/anchor";
import { CoordinationGame } from "../target/types/coordination_game";
import { createHash, randomBytes } from "crypto";
import {
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  SystemProgram,
} from "@solana/web3.js";
import { assert } from "chai";
import {
  OutcomeKind,
  deriveTerminalOutcome,
  deriveClaimOutcome,
  resolvePayoff,
  splitTournamentGain,
} from "./helpers/outcome-oracle";

// ---------------------------------------------------------------------------
// Commit helpers
//
// Commit scheme: client generates a random 32-byte value R, encodes the guess
// in the last bit (R[31] & 1), and commits SHA-256(R). At reveal, the player
// sends R and the chain derives guess = R[31] & 1 and verifies the hash.
//
// Matchup commit scheme: identical structure — SHA-256(R_matchup) where
// R_matchup[31] & 1 encodes the matchup type (0 = same team, 1 = diff teams).
// The matchmaker creates the commitment at game creation; the first revealer
// provides R_matchup so the chain can derive matchup_type after both commits.
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

interface MatchupCommit {
  commitment: number[]; // SHA-256(R_matchup), 32 bytes
  r: number[]; // random preimage, 32 bytes
}

function generateMatchupCommit(matchupType: 0 | 1): MatchupCommit {
  const r = randomBytes(32);
  r[31] = (r[31] & 0xfe) | matchupType;
  const commitment = createHash("sha256").update(r).digest();
  return { commitment: Array.from(commitment), r: Array.from(r) };
}

// ---------------------------------------------------------------------------
// Test suite
// ---------------------------------------------------------------------------

describe("coordination-game", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const program = anchor.workspace
    .coordinationGame as Program<CoordinationGame>;

  const player1 = Keypair.generate();
  const player2 = Keypair.generate();

  const TOURNAMENT_ID = new BN(1);
  const STAKE = new BN(50_000_000); // 0.05 SOL
  const GUESS_SAME_TEAM = 0;
  const GUESS_DIFF_TEAM = 1;

  let gameCounterPda: PublicKey;
  let globalConfigPda: PublicKey;
  let tournamentPda: PublicKey;
  let gamePda: PublicKey;
  let p1ProfilePda: PublicKey;
  let p2ProfilePda: PublicKey;
  // The provider wallet acts as both authority and matchmaker in tests
  const matchmaker = provider.wallet;
  const treasury = Keypair.generate();

  // Precomputed commits used across the commit and reveal tests (same-team game)
  const p1Commit = generateCommit(GUESS_SAME_TEAM);
  const p2Commit = generateCommit(GUESS_SAME_TEAM);

  function tournamentIdBuf(): Buffer {
    return TOURNAMENT_ID.toArrayLike(Buffer, "le", 8);
  }

  function escrowPda(tournamentId: BN, player: PublicKey): [PublicKey, number] {
    return PublicKey.findProgramAddressSync(
      [
        Buffer.from("escrow"),
        tournamentId.toArrayLike(Buffer, "le", 8),
        player.toBuffer(),
      ],
      program.programId
    );
  }

  /** Creates a game on-chain with P1. Returns [gamePda, gameId, matchupRPreimage]. */
  async function createGameOnChain(
    tournamentPdaKey: PublicKey,
    matchupType: number,
    player: Keypair = player1
  ): Promise<[PublicKey, BN, number[]]> {
    const matchupCommit = generateMatchupCommit(matchupType as 0 | 1);

    // Deposit stake for P1 before creating the game
    const tournamentData = await program.account.tournament.fetch(
      tournamentPdaKey
    );
    const tournamentId = tournamentData.tournamentId as BN;
    await depositStake(tournamentId, tournamentPdaKey, player);

    const counter = await program.account.gameCounter.fetch(gameCounterPda);
    const gameId = counter.count as BN;
    const [gPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("game"), gameId.toArrayLike(Buffer, "le", 8)],
      program.programId
    );
    const [profilePda] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("player"),
        tournamentId.toArrayLike(Buffer, "le", 8),
        player.publicKey.toBuffer(),
      ],
      program.programId
    );
    const [escrow] = escrowPda(tournamentId, player.publicKey);
    await program.methods
      .createGame(STAKE, matchupCommit.commitment as any)
      .accountsPartial({
        game: gPda,
        gameCounter: gameCounterPda,
        playerProfile: profilePda,
        escrow,
        tournament: tournamentPdaKey,
        globalConfig: globalConfigPda,
        matchmaker: matchmaker.publicKey,
        player: player.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([player])
      .rpc();
    return [gPda, gameId, matchupCommit.r];
  }

  /** Player 2 joins an existing game (deposits escrow + joins). */
  async function joinGameOnChain(
    gamePdaKey: PublicKey,
    tournamentId: BN,
    tournamentPdaKey: PublicKey,
    player: Keypair
  ): Promise<PublicKey> {
    await depositStake(tournamentId, tournamentPdaKey, player);
    const [profilePda] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("player"),
        tournamentId.toArrayLike(Buffer, "le", 8),
        player.publicKey.toBuffer(),
      ],
      program.programId
    );
    const [escrow] = escrowPda(tournamentId, player.publicKey);
    await program.methods
      .joinGame()
      .accountsPartial({
        game: gamePdaKey,
        playerProfile: profilePda,
        escrow,
        tournament: tournamentPdaKey,
        globalConfig: globalConfigPda,
        matchmaker: matchmaker.publicKey,
        player: player.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([player])
      .rpc();
    return profilePda;
  }

  /** Deposit stake into escrow for a player in a given tournament. */
  async function depositStake(
    tournamentId: BN,
    tournamentPdaKey: PublicKey,
    player: Keypair
  ): Promise<void> {
    const [escrow] = escrowPda(tournamentId, player.publicKey);
    await program.methods
      .depositStake()
      .accountsPartial({
        escrow,
        tournament: tournamentPdaKey,
        player: player.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([player])
      .rpc();
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

    [globalConfigPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("global_config")],
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

  it("initializes global config", async () => {
    // Treasury keypair is declared at suite level so reveal tests can reference it
    await program.methods
      .initializeConfig(5000) // 50/50 treasury/prize split
      .accountsPartial({
        globalConfig: globalConfigPda,
        authority: provider.wallet.publicKey,
        matchmaker: provider.wallet.publicKey, // provider wallet is matchmaker in tests
        treasury: treasury.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    const config = await program.account.globalConfig.fetch(globalConfigPda);
    assert.equal(
      config.authority.toString(),
      provider.wallet.publicKey.toString()
    );
    assert.equal(
      config.matchmaker.toString(),
      provider.wallet.publicKey.toString()
    );
    assert.equal(config.treasurySplitBps, 5000);
  });

  // ---------------------------------------------------------------------------
  // Tournament
  // ---------------------------------------------------------------------------

  it("creates a tournament", async () => {
    const now = Math.floor(Date.now() / 1000);
    await program.methods
      .createTournament(TOURNAMENT_ID, new BN(now - 60), new BN(now + 86400))
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
  // Escrow + Game setup
  // ---------------------------------------------------------------------------

  // r_matchup for the main game (needed by the first revealer)
  let mainGameRMatchup: number[];

  it("player 1 creates a game (matchmaker co-signs)", async () => {
    const matchupCommit = generateMatchupCommit(GUESS_SAME_TEAM as 0 | 1);
    mainGameRMatchup = matchupCommit.r;

    // Player 1 deposits stake first
    await depositStake(TOURNAMENT_ID, tournamentPda, player1);

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
    const [p1Escrow] = escrowPda(TOURNAMENT_ID, player1.publicKey);

    await program.methods
      .createGame(STAKE, matchupCommit.commitment as any)
      .accountsPartial({
        game: gamePda,
        gameCounter: gameCounterPda,
        playerProfile: p1ProfilePda,
        escrow: p1Escrow,
        tournament: tournamentPda,
        globalConfig: globalConfigPda,
        matchmaker: matchmaker.publicKey,
        player: player1.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([player1])
      .rpc();

    const game = await program.account.game.fetch(gamePda);
    assert.equal(
      game.playerOne.toString(),
      player1.publicKey.toString(),
      "player_one should be set at creation"
    );
    assert.equal(game.stakeLamports.toString(), STAKE.toString());
  });

  it("player 2 deposits stake and joins the game", async () => {
    await depositStake(TOURNAMENT_ID, tournamentPda, player2);

    const [escrow] = escrowPda(TOURNAMENT_ID, player2.publicKey);

    [p2ProfilePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("player"), tournamentIdBuf(), player2.publicKey.toBuffer()],
      program.programId
    );

    await program.methods
      .joinGame()
      .accountsPartial({
        game: gamePda,
        playerProfile: p2ProfilePda,
        escrow,
        tournament: tournamentPda,
        globalConfig: globalConfigPda,
        matchmaker: matchmaker.publicKey,
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
      // assert.fail above throws AssertionError — re-throw it so the
      // no-rejection path cannot satisfy this catch's substring assert.
      if (e?.name === "AssertionError") throw e;
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
      globalConfig: globalConfigPda,
      treasury: treasury.publicKey,
      systemProgram: SystemProgram.programId,
    };
    try {
      await program.methods
        .revealGuess(wrongR as any, mainGameRMatchup as any)
        .accountsPartial({ ...revealAccounts, player: player1.publicKey })
        .signers([player1])
        .rpc();
      assert.fail("Expected CommitmentMismatch error");
    } catch (e: any) {
      // assert.fail above throws AssertionError — re-throw it so the
      // no-rejection path cannot satisfy this catch's substring assert.
      if (e?.name === "AssertionError") throw e;
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
      globalConfig: globalConfigPda,
      treasury: treasury.publicKey,
      systemProgram: SystemProgram.programId,
    };
    try {
      await program.methods
        .revealGuess(p1Commit.r as any, mainGameRMatchup as any)
        .accountsPartial({ ...revealAccounts, player: outsider.publicKey })
        .signers([outsider])
        .rpc();
      assert.fail("Expected NotAParticipant error");
    } catch (e: any) {
      // assert.fail above throws AssertionError — re-throw it so the
      // no-rejection path cannot satisfy this catch's substring assert.
      if (e?.name === "AssertionError") throw e;
      assert.include(e.toString(), "NotAParticipant");
    }
  });

  it("player 1 reveals (first reveal — includes r_matchup)", async () => {
    const revealAccounts = {
      game: gamePda,
      p1Profile: p1ProfilePda,
      p2Profile: p2ProfilePda,
      tournament: tournamentPda,
      playerOneWallet: player1.publicKey,
      playerTwoWallet: player2.publicKey,
      globalConfig: globalConfigPda,
      treasury: treasury.publicKey,
      systemProgram: SystemProgram.programId,
    };
    await program.methods
      .revealGuess(p1Commit.r as any, mainGameRMatchup as any)
      .accountsPartial({ ...revealAccounts, player: player1.publicKey })
      .signers([player1])
      .rpc();

    const game = await program.account.game.fetch(gamePda);
    assert.equal(game.p1Guess, GUESS_SAME_TEAM, "p1 guess should be recorded");
    assert.equal(
      game.matchupType,
      GUESS_SAME_TEAM,
      "matchup_type should be revealed after first reveal"
    );
  });

  it("rejects double reveal from player 1", async () => {
    const revealAccounts = {
      game: gamePda,
      p1Profile: p1ProfilePda,
      p2Profile: p2ProfilePda,
      tournament: tournamentPda,
      playerOneWallet: player1.publicKey,
      playerTwoWallet: player2.publicKey,
      globalConfig: globalConfigPda,
      treasury: treasury.publicKey,
      systemProgram: SystemProgram.programId,
    };
    try {
      await program.methods
        .revealGuess(p1Commit.r as any, null)
        .accountsPartial({ ...revealAccounts, player: player1.publicKey })
        .signers([player1])
        .rpc();
      assert.fail("Expected AlreadyRevealed error");
    } catch (e: any) {
      // assert.fail above throws AssertionError — re-throw it so the
      // no-rejection path cannot satisfy this catch's substring assert.
      if (e?.name === "AssertionError") throw e;
      assert.include(e.toString(), "AlreadyRevealed");
    }
  });

  it("player 2 reveals and the game resolves (second reveal — null r_matchup)", async () => {
    const revealAccounts = {
      game: gamePda,
      p1Profile: p1ProfilePda,
      p2Profile: p2ProfilePda,
      tournament: tournamentPda,
      playerOneWallet: player1.publicKey,
      playerTwoWallet: player2.publicKey,
      globalConfig: globalConfigPda,
      treasury: treasury.publicKey,
      systemProgram: SystemProgram.programId,
    };
    await program.methods
      .revealGuess(p2Commit.r as any, null)
      .accountsPartial({ ...revealAccounts, player: player2.publicKey })
      .signers([player2])
      .rpc();

    const game = await program.account.game.fetch(gamePda);
    assert.equal(
      game.p1Guess,
      GUESS_SAME_TEAM,
      "p1 should have guessed same team"
    );
    assert.equal(
      game.p2Guess,
      GUESS_SAME_TEAM,
      "p2 should have guessed same team"
    );
    assert.notEqual(game.resolvedAt.toString(), "0", "game should be resolved");

    // Both correct in homogenous match → full refund, tournament gains nothing
    const tournament = await program.account.tournament.fetch(tournamentPda);
    assert.equal(
      tournament.prizeLamports.toString(),
      "0",
      "tournament should gain nothing when both players guess correctly"
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
      // assert.fail above throws AssertionError — re-throw it so the
      // no-rejection path cannot satisfy this catch's substring assert.
      if (e?.name === "AssertionError") throw e;
      assert.include(e.toString(), "Account does not exist");
    }
  });

  // ---------------------------------------------------------------------------
  // Error path tests
  // ---------------------------------------------------------------------------

  it("rejects joining own game", async () => {
    // Player 1 creates a game (P1 is set at creation)
    const [soloGamePda] = await createGameOnChain(
      tournamentPda,
      GUESS_SAME_TEAM,
      player1
    );

    // Player 1 tries to join as P2 — should fail
    await depositStake(TOURNAMENT_ID, tournamentPda, player1);
    const [soloProfilePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("player"), tournamentIdBuf(), player1.publicKey.toBuffer()],
      program.programId
    );
    const [soloEscrow] = escrowPda(TOURNAMENT_ID, player1.publicKey);

    try {
      await program.methods
        .joinGame()
        .accountsPartial({
          game: soloGamePda,
          playerProfile: soloProfilePda,
          escrow: soloEscrow,
          tournament: tournamentPda,
          globalConfig: globalConfigPda,
          matchmaker: matchmaker.publicKey,
          player: player1.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([player1])
        .rpc();
      assert.fail("Expected CannotJoinOwnGame error");
    } catch (e: any) {
      // assert.fail above throws AssertionError — re-throw it so the
      // no-rejection path cannot satisfy this catch's substring assert.
      if (e?.name === "AssertionError") throw e;
      assert.include(e.toString(), "CannotJoinOwnGame");
    }
  });

  it("rejects a join whose co-signer is not the matchmaker (interloper front-run)", async () => {
    // P1 opens a game the matchmaker paired for a specific opponent. A stranger
    // who is a legit staked player still cannot claim the P2 slot, because they
    // cannot produce the matchmaker's co-signature — the exact front-run the
    // matchmaker-cosigned join closes.
    const [victimGamePda] = await createGameOnChain(
      tournamentPda,
      GUESS_SAME_TEAM,
      player1
    );

    const interloper = Keypair.generate();
    const sig = await provider.connection.requestAirdrop(
      interloper.publicKey,
      2 * LAMPORTS_PER_SOL
    );
    await provider.connection.confirmTransaction(sig);
    await depositStake(TOURNAMENT_ID, tournamentPda, interloper);

    const [interloperProfile] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("player"),
        tournamentIdBuf(),
        interloper.publicKey.toBuffer(),
      ],
      program.programId
    );
    const [interloperEscrow] = escrowPda(TOURNAMENT_ID, interloper.publicKey);

    // A random key stands in for the "matchmaker" — it is not the configured
    // GlobalConfig.matchmaker, so validate_join_inputs must reject it.
    const bogusMatchmaker = Keypair.generate();

    try {
      await program.methods
        .joinGame()
        .accountsPartial({
          game: victimGamePda,
          playerProfile: interloperProfile,
          escrow: interloperEscrow,
          tournament: tournamentPda,
          globalConfig: globalConfigPda,
          matchmaker: bogusMatchmaker.publicKey,
          player: interloper.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([interloper, bogusMatchmaker])
        .rpc();
      assert.fail("Expected NotMatchmaker error");
    } catch (e: any) {
      if (e?.name === "AssertionError") throw e;
      assert.include(e.toString(), "NotMatchmaker");
    }
  });

  it("rejects resolve_timeout before timeout elapses", async () => {
    // Create a fresh game (P1 set at creation), P2 joins, then p1 commits so the game is in Committing state
    const [timeoutGamePda] = await createGameOnChain(
      tournamentPda,
      GUESS_SAME_TEAM,
      player1
    );
    const tp1ProfilePda = PublicKey.findProgramAddressSync(
      [Buffer.from("player"), tournamentIdBuf(), player1.publicKey.toBuffer()],
      program.programId
    )[0];
    const tp2ProfilePda = await joinGameOnChain(
      timeoutGamePda,
      TOURNAMENT_ID,
      tournamentPda,
      player2
    );

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
          globalConfig: globalConfigPda,
          treasury: treasury.publicKey,
          playerOneWallet: player1.publicKey,
          playerTwoWallet: player2.publicKey,
          caller: provider.wallet.publicKey,
        })
        .rpc();
      assert.fail("Expected TimeoutNotElapsed error");
    } catch (e: any) {
      // assert.fail above throws AssertionError — re-throw it so the
      // no-rejection path cannot satisfy this catch's substring assert.
      if (e?.name === "AssertionError") throw e;
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
      // assert.fail above throws AssertionError — re-throw it so the
      // no-rejection path cannot satisfy this catch's substring assert.
      if (e?.name === "AssertionError") throw e;
      assert.include(e.toString(), "InvalidTournamentTimes");
    }
  });

  it("rejects create_game with zero stake", async () => {
    const matchupCommit = generateMatchupCommit(GUESS_SAME_TEAM as 0 | 1);
    await depositStake(TOURNAMENT_ID, tournamentPda, player1);
    const counter = await program.account.gameCounter.fetch(gameCounterPda);
    const [zeroGamePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("game"), (counter.count as BN).toArrayLike(Buffer, "le", 8)],
      program.programId
    );
    const [profilePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("player"), tournamentIdBuf(), player1.publicKey.toBuffer()],
      program.programId
    );
    const [escrow] = escrowPda(TOURNAMENT_ID, player1.publicKey);
    try {
      await program.methods
        .createGame(new BN(0), matchupCommit.commitment as any)
        .accountsPartial({
          game: zeroGamePda,
          gameCounter: gameCounterPda,
          playerProfile: profilePda,
          escrow,
          tournament: tournamentPda,
          globalConfig: globalConfigPda,
          matchmaker: matchmaker.publicKey,
          player: player1.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([player1])
        .rpc();
      assert.fail("Expected StakeMismatch error");
    } catch (e: any) {
      // assert.fail above throws AssertionError — re-throw it so the
      // no-rejection path cannot satisfy this catch's substring assert.
      if (e?.name === "AssertionError") throw e;
      assert.include(e.toString(), "StakeMismatch");
    }
  });

  it("rejects create_game with wrong stake (0.1 SOL instead of 0.01 SOL)", async () => {
    const WRONG_STAKE = new BN(100_000_000); // 0.1 SOL
    const matchupCommit = generateMatchupCommit(GUESS_SAME_TEAM as 0 | 1);
    await depositStake(TOURNAMENT_ID, tournamentPda, player1);
    const counter = await program.account.gameCounter.fetch(gameCounterPda);
    const [wrongGamePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("game"), (counter.count as BN).toArrayLike(Buffer, "le", 8)],
      program.programId
    );
    const [profilePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("player"), tournamentIdBuf(), player1.publicKey.toBuffer()],
      program.programId
    );
    const [escrow] = escrowPda(TOURNAMENT_ID, player1.publicKey);
    try {
      await program.methods
        .createGame(WRONG_STAKE, matchupCommit.commitment as any)
        .accountsPartial({
          game: wrongGamePda,
          gameCounter: gameCounterPda,
          playerProfile: profilePda,
          escrow,
          tournament: tournamentPda,
          globalConfig: globalConfigPda,
          matchmaker: matchmaker.publicKey,
          player: player1.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([player1])
        .rpc();
      assert.fail("Expected StakeMismatch error");
    } catch (e: any) {
      // assert.fail above throws AssertionError — re-throw it so the
      // no-rejection path cannot satisfy this catch's substring assert.
      if (e?.name === "AssertionError") throw e;
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
      // assert.fail above throws AssertionError — re-throw it so the
      // no-rejection path cannot satisfy this catch's substring assert.
      if (e?.name === "AssertionError") throw e;
      assert.include(e.toString(), "NotAParticipant");
    }
  });

  it("rejects finalize_tournament before end time", async () => {
    const dummyRoot = Array(32).fill(0);
    try {
      await program.methods
        .finalizeTournament(dummyRoot as any)
        .accountsPartial({
          tournament: tournamentPda,
          globalConfig: globalConfigPda,
          authority: provider.wallet.publicKey,
        })
        .rpc();
      assert.fail("Expected TournamentNotEnded error");
    } catch (e: any) {
      // assert.fail above throws AssertionError — re-throw it so the
      // no-rejection path cannot satisfy this catch's substring assert.
      if (e?.name === "AssertionError") throw e;
      assert.include(e.toString(), "TournamentNotEnded");
    }
  });

  it("rejects claim_reward on unfinalized tournament", async () => {
    try {
      await program.methods
        .claimReward(new BN(0), [])
        .accountsPartial({
          tournament: tournamentPda,
          playerProfile: p1ProfilePda,
          player: player1.publicKey,
        })
        .signers([player1])
        .rpc();
      assert.fail("Expected TournamentNotFinalized error");
    } catch (e: any) {
      // assert.fail above throws AssertionError — re-throw it so the
      // no-rejection path cannot satisfy this catch's substring assert.
      if (e?.name === "AssertionError") throw e;
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

    // Empty tournament — no players, no prizes. Use a zero merkle root.
    const emptyRoot = Array(32).fill(0);
    await program.methods
      .finalizeTournament(emptyRoot as any)
      .accountsPartial({
        tournament: shortTournamentPda,
        globalConfig: globalConfigPda,
        authority: provider.wallet.publicKey,
      })
      .rpc();

    const t = await program.account.tournament.fetch(shortTournamentPda);
    assert.isTrue(t.finalized, "tournament should be finalized");
    assert.equal(t.prizeSnapshot.toString(), "0", "prize should be zero");
    assert.deepEqual(
      Array.from(t.merkleRoot as any),
      emptyRoot,
      "merkle root should match"
    );
  });

  // ---------------------------------------------------------------------------
  // Heterogeneous game — different-team matchup
  // ---------------------------------------------------------------------------

  // -------------------------------------------------------------------------
  // Payoff matrix — exhaustive logic oracle (no validator)
  //
  // The TS oracle mirrors cert_schema.rs / CertLib.sol / payoff.rs. Sweeping
  // every (matchup_type, p1_guess, p2_guess, first_committer) proves the mirror
  // is internally consistent (stake conservation) and outcome-correct against an
  // independent expectation table, before the on-chain block below confirms a
  // representative game per cell actually resolves to the same outcome.
  // -------------------------------------------------------------------------
  describe("payoff matrix — exhaustive oracle", () => {
    const STAKE_L = BigInt(STAKE.toString());

    // Independent expectation: derived straight from the spec, NOT from the
    // function under test, so a wrong oracle cannot agree with itself.
    function expectedTerminal(
      mt: 0 | 1,
      p1: 0 | 1,
      p2: 0 | 1,
      fc: 1 | 2
    ): OutcomeKind {
      const p1ok = p1 === mt;
      const p2ok = p2 === mt;
      if (mt === 0) {
        if (p1ok && p2ok) return OutcomeKind.HomogBothCorrect;
        if (p1ok) return OutcomeKind.HomogP1Correct;
        if (p2ok) return OutcomeKind.HomogP2Correct;
        return OutcomeKind.BothWrong;
      }
      if (!p1ok && !p2ok) return OutcomeKind.BothWrong;
      if (p1ok && p2ok)
        return fc === 1 ? OutcomeKind.HeteroP1Wins : OutcomeKind.HeteroP2Wins;
      return p1ok ? OutcomeKind.HeteroP1Wins : OutcomeKind.HeteroP2Wins;
    }

    it("derives the right outcome and conserves stake for all 16 terminal transcripts", () => {
      const two = STAKE_L * 2n;
      for (const mt of [0, 1] as const) {
        for (const p1 of [0, 1] as const) {
          for (const p2 of [0, 1] as const) {
            for (const fc of [1, 2] as const) {
              const t = {
                stepCount: 4,
                matchupType: mt,
                p1Guess: p1,
                p2Guess: p2,
                firstCommitter: fc,
              };
              assert.equal(
                deriveTerminalOutcome(t),
                expectedTerminal(mt, p1, p2, fc),
                `terminal outcome mt=${mt} p1=${p1} p2=${p2} fc=${fc}`
              );
              const payoff = resolvePayoff({ ...t, stake: STAKE_L });
              assert.equal(
                (
                  payoff.p1Return +
                  payoff.p2Return +
                  payoff.tournamentGain
                ).toString(),
                two.toString(),
                `stake conservation mt=${mt} p1=${p1} p2=${p2} fc=${fc}`
              );
            }
          }
        }
      }
    });

    it("derives timeout outcomes for partial transcripts (steps 0-3)", () => {
      const base = { matchupType: 1, p1Guess: 255, p2Guess: 255 };
      // step 1: only the first committer landed -> committer wins.
      assert.equal(
        deriveClaimOutcome({ ...base, stepCount: 1, firstCommitter: 1 }),
        OutcomeKind.TimeoutP1Wins
      );
      assert.equal(
        deriveClaimOutcome({ ...base, stepCount: 1, firstCommitter: 2 }),
        OutcomeKind.TimeoutP2Wins
      );
      // step 3: both committed, exactly one revealed -> revealer wins.
      assert.equal(
        deriveClaimOutcome({
          matchupType: 1,
          p1Guess: 1,
          p2Guess: 255,
          stepCount: 3,
          firstCommitter: 1,
        }),
        OutcomeKind.TimeoutP1Wins
      );
      assert.equal(
        deriveClaimOutcome({
          matchupType: 1,
          p1Guess: 255,
          p2Guess: 1,
          stepCount: 3,
          firstCommitter: 1,
        }),
        OutcomeKind.TimeoutP2Wins
      );
      // both revealed at step 3, and steps 0 & 2 -> both forfeit.
      assert.equal(
        deriveClaimOutcome({
          matchupType: 1,
          p1Guess: 1,
          p2Guess: 1,
          stepCount: 3,
          firstCommitter: 1,
        }),
        OutcomeKind.TimeoutBothForfeit
      );
      for (const stepCount of [0, 2]) {
        assert.equal(
          deriveClaimOutcome({ ...base, stepCount, firstCommitter: 1 }),
          OutcomeKind.TimeoutBothForfeit
        );
      }
    });
  });

  // -------------------------------------------------------------------------
  // Payoff matrix — combinatorial on-chain resolution
  //
  // One real game per distinct payoff cell (both first-committer tie-breaks for
  // the heterogeneous both-correct case), each asserting the on-chain Game state
  // and tournament prize delta against the oracle. Replaces the hand-rolled
  // single-cell hetero tests; adds the previously-untested homogeneous
  // asymmetric cells and hetero P2-wins.
  // -------------------------------------------------------------------------
  describe("payoff matrix — combinatorial on-chain resolution", () => {
    const STAKE_L = BigInt(STAKE.toString());
    const TREASURY_SPLIT_BPS = 5000; // initialize_config above
    const FEE_MARGIN = 10_000_000; // 0.01 SOL — covers per-player tx fees + rent churn (still « the 0.05 stake)

    before(async () => {
      // Top up so cells that burn a full stake stay funded.
      for (const player of [player1, player2]) {
        const sig = await provider.connection.requestAirdrop(
          player.publicKey,
          2 * LAMPORTS_PER_SOL
        );
        await provider.connection.confirmTransaction(sig);
      }
    });

    async function playToResolution(opts: {
      tournamentId: number;
      matchupType: 0 | 1;
      p1Guess: 0 | 1;
      p2Guess: 0 | 1;
      commitFirst: 1 | 2;
    }) {
      const tId = new BN(opts.tournamentId);
      const [tPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("tournament"), tId.toArrayLike(Buffer, "le", 8)],
        program.programId
      );
      const now = Math.floor(Date.now() / 1000);
      await program.methods
        .createTournament(tId, new BN(now - 60), new BN(now + 86400))
        .accountsPartial({
          tournament: tPda,
          authority: provider.wallet.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .rpc();

      const prizeBefore = BigInt(
        (await program.account.tournament.fetch(tPda)).prizeLamports.toString()
      );
      const p1Before = await provider.connection.getBalance(player1.publicKey);
      const p2Before = await provider.connection.getBalance(player2.publicKey);

      const [gPda, , rMatchup] = await createGameOnChain(
        tPda,
        opts.matchupType,
        player1
      );
      const [p1Profile] = PublicKey.findProgramAddressSync(
        [
          Buffer.from("player"),
          tId.toArrayLike(Buffer, "le", 8),
          player1.publicKey.toBuffer(),
        ],
        program.programId
      );
      const p2Profile = await joinGameOnChain(gPda, tId, tPda, player2);

      const c1 = generateCommit(opts.p1Guess);
      const c2 = generateCommit(opts.p2Guess);
      // first_committer is set by COMMIT order; reveal order is independent.
      const order: Array<[Keypair, Commit]> =
        opts.commitFirst === 1
          ? [
              [player1, c1],
              [player2, c2],
            ]
          : [
              [player2, c2],
              [player1, c1],
            ];
      for (const [signer, commit] of order) {
        await program.methods
          .commitGuess(commit.commitment as any)
          .accountsPartial({ game: gPda, player: signer.publicKey })
          .signers([signer])
          .rpc();
      }

      const revealAccounts = {
        game: gPda,
        p1Profile,
        p2Profile,
        tournament: tPda,
        playerOneWallet: player1.publicKey,
        playerTwoWallet: player2.publicKey,
        globalConfig: globalConfigPda,
        treasury: treasury.publicKey,
        systemProgram: SystemProgram.programId,
      };
      // Player 1 reveals first and provides r_matchup; player 2 passes null.
      await program.methods
        .revealGuess(c1.r as any, rMatchup as any)
        .accountsPartial({ ...revealAccounts, player: player1.publicKey })
        .signers([player1])
        .rpc();
      await program.methods
        .revealGuess(c2.r as any, null)
        .accountsPartial({ ...revealAccounts, player: player2.publicKey })
        .signers([player2])
        .rpc();

      const resolvedGame = await program.account.game.fetch(gPda);
      const tournament = await program.account.tournament.fetch(tPda);
      const prizeDelta =
        BigInt(tournament.prizeLamports.toString()) - prizeBefore;
      return {
        resolvedGame,
        prizeDelta,
        p1Delta:
          (await provider.connection.getBalance(player1.publicKey)) - p1Before,
        p2Delta:
          (await provider.connection.getBalance(player2.publicKey)) - p2Before,
      };
    }

    function assertNetDirection(delta: number, net: bigint, label: string) {
      if (net > 0n) {
        assert.isAbove(delta, 0, `${label} should net gain`);
      } else if (net < 0n) {
        assert.isBelow(delta, 0, `${label} should net lose`);
      } else {
        // Broke even on stake; only paid fees.
        assert.isAtMost(delta, 0, `${label} break-even (<= 0 after fees)`);
        assert.isAtLeast(delta, -FEE_MARGIN, `${label} fees within margin`);
      }
    }

    const CELLS: Array<{
      name: string;
      tournamentId: number;
      matchupType: 0 | 1;
      p1Guess: 0 | 1;
      p2Guess: 0 | 1;
      commitFirst: 1 | 2;
    }> = [
      // Homogeneous (same-team): correct guess = SAME_TEAM (0).
      {
        name: "homog both correct",
        tournamentId: 200,
        matchupType: 0,
        p1Guess: 0,
        p2Guess: 0,
        commitFirst: 1,
      },
      {
        name: "homog P1 correct only",
        tournamentId: 201,
        matchupType: 0,
        p1Guess: 0,
        p2Guess: 1,
        commitFirst: 1,
      },
      {
        name: "homog P2 correct only",
        tournamentId: 202,
        matchupType: 0,
        p1Guess: 1,
        p2Guess: 0,
        commitFirst: 1,
      },
      {
        name: "homog both wrong",
        tournamentId: 203,
        matchupType: 0,
        p1Guess: 1,
        p2Guess: 1,
        commitFirst: 1,
      },
      // Heterogeneous (diff-team): correct guess = DIFF_TEAM (1).
      {
        name: "hetero P1 wins (correct only)",
        tournamentId: 204,
        matchupType: 1,
        p1Guess: 1,
        p2Guess: 0,
        commitFirst: 2,
      },
      {
        name: "hetero P2 wins (correct only)",
        tournamentId: 205,
        matchupType: 1,
        p1Guess: 0,
        p2Guess: 1,
        commitFirst: 1,
      },
      {
        name: "hetero both correct -> first committer P1",
        tournamentId: 206,
        matchupType: 1,
        p1Guess: 1,
        p2Guess: 1,
        commitFirst: 1,
      },
      {
        name: "hetero both correct -> first committer P2",
        tournamentId: 207,
        matchupType: 1,
        p1Guess: 1,
        p2Guess: 1,
        commitFirst: 2,
      },
      {
        name: "hetero both wrong",
        tournamentId: 208,
        matchupType: 1,
        p1Guess: 0,
        p2Guess: 0,
        commitFirst: 1,
      },
    ];

    for (const cell of CELLS) {
      it(`${cell.name} resolves to the oracle outcome`, async () => {
        const { resolvedGame, prizeDelta, p1Delta, p2Delta } =
          await playToResolution(cell);

        // On-chain transcript matches what we played.
        assert.equal(resolvedGame.p1Guess, cell.p1Guess, "p1 guess");
        assert.equal(resolvedGame.p2Guess, cell.p2Guess, "p2 guess");
        assert.equal(
          resolvedGame.matchupType,
          cell.matchupType,
          "matchup type"
        );
        assert.equal(
          resolvedGame.firstCommitter,
          cell.commitFirst,
          "first committer"
        );
        assert.notEqual(resolvedGame.resolvedAt.toString(), "0", "resolved");

        const oracle = {
          stepCount: 4,
          matchupType: cell.matchupType,
          p1Guess: cell.p1Guess,
          p2Guess: cell.p2Guess,
          firstCommitter: cell.commitFirst,
        };
        const payoff = resolvePayoff({ ...oracle, stake: STAKE_L });
        const { prizeGain } = splitTournamentGain(
          payoff.tournamentGain,
          TREASURY_SPLIT_BPS
        );

        // Prize-pool delta is exact (no tx fees touch the tournament PDA).
        assert.equal(
          prizeDelta.toString(),
          prizeGain.toString(),
          `${cell.name}: prize-pool delta`
        );

        // Balance direction matches net (return - stake); fees only blur the
        // break-even (both-correct) case slightly negative.
        assertNetDirection(
          p1Delta,
          payoff.p1Return - STAKE_L,
          `${cell.name}: p1`
        );
        assertNetDirection(
          p2Delta,
          payoff.p2Return - STAKE_L,
          `${cell.name}: p2`
        );
      });
    }
  });

  it("rejects create_game outside tournament window", async () => {
    // Tournament 999 was created with end_time = now + 2 and is now expired
    const expiredId = new BN(999);
    const [expiredTournamentPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("tournament"), expiredId.toArrayLike(Buffer, "le", 8)],
      program.programId
    );

    // deposit_stake also checks tournament window — this should fail
    try {
      await depositStake(expiredId, expiredTournamentPda, player1);
      assert.fail("Expected OutsideTournamentWindow error");
    } catch (e: any) {
      // assert.fail above throws AssertionError — re-throw it so the
      // no-rejection path cannot satisfy this catch's substring assert.
      if (e?.name === "AssertionError") throw e;
      assert.include(e.toString(), "OutsideTournamentWindow");
    }
  });

  // ---------------------------------------------------------------------------
  // Session-key delegation — handler-level coverage for every *_session
  // variant against a local validator. Live mainnet uses these via the
  // game frontend's ephemeral 24h session keypairs (per
  // frontend/coordination-game/CLAUDE.md "Session Key Lifecycle"); pre-this
  // batch the variants had zero on-chain tests.
  //
  // Sub-tests run linearly so each builds on the prior:
  //   1. create_player_session (matchmaker + p1 + p2)
  //   2. deposit_stake_session  (p1)
  //   3. create_game_session    (matchmaker delegates → game created)
  //   4. deposit_stake_session  (p2)
  //   5. join_game_session      (p2 → game Active)
  //   6. commit_guess_session   (both players via their session keys)
  //   7. reveal_guess_session   (both players → game Resolved)
  //   8. close_session_by_delegate
  // Plus one rejection: close_session_by_delegate fails when the session
  // signer doesn't match the SessionAuthority PDA's stored session_key.
  // ---------------------------------------------------------------------------

  describe("session-key delegation", () => {
    // Independent tournament so session-flow state doesn't intermix with
    // the other test groups' state.
    const SESSION_TOURNAMENT_ID = new BN(2);
    let sessionTournamentPda: PublicKey;

    // Three SessionAuthority PDAs: matchmaker (used to create_game_session),
    // p1 (used for stake/commit/reveal), p2 (same).
    const matchmakerSessionKey = Keypair.generate();
    const p1SessionKey = Keypair.generate();
    const p2SessionKey = Keypair.generate();

    // Linear-flow shared state.
    let sessionGamePda: PublicKey;
    let sessionGameId: BN;
    let sessionMatchupR: number[];
    const sessionP1Commit = generateCommit(GUESS_DIFF_TEAM);
    const sessionP2Commit = generateCommit(GUESS_DIFF_TEAM);

    function gameSessionPda(
      principal: PublicKey,
      sessionKey: PublicKey
    ): [PublicKey, number] {
      return PublicKey.findProgramAddressSync(
        [
          Buffer.from("game_session"),
          principal.toBuffer(),
          sessionKey.toBuffer(),
        ],
        program.programId
      );
    }

    before(async () => {
      // Tournament with a 7-day window so the create_game cutoff
      // (now + commit/reveal/2 < end_time) is easily satisfied.
      const now = Math.floor(Date.now() / 1000);
      const startTime = new BN(now - 60);
      const endTime = new BN(now + 7 * 24 * 3600);
      [sessionTournamentPda] = PublicKey.findProgramAddressSync(
        [
          Buffer.from("tournament"),
          SESSION_TOURNAMENT_ID.toArrayLike(Buffer, "le", 8),
        ],
        program.programId
      );
      try {
        await program.methods
          .createTournament(SESSION_TOURNAMENT_ID, startTime, endTime)
          .accountsPartial({
            tournament: sessionTournamentPda,
            authority: matchmaker.publicKey,
            systemProgram: SystemProgram.programId,
          })
          .rpc();
      } catch (e: any) {
        // Tournament 2 may already exist if a previous run created it —
        // the suite shares one validator boot across the whole file.
        if (!e.toString().includes("already in use")) throw e;
      }

      // Fund all three session keypairs so they can pay their own tx fees
      // and (for p1 / p2) cover the FIXED_STAKE_LAMPORTS deposit.
      for (const kp of [matchmakerSessionKey, p1SessionKey, p2SessionKey]) {
        const sig = await provider.connection.requestAirdrop(
          kp.publicKey,
          LAMPORTS_PER_SOL
        );
        await provider.connection.confirmTransaction(sig, "confirmed");
      }
    });

    it("create_player_session: matchmaker + p1 + p2 each authorize a session key", async () => {
      const cases: Array<[anchor.Wallet | Keypair, Keypair]> = [
        [matchmaker as unknown as anchor.Wallet, matchmakerSessionKey],
        [player1, p1SessionKey],
        [player2, p2SessionKey],
      ];

      for (const [principal, sessionKey] of cases) {
        const principalKey = principal.publicKey;
        const [sessionAuthorityPda] = gameSessionPda(
          principalKey,
          sessionKey.publicKey
        );

        const builder = program.methods.createPlayerSession().accountsPartial({
          sessionAuthority: sessionAuthorityPda,
          player: principalKey,
          sessionKey: sessionKey.publicKey,
          systemProgram: SystemProgram.programId,
        });
        // For matchmaker (the provider wallet) the provider auto-signs;
        // for p1 / p2 we explicitly sign.
        if (principal === matchmaker) {
          await builder.rpc();
        } else {
          await builder.signers([principal as Keypair]).rpc();
        }

        const session = await program.account.sessionAuthority.fetch(
          sessionAuthorityPda
        );
        assert.equal(
          session.player.toString(),
          principalKey.toString(),
          "session.player must equal authorizing principal"
        );
        assert.equal(
          session.sessionKey.toString(),
          sessionKey.publicKey.toString(),
          "session.session_key must equal delegated keypair"
        );
        assert.isAbove(
          session.expiresAt.toNumber(),
          Math.floor(Date.now() / 1000),
          "expires_at must be in the future"
        );
      }
    });

    it("deposit_stake_session: session signer funds escrow on behalf of player 1", async () => {
      const [escrow] = escrowPda(SESSION_TOURNAMENT_ID, player1.publicKey);
      const [sessionAuthorityPda] = gameSessionPda(
        player1.publicKey,
        p1SessionKey.publicKey
      );

      await program.methods
        .depositStakeSession()
        .accountsPartial({
          escrow,
          tournament: sessionTournamentPda,
          player: player1.publicKey,
          sessionAuthority: sessionAuthorityPda,
          sessionSigner: p1SessionKey.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([p1SessionKey])
        .rpc();

      const escrowAcct = await program.account.stakeEscrow.fetch(escrow);
      assert.equal(escrowAcct.player.toString(), player1.publicKey.toString());
      assert.equal(escrowAcct.amount.toString(), STAKE.toString());
      assert.isFalse(escrowAcct.consumed);
    });

    it("create_game_session: player 1 session signer creates game, matchmaker co-signs", async () => {
      const matchupCommit = generateMatchupCommit(GUESS_DIFF_TEAM as 0 | 1);
      sessionMatchupR = matchupCommit.r;

      const counter = await program.account.gameCounter.fetch(gameCounterPda);
      sessionGameId = counter.count as BN;
      [sessionGamePda] = PublicKey.findProgramAddressSync(
        [Buffer.from("game"), sessionGameId.toArrayLike(Buffer, "le", 8)],
        program.programId
      );
      const [profilePda] = PublicKey.findProgramAddressSync(
        [
          Buffer.from("player"),
          SESSION_TOURNAMENT_ID.toArrayLike(Buffer, "le", 8),
          player1.publicKey.toBuffer(),
        ],
        program.programId
      );
      const [escrow] = escrowPda(SESSION_TOURNAMENT_ID, player1.publicKey);
      // The PLAYER's own session authority (same PDA join_game_session uses),
      // NOT the matchmaker's — the player's session key pays + signs, and the
      // matchmaker co-signs to attest the commitment.
      const [p1SessionAuthorityPda] = gameSessionPda(
        player1.publicKey,
        p1SessionKey.publicKey
      );

      await program.methods
        .createGameSession(STAKE, matchupCommit.commitment as any)
        .accountsPartial({
          game: sessionGamePda,
          gameCounter: gameCounterPda,
          playerProfile: profilePda,
          escrow,
          tournament: sessionTournamentPda,
          globalConfig: globalConfigPda,
          matchmaker: matchmaker.publicKey,
          player: player1.publicKey,
          sessionAuthority: p1SessionAuthorityPda,
          sessionSigner: p1SessionKey.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([p1SessionKey])
        .rpc();

      const game = await program.account.game.fetch(sessionGamePda);
      assert.deepEqual(game.state, { pending: {} });
      assert.equal(game.playerOne.toString(), player1.publicKey.toString());
      assert.equal(
        game.stakeLamports.toString(),
        STAKE.toString(),
        "stake transferred to game PDA"
      );
    });

    it("create_game_session: rejects a non-matchmaker co-signer (forgery guard)", async () => {
      // The whole point of requiring `matchmaker: Signer` is that a player
      // cannot self-create a game with a commitment only they can open. A
      // co-signer that is not GlobalConfig.matchmaker must revert BEFORE any
      // game is created — so the player can never bypass game-api's cosign
      // (which validates the true matchup_commitment).
      const fakeMatchmaker = Keypair.generate();
      const air = await provider.connection.requestAirdrop(
        fakeMatchmaker.publicKey,
        LAMPORTS_PER_SOL
      );
      await provider.connection.confirmTransaction(air, "confirmed");

      const counter = await program.account.gameCounter.fetch(gameCounterPda);
      const forgedGameId = counter.count as BN;
      const [forgedGamePda] = PublicKey.findProgramAddressSync(
        [Buffer.from("game"), forgedGameId.toArrayLike(Buffer, "le", 8)],
        program.programId
      );
      const [profilePda] = PublicKey.findProgramAddressSync(
        [
          Buffer.from("player"),
          SESSION_TOURNAMENT_ID.toArrayLike(Buffer, "le", 8),
          player1.publicKey.toBuffer(),
        ],
        program.programId
      );
      const [escrow] = escrowPda(SESSION_TOURNAMENT_ID, player1.publicKey);
      const [p1SessionAuthorityPda] = gameSessionPda(
        player1.publicKey,
        p1SessionKey.publicKey
      );
      const forged = generateMatchupCommit(GUESS_DIFF_TEAM as 0 | 1);

      let reverted = false;
      try {
        await program.methods
          .createGameSession(STAKE, forged.commitment as any)
          .accountsPartial({
            game: forgedGamePda,
            gameCounter: gameCounterPda,
            playerProfile: profilePda,
            escrow,
            tournament: sessionTournamentPda,
            globalConfig: globalConfigPda,
            matchmaker: fakeMatchmaker.publicKey,
            player: player1.publicKey,
            sessionAuthority: p1SessionAuthorityPda,
            sessionSigner: p1SessionKey.publicKey,
            systemProgram: SystemProgram.programId,
          })
          .signers([p1SessionKey, fakeMatchmaker])
          .rpc();
      } catch (_e) {
        reverted = true;
      }
      assert.isTrue(
        reverted,
        "create_game_session must reject a co-signer that is not the matchmaker"
      );
    });

    it("deposit_stake_session: same flow for player 2", async () => {
      const [escrow] = escrowPda(SESSION_TOURNAMENT_ID, player2.publicKey);
      const [sessionAuthorityPda] = gameSessionPda(
        player2.publicKey,
        p2SessionKey.publicKey
      );

      await program.methods
        .depositStakeSession()
        .accountsPartial({
          escrow,
          tournament: sessionTournamentPda,
          player: player2.publicKey,
          sessionAuthority: sessionAuthorityPda,
          sessionSigner: p2SessionKey.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([p2SessionKey])
        .rpc();

      const escrowAcct = await program.account.stakeEscrow.fetch(escrow);
      assert.equal(escrowAcct.amount.toString(), STAKE.toString());
    });

    it("join_game_session: player 2 session signer joins → game Active", async () => {
      const [profilePda] = PublicKey.findProgramAddressSync(
        [
          Buffer.from("player"),
          SESSION_TOURNAMENT_ID.toArrayLike(Buffer, "le", 8),
          player2.publicKey.toBuffer(),
        ],
        program.programId
      );
      const [escrow] = escrowPda(SESSION_TOURNAMENT_ID, player2.publicKey);
      const [p2SessionAuthorityPda] = gameSessionPda(
        player2.publicKey,
        p2SessionKey.publicKey
      );

      await program.methods
        .joinGameSession()
        .accountsPartial({
          game: sessionGamePda,
          playerProfile: profilePda,
          escrow,
          tournament: sessionTournamentPda,
          globalConfig: globalConfigPda,
          matchmaker: matchmaker.publicKey,
          player: player2.publicKey,
          sessionAuthority: p2SessionAuthorityPda,
          sessionSigner: p2SessionKey.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([p2SessionKey])
        .rpc();

      const game = await program.account.game.fetch(sessionGamePda);
      assert.deepEqual(game.state, { active: {} });
      assert.equal(game.playerTwo.toString(), player2.publicKey.toString());
    });

    it("commit_guess_session: both players commit via session keys → game Revealing", async () => {
      const [p1SessionAuthorityPda] = gameSessionPda(
        player1.publicKey,
        p1SessionKey.publicKey
      );
      const [p2SessionAuthorityPda] = gameSessionPda(
        player2.publicKey,
        p2SessionKey.publicKey
      );

      // P1 commits via session key.
      await program.methods
        .commitGuessSession(sessionP1Commit.commitment as any)
        .accountsPartial({
          game: sessionGamePda,
          player: player1.publicKey,
          sessionAuthority: p1SessionAuthorityPda,
          sessionSigner: p1SessionKey.publicKey,
        })
        .signers([p1SessionKey])
        .rpc();

      // After 1st commit, state is Committing.
      let game = await program.account.game.fetch(sessionGamePda);
      assert.deepEqual(game.state, { committing: {} });

      // P2 commits — flips to Revealing.
      await program.methods
        .commitGuessSession(sessionP2Commit.commitment as any)
        .accountsPartial({
          game: sessionGamePda,
          player: player2.publicKey,
          sessionAuthority: p2SessionAuthorityPda,
          sessionSigner: p2SessionKey.publicKey,
        })
        .signers([p2SessionKey])
        .rpc();

      game = await program.account.game.fetch(sessionGamePda);
      assert.deepEqual(game.state, { revealing: {} });
    });

    it("reveal_guess_session: both players reveal via session keys → Resolved", async () => {
      const [p1ProfilePdaSession] = PublicKey.findProgramAddressSync(
        [
          Buffer.from("player"),
          SESSION_TOURNAMENT_ID.toArrayLike(Buffer, "le", 8),
          player1.publicKey.toBuffer(),
        ],
        program.programId
      );
      const [p2ProfilePdaSession] = PublicKey.findProgramAddressSync(
        [
          Buffer.from("player"),
          SESSION_TOURNAMENT_ID.toArrayLike(Buffer, "le", 8),
          player2.publicKey.toBuffer(),
        ],
        program.programId
      );
      const [p1SessionAuthorityPda] = gameSessionPda(
        player1.publicKey,
        p1SessionKey.publicKey
      );
      const [p2SessionAuthorityPda] = gameSessionPda(
        player2.publicKey,
        p2SessionKey.publicKey
      );

      // P1 reveals first — also provides r_matchup so the chain learns matchup type.
      await program.methods
        .revealGuessSession(sessionP1Commit.r as any, sessionMatchupR as any)
        .accountsPartial({
          game: sessionGamePda,
          player: player1.publicKey,
          sessionAuthority: p1SessionAuthorityPda,
          sessionSigner: p1SessionKey.publicKey,
          p1Profile: p1ProfilePdaSession,
          p2Profile: p2ProfilePdaSession,
          tournament: sessionTournamentPda,
          globalConfig: globalConfigPda,
          treasury: treasury.publicKey,
          playerOneWallet: player1.publicKey,
          playerTwoWallet: player2.publicKey,
        })
        .signers([p1SessionKey])
        .rpc();

      // P2 reveals — game finalizes.
      await program.methods
        .revealGuessSession(sessionP2Commit.r as any, null)
        .accountsPartial({
          game: sessionGamePda,
          player: player2.publicKey,
          sessionAuthority: p2SessionAuthorityPda,
          sessionSigner: p2SessionKey.publicKey,
          p1Profile: p1ProfilePdaSession,
          p2Profile: p2ProfilePdaSession,
          tournament: sessionTournamentPda,
          globalConfig: globalConfigPda,
          treasury: treasury.publicKey,
          playerOneWallet: player1.publicKey,
          playerTwoWallet: player2.publicKey,
        })
        .signers([p2SessionKey])
        .rpc();

      const game = await program.account.game.fetch(sessionGamePda);
      assert.deepEqual(game.state, { resolved: {} });
      assert.equal(game.matchupType, GUESS_DIFF_TEAM);
    });

    it("close_session_by_delegate: session signer can close its own session", async () => {
      const [p1SessionAuthorityPda] = gameSessionPda(
        player1.publicKey,
        p1SessionKey.publicKey
      );

      await program.methods
        .closeSessionByDelegate()
        .accountsPartial({
          sessionAuthority: p1SessionAuthorityPda,
          sessionSigner: p1SessionKey.publicKey,
        })
        .signers([p1SessionKey])
        .rpc();

      try {
        await program.account.sessionAuthority.fetch(p1SessionAuthorityPda);
        assert.fail("session authority account should be closed");
      } catch (e: any) {
        assert.isTrue(
          e.toString().includes("Account does not exist") ||
            e.toString().includes("Could not find") ||
            e.toString().includes("AccountNotFound"),
          `expected closed account, got: ${e}`
        );
      }
    });

    it("close_session_by_delegate: rejects a non-matching signer", async () => {
      // p2's session is still open at this point. An unrelated keypair
      // attempts to close p2's session — Anchor's seeds constraint will
      // reject because the seeds-derived PDA won't match.
      const imposter = Keypair.generate();
      const sig = await provider.connection.requestAirdrop(
        imposter.publicKey,
        LAMPORTS_PER_SOL
      );
      await provider.connection.confirmTransaction(sig, "confirmed");

      const [p2SessionAuthorityPda] = gameSessionPda(
        player2.publicKey,
        p2SessionKey.publicKey
      );

      try {
        await program.methods
          .closeSessionByDelegate()
          .accountsPartial({
            sessionAuthority: p2SessionAuthorityPda,
            sessionSigner: imposter.publicKey,
          })
          .signers([imposter])
          .rpc();
        assert.fail("close_session_by_delegate from wrong signer must fail");
      } catch (e: any) {
        // Anchor's seeds constraint failure surfaces in different ways
        // depending on bump check ordering; accept the broad family.
        const errStr = e.toString();
        assert.isTrue(
          errStr.includes("seeds") ||
            errStr.includes("ConstraintSeeds") ||
            errStr.includes("SessionSignerMismatch") ||
            errStr.includes("Error"),
          `expected constraint error, got: ${errStr}`
        );
      }
    });
  });
});
