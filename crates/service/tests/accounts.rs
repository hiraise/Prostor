//! Accounts on a real database: setup, sign-in, sessions, passwords, tokens and recovery.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::json;
use time::macros::datetime;
use time::{Duration, OffsetDateTime};
use trackflow_domain::access::permission::Permission;
use trackflow_domain::auth::{Credential, Scope, SessionPolicy};
use trackflow_domain::settings::Language;
use trackflow_domain::validation::Problem;
use trackflow_service::auth::{self, SignIn, SignedIn};
use trackflow_service::mail::Letters;
use trackflow_service::recovery::{self, Target};
use trackflow_service::setup::{self, Setup};
use trackflow_service::tokens::{self, NewTokenInput};
use trackflow_service::{Error, Passwords, account};
use trackflow_store::instance::SetupState;
use trackflow_store::sessions::Origin;
use trackflow_store::{PgPool, audit, sessions, users};
use trackflow_testkit::TestDb;

const CODE: &str = "TEST-CODE-2345";
const T0: OffsetDateTime = datetime!(2026-10-01 09:00 UTC);
const POLICY: SessionPolicy = SessionPolicy {
    idle: Duration::hours(1),
    remember: Duration::days(30),
};

fn passwords() -> Passwords {
    Passwords::new(4)
}

fn origin() -> Origin {
    Origin {
        ip: Some("192.0.2.10".to_owned()),
        user_agent: Some("tests".to_owned()),
    }
}

fn setup_input() -> Setup {
    Setup {
        code: "test code 2345".to_owned(),
        username: "admin".to_owned(),
        full_name: "Анна Админова".to_owned(),
        email: "Anna@Example.com".to_owned(),
        password: "correct horse".to_owned(),
        system_language: "ru".to_owned(),
        base_url: Some("https://tracker.example.com/".to_owned()),
        time_zone: "Europe/Moscow".to_owned(),
    }
}

async fn set_up(pool: &PgPool) -> SignedIn {
    setup::prepare(pool, Some(CODE)).await.unwrap();
    setup::run(pool, &passwords(), setup_input(), T0, &origin())
        .await
        .unwrap()
}

async fn sign_in(
    pool: &PgPool,
    login: &str,
    password: &str,
    remember: bool,
) -> Result<SignedIn, Error> {
    auth::sign_in(
        pool,
        &passwords(),
        SignIn {
            login: login.to_owned(),
            password: password.to_owned(),
            remember,
        },
        POLICY,
        T0,
        &origin(),
    )
    .await
}

fn problems(error: Error) -> Vec<(String, Problem)> {
    match error {
        Error::Invalid(problems) => problems
            .into_iter()
            .map(|problem| (problem.path, problem.problem))
            .collect(),
        other => panic!("expected invalid input, got {other:?}"),
    }
}

#[tokio::test]
async fn adm_047_setup_creates_the_administrator_and_the_guest_and_signs_in() {
    let db = TestDb::new().await;
    let pool = db.pool();
    assert!(!setup::is_complete(pool).await.unwrap());
    // A pending setup keeps its code across restarts; a configured code replaces it.
    let SetupState::Pending { code } = setup::prepare(pool, None).await.unwrap() else {
        panic!("setup must be pending");
    };
    assert_eq!(
        setup::prepare(pool, None).await.unwrap(),
        SetupState::Pending { code }
    );

    let signed_in = set_up(pool).await;
    assert_eq!(signed_in.user.username, "admin");
    assert_eq!(signed_in.user.email.as_deref(), Some("Anna@Example.com"));
    // ACC-001: the administrator holds the system administrator role globally.
    let access = trackflow_service::access::load(pool, signed_in.user.id)
        .await
        .unwrap();
    assert!(access.has(Permission::SystemWrite) && access.has(Permission::ProjectDelete));
    assert!(signed_in.user.email_verified_at.is_some());
    assert!(signed_in.user.is(users::Builtin::Admin));
    assert!(!signed_in.session.remember);
    assert!(setup::is_complete(pool).await.unwrap());
    assert_eq!(
        setup::prepare(pool, Some(CODE)).await.unwrap(),
        SetupState::Complete
    );

    // ACC-001: the guest exists, disabled, and cannot sign in.
    let guest = sqlx::query!(
        r#"select id, username, disabled_at,
                  (select count(*) from role_assignments where user_id = users.id) as "roles!"
           from users where builtin = 'guest'"#
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(guest.username, "guest");
    assert!(guest.disabled_at.is_some());
    assert_eq!(guest.roles, 0);

    let settings: Vec<(String, serde_json::Value)> =
        trackflow_store::settings::load(pool).await.unwrap();
    let mut settings = settings;
    settings.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        settings,
        [
            ("base_url".to_owned(), json!("https://tracker.example.com")),
            ("default_time_zone".to_owned(), json!("Europe/Moscow")),
            ("system_language".to_owned(), json!("ru")),
        ]
    );
    let events = audit::for_target(pool, "instance", 5).await.unwrap();
    assert_eq!(events[0].action, "instance.setup_completed");
    assert_eq!(events[0].actor_id, Some(signed_in.user.id));

    // The session works, and the setup runs only once.
    let principal = auth::authenticate_session(pool, &signed_in.secret.value, POLICY, T0)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(principal.user, signed_in.user.id);
    let again = setup::run(pool, &passwords(), setup_input(), T0, &origin()).await;
    assert!(matches!(again, Err(Error::Conflict(_))), "{again:?}");
}

