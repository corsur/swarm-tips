use super::*;

#[test]
fn website_instructions_survive_proxy_round_trip() {
    let source = serde_json::json!({
        "task_id": "campaign:task", "state": "claimed", "platform": 9,
        "task_nonce": "3f3da71a00000000343d405cab52c716",
        "website_instructions": {"status": "ready", "html": "<footer>copy exactly</footer>", "guidance": "Publish then submit."},
        "referral_instructions": {"status": "ready", "content_id_format": "campaign_id UUID"},
        "gameplay_instructions": {"status": "ready", "content_id_format": "game_id UUID"},
        "lean_proof_instructions": {"status": "ready", "content_id_format": "lean4:proof:<base64>"}
    });
    let task: TaskSummary = serde_json::from_value(source.clone()).unwrap();
    let output = serde_json::to_value(task).unwrap();
    assert_eq!(output["task_nonce"], source["task_nonce"]);
    assert_eq!(
        output["website_instructions"],
        source["website_instructions"]
    );
    assert_eq!(
        output["referral_instructions"],
        source["referral_instructions"]
    );
    assert_eq!(
        output["gameplay_instructions"],
        source["gameplay_instructions"]
    );
    assert_eq!(
        output["lean_proof_instructions"],
        source["lean_proof_instructions"]
    );
}

#[test]
fn task_summary_parses_orchestrator_wire_format() {
    // This is a real (trimmed) response from
    // shillbot-api GET /tasks. Regression guard against the
    // proxy's TaskSummary drifting away from the orchestrator's
    // TaskResponse shape — that mismatch caused the
    // "error decoding response body" bug in the live MCP server.
    let json = serde_json::json!({
        "task_id": "campaign-uuid:task-uuid",
        "campaign_id": "campaign-uuid",
        "campaign_topic": "Play a round of coordination.game",
        "state": "open",
        "client": "ClientWallet1111111111111111111111111111111",
        "task_pda": "TaskPda111111111111111111111111111111111111",
        "agent": null,
        "content_id": null,
        "composite_score": null,
        "payment_amount": null,
        "platform": 5,
        "created_at": "2026-04-07T08:20:57Z",
        "claimed_at": null,
        "submitted_at": null,
        "brief": {
            "topic": "Play a round of coordination.game",
            "brand_voice": "Direct incentive.",
            "cta": "Play one round at coordination.game",
            "utm_link": "https://coordination.game",
            "blocklist": [],
            "examples": []
        },
        "estimated_payment_lamports": 20_000_000u64,
        "quality_threshold": 200_000u64,
        "scoring_scales": { "views_scale": 5000, "likes_scale": 250, "comments_scale": 50 }
    });

    let parsed: TaskSummary = serde_json::from_value(json).expect("must deserialize");
    assert_eq!(parsed.task_id, "campaign-uuid:task-uuid");
    assert_eq!(parsed.state, "open");
    assert_eq!(parsed.platform, Some(5));
    assert_eq!(parsed.estimated_payment_lamports, Some(20_000_000));
    assert_eq!(
        parsed.campaign_topic.as_deref(),
        Some("Play a round of coordination.game")
    );
    assert!(parsed.brief.is_some());
    assert_eq!(
        parsed.client.as_deref(),
        Some("ClientWallet1111111111111111111111111111111")
    );
    assert_eq!(
        parsed.task_pda.as_deref(),
        Some("TaskPda111111111111111111111111111111111111")
    );
}

#[test]
fn task_list_response_parses_with_next_cursor_camelcase_or_snake() {
    // Orchestrator currently emits snake_case `next_cursor`. Test the
    // happy path to lock in the contract.
    let json = serde_json::json!({
        "tasks": [
            {
                "task_id": "c:t",
                "state": "open",
                "platform": 3,
                "quality_threshold": 0
            }
        ],
        "next_cursor": null
    });
    let parsed: TaskListResponse = serde_json::from_value(json).expect("must deserialize");
    assert_eq!(parsed.tasks.len(), 1);
    assert_eq!(parsed.tasks[0].task_id, "c:t");
    assert!(parsed.next_cursor.is_none());
}

#[test]
fn transaction_response_parses_orchestrator_claim_payload() {
    // Trimmed real shape from `POST /tasks/:id/claim` — the orchestrator
    // returns the unsigned tx as base64. The MCP proxy must round-trip
    // this without losing the `transaction` field.
    let json = serde_json::json!({
        "message": "Sign and submit this transaction to claim the task on-chain. Then call POST /tasks/:id/confirm with the tx signature.",
        "task_id": "campaign-uuid:task-uuid",
        "transaction": "AQAAAA...base64-bytes...AAAB"
    });
    let parsed: TransactionResponse = serde_json::from_value(json).expect("must deserialize");
    assert_eq!(parsed.task_id, "campaign-uuid:task-uuid");
    assert_eq!(
        parsed.transaction.as_deref(),
        Some("AQAAAA...base64-bytes...AAAB")
    );
    assert!(parsed.task_pda.is_none());
}

