/**
 * Claim a player's tournament 1 reward using the merkle proof generated
 * by finalize-tournament-1.ts.
 *
 * Reads scripts/tournament-1-proofs.json, finds the entry matching the
 * caller's wallet, and submits claim_reward(amount, proof). Idempotent:
 * the on-chain `player_profile.claimed` flag prevents double-claim.
 *
 * Usage: ANCHOR_WALLET=~/.config/solana/id.json \
 *        ANCHOR_PROVIDER_URL=https://api.mainnet-beta.solana.com \
 *        npx ts-node scripts/claim-tournament-1-reward.ts
 */

import * as anchor from "@coral-xyz/anchor";
import { PublicKey } from "@solana/web3.js";
import * as fs from "fs";
import * as path from "path";

const idl = JSON.parse(
  fs.readFileSync(
    path.join(__dirname, "../target/idl/coordination_game.json"),
    "utf8",
  ),
);

const PROOFS_FILE = path.join(__dirname, "tournament-1-proofs.json");

async function main() {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = new anchor.Program(idl as anchor.Idl, provider);
  const wallet = provider.wallet.publicKey;

  console.log(`Wallet:  ${wallet.toBase58()}`);
  console.log();

  const proofs = JSON.parse(fs.readFileSync(PROOFS_FILE, "utf8"));
  const entry = proofs.claims.find(
    (c: { wallet: string }) => c.wallet === wallet.toBase58(),
  );
  if (!entry) {
    console.error(`No claim found for ${wallet.toBase58()}`);
    process.exit(1);
  }
  console.log(`Claim: ${entry.amount} lamports (${entry.amount / 1e9} SOL)`);
  console.log(`Proof depth: ${entry.proof.length}`);

  const idBuf = Buffer.alloc(8);
  idBuf.writeBigUInt64LE(BigInt(proofs.tournament_id));
  const [tournamentPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("tournament"), idBuf],
    program.programId,
  );
  const [playerProfilePda] = PublicKey.findProgramAddressSync(
    [Buffer.from("player"), idBuf, wallet.toBuffer()],
    program.programId,
  );

  const profile = await (program.account as any).playerProfile.fetch(
    playerProfilePda,
  );
  if (profile.claimed) {
    console.log("Already claimed. Done.");
    return;
  }

  const proofBytes = entry.proof.map((hex: string) =>
    Array.from(Buffer.from(hex, "hex")),
  );

  const balanceBefore = await provider.connection.getBalance(wallet);
  console.log(`Balance before: ${balanceBefore / 1e9} SOL`);

  const sig = await (program.methods as any)
    .claimReward(new anchor.BN(entry.amount), proofBytes)
    .accountsPartial({
      tournament: tournamentPda,
      playerProfile: playerProfilePda,
      player: wallet,
    })
    .rpc();

  console.log(`OK: signature=${sig}`);

  const balanceAfter = await provider.connection.getBalance(wallet);
  console.log(`Balance after: ${balanceAfter / 1e9} SOL`);
  console.log(`Net change: ${(balanceAfter - balanceBefore) / 1e9} SOL`);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
