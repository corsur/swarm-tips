import { AnchorProvider, BN, Program } from "@coral-xyz/anchor";
import { Buffer } from "buffer";
import {
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
  Transaction,
  type Commitment,
  type TransactionInstruction,
  type VersionedTransaction,
} from "@solana/web3.js";
import { COORDINATION_GAME_IDL, type CoordinationGame } from "../contracts/index.js";
import { SwarmClientError } from "../errors.js";
import {
  decodeGlobalConfig,
  decodeTournament,
  escrowPda,
  gameCounterPda,
  gamePda,
  globalConfigPda,
  playerProfilePda,
  playerSessionPda,
  tournamentPda,
  type GlobalConfigData,
  type TournamentData,
} from "./protocol.js";

export interface SolanaWalletSigner {
  publicKey: PublicKey;
  signTransaction<T extends Transaction | VersionedTransaction>(transaction: T): Promise<T>;
  signAllTransactions?<T extends Transaction | VersionedTransaction>(transactions: T[]): Promise<T[]>;
}

export interface SolanaSessionStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

export interface CoordinationGameSolanaClientOptions {
  connection?: Connection;
  rpcUrl?: string;
  wallet?: SolanaWalletSigner;
  tournamentId?: bigint;
  commitment?: Commitment;
}

const SESSION_STORAGE_KEY = "coordination-session";
const SESSION_TTL_MS = 24 * 60 * 60 * 1000;

function walletForSession(session: Keypair): SolanaWalletSigner {
  return {
    publicKey: session.publicKey,
    async signTransaction<T extends Transaction | VersionedTransaction>(transaction: T): Promise<T> {
      if (transaction instanceof Transaction) transaction.partialSign(session);
      else transaction.sign([session]);
      return transaction;
    },
    async signAllTransactions<T extends Transaction | VersionedTransaction>(transactions: T[]): Promise<T[]> {
      for (const transaction of transactions) await this.signTransaction(transaction);
      return transactions;
    },
  };
}

function readonlyWallet(): SolanaWalletSigner {
  const keypair = Keypair.generate();
  return {
    publicKey: keypair.publicKey,
    signTransaction: async (transaction) => transaction,
    signAllTransactions: async (transactions) => transactions,
  };
}

function anchorProgram(connection: Connection, wallet: SolanaWalletSigner, commitment: Commitment): Program<CoordinationGame> {
  const provider = new AnchorProvider(connection, wallet as never, { commitment, skipPreflight: false });
  return new Program<CoordinationGame>(COORDINATION_GAME_IDL, provider);
}

export function generateSessionKeypair(): Keypair { return Keypair.generate(); }

export function readStoredSession(
  storage: SolanaSessionStorage,
  walletAddress: string,
  now: () => number = Date.now,
): Keypair | null {
  const raw = storage.getItem(SESSION_STORAGE_KEY);
  if (!raw) return null;
  try {
    const value = JSON.parse(raw) as { secretKey?: unknown; createdAt?: unknown; wallet?: unknown };
    if (!Array.isArray(value.secretKey) || value.secretKey.length !== 64 || typeof value.createdAt !== "number" || value.wallet !== walletAddress || now() - value.createdAt > SESSION_TTL_MS) {
      storage.removeItem(SESSION_STORAGE_KEY);
      return null;
    }
    return Keypair.fromSecretKey(Uint8Array.from(value.secretKey as number[]));
  } catch {
    storage.removeItem(SESSION_STORAGE_KEY);
    return null;
  }
}

export function storeSession(
  storage: SolanaSessionStorage,
  session: Keypair,
  walletAddress: string,
  now: () => number = Date.now,
): void {
  storage.setItem(SESSION_STORAGE_KEY, JSON.stringify({
    secretKey: Array.from(session.secretKey),
    createdAt: now(),
    wallet: walletAddress,
  }));
}

export function clearSession(storage: SolanaSessionStorage): void {
  storage.removeItem(SESSION_STORAGE_KEY);
}

export class CoordinationGameSolanaClient {
  readonly connection: Connection;
  readonly wallet?: SolanaWalletSigner;
  readonly tournamentId: bigint;
  readonly commitment: Commitment;

  constructor(options: CoordinationGameSolanaClientOptions) {
    if (!options.connection && !options.rpcUrl) {
      throw new SwarmClientError({ code: "INVALID_ARGUMENT", operation: "solana.constructor", message: "connection or rpcUrl is required" });
    }
    this.connection = options.connection ?? new Connection(options.rpcUrl!, options.commitment ?? "confirmed");
    this.wallet = options.wallet;
    this.tournamentId = options.tournamentId ?? 1n;
    this.commitment = options.commitment ?? "confirmed";
  }

  program(wallet = this.wallet): Program<CoordinationGame> {
    return anchorProgram(this.connection, wallet ?? readonlyWallet(), this.commitment);
  }

