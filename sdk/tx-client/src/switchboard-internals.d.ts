declare module "@switchboard-xyz/on-demand/dist/esm/instruction-utils/secp256k1-instruction-utils.js" {
  import type { TransactionInstruction } from "@solana/web3.js";

  interface Secp256k1Signature {
    ethAddress: Buffer;
    signature: Buffer;
    message: Buffer;
    recoveryId: number;
    oracleIdx: number;
  }

  export class Secp256k1InstructionUtils {
    static buildSecp256k1Instruction(
      signatures: Secp256k1Signature[],
      instructionIndex: number
    ): TransactionInstruction;
  }
}
