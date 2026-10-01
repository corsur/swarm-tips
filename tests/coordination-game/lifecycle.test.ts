import * as anchor from "@coral-xyz/anchor";
import { Program, BN } from "@coral-xyz/anchor";
import type { CoordinationGame } from "../../target/types/coordination_game";
import {
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  SystemProgram,
} from "@solana/web3.js";
import { assert } from "chai";
import {
  TOURNAMENT_ID,
  STAKE,
  GUESS_SAME_TEAM,
  generateCommit,
  generateMatchupCommit,
  tournamentIdBuf,
  escrowPda,
  depositStake,
  createGameOnChain,
  joinGameOnChain,
  sharedTreasury,
} from "./common.ts";

describe("coordination-game lifecycle", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const program = anchor.workspace
    .coordinationGame as Program<CoordinationGame>;

  const player1 = Keypair.generate();
  const player2 = Keypair.generate();

  let gameCounterPda: PublicKey;
  let globalConfigPda: PublicKey;
  let tournamentPda: PublicKey;
  let gamePda: PublicKey;
  let p1ProfilePda: PublicKey;
  let p2ProfilePda: PublicKey;
  const matchmaker = provider.wallet;
  const treasury = sharedTreasury;

  const p1Commit = generateCommit(GUESS_SAME_TEAM);
  const p2Commit = generateCommit(GUESS_SAME_TEAM);

  let mainGameRMatchup: number[];

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
    await program.methods
      .initializeConfig(5000) // 50/50 treasury/prize split
      .accountsPartial({
        globalConfig: globalConfigPda,
        authority: provider.wallet.publicKey,
        matchmaker: provider.wallet.publicKey,
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

  it("player 1 creates a game (matchmaker co-signs)", async () => {
    const matchupCommit = generateMatchupCommit(GUESS_SAME_TEAM as 0 | 1);
    mainGameRMatchup = matchupCommit.r;

    await depositStake(program, TOURNAMENT_ID, tournamentPda, player1);

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
    const [p1Escrow] = escrowPda(
      program.programId,
      TOURNAMENT_ID,
      player1.publicKey
    );

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
    await depositStake(program, TOURNAMENT_ID, tournamentPda, player2);

    const [escrow] = escrowPda(
      program.programId,
      TOURNAMENT_ID,
      player2.publicKey
    );

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

  it("rejects reveal with wrong preimage", async () => {
    const wrongR = Array(32).fill(0xff);
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
      if (e?.name === "AssertionError") throw e;
      assert.include(e.toString(), "NotAParticipant");
    }
  });

  it("player 1 reveals (first reveal — provides r_matchup)", async () => {
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
    assert.equal(
      game.p1Guess,
      GUESS_SAME_TEAM,
      "p1 should have guessed same team"
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

    const tournament = await program.account.tournament.fetch(tournamentPda);
    assert.equal(
      tournament.prizeLamports.toString(),
      "0",
      "tournament should gain nothing when both players guess correctly"
    );
  });

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
      if (e?.name === "AssertionError") throw e;
      assert.include(e.toString(), "Account does not exist");
    }
  });

  it("rejects joining own game", async () => {
    const [soloGamePda] = await createGameOnChain(
      program,
      gameCounterPda,
      globalConfigPda,
      matchmaker.publicKey,
      tournamentPda,
      GUESS_SAME_TEAM,
      player1
    );

    await depositStake(program, TOURNAMENT_ID, tournamentPda, player1);
    const [soloProfilePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("player"), tournamentIdBuf(), player1.publicKey.toBuffer()],
      program.programId
    );
    const [soloEscrow] = escrowPda(
      program.programId,
      TOURNAMENT_ID,
      player1.publicKey
    );

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
      if (e?.name === "AssertionError") throw e;
      assert.include(e.toString(), "CannotJoinOwnGame");
    }
  });

  it("rejects a join whose co-signer is not the matchmaker (interloper front-run)", async () => {
    const [victimGamePda] = await createGameOnChain(
      program,
      gameCounterPda,
      globalConfigPda,
      matchmaker.publicKey,
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
    await depositStake(program, TOURNAMENT_ID, tournamentPda, interloper);

    const [interloperProfile] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("player"),
        tournamentIdBuf(),
        interloper.publicKey.toBuffer(),
      ],
      program.programId
    );
    const [interloperEscrow] = escrowPda(
      program.programId,
      TOURNAMENT_ID,
      interloper.publicKey
    );

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
    const [timeoutGamePda] = await createGameOnChain(
      program,
      gameCounterPda,
      globalConfigPda,
      matchmaker.publicKey,
      tournamentPda,
      GUESS_SAME_TEAM,
      player1
    );
    const tp1ProfilePda = PublicKey.findProgramAddressSync(
      [Buffer.from("player"), tournamentIdBuf(), player1.publicKey.toBuffer()],
      program.programId
    )[0];
    const tp2ProfilePda = await joinGameOnChain(
      program,
      globalConfigPda,
      matchmaker.publicKey,
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
      if (e?.name === "AssertionError") throw e;
      assert.include(e.toString(), "InvalidTournamentTimes");
    }
  });

  it("rejects create_game with zero stake", async () => {
    const matchupCommit = generateMatchupCommit(GUESS_SAME_TEAM as 0 | 1);
    await depositStake(program, TOURNAMENT_ID, tournamentPda, player1);
    const counter = await program.account.gameCounter.fetch(gameCounterPda);
    const [zeroGamePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("game"), (counter.count as BN).toArrayLike(Buffer, "le", 8)],
      program.programId
    );
    const [profilePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("player"), tournamentIdBuf(), player1.publicKey.toBuffer()],
      program.programId
    );
    const [escrow] = escrowPda(
      program.programId,
      TOURNAMENT_ID,
      player1.publicKey
    );
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
      if (e?.name === "AssertionError") throw e;
      assert.include(e.toString(), "StakeMismatch");
    }
  });

  it("rejects create_game with wrong stake (0.1 SOL instead of 0.01 SOL)", async () => {
    const WRONG_STAKE = new BN(100_000_000); // 0.1 SOL
    const matchupCommit = generateMatchupCommit(GUESS_SAME_TEAM as 0 | 1);
    await depositStake(program, TOURNAMENT_ID, tournamentPda, player1);
    const counter = await program.account.gameCounter.fetch(gameCounterPda);
    const [wrongGamePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("game"), (counter.count as BN).toArrayLike(Buffer, "le", 8)],
      program.programId
    );
    const [profilePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("player"), tournamentIdBuf(), player1.publicKey.toBuffer()],
      program.programId
    );
    const [escrow] = escrowPda(
      program.programId,
      TOURNAMENT_ID,
      player1.publicKey
    );
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
      if (e?.name === "AssertionError") throw e;
      assert.include(e.toString(), "StakeMismatch");
    }
  });

  it("rejects commit from non-participant", async () => {
    const counter = await program.account.gameCounter.fetch(gameCounterPda);
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
      if (e?.name === "AssertionError") throw e;
      assert.include(e.toString(), "TournamentNotFinalized");
    }
  });

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

    // Wait for tournament window to close
    await new Promise((r) => setTimeout(r, 10000));

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
});
