import * as anchor from "@coral-xyz/anchor";
import type { Idl } from "@coral-xyz/anchor";
import { PublicKey, TransactionInstruction } from "@solana/web3.js";
import { Buffer } from "buffer";
import coordinationGameIdl from "./idl/coordination_game.json" with { type: "json" };
import shillbotIdl from "./idl/shillbot.json" with { type: "json" };

const anchorRuntime =
  (anchor as typeof anchor & { default?: typeof anchor }).default ?? anchor;
const { BN, BorshInstructionCoder } = anchorRuntime;

export type { CoordinationGame } from "./generated/coordination_game.js";
export type { Shillbot } from "./generated/shillbot.js";

/** Canonical raw Anchor IDLs used for runtime instruction encoding. */
export const COORDINATION_GAME_IDL = coordinationGameIdl as unknown as Idl;
export const SHILLBOT_IDL = shillbotIdl as unknown as Idl;

export const COORDINATION_GAME_PROGRAM_ID = new PublicKey(
  COORDINATION_GAME_IDL.address
);
export const SHILLBOT_PROGRAM_ID = new PublicKey(SHILLBOT_IDL.address);

export { BN };

export type PublicKeyInput = PublicKey | string;

export interface BuildShillbotInstructionRequest {
  /** Snake-case instruction name from the canonical raw Anchor IDL. */
  name: string;
  /** Account names from the IDL mapped to their concrete public keys. */
  accounts: Readonly<Record<string, PublicKeyInput>>;
  /** Anchor/Borsh arguments. Integer values wider than u32 must be BN values. */
  args?: Readonly<Record<string, unknown>>;
}

type IdlAccountItem = {
  name: string;
  writable?: boolean;
  signer?: boolean;
  address?: string;
  accounts?: IdlAccountItem[];
};

const shillbotCoder = new BorshInstructionCoder(SHILLBOT_IDL);

export function shillbotInstructionDiscriminator(name: string): Buffer {
  const definition = SHILLBOT_IDL.instructions.find(
    (instruction) => instruction.name === name
  );
  if (!definition) throw new Error(`unknown Shillbot instruction: ${name}`);
  return Buffer.from(definition.discriminator);
}

function flattenAccounts(items: readonly IdlAccountItem[]): IdlAccountItem[] {
  return items.flatMap((item) =>
    item.accounts ? flattenAccounts(item.accounts) : [item]
  );
}

/**
 * Encode a Shillbot instruction using the generated IDL as the ABI authority.
 * Account order, signer/writable flags, discriminators, and argument encoding
 * therefore cannot drift independently in downstream clients.
 */
export function buildShillbotInstruction(
  request: BuildShillbotInstructionRequest
): TransactionInstruction {
  const definition = SHILLBOT_IDL.instructions.find(
    (instruction) => instruction.name === request.name
  );
  if (!definition) {
    throw new Error(`unknown Shillbot instruction: ${request.name}`);
  }

  const accountDefinitions = flattenAccounts(
    definition.accounts as IdlAccountItem[]
  );
  const knownAccounts = new Set(
    accountDefinitions.map((account) => account.name)
  );
  const unknownAccount = Object.keys(request.accounts).find(
    (name) => !knownAccounts.has(name)
  );
  if (unknownAccount) {
    throw new Error(
      `unknown account ${unknownAccount} for Shillbot instruction ${request.name}`
    );
  }

  const expectedArguments = new Set(
    definition.args.map((argument) => argument.name)
  );
  const suppliedArguments = Object.keys(request.args ?? {});
  const unknownArgument = suppliedArguments.find(
    (name) => !expectedArguments.has(name)
  );
  if (unknownArgument) {
    throw new Error(
      `unknown argument ${unknownArgument} for Shillbot instruction ${request.name}`
    );
  }
  const missingArgument = definition.args.find(
    (argument) => !(argument.name in (request.args ?? {}))
  );
  if (missingArgument) {
    throw new Error(
      `missing argument ${missingArgument.name} for Shillbot instruction ${request.name}`
    );
  }

  const keys = accountDefinitions.map((account) => {
    const supplied = request.accounts[account.name];
    const value = supplied ?? account.address;
    if (!value) {
      throw new Error(
        `missing account ${account.name} for Shillbot instruction ${request.name}`
      );
    }
    return {
      pubkey: new PublicKey(value),
      isSigner: account.signer === true,
      isWritable: account.writable === true,
    };
  });

  return new TransactionInstruction({
    programId: SHILLBOT_PROGRAM_ID,
    keys,
    data: shillbotCoder.encode(request.name, request.args ?? {}),
  });
}

function u64(value: bigint): Buffer {
  const out = Buffer.alloc(8);
  out.writeBigUInt64LE(value);
  return out;
}

export function globalStatePda(): PublicKey {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("shillbot_global")],
    SHILLBOT_PROGRAM_ID
  )[0];
}

export function taskPda(nonce: bigint, client: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("task"), u64(nonce), client.toBuffer()],
    SHILLBOT_PROGRAM_ID
  )[0];
}

export function clientStatePda(client: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("client_state"), client.toBuffer()],
    SHILLBOT_PROGRAM_ID
  )[0];
}

export function agentStatePda(agent: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("agent_state"), agent.toBuffer()],
    SHILLBOT_PROGRAM_ID
  )[0];
}
