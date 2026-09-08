/** The unified client package version. Functional APIs live in typed subpaths. */
export const CLIENT_VERSION = "0.2.0" as const;
export const CLIENT_MANIFEST_SCHEMA = "swarm.client-manifest/v1" as const;
export { SwarmClientError } from "./errors.js";
export type { SwarmClientErrorCode, SwarmClientErrorOptions } from "./errors.js";