  sessionProgram(session: Keypair): Program<CoordinationGame> {
    return anchorProgram(this.connection, walletForSession(session), this.commitment);
  }

  private requireWallet(): SolanaWalletSigner {
    if (!this.wallet) throw new SwarmClientError({ code: "INVALID_ARGUMENT", operation: "solana.wallet", message: "A wallet signer is required" });
    return this.wallet;
  }

  liveStakeRemainingAccounts() {
    return [{ pubkey: globalConfigPda()[0], isSigner: false, isWritable: false }];
  }

  async fetchTournament(tournamentId = this.tournamentId): Promise<TournamentData | null> {
    const info = await this.connection.getAccountInfo(tournamentPda(tournamentId)[0]);
    return info ? decodeTournament(new Uint8Array(info.data)) : null;
  }

  async fetchGlobalConfig(): Promise<GlobalConfigData> {
    const info = await this.connection.getAccountInfo(globalConfigPda()[0]);
    if (!info) throw new SwarmClientError({ code: "INVALID_RESPONSE", operation: "solana.fetchGlobalConfig", message: "GlobalConfig account not found on-chain" });
    return decodeGlobalConfig(new Uint8Array(info.data));
  }

  async buildCreatePlayerSessionInstruction(sessionPublicKey: PublicKey): Promise<TransactionInstruction> {
    const wallet = this.requireWallet();
    return this.program(wallet).methods.createPlayerSession().accountsPartial({ sessionKey: sessionPublicKey }).instruction();
  }

  async depositStake(tournament: PublicKey, session?: Keypair | null): Promise<string> {
    const wallet = this.requireWallet();
    const escrow = escrowPda(this.tournamentId, wallet.publicKey)[0];
    if (session) {
      return this.sessionProgram(session).methods.depositStakeSession().accountsPartial({
        player: wallet.publicKey,
        sessionAuthority: playerSessionPda(wallet.publicKey, session.publicKey)[0],
        tournament,
        escrow,
      }).remainingAccounts(this.liveStakeRemainingAccounts()).rpc();
    }
    return this.program(wallet).methods.depositStake().accountsPartial({ tournament, escrow }).remainingAccounts(this.liveStakeRemainingAccounts()).rpc();
  }

  async buildCreateGameTransaction(input: {
    stake: BN;
    matchupCommitment: number[];
    tournament: PublicKey;
    escrow: PublicKey;
    globalConfig: PublicKey;
    matchmaker: PublicKey;
    playerProfile: PublicKey;
    gameCounter: PublicKey;
    game: PublicKey;
    session?: Keypair;
    player?: PublicKey;
  }): Promise<Transaction> {
    const wallet = this.requireWallet();
    if (input.session) {
      const player = input.player ?? wallet.publicKey;
      return this.sessionProgram(input.session).methods.createGameSession(input.stake, input.matchupCommitment).accountsPartial({
        game: input.game,
        gameCounter: input.gameCounter,
        playerProfile: input.playerProfile,
        escrow: input.escrow,
        tournament: input.tournament,
        globalConfig: input.globalConfig,
        matchmaker: input.matchmaker,
        player,
        sessionAuthority: playerSessionPda(player, input.session.publicKey)[0],
        sessionSigner: input.session.publicKey,
        systemProgram: SystemProgram.programId,
      }).transaction();
    }
    return this.program(wallet).methods.createGame(input.stake, input.matchupCommitment).accountsPartial({
      game: input.game,
      gameCounter: input.gameCounter,
      playerProfile: input.playerProfile,
      escrow: input.escrow,
      tournament: input.tournament,
      globalConfig: input.globalConfig,
      matchmaker: input.matchmaker,
      player: wallet.publicKey,
      systemProgram: SystemProgram.programId,
    }).transaction();
  }

  async buildJoinGameTransaction(input: {
    game: PublicKey;
    tournament: PublicKey;
    escrow: PublicKey;
    preInstructions?: TransactionInstruction[];
    session?: Keypair;
  }): Promise<{ transaction: Transaction; payer: PublicKey; matchmaker: PublicKey }> {
    const wallet = this.requireWallet();
    const config = await this.fetchGlobalConfig();
    const globalConfig = globalConfigPda()[0];
    const program = input.session ? this.sessionProgram(input.session) : this.program(wallet);
    const builder = input.session
      ? program.methods.joinGameSession().accountsPartial({
          player: wallet.publicKey,
          sessionAuthority: playerSessionPda(wallet.publicKey, input.session.publicKey)[0],
          game: input.game,
          tournament: input.tournament,
          escrow: input.escrow,
          globalConfig,
          matchmaker: config.matchmaker,
        })
      : program.methods.joinGame().accountsPartial({
          game: input.game,
          tournament: input.tournament,
          escrow: input.escrow,
          globalConfig,
          matchmaker: config.matchmaker,
        });
    const transaction = await builder.preInstructions(input.preInstructions ?? []).transaction();
    const latest = await this.connection.getLatestBlockhash(this.commitment);
    transaction.recentBlockhash = latest.blockhash;
    transaction.lastValidBlockHeight = latest.lastValidBlockHeight;
    const payer = input.session?.publicKey ?? wallet.publicKey;
    transaction.feePayer = payer;
    transaction.signatures = [
      { publicKey: payer, signature: null },
      { publicKey: config.matchmaker, signature: null },
    ];
    return { transaction, payer, matchmaker: config.matchmaker };
  }

