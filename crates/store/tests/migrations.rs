//! Migrations on startup (ADM-048).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use prostor_testkit::TestDb;
use sqlx::migrate::MigrateError;

#[tokio::test]
async fn adm_048_migrations_apply_once_and_report_what_they_did() {
    let db = TestDb::empty().await;
    let first = prostor_store::migrate(db.options()).await.unwrap();
    let all: Vec<i64> = prostor_store::MIGRATOR.iter().map(|m| m.version).collect();
    assert_eq!(first.applied.len(), all.len());
    assert_eq!(first.applied[0], "0001_foundation");
    assert_eq!(first.latest, all.last().copied());

    let second = prostor_store::migrate(db.options()).await.unwrap();
    assert!(second.applied.is_empty());
    assert_eq!(second.latest, first.latest);
}

#[tokio::test]
async fn adm_048_a_changed_released_migration_is_refused() {
    let db = TestDb::new().await;
    sqlx::query("update _sqlx_migrations set checksum = '\\x00' where version = 1")
        .execute(db.pool())
        .await
        .unwrap();
    let error = prostor_store::migrate(db.options()).await.unwrap_err();
    assert!(matches!(error, MigrateError::VersionMismatch(1)), "{error}");
}

#[tokio::test]
async fn adm_048_parallel_nodes_migrate_one_after_another() {
    let db = TestDb::empty().await;
    let runs = (0..4).map(|_| {
        let options = db.options().clone();
        tokio::spawn(async move { prostor_store::migrate(&options).await })
    });
    let mut applied_by_someone = 0;
    for run in runs {
        let report = run.await.unwrap().unwrap();
        applied_by_someone += report.applied.len();
    }
    assert_eq!(applied_by_someone, prostor_store::MIGRATOR.iter().count());
}