#[test]
fn confirm_action_serializes_snake_case() {
    // Must match shillbot-api's
    // #[serde(rename_all = "snake_case")] on ConfirmAction.
    assert_eq!(
        serde_json::to_value(ConfirmAction::Claim).unwrap(),
        serde_json::Value::String("claim".to_string())
    );
    assert_eq!(
        serde_json::to_value(ConfirmAction::Submit).unwrap(),
        serde_json::Value::String("submit".to_string())
    );
    // approve must serialize as `"approve"` (snake_case) to match the
    // orchestrator enum; a casing drift would silently route confirm
    // calls into serde's fallthrough and break the client review gate.
    assert_eq!(
        serde_json::to_value(ConfirmAction::Approve).unwrap(),
        serde_json::Value::String("approve".to_string())
    );
    assert_eq!(
        serde_json::to_value(ConfirmAction::Verify).unwrap(),
        serde_json::Value::String("verify".to_string())
    );
    assert_eq!(
        serde_json::to_value(ConfirmAction::Finalize).unwrap(),
        serde_json::Value::String("finalize".to_string())
    );
}

#[test]
fn network_query_suffix_none_is_empty() {
    // Mainnet-default behaviour: omitting the network must produce no
    // query suffix so the orchestrator's existing routes keep working
    // unchanged for clients that don't pass `network`.
    assert_eq!(network_query_suffix(None), "");
}

#[test]
fn network_query_suffix_devnet_is_url_encoded() {
    // Devnet is the primary non-default value; the orchestrator
    // dispatches per-network internally based on this exact token.
    assert_eq!(network_query_suffix(Some("devnet")), "?network=devnet");
}

#[test]
fn network_query_suffix_mainnet_is_explicit_when_passed() {
    // We don't strip mainnet here — let the orchestrator handle it.
    // This keeps the proxy honest: what came in goes out, modulo
    // url-encoding.
    assert_eq!(network_query_suffix(Some("mainnet")), "?network=mainnet");
}

#[test]
fn network_query_suffix_url_encodes_unexpected_chars() {
    // The handler validates the token before reaching the proxy, but
    // belt-and-suspenders: anything weird that does sneak through is
    // url-encoded so it can't break the URL parse on the orchestrator
    // side. `=` -> `%3D`.
    let suffix = network_query_suffix(Some("a=b"));
    assert_eq!(suffix, "?network=a%3Db");
}

#[test]
fn test_earnings_response_serialization() {
    let earnings = EarningsResponse {
        total_earned_lamports: 10_000_000,
        total_subsidy_lamports: 1_000_000,
        tasks_completed: 5,
        average_score: 0.75,
    };

    let json = serde_json::to_string(&earnings).unwrap();
    let parsed: EarningsResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.total_earned_lamports, 10_000_000);
    assert_eq!(parsed.total_subsidy_lamports, 1_000_000);
    assert_eq!(parsed.tasks_completed, 5);
    assert_eq!(parsed.average_score, 0.75);
}

