use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Method, Request, header};
use chrono::Utc;
use http_body_util::BodyExt;
use macro_authorization::{
    InternalIdentityClaims, MacroAuthorizationError, MacroAuthorizationService,
};
use macro_user_id::user_id::MacroUserIdStr;
use model_user::UserContext;
use rootcause::Report;
use serde_json::json;
use tower::ServiceExt;

use super::*;

const USER_ID: &str = "macro|user-kv-router@macro.com";
const VALID_JWT: &str = "valid";

#[derive(Clone)]
struct FakeAuthorizationService;

impl MacroAuthorizationService for FakeAuthorizationService {
    async fn authorize(&self, jwt: &str) -> Result<UserContext, Report<MacroAuthorizationError>> {
        if jwt != VALID_JWT {
            return Err(Report::new(MacroAuthorizationError::InvalidCredentials));
        }
        Ok(UserContext {
            user_id: USER_ID.to_string(),
            fusion_user_id: "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb".to_string(),
            permissions: None,
            organization_id: None,
        })
    }

    async fn authorize_internal(
        &self,
        _provided_key: &str,
        _claims: InternalIdentityClaims,
    ) -> Result<Option<UserContext>, Report<MacroAuthorizationError>> {
        Err(Report::new(MacroAuthorizationError::InvalidCredentials))
    }
}

/// Builds the error a failing fake service returns.
type MakeError = fn() -> UserKvError;

/// What the fake service should answer, and a log of what it was asked.
#[derive(Clone, Default)]
struct FakeUserKvService {
    calls: Arc<Mutex<Vec<String>>>,
    fail_with: Arc<Mutex<Option<MakeError>>>,
}

impl FakeUserKvService {
    fn failing(error: MakeError) -> Self {
        let service = Self::default();
        *service.fail_with.lock().unwrap() = Some(error);
        service
    }

    fn record(&self, call: String) -> Result<(), UserKvError> {
        self.calls.lock().unwrap().push(call);
        match *self.fail_with.lock().unwrap() {
            Some(error) => Err(error()),
            None => Ok(()),
        }
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

fn entry(namespace: &KvNamespace, key: &KvKey, value: KvValue) -> UserKvEntry {
    UserKvEntry {
        namespace: namespace.clone(),
        key: key.clone(),
        value,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

impl UserKvService for FakeUserKvService {
    async fn list_entries(
        &self,
        user_id: &MacroUserIdStr<'_>,
        namespace: &KvNamespace,
    ) -> Result<Vec<UserKvEntry>, UserKvError> {
        self.record(format!("list {} {namespace}", user_id.as_ref()))?;
        Ok(vec![entry(
            namespace,
            &KvKey::parse("calendar")?,
            KvValue::new(),
        )])
    }

    async fn get_entry(
        &self,
        user_id: &MacroUserIdStr<'_>,
        namespace: &KvNamespace,
        key: &KvKey,
    ) -> Result<UserKvEntry, UserKvError> {
        self.record(format!("get {} {namespace}/{key}", user_id.as_ref()))?;
        Ok(entry(namespace, key, KvValue::new()))
    }

    async fn put_entry(
        &self,
        user_id: &MacroUserIdStr<'_>,
        namespace: &KvNamespace,
        key: &KvKey,
        value: KvValue,
    ) -> Result<UserKvEntry, UserKvError> {
        self.record(format!(
            "put {} {namespace}/{key} {}",
            user_id.as_ref(),
            serde_json::Value::Object(value.clone())
        ))?;
        Ok(entry(namespace, key, value))
    }

    async fn delete_entry(
        &self,
        user_id: &MacroUserIdStr<'_>,
        namespace: &KvNamespace,
        key: &KvKey,
    ) -> Result<(), UserKvError> {
        self.record(format!("delete {} {namespace}/{key}", user_id.as_ref()))
    }
}

fn router(service: FakeUserKvService) -> axum::Router {
    user_kv_router(UserKvRouterState::new(
        Arc::new(service),
        MacroAuthorizationState::new(Arc::new(FakeAuthorizationService)),
    ))
}

async fn send(
    service: &FakeUserKvService,
    method: Method,
    uri: &str,
    body: Option<serde_json::Value>,
    authorized: bool,
) -> (StatusCode, serde_json::Value) {
    let mut request = Request::builder().method(method).uri(uri);
    if authorized {
        request = request.header(header::AUTHORIZATION, format!("Bearer {VALID_JWT}"));
    }
    let request = match body {
        Some(body) => request
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string())),
        None => request.body(Body::empty()),
    }
    .expect("request should build");

    let response = router(service.clone())
        .oneshot(request)
        .await
        .expect("router should respond");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body should collect")
        .to_bytes();
    // Axum's own rejections (a malformed body) are plain text; keep them
    // as a string so callers can still assert on the status.
    let json = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or_else(|_| {
            serde_json::Value::String(String::from_utf8_lossy(&bytes).into_owned())
        })
    };
    (status, json)
}

#[tokio::test]
async fn routes_reach_the_service_for_the_signed_in_user() {
    let service = FakeUserKvService::default();

    let (status, body) = send(&service, Method::GET, "/tours", None, true).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["entries"][0]["key"], "calendar");

