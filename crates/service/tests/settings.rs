//! The general admin settings (ADM-002, ADM-003, ADM-016, ADM-023, AUTH-004, UI-004; plan step
//! 1.11 §3.1–§3.2, §7.1) on a real database: the registry validates what `save` stores, every
//! change lands in one transaction with an audit row, the snapshot reloads, and flags ride the
//! same flow. A member reads and saves nothing.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::{Value, json};
use time::OffsetDateTime;
use time::macros::datetime;
use trackflow_domain::Actor;
use trackflow_domain::access::Access;
use trackflow_domain::preferences;
use trackflow_domain::settings::{self, DateFormat, Flag, Home, Snapshot};
use trackflow_service::access;
use trackflow_service::settings as service;
use trackflow_service::setup::{self, Setup};
use trackflow_service::users;
use trackflow_store::PgPool;
use trackflow_store::sessions::Origin as SessionOrigin;
use trackflow_testkit::TestDb;

const T0: OffsetDateTime = datetime!(2026-11-09 09:00 UTC);

fn passwords() -> trackflow_service::Passwords {
    trackflow_service::Passwords::new(4)
}

/// A local flag of the tests (ADM-023): the production list is empty in P1, the flow is the same.
static TEST_FLAG: Flag = Flag {
    name: "test_flag",
    description: "A flag of the tests",
    default: true,
    may_switch_off: true,
};

async fn admin(pool: &PgPool) -> Access {
    setup::prepare(pool, Some("TEST-CODE-2345")).await.unwrap();
    let signed_in = setup::run(
        pool,
        &passwords(),
        Setup {
            code: "TEST-CODE-2345".to_owned(),
            username: "admin".to_owned(),
            full_name: "Anna Admin".to_owned(),
            email: "chief@example.com".to_owned(),
            password: "correct horse".to_owned(),
            system_language: "en".to_owned(),
            base_url: None,
            time_zone: "UTC".to_owned(),
        },
        T0,
        &SessionOrigin::default(),
    )
    .await
    .unwrap();
    access::load(pool, signed_in.user.id).await.unwrap()
}

/// A plain member, for refusals.
async fn member(pool: &PgPool, admin: &Access, username: &str) -> Access {
    let user = users::create(
        pool,
        &passwords(),
        admin,
        users::NewAccount {
            username: username.to_owned(),
            full_name: format!("{username} Person"),
            email: format!("{username}@example.com"),
            password: "correct horse".to_owned(),
            must_change_password: false,
        },
    )
    .await
    .unwrap()
    .id;
    access::load(pool, user).await.unwrap()
}

/// The settings snapshot of this database.
async fn snapshot(pool: &PgPool) -> Snapshot {
    let rows = trackflow_store::settings::load(pool).await.unwrap();
    let (snapshot, errors) = Snapshot::from_stored(rows);
    assert!(errors.is_empty(), "{errors:?}");
    snapshot
}

async fn set(
    pool: &PgPool,
    admin: &Access,
    key: &str,
    value: Option<&Value>,
) -> Result<bool, trackflow_service::settings::SetError> {
    service::set(pool, key, value, Actor::User(admin.user()), None).await
}

#[tokio::test]
async fn adm_003_base_url_validation() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;

    // A member changes nothing.
    let plain = member(pool, &admin, "plain").await;
    let error = service::save(
        pool,
        &plain,
        &[("base_url".to_owned(), Some(json!("https://t.example.com")))],
        None,
    )
    .await
    .unwrap_err();
    assert!(
        matches!(error, trackflow_service::Error::Forbidden(_)),
        "{error:?}"
    );

    // Not a URL, a query string, credentials: refused with the field named.
    for bad in [
        json!("example.com"),
        json!("ftp://example.com"),
        json!("https://example.com/?a=1"),
        json!("https://user:secret@example.com"),
    ] {
        let error = set(pool, &admin, "base_url", Some(&bad)).await.unwrap_err();
        assert!(
            matches!(
                error,
                trackflow_service::settings::SetError::Invalid(
                    trackflow_domain::settings::SettingError::Invalid { .. }
                )
            ),
            "{bad}: {error:?}"
        );
    }

    // A good address is stored in one canonical form, with an audit row.
    set(
        pool,
        &admin,
        "base_url",
        Some(&json!("  https://Tracker.example.com/  ")),
    )
    .await
    .unwrap();
    let view = snapshot(pool).await;
    assert_eq!(
        view.get(&settings::BASE_URL),
        Some("https://tracker.example.com".to_owned())
    );
    assert!(view.is_stored("base_url"));
    let events = trackflow_store::audit::for_target(pool, "setting:base_url", 5)
        .await
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].details["new"], "https://tracker.example.com");

    // `null` returns to the default and drops the row.
    set(pool, &admin, "base_url", None).await.unwrap();
    let view = snapshot(pool).await;
    assert_eq!(view.get(&settings::BASE_URL), None);
    assert!(!view.is_stored("base_url"));
}

