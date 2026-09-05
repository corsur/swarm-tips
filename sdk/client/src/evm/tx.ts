import { encodeFunctionData, type Abi, type Hex } from "viem";

/** An unsigned EVM call the connected wallet signs + submits. */
export interface UnsignedEvmCall {
  to: Hex;
  data: Hex;
  /** Native value in wei; defaults to 0. */
  value?: bigint;
}

/**
 * Any wallet able to submit a prepared transaction — structurally satisfied by a
 * wagmi / viem `WalletClient` (whose `sendTransaction` accepts a superset of
 * these fields). Loosely typed on the argument so the kit doesn't depend on
 * wagmi's full generic `SendTransactionParameters`.
 */
export interface CallSender {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  sendTransaction: (args: any) => Promise<Hex>;
}

/** Submit an unsigned `{to,data,value}` call via the connected wallet. */
export function sendUnsignedCall(
  wallet: CallSender,
  call: UnsignedEvmCall
): Promise<Hex> {
  return wallet.sendTransaction({
    to: call.to,
    data: call.data,
    value: call.value ?? 0n,
  });
}

/** Encode a contract call (viem `encodeFunctionData`) and submit it. */
export function sendContractCall(
  wallet: CallSender,
  args: {
    abi: Abi;
    functionName: string;
    args: readonly unknown[];
    to: Hex;
    value?: bigint;
  }
): Promise<Hex> {
  const data = encodeFunctionData({
    abi: args.abi,
    functionName: args.functionName,
    args: args.args,
  });
  return sendUnsignedCall(wallet, { to: args.to, data, value: args.value });
}
