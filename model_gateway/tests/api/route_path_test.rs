//! The Conversations routes answer a path parameter axum cannot decode (a
//! percent-encoded id that is not UTF-8) in the gateway's error envelope,
//! `400 invalid_request` with axum's own words, where the plain `Path`
//! extractor answered `text/plain`. Storage-backed: no worker is needed.

use axum::{
    body::Body,
    extract::Request,
    http::{header::CONTENT_TYPE, Method, StatusCode},
};
use serde_json::{json, Value};
use smg::config::RouterConfig;
use tower::ServiceExt;

use crate::common::AppTestContext;

async fn app_over_no_workers() -> (AppTestContext, axum::Router) {
    let mut config = RouterConfig::builder()
        .regular_mode(vec![])
        .random_policy()
        .host("127.0.0.1")
        .port(3015)
        .max_payload_size(256 * 1024 * 1024)
        .request_timeout_secs(600)
        .worker_startup_timeout_secs(1)
        .worker_startup_check_interval_secs(1)
        .max_concurrent_requests(64)
        .queue_size(0)
        .queue_timeout_secs(60)
        .build_unchecked();
    config.health_check.disable_health_check = true;
    let ctx = AppTestContext::new_with_config(config, vec![]).await;
    let app = ctx.create_app();
    (ctx, app)
}

/// One call of the app: the status, the content type and the body as text
/// (a null `body` sends none).
#[expect(clippy::unwrap_used, reason = "test helper: failures should panic")]
async fn call(
    app: &axum::Router,
    method: Method,
    uri: &str,
    body: Value,
) -> (StatusCode, String, String) {
    let request = if body.is_null() {
        Request::builder()
            .method(method)
            .uri(uri)
            .body(Body::empty())
            .unwrap()
    } else {
        Request::builder()
            .method(method)
            .uri(uri)
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap()
    };
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        content_type,
        String::from_utf8_lossy(&bytes).into_owned(),
    )
}

/// The envelope's `error.code` and `error.message` of a JSON body.
fn error_of(body: &str) -> (String, String) {
    let json: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let error = &json["error"];
    (
        error["code"].as_str().unwrap_or_default().to_string(),
        error["message"].as_str().unwrap_or_default().to_string(),
    )
}

#[tokio::test]
async fn a_conversation_id_that_is_not_utf8_is_refused_in_the_envelope() {
    let (_ctx, app) = app_over_no_workers().await;
    let (status, content_type, body) =
        call(&app, Method::GET, "/v1/conversations/%00%ff", Value::Null).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        content_type.starts_with("application/json"),
        "content type {content_type:?}, body {body:?}"
    );
    let (code, message) = error_of(&body);
    assert_eq!(code, "invalid_request", "{body}");
    assert_eq!(
        message, "Invalid URL: Invalid UTF-8 in `conversation_id`",
        "{body}"
    );
}

#[tokio::test]
async fn an_item_id_that_is_not_utf8_is_refused_in_the_envelope() {
    let (_ctx, app) = app_over_no_workers().await;
    let (status, content_type, body) = call(
        &app,
        Method::DELETE,
        "/v1/conversations/conv_items/items/%00%ff",
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        content_type.starts_with("application/json"),
        "content type {content_type:?}, body {body:?}"
    );
    let (code, message) = error_of(&body);
    assert_eq!(code, "invalid_request", "{body}");
    assert!(
        message.starts_with("Invalid URL: Invalid UTF-8 in "),
        "{body}"
    );
}

#[tokio::test]
async fn the_path_is_refused_before_the_body_is_read() {
    let (_ctx, app) = app_over_no_workers().await;
    let (status, content_type, body) = call(
        &app,
        Method::POST,
        "/v1/conversations/%00%ff/items",
        json!({"items": [{"type": "message", "role": "user", "content": "hi"}]}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        content_type.starts_with("application/json"),
        "content type {content_type:?}, body {body:?}"
    );
    let (code, message) = error_of(&body);
    assert_eq!(code, "invalid_request", "{body}");
    assert_eq!(
        message, "Invalid URL: Invalid UTF-8 in `conversation_id`",
        "{body}"
    );
}

#[tokio::test]
async fn a_missing_conversation_still_answers_not_found_in_the_envelope() {
    let (_ctx, app) = app_over_no_workers().await;
    let (status, content_type, body) = call(
        &app,
        Method::GET,
        "/v1/conversations/conv_missing",
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert!(
        content_type.starts_with("application/json"),
        "content type {content_type:?}, body {body:?}"
    );
    let (code, _message) = error_of(&body);
    assert_eq!(code, "not_found", "{body}");
}
