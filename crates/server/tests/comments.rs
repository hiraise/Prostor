//! Comments, the activity stream and previews over HTTP (step 1.4a).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::http::{Method, StatusCode};
use prostor_server::throttle::Throttle;
use serde_json::{Value, json};

mod common;

use common::*;

#[tokio::test(flavor = "multi_thread")]
async fn cnt_001_cnt_005_hist_006_comments_over_http() {
    let node = Node::start(Throttle::default()).await;
    let admin = node.set_up().await;
    let call = |method: Method, path: String, body: Option<Value>| {
        let node = &node;
        let admin = admin.clone();
        async move { node.call(method, &path, Some(&admin), body).await }
    };
    let web = call(
        Method::POST,
        "/api/v1/projects".into(),
        Some(json!({ "template": "blank", "name": "Web", "key": "WEB" })),
    )
    .await;
    let web_id = web.body["id"].as_i64().unwrap();
    let issue = call(
        Method::POST,
        "/api/v1/issues".into(),
        Some(json!({ "project": web_id, "summary": "Login fails", "description": "See WEB-1" })),
    )
    .await;
    assert_eq!(issue.status, StatusCode::CREATED, "{}", issue.body);
    let id = issue.body["issue"]["id"].as_i64().unwrap();
    // The description comes rendered, with its references.
    assert!(
        issue.body["issue"]["description_html"]
            .as_str()
            .unwrap()
            .contains("tf-issue:WEB-1")
    );
    assert_eq!(
        issue.body["issue"]["refs"]["issues"][0]["readable_id"],
        "WEB-1"
    );

    let posted = call(
        Method::POST,
        format!("/api/v1/issues/{id}/comments"),
        Some(json!({ "text": "Thanks, @admin" })),
    )
    .await;
    assert_eq!(posted.status, StatusCode::CREATED, "{}", posted.body);
    let comment = posted.body["comment"]["id"].as_i64().unwrap();
    assert_eq!(
        posted.body["comment"]["refs"]["users"][0]["username"],
        "admin"
    );
    assert_eq!(posted.body["comment"]["updated_at"], Value::Null);
    let empty = call(
        Method::POST,
        format!("/api/v1/issues/{id}/comments"),
        Some(json!({ "text": " " })),
    )
    .await;
    assert_eq!(empty.status, StatusCode::BAD_REQUEST);
    assert_eq!(empty.body["fields"][0]["path"], "text");

    let edited = call(
        Method::PATCH,
        format!("/api/v1/comments/{comment}"),
        Some(json!({ "text": "Thank you", "version": posted.body["comment"]["version"] })),
    )
    .await;
    assert_eq!(edited.status, StatusCode::OK, "{}", edited.body);
    assert_eq!(edited.body["html"], "<p>Thank you</p>\n");
    let stale = call(
        Method::PATCH,
        format!("/api/v1/comments/{comment}"),
        Some(json!({ "text": "x", "version": posted.body["comment"]["version"] })),
    )
    .await;
    assert_eq!(stale.status, StatusCode::CONFLICT);

    let activity = call(Method::GET, format!("/api/v1/issues/{id}/activity"), None).await;
    assert_eq!(activity.status, StatusCode::OK, "{}", activity.body);
    let items = activity.body["items"].as_array().unwrap();
    assert_eq!(items[0]["type"], "comment");
    assert_eq!(items[0]["comment"]["text"], "Thank you");
    assert_eq!(items[1]["type"], "change");
    assert_eq!(items[1]["kind"], "created");
    assert_eq!(activity.body["older"], Value::Null);

    let deleted = call(Method::DELETE, format!("/api/v1/comments/{comment}"), None).await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    let placeholder = call(Method::GET, format!("/api/v1/comments/{comment}"), None).await;
    assert_eq!(placeholder.body["text"], "");
    assert_eq!(placeholder.body["deleted"]["by"]["username"], "admin");
    assert_eq!(placeholder.body["can"]["restore"], true);
    let restored = call(
        Method::POST,
        format!("/api/v1/comments/{comment}/restore"),
        None,
    )
    .await;
    assert_eq!(restored.status, StatusCode::OK);
    assert_eq!(restored.body["text"], "Thank you");

    let around = call(
        Method::GET,
        format!("/api/v1/issues/{id}/activity?around_comment={comment}&changes=false"),
        None,
    )
    .await;
    assert_eq!(around.body["items"][0]["comment"]["id"], comment);
    let neither = call(
        Method::GET,
        format!("/api/v1/issues/{id}/activity?changes=false&comments=false"),
        None,
    )
    .await;
    assert_eq!(neither.status, StatusCode::BAD_REQUEST);

    let candidates = call(
        Method::GET,
        format!("/api/v1/issues/{id}/mention-candidates?q=ad"),
        None,
    )
    .await;
    assert_eq!(candidates.body["users"][0]["username"], "admin");
}

#[tokio::test(flavor = "multi_thread")]
async fn cnt_038_previews_render_as_stored() {
    let node = Node::start(Throttle::default()).await;
    let admin = node.set_up().await;
    let preview = node
        .call(
            Method::POST,
            "/api/v1/markdown",
            Some(&admin),
            Some(json!({ "text": "# Hi\n\n<script>x</script>\n\nHello @admin" })),
        )
        .await;
    assert_eq!(preview.status, StatusCode::OK, "{}", preview.body);
    let html = preview.body["html"].as_str().unwrap();
    assert!(html.starts_with("<h1>Hi</h1>"));
    assert!(html.contains("&lt;script&gt;"));
    assert!(html.contains("tf-user:"));
    assert_eq!(preview.body["refs"]["users"][0]["username"], "admin");
    let basic = node
        .call(
            Method::POST,
            "/api/v1/markdown",
            Some(&admin),
            Some(json!({ "text": "@admin", "profile": "basic" })),
        )
        .await;
    assert_eq!(basic.body["html"], "<p>@admin</p>\n");
    let unknown = node
        .call(
            Method::POST,
            "/api/v1/markdown",
            Some(&admin),
            Some(json!({ "text": "x", "issue": 999 })),
        )
        .await;
    assert_eq!(unknown.status, StatusCode::NOT_FOUND);
    let anonymous = node
        .call(
            Method::POST,
            "/api/v1/markdown",
            None,
            Some(json!({ "text": "x" })),
        )
        .await;
    assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
}
