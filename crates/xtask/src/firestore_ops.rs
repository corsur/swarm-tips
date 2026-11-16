use crate::http::{
    gcloud_access_token, percent_encode, CurlTransport, HttpFailure, HttpRequest, HttpTransport,
};
use anyhow::{bail, ensure, Context, Result};
use base64::Engine as _;
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;

const DEFAULT_FIRESTORE_PROJECT: &str = "coordination-game-prod";
const DEFAULT_SOLANA_RPC: &str = "https://api.mainnet-beta.solana.com";
const EXPIRE_TIMEOUT_SECS: i64 = 14 * 24 * 3600;
const CHALLENGE_WINDOW_SECS: i64 = 24 * 3600;
const MAX_FIRESTORE_PAGES: usize = 1_000;
const RPC_BATCH_SIZE: usize = 100;

#[derive(Debug, Clone)]
pub struct AuditArgs {
    pub project: String,
    pub rpc_url: String,
}

impl Default for AuditArgs {
    fn default() -> Self {
        Self {
            project: DEFAULT_FIRESTORE_PROJECT.to_string(),
            rpc_url: DEFAULT_SOLANA_RPC.to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReconcileArgs {
    pub task_id: String,
    pub new_state: String,
    pub project: String,
    pub execute: bool,
}

#[derive(Debug, Clone)]
struct CandidateTask {
    task_id: String,
    on_chain_address: String,
    fs_state: String,
    platform: Option<String>,
    submitted_at_ts: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuditEntry {
    pub task_id: String,
    pub on_chain_address: String,
    pub fs_state: String,
    pub platform: Option<String>,
    pub submitted_at_ts: Option<i64>,
    pub on_chain_state: u8,
    pub on_chain_escrow: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct AuditOutput {
    pub needs_expire: Vec<AuditEntry>,
    pub needs_finalize: Vec<AuditEntry>,
    pub fs_stale: Vec<AuditEntry>,
    pub recent: Vec<AuditEntry>,
}

pub fn run_audit_stuck_shillbot(args: &AuditArgs) -> Result<()> {
    ensure!(
        !args.project.is_empty(),
        "Firestore project must be non-empty"
    );
    ensure!(!args.rpc_url.is_empty(), "RPC URL must be non-empty");
    let token = gcloud_access_token()?;
    let transport = CurlTransport;
    let now = Utc::now().timestamp();
    let report = audit_stuck_shillbot_with(&transport, args, &token, now)?;
    println!("{}", serde_json::to_string(&report)?);
    Ok(())
}

pub fn audit_stuck_shillbot_with<T: HttpTransport>(
    transport: &T,
    args: &AuditArgs,
    access_token: &str,
    now_ts: i64,
) -> Result<AuditOutput> {
    ensure!(!access_token.is_empty(), "access token must be non-empty");
    let docs = fetch_candidate_tasks(transport, &args.project, access_token)?;
    let addrs: Vec<String> = docs.iter().map(|d| d.on_chain_address.clone()).collect();
    let states_by_addr = fetch_onchain_states(transport, &args.rpc_url, &addrs)?;

    let mut out = AuditOutput::default();
    for task in docs {
        let &(on_chain_state, on_chain_escrow) =
            states_by_addr.get(&task.on_chain_address).ok_or_else(|| {
                anyhow::anyhow!("missing RPC account entry for {}", task.on_chain_address)
            })?;
        let sub_ts = task.submitted_at_ts;
        let entry = AuditEntry {
            task_id: task.task_id,
            on_chain_address: task.on_chain_address,
            fs_state: task.fs_state,
            platform: task.platform,
            submitted_at_ts: sub_ts,
            on_chain_state,
            on_chain_escrow,
        };
        classify_audit_entry(&mut out, entry, now_ts);
    }
    Ok(out)
}

fn classify_audit_entry(out: &mut AuditOutput, entry: AuditEntry, now_ts: i64) {
    match entry.on_chain_state {
        255 => out.fs_stale.push(entry),
        2 | 7 => {
            if entry
                .submitted_at_ts
                .is_some_and(|ts| now_ts.saturating_sub(ts) > EXPIRE_TIMEOUT_SECS)
            {
                out.needs_expire.push(entry);
            } else {
                out.recent.push(entry);
            }
        }
        3 => {
            if entry
                .submitted_at_ts
                .is_some_and(|ts| now_ts.saturating_sub(ts) > CHALLENGE_WINDOW_SECS)
            {
                out.needs_finalize.push(entry);
            } else {
                out.recent.push(entry);
            }
        }
        _ => out.recent.push(entry),
    }
}

fn fetch_candidate_tasks<T: HttpTransport>(
    transport: &T,
    project: &str,
    access_token: &str,
) -> Result<Vec<CandidateTask>> {
    let encoded_project = percent_encode(project);
    let mut candidates = Vec::new();
    let mut page_token: Option<String> = None;

    for _ in 0..MAX_FIRESTORE_PAGES {
        let mut url = format!(
            "https://firestore.googleapis.com/v1/projects/{encoded_project}/databases/(default)/documents/tasks?pageSize=300"
        );
        if let Some(tok) = &page_token {
            url.push_str("&pageToken=");
            url.push_str(&percent_encode(tok));
        }
        let body = transport.send(&HttpRequest {
            method: "GET",
            url: &url,
            bearer_token: Some(access_token),
            content_type: None,
            body: None,
        })?;
        let page: Value =
            serde_json::from_slice(&body).context("invalid Firestore list response")?;
        if let Some(docs) = page.get("documents").and_then(Value::as_array) {
            for doc in docs {
                if let Some(cand) = parse_candidate_doc(doc) {
                    candidates.push(cand);
                }
            }
        }
        match page.get("nextPageToken").and_then(Value::as_str) {
            Some(next) if !next.is_empty() => page_token = Some(next.to_string()),
            _ => return Ok(candidates),
        }
    }
    bail!("Firestore pagination exceeded {MAX_FIRESTORE_PAGES} pages")
}

fn parse_candidate_doc(doc: &Value) -> Option<CandidateTask> {
    let doc_name = doc.get("name")?.as_str()?;
    let task_id = doc_name.rsplit('/').next()?.to_string();
    let fields = doc.get("fields")?;
    let network = fs_string(fields, "network")?;
    let state = fs_string(fields, "state")?;
    let addr = fs_string(fields, "on_chain_address")?;
    if network != "mainnet"
        || !matches!(state.as_str(), "submitted" | "verified" | "approved")
        || addr.is_empty()
    {
        return None;
    }
    let platform = fs_string(fields, "platform");
    let submitted_at_ts = fs_timestamp_secs(fields, "submitted_at");
    Some(CandidateTask {
        task_id,
        on_chain_address: addr,
        fs_state: state,
        platform,
        submitted_at_ts,
    })
}

fn fs_string(fields: &Value, key: &str) -> Option<String> {
    let field = fields.get(key)?;
    field
        .get("stringValue")
        .or_else(|| field.get("timestampValue"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn fs_timestamp_secs(fields: &Value, key: &str) -> Option<i64> {
    let field = fields.get(key)?;
    if let Some(i_str) = field.get("integerValue").and_then(Value::as_str) {
        return i_str.parse::<i64>().ok();
    }
    let raw = field
        .get("timestampValue")
        .or_else(|| field.get("stringValue"))
        .and_then(Value::as_str)?;
    parse_iso_timestamp(raw)
}

fn parse_iso_timestamp(raw: &str) -> Option<i64> {
    let normalized = raw.replace('Z', "+00:00");
    if let Ok(dt) = DateTime::parse_from_rfc3339(&normalized) {
        return Some(dt.timestamp());
    }
    if let Ok(dt) = DateTime::parse_from_str(&normalized, "%Y-%m-%d %H:%M:%S%.f%:z") {
        return Some(dt.timestamp());
    }
    NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S%.f")
        .ok()
        .map(|ndt| ndt.and_utc().timestamp())
}

fn fetch_onchain_states<T: HttpTransport>(
    transport: &T,
    rpc_url: &str,
    addrs: &[String],
) -> Result<HashMap<String, (u8, u64)>> {
    let mut states = HashMap::with_capacity(addrs.len());
    for batch in addrs.chunks(RPC_BATCH_SIZE) {
        let payload = serde_json::to_vec(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getMultipleAccounts",
            "params": [batch, {"encoding": "base64"}]
        }))?;
        let resp_bytes = transport.send(&HttpRequest {
            method: "POST",
            url: rpc_url,
            bearer_token: None,
            content_type: Some("application/json"),
            body: Some(&payload),
        })?;
        let resp: Value = serde_json::from_slice(&resp_bytes).context("invalid Solana RPC JSON")?;
        let values = resp
            .get("result")
            .and_then(|r| r.get("value"))
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow::anyhow!("RPC response missing result.value array"))?;
        ensure!(values.len() == batch.len(), "RPC account count mismatch");
        for (addr, acct) in batch.iter().zip(values.iter()) {
            states.insert(addr.clone(), decode_task_account(acct)?);
        }
    }
    Ok(states)
}

fn decode_task_account(acct: &Value) -> Result<(u8, u64)> {
    if acct.is_null() {
        return Ok((255, 0));
    }
    let encoded = acct
        .get("data")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first())
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("malformed Solana account data tuple"))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .context("invalid base64 in Solana account data")?;
    ensure!(
        bytes.len() >= 90,
        "task account shorter than 90 bytes: {}",
        bytes.len()
    );
    let state = bytes[80];
    let escrow_slice: [u8; 8] = bytes[82..90]
        .try_into()
        .map_err(|_| anyhow::anyhow!("failed slicing task escrow bytes"))?;
    Ok((state, u64::from_le_bytes(escrow_slice)))
}

pub fn run_reconcile_firestore_state(args: &ReconcileArgs) -> Result<()> {
    validate_reconcile_inputs(args)?;
    let token = gcloud_access_token()?;
    let transport = CurlTransport;
    let now_iso = Utc::now().to_rfc3339();
    let output = reconcile_firestore_state_with(&transport, args, &token, &now_iso)?;
    println!("{output}");
    Ok(())
}

fn validate_reconcile_inputs(args: &ReconcileArgs) -> Result<()> {
    ensure!(
        !args.task_id.is_empty()
            && !args.task_id.contains('/')
            && !args.task_id.contains("..")
            && args.task_id.bytes().all(|b| b.is_ascii_graphic()),
        "invalid task_id: {}",
        args.task_id
    );
    ensure!(
        !args.new_state.is_empty() && args.new_state.bytes().all(|b| b.is_ascii_graphic()),
        "invalid new_state: {}",
        args.new_state
    );
    ensure!(
        !args.project.is_empty(),
        "Firestore project must be non-empty"
    );
    Ok(())
}

pub fn reconcile_firestore_state_with<T: HttpTransport>(
    transport: &T,
    args: &ReconcileArgs,
    access_token: &str,
    now_iso: &str,
) -> Result<String> {
    validate_reconcile_inputs(args)?;
    ensure!(!access_token.is_empty(), "access token must be non-empty");

    let doc_url = format!(
        "https://firestore.googleapis.com/v1/projects/{}/databases/(default)/documents/tasks/{}",
        percent_encode(&args.project),
        percent_encode(&args.task_id)
    );
    let current_bytes = match transport.send(&HttpRequest {
        method: "GET",
        url: &doc_url,
        bearer_token: Some(access_token),
        content_type: None,
        body: None,
    }) {
        Ok(bytes) => bytes,
        Err(HttpFailure::Status { status: 404, .. }) => {
            bail!("task document not found: {}", args.task_id)
        }
        Err(err) => return Err(err.into()),
    };

    let current_doc: Value =
        serde_json::from_slice(&current_bytes).context("invalid Firestore document JSON")?;
    let fields = current_doc.get("fields").unwrap_or(&Value::Null);
    let old_state = fs_string(fields, "state").unwrap_or_else(|| "<unset>".to_string());
    let old_finalized = fs_string(fields, "finalized_at").unwrap_or_else(|| "<unset>".to_string());

    let mode = if args.execute { "execute" } else { "dry-run" };
    let diff = format!(
        "reconcile-firestore-state ({mode}): projects/{}/tasks/{}\n  state:        {old_state:?} -> {:?}\n  finalized_at: {old_finalized:?} -> {now_iso:?}",
        args.project, args.task_id, args.new_state
    );

    if !args.execute {
        return Ok(format!(
            "{diff}\ndry-run only: pass --execute (or --apply) to write this Firestore update"
        ));
    }

    let patch_url = format!(
        "{doc_url}?updateMask.fieldPaths=state&updateMask.fieldPaths=finalized_at&currentDocument.exists=true"
    );
    let patch_body = serde_json::to_vec(&json!({
        "fields": {
            "state": { "stringValue": args.new_state },
            "finalized_at": { "stringValue": now_iso }
        }
    }))?;
    transport.send(&HttpRequest {
        method: "PATCH",
        url: &patch_url,
        bearer_token: Some(access_token),
        content_type: Some("application/json"),
        body: Some(&patch_body),
    })?;

    Ok(format!("{diff}\nok {} → {}", args.task_id, args.new_state))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;

    struct MockTransport {
        responses: RefCell<VecDeque<std::result::Result<Vec<u8>, HttpFailure>>>,
        requests: RefCell<Vec<(String, String)>>,
    }

    impl MockTransport {
        fn new(responses: Vec<std::result::Result<Vec<u8>, HttpFailure>>) -> Self {
            Self {
                responses: RefCell::new(responses.into()),
                requests: RefCell::new(Vec::new()),
            }
        }
    }

    impl HttpTransport for MockTransport {
        fn send(&self, request: &HttpRequest<'_>) -> std::result::Result<Vec<u8>, HttpFailure> {
            self.requests
                .borrow_mut()
                .push((request.method.to_string(), request.url.to_string()));
            self.responses
                .borrow_mut()
                .pop_front()
                .expect("unexpected request in MockTransport")
        }
    }

    fn make_account_b64(state: u8, escrow: u64) -> String {
        let mut raw = vec![0u8; 96];
        raw[80] = state;
        raw[82..90].copy_from_slice(&escrow.to_le_bytes());
        base64::engine::general_purpose::STANDARD.encode(raw)
    }

    #[test]
    fn audit_categorizes_stale_expired_finalizable_and_recent() {
        let now = 2_000_000_000i64;
        let old_sub = DateTime::from_timestamp(now - EXPIRE_TIMEOUT_SECS - 10, 0)
            .unwrap()
            .to_rfc3339();
        let challenge_sub = DateTime::from_timestamp(now - CHALLENGE_WINDOW_SECS - 10, 0)
            .unwrap()
            .to_rfc3339();
        let fresh_sub = DateTime::from_timestamp(now - 60, 0).unwrap().to_rfc3339();

        let fs_page = serde_json::to_vec(&json!({
            "documents": [
                {
                    "name": "projects/p/databases/(default)/documents/tasks/t-stale",
                    "fields": {
                        "network": {"stringValue": "mainnet"},
                        "state": {"stringValue": "submitted"},
                        "on_chain_address": {"stringValue": "AddrStale"},
                        "submitted_at": {"timestampValue": old_sub}
                    }
                },
                {
                    "name": "projects/p/databases/(default)/documents/tasks/t-expire",
                    "fields": {
                        "network": {"stringValue": "mainnet"},
                        "state": {"stringValue": "submitted"},
                        "on_chain_address": {"stringValue": "AddrExpire"},
                        "submitted_at": {"stringValue": old_sub}
                    }
                },
                {
                    "name": "projects/p/databases/(default)/documents/tasks/t-finalize",
                    "fields": {
                        "network": {"stringValue": "mainnet"},
                        "state": {"stringValue": "verified"},
                        "on_chain_address": {"stringValue": "AddrFinalize"},
                        "submitted_at": {"timestampValue": challenge_sub}
                    }
                },
                {
                    "name": "projects/p/databases/(default)/documents/tasks/t-recent",
                    "fields": {
                        "network": {"stringValue": "mainnet"},
                        "state": {"stringValue": "approved"},
                        "on_chain_address": {"stringValue": "AddrRecent"},
                        "submitted_at": {"timestampValue": fresh_sub}
                    }
                }
            ]
        }))
        .unwrap();

        let rpc_resp = serde_json::to_vec(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "value": [
                    null,
                    {"data": [make_account_b64(2, 50_000), "base64"]},
                    {"data": [make_account_b64(3, 75_000), "base64"]},
                    {"data": [make_account_b64(7, 90_000), "base64"]}
                ]
            }
        }))
        .unwrap();

        let mock = MockTransport::new(vec![Ok(fs_page), Ok(rpc_resp)]);
        let report = audit_stuck_shillbot_with(&mock, &AuditArgs::default(), "tok", now).unwrap();
        assert_eq!(report.fs_stale.len(), 1);
        assert_eq!(report.fs_stale[0].task_id, "t-stale");
        assert_eq!(report.needs_expire.len(), 1);
        assert_eq!(report.needs_expire[0].task_id, "t-expire");
        assert_eq!(report.needs_expire[0].on_chain_escrow, 50_000);
        assert_eq!(report.needs_finalize.len(), 1);
        assert_eq!(report.needs_finalize[0].task_id, "t-finalize");
        assert_eq!(report.recent.len(), 1);
        assert_eq!(report.recent[0].task_id, "t-recent");
    }

