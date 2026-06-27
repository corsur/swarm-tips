// In-process SolanaRuntime backed by anchor-bankrun (no validator, no network,
// CI-safe). It owns the clock, so it implements warpTo — the capability deadline
// / claim-window scenarios need. Mirrors the setup in tests/shillbot-lifecycle.ts.

import { startAnchor, BankrunProvider } from "anchor-bankrun";
import { Program } from "@coral-xyz/anchor";
import { Clock } from "solana-bankrun";
import { PublicKey, SystemProgram, Transaction } from "@solana/web3.js";
import { CoordinationGame } from "../../target/types/coordination_game";
import { SolanaRuntime } from "./target";

const IDL = require("../../target/idl/coordination_game.json");

export interface BankrunHandle {
  runtime: SolanaRuntime;
  context: Awaited<ReturnType<typeof startAnchor>>;
  provider: BankrunProvider;
}

/** Boot a fresh in-process Solana VM with the coordination-game program loaded. */
export async function startBankrun(): Promise<BankrunHandle> {
  const context = await startAnchor(".", [], []);
  const provider = new BankrunProvider(context);
  const program = new Program<CoordinationGame>(IDL, provider);

  const runtime: SolanaRuntime = {
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

  return { runtime, context, provider };
}
