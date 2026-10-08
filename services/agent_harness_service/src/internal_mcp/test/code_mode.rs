use super::*;
use agent_code_mode::{
    domain::{ExecutionId, ExecutionStore, RecordedCallStatus},
    outbound::{postgres::PgExecutionStore, tools::ToolsetDispatcher},
};
use ai_toolset::{AsyncTool, ServiceContext, ToolAnnotated, ToolAnnotations, ToolResult};
use async_trait::async_trait;
use code_execution::{
    domain::{ExecutionService, Limits},
    outbound::{client::RunnerClient, deno::DenoRunner},
    protocol::ServiceToken,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::Barrier;

#[derive(Clone)]
struct TestContext {
    gate: Arc<Barrier>,
}

#[derive(Deserialize, JsonSchema)]
#[schemars(
    title = "LookupFixture",
    description = "Look up one fixture using the authorized owner."
)]
struct LookupFixture {
    #[schemars(description = "The fixture to look up.")]
    value: String,
}

#[derive(Serialize, JsonSchema)]
struct LookupResult {
    value: String,
    owner: String,
}

impl ToolAnnotated for LookupFixture {
    const ANNOTATIONS: ToolAnnotations = ToolAnnotations::read_only("Look up fixture");
}

#[async_trait]
impl AsyncTool<TestContext> for LookupFixture {
    type Output = LookupResult;
    async fn call(
        &self,
        context: ServiceContext<TestContext>,
        request: RequestContext,
    ) -> ToolResult<Self::Output> {
        // Serial dispatch cannot pass: both real tool futures must be in flight.
        context.gate.wait().await;
        Ok(LookupResult {
            value: self.value.clone(),
            owner: request.user_id.to_string(),
        })
    }
}

