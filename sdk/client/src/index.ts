/** The unified client package version, synchronized from package.json. */
export { CLIENT_VERSION } from "./version.js";
export const CLIENT_MANIFEST_SCHEMA = "swarm.client-manifest/v1" as const;

export { SwarmClientError } from "./errors.js";
export type { SwarmClientErrorCode, SwarmClientErrorOptions } from "./errors.js";

export {
  SwarmAgent,
  createSwarmAgent,
  type ExecuteTaskOptions,
  type ExecutedTaskResult,
  type NextTaskAction,
  type SwarmAgentOptions,
  type SwarmNetwork,
  type TaskLifecycleStep,
} from "./agent.js";

export {
  WalletTokenManager,
  type TokenProvider,
  type WalletTokenManagerOptions,
} from "./auth/token-manager.js";

export {
  fromEvmPrivateKey,
  fromEvmWalletClient,
  fromSolanaKeypair,
  fromSolanaSecretKey,
  fromSolanaWalletAdapter,
  redactSigner,
  type EvmCallRequest,
  type EvmPrivateKeyOptions,
  type EvmSwarmSigner,
  type EvmWalletClientLike,
  type SolanaSwarmSigner,
  type SolanaWalletAdapterLike,
  type SwarmSigner,
} from "./signer/index.js";

export {
  SHILLBOT_ESCROW_DEPLOYMENTS,
  SHILLBOT_EVM_SELECTORS,
  buildOnChainActionProofRef,
  estimateSolanaNetworkFeeLamports,
  formatOnChainActionAttributionMessage,
  validatePreparedEvmCall,
  validatePreparedTransaction,
  type BuildOnChainActionProofRefParams,
  type BuildVerifyOptions,
  type PreparedEvmCallExpectation,
  type PreparedTransactionExpectation,
  type PreparedTransactionResponse,
  type ValidatedEvmCall,
  type VerificationDataResponse,
} from "./shillbot/api.js";

export {
  OPENAPI_SERVERS,
  OPENAPI_VERSION,
  type OpenApiOperationId,
  type OpenApiOperations,
  type OpenApiPaths,
  type OpenApiSchemas,
  type OpenApiServerUrl,
} from "./generated/openapi.js";
