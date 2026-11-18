//! HTTP behavior of the router with in-memory web assets and no database: the node stays in the
//! "starting" phase. Behavior with a database: `startup.rs`.
// Test helpers outside `#[test]` functions may panic: a panic is a failed test.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum::response::Response;
use http_body_util::BodyExt;
use taskmaster_server::lifecycle::Phase;
use taskmaster_server::web::Assets;
use taskmaster_server::{AppState, router};
use tower::ServiceExt;

struct MemoryAssets(HashMap<&'static str, &'static str>);

impl Assets for MemoryAssets {
    fn get(&self, path: &str) -> Option<Cow<'static, [u8]>> {
        self.0.get(path).map(|text| Cow::Borrowed(text.as_bytes()))
    }
}

const SHELL: &str = "<!doctype html><html><body><div id=\"app\"></div></body></html>";

/// A node that never connects: the pool is lazy and points at a closed port.
fn state_with(files: &[(&'static str, &'static str)]) -> AppState {
    let options = taskmaster_store::connect_options("postgres://nobody@127.0.0.1:9/none").unwrap();
    AppState::new(
        Arc::new(MemoryAssets(files.iter().copied().collect())),
        taskmaster_store::pool(&options, 2),
    )
}

fn app_with(files: &[(&'static str, &'static str)]) -> axum::Router {
    router(state_with(files))
}

fn app() -> axum::Router {
    app_with(&[
        ("index.html", SHELL),
        ("favicon.svg", "<svg/>"),
        ("_app/immutable/entry.abc123.js", "console.log(1)"),
    ])
}

async fn get(app: axum::Router, uri: &str) -> Response {
    app.oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap()
}

