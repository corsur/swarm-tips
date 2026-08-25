//! Google-A2A (Agent2Agent) JSON-RPC facade — **translation only**.
//!
//! The live agent inbox (`src/inbox.rs`) is a wallet-addressed store-and-forward
//! mailbox. This module maps the A2A JSON-RPC wire shape onto the SAME inbox
//! ops the MCP tools and the `/internal/inbox/*` REST twins call — so every
//! quota, bound, TTL, tier limit, and `agent_message_*` event fires identically.
//! Per decision.md condition 7 **no A2A envelope is ever persisted**: A2A JSON
//! is decoded on the way in and re-encoded on the way out; Firestore only ever
//! sees the inbox's own `InboxMessageDoc` / `WebhookDoc`.
//!
//! ```text
//!   A2A client (JSON-RPC over HTTP, X-Inbox-Session header)
//!        │
//!        ▼  POST /a2a
//!   ┌─────────────────────────────────────────────────────────────┐
//!   │ message/send  → inbox::send_message                           │
//!   │   message.parts[text]        → body                           │
//!   │   message.metadata.to_wallet → to_wallet (A2A has no in-band  │
//!   │                                recipient for a relay server)  │
//!   │   message.contextId          → thread_id                      │
//!   │   message.metadata.intent    → intent                         │
//!   │   ← SendReceipt              → Task{state:"completed"}         │
//!   │                                                               │
//!   │ tasks/get     → inbox::get_messages(thread=id, include_sent)  │
//!   │   params.id                  → thread_id (an A2A Task == a     │
//!   │                                swarm inbox thread/context)    │
//!   │   ← ReadPage                 → Task{history:[Message…]}        │
//!   │     direction "sent"→role "user", "received"→role "agent"     │
//!   │                                                               │
//!   │ tasks/pushNotificationConfig/set → inbox::register_webhook    │
//!   │   pushNotificationConfig.url  → webhook url (SSRF + handshake) │
//!   │   ← WebhookDoc               → TaskPushNotificationConfig      │
//!   │     A2A static `token` is superseded by per-delivery HMAC;    │
//!   │     the minted hmac_secret is returned AS the config token.   │
//!   └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! This module is pure: no I/O, no session state. The thin async handlers live
//! in `inbox_http.rs` (they own auth via `require_verified_mailbox` and the
//! `inbox::` op calls); everything here is unit-tested without Firestore.

use crate::inbox::{MessageOut, ReadPage, SendReceipt, WebhookDoc};
use serde_json::{json, Value};

// JSON-RPC 2.0 error codes (A2A uses the standard set for envelope errors).
pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const INTERNAL_ERROR: i64 = -32603;

/// A parsed, well-formed JSON-RPC 2.0 request. `id` is echoed verbatim into the
/// response envelope (JSON-RPC lets it be a string, number, or null).
#[derive(Debug)]
pub struct JsonRpcRequest {
    pub id: Value,
    pub method: String,
    pub params: Value,
}

/// Decode the JSON-RPC envelope. Errors here map to HTTP 400 at the handler —
/// the "malformed A2A envelope" boundary rejection. Reject rather than guess:
/// a missing `jsonrpc`/`method` is a client bug, not something to coerce.
pub fn parse_request(raw: &str) -> Result<JsonRpcRequest, (i64, String)> {
    let value: Value =
        serde_json::from_str(raw).map_err(|e| (PARSE_ERROR, format!("invalid JSON: {e}")))?;
    let obj = value
        .as_object()
        .ok_or((INVALID_REQUEST, "request must be a JSON object".to_string()))?;
    match obj.get("jsonrpc").and_then(Value::as_str) {
        Some("2.0") => {}
        _ => return Err((INVALID_REQUEST, "jsonrpc must be \"2.0\"".to_string())),
    }
    let method = obj
        .get("method")
        .and_then(Value::as_str)
        .filter(|m| !m.is_empty())
        .ok_or((INVALID_REQUEST, "method is required".to_string()))?
        .to_string();
    Ok(JsonRpcRequest {
        id: obj.get("id").cloned().unwrap_or(Value::Null),
        method,
        params: obj.get("params").cloned().unwrap_or(Value::Null),
    })
}

