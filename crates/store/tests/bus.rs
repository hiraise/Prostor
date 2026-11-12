//! Notifications between nodes.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use trackflow_store::bus::{self, Bus, BusEvent};
use trackflow_testkit::TestDb;

async fn next(events: &mut broadcast::Receiver<BusEvent>) -> BusEvent {
    tokio::time::timeout(Duration::from_secs(5), events.recv())
        .await
        .expect("no notification within 5 s")
        .unwrap()
}

#[tokio::test]
async fn notifications_arrive_on_commit_and_never_on_rollback() {
    let db = TestDb::new().await;
    let stop = CancellationToken::new();
    let (bus, task) = Bus::start(db.pool().clone(), stop.clone()).await.unwrap();
    let mut events = bus.subscribe();

    let mut rolled_back = db.pool().begin().await.unwrap();
    bus::notify(&mut rolled_back, bus::SETTINGS, "lost")
        .await
        .unwrap();
    rolled_back.rollback().await.unwrap();

    let mut committed = db.pool().begin().await.unwrap();
    bus::notify(&mut committed, bus::SETTINGS, "base_url")
        .await
        .unwrap();
    committed.commit().await.unwrap();

    assert_eq!(
        next(&mut events).await,
        BusEvent::Notified {
            channel: bus::SETTINGS.to_owned(),
            payload: "base_url".to_owned(),
        }
    );
    stop.cancel();
    task.await.unwrap();
}

#[tokio::test]
async fn subscribers_resync_after_the_listener_reconnects() {
    let db = TestDb::new().await;
    let stop = CancellationToken::new();
    let (bus, task) = Bus::start(db.pool().clone(), stop.clone()).await.unwrap();
    let mut events = bus.subscribe();

    // Cut the listening connection, as a database restart would.
    sqlx::query(
        "select pg_terminate_backend(pid) from pg_stat_activity
         where datname = current_database() and query ilike 'listen%'",
    )
    .execute(db.pool())
    .await
    .unwrap();
    assert_eq!(next(&mut events).await, BusEvent::Resync);

    let mut conn = db.pool().acquire().await.unwrap();
    bus::notify(&mut conn, bus::JOBS, "after").await.unwrap();
    drop(conn);
    assert!(
        matches!(next(&mut events).await, BusEvent::Notified { payload, .. } if payload == "after")
    );
    stop.cancel();
    task.await.unwrap();
}