async fn text(response: Response) -> String {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

fn header_of(response: &Response, name: header::HeaderName) -> &str {
    response
        .headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
}

async fn json_of(response: Response) -> serde_json::Value {
    serde_json::from_str(&text(response).await).unwrap()
}

#[tokio::test]
async fn adm_033_liveness_answers_while_the_node_starts() {
    let response = get(app(), "/api/v1/health/live").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(header_of(&response, header::CONTENT_TYPE).starts_with("application/json"));
    assert_eq!(header_of(&response, header::CACHE_CONTROL), "no-store");
    let body = json_of(response).await;
    assert_eq!(body["status"], "live");
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
}

#[tokio::test]
async fn adm_033_readiness_fails_until_startup_is_complete() {
    let state = state_with(&[]);
    for (phase, expected) in [
        (Phase::Starting, "starting"),
        (Phase::Migrating, "migrating"),
        (Phase::Stopping, "stopping"),
    ] {
        state.lifecycle.set(phase);
        let response = get(router(state.clone()), "/api/v1/health/ready").await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = json_of(response).await;
        assert_eq!(body["status"], expected);
        assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
    }
}

#[tokio::test]
async fn adm_033_readiness_fails_when_the_database_does_not_answer() {
    let state = state_with(&[]);
    state.lifecycle.set(Phase::Ready);
    let response = get(router(state), "/api/v1/health/ready").await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(json_of(response).await["status"], "database_unavailable");
}

#[tokio::test]
async fn endpoints_that_need_the_database_wait_for_a_ready_node() {
    let response = get(app(), "/api/v1/bootstrap").await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = json_of(response).await;
    assert_eq!(body["code"], "unavailable");
    assert!(body["message"].as_str().unwrap().contains("starting"));
}

#[tokio::test]
async fn unknown_api_paths_answer_json_404_not_the_shell() {
    for uri in ["/api/v1/nope", "/api/v2/health", "/api", "/api/"] {
        let response = get(app(), uri).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{uri}");
        assert!(
            header_of(&response, header::CONTENT_TYPE).starts_with("application/json"),
            "{uri}"
        );
        let body: serde_json::Value = serde_json::from_str(&text(response).await).unwrap();
        assert_eq!(body["code"], "not_found", "{uri}");
    }
}

#[tokio::test]
async fn app_routes_get_the_shell_uncached() {
    for uri in [
        "/",
        "/issues/ABC-12",
        "/boards/3?sprint=current",
        "/index.html",
    ] {
        let response = get(app(), uri).await;
        assert_eq!(response.status(), StatusCode::OK, "{uri}");
        assert!(
            header_of(&response, header::CONTENT_TYPE).starts_with("text/html"),
            "{uri}"
        );
        assert_eq!(
            header_of(&response, header::CACHE_CONTROL),
            "no-cache",
            "{uri}"
        );
        assert_eq!(text(response).await, SHELL, "{uri}");
    }
}

#[tokio::test]
async fn the_shell_refuses_framing_and_cross_site_referrers() {
    let response = get(app(), "/issues/ABC-12").await;
    assert_eq!(
        header_of(&response, header::CONTENT_SECURITY_POLICY),
        "frame-ancestors 'none'"
    );
    assert_eq!(header_of(&response, header::REFERRER_POLICY), "same-origin");
    assert_eq!(
        header_of(&response, header::X_CONTENT_TYPE_OPTIONS),
        "nosniff"
    );
}

#[tokio::test]
async fn hashed_bundle_files_are_cached_forever() {
    let response = get(app(), "/_app/immutable/entry.abc123.js").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(header_of(&response, header::CONTENT_TYPE).contains("javascript"));
    assert_eq!(
        header_of(&response, header::CACHE_CONTROL),
        "public, max-age=31536000, immutable"
    );
    assert_eq!(
        header_of(&response, header::X_CONTENT_TYPE_OPTIONS),
        "nosniff"
    );
}

#[tokio::test]
async fn other_files_are_served_with_revalidation() {
    let response = get(app(), "/favicon.svg").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(header_of(&response, header::CONTENT_TYPE), "image/svg+xml");
    assert_eq!(header_of(&response, header::CACHE_CONTROL), "no-cache");
}

#[tokio::test]
async fn missing_bundle_file_is_404_not_the_shell() {
    let response = get(app(), "/_app/immutable/gone.js").await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn unbuilt_web_app_answers_503_with_a_hint() {
    let response = get(app_with(&[]), "/").await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(text(response).await.contains("make web"));
}

#[tokio::test]
async fn responses_are_compressed_when_the_client_accepts_it() {
    const BIG: &str = concat!(
        "<!doctype html><html><body>",
        "taskmaster taskmaster taskmaster taskmaster taskmaster taskmaster taskmaster taskmaster taskmaster taskmaster ",
        "taskmaster taskmaster taskmaster taskmaster taskmaster taskmaster taskmaster taskmaster taskmaster taskmaster ",
        "</body></html>"
    );
    let request = Request::get("/")
        .header(header::ACCEPT_ENCODING, "br")
        .body(Body::empty())
        .unwrap();
    let response = app_with(&[("index.html", BIG)])
        .oneshot(request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(header_of(&response, header::CONTENT_ENCODING), "br");
}

#[tokio::test]
async fn unsupported_methods_answer_405_in_the_error_format() {
    let request = Request::post("/api/v1/health/live")
        .body(Body::empty())
        .unwrap();
    let response = app().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(json_of(response).await["code"], "method_not_allowed");
}

#[tokio::test]
async fn every_response_carries_a_request_id() {
    let first = get(app(), "/api/v1/health/live").await;
    let second = get(app(), "/").await;
    let first_id = header_of(&first, "x-request-id".parse().unwrap()).to_owned();
    let second_id = header_of(&second, "x-request-id".parse().unwrap()).to_owned();
    assert_eq!(first_id.len(), 32, "{first_id}");
    assert_ne!(first_id, second_id);
}

#[tokio::test]
async fn a_valid_incoming_request_id_is_kept_and_a_bad_one_replaced() {
    for (sent, kept) in [("proxy-1234.abc", true), ("has spaces", false)] {
        let request = Request::get("/api/v1/health/live")
            .header("x-request-id", sent)
            .body(Body::empty())
            .unwrap();
        let response = app().oneshot(request).await.unwrap();
        let id = header_of(&response, "x-request-id".parse().unwrap());
        assert_eq!(id == sent, kept, "{sent} -> {id}");
    }
}

#[tokio::test]
async fn the_api_document_is_served() {
    let response = get(app(), "/api/v1/openapi.json").await;
    assert_eq!(response.status(), StatusCode::OK);
    let document = json_of(response).await;
    assert_eq!(document["servers"][0]["url"], "/api/v1");
    assert!(document["paths"]["/health/ready"]["get"].is_object());
    assert!(document["components"]["schemas"]["ErrorBody"].is_object());
}
