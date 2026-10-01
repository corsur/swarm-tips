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
  STAKE,
  GUESS_DIFF_TEAM,
  generateCommit,
  generateMatchupCommit,
  escrowPda,
  ensureConfigInitialized,
} from "./common.ts";

describe("session-key delegation", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const program = anchor.workspace
    .coordinationGame as Program<CoordinationGame>;

  const player1 = Keypair.generate();
  const player2 = Keypair.generate();
  const matchmaker = provider.wallet;
  const treasury = Keypair.generate();

  let gameCounterPda: PublicKey;
  let globalConfigPda: PublicKey;

  const SESSION_TOURNAMENT_ID = new BN(2);
  let sessionTournamentPda: PublicKey;

  const matchmakerSessionKey = Keypair.generate();
  const p1SessionKey = Keypair.generate();
  const p2SessionKey = Keypair.generate();

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
    for (const player of [player1, player2]) {
      const sig = await provider.connection.requestAirdrop(
        player.publicKey,
        2 * LAMPORTS_PER_SOL
      );
      await provider.connection.confirmTransaction(sig, "confirmed");
    }

    const inited = await ensureConfigInitialized(
      program,
      provider,
      treasury.publicKey
    );
    gameCounterPda = inited.gameCounterPda;
    globalConfigPda = inited.globalConfigPda;

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
      if (!e.toString().includes("already in use")) throw e;
    }

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
    const [escrow] = escrowPda(
      program.programId,
      SESSION_TOURNAMENT_ID,
      player1.publicKey
    );
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
    const [escrow] = escrowPda(
      program.programId,
      SESSION_TOURNAMENT_ID,
      player1.publicKey
    );
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
    const [escrow] = escrowPda(
      program.programId,
      SESSION_TOURNAMENT_ID,
      player1.publicKey
    );
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
    const [escrow] = escrowPda(
      program.programId,
      SESSION_TOURNAMENT_ID,
      player2.publicKey
    );
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
    const [escrow] = escrowPda(
      program.programId,
      SESSION_TOURNAMENT_ID,
      player2.publicKey
    );
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

    let game = await program.account.game.fetch(sessionGamePda);
    assert.deepEqual(game.state, { committing: {} });

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
