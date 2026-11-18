//! Cross-origin access to `/api/v1` (INT-056, plan step 1.12 §7.2): the origins the
//! `api.allowed_origins` setting lists get preflight answers and readable answers; nobody else
//! does, and the default (an empty list) changes nothing. No answer ever allows credentials, so
//! cookie sessions stay same-origin however the list reads.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::http::{Method, StatusCode, header};
use prostor_server::throttle::Throttle;
use serde_json::json;

mod common;

use common::*;

/// How long a settings reload may take to reach the node's cache (the bus answers in
/// milliseconds; this only keeps a loaded machine from failing the test).
const RELOAD: std::time::Duration = std::time::Duration::from_secs(2);

#[tokio::test]
async fn int_056_default_is_same_origin_only() {
    let node = Node::start(Throttle::default()).await;

    // A foreign origin on a plain request gets no header: the browser keeps the answer to
    // itself, exactly as before the setting existed.
    let answer = node
        .send(
            Method::GET,
            "/api/v1/bootstrap",
            None,
            None,
            Some("https://app.example.com"),
        )
        .await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
    assert!(
        answer
            .headers
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none()
    );

    // Its preflight is nobody's: no CORS answer, the browser sees an error (405, the API's
    // answer to a method a route does not take).
    let (status, headers, _) = node
        .raw(
            Method::OPTIONS,
            "/api/v1/bootstrap",
            None,
            &[
                (header::ORIGIN, "https://app.example.com"),
                (header::ACCESS_CONTROL_REQUEST_METHOD, "GET"),
            ],
            Vec::new(),
        )
        .await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    assert!(headers.get(header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none());

    node.stop().await;
}

#[tokio::test]
async fn int_056_listed_origin_passes_preflight_and_request() {
    let node = Node::start(Throttle::default()).await;
    let admin = node.set_up().await;

    // The administrator lists one origin; the screen's answer carries it stored.
    let put = node
        .call(
            Method::PUT,
            "/api/v1/admin/settings",
            Some(&admin),
            Some(json!({ "api.allowed_origins": ["https://app.example.com"] })),
        )
        .await;
    assert_eq!(put.status, StatusCode::OK, "{}", put.body);
    let listed = put.body["settings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|setting| setting["key"] == "api.allowed_origins")
        .unwrap();
    assert_eq!(listed["value"], json!(["https://app.example.com"]));
    assert_eq!(listed["is_stored"], true);

    // The preflight answers what a token-bearing script's browser asks — never credentials.
    let (status, headers, _) = node
        .raw(
            Method::OPTIONS,
            "/api/v1/users/me",
            None,
            &[
                (header::ORIGIN, "https://app.example.com"),
                (header::ACCESS_CONTROL_REQUEST_METHOD, "GET"),
                (
                    header::ACCESS_CONTROL_REQUEST_HEADERS,
                    "authorization, content-type",
                ),
            ],
            Vec::new(),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(
        headers.get(header::ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(),
        "https://app.example.com"
    );
    assert_eq!(
        headers.get(header::ACCESS_CONTROL_ALLOW_METHODS).unwrap(),
        "GET, HEAD, POST, PUT, PATCH, DELETE, OPTIONS"
    );
    assert_eq!(
        headers.get(header::ACCESS_CONTROL_ALLOW_HEADERS).unwrap(),
        "authorization, content-type"
    );
    assert!(
        headers
            .get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
            .is_none()
    );
    assert_eq!(headers.get(header::VARY).unwrap(), "Origin");

    // A plain request from the listed origin carries the readable-by header, once the setting
    // has reached the node's cache.
    let mut readable = false;
    let deadline = std::time::Instant::now() + RELOAD;
    while std::time::Instant::now() < deadline {
        let answer = node
            .send(
                Method::GET,
                "/api/v1/bootstrap",
                None,
                None,
                Some("https://app.example.com"),
            )
            .await;
        assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
        readable = answer
            .headers
            .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN);
        if readable {
            assert_eq!(
                answer
                    .headers
                    .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                    .unwrap(),
                "https://app.example.com"
            );
            assert!(
                answer
                    .headers
                    .get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
                    .is_none()
            );
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(readable, "the listed origin never became readable");

    // An origin the list does not name gets nothing, list or no list; a request without an
    // Origin header — same-origin — never changes either.
    let other = node
        .send(
            Method::GET,
            "/api/v1/bootstrap",
            None,
            None,
            Some("https://other.example"),
        )
        .await;
    assert_eq!(other.status, StatusCode::OK);
    assert!(
        other
            .headers
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none()
    );
    let same = node
        .call(Method::GET, "/api/v1/bootstrap", None, None)
        .await;
    assert_eq!(same.status, StatusCode::OK);
    assert!(
        same.headers
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none()
    );

    node.stop().await;
}
