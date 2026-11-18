//! The inbox over HTTP (step 1.10a-3, plan §6.3): folders, threads with their inline rows, the
//! read state, the unread summary and the `inbox:` live topic with its permission check.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use taskmaster_server::throttle::Throttle;
use time::OffsetDateTime;

mod common;

use common::live::connect;
use common::*;

/// An administrator, Bob in the team of project WEB, and issues in it.
struct World {
    node: Node,
    admin: String,
    bob: String,
    bob_id: i64,
    web: i64,
}

impl World {
    async fn new() -> Self {
        let node = Node::start(Throttle::default()).await;
        let admin = node.set_up().await;
        let web = node
            .call(
                Method::POST,
                "/api/v1/projects",
                Some(&admin),
                Some(json!({ "template": "blank", "name": "Web", "key": "WEB" })),
            )
            .await;
        assert_eq!(web.status, StatusCode::CREATED, "{}", web.body);
        let web = web.body["id"].as_i64().unwrap();
        let bob = node
            .call(
                Method::POST,
                "/api/v1/users",
                Some(&admin),
                Some(json!({
                    "full_name": "Bob Brown", "username": "bob", "email": "bob@example.com",
                    "password": "bob's password",
                })),
            )
            .await;
        assert_eq!(bob.status, StatusCode::CREATED, "{}", bob.body);
        let bob_id = bob.body["id"].as_i64().unwrap();
        let team = node
            .call(
                Method::POST,
                &format!("/api/v1/projects/{web}/team"),
                Some(&admin),
                Some(json!({ "users": [bob_id] })),
            )
            .await;
        assert_eq!(team.status, StatusCode::OK, "{}", team.body);
        let session = node
            .call(
                Method::POST,
                "/api/v1/session",
                None,
                Some(json!({ "login": "bob", "password": "bob's password" })),
            )
            .await;
        let bob = session.session_cookie().unwrap();
        Self {
            node,
            admin,
            bob,
            bob_id,
            web,
        }
    }

    async fn admin_call(&self, method: Method, path: &str, body: Option<Value>) -> Answer {
        self.node.call(method, path, Some(&self.admin), body).await
    }

    async fn bob_call(&self, method: Method, path: &str, body: Option<Value>) -> Answer {
        self.node.call(method, path, Some(&self.bob), body).await
    }

    /// An issue Bob watches (a star is a watch, ISS-030).
    async fn watched_issue(&self, summary: &str) -> i64 {
        let created = self
            .admin_call(
                Method::POST,
                "/api/v1/issues",
                Some(json!({ "project": self.web, "summary": summary })),
            )
            .await;
        assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
        let id = created.body["issue"]["id"].as_i64().unwrap();
        // The creation batch fires before the star: candidates are resolved at fire time.
        self.fire().await;
        let star = self
            .bob_call(Method::PUT, &format!("/api/v1/issues/{id}/star"), None)
            .await;
        assert_eq!(star.status, StatusCode::NO_CONTENT, "{}", star.body);
        id
    }

    /// A comment of the administrator that mentions Bob, then the sweep firing everything.
    async fn mention(&self, issue: i64) {
        let posted = self
            .admin_call(
                Method::POST,
                &format!("/api/v1/issues/{issue}/comments"),
                Some(json!({ "text": "Could you look, @bob?" })),
            )
            .await;
        assert_eq!(posted.status, StatusCode::CREATED, "{}", posted.body);
        self.fire().await;
    }

    /// Fires the sweep past the batching window, as its task would.
    async fn fire(&self) {
        taskmaster_service::notifications::sweep(
            &self.node.state.db,
            OffsetDateTime::now_utc() + Duration::from_secs(70),
            &taskmaster_service::mail::Outgoing::default(),
        )
        .await
        .unwrap();
    }
}