/// Flow tests: every Shillbot proxy method must forward
/// `network = Some("devnet")` as a `?network=devnet` query string. The
/// orchestrator already dispatches per-network internally based on this
/// exact token; the regression risk we're guarding against is the proxy
/// silently dropping the parameter and pinning every call to mainnet.
///
/// We use `wiremock` to stand up a fake orchestrator that asserts the
/// query string the proxy emits. Each method gets its own happy-path
/// devnet flow so a failure points at the offending method without
/// fishing through a single combined assertion.
mod network_flow_tests {
    use super::*;
    use wiremock::matchers::{body_partial_json, header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Minimal `TaskSummary`-shaped JSON the proxy can deserialize.
    fn minimal_task_json(task_id: &str, state: &str) -> serde_json::Value {
        serde_json::json!({
            "task_id": task_id,
            "state": state,
            "platform": 5,
            "quality_threshold": 0,
        })
    }

    /// Minimal `TransactionResponse`-shaped JSON.
    fn minimal_tx_json(task_id: &str) -> serde_json::Value {
        serde_json::json!({
            "message": "ok",
            "task_id": task_id,
            "transaction": "AAAA",
        })
    }

    #[tokio::test]
    async fn list_tasks_forwards_devnet_query() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/tasks"))
            .and(query_param("network", "devnet"))
            .and(query_param("limit", "20"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "tasks": [{
                    "task_id": "c:t",
                    "state": "open",
                    "platform": 5,
                    "quality_threshold": 0,
                    "client": "ClientWallet111",
                    "task_pda": "TaskPda111",
                }],
                "next_cursor": null,
            })))
            .expect(1)
            .mount(&server)
            .await;

        let proxy = OrchestratorProxy::new(server.uri(), server.uri())
            .authenticated("test-signed-session".into());
        let result = proxy
            .list_tasks(None, None, Some("devnet"))
            .await
            .expect("ok");
        assert_eq!(result.tasks.len(), 1);
        // The new optional ownership fields must round-trip through the
        // proxy's TaskSummary so MCP agents can see who funds the task
        // and its on-chain PDA.
        assert_eq!(result.tasks[0].client.as_deref(), Some("ClientWallet111"));
        assert_eq!(result.tasks[0].task_pda.as_deref(), Some("TaskPda111"));
    }

    #[tokio::test]
    async fn list_tasks_no_network_omits_query_param() {
        // Mainnet-default behaviour: the orchestrator's default route
        // must keep working unchanged when network is None.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/tasks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "tasks": [],
                "next_cursor": null,
            })))
            .expect(1)
            .mount(&server)
            .await;

        let proxy = OrchestratorProxy::new(server.uri(), server.uri())
            .authenticated("test-signed-session".into());
        // Verify the request URL had no `network` param. Wiremock
        // doesn't expose a "missing param" matcher cleanly, so we
        // pull the request log and assert it directly.
        proxy.list_tasks(None, None, None).await.expect("ok");
        let received = server.received_requests().await.expect("requests recorded");
        assert_eq!(received.len(), 1);
        let url = received[0].url.as_str();
        assert!(
            !url.contains("network="),
            "expected no network query param when network=None, got url={url}"
        );
    }

    #[tokio::test]
    async fn get_task_details_forwards_devnet_query() {
        let server = MockServer::start().await;
        let mut body = minimal_task_json("c:t", "open");
        body["statement_lean"] = serde_json::json!("def statementProp : Prop := True\n");
        body["lean_policy"] = serde_json::json!(2);
        body["requires_approval"] = serde_json::json!(false);
        body["check_detail"] = serde_json::json!("verified proof");
        body["onchain_action_instructions"] = serde_json::json!({ "status": "ready" });
        body["onchain_action_spec"] =
            serde_json::json!({ "schema_version": 1, "target_chain": "eip155:8453" });
        body["trust_level"] = serde_json::json!("anchored");
        Mock::given(method("GET"))
            .and(path("/tasks/c:t"))
            .and(query_param("network", "devnet"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body.clone()))
            .expect(1)
            .mount(&server)
            .await;

        let proxy = OrchestratorProxy::new(server.uri(), server.uri())
            .authenticated("test-signed-session".into());
        let task = proxy
            .get_task_details("c:t", Some("devnet"))
            .await
            .expect("ok");
        assert_eq!(task.task_id, "c:t");
        let result = serde_json::to_value(task).unwrap();
        for field in [
            "statement_lean",
            "lean_policy",
            "requires_approval",
            "check_detail",
            "onchain_action_instructions",
            "onchain_action_spec",
            "trust_level",
        ] {
            assert_eq!(result[field], body[field]);
        }
    }

    #[tokio::test]
    async fn claim_task_forwards_devnet_query() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/tasks/c:t/claim"))
            .and(query_param("network", "devnet"))
            .and(query_param("sponsor", "auto"))
            .and(header("authorization", "Bearer test-signed-session"))
            .respond_with(ResponseTemplate::new(200).set_body_json(minimal_tx_json("c:t")))
            .expect(1)
            .mount(&server)
            .await;

        let proxy = OrchestratorProxy::new(server.uri(), server.uri())
            .authenticated("test-signed-session".into());
        let resp = proxy
            .claim_task("c:t", "wallet1", Some("devnet"))
            .await
            .expect("ok");
        assert_eq!(resp.task_id, "c:t");
        assert_eq!(resp.transaction.as_deref(), Some("AAAA"));
    }

    #[tokio::test]
    async fn claim_sponsorship_template_requires_sponsor_and_network() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/tasks/c:t/claim"))
            .and(query_param("network", "devnet"))
            .and(query_param("sponsor", "true"))
            .and(header("authorization", "Bearer test-signed-session"))
            .respond_with(ResponseTemplate::new(200).set_body_json(minimal_tx_json("c:t")))
            .expect(1)
            .mount(&server)
            .await;

        let proxy = OrchestratorProxy::new(server.uri(), server.uri())
            .authenticated("test-signed-session".into());
        let resp = proxy
            .sponsorship_template("c:t", "wallet1", "claim", None, Some("devnet"))
            .await
            .expect("ok");
        assert_eq!(resp.task_id, "c:t");
    }

    #[tokio::test]
    async fn submit_sponsorship_template_forwards_content_id() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/tasks/c:t/submit"))
            .and(query_param("network", "devnet"))
            .and(query_param("sponsor", "true"))
            .and(header("authorization", "Bearer test-signed-session"))
            .and(body_partial_json(
                serde_json::json!({ "content_id": "post:123" }),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(minimal_tx_json("c:t")))
            .expect(1)
            .mount(&server)
            .await;

        let proxy = OrchestratorProxy::new(server.uri(), server.uri())
            .authenticated("test-signed-session".into());
        let resp = proxy
            .sponsorship_template("c:t", "wallet1", "submit", Some("post:123"), Some("devnet"))
            .await
            .expect("ok");
        assert_eq!(resp.task_id, "c:t");
    }

    #[tokio::test]
    async fn create_campaign_posts_brief_and_returns_id() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/campaigns"))
            .and(query_param("network", "devnet"))
            .and(header("authorization", "Bearer test-signed-session"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "campaign_id": "camp1" })),
            )
            .expect(1)
            .mount(&server)
            .await;

        let proxy = OrchestratorProxy::new(server.uri(), server.uri())
            .authenticated("test-signed-session".into());
        let brief = serde_json::json!({
            "topic": "t", "brand_voice": "v", "cta": "c", "utm_link": "u"
        });
        let id = proxy
            .create_campaign(CreateCampaignParams {
                wallet_pubkey: "wallet1",
                brief,
                budget_lamports: 20_000_000,
                platform: 5,
                requires_approval: false,
                statement_lean: None,
                lean_policy: None,
                onchain_action: None,
                network: Some("devnet"),
            })
            .await
            .expect("ok");
        assert_eq!(id, "camp1");
    }

    #[tokio::test]
    async fn fund_campaign_builds_self_paid_create_tx() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/campaigns/camp1/fund"))
            .and(query_param("network", "devnet"))
            .and(header("authorization", "Bearer test-signed-session"))
            .respond_with(ResponseTemplate::new(200).set_body_json(minimal_tx_json("camp1:task1")))
            .expect(1)
            .mount(&server)
            .await;

        let proxy = OrchestratorProxy::new(server.uri(), server.uri())
            .authenticated("test-signed-session".into());
        let resp = proxy
            .fund_campaign("camp1", "wallet1", 20_000_000, Some("devnet"))
            .await
            .expect("ok");
        assert_eq!(resp.task_id, "camp1:task1");
        assert!(resp.transaction.is_some());
    }

    #[tokio::test]
    async fn onboard_agent_posts_to_onboard_endpoint_with_network() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/agent/onboard"))
            .and(query_param("network", "mainnet"))
            .and(header("authorization", "Bearer test-signed-session"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "signature": "sig123",
                "message": "Onboarded: ... Claim tasks with ?network=mainnet&sponsor=true.",
            })))
            .expect(1)
            .mount(&server)
            .await;

        let proxy = OrchestratorProxy::new(server.uri(), server.uri())
            .authenticated("test-signed-session".into());
        let resp = proxy
            .onboard_agent("wallet1", Some("mainnet"))
            .await
            .expect("ok");
        assert_eq!(resp["signature"], "sig123");
    }

    #[tokio::test]
    async fn onboard_agent_rejects_empty_wallet_without_calling() {
        let server = MockServer::start().await;
        let proxy = OrchestratorProxy::new(server.uri(), server.uri())
            .authenticated("test-signed-session".into());
        let err = proxy
            .onboard_agent("", Some("mainnet"))
            .await
            .expect_err("empty wallet must be rejected");
        assert!(matches!(err, McpServiceError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn fund_campaign_rejects_zero_amount_without_calling() {
        // amount 0 must be rejected at the boundary — no HTTP call made.
        let server = MockServer::start().await;
        let proxy = OrchestratorProxy::new(server.uri(), server.uri())
            .authenticated("test-signed-session".into());
        let err = proxy
            .fund_campaign("camp1", "wallet1", 0, Some("devnet"))
            .await
            .expect_err("zero amount must be rejected");
        assert!(matches!(err, McpServiceError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn submit_task_forwards_devnet_query() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/tasks/c:t/submit"))
            .and(query_param("network", "devnet"))
            .and(query_param("sponsor", "auto"))
            .and(body_partial_json(
                serde_json::json!({ "content_id": "yt-abc" }),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(minimal_tx_json("c:t")))
            .expect(1)
            .mount(&server)
            .await;

        let proxy = OrchestratorProxy::new(server.uri(), server.uri())
            .authenticated("test-signed-session".into());
        let resp = proxy
            .submit_task(SubmitTaskParams {
                task_id: "c:t",
                wallet_pubkey: "wallet1",
                content_id: "yt-abc",
                network: Some("devnet"),
                ..Default::default()
            })
            .await
            .expect("ok");
        assert_eq!(resp.task_id, "c:t");
        assert_eq!(resp.transaction.as_deref(), Some("AAAA"));
    }

    #[tokio::test]
    async fn claim_and_submit_omit_sponsor_on_mainnet_and_forward_lean_proof_body() {
        let server = MockServer::start().await;
        let lean_src = "import Mathlib\ntheorem t : True := trivial\n";

        Mock::given(method("POST"))
            .and(path("/tasks/c:lean/claim"))
            .respond_with(ResponseTemplate::new(200).set_body_json(minimal_tx_json("c:lean")))
            .expect(2)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/tasks/c:lean/submit"))
            .and(body_partial_json(serde_json::json!({
                "content_id": "",
                "proof_lean": lean_src,
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "message": "ok",
                "task_id": "c:lean",
                "transaction": "BBBB",
                "content_id": "lean4:inline:sha256:0123456789abcdef",
            })))
            .expect(1)
            .mount(&server)
            .await;

        let proxy = OrchestratorProxy::new(server.uri(), server.uri())
            .authenticated("test-signed-session".into());

        // 1. Default-mainnet (`None`) and explicit `"mainnet"` claims send NO `sponsor` param (U1).
        let claim_resp = proxy
            .claim_task("c:lean", "wallet1", None)
            .await
            .expect("default mainnet claim ok");
        assert_eq!(claim_resp.task_id, "c:lean");
        assert_eq!(claim_resp.transaction.as_deref(), Some("AAAA"));

        let explicit_mainnet_claim = proxy
            .claim_task("c:lean", "wallet1", Some("mainnet"))
            .await
            .expect("explicit mainnet claim ok");
        assert_eq!(explicit_mainnet_claim.task_id, "c:lean");

        // 2. LeanProof submit on mainnet forwards verbatim `proof_lean` body & NO `sponsor` param.
        let submit_resp = proxy
            .submit_task(SubmitTaskParams {
                task_id: "c:lean",
                wallet_pubkey: "wallet1",
                content_id: "",
                proof_lean: Some(lean_src),
                ..Default::default()
            })
            .await
            .expect("lean submit ok");
        assert_eq!(submit_resp.task_id, "c:lean");
        assert_eq!(submit_resp.transaction.as_deref(), Some("BBBB"));
        assert_eq!(
            submit_resp.content_id.as_deref(),
            Some("lean4:inline:sha256:0123456789abcdef")
        );

        let received = server.received_requests().await.expect("requests");
        assert_eq!(received.len(), 3);
        assert_eq!(
            received[0].url.query(),
            None,
            "default mainnet must omit sponsor"
        );
        assert_eq!(
            received[1].url.query(),
            Some("network=mainnet"),
            "explicit mainnet must omit sponsor"
        );
        assert_eq!(
            received[2].url.query(),
            None,
            "default mainnet submit must omit sponsor"
        );

        // 3. Reject when BOTH content_id and proof_lean are empty/whitespace without hitting HTTP.
        let err = proxy
            .submit_task(SubmitTaskParams {
                task_id: "c:lean",
                wallet_pubkey: "wallet1",
                content_id: "",
                proof_lean: Some("   "),
                ..Default::default()
            })
            .await
            .expect_err("empty content_id + whitespace proof_lean must reject");
        assert!(matches!(err, McpServiceError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn preflight_task_forwards_compile_true_and_submit_task_forwards_attribution() {
        let server = MockServer::start().await;
        let lean_src = "theorem proof : statementProp := by exact trivial\n";

        Mock::given(method("POST"))
            .and(path("/tasks/c:lean/preflight"))
            .and(query_param("network", "devnet"))
            .and(body_partial_json(serde_json::json!({
                "proof_lean": lean_src,
                "compile": true,
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "task_id": "c:lean",
                "lean_policy": 3,
                "detail": "ok",
                "compiled": true,
                "verdict": "Pass",
            })))
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/tasks/c:lean/submit"))
            .and(body_partial_json(serde_json::json!({
                "content_id": "",
                "proof_lean": lean_src,
                "agent_harness": "swarm-cli",
                "model_family": "gemini-3-pro",
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(minimal_tx_json("c:lean")))
            .expect(1)
            .mount(&server)
            .await;

        let proxy = OrchestratorProxy::new(server.uri(), server.uri())
            .authenticated("test-signed-session".into());

        let preflight = proxy
            .preflight_task(PreflightTaskParams {
                task_id: "c:lean",
                proof_lean: Some(lean_src),
                compile: Some(true),
                network: Some("devnet"),
                ..Default::default()
            })
            .await
            .expect("preflight ok");
        assert_eq!(preflight["ok"], true);
        assert_eq!(preflight["compiled"], true);

        let submit = proxy
            .submit_task(SubmitTaskParams {
                task_id: "c:lean",
                wallet_pubkey: "wallet1",
                content_id: "",
                proof_lean: Some(lean_src),
                agent_harness: Some("  swarm-cli  "),
                model_family: Some("gemini-3-pro"),
                network: None,
            })
            .await
            .expect("submit with attribution ok");
        assert_eq!(submit.task_id, "c:lean");
    }

    #[tokio::test]
    async fn get_verification_data_forwards_devnet_query() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/tasks/c:t/build-verify"))
            .and(query_param("network", "devnet"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "task_id": "c:t",
                "task_pda": "PDA111",
                "composite_score": 750_000,
                "verification_hash": "deadbeef",
                "global_state": "GS",
                "switchboard_feed": "FEED",
            })))
            .expect(1)
            .mount(&server)
            .await;

        let proxy = OrchestratorProxy::new(server.uri(), server.uri())
            .authenticated("test-signed-session".into());
        let resp = proxy
            .get_verification_data("c:t", "wallet1", Some("devnet"))
            .await
            .expect("ok");
        assert_eq!(resp.task_id, "c:t");
        assert_eq!(resp.composite_score, 750_000);
    }

    #[tokio::test]
    async fn build_finalize_forwards_devnet_query() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/tasks/c:t/build-finalize"))
            .and(query_param("network", "devnet"))
            .respond_with(ResponseTemplate::new(200).set_body_json(minimal_tx_json("c:t")))
            .expect(1)
            .mount(&server)
            .await;

        let proxy = OrchestratorProxy::new(server.uri(), server.uri())
            .authenticated("test-signed-session".into());
        proxy
            .build_finalize("c:t", "wallet1", Some("devnet"))
            .await
            .expect("ok");
    }

    #[tokio::test]
    async fn approve_task_forwards_devnet_query() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/tasks/c:t/approve"))
            .and(query_param("network", "devnet"))
            .respond_with(ResponseTemplate::new(200).set_body_json(minimal_tx_json("c:t")))
            .expect(1)
            .mount(&server)
            .await;

        let proxy = OrchestratorProxy::new(server.uri(), server.uri())
            .authenticated("test-signed-session".into());
        proxy
            .approve_task("c:t", "wallet1", Some("devnet"))
            .await
            .expect("ok");
    }

    #[tokio::test]
    async fn list_pending_approval_forwards_devnet_query() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/client/pending-approval"))
            .and(query_param("network", "devnet"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "tasks": [],
                "next_cursor": null,
            })))
            .expect(1)
            .mount(&server)
            .await;

        let proxy = OrchestratorProxy::new(server.uri(), server.uri())
            .authenticated("test-signed-session".into());
        let resp = proxy
            .list_pending_approval("wallet1", Some("devnet"))
            .await
            .expect("ok");
        assert!(resp.tasks.is_empty());
    }

    #[tokio::test]
    async fn confirm_task_forwards_devnet_query() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/tasks/c:t/confirm"))
            .and(query_param("network", "devnet"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "task_id": "c:t",
                "action": "claim",
                "message": "ok",
            })))
            .expect(1)
            .mount(&server)
            .await;

        let proxy = OrchestratorProxy::new(server.uri(), server.uri())
            .authenticated("test-signed-session".into());
        proxy
            .confirm_task(
                "c:t",
                "wallet1",
                "sig",
                ConfirmAction::Claim,
                None,
                Some("devnet"),
            )
            .await
            .expect("ok");
    }

    #[tokio::test]
    async fn confirm_task_create_forwards_task_pda() {
        // The create confirmation MUST include task_pda in the body — the
        // orchestrator has no on-chain address for the task yet.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/tasks/c:t/confirm"))
            .and(body_partial_json(serde_json::json!({
                "action": "create",
                "task_pda": "PDA123",
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "task_id": "c:t",
                "action": "create",
                "message": "ok",
            })))
            .expect(1)
            .mount(&server)
            .await;

        let proxy = OrchestratorProxy::new(server.uri(), server.uri())
            .authenticated("test-signed-session".into());
        proxy
            .confirm_task(
                "c:t",
                "wallet1",
                "sig",
                ConfirmAction::Create,
                Some("PDA123"),
                Some("devnet"),
            )
            .await
            .expect("ok");
    }

    #[tokio::test]
    async fn get_earnings_forwards_devnet_query() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/agent/earnings"))
            .and(query_param("network", "devnet"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "total_earned_lamports": 0u64,
                "total_subsidy_lamports": 0u64,
                "tasks_completed": 0u64,
                "average_score": 0.0,
            })))
            .expect(1)
            .mount(&server)
            .await;

        let proxy = OrchestratorProxy::new(server.uri(), server.uri())
            .authenticated("test-signed-session".into());
        proxy
            .get_earnings("wallet1", Some("devnet"))
            .await
            .expect("ok");
    }

    /// End-to-end flow test (the project standard's "flow tests with
    /// mocked I/O"): claim_task -> submit_task -> get_verification_data
    /// -> build_finalize, all with `network = "devnet"`. Asserts each
    /// of the four mocks is hit exactly once, with the network query
    /// string present on every call. Catches a regression where any
    /// single method drops the param mid-pipeline.
    #[tokio::test]
    async fn earn_lifecycle_flow_threads_devnet_through_every_call() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/tasks/c:t/claim"))
            .and(query_param("network", "devnet"))
            .and(query_param("sponsor", "auto"))
            .respond_with(ResponseTemplate::new(200).set_body_json(minimal_tx_json("c:t")))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/tasks/c:t/submit"))
            .and(query_param("network", "devnet"))
            .and(query_param("sponsor", "auto"))
            .respond_with(ResponseTemplate::new(200).set_body_json(minimal_tx_json("c:t")))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/tasks/c:t/build-verify"))
            .and(query_param("network", "devnet"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "task_id": "c:t",
                "task_pda": "PDA111",
                "composite_score": 1u64,
                "verification_hash": "h",
                "global_state": "g",
                "switchboard_feed": "f",
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/tasks/c:t/build-finalize"))
            .and(query_param("network", "devnet"))
            .respond_with(ResponseTemplate::new(200).set_body_json(minimal_tx_json("c:t")))
            .expect(1)
            .mount(&server)
            .await;

        let proxy = OrchestratorProxy::new(server.uri(), server.uri())
            .authenticated("test-signed-session".into());
        proxy
            .claim_task("c:t", "wallet1", Some("devnet"))
            .await
            .expect("claim ok");
        proxy
            .submit_task(SubmitTaskParams {
                task_id: "c:t",
                wallet_pubkey: "wallet1",
                content_id: "yt-id",
                network: Some("devnet"),
                ..Default::default()
            })
            .await
            .expect("submit ok");
        proxy
            .get_verification_data("c:t", "wallet1", Some("devnet"))
            .await
            .expect("verify-data ok");
        proxy
            .build_finalize("c:t", "wallet1", Some("devnet"))
            .await
            .expect("finalize ok");

        // The `expect(1)` clauses on each Mock above are checked at
        // server drop. Make the assertion explicit anyway by counting
        // the recorded requests — four calls, four hits.
        let requests = server.received_requests().await.expect("requests recorded");
        assert_eq!(requests.len(), 4, "expected exactly four orchestrator hits");
        for req in requests {
            assert!(
                req.url.query().unwrap_or("").contains("network=devnet"),
                "request {} missing network=devnet query: {}",
                req.url.path(),
                req.url
            );
        }
    }
}