#[sqlx::test(migrator = "macro_db_migrator::MACRO_DB_MIGRATIONS")]
#[ignore = "requires pinned Deno runtime"]
async fn authenticated_mcp_runs_typescript_parallel_tools_and_persists_ui_results(
    pool: sqlx::PgPool,
) {
    sqlx::raw_sql(include_str!(
        "../../../../../crates/agent_code_mode/src/outbound/postgres/test/sessions.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::raw_sql("UPDATE agent_session SET turn_state = 'running', turn_action_id = '00000000-0000-0000-0000-000000000003'").execute(&pool).await.unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let path = tokio::process::Command::new("which")
        .arg("deno")
        .output()
        .await
        .unwrap();
    assert!(path.status.success(), "Deno 2.9.6 must be on PATH");
    let deno = DenoRunner::new(
        String::from_utf8(path.stdout).unwrap().trim().into(),
        scratch.path().to_owned(),
        128,
    )
    .await
    .unwrap();
    let runner = ExecutionService::new(Arc::new(deno), Limits::default()).unwrap();
    let token = ServiceToken::new("test-code-mode-runner-token-32-chars".into()).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client = RunnerClient::new(
        format!("ws://{}/v1/execute", listener.local_addr().unwrap()),
        token.clone(),
        4,
        8,
    )
    .unwrap();
    let runner_app = code_execution::inbound::router(runner.clone(), token, 8);
    let server = tokio::spawn(async move { axum::serve(listener, runner_app).await.unwrap() });
    let store = Arc::new(PgExecutionStore::new(pool.clone()));
    let code_mode = Arc::new(agent_code_mode::domain::CodeModeService::new(
        Some(Arc::new(client)),
        Arc::new(ToolsetDispatcher::new(
            Arc::new(AsyncToolCollection::new().add_tool::<LookupFixture, TestContext>()),
            TestContext {
                gate: Arc::new(Barrier::new(2)),
            },
            |context, _| context.clone(),
        )),
        store.clone(),
        Arc::new(agent_code_mode::outbound::turns::SessionTurns(
            agent_session::outbound::postgres::PgAgentSessionRepo::new(
                pool.clone(),
                entity_registry_db_utils::OwnedEntityRegistrar::new(
                    entity_registry::OwnerGrantPolicy::new(
                        bots::outbound::pg_bots_repo::PgBotsRepo::new(pool.clone()),
                    ),
                ),
            ),
        )),
    ));
    let repo = InMemoryAgentSessionRepo::new();
    let session = test_agent_session(AgentSessionId::new_from_uuid(macro_uuid::Uuid::from_u128(
        1,
    )));
    repo.insert_session(session.clone());
    repo.set_egress_token_hash(session.id, &SessionToken::new("session-secret").hash())
        .await
        .unwrap();
    let app = router(
        Arc::new(repo.clone()),
        Arc::new(SessionPullRequestService::new(
            repo.clone(),
            RecordingRealtime::new(),
        )),
        "localhost".into(),
        Some(code_mode.clone()),
    );
    assert_eq!(
        rpc(app.clone(), Some("wrong-session"), "tools/list", json!({}))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let (_, listed) = rpc(app.clone(), Some("session-secret"), "tools/list", json!({})).await;
    let names: Vec<_> = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"ExecuteCode") && names.contains(&"DescribeCodeTools"));
    let (_, docs) = rpc(
        app.clone(),
        Some("session-secret"),
        "tools/call",
        json!({"name": "DescribeCodeTools", "arguments": {"names": ["LookupFixture"]}}),
    )
    .await;
    let doc = &docs["result"]["structuredContent"]["tools"][0];
    assert_eq!(doc["name"], "LookupFixture");
    assert_eq!(
        doc["output_schema"]["$defs"]["LookupResult"]["properties"]["owner"]["type"],
        "string"
    );
    let source = "const values: string[] = ['first', 'second']; const rows = await Promise.all(values.map(value => sdk.LookupFixture({value}))); return rows.map(row => row.value);";
    let (_, response) = rpc(app.clone(), Some("session-secret"), "tools/call", json!({"name": "ExecuteCode", "arguments": {"execution_id": ExecutionId::mint(), "source": source}})).await;
    assert_ne!(response["result"]["isError"], true, "{response}");
    let receipt = &response["result"]["structuredContent"];
    assert_eq!(receipt["result"], json!(["first", "second"]), "{response}");
    assert!(receipt.get("calls").is_none());
    // Text-only MCP clients retain the receipt too (Claude's native transport).
    let text: serde_json::Value =
        serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(&text, receipt);
    let execution =
        ExecutionId::from_uuid(receipt["executionId"].as_str().unwrap().parse().unwrap());
    let stored = store.get(session.id, execution).await.unwrap();
    assert_eq!(stored.calls.len(), 2);
    for call in stored.calls {
        assert_eq!(call.status, RecordedCallStatus::Completed);
        assert_eq!(
            call.output.unwrap()["owner"],
            session.owner_user().unwrap().to_string()
        );
    }
    let (_, failed) = rpc(app.clone(), Some("session-secret"), "tools/call", json!({"name": "ExecuteCode", "arguments": {"execution_id": ExecutionId::mint(), "source": "await sdk.SendEmail({}); return true;"}})).await;
    assert_ne!(failed["result"]["isError"], true, "{failed}");
    assert_eq!(failed["result"]["structuredContent"]["status"], "failed");
    // A Stop processed on another replica changes shared session state. The
    // stateless MCP request itself stays connected and never sends cancellation.
    let cancelled_id = ExecutionId::mint();
    let running = tokio::spawn(async move {
        rpc(app, Some("session-secret"), "tools/call", json!({"name": "ExecuteCode", "arguments": {
            "execution_id": cancelled_id,
            "source": "await Promise.all([sdk.LookupFixture({value:'one'}), sdk.LookupFixture({value:'two'})]); await new Promise(r => setTimeout(r, 10000)); await sdk.LookupFixture({value:'must not run'}); return true;"
        }})).await
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Ok(record) = store.get(session.id, cancelled_id).await
                && record.calls.len() == 2
                && record
                    .calls
                    .iter()
                    .all(|call| call.status == RecordedCallStatus::Completed)
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    sqlx::raw_sql("UPDATE agent_session SET turn_state = 'stopping'")
        .execute(&pool)
        .await
        .unwrap();
    let (_, stopped) = tokio::time::timeout(std::time::Duration::from_secs(3), running)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stopped["result"]["structuredContent"]["status"], "cancelled",
        "{stopped}"
    );
    let recovered = store.get(session.id, cancelled_id).await.unwrap();
    assert_eq!(recovered.calls.len(), 2);
    assert!(
        recovered
            .calls
            .iter()
            .all(|call| call.status == RecordedCallStatus::Completed)
    );
    runner.shutdown().await;
    assert_eq!(std::fs::read_dir(scratch.path()).unwrap().count(), 0);
    server.abort();
}
