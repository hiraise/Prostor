//! Sign-in over HTTP: the setup wizard, session cookies, API tokens, the same-origin rule and
//! throttling, against a real database.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use axum::http::{Method, StatusCode, header};
use serde_json::{Value, json};
use trackflow_server::throttle::{Limits, Throttle};

mod common;

use common::*;

#[tokio::test(flavor = "multi_thread")]
async fn adm_047_the_setup_wizard_runs_once_and_signs_the_administrator_in() {
    let node = Node::start(Throttle::default()).await;
    let bootstrap = node
        .call(Method::GET, "/api/v1/bootstrap", None, None)
        .await;
    assert_eq!(bootstrap.body["setup_required"], true);

    let wrong = node
        .call(
            Method::POST,
            "/api/v1/setup",
            None,
            Some(setup_body("NOPE-NOPE-0000")),
        )
        .await;
    assert_eq!(wrong.status, StatusCode::BAD_REQUEST);
    assert_eq!(wrong.body["fields"][0]["path"], "code");
    assert_eq!(wrong.body["fields"][0]["code"], "incorrect");

    let answer = node
        .call(Method::POST, "/api/v1/setup", None, Some(setup_body(CODE)))
        .await;
    assert_eq!(answer.status, StatusCode::CREATED, "{}", answer.body);
    assert_eq!(answer.body["user"]["username"], "admin");
    assert_eq!(answer.body["remember"], false);
    // The base URL is HTTPS, so the cookie is Secure; without remember-me it has no lifetime.
    let cookie = answer.set_cookie();
    for part in ["HttpOnly", "SameSite=Lax", "Path=/api", "Secure"] {
        assert!(cookie.contains(part), "{cookie}");
    }
    assert!(!cookie.contains("Max-Age"), "{cookie}");
    assert_eq!(
        answer.headers.get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );

    let bootstrap = node
        .call(Method::GET, "/api/v1/bootstrap", None, None)
        .await;
    assert_eq!(bootstrap.body["setup_required"], false);
    assert_eq!(bootstrap.body["base_url"], format!("https://{HOST}"));
    let again = node
        .call(Method::POST, "/api/v1/setup", None, Some(setup_body(CODE)))
        .await;
    assert_eq!(again.status, StatusCode::CONFLICT);
    node.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn auth_001_auth_005_sign_in_use_the_session_and_sign_out() {
    let node = Node::start(Throttle::default()).await;
    node.set_up().await;

    let wrong = node.sign_in("wrong horse", false).await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    assert_eq!(wrong.code(), "invalid_credentials");
    assert!(wrong.session_cookie().is_none());

    let signed_in = node.sign_in("correct horse", false).await;
    assert_eq!(signed_in.status, StatusCode::CREATED);
    let cookie = signed_in.session_cookie().unwrap();

    let session = node
        .call(Method::GET, "/api/v1/session", Some(&cookie), None)
        .await;
    assert_eq!(session.status, StatusCode::OK);
    assert_eq!(session.body["user"]["email"], "anna@example.com");
    let me = node
        .call(Method::GET, "/api/v1/users/me", Some(&cookie), None)
        .await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.body["full_name"], "Anna Admin");

    let out = node
        .call(Method::DELETE, "/api/v1/session", Some(&cookie), None)
        .await;
    assert_eq!(out.status, StatusCode::NO_CONTENT);
    assert!(
        out.set_cookie().contains("Max-Age=0"),
        "{}",
        out.set_cookie()
    );
    let after = node
        .call(Method::GET, "/api/v1/session", Some(&cookie), None)
        .await;
    assert_eq!(after.status, StatusCode::UNAUTHORIZED);
    assert_eq!(after.code(), "unauthenticated");

    let nobody = node.call(Method::GET, "/api/v1/users/me", None, None).await;
    assert_eq!(nobody.status, StatusCode::UNAUTHORIZED);
    let basic = node
        .call(
            Method::GET,
            "/api/v1/users/me",
            Some("Basic YWRtaW46eA=="),
            None,
        )
        .await;
    assert_eq!(basic.status, StatusCode::UNAUTHORIZED);
    node.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn auth_003_remember_me_keeps_the_cookie_for_its_duration() {
    let node = Node::start(Throttle::default()).await;
    node.set_up().await;
    let signed_in = node.sign_in("correct horse", true).await;
    assert_eq!(signed_in.body["remember"], true);
    let cookie = signed_in.set_cookie();
    assert!(
        cookie.contains(&format!("Max-Age={}", 30 * 24 * 60 * 60)),
        "{cookie}"
    );
    node.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn auth_029_auth_030_tokens_call_the_api_but_never_manage_credentials() {
    let node = Node::start(Throttle::default()).await;
    let cookie = node.set_up().await;

    let created = node
        .call(
            Method::POST,
            "/api/v1/users/me/tokens",
            Some(&cookie),
            Some(json!({ "name": "CI", "scopes": ["tracker.read", "tracker.write"] })),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    assert_eq!(created.body["token"]["scopes"], json!(["tracker.write"]));
    assert_eq!(created.body["token"]["last_used_at"], Value::Null);
    let secret = created.body["secret"].as_str().unwrap().to_owned();
    assert!(secret.starts_with("tfp_"));

    let me = node
        .call(Method::GET, "/api/v1/users/me", Some(&secret), None)
        .await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.body["username"], "admin");
    let lowercase = node
        .call(
            Method::GET,
            "/api/v1/users/me",
            Some(&format!("bearer {secret}")),
            None,
        )
        .await;
    assert_eq!(lowercase.status, StatusCode::OK, "the scheme ignores case");

    // Credentials, sessions and tokens are managed only from a signed-in browser.
    for (method, uri, body) in [
        (Method::GET, "/api/v1/session", None),
        (Method::GET, "/api/v1/users/me/tokens", None),
        (
            Method::POST,
            "/api/v1/users/me/tokens",
            Some(json!({ "name": "more", "scopes": ["admin.write"] })),
        ),
        (
            Method::POST,
            "/api/v1/users/me/password",
            Some(json!({ "current_password": "correct horse", "new_password": "stolen pass" })),
        ),
    ] {
        let answer = node.call(method, uri, Some(&secret), body).await;
        assert_eq!(answer.status, StatusCode::FORBIDDEN, "{uri}");
    }

    let list = node
        .call(
            Method::GET,
            "/api/v1/users/me/tokens?limit=10",
            Some(&cookie),
            None,
        )
        .await;
    assert_eq!(list.status, StatusCode::OK);
    assert_eq!(list.body["items"][0]["name"], "CI");
    assert!(list.body["items"][0]["last_used_at"].is_string());
    assert_eq!(list.body["next_cursor"], Value::Null);
    let too_many = node
        .call(
            Method::GET,
            "/api/v1/users/me/tokens?limit=500",
            Some(&cookie),
            None,
        )
        .await;
    assert_eq!(too_many.status, StatusCode::BAD_REQUEST);

    let id = created.body["token"]["id"].as_i64().unwrap();
    let deleted = node
        .call(
            Method::DELETE,
            &format!("/api/v1/users/me/tokens/{id}"),
            Some(&cookie),
            None,
        )
        .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    let revoked = node
        .call(Method::GET, "/api/v1/users/me", Some(&secret), None)
        .await;
    assert_eq!(revoked.status, StatusCode::UNAUTHORIZED);
    node.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn auth_009_password_change_over_http() {
    let node = Node::start(Throttle::default()).await;
    let cookie = node.set_up().await;
    let wrong = node
        .call(
            Method::POST,
            "/api/v1/users/me/password",
            Some(&cookie),
            Some(json!({ "current_password": "nope", "new_password": "brand new pass" })),
        )
        .await;
    assert_eq!(wrong.status, StatusCode::BAD_REQUEST);
    assert_eq!(wrong.body["fields"][0]["path"], "current_password");
    assert_eq!(wrong.body["fields"][0]["code"], "incorrect");
    let changed = node
        .call(
            Method::POST,
            "/api/v1/users/me/password",
            Some(&cookie),
            Some(json!({ "current_password": "correct horse", "new_password": "brand new pass" })),
        )
        .await;
    assert_eq!(changed.status, StatusCode::NO_CONTENT);
    assert_eq!(
        node.sign_in("brand new pass", false).await.status,
        StatusCode::CREATED
    );
    node.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn requests_from_other_sites_are_refused() {
    let node = Node::start(Throttle::default()).await;
    let cookie = node.set_up().await;
    let evil = Some("https://evil.example");
    let body = json!({ "name": "x", "scopes": ["tracker.read"] });

    let refused = node
        .send(
            Method::POST,
            "/api/v1/users/me/tokens",
            Some(&cookie),
            Some(body.clone()),
            evil,
        )
        .await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    let sign_in = node
        .send(
            Method::POST,
            "/api/v1/session",
            None,
            Some(json!({ "login": "admin", "password": "correct horse" })),
            evil,
        )
        .await;
    assert_eq!(sign_in.status, StatusCode::FORBIDDEN);
    let sign_out = node
        .send(Method::DELETE, "/api/v1/session", Some(&cookie), None, evil)
        .await;
    assert_eq!(sign_out.status, StatusCode::FORBIDDEN);

    // Reading is allowed from anywhere: the answer is not readable by the other site anyway.
    let read = node
        .send(Method::GET, "/api/v1/users/me", Some(&cookie), None, evil)
        .await;
    assert_eq!(read.status, StatusCode::OK);
    // Clients that are not browsers send no Origin.
    let script = node
        .send(
            Method::POST,
            "/api/v1/users/me/tokens",
            Some(&cookie),
            Some(body),
            None,
        )
        .await;
    assert_eq!(script.status, StatusCode::CREATED);
    node.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn repeated_failures_are_throttled() {
    let node = Node::start(Throttle::new(Limits {
        per_login: 2,
        per_address: 100,
        setup: 2,
        window: Duration::from_secs(60),
    }))
    .await;
    for _ in 0..2 {
        let wrong = node
            .call(
                Method::POST,
                "/api/v1/setup",
                None,
                Some(setup_body("NOPE-NOPE-0000")),
            )
            .await;
        assert_eq!(wrong.status, StatusCode::BAD_REQUEST);
    }
    let blocked = node
        .call(Method::POST, "/api/v1/setup", None, Some(setup_body(CODE)))
        .await;
    assert_eq!(blocked.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(blocked.code(), "rate_limited");
    let wait: u64 = blocked.headers[header::RETRY_AFTER]
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!((1..=60).contains(&wait), "{wait}");
    node.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn wrong_passwords_are_throttled_per_login() {
    let node = Node::start(Throttle::new(Limits {
        per_login: 2,
        per_address: 100,
        setup: 10,
        window: Duration::from_secs(60),
    }))
    .await;
    node.set_up().await;
    for _ in 0..2 {
        assert_eq!(
            node.sign_in("wrong", false).await.status,
            StatusCode::UNAUTHORIZED
        );
    }
    // Even the right password waits now: guessing must not tell when it succeeded.
    let blocked = node.sign_in("correct horse", false).await;
    assert_eq!(blocked.status, StatusCode::TOO_MANY_REQUESTS);
    node.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn ui_018_preferences_over_http() {
    let node = Node::start(Throttle::default()).await;
    let cookie = node.set_up().await;
    let call = |method: Method, body: Option<Value>| {
        let node = &node;
        let cookie = cookie.clone();
        async move {
            node.call(method, "/api/v1/users/me/preferences", Some(&cookie), body)
                .await
        }
    };
    let empty = call(Method::GET, None).await;
    assert_eq!(empty.status, StatusCode::OK, "{}", empty.body);
    assert_eq!(empty.body, json!({}));
    // Unknown keys are ignored; given keys are set.
    let saved = call(
        Method::PATCH,
        Some(json!({ "home": "projects", "issue_panel_width": 240, "future_key": 1 })),
    )
    .await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.body);
    assert_eq!(
        saved.body,
        json!({ "home": "projects", "issue_panel_width": 240 })
    );
    // `null` resets a key, the others stay.
    let reset = call(
        Method::PATCH,
        Some(json!({ "home": null, "list_sidebar_folded": ["tags"] })),
    )
    .await;
    assert_eq!(
        reset.body,
        json!({ "issue_panel_width": 240, "list_sidebar_folded": ["tags"] })
    );
    let bad = call(Method::PATCH, Some(json!({ "issue_panel_width": 999 }))).await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    assert_eq!(bad.body["fields"][0]["path"], "issue_panel_width");
    assert_eq!(bad.body["fields"][0]["code"], "out_of_range");
    let wrong = call(Method::PATCH, Some(json!({ "home": "dashboards" }))).await;
    assert_eq!(wrong.status, StatusCode::BAD_REQUEST);
    assert_eq!(call(Method::GET, None).await.body, reset.body);
    node.stop().await;
}