pub fn result_envelope(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

pub fn error_envelope(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

// ---------------------------------------------------------------------------
// message/send  ↔  inbox::send_message
// ---------------------------------------------------------------------------

/// The inbox-send fields extracted from an A2A `message/send` params object.
/// Shape-only: recipient/thread/intent/body semantics are (re)validated by
/// `inbox::send_message`, so the A2A path rejects identically to the twins.
#[derive(Debug, PartialEq, Eq)]
pub struct MessageSend {
    pub to_wallet: String,
    pub body: String,
    pub thread_id: Option<String>,
    pub intent: Option<String>,
    pub message_id: Option<String>,
}

fn nonempty_str(v: Option<&Value>) -> Option<String> {
    v.and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Concatenate the text of every `{"kind":"text","text":…}` part with newlines.
/// Non-text parts (data/file) carry no inbox body and are skipped; a message
/// with no text yields an empty body, which `send_message` rejects with the
/// same `empty_body` reason the MCP tool logs.
fn text_from_parts(parts: Option<&Value>) -> Result<String, String> {
    let arr = parts
        .and_then(Value::as_array)
        .ok_or("message.parts must be an array")?;
    let mut out = String::new();
    for part in arr {
        if part.get("kind").and_then(Value::as_str) != Some("text") {
            continue;
        }
        let text = part
            .get("text")
            .and_then(Value::as_str)
            .ok_or("a text part requires a string 'text' field")?;
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(text);
    }
    Ok(out)
}

pub fn map_message_send(params: &Value) -> Result<MessageSend, String> {
    let msg = params
        .get("message")
        .filter(|m| m.is_object())
        .ok_or("params.message is required (an A2A Message object)")?;
    let metadata = msg.get("metadata");
    // A2A carries no in-band recipient when the peer is a relay/inbox server —
    // the "target" is only the URL. The facade reads the wallet the message is
    // addressed to from message.metadata.to_wallet (a metadata extension).
    let to_wallet = nonempty_str(metadata.and_then(|m| m.get("to_wallet"))).ok_or(
        "message.metadata.to_wallet is required — the A2A facade routes to a wallet-addressed mailbox",
    )?;
    let body = text_from_parts(msg.get("parts"))?;
    Ok(MessageSend {
        to_wallet,
        body,
        thread_id: nonempty_str(msg.get("contextId")),
        intent: nonempty_str(metadata.and_then(|m| m.get("intent"))),
        message_id: nonempty_str(msg.get("messageId")),
    })
}

/// A `send_message` receipt as an A2A Task. Store-and-forward delivery is
/// synchronous and terminal, so the task is born `completed`.
pub fn send_receipt_to_task(receipt: &SendReceipt) -> Value {
    json!({
        "kind": "task",
        "id": receipt.msg_id,
        "contextId": receipt.thread_id,
        "status": { "state": "completed" },
        "metadata": {
            "to_wallet": receipt.to,
            "intent": receipt.intent,
            "expires_at": receipt.expires_at.to_rfc3339(),
            "sends_remaining_today": receipt.sends_remaining_today,
        },
    })
}

// ---------------------------------------------------------------------------
// tasks/get  ↔  inbox::get_messages   (an A2A Task == a swarm inbox thread)
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, Eq)]
pub struct TaskGet {
    pub thread_id: String,
    pub history_length: Option<u32>,
}

pub fn map_tasks_get(params: &Value) -> Result<TaskGet, String> {
    let thread_id = nonempty_str(params.get("id"))
        .ok_or("params.id is required (maps to the inbox thread/context id)")?;
    let history_length = match params.get("historyLength") {
        None | Some(Value::Null) => None,
        Some(v) => {
            let n = v
                .as_u64()
                .ok_or("historyLength must be a non-negative integer")?;
            Some(u32::try_from(n).map_err(|_| "historyLength is too large".to_string())?)
        }
    };
    Ok(TaskGet {
        thread_id,
        history_length,
    })
}

/// A2A `role` is relative to the client: the session wallet is the "user";
/// every other wallet is an "agent". `include_sent` reads surface the sender's
/// own mirror with `direction:"sent"`, so this split reconstructs both sides.
fn message_out_to_a2a(m: &MessageOut) -> Value {
    let role = if m.direction == "sent" {
        "user"
    } else {
        "agent"
    };
    json!({
        "kind": "message",
        "role": role,
        "messageId": m.msg_id,
        "taskId": m.thread_id,
        "contextId": m.thread_id,
        "parts": [ { "kind": "text", "text": m.body } ],
        "metadata": {
            "from_wallet": m.from_wallet,
            "to_wallet": m.to_wallet,
            "intent": m.intent,
            "sent_at": m.sent_at,
            "direction": m.direction,
        },
    })
}

pub fn read_page_to_task(thread_id: &str, page: &ReadPage) -> Value {
    let history: Vec<Value> = page.messages.iter().map(message_out_to_a2a).collect();
    json!({
        "kind": "task",
        "id": thread_id,
        "contextId": thread_id,
        "status": { "state": "completed" },
        "history": history,
        "metadata": {
            "count": page.messages.len(),
            "next_cursor": page.next_cursor,
        },
    })
}

// ---------------------------------------------------------------------------
// tasks/pushNotificationConfig/set  ↔  inbox::register_webhook
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, Eq)]
pub struct PushConfigSet {
    pub url: String,
    pub token: Option<String>,
    pub task_id: Option<String>,
}