/// Flow tests for the paid video path (`generate_video` /
/// `check_video_status` proxies) and the VOW attestation reads — the
/// tool surfaces that previously had zero coverage. Same wiremock
/// pattern as `network_flow_tests`.
mod video_and_attestation_flow_tests {
    use super::*;
    use base64::Engine;
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn proxy_for(server: &MockServer) -> OrchestratorProxy {
        OrchestratorProxy::new(server.uri(), server.uri())
            .authenticated("test-signed-session".into())
    }

    fn full_attestation_json() -> serde_json::Value {
        serde_json::json!({
            "version": "vow/v1",
            "network": "devnet",
            "program_id": "2tR37nqMpwdV4DVUHjzUmL1rH2DtkA8zrRA4EAhT7KMi",
            "account": "TaskPda111",
            "account_kind": "Task",
            "task_id": "2590886815712535641",
            "client": "ClientWallet1",
            "agent": "AgentWallet1",
            "state": "verified",
            "platform": 0,
            "composite_score": "910000",
            "score_max": "1000000",
            "verified_at": "2026-07-15T00:00:00Z",
            "verification_hash": "vh",
            "content_hash": "ch",
            "content_id_hash": "cih",
            "oracle_feed": null,
            "extensions": { "future_field": "preserved" },
            "verifier_instructions": "instructions",
        })
    }