    #[test]
    fn reconcile_defaults_to_dry_run_and_patches_only_on_execute() {
        let existing_doc = serde_json::to_vec(&json!({
            "name": "projects/coordination-game-prod/databases/(default)/documents/tasks/task-1",
            "fields": {
                "state": {"stringValue": "submitted"}
            }
        }))
        .unwrap();

        let dry_mock = MockTransport::new(vec![Ok(existing_doc.clone())]);
        let dry_out = reconcile_firestore_state_with(
            &dry_mock,
            &ReconcileArgs {
                task_id: "task-1".into(),
                new_state: "expired".into(),
                project: DEFAULT_FIRESTORE_PROJECT.into(),
                execute: false,
            },
            "tok",
            "2026-11-16T00:00:00+00:00",
        )
        .unwrap();
        assert!(dry_out.contains("dry-run only"));
        assert_eq!(dry_mock.requests.borrow().len(), 1);
        assert_eq!(dry_mock.requests.borrow()[0].0, "GET");

        let exec_mock = MockTransport::new(vec![Ok(existing_doc), Ok(b"{}".to_vec())]);
        let exec_out = reconcile_firestore_state_with(
            &exec_mock,
            &ReconcileArgs {
                task_id: "task-1".into(),
                new_state: "expired".into(),
                project: DEFAULT_FIRESTORE_PROJECT.into(),
                execute: true,
            },
            "tok",
            "2026-11-16T00:00:00+00:00",
        )
        .unwrap();
        assert!(exec_out.contains("ok task-1 → expired"));
        assert_eq!(exec_mock.requests.borrow().len(), 2);
        assert_eq!(exec_mock.requests.borrow()[1].0, "PATCH");
        assert!(exec_mock.requests.borrow()[1]
            .1
            .contains("updateMask.fieldPaths=state"));
    }

    #[test]
    fn reconcile_rejects_missing_doc_and_path_traversal() {
        let mock_404 = MockTransport::new(vec![Err(HttpFailure::Status {
            status: 404,
            body: Vec::new(),
        })]);
        let err = reconcile_firestore_state_with(
            &mock_404,
            &ReconcileArgs {
                task_id: "missing-id".into(),
                new_state: "expired".into(),
                project: DEFAULT_FIRESTORE_PROJECT.into(),
                execute: true,
            },
            "tok",
            "2026-11-16T00:00:00+00:00",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("not found"), "unexpected error: {err}");

        let bad_mock = MockTransport::new(vec![]);
        assert!(reconcile_firestore_state_with(
            &bad_mock,
            &ReconcileArgs {
                task_id: "../bad".into(),
                new_state: "expired".into(),
                project: DEFAULT_FIRESTORE_PROJECT.into(),
                execute: false,
            },
            "tok",
            "2026-11-16T00:00:00+00:00",
        )
        .is_err());
    }
}