pub fn map_push_config_set(params: &Value) -> Result<PushConfigSet, String> {
    let cfg = params
        .get("pushNotificationConfig")
        .filter(|c| c.is_object())
        .ok_or("params.pushNotificationConfig is required")?;
    let url = nonempty_str(cfg.get("url")).ok_or("pushNotificationConfig.url is required")?;
    Ok(PushConfigSet {
        url,
        token: nonempty_str(cfg.get("token")),
        task_id: nonempty_str(params.get("taskId")),
    })
}

/// A registered webhook as an A2A TaskPushNotificationConfig. Our push channel
/// authenticates each delivery with a per-registration HMAC secret rather than
/// the client's static `token`, so the minted secret is returned AS the config
/// token and the HMAC scheme is spelled out in `authentication`.
pub fn webhook_to_push_config(task_id: Option<&str>, doc: &WebhookDoc) -> Value {
    json!({
        "taskId": task_id,
        "pushNotificationConfig": {
            "id": doc.wallet,
            "url": doc.url,
            "token": doc.hmac_secret,
            "authentication": {
                "schemes": ["HMAC-SHA256"],
                "description": "X-Swarm-Signature: sha256=<hex HMAC-SHA256 of the raw request body, keyed by token>; X-Swarm-Delivery-Id dedups redeliveries",
            },
        },
        "metadata": {
            "verified": doc.verified,
            "consecutive_failures": doc.consecutive_failures,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inbox::MessageOut;

    // -- envelope parsing (malformed → HTTP 400 at the handler) -------------

    #[test]
    fn parse_request_accepts_well_formed_and_defaults_id_params() {
        let req = parse_request(r#"{"jsonrpc":"2.0","id":"1","method":"tasks/get"}"#).expect("ok");
        assert_eq!(req.id, json!("1"));
        assert_eq!(req.method, "tasks/get");
        assert_eq!(req.params, Value::Null);

        // Numeric id and object params are preserved verbatim.
        let req =
            parse_request(r#"{"jsonrpc":"2.0","id":7,"method":"message/send","params":{"a":1}}"#)
                .expect("ok");
        assert_eq!(req.id, json!(7));
        assert_eq!(req.params, json!({"a":1}));
    }

    #[test]
    fn parse_request_rejects_malformed_envelopes_with_codes() {
        assert_eq!(parse_request("not json").unwrap_err().0, PARSE_ERROR);
        assert_eq!(parse_request("[]").unwrap_err().0, INVALID_REQUEST);
        assert_eq!(
            parse_request(r#"{"id":"1","method":"tasks/get"}"#)
                .unwrap_err()
                .0,
            INVALID_REQUEST,
            "missing jsonrpc"
        );
        assert_eq!(
            parse_request(r#"{"jsonrpc":"1.0","id":"1","method":"tasks/get"}"#)
                .unwrap_err()
                .0,
            INVALID_REQUEST,
            "wrong jsonrpc version"
        );
        assert_eq!(
            parse_request(r#"{"jsonrpc":"2.0","id":"1"}"#)
                .unwrap_err()
                .0,
            INVALID_REQUEST,
            "missing method"
        );
        assert_eq!(
            parse_request(r#"{"jsonrpc":"2.0","id":"1","method":""}"#)
                .unwrap_err()
                .0,
            INVALID_REQUEST,
            "empty method"
        );
    }

    #[test]
    fn envelopes_carry_id_and_shape() {
        let ok = result_envelope(&json!("1"), json!({"k": "v"}));
        assert_eq!(ok["jsonrpc"], "2.0");
        assert_eq!(ok["id"], "1");
        assert_eq!(ok["result"]["k"], "v");

        let err = error_envelope(&json!(2), INVALID_PARAMS, "bad");
        assert_eq!(err["id"], 2);
        assert_eq!(err["error"]["code"], INVALID_PARAMS);
        assert_eq!(err["error"]["message"], "bad");
        assert!(err.get("result").is_none());
    }

    // -- message/send round-trip -------------------------------------------

    #[test]
    fn map_message_send_extracts_inbox_fields() {
        let params = json!({
            "message": {
                "role": "user",
                "messageId": "m-1",
                "contextId": "task:42",
                "parts": [
                    { "kind": "text", "text": "line one" },
                    { "kind": "data", "data": { "ignored": true } },
                    { "kind": "text", "text": "line two" }
                ],
                "metadata": { "to_wallet": "CKsZ7ZMLLUzbHUeu2Vm5mjuB8QQi3vfvqvXFdFxT7xmY", "intent": "task_offer" }
            }
        });
        let mapped = map_message_send(&params).expect("ok");
        assert_eq!(
            mapped,
            MessageSend {
                to_wallet: "CKsZ7ZMLLUzbHUeu2Vm5mjuB8QQi3vfvqvXFdFxT7xmY".to_string(),
                body: "line one\nline two".to_string(),
                thread_id: Some("task:42".to_string()),
                intent: Some("task_offer".to_string()),
                message_id: Some("m-1".to_string()),
            }
        );
    }

    #[test]
    fn map_message_send_minimal_defaults_optionals_none() {
        let params = json!({
            "message": {
                "parts": [ { "kind": "text", "text": "hi" } ],
                "metadata": { "to_wallet": "0x996213ed4099707059B8B5D7489FFF23DAC9770D" }
            }
        });
        let mapped = map_message_send(&params).expect("ok");
        assert_eq!(mapped.body, "hi");
        assert!(
            mapped.thread_id.is_none() && mapped.intent.is_none() && mapped.message_id.is_none()
        );
    }

    #[test]
    fn map_message_send_rejects_missing_recipient_and_missing_parts() {
        // No metadata.to_wallet — a relay message with no addressee.
        let err = map_message_send(&json!({
            "message": { "parts": [ { "kind": "text", "text": "hi" } ] }
        }))
        .expect_err("no recipient");
        assert!(err.contains("to_wallet"), "{err}");

        // No message object at all.
        assert!(map_message_send(&json!({})).is_err());
        // parts not an array.
        assert!(map_message_send(&json!({
            "message": { "parts": "hi", "metadata": { "to_wallet": "w" } }
        }))
        .is_err());
        // text part missing its text field.
        assert!(map_message_send(&json!({
            "message": { "parts": [ { "kind": "text" } ], "metadata": { "to_wallet": "w" } }
        }))
        .is_err());
    }

    #[test]
    fn send_receipt_to_task_is_completed_task() {
        let receipt = SendReceipt {
            msg_id: "00000000000000001234_abcd0001".to_string(),
            to: "solana:x:CKsZ".to_string(),
            thread_id: "dm:a|b".to_string(),
            intent: Some("task_offer".to_string()),
            bytes: 2,
            expires_at: chrono::Utc::now(),
            sends_remaining_today: 41,
        };
        let task = send_receipt_to_task(&receipt);
        assert_eq!(task["kind"], "task");
        assert_eq!(task["id"], receipt.msg_id);
        assert_eq!(task["contextId"], "dm:a|b");
        assert_eq!(task["status"]["state"], "completed");
        assert_eq!(task["metadata"]["to_wallet"], "solana:x:CKsZ");
        assert_eq!(task["metadata"]["sends_remaining_today"], 41);
    }

    // -- tasks/get read mapping --------------------------------------------

    #[test]
    fn map_tasks_get_parses_id_and_history_length() {
        let g = map_tasks_get(&json!({ "id": "task:42", "historyLength": 10 })).expect("ok");
        assert_eq!(
            g,
            TaskGet {
                thread_id: "task:42".to_string(),
                history_length: Some(10),
            }
        );
        let g = map_tasks_get(&json!({ "id": "dm:a|b" })).expect("ok");
        assert!(g.history_length.is_none());

        assert!(map_tasks_get(&json!({})).is_err(), "id required");
        assert!(
            map_tasks_get(&json!({ "id": "t", "historyLength": -1 })).is_err(),
            "negative history"
        );
        assert!(
            map_tasks_get(&json!({ "id": "t", "historyLength": "10" })).is_err(),
            "non-integer history"
        );
    }

    #[test]
    fn read_page_to_task_maps_direction_to_role() {
        let page = ReadPage {
            messages: vec![
                MessageOut {
                    msg_id: "m1".to_string(),
                    from_wallet: "solana:x:ME".to_string(),
                    to_wallet: "solana:x:PEER".to_string(),
                    thread_id: "dm:me|peer".to_string(),
                    intent: Some("task_offer".to_string()),
                    body: "my offer".to_string(),
                    sent_at: "2026-08-25T00:00:00Z".to_string(),
                    seed: false,
                    direction: "sent".to_string(),
                },
                MessageOut {
                    msg_id: "m2".to_string(),
                    from_wallet: "solana:x:PEER".to_string(),
                    to_wallet: "solana:x:ME".to_string(),
                    thread_id: "dm:me|peer".to_string(),
                    intent: None,
                    body: "their reply".to_string(),
                    sent_at: "2026-08-25T00:01:00Z".to_string(),
                    seed: false,
                    direction: "received".to_string(),
                },
            ],
            next_cursor: Some("cursor-2".to_string()),
            fast_path: false,
            filtered_below_min_trust: 0,
            filtered_muted: 0,
        };
        let task = read_page_to_task("dm:me|peer", &page);
        assert_eq!(task["kind"], "task");
        assert_eq!(task["id"], "dm:me|peer");
        assert_eq!(task["metadata"]["count"], 2);
        assert_eq!(task["metadata"]["next_cursor"], "cursor-2");

        let history = task["history"].as_array().expect("history array");
        assert_eq!(history.len(), 2);
        // sender's own mirror → "user"; peer's message → "agent".
        assert_eq!(history[0]["role"], "user");
        assert_eq!(history[0]["parts"][0]["text"], "my offer");
        assert_eq!(history[0]["parts"][0]["kind"], "text");
        assert_eq!(history[1]["role"], "agent");
        assert_eq!(history[1]["messageId"], "m2");
        assert_eq!(history[1]["metadata"]["direction"], "received");
    }

    // -- pushNotificationConfig/set mapping --------------------------------

    #[test]
    fn map_push_config_set_extracts_url_token_taskid() {
        let p = map_push_config_set(&json!({
            "taskId": "task:42",
            "pushNotificationConfig": { "url": "https://a.example/hook", "token": "client-tok" }
        }))
        .expect("ok");
        assert_eq!(
            p,
            PushConfigSet {
                url: "https://a.example/hook".to_string(),
                token: Some("client-tok".to_string()),
                task_id: Some("task:42".to_string()),
            }
        );

        // url is the only required field.
        let p = map_push_config_set(&json!({
            "pushNotificationConfig": { "url": "https://b.example/h" }
        }))
        .expect("ok");
        assert!(p.token.is_none() && p.task_id.is_none());

        assert!(map_push_config_set(&json!({})).is_err(), "config required");
        assert!(
            map_push_config_set(&json!({ "pushNotificationConfig": {} })).is_err(),
            "url required"
        );
    }

    #[test]
    fn webhook_to_push_config_returns_hmac_as_token() {
        let doc = WebhookDoc {
            wallet: "solana:x:ME".to_string(),
            url: "https://a.example/hook".to_string(),
            hmac_secret: "deadbeefsecret".to_string(),
            challenge_token: "chal".to_string(),
            verified: true,
            consecutive_failures: 0,
            disabled_at: None,
            last_delivery_at: None,
            pending_delivery_id: String::new(),
            created_at: firestore::FirestoreTimestamp(chrono::Utc::now()),
        };
        let cfg = webhook_to_push_config(Some("task:42"), &doc);
        assert_eq!(cfg["taskId"], "task:42");
        assert_eq!(
            cfg["pushNotificationConfig"]["url"],
            "https://a.example/hook"
        );
        // A2A's static token slot carries our per-delivery HMAC secret.
        assert_eq!(cfg["pushNotificationConfig"]["token"], "deadbeefsecret");
        assert_eq!(
            cfg["pushNotificationConfig"]["authentication"]["schemes"][0],
            "HMAC-SHA256"
        );
        assert_eq!(cfg["metadata"]["verified"], true);
    }
}
