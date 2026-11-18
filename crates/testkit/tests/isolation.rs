//! Every test gets its own migrated database, dropped afterwards.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use prostor_testkit::TestDb;

async fn exists(name: &str) -> bool {
    let probe = TestDb::empty().await;
    sqlx::query_scalar("select exists (select 1 from pg_database where datname = $1)")
        .bind(name)
        .fetch_one(probe.pool())
        .await
        .unwrap()
}

#[tokio::test]
async fn databases_are_migrated_separate_and_dropped() {
    let first = TestDb::new().await;
    let second = TestDb::new().await;
    assert_ne!(first.name(), second.name());

    sqlx::query("insert into settings (key, value) values ('probe', '1')")
        .execute(first.pool())
        .await
        .unwrap();
    let in_second: i64 = sqlx::query_scalar("select count(*) from settings")
        .fetch_one(second.pool())
        .await
        .unwrap();
    assert_eq!(in_second, 0);

    let name = first.name().to_owned();
    drop(first);
    let mut gone = false;
    for _ in 0..20 {
        if !exists(&name).await {
            gone = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(gone, "{name} was not dropped");
}
