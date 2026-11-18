//! The admin screens over HTTP (plan step 1.11 §7.1): the jobs of «Задания» (ADM-031, ADM-032) —
//! permissions, keyset pages, the cancel paths, the retry of a failed job and the filter terms —
//! and the settings the bootstrap carries (ADM-002).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::http::{Method, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use taskmaster_domain::Actor;
use taskmaster_server::throttle::Throttle;
use taskmaster_store::jobs::{Enqueue, JobKind};

mod common;

use common::*;

/// A cancellable kind of the tests.
#[derive(Serialize, Deserialize)]
struct Alpha {
    step: i32,
}

impl JobKind for Alpha {
    const KIND: &'static str = "test.alpha";
    const CANCELLABLE: bool = true;
}

/// Another cancellable kind, for the filter terms.
#[derive(Serialize, Deserialize)]
struct Beta {
    step: i32,
}

impl JobKind for Beta {
    const KIND: &'static str = "test.beta";
    const CANCELLABLE: bool = true;
}

/// A kind that cannot be cancelled while it runs.
#[derive(Serialize, Deserialize)]
struct Rigid;

impl JobKind for Rigid {
    const KIND: &'static str = "test.rigid";
}

/// A world with an administrator, a plain member and the jobs they started.
struct World {
    node: Node,
    admin: String,
    admin_id: i64,
    plain: String,
    bob_id: i64,
}

impl World {
    async fn new() -> Self {
        let node = Node::start(Throttle::default()).await;
        let admin = node.set_up().await;
        let me = node
            .call(Method::GET, "/api/v1/users/me", Some(&admin), None)
            .await;
        assert_eq!(me.status, StatusCode::OK, "{}", me.body);
        let admin_id = me.body["id"].as_i64().unwrap();
        let plain = node
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
        assert_eq!(plain.status, StatusCode::CREATED, "{}", plain.body);
        let bob_id = plain.body["id"].as_i64().unwrap();
        let signed_in = node
            .call(
                Method::POST,
                "/api/v1/session",
                None,
                Some(json!({ "login": "bob", "password": "bob's password", "remember": false })),
            )
            .await;
        assert_eq!(signed_in.status, StatusCode::CREATED, "{}", signed_in.body);
        let plain = signed_in.session_cookie().expect("bob signs in");
        Self {
            node,
            admin,
            admin_id,
            plain,
            bob_id,
        }
    }

    /// Queues an alpha job; `plain: true` makes Bob its initiator.
    async fn probe(&self, step: i32, plain: bool) -> i64 {
        self.enqueue_kind(&Alpha { step }, plain).await
    }

    /// Queues a beta job of Bob.
    async fn beta(&self, step: i32) -> i64 {
        self.enqueue_kind(&Beta { step }, true).await
    }

    async fn enqueue_kind<J: JobKind>(&self, job: &J, plain: bool) -> i64 {
        let initiator = if plain {
            Actor::User(self.bob_id)
        } else {
            Actor::User(self.admin_id)
        };
        self.enqueue(
            job,
            Enqueue {
                initiator,
                ..Enqueue::default()
            },
        )
        .await
    }

    async fn enqueue<J: JobKind>(&self, job: &J, options: Enqueue) -> i64 {
        let mut conn = self.node.state.db.acquire().await.unwrap();
        taskmaster_store::jobs::enqueue(&mut conn, job, options)
            .await
            .unwrap()
            .id
    }