#[tokio::test]
async fn adm_047_setup_reports_every_problem_at_once() {
    let db = TestDb::new().await;
    let pool = db.pool();
    setup::prepare(pool, Some(CODE)).await.unwrap();
    let error = setup::run(
        pool,
        &passwords(),
        Setup {
            code: "WRONG-CODE-0000".to_owned(),
            username: "guest".to_owned(),
            full_name: " ".to_owned(),
            email: "not-an-email".to_owned(),
            password: "short".to_owned(),
            system_language: "de".to_owned(),
            base_url: Some("ftp://tracker".to_owned()),
            time_zone: "Mars/Olympus".to_owned(),
        },
        T0,
        &origin(),
    )
    .await
    .unwrap_err();
    assert_eq!(
        problems(error),
        [
            ("code".to_owned(), Problem::Incorrect),
            ("username".to_owned(), Problem::Taken),
            ("full_name".to_owned(), Problem::Required),
            ("email".to_owned(), Problem::Invalid),
            ("password".to_owned(), Problem::TooShort),
            ("system_language".to_owned(), Problem::Invalid),
            ("base_url".to_owned(), Problem::Invalid),
            ("time_zone".to_owned(), Problem::Invalid),
        ]
    );
    assert!(!setup::is_complete(pool).await.unwrap());
}

#[tokio::test]
async fn auth_001_sign_in_with_username_or_email_in_any_case() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = set_up(pool).await.user;
    for login in ["admin", "ADMIN", " anna@example.com ", "Anna@Example.COM"] {
        let signed_in = sign_in(pool, login, "correct horse", false).await.unwrap();
        assert_eq!(signed_in.user.id, admin.id, "{login}");
    }
    for (login, password) in [
        ("admin", "wrong horse"),
        ("nobody", "correct horse"),
        ("guest", "correct horse"),
        ("admin", &"x".repeat(300)),
    ] {
        let refused = sign_in(pool, login, password, false).await;
        assert!(
            matches!(refused, Err(Error::InvalidCredentials)),
            "{login}: {refused:?}"
        );
    }
    let missing = sign_in(pool, " ", "", false).await.unwrap_err();
    assert_eq!(
        problems(missing),
        [
            ("login".to_owned(), Problem::Required),
            ("password".to_owned(), Problem::Required),
        ]
    );
}

