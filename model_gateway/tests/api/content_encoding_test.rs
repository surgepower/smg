#![allow(clippy::unwrap_used)]
//! A JSON body under a `Content-Encoding` the gateway does not decode is a
//! 415 in the route's envelope before the body is parsed: the public one on
//! the data plane, the control plane's on the management routes.

use axum::{
    body::Body,
    http::{
        header::{CONTENT_ENCODING, CONTENT_TYPE},
        Request, StatusCode,
    },
};
use tower::ServiceExt;

use crate::common::{
    mock_worker::{HealthStatus, MockWorkerConfig, WorkerType},
    AppTestContext,
};

/// The status and the body of a POST of `body` to `uri` under `encoding`.
async fn post(
    app: &axum::Router,
    uri: &str,
    encoding: &str,
    body: &'static str,
) -> (StatusCode, serde_json::Value) {
    let req = Request::builder()
        .method("POST")
        .uri(uri)
        .header(CONTENT_TYPE, "application/json")
        .header(CONTENT_ENCODING, encoding)
        .body(Body::from(body))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| serde_json::json!({"not_json": String::from_utf8_lossy(&bytes)}));
    (status, body)
}

/// A compressed body is a 415 naming the encoding, in the public envelope
/// on the chat route (`json_parse_error`) and in the control plane's on the
/// worker route (`UNSUPPORTED_MEDIA_TYPE`); `identity` is no encoding and
/// the body is read as before.
#[tokio::test]
async fn a_compressed_body_is_a_415_in_the_envelope_on_both_planes() {
    let ctx = AppTestContext::new(vec![MockWorkerConfig {
        port: 18134,
        worker_type: WorkerType::Regular,
        health_status: HealthStatus::Healthy,
        response_delay_ms: 0,
        fail_rate: 0.0,
    }])
    .await;
    let app = ctx.create_app();
    let chat = r#"{"model":"mock-model-1","messages":[{"role":"user","content":"hi"}]}"#;

    let (status, body) = post(&app, "/v1/chat/completions", "gzip", chat).await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE, "{body}");
    assert_eq!(body["error"]["code"], "json_parse_error", "{body}");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap_or("")
            .contains("gzip"),
        "{body}"
    );

    let (status, body) = post(&app, "/workers", "br", r#"{"url":"http://127.0.0.1:1"}"#).await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE, "{body}");
    assert_eq!(body["code"], "UNSUPPORTED_MEDIA_TYPE", "{body}");
    assert!(
        body["error"].as_str().unwrap_or("").contains("br"),
        "{body}"
    );

    let (status, body) = post(&app, "/v1/chat/completions", "identity", chat).await;
    assert_ne!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE, "{body}");
    ctx.shutdown().await;
}