    /// The ids of a filtered jobs list.
    async fn filtered(&self, query: &str) -> Vec<i64> {
        let answer = self
            .node
            .call(
                Method::GET,
                &format!("/api/v1/admin/jobs?{query}"),
                Some(&self.admin),
                None,
            )
            .await;
        assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
        // The setup leaves a row of its own; only the test jobs count here.
        answer.body["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|job| job["kind"].as_str().unwrap().starts_with("test."))
            .map(|job| job["id"].as_i64().unwrap())
            .collect()
    }

    /// Puts a job into a state by hand: no handler runs in these tests.
    async fn set_state(&self, id: i64, state: &str) {
        sqlx::query!(
            "update jobs set state = $2, finished_at = now(), last_error = 'boom'
             where id = $1",
            id,
            state,
        )
        .execute(&self.node.state.db)
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn adm_031_jobs_list_permissions() {
    let world = World::new().await;
    // A member without the admin permissions reads nothing.
    let answer = world
        .node
        .call(Method::GET, "/api/v1/admin/jobs", Some(&world.plain), None)
        .await;
    assert_eq!(answer.status, StatusCode::FORBIDDEN, "{}", answer.body);
    // The administrator sees the list, newest first.
    let first = world.probe(1, false).await;
    let second = world.probe(2, false).await;
    let answer = world
        .node
        .call(Method::GET, "/api/v1/admin/jobs", Some(&world.admin), None)
        .await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
    // The setup leaves a row of its own; only the test jobs count here.
    let ids: Vec<i64> = answer.body["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|job| job["kind"].as_str().unwrap().starts_with("test."))
        .map(|job| job["id"].as_i64().unwrap())
        .collect();
    assert_eq!(ids, [second, first]);
    assert_eq!(answer.body["next"], Value::Null);
}

#[tokio::test]
async fn adm_031_jobs_keyset() {
    let world = World::new().await;
    let ids = [
        world.probe(1, false).await,
        world.probe(2, false).await,
        world.probe(3, false).await,
    ];
    // A page of two carries the newest two and the id to continue from.
    let answer = world
        .node
        .call(
            Method::GET,
            "/api/v1/admin/jobs?limit=2",
            Some(&world.admin),
            None,
        )
        .await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
    let page: Vec<i64> = answer.body["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|job| job["kind"].as_str().unwrap().starts_with("test."))
        .map(|job| job["id"].as_i64().unwrap())
        .collect();
    assert_eq!(page, [ids[2], ids[1]]);
    assert_eq!(answer.body["next"], json!(ids[1]));
    // The next page ends the list without a cursor.
    let answer = world
        .node
        .call(
            Method::GET,
            &format!("/api/v1/admin/jobs?limit=2&before={}", ids[1]),
            Some(&world.admin),
            None,
        )
        .await;
    let page: Vec<i64> = answer.body["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|job| job["kind"].as_str().unwrap().starts_with("test."))
        .map(|job| job["id"].as_i64().unwrap())
        .collect();
    assert_eq!(page, [ids[0]]);
    assert_eq!(answer.body["next"], Value::Null);
}

#[tokio::test]
async fn adm_031_cancel_paths() {
    let world = World::new().await;
    // A queued cancellable job is cancelled at once.
    let queued = world.probe(1, false).await;
    let answer = world
        .node
        .call(
            Method::POST,
            &format!("/api/v1/admin/jobs/{queued}/cancel"),
            Some(&world.admin),
            None,
        )
        .await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
    assert_eq!(answer.body["state"], "cancelled");
    assert_eq!(answer.body["cancel_requested"], true);
    // A member cancels nothing.
    let answer = world
        .node
        .call(
            Method::POST,
            &format!("/api/v1/admin/jobs/{queued}/cancel"),
            Some(&world.plain),
            None,
        )
        .await;
    assert_eq!(answer.status, StatusCode::FORBIDDEN, "{}", answer.body);
    // A running job that cannot be cancelled, and one already final, answer 409.
    let rigid = world.enqueue(&Rigid, Enqueue::default()).await;
    world.set_state(rigid, "running").await;
    let answer = world
        .node
        .call(
            Method::POST,
            &format!("/api/v1/admin/jobs/{rigid}/cancel"),
            Some(&world.admin),
            None,
        )
        .await;
    assert_eq!(answer.status, StatusCode::CONFLICT, "{}", answer.body);
    let finished = world.probe(2, false).await;
    world.set_state(finished, "finished").await;
    let answer = world
        .node
        .call(
            Method::POST,
            &format!("/api/v1/admin/jobs/{finished}/cancel"),
            Some(&world.admin),
            None,
        )
        .await;
    assert_eq!(answer.status, StatusCode::CONFLICT, "{}", answer.body);
    // An unknown job is not found.
    let answer = world
        .node
        .call(
            Method::POST,
            "/api/v1/admin/jobs/999999/cancel",
            Some(&world.admin),
            None,
        )
        .await;
    assert_eq!(answer.status, StatusCode::NOT_FOUND, "{}", answer.body);
}

#[tokio::test]
async fn adm_031_retry_failed() {
    let world = World::new().await;
    // Only a failed job can be retried.
    let queued = world.probe(1, false).await;
    let answer = world
        .node
        .call(
            Method::POST,
            &format!("/api/v1/admin/jobs/{queued}/retry"),
            Some(&world.admin),
            None,
        )
        .await;
    assert_eq!(answer.status, StatusCode::CONFLICT, "{}", answer.body);

    // The retry queues a fresh job: the same kind and payload, the admin as initiator, a clean
    // count of attempts.
    let failed = world
        .enqueue(
            &Alpha { step: 7 },
            Enqueue {
                initiator: Actor::System,
                ..Enqueue::default()
            },
        )
        .await;
    world.set_state(failed, "failed").await;
    let answer = world
        .node
        .call(
            Method::POST,
            &format!("/api/v1/admin/jobs/{failed}/retry"),
            Some(&world.admin),
            None,
        )
        .await;
    assert_eq!(answer.status, StatusCode::CREATED, "{}", answer.body);
    assert_eq!(answer.body["state"], "queued");
    assert_eq!(answer.body["kind"], "test.alpha");
    assert_eq!(answer.body["initiator"]["username"], "admin");
    assert_eq!(answer.body["attempts"], 0);
    let fresh = answer.body["id"].as_i64().unwrap();
    let payload = sqlx::query_scalar!(
        r#"select payload as "payload: Value" from jobs where id = $1"#,
        fresh,
    )
    .fetch_one(&world.node.state.db)
    .await
    .unwrap();
    assert_eq!(payload, json!({ "step": 7 }));
    // The failed original stays as it was.
    let old = sqlx::query_scalar!(
        r#"select state as "state: String" from jobs where id = $1"#,
        failed,
    )
    .fetch_one(&world.node.state.db)
    .await
    .unwrap();
    assert_eq!(old, "failed");
}

#[tokio::test]
async fn adm_032_job_filter_terms() {
    let world = World::new().await;
    // An alpha of the administrator and a beta of Bob.
    let alpha = world.probe(1, false).await;
    let beta = world.beta(2).await;
    let gamma = world.enqueue(&Rigid, Enqueue::default()).await;
    world.set_state(gamma, "failed").await;

    // A word matches the kind or the initiator's name.
    assert_eq!(world.filtered("q=alpha").await, [alpha]);
    assert_eq!(world.filtered("q=bob").await, [beta]);
    assert_eq!(world.filtered("q=anna").await, [alpha]);
    // A leading minus excludes; several words must all hold.
    assert_eq!(world.filtered("q=-beta").await, [gamma, alpha]);
    assert_eq!(world.filtered("q=test%20-beta").await, [gamma, alpha]);
    assert!(world.filtered("q=alpha%20beta").await.is_empty());
    // The state filter and the words combine.
    let answer = world
        .node
        .call(
            Method::GET,
            "/api/v1/admin/jobs?state=failed&q=-beta",
            Some(&world.admin),
            None,
        )
        .await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
    let ids: Vec<i64> = answer.body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|job| job["id"].as_i64().unwrap())
        .collect();
    assert_eq!(ids, [gamma]);
}

#[tokio::test]
async fn adm_002_bootstrap_grows() {
    let world = World::new().await;
    // The bootstrap is public and carries both new settings at their defaults (UI-004, ADM-016).
    let answer = world
        .node
        .call(Method::GET, "/api/v1/bootstrap", None, None)
        .await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
    assert_eq!(answer.body["default_home"], "issues");
    assert_eq!(answer.body["default_date_format"], "locale");

    // The settings screen lists every non-secret, non-mail setting with default and is_stored.
    let answer = world
        .node
        .call(
            Method::GET,
            "/api/v1/admin/settings",
            Some(&world.admin),
            None,
        )
        .await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
    let settings: Vec<Value> = answer.body["settings"].as_array().unwrap().to_vec();
    let keys: Vec<&str> = settings
        .iter()
        .map(|setting| setting["key"].as_str().unwrap())
        .collect();
    assert_eq!(
        keys,
        [
            "base_url",
            "system_language",
            "default_time_zone",
            "default_home",
            "default_date_format",
            "api.allowed_origins",
            "upload_max_bytes",
            "session_idle_minutes",
            "remember_me_days",
        ]
    );
    assert!(answer.body["flags"].as_array().unwrap().is_empty());
    let home = settings
        .iter()
        .find(|setting| setting["key"] == "default_home")
        .unwrap();
    assert_eq!(home["value"], "issues");
    assert_eq!(home["default"], "issues");
    assert_eq!(home["is_stored"], false);

    // A change set saves and the bootstrap follows; `null` returns to the default.
    let answer = world
        .node
        .call(
            Method::PUT,
            "/api/v1/admin/settings",
            Some(&world.admin),
            Some(json!({ "default_home": "projects", "default_date_format": "iso" })),
        )
        .await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
    let bootstrap = world
        .node
        .call(Method::GET, "/api/v1/bootstrap", None, None)
        .await;
    assert_eq!(bootstrap.body["default_home"], "projects");
    assert_eq!(bootstrap.body["default_date_format"], "iso");
    let answer = world
        .node
        .call(
            Method::PUT,
            "/api/v1/admin/settings",
            Some(&world.admin),
            Some(json!({ "default_home": null, "default_date_format": null })),
        )
        .await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);

    // A member saves nothing; a mail key and an unknown key refuse their own ways.
    let answer = world
        .node
        .call(
            Method::PUT,
            "/api/v1/admin/settings",
            Some(&world.plain),
            Some(json!({ "default_home": "boards" })),
        )
        .await;
    assert_eq!(answer.status, StatusCode::FORBIDDEN, "{}", answer.body);
    let answer = world
        .node
        .call(
            Method::PUT,
            "/api/v1/admin/settings",
            Some(&world.admin),
            Some(json!({ "smtp_url": "smtp://127.0.0.1:1025" })),
        )
        .await;
    assert_eq!(answer.status, StatusCode::FORBIDDEN, "{}", answer.body);
    let answer = world
        .node
        .call(
            Method::PUT,
            "/api/v1/admin/settings",
            Some(&world.admin),
            Some(json!({ "no_such_setting": 1 })),
        )
        .await;
    assert_eq!(answer.status, StatusCode::BAD_REQUEST, "{}", answer.body);
    assert_eq!(answer.body["fields"][0]["path"], "no_such_setting");
}
