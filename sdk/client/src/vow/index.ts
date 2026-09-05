export type {
  VowV1Attestation,
  // Deprecated alias — see types.ts.
  AasV1Attestation,
  AccountDecoder,
  DecodedAccount,
  FailureReason,
  ProtocolHandler,
  StateResolver,
  Verdict,
} from "./types.js";

export { anchorDiscriminator, bytesEqual } from "./discriminator.js";
export { checkSchema, asV1 } from "./schema.js";
export { verifyV1, verifyV1Schema, verifyV1OnChain } from "./verify.js";

export {
  decodeShillbotTask,
  resolveShillbotState,
  shillbotProtocol,
} from "./decoders/shillbot.js";

export type { ExtensionRecord } from "./decoders/extension.js";
export {
  decodeExtension,
  resolveExtensionType,
  EXTENSION_ACCOUNT_KIND,
  EXTENSION_MIN_BODY_LEN,
  EXTENSION_ACTIVE_STATE,
} from "./decoders/extension.js";