    let (status, body) = send(&service, Method::GET, "/tours/calendar", None, true).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["namespace"], "tours");
    assert!(body["createdAt"].is_string(), "camelCase timestamps");

    let (status, body) = send(
        &service,
        Method::PUT,
        "/tours/calendar",
        Some(json!({ "value": { "status": "completed" } })),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["value"], json!({ "status": "completed" }));

    let (status, _) = send(&service, Method::DELETE, "/tours/calendar", None, true).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    assert_eq!(
        service.calls(),
        [
            format!("list {USER_ID} tours"),
            format!("get {USER_ID} tours/calendar"),
            format!(r#"put {USER_ID} tours/calendar {{"status":"completed"}}"#),
            format!("delete {USER_ID} tours/calendar"),
        ]
    );
}

#[tokio::test]
async fn unauthenticated_requests_never_reach_the_service() {
    let service = FakeUserKvService::default();
    let (status, _) = send(&service, Method::GET, "/tours", None, false).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(service.calls().is_empty());
}

#[tokio::test]
async fn invalid_slugs_and_non_object_values_are_rejected_at_the_edge() {
    let service = FakeUserKvService::default();

    let (status, body) = send(&service, Method::GET, "/Tours", None, true).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body["message"]
            .as_str()
            .unwrap()
            .contains("invalid namespace")
    );

    let (status, _) = send(
        &service,
        Method::PUT,
        "/tours/calendar",
        Some(json!({ "value": [1, 2, 3] })),
        true,
    )
    .await;
    assert!(status.is_client_error(), "arrays aren't objects: {status}");

    assert!(service.calls().is_empty());
}

#[tokio::test]
async fn domain_errors_map_to_http_statuses() {
    let cases: [(MakeError, StatusCode); 4] = [
        (|| UserKvError::NotFound, StatusCode::NOT_FOUND),
        (
            || UserKvError::ValueTooLarge {
                size: 20_000,
                limit: 16_384,
            },
            StatusCode::PAYLOAD_TOO_LARGE,
        ),
        (
            || UserKvError::EntryLimitReached { limit: 1000 },
            StatusCode::BAD_REQUEST,
        ),
        (
            || UserKvError::Internal(rootcause::report!("boom")),
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
    ];
    for (error, expected) in cases {
        let service = FakeUserKvService::failing(error);
        let (status, body) = send(&service, Method::GET, "/tours/calendar", None, true).await;
        assert_eq!(status, expected);
        if expected == StatusCode::INTERNAL_SERVER_ERROR {
            assert_eq!(
                body["message"], "internal server error",
                "no internals leak"
            );
        }
    }
}