    #[tokio::test]
    async fn create_short_crypto_sends_body_and_payment_header() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/shorts/create-crypto"))
            .and(body_partial_json(serde_json::json!({
                "prompt": "make a short",
                "url": "https://example.com",
            })))
            .and(header("payment-signature", "b64payload"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                serde_json::json!({ "session_id": "sess-1", "status": "generating" }),
            ))
            .expect(1)
            .mount(&server)
            .await;

        let result = proxy_for(&server)
            .create_short_crypto(
                "make a short",
                Some("https://example.com"),
                Some("b64payload"),
            )
            .await
            .expect("ok");
        assert_eq!(result["session_id"], "sess-1");
    }

    #[tokio::test]
    async fn create_short_crypto_402_parses_payment_required_header() {
        let server = MockServer::start().await;
        let details = serde_json::json!({
            "chain": "solana",
            "address": "PayHere111",
            "amount": "5",
            "memo": "m-1",
        });
        let encoded = base64::engine::general_purpose::STANDARD
            .encode(serde_json::to_vec(&details).expect("encode"));
        Mock::given(method("POST"))
            .and(path("/shorts/create-crypto"))
            .respond_with(
                ResponseTemplate::new(402).insert_header("Payment-Required", encoded.as_str()),
            )
            .expect(1)
            .mount(&server)
            .await;

        // First call (no tx_signature) → stable payment_required envelope.
        let result = proxy_for(&server)
            .create_short_crypto("make a short", None, None)
            .await
            .expect("402 is a successful envelope, not an error");
        assert_eq!(result["status"], "payment_required");
        assert_eq!(result["payment_details"]["address"], "PayHere111");
        assert_eq!(result["payment_details"]["amount"], "5");
    }

    #[tokio::test]
    async fn create_short_crypto_402_without_header_is_an_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/shorts/create-crypto"))
            .respond_with(ResponseTemplate::new(402))
            .expect(1)
            .mount(&server)
            .await;

        let err = proxy_for(&server)
            .create_short_crypto("p", None, None)
            .await
            .expect_err("missing Payment-Required header must fail");
        match err {
            McpServiceError::OrchestratorError(msg) => {
                assert!(msg.contains("Payment-Required"), "names the header: {msg}")
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[tokio::test]
    async fn create_short_crypto_surfaces_server_error_with_status() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/shorts/create-crypto"))
            .respond_with(
                ResponseTemplate::new(500)
                    .set_body_json(serde_json::json!({ "error": "generation backend down" })),
            )
            .expect(1)
            .mount(&server)
            .await;

        let err = proxy_for(&server)
            .create_short_crypto("p", None, None)
            .await
            .expect_err("500 must fail");
        match err {
            McpServiceError::OrchestratorError(msg) => {
                assert!(msg.contains("500"), "carries the status: {msg}");
                assert!(
                    msg.contains("generation backend down"),
                    "carries body: {msg}"
                );
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[tokio::test]
    async fn get_short_status_fetches_by_session_id() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/shorts/sess-9"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                serde_json::json!({ "status": "done", "video_url": "https://v/x.mp4" }),
            ))
            .expect(1)
            .mount(&server)
            .await;

        let result = proxy_for(&server)
            .get_short_status("sess-9")
            .await
            .expect("ok");
        assert_eq!(result["video_url"], "https://v/x.mp4");
    }

    #[tokio::test]
    async fn get_short_status_rejects_empty_session_id_without_calling() {
        let server = MockServer::start().await;
        // No mock mounted: an HTTP call would 404 and fail differently.
        let err = proxy_for(&server)
            .get_short_status("")
            .await
            .expect_err("empty session_id must be rejected at the boundary");
        assert!(matches!(err, McpServiceError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn get_attestation_fetches_and_parses_the_vow_tuple() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/tasks/task-42/attestation"))
            .respond_with(ResponseTemplate::new(200).set_body_json(full_attestation_json()))
            .expect(1)
            .mount(&server)
            .await;

        let att = proxy_for(&server)
            .get_attestation("task-42", None)
            .await
            .expect("ok");
        assert_eq!(att.version, "vow/v1");
        assert_eq!(att.task_id, "2590886815712535641");
        assert_eq!(att.agent, "AgentWallet1");
        assert_eq!(att.composite_score, "910000");
        let roundtrip = serde_json::to_value(&att).expect("serialize");
        assert_eq!(roundtrip, full_attestation_json());
    }

    #[tokio::test]
    async fn get_attestation_by_pda_uses_the_pda_route() {
        // Must be base58-pubkey-shaped: the proxy rejects short ids at the boundary.
        const PDA: &str = "GtBz1WcJs5tKMLQWaZdd3osYsGzK59onCvbNVPMaQSLU";
        let server = MockServer::start().await;
        let mut body = full_attestation_json();
        body["account"] = serde_json::json!(PDA);
        body.as_object_mut()
            .unwrap()
            .remove("verifier_instructions");
        Mock::given(method("GET"))
            .and(path(format!("/tasks/by-pda/{PDA}/attestation")))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .expect(1)
            .mount(&server)
            .await;

        let att = proxy_for(&server)
            .get_attestation_by_pda(PDA, None)
            .await
            .expect("ok");
        assert_eq!(att.account, PDA);
        assert!(att.verifier_instructions.is_none());
    }

    #[tokio::test]
    async fn get_attestation_surfaces_409_not_yet_attested() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/tasks/task-1/attestation"))
            .respond_with(ResponseTemplate::new(409).set_body_string("task not yet attested"))
            .expect(1)
            .mount(&server)
            .await;

        let err = proxy_for(&server)
            .get_attestation("task-1", None)
            .await
            .expect_err("409 must fail");
        match err {
            McpServiceError::OrchestratorError(msg) => {
                assert!(msg.contains("409"), "carries status: {msg}");
                assert!(msg.contains("not yet attested"), "carries body: {msg}");
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[tokio::test]
    async fn attestation_lookups_reject_empty_identifiers() {
        let server = MockServer::start().await;
        let p = proxy_for(&server);
        assert!(matches!(
            p.get_attestation("", None).await.expect_err("must fail"),
            McpServiceError::InvalidInput(_)
        ));
        assert!(matches!(
            p.get_attestation_by_pda("", None)
                .await
                .expect_err("must fail"),
            McpServiceError::InvalidInput(_)
        ));
    }

    #[tokio::test]
    async fn build_expire_posts_to_build_expire_and_confirms_expire_action() {
        use wiremock::matchers::query_param;

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/tasks/c:exp/build-expire"))
            .and(query_param("network", "devnet"))
            .and(header("authorization", "Bearer test-signed-session"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "message": "ok",
                "task_id": "c:exp",
                "transaction": "AAAA",
            })))
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/tasks/c:exp/confirm"))
            .and(body_partial_json(serde_json::json!({
                "action": "expire",
                "tx_signature": "sig-exp-1",
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "task_id": "c:exp",
                "action": "expire",
                "message": "expired",
            })))
            .expect(1)
            .mount(&server)
            .await;

        let proxy = OrchestratorProxy::new(server.uri(), server.uri())
            .authenticated("test-signed-session".into());
        let built = proxy
            .build_expire("c:exp", "client-wallet", Some("devnet"))
            .await
            .expect("build_expire ok");
        assert_eq!(built.task_id, "c:exp");
        assert_eq!(built.transaction.as_deref(), Some("AAAA"));

        let confirmed = proxy
            .confirm_task(
                "c:exp",
                "client-wallet",
                "sig-exp-1",
                ConfirmAction::Expire,
                None,
                None,
            )
            .await
            .expect("confirm expire ok");
        assert_eq!(confirmed.action, "expire");
    }
}
