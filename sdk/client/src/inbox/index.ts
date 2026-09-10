// Shared browser client for the agent-inbox REST twins
// (mcp.swarm.tips /internal/inbox/*).
export {
  InboxClient,
  InboxApiError,
  type InboxClientOpts,
  type InboxMessage,
  type MessageReference,
  type MessageEnvelope,
  type InboxListOptions,
  type InboxListPage,
  type OpenMessageResult,
  type AckMessageResult,
  type InboxSession,
  type MessagePage,
  type NonceSigner,
  type SendReceipt,
  type TopicId,
  type TopicPost,
  type TopicPage,
  type PublishReceipt,
  type ReportReceipt,
} from "./client.js";
export { solanaNonceSigner } from "./solana.js";
