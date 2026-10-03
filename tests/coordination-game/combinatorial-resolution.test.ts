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
  OutcomeKind,
  resolvePayoff,
  splitTournamentGain,
} from "../helpers/outcome-oracle.ts";
import {
  STAKE,
  Commit,
  generateCommit,
  depositStake,
  createGameOnChain,
  joinGameOnChain,
  ensureConfigInitialized,
  sharedTreasury,
} from "./common.ts";

describe("payoff matrix — combinatorial on-chain resolution", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const program = anchor.workspace
    .coordinationGame as Program<CoordinationGame>;

  const player1 = Keypair.generate();
  const player2 = Keypair.generate();
  let gameCounterPda: PublicKey;
  let globalConfigPda: PublicKey;
  let treasuryKey = sharedTreasury.publicKey;

  const STAKE_L = BigInt(STAKE.toString());
  const TREASURY_SPLIT_BPS = 5000;
  const FEE_MARGIN = 10_000_000; // 0.01 SOL

  before(async () => {
    for (const player of [player1, player2]) {
      const sig = await provider.connection.requestAirdrop(
        player.publicKey,
        2 * LAMPORTS_PER_SOL
      );
      await provider.connection.confirmTransaction(sig);
    }

    const inited = await ensureConfigInitialized(
      program,
      provider,
      treasuryKey
    );
    gameCounterPda = inited.gameCounterPda;
    globalConfigPda = inited.globalConfigPda;
    treasuryKey = inited.treasury;
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
      program,
      gameCounterPda,
      globalConfigPda,
      provider.wallet.publicKey,
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
    const p2Profile = await joinGameOnChain(
      program,
      globalConfigPda,
      provider.wallet.publicKey,
      gPda,
      tId,
      tPda,
      player2
    );

    const c1 = generateCommit(opts.p1Guess);
    const c2 = generateCommit(opts.p2Guess);
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
      treasury: treasuryKey,
      systemProgram: SystemProgram.programId,
    };
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

      assert.equal(resolvedGame.p1Guess, cell.p1Guess, "p1 guess");
      assert.equal(resolvedGame.p2Guess, cell.p2Guess, "p2 guess");
      assert.equal(resolvedGame.matchupType, cell.matchupType, "matchup type");
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

      assert.equal(
        prizeDelta.toString(),
        prizeGain.toString(),
        `${cell.name}: prize-pool delta`
      );

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

  it("rejects create_game outside tournament window", async () => {
    const expiredId = new BN(997);
    const [expiredTournamentPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("tournament"), expiredId.toArrayLike(Buffer, "le", 8)],
      program.programId
    );
    const now = Math.floor(Date.now() / 1000);
    await program.methods
      .createTournament(expiredId, new BN(now + 3600), new BN(now + 7200))
      .accountsPartial({
        tournament: expiredTournamentPda,
        authority: provider.wallet.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    try {
      await depositStake(program, expiredId, expiredTournamentPda, player1);
      assert.fail("Expected OutsideTournamentWindow error");
    } catch (e: any) {
      if (e?.name === "AssertionError") throw e;
      assert.include(e.toString(), "OutsideTournamentWindow");
    }
  });
});
