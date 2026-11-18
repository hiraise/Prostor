//! Personal interface settings on a real database (step 1.7a, UI-018): defaults, merging and
//! resetting keys, validation, unknown stored keys and the size limit.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use taskmaster_domain::UserId;
use taskmaster_domain::preferences::{
    COLUMNS_MAX, Home, PanelSide, Patch, Preferences, RECENT_MAX, SidebarSection,
};
use taskmaster_domain::validation::Problem;
use taskmaster_service::preferences;
use taskmaster_service::setup::{self, Setup};
use taskmaster_service::{Error, Passwords};
use taskmaster_store::PgPool;
use taskmaster_store::sessions::Origin as SessionOrigin;
use taskmaster_testkit::TestDb;
use time::macros::datetime;

async fn set_up(pool: &PgPool) -> UserId {
    setup::prepare(pool, Some("TEST-CODE-2345")).await.unwrap();
    setup::run(
        pool,
        &Passwords::new(4),
        Setup {
            code: "TEST-CODE-2345".to_owned(),
            username: "admin".to_owned(),
            full_name: "Anna Admin".to_owned(),
            email: "anna@example.com".to_owned(),
            password: "correct horse".to_owned(),
            system_language: "en".to_owned(),
            base_url: None,
            time_zone: "UTC".to_owned(),
        },
        datetime!(2026-10-01 09:00 UTC),
        &SessionOrigin::default(),
    )
    .await
    .unwrap()
    .user
    .id
}

fn problems(error: Error) -> Vec<(String, Problem)> {
    match error {
        Error::Invalid(problems) => problems.into_iter().map(|p| (p.path, p.problem)).collect(),
        other => panic!("expected invalid input, got {other:?}"),
    }
}

#[tokio::test]
async fn ui_018_defaults_when_unset() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let me = set_up(pool).await;
    assert_eq!(
        preferences::get(pool, me).await.unwrap(),
        Preferences::default()
    );
}

#[tokio::test]
async fn ui_018_patch_merges_and_resets() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let me = set_up(pool).await;
    let first = preferences::update(
        pool,
        me,
        Patch {
            home: Some(Some(Home::Projects)),
            issue_panel_side: Some(Some(PanelSide::Start)),
            list_sidebar_folded: Some(Some(vec![
                SidebarSection::Tags,
                SidebarSection::Tags,
                SidebarSection::Drafts,
            ])),
            ..Patch::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        first.list_sidebar_folded,
        Some(vec![SidebarSection::Tags, SidebarSection::Drafts])
    );
    // A later change keeps the other keys; `null` resets one.
    let second = preferences::update(
        pool,
        me,
        Patch {
            home: Some(None),
            issue_panel_width: Some(Some(300)),
            ..Patch::default()
        },
    )
    .await
    .unwrap();
    let expected = Preferences {
        issue_panel_side: Some(PanelSide::Start),
        issue_panel_width: Some(300),
        list_sidebar_folded: Some(vec![SidebarSection::Tags, SidebarSection::Drafts]),
        ..Preferences::default()
    };
    assert_eq!(second, expected);
    assert_eq!(preferences::get(pool, me).await.unwrap(), expected);
    // Resetting a key never set is fine.
    preferences::update(
        pool,
        me,
        Patch {
            last_project: Some(None),
            ..Patch::default()
        },
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn ui_018_rejects_out_of_range() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let me = set_up(pool).await;
    let err = preferences::update(
        pool,
        me,
        Patch {
            home: Some(Some(Home::Projects)),
            list_sidebar_width: Some(Some(100)),
            list_columns: Some(Some(vec!["colour".to_owned()])),
            ..Patch::default()
        },
    )
    .await
    .unwrap_err();
    assert_eq!(
        problems(err),
        [
            ("list_sidebar_width".to_owned(), Problem::OutOfRange),
            ("list_columns".to_owned(), Problem::Invalid),
        ]
    );
    // Nothing of a refused change is kept.
    assert_eq!(
        preferences::get(pool, me).await.unwrap(),
        Preferences::default()
    );
}

#[tokio::test]
async fn ui_018_ignores_unknown_keys() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let me = set_up(pool).await;
    // A key of an older or newer version, and a value that no longer reads, are left out.
    sqlx::query(
        "insert into user_preferences (user_id, data)
         values ($1, '{\"old_key\": 1, \"home\": \"dashboards\", \"rail_collapsed\": true}')",
    )
    .bind(me)
    .execute(pool)
    .await
    .unwrap();
    let found = preferences::get(pool, me).await.unwrap();
    assert_eq!(
        found,
        Preferences {
            rail_collapsed: Some(true),
            ..Preferences::default()
        }
    );
}

#[tokio::test]
async fn ui_018_size_limit() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let me = set_up(pool).await;
    // The largest object the rules allow is stored whole.
    let columns: Vec<String> = (1..=COLUMNS_MAX as i64)
        .map(|i| format!("field:{}", i64::MAX - i))
        .collect();
    let recent: Vec<i64> = (1..=RECENT_MAX as i64).map(|i| i64::MAX - i).collect();
    let saved = preferences::update(
        pool,
        me,
        Patch {
            list_columns: Some(Some(columns.clone())),
            recent_issues: Some(Some(recent.clone())),
            ..Patch::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(saved.list_columns, Some(columns));
    assert_eq!(saved.recent_issues, Some(recent));
    // Too much stored in all is refused (filled past the rules, as an older version might).
    sqlx::query("update user_preferences set data = data || jsonb_build_object('old', repeat('x', 17000)) where user_id = $1")
        .bind(me)
        .execute(pool)
        .await
        .unwrap();
    let err = preferences::update(
        pool,
        me,
        Patch {
            home: Some(Some(Home::Projects)),
            ..Patch::default()
        },
    )
    .await
    .unwrap_err();
    assert_eq!(
        problems(err),
        [("preferences".to_owned(), Problem::TooLong)]
    );
}
