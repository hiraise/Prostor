//! A node against a real database: startup, readiness, migrations, bootstrap and settings.
//! Tests that start a node run on the multi-thread runtime (see `taskmaster_testkit`).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use taskmaster_domain::Actor;
use taskmaster_server::lifecycle::Phase;
use taskmaster_server::settings::SetError;
use taskmaster_server::startup::{self, Services, StartupError};
use taskmaster_server::web::NoAssets;
use taskmaster_server::{AppState, router};
use taskmaster_store::jobs::{Handlers, RunnerConfig};
use taskmaster_testkit::TestDb;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;

fn node(db: &TestDb) -> AppState {
    AppState::new(Arc::new(NoAssets), db.pool().clone())
}

fn runner() -> RunnerConfig {
    RunnerConfig {
        workers: 1,
        poll_interval: Duration::from_millis(100),
        shutdown_grace: Duration::from_secs(1),
        ..RunnerConfig::default()
    }
}

async fn start(state: &AppState, db: &TestDb) -> Result<Option<Services>, StartupError> {
    startup::start(
        state,
        db.options(),
        Handlers::new(),
        runner(),
        &CancellationToken::new(),
    )
    .await
}

async fn get(state: &AppState, uri: &str) -> (StatusCode, Value) {
    let response = router(state.clone())
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test(flavor = "multi_thread")]
async fn adm_048_startup_migrates_an_empty_database_then_reports_ready() {
    let db = TestDb::empty().await;
    let state = node(&db);
    let (status, body) = get(&state, "/api/v1/health/ready").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["status"], "starting");

    let services = start(&state, &db).await.unwrap().unwrap();
    assert_eq!(state.lifecycle.phase(), Phase::Ready);
    let (status, body) = get(&state, "/api/v1/health/ready").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ready");

    // Each migration is recorded with its time and result.
    let recorded: Vec<(i64, bool)> =
        sqlx::query_as("select version, success from _sqlx_migrations order by version")
            .fetch_all(db.pool())
            .await
            .unwrap();
    let expected: Vec<(i64, bool)> = taskmaster_store::MIGRATOR
        .iter()
        .map(|m| (m.version, true))
        .collect();
    assert_eq!(recorded, expected);
    services.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn adm_048_a_second_start_finds_the_schema_up_to_date() {
    let db = TestDb::new().await;
    let report = taskmaster_store::migrate(db.options()).await.unwrap();
    assert!(report.applied.is_empty());
    let state = node(&db);
    start(&state, &db).await.unwrap().unwrap().stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn adm_048_startup_refuses_a_database_of_a_newer_version() {
    let db = TestDb::new().await;
    sqlx::query(
        "insert into _sqlx_migrations (version, description, success, checksum, execution_time)
         values (999999, 'from the future', true, '\\x00', 1)",
    )
    .execute(db.pool())
    .await
    .unwrap();

    let state = node(&db);
    let error = start(&state, &db).await.err().expect("startup must fail");
    let message = error.to_string();
    assert!(message.contains("999999"), "{message}");
    assert!(
        message.contains("Downgrades are not supported"),
        "{message}"
    );
    assert_ne!(state.lifecycle.phase(), Phase::Ready);
}

#[tokio::test(flavor = "multi_thread")]
async fn adm_033_startup_gives_up_waiting_for_the_database_on_shutdown() {
    let options = taskmaster_store::connect_options("postgres://nobody@127.0.0.1:9/none").unwrap();
    let state = AppState::new(Arc::new(NoAssets), taskmaster_store::pool(&options, 2));
    let stop = CancellationToken::new();
    let cancel = stop.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        cancel.cancel();
    });
    let started = startup::start(&state, &options, Handlers::new(), runner(), &stop).await;
    assert!(matches!(started, Ok(None)));
    assert_eq!(state.lifecycle.phase(), Phase::Starting);
}

#[tokio::test(flavor = "multi_thread")]
async fn adm_002_bootstrap_reports_defaults_and_follows_changes_on_every_node() {
    let db = TestDb::new().await;
    let first = node(&db);
    let second = node(&db);
    let first_services = start(&first, &db).await.unwrap().unwrap();
    let second_services = start(&second, &db).await.unwrap().unwrap();

    let (status, body) = get(&first, "/api/v1/bootstrap").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({
            "version": env!("CARGO_PKG_VERSION"),
            "base_url": null,
            "setup_required": true,
            "email_enabled": false,
            "gravatar": false,
            "default_home": "issues",
            "default_date_format": "locale",
            "limits": { "upload_max_bytes": 10 * 1024 * 1024 },
            "work_schedule": { "day_minutes": 480, "week_days": 5 },
        })
    );

    let changed = first
        .settings
        .set(
            db.pool(),
            "upload_max_bytes",
            Some(&json!(2048)),
            Actor::System,
        )
        .await
        .unwrap();
    assert!(changed);
    let (_, body) = get(&first, "/api/v1/bootstrap").await;
    assert_eq!(body["limits"]["upload_max_bytes"], 2048);

    // The other node learns about the change through the notification bus.
    let mut seen = Value::Null;
    for _ in 0..50 {
        seen = get(&second, "/api/v1/bootstrap").await.1["limits"]["upload_max_bytes"].clone();
        if seen == 2048 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(seen, 2048);

    first_services.stop().await;
    second_services.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn adm_002_setting_changes_are_validated_and_audited() {
    let db = TestDb::new().await;
    let state = node(&db);
    let services = start(&state, &db).await.unwrap().unwrap();

    let refused = state
        .settings
        .set(
            db.pool(),
            "base_url",
            Some(&json!("ftp://tracker")),
            Actor::System,
        )
        .await;
    assert!(matches!(refused, Err(SetError::Invalid(_))));
    let unknown = state
        .settings
        .set(db.pool(), "no_such_setting", Some(&json!(1)), Actor::System)
        .await;
    assert!(matches!(unknown, Err(SetError::Invalid(_))));

    let mut conn = db.pool().acquire().await.unwrap();
    let admin = taskmaster_store::users::insert(
        &mut conn,
        &taskmaster_store::users::NewUser {
            username: "anna",
            full_name: "Anna",
            email: None,
            builtin: None,
            invited: false,
            disabled: false,
        },
    )
    .await
    .unwrap();
    drop(conn);
    let set = |value: Option<Value>| {
        let state = state.clone();
        let pool = db.pool().clone();
        async move {
            state
                .settings
                .set(&pool, "base_url", value.as_ref(), Actor::User(admin))
                .await
                .unwrap()
        }
    };
    assert!(set(Some(json!("https://tracker.example.com/"))).await);
    assert!(!set(Some(json!("https://tracker.example.com"))).await);
    assert!(set(None).await);

    let events = taskmaster_store::audit::for_target(db.pool(), "setting:base_url", 10)
        .await
        .unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(
        events[0].details,
        json!({ "old": "https://tracker.example.com", "new": null })
    );
    assert_eq!(
        events[1].details,
        json!({ "old": null, "new": "https://tracker.example.com" })
    );
    assert!(events.iter().all(|event| event.actor_id == Some(admin)));
    assert!(events.iter().all(|event| event.action == "setting.changed"));
    services.stop().await;
}
