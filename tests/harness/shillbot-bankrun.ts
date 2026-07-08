// In-process SolanaRuntime<Shillbot> backed by anchor-bankrun (no validator, no
// network, CI-safe). Mirrors bankrun.ts but loads the shillbot program and adds
// the one capability the game runtime doesn't need: priming a mock Switchboard
// pull-feed account so a kind-0 `verify_task` reads a chosen score. Kind-1
// (attested) verification needs no feed — the attester signs directly.

import { startAnchor, BankrunProvider } from "anchor-bankrun";
import { Program } from "@coral-xyz/anchor";
import { Clock } from "solana-bankrun";
import {
  LAMPORTS_PER_SOL,
  PublicKey,
  SystemProgram,
  Transaction,
} from "@solana/web3.js";
import { Shillbot } from "../../target/types/shillbot";
import { SolanaRuntime } from "./target";
import { createMockPullFeedData } from "../helpers/mock-switchboard-feed";
import {
  DUMMY_SWITCHBOARD_FEED,
  SWITCHBOARD_PROGRAM_ID,
} from "./shillbot-steps";

const IDL = require("../../target/idl/shillbot.json");

export interface ShillbotBankrunHandle {
  runtime: SolanaRuntime<Shillbot>;
  context: Awaited<ReturnType<typeof startAnchor>>;
  provider: BankrunProvider;
  /** Prime the mock Switchboard feed so the next kind-0 verify reads `score`. */
  primeFeed(score: number): Promise<void>;
}

/** Boot a fresh in-process Solana VM with the shillbot program loaded. */
export async function startShillbotBankrun(): Promise<ShillbotBankrunHandle> {
  const context = await startAnchor(".", [], []);
  const provider = new BankrunProvider(context);
  const program = new Program<Shillbot>(IDL, provider);

  const runtime: SolanaRuntime<Shillbot> = {
    program,
    payer: provider.wallet.publicKey,

    pda(seeds) {
      return PublicKey.findProgramAddressSync(seeds, program.programId)[0];
    },

    async getBalance(pk) {
      const acct = await context.banksClient.getAccount(pk);
      return acct === null ? 0n : BigInt(acct.lamports);
    },

    async now() {
      const c = await context.banksClient.getClock();
      return Number(c.unixTimestamp);
    },

    async warpTo(ts) {
      const c = await context.banksClient.getClock();
      context.setClock(
        new Clock(
          c.slot,
          c.epochStartTimestamp,
          c.epoch,
          c.leaderScheduleEpoch,
          BigInt(ts)
        )
      );
    },

    async warpBySlots(slots) {
      const c = await context.banksClient.getClock();
      context.setClock(
        new Clock(
          c.slot + BigInt(slots),
          c.epochStartTimestamp,
          c.epoch,
          c.leaderScheduleEpoch,
          c.unixTimestamp
        )
      );
    },

    async fund(recipient, lamports) {
      const tx = new Transaction().add(
        SystemProgram.transfer({
          fromPubkey: provider.wallet.publicKey,
          toPubkey: recipient,
          lamports: Number(lamports),
        })
      );
      await provider.sendAndConfirm(tx);
    },
  };

  async function primeFeed(score: number): Promise<void> {
    const clock = await context.banksClient.getClock();
    context.setAccount(DUMMY_SWITCHBOARD_FEED, {
      lamports: LAMPORTS_PER_SOL,
      data: createMockPullFeedData(score, clock.slot),
      owner: SWITCHBOARD_PROGRAM_ID,
      executable: false,
    });
  }

  return { runtime, context, provider, primeFeed };
}
