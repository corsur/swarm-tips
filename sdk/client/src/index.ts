/** The unified client package version, synchronized from package.json. */
export { CLIENT_VERSION } from "./version.js";
export const CLIENT_MANIFEST_SCHEMA = "swarm.client-manifest/v1" as const;

export { SwarmClientError } from "./errors.js";
export type { SwarmClientErrorCode, SwarmClientErrorOptions } from "./errors.js";

export {
  DEFAULT_MAX_NETWORK_FEE_WEI,
  SwarmAgent,
  createSwarmAgent,
  extractCreateEscrowLamports,
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
  type EvmTransactionSigner,
  type EvmWalletClientLike,
  type SolanaSwarmSigner,
  type SolanaWalletAdapterLike,
  type SwarmSigner,
} from "./signer/index.js";

export {
  MAX_COMPUTE_UNIT_LIMIT,
  MAX_COMPUTE_UNIT_PRICE_MICROLAMPORTS,
  SHILLBOT_ESCROW_DEPLOYMENTS,
  SHILLBOT_EVM_SELECTORS,
  SHILLBOT_INSTRUCTION_SPECS,
  buildOnChainActionProofRef,
  estimateSolanaNetworkFeeLamports,
  formatOnChainActionAttributionMessage,
  normalizeEvmChainId,
  validateComputeBudgetInstruction,
  validateComputeBudgetInstructions,
  validatePreparedEvmCall,
  validatePreparedTransaction,
  verifyBuiltTransaction,
  type AuthoritySlotRule,
  type BuildOnChainActionProofRefParams,
  type BuildVerifyOptions,
  type PreparedEvmCallExpectation,
  type PreparedTransactionExpectation,
  type PreparedTransactionResponse,
  type ShillbotInstructionName,
  type ShillbotInstructionSpec,
  type ValidatedEvmCall,
  type VerificationDataResponse,
  type VerifyBuiltTransactionExpectation,
} from "./shillbot/api.js";

export {
  VALID_TOPICS,
  isValidTopicId,
  type TopicId,
} from "./inbox/types.js";

export {
  DEFAULT_MAX_VOW_BYTES,
  fetchVowAttestationJson,
  validateVowUrl,
  verifyVow,
  type FetchVowOptions,
  type VerifyVowOptions,
} from "./vow/verify.js";

export {
  OPENAPI_SERVERS,
  OPENAPI_VERSION,
  type OpenApiOperationId,
  type OpenApiOperations,
  type OpenApiPaths,
  type OpenApiSchemas,
  type OpenApiServerUrl,
} from "./generated/openapi.js";
