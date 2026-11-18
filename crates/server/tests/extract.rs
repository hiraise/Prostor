//! Rejected request bodies and query strings answer in the API error format.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum::response::{Json, Response};
use axum::routing::{get, post};
use http_body_util::BodyExt;
use prostor_api::page::{MAX_LIMIT, PageQuery};
use prostor_server::error::{ApiError, JsonBody, QueryParams};
use serde::Deserialize;
use serde_json::{Value, json};
use tower::ServiceExt;

#[derive(Deserialize)]
struct Input {
    name: String,
}

async fn echo(JsonBody(input): JsonBody<Input>) -> String {
    input.name
}

async fn page(QueryParams(query): QueryParams<PageQuery>) -> Result<Json<Value>, ApiError> {
    let page = query.resolve::<i64>(MAX_LIMIT).map_err(ApiError::invalid)?;
    Ok(Json(json!({ "limit": page.limit, "after": page.after })))
}

fn app() -> Router {
    Router::new()
        .route("/echo", post(echo))
        .route("/page", get(page))
}

async fn send(request: Request<Body>) -> (StatusCode, Value) {
    let response: Response = app().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn post_json(body: impl Into<Body>, content_type: &str) -> Request<Body> {
    Request::post("/echo")
        .header(header::CONTENT_TYPE, content_type)
        .body(body.into())
        .unwrap()
}

#[tokio::test]
async fn bad_json_bodies_are_invalid_requests() {
    for body in ["{not json", r#"{"title": "no name"}"#, r#"{"name": 5}"#] {
        let (status, error) = send(post_json(body, "application/json")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(error["code"], "invalid_request", "{body}");
        assert!(!error["message"].as_str().unwrap().is_empty());
    }
}

#[tokio::test]
async fn a_body_that_is_not_json_is_unsupported() {
    let (status, error) = send(post_json(r#"{"name": "x"}"#, "text/plain")).await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_eq!(error["code"], "unsupported_media_type");
}

#[tokio::test]
async fn an_oversized_body_is_refused() {
    let body = format!(r#"{{"name": "{}"}}"#, "x".repeat(3 * 1024 * 1024));
    let (status, error) = send(post_json(body, "application/json")).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(error["code"], "payload_too_large");
}

#[tokio::test]
async fn int_056_page_parameters_are_validated() {
    let get = |uri: &str| Request::get(uri).body(Body::empty()).unwrap();

    let (status, body) = send(get("/page")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "limit": 50, "after": null }));

    let (status, error) = send(get("/page?limit=abc")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error["code"], "invalid_request");

    let (status, error) = send(get("/page?limit=0&cursor=%25%25")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let paths: Vec<&str> = error["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|field| field["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, ["limit", "cursor"]);
}