/// Folders, threads, read state and the summary over HTTP; the `system` folder is the
/// administration area's (NTF-048).
#[tokio::test(flavor = "multi_thread")]
async fn ntf_048_ntf_049_ntf_050_inbox_folders_threads_and_read_state() {
    let world = World::new().await;
    let first = world.watched_issue("Login fails").await;
    world.mention(first).await;

    // All folders hold the row; the thread head carries the row and its inline rows.
    let all = world
        .bob_call(Method::GET, "/api/v1/notifications", None)
        .await;
    assert_eq!(all.status, StatusCode::OK, "{}", all.body);
    assert_eq!(all.body["threads"].as_array().unwrap().len(), 1);
    let thread = &all.body["threads"][0];
    let head = &thread["head"];
    assert_eq!(head["kind"], "comment");
    assert_eq!(head["issue_id"], first);
    assert_eq!(head["author"]["username"], "admin");
    assert_eq!(head["content"]["issue"]["readable_id"], "WEB-1");
    let lines = head["content"]["lines"].as_array().unwrap();
    assert_eq!(lines[0]["t"], "comment");
    assert!(
        lines[0]["excerpt"]
            .as_str()
            .unwrap()
            .contains("Could you look")
    );
    let mut reasons = head["reasons"].as_array().unwrap().clone();
    reasons.sort_by_key(|r| r.as_str().unwrap_or_default().to_owned());
    assert_eq!(reasons, vec![json!("mention"), json!("watch")]);
    assert!(!head["read"].as_bool().unwrap());
    // The newest rows inline, the head first (NTF-049).
    assert_eq!(thread["latest"].as_array().unwrap().len(), 1);
    assert_eq!(thread["latest"][0]["id"], head["id"]);

    // Direct holds the mention; subscriptions does not; the whole thread comes with `issue`.
    let direct = world
        .bob_call(Method::GET, "/api/v1/notifications?folder=direct", None)
        .await;
    assert_eq!(direct.status, StatusCode::OK, "{}", direct.body);
    assert_eq!(direct.body["threads"].as_array().unwrap().len(), 1);
    let subscriptions = world
        .bob_call(
            Method::GET,
            "/api/v1/notifications?folder=subscriptions",
            None,
        )
        .await;
    assert_eq!(
        subscriptions.status,
        StatusCode::OK,
        "{}",
        subscriptions.body
    );
    assert_eq!(subscriptions.body["threads"].as_array().unwrap().len(), 0);
    let thread_view = world
        .bob_call(
            Method::GET,
            &format!("/api/v1/notifications?issue={first}"),
            None,
        )
        .await;
    assert_eq!(thread_view.status, StatusCode::OK, "{}", thread_view.body);
    assert_eq!(thread_view.body["threads"].as_array().unwrap().len(), 1);
    assert_eq!(thread_view.body["threads"][0]["head"]["id"], head["id"]);

    // The system folder is part of the administration area (NTF-048).
    let denied = world
        .bob_call(Method::GET, "/api/v1/notifications?folder=system", None)
        .await;
    assert_eq!(denied.status, StatusCode::FORBIDDEN, "{}", denied.body);
    let allowed = world
        .admin_call(Method::GET, "/api/v1/notifications?folder=system", None)
        .await;
    assert_eq!(allowed.status, StatusCode::OK, "{}", allowed.body);
    let bad = world
        .bob_call(Method::GET, "/api/v1/notifications?folder=other", None)
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST, "{}", bad.body);

    // The summary counts the unread rows of every folder (the badge).
    let summary = world
        .bob_call(Method::GET, "/api/v1/notifications/summary", None)
        .await;
    assert_eq!(summary.status, StatusCode::OK, "{}", summary.body);
    assert_eq!(
        summary.body,
        json!({ "total": 1, "direct": 1, "subscriptions": 0, "system": 0 })
    );

    // Read state (NTF-050): the whole thread, one row back to unread, everything read.
    let read = world
        .bob_call(
            Method::POST,
            "/api/v1/notifications/read",
            Some(json!({ "issue": first })),
        )
        .await;
    assert_eq!(read.status, StatusCode::OK, "{}", read.body);
    assert_eq!(
        read.body,
        json!({ "total": 0, "direct": 0, "subscriptions": 0, "system": 0 })
    );
    let unread_only = world
        .bob_call(Method::GET, "/api/v1/notifications?unread=only", None)
        .await;
    assert_eq!(unread_only.body["threads"].as_array().unwrap().len(), 0);
    let row = head["id"].as_i64().unwrap();
    let unread = world
        .bob_call(
            Method::POST,
            "/api/v1/notifications/read",
            Some(json!({ "ids": [row], "unread": true })),
        )
        .await;
    assert_eq!(unread.body["total"], 1);
    let again = world
        .bob_call(
            Method::POST,
            "/api/v1/notifications/read",
            Some(json!({ "all": true })),
        )
        .await;
    assert_eq!(again.body["total"], 0);
    let none = world
        .bob_call(Method::POST, "/api/v1/notifications/read", Some(json!({})))
        .await;
    assert_eq!(none.status, StatusCode::BAD_REQUEST, "{}", none.body);

    // Pages are keyset pages of thread heads, newest first (NTF-049).
    let second = world.watched_issue("Chart missing").await;
    world.mention(second).await;
    let page = world
        .bob_call(Method::GET, "/api/v1/notifications?limit=1", None)
        .await;
    assert_eq!(page.body["threads"].as_array().unwrap().len(), 1);
    assert_eq!(page.body["threads"][0]["head"]["issue_id"], second);
    let cursor = page.body["next"].as_i64().unwrap();
    let older = world
        .bob_call(
            Method::GET,
            &format!("/api/v1/notifications?limit=1&before={cursor}"),
            None,
        )
        .await;
    assert_eq!(older.body["threads"].as_array().unwrap().len(), 1);
    assert_eq!(older.body["threads"][0]["head"]["issue_id"], first);
    assert_eq!(older.body["next"], Value::Null);
}

