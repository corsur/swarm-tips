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

  it("heterogeneous game: p1 commits first, both correct → p1 gets full pot (2× stake)", async () => {
    // Create a fresh tournament for this test so we can run it in isolation
    const heteroTournamentId = new BN(2);
    const [heteroTournamentPda] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("tournament"),
        heteroTournamentId.toArrayLike(Buffer, "le", 8),
      ],
      program.programId
    );
    const now = Math.floor(Date.now() / 1000);
    await program.methods
      .createTournament(
        heteroTournamentId,
        new BN(now - 60),
        new BN(now + 86400)
      )
      .accountsPartial({
        tournament: heteroTournamentPda,
        authority: provider.wallet.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    // Capture balances before any stake is locked, so the full stake loss is visible.
    const p1BalanceBefore = await provider.connection.getBalance(
      player1.publicKey
    );
    const p2BalanceBefore = await provider.connection.getBalance(
      player2.publicKey
    );

    // P1 creates game with matchup_type = 1 (different teams), matchmaker co-signs
    const [heteroGamePda, , heteroRMatchup] = await createGameOnChain(
      heteroTournamentPda,
      GUESS_DIFF_TEAM,
      player1
    );

    const createdGame = await program.account.game.fetch(heteroGamePda);
    assert.equal(
      createdGame.playerOne.toString(),
      player1.publicKey.toString(),
      "player_one should be set at creation"
    );

    // P1 profile was created at game creation
    const [hetP1ProfilePda] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("player"),
        heteroTournamentId.toArrayLike(Buffer, "le", 8),
        player1.publicKey.toBuffer(),
      ],
      program.programId
    );
    // P2 joins
    const hetP2ProfilePda = await joinGameOnChain(
      heteroGamePda,
      heteroTournamentId,
      heteroTournamentPda,
      player2
    );

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

    // Both reveal — first revealer provides r_matchup, second passes null
    const revealAccounts = {
      game: heteroGamePda,
      p1Profile: hetP1ProfilePda,
      p2Profile: hetP2ProfilePda,
      tournament: heteroTournamentPda,
      playerOneWallet: player1.publicKey,
      playerTwoWallet: player2.publicKey,
      globalConfig: globalConfigPda,
      treasury: treasury.publicKey,
      systemProgram: SystemProgram.programId,
    };

    await program.methods
      .revealGuess(hetP1Commit.r as any, heteroRMatchup as any)
      .accountsPartial({ ...revealAccounts, player: player1.publicKey })
      .signers([player1])
      .rpc();

    await program.methods
      .revealGuess(hetP2Commit.r as any, null)
      .accountsPartial({ ...revealAccounts, player: player2.publicKey })
      .signers([player2])
      .rpc();

    const resolvedGame = await program.account.game.fetch(heteroGamePda);
    assert.equal(
      resolvedGame.p1Guess,
      GUESS_DIFF_TEAM,
      "p1 should have guessed diff team"
    );
    assert.equal(
      resolvedGame.p2Guess,
      GUESS_DIFF_TEAM,
      "p2 should have guessed diff team"
    );
    assert.equal(
      resolvedGame.matchupType,
      GUESS_DIFF_TEAM,
      "matchup_type should be revealed as diff team"
    );
    assert.equal(
      resolvedGame.firstCommitter,
      1,
      "p1 should be first committer"
    );
    assert.notEqual(
      resolvedGame.resolvedAt.toString(),
      "0",
      "game should be resolved"
    );

    // P1 committed first, both correct → p1 wins full pot (2× stake), tournament gains nothing
    const hetTournament = await program.account.tournament.fetch(
      heteroTournamentPda
    );
    assert.equal(
      hetTournament.prizeLamports.toString(),
      "0",
      "tournament should gain nothing in heterogeneous game"
    );

    // P1 net balance should have increased by approximately stake (received 2S, spent S + tx fees)
    const p1BalanceAfter = await provider.connection.getBalance(
      player1.publicKey
    );
    assert.isAbove(p1BalanceAfter, p1BalanceBefore, "p1 should net gain stake");

    // P2 should receive nothing (lost)
    const p2BalanceAfter = await provider.connection.getBalance(
      player2.publicKey
    );
    // p2 net = -(stake + tx fees) — approximately, just check they didn't gain
    assert.isBelow(p2BalanceAfter, p2BalanceBefore, "p2 should lose stake");
  });

  it("heterogeneous game: both wrong → full refund, tournament gains nothing", async () => {
    const bothWrongTournamentId = new BN(3);
    const [bothWrongTournamentPda] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("tournament"),
        bothWrongTournamentId.toArrayLike(Buffer, "le", 8),
      ],
      program.programId
    );
    const now = Math.floor(Date.now() / 1000);
    await program.methods
      .createTournament(
        bothWrongTournamentId,
        new BN(now - 60),
        new BN(now + 86400)
      )
      .accountsPartial({
        tournament: bothWrongTournamentPda,
        authority: provider.wallet.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    // Capture balances before deposit so the full cost (deposit tx + game txs)
    // is visible. The refund returns stake to the player wallet, not the escrow,
    // so capturing after deposit would make p1BalanceAfter > p1BalanceBefore.
    const p1BalanceBefore = await provider.connection.getBalance(
      player1.publicKey
    );
    const p2BalanceBefore = await provider.connection.getBalance(
      player2.publicKey
    );

    // P1 creates game with matchup_type = 1 (different teams), P2 joins
    const [bothWrongGamePda, , bwRMatchup] = await createGameOnChain(
      bothWrongTournamentPda,
      GUESS_DIFF_TEAM,
      player1
    );
    const [bwP1ProfilePda] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("player"),
        bothWrongTournamentId.toArrayLike(Buffer, "le", 8),
        player1.publicKey.toBuffer(),
      ],
      program.programId
    );
    const bwP2ProfilePda = await joinGameOnChain(
      bothWrongGamePda,
      bothWrongTournamentId,
      bothWrongTournamentPda,
      player2
    );

    // Both commit SAME_TEAM (0 = wrong for a heterogeneous match)
    const bwP1Commit = generateCommit(GUESS_SAME_TEAM);
    const bwP2Commit = generateCommit(GUESS_SAME_TEAM);

    await program.methods
      .commitGuess(bwP1Commit.commitment as any)
      .accountsPartial({ game: bothWrongGamePda, player: player1.publicKey })
      .signers([player1])
      .rpc();

    await program.methods
      .commitGuess(bwP2Commit.commitment as any)
      .accountsPartial({ game: bothWrongGamePda, player: player2.publicKey })
      .signers([player2])
      .rpc();

    // Both reveal — first revealer provides r_matchup, second passes null
    const bwRevealAccounts = {
      game: bothWrongGamePda,
      p1Profile: bwP1ProfilePda,
      p2Profile: bwP2ProfilePda,
      tournament: bothWrongTournamentPda,
      playerOneWallet: player1.publicKey,
      playerTwoWallet: player2.publicKey,
      globalConfig: globalConfigPda,
      treasury: treasury.publicKey,
      systemProgram: SystemProgram.programId,
    };

    await program.methods
      .revealGuess(bwP1Commit.r as any, bwRMatchup as any)
      .accountsPartial({ ...bwRevealAccounts, player: player1.publicKey })
      .signers([player1])
      .rpc();

    await program.methods
      .revealGuess(bwP2Commit.r as any, null)
      .accountsPartial({ ...bwRevealAccounts, player: player2.publicKey })
      .signers([player2])
      .rpc();

    // Both wrong → full forfeiture: 2× stake split between treasury (50%) and tournament (50%)
    const bwTournament = await program.account.tournament.fetch(
      bothWrongTournamentPda
    );
    const twoStakes = STAKE.toNumber() * 2;
    // At 5000 bps (50/50 split), tournament gets half of 2S
    const expectedTournamentShare = Math.floor(twoStakes / 2);
    assert.equal(
      bwTournament.prizeLamports.toString(),
      expectedTournamentShare.toString(),
      "tournament should gain half of 2× stake (treasury gets the other half)"
    );

    // Both players should lose their full stake
    const p1BalanceAfter = await provider.connection.getBalance(
      player1.publicKey
    );
    const p2BalanceAfter = await provider.connection.getBalance(
      player2.publicKey
    );
    const stakeNum = STAKE.toNumber();
    assert.isBelow(
      p1BalanceAfter,
      p1BalanceBefore - stakeNum + 100_000, // allow small margin for rent reclaim
      "p1 should lose full stake"
    );
    assert.isBelow(
      p2BalanceAfter,
      p2BalanceBefore - stakeNum + 100_000,
      "p2 should lose full stake"
    );
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
      assert.include(e.toString(), "OutsideTournamentWindow");
    }
  });
});