#[tokio::test]
async fn adm_016_format_validation() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let format = |value: &str| match value {
        "locale" => DateFormat::Locale,
        "dmy_dot" => DateFormat::DmyDot,
        "dmy_name" => DateFormat::DmyName,
        "iso" => DateFormat::Iso,
        other => panic!("no such date format: {other}"),
    };

    // The four values pass; anything else is a wrong type.
    for value in ["locale", "dmy_dot", "dmy_name", "iso"] {
        set(pool, &admin, "default_date_format", Some(&json!(value)))
            .await
            .unwrap();
        assert_eq!(
            snapshot(pool).await.get(&settings::DEFAULT_DATE_FORMAT),
            format(value),
            "{value}"
        );
    }
    for bad in [json!("31.12.2000"), json!(2), json!(null)] {
        let error = set(pool, &admin, "default_date_format", Some(&bad))
            .await
            .unwrap_err();
        assert!(
            matches!(
                error,
                trackflow_service::settings::SetError::Invalid(
                    trackflow_domain::settings::SettingError::WrongType { .. }
                )
            ),
            "{bad}: {error:?}"
        );
    }
    // Resetting to the default stores nothing.
    set(pool, &admin, "default_date_format", None)
        .await
        .unwrap();
    assert!(!snapshot(pool).await.is_stored("default_date_format"));
}

#[tokio::test]
async fn ui_004_default_home() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;

    // Both layers agree on the default: the registry and the personal preference.
    let view = snapshot(pool).await;
    assert_eq!(view.get(&settings::DEFAULT_HOME), Home::Issues);
    assert_eq!(preferences::Home::default(), preferences::Home::Issues);

    // The change flow stores another page and returns to the default.
    set(pool, &admin, "default_home", Some(&json!("boards")))
        .await
        .unwrap();
    let view = snapshot(pool).await;
    assert_eq!(view.get(&settings::DEFAULT_HOME), Home::Boards);
    assert!(view.is_stored("default_home"));
    set(pool, &admin, "default_home", Some(&json!("issues")))
        .await
        .unwrap();
    assert!(!snapshot(pool).await.is_stored("default_home"));

    // A page the registry does not know is a wrong type, not a stored string.
    let error = set(pool, &admin, "default_home", Some(&json!("dashboards")))
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        trackflow_service::settings::SetError::Invalid(
            trackflow_domain::settings::SettingError::WrongType { .. }
        )
    ));
}

#[tokio::test]
async fn adm_023_flag_registry() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;

    // Without a row the declared default holds.
    assert!(snapshot(pool).await.flag(&TEST_FLAG));

    // The change rides the setting flow: one row, one audit event.
    service::set_flag(pool, &TEST_FLAG, Some(false), Actor::User(admin.user()))
        .await
        .unwrap();
    let view = snapshot(pool).await;
    assert!(!view.flag(&TEST_FLAG));
    assert!(view.flag_is_stored(&TEST_FLAG));
    let events = trackflow_store::audit::for_target(pool, "setting:flag.test_flag", 5)
        .await
        .unwrap();
    assert_eq!(events.len(), 1);
    // `null` stands for the default on either side, as with settings.
    assert_eq!(events[0].details["old"], Value::Null);
    assert_eq!(events[0].details["new"], json!(false));

    // Storing the default again drops the row; the screen refuses a member.
    service::set_flag(pool, &TEST_FLAG, Some(true), Actor::User(admin.user()))
        .await
        .unwrap();
    assert!(!snapshot(pool).await.flag_is_stored(&TEST_FLAG));
    let plain = member(pool, &admin, "plain").await;
    let error = service::admin_view(&plain, &snapshot(pool).await).unwrap_err();
    assert!(
        matches!(error, trackflow_service::Error::Forbidden(_)),
        "{error:?}"
    );
}

#[tokio::test]
async fn auth_004_bounds() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;

    // The bounds hold through the change flow: idle 10 min–12 h, remember-me 1–365 days.
    for (key, bad) in [
        ("session_idle_minutes", json!(9)),
        ("session_idle_minutes", json!(721)),
        ("remember_me_days", json!(0)),
        ("remember_me_days", json!(366)),
    ] {
        let error = set(pool, &admin, key, Some(&bad)).await.unwrap_err();
        assert!(
            matches!(
                error,
                trackflow_service::settings::SetError::Invalid(
                    trackflow_domain::settings::SettingError::Invalid { .. }
                )
            ),
            "{key} = {bad}: {error:?}"
        );
        assert!(!snapshot(pool).await.is_stored(key));
    }
    for (key, good) in [
        ("session_idle_minutes", json!(10)),
        ("session_idle_minutes", json!(720)),
        ("remember_me_days", json!(365)),
    ] {
        set(pool, &admin, key, Some(&good)).await.unwrap();
        assert!(snapshot(pool).await.is_stored(key), "{key} = {good}");
    }
    // The policy reads the stored values.
    let policy = settings::session_policy(&snapshot(pool).await);
    assert_eq!(policy.idle, time::Duration::hours(12));
    assert_eq!(policy.remember, time::Duration::days(365));
}