  async applyCosignature(transaction: Transaction, signer: PublicKey, signatureBase64: string): Promise<Transaction> {
    const index = transaction.signatures.findIndex((entry) => entry.publicKey.equals(signer));
    if (index < 0) throw new SwarmClientError({ code: "TRANSACTION_MISMATCH", operation: "solana.applyCosignature", message: "Cosigner is not a required signer" });
    transaction.signatures[index] = { publicKey: signer, signature: Buffer.from(signatureBase64, "base64") };
    return transaction;
  }

  messageBase64(transaction: Transaction): string {
    return Buffer.from(transaction.serializeMessage()).toString("base64");
  }

  async signBroadcastAndConfirm(transaction: Transaction, session?: Keypair): Promise<string> {
    const wallet = this.requireWallet();
    let signed: Transaction;
    if (session) {
      transaction.partialSign(session);
      signed = transaction;
    } else {
      signed = await wallet.signTransaction(transaction);
    }
    const signature = await this.connection.sendRawTransaction(signed.serialize());
    await this.connection.confirmTransaction(signature, this.commitment);
    return signature;
  }

  async commitGuess(commitment: number[], game: PublicKey, session?: Keypair | null): Promise<string> {
    const wallet = this.requireWallet();
    if (session) return this.sessionProgram(session).methods.commitGuessSession(commitment).accountsPartial({ player: wallet.publicKey, sessionAuthority: playerSessionPda(wallet.publicKey, session.publicKey)[0], game }).rpc();
    return this.program(wallet).methods.commitGuess(commitment).accountsPartial({ game }).rpc();
  }

  async revealGuess(r: number[], rMatchup: number[] | null, accounts: Record<string, PublicKey>, session?: Keypair | null): Promise<string> {
    const wallet = this.requireWallet();
    if (session) return this.sessionProgram(session).methods.revealGuessSession(r, rMatchup).accountsPartial({ ...accounts, player: wallet.publicKey, sessionAuthority: playerSessionPda(wallet.publicKey, session.publicKey)[0] } as never).rpc();
    return this.program(wallet).methods.revealGuess(r, rMatchup).accountsPartial(accounts as never).rpc();
  }

  async closeGame(game: PublicKey, session?: Keypair | null): Promise<string> {
    const wallet = this.requireWallet();
    if (session) return this.sessionProgram(session).methods.closeGame().accountsPartial({ game, caller: session.publicKey }).rpc();
    return this.program(wallet).methods.closeGame().accountsPartial({ game }).rpc();
  }

  async resolveTimeout(gameId: bigint, session?: Keypair | null): Promise<string> {
    const wallet = this.requireWallet();
    const program = session ? this.sessionProgram(session) : this.program(wallet);
    const gameKey = gamePda(gameId)[0];
    const game = await program.account.game.fetch(gameKey);
    const config = await this.fetchGlobalConfig();
    const tournamentId = BigInt(game.tournamentId.toString());
    return program.methods.resolveTimeout().accountsPartial({
      game: gameKey,
      p1Profile: playerProfilePda(tournamentId, game.playerOne)[0],
      p2Profile: playerProfilePda(tournamentId, game.playerTwo)[0],
      tournament: tournamentPda(tournamentId)[0],
      globalConfig: globalConfigPda()[0],
      treasury: config.treasury,
      playerOneWallet: game.playerOne,
      playerTwoWallet: game.playerTwo,
      caller: session?.publicKey ?? wallet.publicKey,
    }).rpc();
  }

  async closeSessionByDelegate(session: Keypair, player: PublicKey): Promise<string> {
    return this.sessionProgram(session).methods.closeSessionByDelegate().accountsPartial({
      sessionAuthority: playerSessionPda(player, session.publicKey)[0],
      sessionSigner: session.publicKey,
    }).rpc();
  }

  deriveAddresses(gameId: bigint, player: PublicKey) {
    return {
      globalConfig: globalConfigPda()[0],
      gameCounter: gameCounterPda()[0],
      game: gamePda(gameId)[0],
      tournament: tournamentPda(this.tournamentId)[0],
      playerProfile: playerProfilePda(this.tournamentId, player)[0],
      escrow: escrowPda(this.tournamentId, player)[0],
    };
  }
}