#[tokio::test]
async fn auth_004_a_session_ends_after_the_idle_timeout_and_activity_extends_it() {
    let db = TestDb::new().await;
    let pool = db.pool();
    set_up(pool).await;
    let session = sign_in(pool, "admin", "correct horse", false)
        .await
        .unwrap();
    let check = |at: OffsetDateTime| {
        let secret = session.secret.value.clone();
        async move {
            auth::authenticate_session(pool, &secret, POLICY, at)
                .await
                .unwrap()
        }
    };
    // Each request moves the end an hour ahead; an hour without requests ends the session.
    assert!(check(T0 + Duration::minutes(50)).await.is_some());
    assert!(check(T0 + Duration::minutes(109)).await.is_some());
    assert!(check(T0 + Duration::minutes(170)).await.is_none());
    // An ended session is removed.
    assert!(
        sessions::get(pool, session.session.id)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn auth_003_a_remembered_session_outlives_idleness_until_its_duration() {
    let db = TestDb::new().await;
    let pool = db.pool();
    set_up(pool).await;
    let session = sign_in(pool, "admin", "correct horse", true).await.unwrap();
    assert!(session.session.remember);
    let secret = &session.secret.value;
    assert!(
        auth::authenticate_session(pool, secret, POLICY, T0 + Duration::days(29))
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        auth::authenticate_session(pool, secret, POLICY, T0 + Duration::days(30))
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn auth_005_sign_out_ends_the_session() {
    let db = TestDb::new().await;
    let pool = db.pool();
    set_up(pool).await;
    let first = sign_in(pool, "admin", "correct horse", false)
        .await
        .unwrap();
    let second = sign_in(pool, "admin", "correct horse", false)
        .await
        .unwrap();
    auth::sign_out_by_cookie(pool, &first.secret.value)
        .await
        .unwrap();
    // Signing out twice, or with an unknown cookie, is fine.
    auth::sign_out_by_cookie(pool, &first.secret.value)
        .await
        .unwrap();
    auth::sign_out_by_cookie(pool, "made-up").await.unwrap();
    assert!(
        auth::authenticate_session(pool, &first.secret.value, POLICY, T0)
            .await
            .unwrap()
            .is_none()
    );
    // Other sessions of the same person stay.
    assert!(
        auth::authenticate_session(pool, &second.secret.value, POLICY, T0)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn auth_009_changing_the_password_ends_other_sessions() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let setup_session = set_up(pool).await;
    let current = sign_in(pool, "admin", "correct horse", false)
        .await
        .unwrap();
    let principal = auth::authenticate_session(pool, &current.secret.value, POLICY, T0)
        .await
        .unwrap()
        .unwrap();

    let wrong = account::change_password(pool, &passwords(), &principal, "wrong", "new password 1")
        .await
        .unwrap_err();
    assert_eq!(
        problems(wrong),
        [("current_password".to_owned(), Problem::Incorrect)]
    );
    let weak = account::change_password(pool, &passwords(), &principal, "correct horse", "short")
        .await
        .unwrap_err();
    assert_eq!(
        problems(weak),
        [("new_password".to_owned(), Problem::TooShort)]
    );

    account::change_password(
        pool,
        &passwords(),
        &principal,
        "correct horse",
        "new password 1",
    )
    .await
    .unwrap();
    assert!(
        auth::authenticate_session(pool, &setup_session.secret.value, POLICY, T0)
            .await
            .unwrap()
            .is_none(),
        "other sessions end"
    );
    assert!(
        auth::authenticate_session(pool, &current.secret.value, POLICY, T0)
            .await
            .unwrap()
            .is_some(),
        "the current session stays"
    );
    assert!(matches!(
        sign_in(pool, "admin", "correct horse", false).await,
        Err(Error::InvalidCredentials)
    ));
    sign_in(pool, "admin", "new password 1", false)
        .await
        .unwrap();
    let events = audit::for_target(pool, &format!("user:{}", principal.user), 5)
        .await
        .unwrap();
    assert_eq!(events[0].action, "user.password_changed");
    assert_eq!(events[0].details, json!({ "sessions_ended": 1 }));
}

#[tokio::test]
async fn auth_029_tokens_work_until_deleted_and_list_newest_first() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let signed_in = set_up(pool).await;
    let owner = auth::authenticate_session(pool, &signed_in.secret.value, POLICY, T0)
        .await
        .unwrap()
        .unwrap();

    let invalid = tokens::create(
        pool,
        &owner,
        NewTokenInput {
            name: " ".to_owned(),
            scopes: vec!["root".to_owned()],
        },
    )
    .await
    .unwrap_err();
    assert_eq!(
        problems(invalid),
        [
            ("name".to_owned(), Problem::Required),
            ("scopes".to_owned(), Problem::Invalid),
        ]
    );

    let mut created = Vec::new();
    for name in ["ci", "reports", "backup"] {
        created.push(
            tokens::create(
                pool,
                &owner,
                NewTokenInput {
                    name: name.to_owned(),
                    scopes: vec!["tracker.read".to_owned(), "tracker.write".to_owned()],
                },
            )
            .await
            .unwrap(),
        );
    }
    let ci = &created[0];
    assert_eq!(ci.token.scopes, ["tracker.write"]);
    assert_eq!(ci.token.created_by, Some(owner.user));
    assert_eq!(ci.token.last_used_at, None);

    let principal = auth::authenticate_token(pool, &ci.secret.value, T0)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(principal.user, owner.user);
    assert!(matches!(principal.credential, Credential::Token { id, .. } if id == ci.token.id));
    assert!(principal.allows(Scope::TrackerRead) && !principal.allows(Scope::AdminRead));
    for bad in ["tfp_made-up", "not a token", &signed_in.secret.value] {
        assert!(
            auth::authenticate_token(pool, bad, T0)
                .await
                .unwrap()
                .is_none(),
            "{bad}"
        );
    }

    let first_page = tokens::list(pool, owner.user, None, 2).await.unwrap();
    assert_eq!(
        first_page
            .iter()
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>(),
        ["backup", "reports"]
    );
    let rest = tokens::list(pool, owner.user, Some(first_page[1].id), 2)
        .await
        .unwrap();
    assert_eq!(rest.len(), 1);
    assert_eq!(rest[0].name, "ci");
    assert!(rest[0].last_used_at.is_some(), "the use is recorded");

    tokens::delete(pool, &owner, ci.token.id).await.unwrap();
    assert!(
        auth::authenticate_token(pool, &ci.secret.value, T0)
            .await
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        tokens::delete(pool, &owner, ci.token.id).await,
        Err(Error::NotFound(_))
    ));
}

fn no_mail() -> Letters {
    Letters {
        enabled: false,
        base_url: None,
        language: Language::En,
    }
}

#[tokio::test]
async fn adm_051_recovery_restores_access_from_the_command_line() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let early = recovery::recover(pool, &passwords(), Target::BuiltinAdmin, &no_mail()).await;
    assert!(matches!(early, Err(Error::Conflict(_))), "{early:?}");

    let signed_in = set_up(pool).await;
    let recovered = recovery::recover(pool, &passwords(), Target::BuiltinAdmin, &no_mail())
        .await
        .unwrap();
    assert_eq!(recovered.user.id, signed_in.user.id);
    assert!(!recovered.created);
    assert_eq!(recovered.sessions_ended, 1);
    assert!(
        auth::authenticate_session(pool, &signed_in.secret.value, POLICY, T0)
            .await
            .unwrap()
            .is_none()
    );
    // The printed password works, typed exactly as shown.
    sign_in(pool, "admin", &recovered.password, false)
        .await
        .unwrap();
    let events = audit::for_target(pool, &format!("user:{}", recovered.user.id), 5)
        .await
        .unwrap();
    assert_eq!(events[0].action, "user.recovered");
    assert_eq!(events[0].actor_id, None);

    // By email; the guest cannot become an administrator.
    let by_email = recovery::recover(
        pool,
        &passwords(),
        Target::Login("ANNA@example.com".to_owned()),
        &no_mail(),
    )
    .await
    .unwrap();
    assert_eq!(by_email.user.id, signed_in.user.id);
    assert!(matches!(
        recovery::recover(
            pool,
            &passwords(),
            Target::Login("guest".to_owned()),
            &no_mail()
        )
        .await,
        Err(Error::Conflict(_))
    ));
    assert!(matches!(
        recovery::recover(
            pool,
            &passwords(),
            Target::Login("nobody".to_owned()),
            &no_mail()
        )
        .await,
        Err(Error::NotFound(_))
    ));
}

#[tokio::test]
async fn auth_073_recovery_creates_a_new_administrator() {
    let db = TestDb::new().await;
    let pool = db.pool();
    set_up(pool).await;
    let taken = recovery::recover(
        pool,
        &passwords(),
        Target::New {
            username: "Admin".to_owned(),
            email: "anna@example.com".to_owned(),
            full_name: None,
        },
        &no_mail(),
    )
    .await
    .unwrap_err();
    assert_eq!(
        problems(taken),
        [
            ("username".to_owned(), Problem::Taken),
            ("email".to_owned(), Problem::Taken),
        ]
    );
    let created = recovery::recover(
        pool,
        &passwords(),
        Target::New {
            username: "ops".to_owned(),
            email: "ops@example.com".to_owned(),
            full_name: Some("Ops".to_owned()),
        },
        &no_mail(),
    )
    .await
    .unwrap();
    assert!(created.created);
    assert!(
        trackflow_service::access::load(pool, created.user.id)
            .await
            .unwrap()
            .has(Permission::SystemWrite)
    );
    assert_eq!(created.user.full_name, "Ops");
    // The one-time password works once: a new one must be chosen at once (AUTH-010).
    let signed_in = sign_in(pool, "ops@example.com", &created.password, false)
        .await
        .unwrap();
    assert!(signed_in.session.must_change_password);
}
