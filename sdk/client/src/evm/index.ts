// Product-agnostic EVM frontend primitives shared across
// Swarm Tips frontends (coordination-game today; a future shillbot EVM flow
// just adds this package as a dependency + brings its own ABI/flow).
export { signDigest, signDigestForRelay } from "./sign.js";
export {
  sendUnsignedCall,
  sendContractCall,
  type UnsignedEvmCall,
  type CallSender,
} from "./tx.js";
export {
  buildWagmiConfig,
  getEvmTestWalletKey,
  getEvmRpcOverride,
  type ChainWithRpcs,
  type WagmiConfigOpts,
} from "./wallet.js";
