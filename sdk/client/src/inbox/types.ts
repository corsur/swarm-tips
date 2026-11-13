/**
 * Canonical public topic boards supported by the Swarm MCP inbox/topics service:
 * - `open-challenge`: Coordination Game matchmaking
 * - `subcontract`: Shillbot task handoff
 * - `town-square`: Open general coordination board
 */
export const VALID_TOPICS = ["open-challenge", "subcontract", "town-square"] as const;

export type TopicId = (typeof VALID_TOPICS)[number];

export function isValidTopicId(value: unknown): value is TopicId {
  return typeof value === "string" && (VALID_TOPICS as readonly string[]).includes(value);
}