/// The `inbox:` topic takes only its owner's notifications, and open tabs hear new rows and
/// read changes (plan §3.6).
#[tokio::test(flavor = "multi_thread")]
async fn ntf_049_inbox_topic_reaches_only_its_owner() {
    let world = World::new().await;
    let issue = world.watched_issue("Login fails").await;
    let address = world.node.serve().await;
    let mut live = connect(address, Some(&world.bob)).await;
    let mine = format!("inbox:{}", world.bob_id);
    let answered = live.subscribe(&[&mine, "inbox:1"], None).await;
    let subscribed = answered.iter().find(|m| m["type"] == "subscribed").unwrap()["topics"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(subscribed, vec![json!(mine)]);
    let denied = answered.iter().find(|m| m["type"] == "denied").unwrap()["topics"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(denied, vec![json!("inbox:1")]);

    // A fired batch reaches the reader's open tab without a fetch.
    world.mention(issue).await;
    let event = live.next().await;
    assert_eq!(event["type"], "event", "{event}");
    assert_eq!(event["topics"], json!([mine]));
    assert_eq!(event["entity"]["kind"], "inbox");
    assert_eq!(event["entity"]["id"], world.bob_id);
    assert_eq!(event["change"], "created");
    assert_eq!(event["patch"]["kind"], "notification");
    assert_eq!(event["patch"]["event"], "comment");
    assert_eq!(event["patch"]["issue"], "WEB-1");
    assert_eq!(event["patch"]["author"], "Anna Admin");
    assert!(
        event["patch"]["excerpt"]
            .as_str()
            .unwrap()
            .contains("Could you look")
    );
    let reasons = event["patch"]["reasons"].as_array().unwrap().clone();
    assert!(reasons.contains(&json!("mention")));

    // A read change answers the new counts (NTF-050).
    let read = world
        .bob_call(
            Method::POST,
            "/api/v1/notifications/read",
            Some(json!({ "all": true })),
        )
        .await;
    assert_eq!(read.status, StatusCode::OK, "{}", read.body);
    let event = live.next().await;
    assert_eq!(event["type"], "event", "{event}");
    assert_eq!(
        event["patch"],
        json!({ "kind": "read", "total": 0, "direct": 0, "subscriptions": 0, "system": 0 })
    );
}
