//! The live gateway over a real socket (step 1.6): sign-in and origin, topics and their access
//! checks, events after HTTP changes, notices, replay after a reconnect, ping, malformed messages
//! and shutdown.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::future::IntoFuture;
use std::net::SocketAddr;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use taskmaster_api::live::close;
use taskmaster_server::lifecycle::Phase;
use taskmaster_server::live::Config;
use taskmaster_server::throttle::Throttle;
use tokio_tungstenite::tungstenite;

mod common;

use common::live::{Live, connect, try_connect};
use common::*;

/// An administrator, Bob in the team of project WEB, and one issue in it.
struct World {
    node: Node,
    address: SocketAddr,
    admin: String,
    admin_id: i64,
    bob: String,
    bob_id: i64,
    web: i64,
    issue: i64,
    estimation: i64,
}

impl World {
    async fn new() -> Self {
        Self::on(Node::start(Throttle::default()).await).await
    }

    async fn on(node: Node) -> Self {
        let admin = node.set_up().await;
        let me = node
            .call(Method::GET, "/api/v1/users/me", Some(&admin), None)
            .await;
        let admin_id = me.body["id"].as_i64().unwrap();
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
        let mut world = Self {
            address: node.serve().await,
            node,
            admin,
            admin_id,
            bob,
            bob_id,
            web,
            issue: 0,
            estimation: 0,
        };
        let created = world
            .create(json!({ "project": web, "summary": "Login fails" }))
            .await;
        world.issue = created["id"].as_i64().unwrap();
        world.estimation = created["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["field"]["name"] == "Estimation")
            .unwrap()["field"]["id"]
            .as_i64()
            .unwrap();
        world
    }

    async fn admin_call(&self, method: Method, path: &str, body: Option<Value>) -> Answer {
        self.node.call(method, path, Some(&self.admin), body).await
    }

    async fn create(&self, body: Value) -> Value {
        let created = self
            .admin_call(Method::POST, "/api/v1/issues", Some(body))
            .await;
        assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
        created.body["issue"].clone()
    }

    /// The administrator changes the issue's estimation; answers its new version.
    async fn estimate(&self, issue: i64, value: &str) -> i64 {
        let changed = self
            .admin_call(
                Method::PUT,
                &format!("/api/v1/issues/{issue}/fields/{}", self.estimation),
                Some(json!({ "value": value })),
            )
            .await;
        assert_eq!(changed.status, StatusCode::OK, "{}", changed.body);
        changed.body["issue"]["version"].as_i64().unwrap()
    }

    /// Another project of the administrator; Bob is not in it.
    async fn project(&self, name: &str, key: &str) -> i64 {
        let created = self
            .admin_call(
                Method::POST,
                "/api/v1/projects",
                Some(json!({ "template": "blank", "name": name, "key": key })),
            )
            .await;
        assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
        created.body["id"].as_i64().unwrap()
    }

    async fn live(&self, credential: &str) -> Live {
        connect(self.address, Some(credential)).await
    }

    /// A connection of the administrator subscribed to the issue: when it got an event, the pump
    /// has dispatched it to everyone.
    async fn watcher(&self) -> Live {
        let mut admin = self.live(&self.admin).await;
        admin
            .subscribe(&[&format!("issue:{}", self.issue)], None)
            .await;
        admin
    }
}

fn topic(issue: i64) -> String {
    format!("issue:{issue}")
}

#[tokio::test(flavor = "multi_thread")]
async fn iss_048_an_editor_change_reaches_a_viewer() {
    let world = World::new().await;
    // Bob listens with an API token, as a script would.
    let token = world
        .node
        .call(
            Method::POST,
            "/api/v1/users/me/tokens",
            Some(&world.bob),
            Some(json!({ "name": "Live", "scopes": ["tracker.read"] })),
        )
        .await;
    assert_eq!(token.status, StatusCode::CREATED, "{}", token.body);
    let mut bob = world.live(token.body["secret"].as_str().unwrap()).await;
    let answer = bob.subscribe(&[&topic(world.issue)], None).await;
    assert_eq!(answer.len(), 1, "{answer:?}");
    assert_eq!(answer[0]["topics"], json!([topic(world.issue)]));
    assert!(answer[0]["head"].as_i64().unwrap() > 0);

    let version = world.estimate(world.issue, "2h").await;
    let event = bob.next().await;
    assert_eq!(
        event,
        json!({
            "type": "event",
            "id": event["id"],
            "topics": [topic(world.issue)],
            "entity": { "kind": "issue", "id": world.issue, "version": version },
            "change": "updated",
            "patch": null,
        })
    );
    assert!(event["id"].as_i64().unwrap() > answer[0]["head"].as_i64().unwrap());
}

#[tokio::test(flavor = "multi_thread")]
async fn iss_048_one_message_for_several_topics() {
    let world = World::new().await;
    let other = world
        .create(json!({ "project": world.web, "summary": "Other" }))
        .await["id"]
        .as_i64()
        .unwrap();
    let types = world
        .admin_call(Method::GET, "/api/v1/link-types", None)
        .await;
    let related = types.body["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["builtin"] == "related")
        .unwrap()["id"]
        .as_i64()
        .unwrap();
    let mut bob = world.live(&world.bob).await;
    bob.subscribe(&[&topic(world.issue), &topic(other)], None)
        .await;

    let added = world
        .admin_call(
            Method::POST,
            &format!("/api/v1/issues/{}/links", world.issue),
            Some(json!({ "link_type": related, "side": "source", "issues": [other] })),
        )
        .await;
    assert_eq!(added.status, StatusCode::OK, "{}", added.body);
    let mut links = Vec::new();
    while links.is_empty() {
        let event = bob.next().await;
        if event["entity"]["kind"] == "issue_link" {
            links.push(event);
        }
    }
    links.extend(
        bob.drain()
            .await
            .into_iter()
            .filter(|e| e["entity"]["kind"] == "issue_link"),
    );
    assert_eq!(links.len(), 1, "{links:?}");
    let mut topics: Vec<String> = links[0]["topics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.as_str().unwrap().to_owned())
        .collect();
    topics.sort();
    let mut expected = vec![topic(world.issue), topic(other)];
    expected.sort();
    assert_eq!(topics, expected);
    assert_eq!(links[0]["change"], "created");
}

#[tokio::test(flavor = "multi_thread")]
async fn acc_049_a_hidden_issue_is_denied_and_its_events_never_arrive() {
    let world = World::new().await;
    let hidden = world
        .create(json!({
            "project": world.web, "summary": "Salaries",
            "audience": { "users": [world.admin_id] },
        }))
        .await["id"]
        .as_i64()
        .unwrap();
    let mut bob = world.live(&world.bob).await;
    // Hidden, missing and misspelled topics are denied alike.
    bob.send(json!({
        "type": "subscribe",
        "topics": [topic(hidden), "issue:999999", "issue:01", "board:1", "list:2,1"],
        "since": null,
    }))
    .await;
    assert_eq!(
        bob.next().await,
        json!({
            "type": "denied",
            "topics": [topic(hidden), "issue:999999", "issue:01", "board:1", "list:2,1"],
        })
    );
    world.estimate(hidden, "1h").await;
    bob.assert_quiet().await;

    // Bob follows a visible issue; it is then limited to the administrator.
    let mut admin = world.watcher().await;
    bob.subscribe(&[&topic(world.issue)], None).await;
    let limited = world
        .admin_call(
            Method::PATCH,
            &format!("/api/v1/issues/{}", world.issue),
            Some(json!({ "audience": { "users": [world.admin_id] } })),
        )
        .await;
    assert_eq!(limited.status, StatusCode::OK, "{}", limited.body);
    assert_eq!(admin.next().await["type"], "event");
    world.estimate(world.issue, "2h").await;
    assert_eq!(admin.next().await["type"], "event");
    bob.assert_quiet().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn fld_062_a_private_field_change_carries_no_value() {
    let world = World::new().await;
    let fields = world
        .admin_call(
            Method::GET,
            &format!("/api/v1/projects/{}/fields", world.web),
            None,
        )
        .await;
    let attached = fields.body["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["field"]["id"] == world.estimation)
        .unwrap()
        .clone();
    let private = world
        .admin_call(
            Method::PATCH,
            &format!("/api/v1/projects/{}/fields/{}", world.web, world.estimation),
            Some(json!({ "private": true, "version": attached["version"] })),
        )
        .await;
    assert_eq!(private.status, StatusCode::OK, "{}", private.body);
    let mut bob = world.live(&world.bob).await;
    bob.subscribe(&[&topic(world.issue)], None).await;

    let version = world.estimate(world.issue, "3h").await;
    let event = bob.next().await;
    assert_eq!(event["entity"]["version"], version);
    assert_eq!(event["patch"], Value::Null);
    let keys: Vec<&String> = event.as_object().unwrap().keys().collect();
    assert_eq!(keys.len(), 6, "{event}");
}

#[tokio::test(flavor = "multi_thread")]
async fn adm_002_a_setting_change_reaches_open_connections() {
    let world = World::new().await;
    let mut bob = world.live(&world.bob).await;
    let answer = bob.subscribe(&["instance"], None).await;
    let head = answer[0]["head"].as_i64().unwrap();

    let mut conn = world.node.state.db.acquire().await.unwrap();
    taskmaster_store::bus::notify(
        &mut conn,
        taskmaster_store::bus::SETTINGS,
        "system_language",
    )
    .await
    .unwrap();
    assert_eq!(
        bob.next().await,
        json!({
            "type": "event",
            "id": head,
            "topics": ["instance"],
            "entity": { "kind": "settings", "id": 0, "version": 0 },
            "change": "updated",
            "patch": null,
        })
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn live_access_change_applies_before_the_next_event() {
    let world = World::new().await;
    let mut admin = world.watcher().await;
    let mut bob = world.live(&world.bob).await;
    bob.subscribe(&[&topic(world.issue), "instance"], None)
        .await;
    world.estimate(world.issue, "1h").await;
    assert_eq!(bob.next().await["type"], "event");

    let removed = world
        .admin_call(
            Method::POST,
            &format!("/api/v1/projects/{}/team/remove", world.web),
            Some(json!({ "users": [world.bob_id] })),
        )
        .await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.body);
    // Bob's tab learns that access changed …
    let notice = bob.next().await;
    assert_eq!(notice["entity"]["kind"], "access", "{notice}");
    // … and the next change of the issue, which Bob no longer sees, does not reach him.
    admin.drain().await;
    world.estimate(world.issue, "2h").await;
    assert_eq!(admin.next().await["type"], "event");
    bob.assert_quiet().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn live_replay_after_reconnect() {
    let world = World::new().await;
    let mut admin = world.watcher().await;
    let mut bob = world.live(&world.bob).await;
    bob.subscribe(&[&topic(world.issue)], None).await;
    world.estimate(world.issue, "1h").await;
    let seen = bob.next().await["id"].as_i64().unwrap();
    admin.next().await;
    drop(bob);

    let second = world.estimate(world.issue, "2h").await;
    let third = world.estimate(world.issue, "3h").await;
    let missed = [admin.next().await, admin.next().await];

    let mut bob = world.live(&world.bob).await;
    let answer = bob.subscribe(&[&topic(world.issue)], Some(seen)).await;
    assert_eq!(answer.len(), 3, "{answer:?}");
    for (got, (sent, version)) in answer.iter().zip(missed.iter().zip([second, third])) {
        assert_eq!(got["id"], sent["id"]);
        assert_eq!(got["entity"]["version"], version);
    }
    assert_eq!(answer[2]["type"], "subscribed");
    assert_eq!(answer[2]["head"], missed[1]["id"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn live_resync_when_since_is_too_old() {
    // The replay buffer keeps two events.
    let world = World::on(
        Node::start_live(Config {
            buffer_max: 2,
            ..Config::default()
        })
        .await,
    )
    .await;
    let mut admin = world.watcher().await;
    let mut bob = world.live(&world.bob).await;
    bob.subscribe(&[&topic(world.issue), "instance"], None)
        .await;
    world.estimate(world.issue, "1h").await;
    let seen = bob.next().await["id"].as_i64().unwrap();
    admin.next().await;
    drop(bob);
    for value in ["2h", "3h", "4h"] {
        world.estimate(world.issue, value).await;
        admin.next().await;
    }

    let mut bob = world.live(&world.bob).await;
    let answer = bob
        .subscribe(&[&topic(world.issue), "instance"], Some(seen))
        .await;
    assert_eq!(answer.len(), 2, "{answer:?}");
    // Notices are never replayed, so only the issue needs a reload.
    assert_eq!(
        answer[0],
        json!({ "type": "resync", "topics": [topic(world.issue)] })
    );
    assert_eq!(answer[1]["type"], "subscribed");
}

#[tokio::test(flavor = "multi_thread")]
async fn live_cross_site_handshake_is_refused() {
    let world = World::new().await;
    let refused = try_connect(
        world.address,
        Some(&world.bob),
        Some("https://evil.example"),
    )
    .await;
    match refused {
        Err(tungstenite::Error::Http(response)) => {
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
        }
        other => panic!("expected a refusal, got {:?}", other.map(|_| ())),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn live_without_sign_in_is_refused() {
    let world = World::new().await;
    match try_connect(world.address, None, None).await {
        Err(tungstenite::Error::Http(response)) => {
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }
        other => panic!("expected a refusal, got {:?}", other.map(|_| ())),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn live_ping_pong() {
    let world = World::new().await;
    let mut bob = world.live(&world.bob).await;
    bob.send(json!({ "type": "ping" })).await;
    assert_eq!(bob.next().await, json!({ "type": "pong" }));
}

#[tokio::test(flavor = "multi_thread")]
async fn live_malformed_message_closes_with_policy() {
    let world = World::new().await;
    let mut bob = world.live(&world.bob).await;
    bob.send_text("nonsense").await;
    assert_eq!(bob.closed().await, close::POLICY);
    let mut bob = world.live(&world.bob).await;
    bob.send(json!({ "type": "shout" })).await;
    assert_eq!(bob.closed().await, close::POLICY);
}

#[tokio::test(flavor = "multi_thread")]
async fn live_shutdown_closes_with_restart() {
    let world = World::new().await;
    // Served as the binary serves: requests drain after the stop signal.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let stop = tokio_util::sync::CancellationToken::new();
    let app = taskmaster_server::router(world.node.state.clone())
        .into_make_service_with_connect_info::<SocketAddr>();
    let server = tokio::spawn(
        axum::serve(listener, app)
            .with_graceful_shutdown(stop.clone().cancelled_owned())
            .into_future(),
    );
    let mut bob = connect(address, Some(&world.bob)).await;
    bob.subscribe(&[&topic(world.issue)], None).await;

    stop.cancel();
    world.node.state.lifecycle.set(Phase::Stopping);
    world.node.state.live.stop();
    assert_eq!(bob.closed().await, close::RESTART);
    // An open live connection does not hold the server up.
    tokio::time::timeout(std::time::Duration::from_secs(5), server)
        .await
        .expect("the server stops")
        .unwrap()
        .unwrap();
    // New connections are refused while the node stops.
    match try_connect(
        world.address,
        Some(&world.bob),
        Some(&format!("https://{HOST}")),
    )
    .await
    {
        Err(tungstenite::Error::Http(response)) => {
            assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        }
        other => panic!("expected a refusal, got {:?}", other.map(|_| ())),
    }
}

/// The next event's topics, sorted; other messages are skipped.
async fn next_topics(live: &mut Live) -> Vec<String> {
    loop {
        let message = live.next().await;
        if message["type"] == "event" {
            let mut topics: Vec<String> = message["topics"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| t.as_str().unwrap().to_owned())
                .collect();
            topics.sort();
            return topics;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn qry_086_a_change_in_the_scope_reaches_the_list() {
    let world = World::new().await;
    let mut bob = world.live(&world.bob).await;
    let list = format!("list:{}", world.web);
    bob.subscribe(&[&list, "list:all"], None).await;
    let version = world.estimate(world.issue, "3h").await;
    let event = bob.next().await;
    assert_eq!(
        event["entity"],
        json!({ "kind": "issue", "id": world.issue, "version": version })
    );
    let mut topics = vec![list, "list:all".to_owned()];
    topics.sort();
    let mut got: Vec<String> = serde_json::from_value(event["topics"].clone()).unwrap();
    got.sort();
    assert_eq!(got, topics);
    // A new issue of the project too.
    let created = world
        .create(json!({ "project": world.web, "summary": "New" }))
        .await;
    let event = bob.next().await;
    assert_eq!(event["entity"]["id"], created["id"]);
    assert_eq!(event["change"], "created");
}

#[tokio::test(flavor = "multi_thread")]
async fn qry_086_a_change_elsewhere_does_not() {
    let world = World::new().await;
    let api = world.project("Api", "API").await;
    let elsewhere = world
        .create(json!({ "project": api, "summary": "Elsewhere" }))
        .await["id"]
        .as_i64()
        .unwrap();
    let mut admin = world.live(&world.admin).await;
    let list = format!("list:{}", world.web);
    admin.subscribe(&[&list, &topic(elsewhere)], None).await;
    world.estimate(elsewhere, "1h").await;
    // The issue's own topic hears it, the list of the other project does not.
    assert_eq!(next_topics(&mut admin).await, [topic(elsewhere)]);
    admin.assert_quiet().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn qry_086_a_move_reaches_lists_of_both_projects() {
    let world = World::new().await;
    let api = world.project("Api", "API").await;
    let mut admin = world.live(&world.admin).await;
    let (from, to) = (format!("list:{}", world.web), format!("list:{api}"));
    admin.subscribe(&[&from, &to], None).await;
    let moved = world
        .admin_call(
            Method::POST,
            &format!("/api/v1/issues/{}/move", world.issue),
            Some(json!({ "project": api })),
        )
        .await;
    assert_eq!(moved.status, StatusCode::OK, "{}", moved.body);
    let mut both = vec![from, to];
    both.sort();
    // One message for both lists.
    assert_eq!(next_topics(&mut admin).await, both);
}

#[tokio::test(flavor = "multi_thread")]
async fn qry_086_only_issue_events_reach_lists() {
    let world = World::new().await;
    let mut admin = world.live(&world.admin).await;
    admin
        .subscribe(&["list:all", &topic(world.issue)], None)
        .await;
    let commented = world
        .admin_call(
            Method::POST,
            &format!("/api/v1/issues/{}/comments", world.issue),
            Some(json!({ "text": "Seen it" })),
        )
        .await;
    assert_eq!(commented.status, StatusCode::CREATED, "{}", commented.body);
    let mut events = Vec::new();
    while !events
        .iter()
        .any(|e: &Value| e["entity"]["kind"] == "comment")
    {
        events.push(admin.next().await);
    }
    events.extend(admin.drain().await);
    for event in events {
        let lists = event["topics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t.as_str().unwrap().starts_with("list:"));
        assert_eq!(lists, event["entity"]["kind"] == "issue", "{event}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn qry_086_a_list_of_an_unreadable_project_is_denied() {
    let world = World::new().await;
    let api = world.project("Api", "API").await;
    let mut bob = world.live(&world.bob).await;
    let (low, high) = (world.web.min(api), world.web.max(api));
    let both = format!("list:{low},{high}");
    let web = format!("list:{}", world.web);
    bob.send(json!({ "type": "subscribe", "topics": [&both, format!("list:{api}"), &web] }))
        .await;
    assert_eq!(
        bob.next().await,
        json!({ "type": "denied", "topics": [both, format!("list:{api}")] })
    );
    assert_eq!(bob.next().await["topics"], json!([web]));
}

#[tokio::test(flavor = "multi_thread")]
async fn live_session_end_closes_the_socket() {
    let world = World::on(
        Node::start_live(Config {
            sweep: std::time::Duration::from_millis(100),
            ..Config::default()
        })
        .await,
    )
    .await;
    let mut bob = world.live(&world.bob).await;
    bob.subscribe(&["instance"], None).await;
    let out = world
        .node
        .call(Method::DELETE, "/api/v1/session", Some(&world.bob), None)
        .await;
    assert!(out.status.is_success(), "{}", out.body);
    assert_eq!(bob.closed().await, close::SESSION_ENDED);
    // The administrator's connection stays.
    let mut admin = world.live(&world.admin).await;
    admin.subscribe(&["instance"], None).await;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    admin.send(json!({ "type": "ping" })).await;
    assert_eq!(admin.next().await, json!({ "type": "pong" }));
}

#[tokio::test(flavor = "multi_thread")]
async fn live_limit_too_many_topics_are_denied() {
    let world = World::new().await;
    let mut admin = world.live(&world.admin).await;
    // At most 100 per `subscribe`: the rest are denied.
    let asked: Vec<String> = (1..=101).map(|n| format!("list:{n}")).collect();
    admin
        .send(json!({ "type": "subscribe", "topics": asked }))
        .await;
    assert_eq!(
        admin.next().await,
        json!({ "type": "denied", "topics": ["list:101"] })
    );
    assert_eq!(admin.next().await["type"], "subscribed");
    // At most 500 per connection; asking again for one it holds is fine.
    for batch in 1..5 {
        let asked: Vec<String> = (1..=100)
            .map(|n| format!("list:{}", batch * 100 + n))
            .collect();
        let refs: Vec<&str> = asked.iter().map(String::as_str).collect();
        let answer = admin.subscribe(&refs, None).await;
        assert_eq!(answer.len(), 1, "{answer:?}");
    }
    admin
        .send(json!({ "type": "subscribe", "topics": ["list:1", "list:501"] }))
        .await;
    assert_eq!(
        admin.next().await,
        json!({ "type": "denied", "topics": ["list:501"] })
    );
    assert_eq!(admin.next().await["topics"], json!(["list:1"]));
}

#[tokio::test(flavor = "multi_thread")]
async fn live_limit_oversized_message_closes() {
    let world = World::new().await;
    let mut bob = world.live(&world.bob).await;
    let topics: Vec<String> = (0..2000).map(|n| format!("issue:{n}")).collect();
    let text = json!({ "type": "subscribe", "topics": topics }).to_string();
    assert!(text.len() > taskmaster_server::live::MESSAGE_MAX);
    bob.send_text(&text).await;
    assert_eq!(bob.closed().await, close::POLICY);
}

#[tokio::test(flavor = "multi_thread")]
async fn live_limit_slow_client_is_closed() {
    let world = World::on(
        Node::start_live(Config {
            queue: 4,
            ..Config::default()
        })
        .await,
    )
    .await;
    for n in 0..10 {
        world
            .create(json!({ "project": world.web, "summary": format!("Issue {n}") }))
            .await;
    }
    // A replay of every event at once overflows a queue the client does not drain.
    let mut admin = world.live(&world.admin).await;
    admin
        .send(json!({ "type": "subscribe", "topics": ["list:all"], "since": 0 }))
        .await;
    assert_eq!(admin.closed().await, close::TOO_SLOW);
}

/// A kanban board of WEB that every reader of WEB views, holding the world's issue.
struct Board {
    id: i64,
    sprint: i64,
    columns: Vec<i64>,
    version: i64,
}

impl World {
    async fn board(&self) -> Board {
        let created = self
            .admin_call(
                Method::POST,
                "/api/v1/boards",
                Some(json!({
                    "preset": "kanban", "name": "Flow", "projects": [self.web],
                    "sharing": { "view_readers": true },
                })),
            )
            .await;
        assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
        let id = created.body["id"].as_i64().unwrap();
        let view = self
            .admin_call(Method::GET, &format!("/api/v1/boards/{id}/view"), None)
            .await;
        assert_eq!(view.status, StatusCode::OK, "{}", view.body);
        let sprint = view.body["sprint"]["id"].as_i64().unwrap();
        let columns = view.body["board"]["columns"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["id"].as_i64().unwrap())
            .collect();
        let board = Board {
            id,
            sprint,
            columns,
            version: view.body["board"]["version"].as_i64().unwrap(),
        };
        self.add_card(&board, self.issue).await;
        board
    }

    async fn add_card(&self, board: &Board, issue: i64) {
        let added = self
            .admin_call(
                Method::POST,
                &format!("/api/v1/boards/{}/sprints/{}/cards", board.id, board.sprint),
                Some(json!({ "issue": issue })),
            )
            .await;
        assert_eq!(added.status, StatusCode::OK, "{}", added.body);
    }

    async fn version_of(&self, issue: i64) -> i64 {
        let got = self
            .admin_call(Method::GET, &format!("/api/v1/issues/{issue}"), None)
            .await;
        got.body["version"].as_i64().unwrap()
    }

    async fn move_card(&self, board: &Board, issue: i64, column: i64, after: Option<i64>) {
        let version = self.version_of(issue).await;
        let moved = self
            .admin_call(
                Method::POST,
                &format!(
                    "/api/v1/boards/{}/sprints/{}/cards/{issue}/move",
                    board.id, board.sprint
                ),
                Some(json!({ "column": column, "after": after, "version": version })),
            )
            .await;
        assert_eq!(moved.status, StatusCode::OK, "{}", moved.body);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn brd_114_a_move_reaches_a_board_socket() {
    let world = World::new().await;
    let board = world.board().await;
    let topic = format!("board:{}", board.id);
    let mut bob = world.live(&world.bob).await;
    let answer = bob.subscribe(&[&topic], None).await;
    assert_eq!(
        answer.last().unwrap()["topics"],
        json!([topic]),
        "{answer:?}"
    );

    world
        .move_card(&board, world.issue, board.columns[1], None)
        .await;
    let event = bob.next().await;
    assert_eq!(event["topics"], json!([topic]));
    assert_eq!(event["entity"]["kind"], "issue");
    assert_eq!(event["entity"]["id"], world.issue);

    // A new issue of a board project reaches it too: the board may show it.
    let created = world
        .create(json!({ "project": world.web, "summary": "New" }))
        .await;
    assert_eq!(bob.next().await["entity"]["id"], created["id"]);

    // An issue Bob may not see never arrives.
    let hidden = world
        .create(json!({
            "project": world.web, "summary": "Secret",
            "audience": { "users": [world.admin_id] },
        }))
        .await["id"]
        .as_i64()
        .unwrap();
    let mut admin = world.live(&world.admin).await;
    admin.subscribe(&[&topic], None).await;
    world.add_card(&board, hidden).await;
    loop {
        let seen = admin.next().await;
        if seen["entity"]["id"] == hidden {
            break;
        }
    }
    bob.assert_quiet().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn brd_114_a_board_shared_away_stops_board_events() {
    let world = World::new().await;
    let board = world.board().await;
    let topic = format!("board:{}", board.id);
    let mut bob = world.live(&world.bob).await;
    bob.subscribe(&[&topic], None).await;
    let mut admin = world.live(&world.admin).await;
    admin.subscribe(&[&topic], None).await;

    let renamed = world
        .admin_call(
            Method::PATCH,
            &format!("/api/v1/boards/{}", board.id),
            Some(json!({ "name": "Flow 2", "version": board.version })),
        )
        .await;
    assert_eq!(renamed.status, StatusCode::OK, "{}", renamed.body);
    let event = bob.next().await;
    assert_eq!(
        event["entity"],
        json!({ "kind": "board", "id": board.id, "version": renamed.body["version"] })
    );
    assert_eq!(event["change"], "updated");
    admin.next().await;

    let private = world
        .admin_call(
            Method::PATCH,
            &format!("/api/v1/boards/{}", board.id),
            Some(json!({ "sharing": {}, "version": renamed.body["version"] })),
        )
        .await;
    assert_eq!(private.status, StatusCode::OK, "{}", private.body);
    assert_eq!(admin.next().await["entity"]["kind"], "board");
    bob.assert_quiet().await;
    // Subscribing again is denied now.
    bob.send(json!({ "type": "subscribe", "topics": [topic] }))
        .await;
    let answer = bob.next().await;
    assert_eq!(answer["type"], "denied", "{answer}");
}

#[tokio::test(flavor = "multi_thread")]
async fn brd_115_order_patch() {
    let world = World::new().await;
    let board = world.board().await;
    let second = world
        .create(json!({ "project": world.web, "summary": "Second" }))
        .await["id"]
        .as_i64()
        .unwrap();
    world.add_card(&board, second).await;
    let topic = format!("board:{}", board.id);
    let mut bob = world.live(&world.bob).await;
    bob.subscribe(&[&topic], None).await;

    // Same column, after the second card: only the order changes.
    let column = board.columns[0];
    world
        .move_card(&board, world.issue, column, Some(second))
        .await;
    let event = bob.next().await;
    assert_eq!(event["entity"]["kind"], "card_order", "{event}");
    assert_eq!(event["entity"]["id"], world.issue);
    assert_eq!(
        event["patch"],
        json!({ "issue": world.issue, "after": second, "sprint": board.sprint })
    );
    // The issue itself did not change.
    bob.assert_quiet().await;
}

#[tokio::test]
async fn brd_115_sprint_events() {
    let world = World::new().await;
    let board = world.board().await;
    let topic = format!("board:{}", board.id);
    let mut bob = world.live(&world.bob).await;
    bob.subscribe(&[&topic], None).await;

    let on = world
        .admin_call(
            Method::PATCH,
            &format!("/api/v1/boards/{}", board.id),
            Some(json!({ "sprints_on": true, "version": board.version })),
        )
        .await;
    assert_eq!(on.status, StatusCode::OK, "{}", on.body);
    let mut kinds = vec![
        bob.next().await["entity"]["kind"].clone(),
        bob.next().await["entity"]["kind"].clone(),
    ];
    kinds.sort_by_key(|k| k.to_string());
    assert_eq!(kinds, vec![json!("board"), json!("sprint")]);

    let created = world
        .admin_call(
            Method::POST,
            &format!("/api/v1/boards/{}/sprints", board.id),
            Some(json!({ "name": "Sprint 2" })),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    let event = bob.next().await;
    assert_eq!(
        event["entity"],
        json!({ "kind": "sprint", "id": created.body["id"], "version": created.body["version"] })
    );
    assert_eq!(event["change"], "created");
}

#[tokio::test(flavor = "multi_thread")]
async fn brd_116_a_backlog_reorder_reaches_the_board() {
    let world = World::new().await;
    let board = world.board().await;
    let got = world
        .admin_call(Method::GET, &format!("/api/v1/boards/{}", board.id), None)
        .await;
    let search = got.body["backlog"].as_i64().expect("a backlog");
    let other = world
        .create(json!({ "project": world.web, "summary": "In the backlog" }))
        .await["id"]
        .as_i64()
        .unwrap();
    let topic = format!("board:{}", board.id);
    let mut admin = world.live(&world.admin).await;
    admin.subscribe(&[&topic], None).await;

    let ordered = world
        .admin_call(
            Method::POST,
            &format!("/api/v1/boards/{}/backlog/order", board.id),
            Some(json!({ "issue": other })),
        )
        .await;
    assert_eq!(ordered.status, StatusCode::NO_CONTENT, "{}", ordered.body);
    let event = loop {
        let seen = admin.next().await;
        if seen["entity"]["kind"] == "card_order" {
            break seen;
        }
    };
    assert_eq!(event["entity"]["id"], other);
    assert_eq!(
        event["patch"],
        json!({ "issue": other, "after": null, "search": search })
    );
}
