//! Access on a real database (step 1.1): accounts managed by administrators, groups, roles,
//! assignments, effective access, capabilities, and the emailed one-time links.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use taskmaster_domain::access::Access;
use taskmaster_domain::access::capability::Capability;
use taskmaster_domain::access::identity::Tier;
use taskmaster_domain::access::permission::Permission;
use taskmaster_domain::access::role::PredefinedRole;
use taskmaster_domain::accounts::DisabledLabel;
use taskmaster_domain::auth::SessionPolicy;
use taskmaster_domain::settings::Language;
use taskmaster_domain::validation::Problem;
use taskmaster_domain::{GroupId, RoleId, UserId};
use taskmaster_service::access::{self, Source};
use taskmaster_service::auth::{self, SignIn, SignedIn};
use taskmaster_service::groups::{self, GroupChange, NewGroup};
use taskmaster_service::links::{self, Acceptance};
use taskmaster_service::mail::Letters;
use taskmaster_service::roles::{self, NewRole};
use taskmaster_service::setup::{self, Setup};
use taskmaster_service::users::{self, IdentityInput, NewAccount, StateFilter};
use taskmaster_service::{Error, Passwords, capabilities};
use taskmaster_store::access::Holder;
use taskmaster_store::sessions::Origin;
use taskmaster_store::{PgPool, sessions};
use taskmaster_testkit::TestDb;
use time::macros::datetime;
use time::{Duration, OffsetDateTime};

const T0: OffsetDateTime = datetime!(2026-10-01 09:00 UTC);
/// Groups of people, without project teams.
const GROUPS: taskmaster_store::groups::Kinds = taskmaster_store::groups::Kinds {
    groups: true,
    teams: false,
};
const POLICY: SessionPolicy = SessionPolicy {
    idle: Duration::hours(1),
    remember: Duration::days(30),
};

fn passwords() -> Passwords {
    Passwords::new(4)
}

fn origin() -> Origin {
    Origin::default()
}

fn letters() -> Letters {
    Letters {
        enabled: true,
        base_url: Some("https://tracker.example.com".to_owned()),
        language: Language::En,
    }
}

/// Runs the setup; returns the administrator.
async fn set_up(pool: &PgPool) -> SignedIn {
    setup::prepare(pool, Some("TEST-CODE-2345")).await.unwrap();
    setup::run(
        pool,
        &passwords(),
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
        T0,
        &origin(),
    )
    .await
    .unwrap()
}

async fn access_of(pool: &PgPool, user: UserId) -> Access {
    access::load(pool, user).await.unwrap()
}

async fn create(pool: &PgPool, actor: &Access, username: &str, full_name: &str) -> UserId {
    users::create(
        pool,
        &passwords(),
        actor,
        NewAccount {
            username: username.to_owned(),
            full_name: full_name.to_owned(),
            email: format!("{username}@example.com"),
            password: "correct horse".to_owned(),
            must_change_password: false,
        },
    )
    .await
    .unwrap()
    .id
}

async fn sign_in(pool: &PgPool, login: &str, password: &str) -> Result<SignedIn, Error> {
    auth::sign_in(
        pool,
        &passwords(),
        SignIn {
            login: login.to_owned(),
            password: password.to_owned(),
            remember: false,
        },
        POLICY,
        T0,
        &origin(),
    )
    .await
}

async fn role_id(pool: &PgPool, role: PredefinedRole) -> RoleId {
    taskmaster_store::roles::builtin_id(pool, role.key())
        .await
        .unwrap()
        .unwrap()
}

/// The secret of the newest queued email's link.
async fn last_link(pool: &PgPool) -> (String, String) {
    let row = sqlx::query!(
        r#"select payload->>'to' as "to!", payload->>'text' as "text!" from jobs
           where kind = 'mail.send' order by id desc limit 1"#
    )
    .fetch_one(pool)
    .await
    .unwrap();
    let link = row
        .text
        .split_whitespace()
        .find(|word| word.starts_with("https://tracker.example.com/") && word.contains('#'))
        .unwrap_or_else(|| panic!("no link in {}", row.text));
    let secret = link.split('#').nth(1).unwrap().to_owned();
    (row.to, secret)
}

async fn mails(pool: &PgPool) -> i64 {
    sqlx::query_scalar!(r#"select count(*) as "n!" from jobs where kind = 'mail.send'"#)
        .fetch_one(pool)
        .await
        .unwrap()
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

fn missing(error: Error) -> Vec<&'static str> {
    match error {
        Error::Forbidden(missing) => missing.keys(),
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[tokio::test]
async fn acc_001_acc_021_acc_052_a_new_instance_has_its_groups_roles_and_administrator() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = set_up(pool).await.user;
    let access = access_of(pool, admin.id).await;
    // ACC-052: the system administrator has every permission.
    assert!(Permission::ALL.iter().all(|p| access.has(*p)));
    assert_eq!(access.groups().len(), 2, "Everyone and All members");

    // ACC-021: a new account holds the directory viewer role through All members.
    let bob = create(pool, &access, "bob", "Bob Builder").await;
    let bob_access = access_of(pool, bob).await;
    assert!(bob_access.has(Permission::UserDirectory));
    assert!(bob_access.has(Permission::UserDetails));
    assert!(bob_access.has(Permission::UserSelf));
    assert!(!bob_access.has(Permission::UserCreate));
    // ACC-061: the role's source is the built-in group.
    let bob_user = taskmaster_store::users::get(pool, bob)
        .await
        .unwrap()
        .unwrap();
    let roles = access::effective_roles(pool, &bob_user).await.unwrap();
    assert_eq!(roles.len(), 1);
    assert_eq!(roles[0].role.builtin.as_deref(), Some("directory_viewer"));
    assert!(matches!(
        &roles[0].sources[..],
        [Source::Group {
            builtin: Some(_),
            ..
        }]
    ));

    // The guest is disabled and has nothing.
    let guest: UserId = sqlx::query_scalar!("select id from users where builtin = 'guest'")
        .fetch_one(pool)
        .await
        .unwrap();
    let guest_access = access_of(pool, guest).await;
    assert!(guest_access.global().is_empty());
}

#[tokio::test]
async fn acc_003_auth_010_administrators_create_accounts_that_may_have_to_change_the_password() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = set_up(pool).await.user;
    let access = access_of(pool, admin.id).await;
    let created = users::create(
        pool,
        &passwords(),
        &access,
        NewAccount {
            username: "carol".to_owned(),
            full_name: "Carol".to_owned(),
            email: "carol@example.com".to_owned(),
            password: "first password".to_owned(),
            must_change_password: true,
        },
    )
    .await
    .unwrap();
    assert!(created.email_verified_at.is_none());

    // Taken values are reported all at once.
    let taken = users::create(
        pool,
        &passwords(),
        &access,
        NewAccount {
            username: "CAROL".to_owned(),
            full_name: "Carol 2".to_owned(),
            email: "Carol@Example.com".to_owned(),
            password: "first password".to_owned(),
            must_change_password: false,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(
        problems(taken),
        [
            ("username".to_owned(), Problem::Taken),
            ("email".to_owned(), Problem::Taken)
        ]
    );

    // A member cannot create accounts (ACC-036).
    let member = access_of(pool, created.id).await;
    let refused = users::create(
        pool,
        &passwords(),
        &member,
        NewAccount {
            username: "dave".to_owned(),
            full_name: "Dave".to_owned(),
            email: "dave@example.com".to_owned(),
            password: "first password".to_owned(),
            must_change_password: false,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(missing(refused), ["user.create"]);

    // AUTH-010: the first sign-in must choose a new password, which then clears the flag.
    let signed_in = sign_in(pool, "carol", "first password").await.unwrap();
    assert!(signed_in.session.must_change_password);
    let principal = auth::authenticate_session(pool, &signed_in.secret.value, POLICY, T0)
        .await
        .unwrap()
        .unwrap();
    assert!(principal.must_change_password());
    taskmaster_service::account::change_password(
        pool,
        &passwords(),
        &principal,
        "first password",
        "my own password",
    )
    .await
    .unwrap();
    let again = sign_in(pool, "carol", "my own password").await.unwrap();
    assert!(!again.session.must_change_password);

    // An administrator can require it again, for others only.
    users::require_password_change(pool, &access, created.id)
        .await
        .unwrap();
    assert!(
        sign_in(pool, "carol", "my own password")
            .await
            .unwrap()
            .session
            .must_change_password
    );
    assert!(matches!(
        users::require_password_change(pool, &access, admin.id).await,
        Err(Error::Conflict(_))
    ));
}

#[tokio::test]
async fn acc_004_invited_people_join_with_their_own_password() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = set_up(pool).await.user;
    let access = access_of(pool, admin.id).await;
    create(pool, &access, "eve", "Eve").await;

    let invited = users::invite(
        pool,
        &access,
        &letters(),
        "eve@other.org, frank.k+team@example.com",
        T0,
    )
    .await
    .unwrap();
    assert_eq!(invited.len(), 2);
    // The username comes from the address, with a number when taken.
    assert_eq!(invited[0].username, "eve2");
    assert_eq!(invited[1].username, "frank.k");
    assert_eq!(invited[1].full_name, "frank.k+team");
    assert!(invited.iter().all(|user| user.invited_at.is_some()));
    assert_eq!(mails(pool).await, 2);

    // An address with an account stops the whole invitation.
    let taken = users::invite(
        pool,
        &access,
        &letters(),
        "new@example.com EVE@OTHER.ORG",
        T0,
    )
    .await
    .unwrap_err();
    assert_eq!(problems(taken), [("emails".to_owned(), Problem::Taken)]);
    // Without outgoing email nothing is created.
    let mut off = letters();
    off.enabled = false;
    assert!(matches!(
        users::invite(pool, &access, &off, "x@example.com", T0).await,
        Err(Error::Conflict(_))
    ));

    // Resending replaces the link: the first one stops working.
    let (_, first) = last_link(pool).await;
    users::resend_invitation(pool, &access, &letters(), invited[1].id, T0)
        .await
        .unwrap();
    let (to, second) = last_link(pool).await;
    assert_eq!(to, "frank.k+team@example.com");
    assert_ne!(first, second);
    let info = links::inspect(pool, &second, T0).await.unwrap();
    assert_eq!(info.username, "frank.k");
    assert!(!info.expired);

    let acceptance = |password: &str| Acceptance {
        password: password.to_owned(),
        full_name: Some("Frank Kent".to_owned()),
        username: Some("frank".to_owned()),
    };
    assert!(matches!(
        links::accept_invitation(
            pool,
            &passwords(),
            &first,
            acceptance("frank's password"),
            T0,
            &origin()
        )
        .await,
        Err(Error::NotFound(_))
    ));
    let joined = links::accept_invitation(
        pool,
        &passwords(),
        &second,
        acceptance("frank's password"),
        T0,
        &origin(),
    )
    .await
    .unwrap();
    assert_eq!(joined.user.username, "frank");
    assert_eq!(joined.user.full_name, "Frank Kent");
    assert!(joined.user.invited_at.is_none());
    assert!(joined.user.email_verified_at.is_some());
    // The link works once.
    assert!(matches!(
        links::accept_invitation(
            pool,
            &passwords(),
            &second,
            acceptance("x password"),
            T0,
            &origin()
        )
        .await,
        Err(Error::NotFound(_))
    ));
    sign_in(pool, "frank", "frank's password").await.unwrap();
    assert!(matches!(
        users::resend_invitation(pool, &access, &letters(), joined.user.id, T0).await,
        Err(Error::Conflict(_))
    ));
}

#[tokio::test]
async fn auth_008_a_forgotten_password_is_reset_from_a_verified_address() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = set_up(pool).await;
    let access = access_of(pool, admin.user.id).await;
    let bob = create(pool, &access, "bob", "Bob").await;

    // Unknown and unverified addresses answer the same and send nothing.
    links::request_password_reset(pool, &letters(), "nobody@example.com", T0)
        .await
        .unwrap();
    links::request_password_reset(pool, &letters(), "bob@example.com", T0)
        .await
        .unwrap();
    assert_eq!(mails(pool).await, 0);

    // The administrator's address was verified by the setup.
    links::request_password_reset(pool, &letters(), "ANNA@example.com", T0)
        .await
        .unwrap();
    assert_eq!(mails(pool).await, 1);
    // A second request within a minute sends nothing more.
    links::request_password_reset(
        pool,
        &letters(),
        "anna@example.com",
        T0 + Duration::seconds(30),
    )
    .await
    .unwrap();
    assert_eq!(mails(pool).await, 1);
    let (to, secret) = last_link(pool).await;
    assert_eq!(to, "anna@example.com");

    // Expired after an hour.
    assert!(matches!(
        links::reset_password(
            pool,
            &passwords(),
            &secret,
            "new password!",
            T0 + Duration::hours(2),
            &origin()
        )
        .await,
        Err(Error::Conflict(_))
    ));
    let other = sign_in(pool, "anna@example.com", "correct horse")
        .await
        .unwrap();
    let reset = links::reset_password(pool, &passwords(), &secret, "new password!", T0, &origin())
        .await
        .unwrap();
    assert_eq!(reset.user.id, admin.user.id);
    // Every older session ended; the old password no longer works.
    assert_eq!(
        sessions::count_for_user(pool, admin.user.id).await.unwrap(),
        1
    );
    assert!(
        auth::authenticate_session(pool, &other.secret.value, POLICY, T0)
            .await
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        sign_in(pool, "anna@example.com", "correct horse").await,
        Err(Error::InvalidCredentials)
    ));
    sign_in(pool, "anna@example.com", "new password!")
        .await
        .unwrap();
    assert!(matches!(
        links::reset_password(pool, &passwords(), &secret, "again password", T0, &origin()).await,
        Err(Error::NotFound(_))
    ));
    let _ = bob;
}

#[tokio::test]
async fn auth_014_acc_017_a_changed_email_is_confirmed_by_a_link() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = set_up(pool).await.user;
    let admin_access = access_of(pool, admin.id).await;
    let bob = create(pool, &admin_access, "bob", "Bob").await;
    let bob_access = access_of(pool, bob).await;
    let version = |id| async move {
        taskmaster_store::users::get(pool, id)
            .await
            .unwrap()
            .unwrap()
            .version
    };

    // One's own email needs the current password.
    let change = |password: Option<&str>, version: i64| IdentityInput {
        email: Some("bob@new.example.com".to_owned()),
        current_password: password.map(str::to_owned),
        version,
        ..IdentityInput::default()
    };
    let v = version(bob).await;
    let refused = users::update_identity(
        pool,
        &passwords(),
        &bob_access,
        &letters(),
        bob,
        change(Some("wrong"), v),
        T0,
    )
    .await
    .unwrap_err();
    assert_eq!(
        problems(refused),
        [("current_password".to_owned(), Problem::Incorrect)]
    );
    let changed = users::update_identity(
        pool,
        &passwords(),
        &bob_access,
        &letters(),
        bob,
        change(Some("correct horse"), v),
        T0,
    )
    .await
    .unwrap();
    assert_eq!(changed.email.as_deref(), Some("bob@new.example.com"));
    assert!(changed.email_verified_at.is_none());
    let (to, secret) = last_link(pool).await;
    assert_eq!(to, "bob@new.example.com");
    assert_eq!(
        links::confirm_email(pool, &secret, T0).await.unwrap(),
        "bob@new.example.com"
    );
    let verified = taskmaster_store::users::get(pool, bob)
        .await
        .unwrap()
        .unwrap();
    assert!(verified.email_verified_at.is_some());

    // A stale version is a conflict; names change without a password.
    let stale = users::update_identity(
        pool,
        &passwords(),
        &bob_access,
        &letters(),
        bob,
        IdentityInput {
            full_name: Some("Robert".to_owned()),
            version: v,
            ..IdentityInput::default()
        },
        T0,
    )
    .await;
    assert!(matches!(stale, Err(Error::Conflict(_))));
    // Someone else's profile needs user.update.
    let refused = users::update_identity(
        pool,
        &passwords(),
        &bob_access,
        &letters(),
        admin.id,
        IdentityInput {
            full_name: Some("Hacked".to_owned()),
            version: version(admin.id).await,
            ..IdentityInput::default()
        },
        T0,
    )
    .await
    .unwrap_err();
    assert_eq!(missing(refused), ["user.update"]);
    // An administrator renames anyone; a taken username is refused.
    let taken = users::update_identity(
        pool,
        &passwords(),
        &admin_access,
        &letters(),
        bob,
        IdentityInput {
            username: Some("Admin".to_owned()),
            version: version(bob).await,
            ..IdentityInput::default()
        },
        T0,
    )
    .await
    .unwrap_err();
    assert_eq!(problems(taken), [("username".to_owned(), Problem::Taken)]);
}

#[tokio::test]
async fn acc_005_disabled_accounts_lose_access_at_once() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = set_up(pool).await.user;
    let access = access_of(pool, admin.id).await;
    let bob = create(pool, &access, "bob", "Bob").await;
    let session = sign_in(pool, "bob", "correct horse").await.unwrap();

    users::disable(pool, &access, &[bob], "left in May", DisabledLabel::Left)
        .await
        .unwrap();
    let user = taskmaster_store::users::get(pool, bob)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(user.disabled_reason.as_deref(), Some("left in May"));
    assert_eq!(user.disabled_label.as_deref(), Some("left"));
    assert!(
        auth::authenticate_session(pool, &session.secret.value, POLICY, T0)
            .await
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        sign_in(pool, "bob", "correct horse").await,
        Err(Error::InvalidCredentials)
    ));
    assert!(access_of(pool, bob).await.global().is_empty());

    // Not oneself, not the guest.
    assert!(matches!(
        users::disable(pool, &access, &[admin.id], "", DisabledLabel::Disabled).await,
        Err(Error::Conflict(_))
    ));
    let guest: UserId = sqlx::query_scalar!("select id from users where builtin = 'guest'")
        .fetch_one(pool)
        .await
        .unwrap();
    assert!(matches!(
        users::enable(pool, &access, &[guest]).await,
        Err(Error::Conflict(_))
    ));

    users::enable(pool, &access, &[bob]).await.unwrap();
    sign_in(pool, "bob", "correct horse").await.unwrap();
    let rows = users::directory(pool, &access, "", StateFilter::Disabled, None, 10)
        .await
        .unwrap();
    assert!(rows.iter().all(|row| row.user.is_guest()));
}

#[tokio::test]
async fn acc_006_acc_013_deleted_accounts_resolve_to_their_replacement() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = set_up(pool).await.user;
    let access = access_of(pool, admin.id).await;
    let bob = create(pool, &access, "bob", "Bob").await;
    let carol = create(pool, &access, "carol", "Carol").await;
    let dave = create(pool, &access, "dave", "Dave").await;
    let group = groups::create(
        pool,
        &access,
        NewGroup {
            name: "Team".to_owned(),
            ..NewGroup::default()
        },
    )
    .await
    .unwrap();
    groups::add_members(pool, &access, group.id, &[bob])
        .await
        .unwrap();
    sign_in(pool, "bob", "correct horse").await.unwrap();

    let impact = users::deletion_impact(pool, &access, &[bob]).await.unwrap();
    assert_eq!(
        (impact.accounts, impact.sessions, impact.memberships),
        (1, 1, 1)
    );
    assert!(matches!(
        users::delete(pool, &access, &[admin.id], bob).await,
        Err(Error::Conflict(_))
    ));
    users::delete(pool, &access, &[bob], carol).await.unwrap();
    let gone = taskmaster_store::users::get(pool, bob)
        .await
        .unwrap()
        .unwrap();
    assert!(gone.is_deleted() && gone.email.is_none() && gone.full_name.is_empty());
    assert_eq!(sessions::count_for_user(pool, bob).await.unwrap(), 0);
    // The old ID opens the replacement's profile (ACC-013).
    assert_eq!(
        users::profile(pool, &access, bob).await.unwrap().user.id,
        carol
    );
    // Aliases stay one step when the replacement goes too.
    users::delete(pool, &access, &[carol], dave).await.unwrap();
    assert_eq!(
        users::profile(pool, &access, bob).await.unwrap().user.id,
        dave
    );
    // The username is free again; the old account cannot sign in.
    create(pool, &access, "bob", "New Bob").await;
    assert_eq!(
        sign_in(pool, "bob", "correct horse")
            .await
            .unwrap()
            .user
            .full_name,
        "New Bob"
    );
}

#[tokio::test]
async fn acc_015_acc_016_profiles_show_what_the_viewer_may_see() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = set_up(pool).await.user;
    let access = access_of(pool, admin.id).await;
    let bob = create(pool, &access, "bob", "Bob").await;
    let bob_access = access_of(pool, bob).await;
    let seen = users::profile(pool, &bob_access, admin.id).await.unwrap();
    assert_eq!(seen.tier, Tier::Details);
    assert!(seen.groups.is_some() && seen.password.is_none());
    let own = users::profile(pool, &bob_access, bob).await.unwrap();
    assert_eq!(own.tier, Tier::Full);
    assert!(own.password.unwrap().has_password);

    // Without the directory viewer role of All members, others are hidden.
    let all_members = taskmaster_store::groups::builtin_ids(pool).await.unwrap().1;
    let assignment = roles::assignments_of_holder(pool, &access, Holder::Group(all_members))
        .await
        .unwrap()[0]
        .id;
    roles::revoke(pool, &access, assignment).await.unwrap();
    let bob_access = access_of(pool, bob).await;
    assert!(matches!(
        users::profile(pool, &bob_access, admin.id).await,
        Err(Error::NotFound(_))
    ));
    assert!(
        users::directory(pool, &bob_access, "", StateFilter::All, None, 10)
            .await
            .is_err()
    );
    let names = taskmaster_service::people::Names::with_secret(b"key");
    let admin_user = taskmaster_store::users::get(pool, admin.id)
        .await
        .unwrap()
        .unwrap();
    let shown = names.show(&bob_access, &admin_user);
    assert!(matches!(
        shown.shown,
        taskmaster_service::people::Shown::Masked(_)
    ));
}

#[tokio::test]
async fn acc_070_lookup_matches_names_usernames_and_addresses() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = set_up(pool).await.user;
    let access = access_of(pool, admin.id).await;
    create(pool, &access, "jdoe", "John Doe").await;
    create(pool, &access, "mary", "Mary Jane").await;
    let find = |text: &'static str| {
        let access = access.clone();
        async move {
            users::directory(pool, &access, text, StateFilter::Enabled, None, 10)
                .await
                .unwrap()
                .into_iter()
                .map(|row| row.user.username)
                .collect::<Vec<_>>()
        }
    };
    assert_eq!(find("jo").await, ["jdoe"]);
    assert_eq!(find("doe").await, ["jdoe"], "a word of the full name");
    assert_eq!(find("MAR").await, ["mary"]);
    assert_eq!(
        find("example.c").await,
        ["admin", "jdoe", "mary"],
        "email domain, by name"
    );
    assert!(find("oh").await.is_empty(), "only starts of words");
    assert!(find("%").await.is_empty(), "wildcards are plain text");

    // Pages follow the name order.
    let first = users::directory(pool, &access, "", StateFilter::Enabled, None, 2)
        .await
        .unwrap();
    let last = first.last().unwrap();
    let next = users::directory(
        pool,
        &access,
        "",
        StateFilter::Enabled,
        Some((last.user.full_name.clone(), last.user.id)),
        2,
    )
    .await
    .unwrap();
    assert_eq!(first.len() + next.len(), 3);
    assert!(
        next.iter()
            .all(|row| first.iter().all(|seen| seen.user.id != row.user.id))
    );
}

#[tokio::test]
async fn acc_023_acc_024_acc_026_acc_032_groups_and_their_members() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = set_up(pool).await.user;
    let access = access_of(pool, admin.id).await;
    let bob = create(pool, &access, "bob", "Bob").await;
    let carol = create(pool, &access, "carol", "Carol").await;

    let devs = groups::create(
        pool,
        &access,
        NewGroup {
            name: "Developers".to_owned(),
            ..NewGroup::default()
        },
    )
    .await
    .unwrap();
    let taken = groups::create(
        pool,
        &access,
        NewGroup {
            name: "developers".to_owned(),
            ..NewGroup::default()
        },
    )
    .await
    .unwrap_err();
    assert_eq!(problems(taken), [("name".to_owned(), Problem::Taken)]);
    let reserved = groups::create(
        pool,
        &access,
        NewGroup {
            name: "Everyone".to_owned(),
            ..NewGroup::default()
        },
    )
    .await
    .unwrap_err();
    assert_eq!(problems(reserved), [("name".to_owned(), Problem::Taken)]);

    // ACC-024: auto-join adds every new account.
    let devs = groups::update(
        pool,
        &access,
        devs.id,
        GroupChange {
            auto_join: Some(true),
            version: devs.version,
            ..GroupChange::default()
        },
    )
    .await
    .unwrap();
    let dave = create(pool, &access, "dave", "Dave").await;
    assert!(access_of(pool, dave).await.in_group(devs.id));
    let (everyone, _) = taskmaster_store::groups::builtin_ids(pool).await.unwrap();
    let everyone_group = taskmaster_store::groups::get(pool, everyone)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        groups::update(
            pool,
            &access,
            everyone,
            GroupChange {
                name: Some("All".to_owned()),
                version: everyone_group.version,
                ..GroupChange::default()
            }
        )
        .await,
        Err(Error::Conflict(_))
    ));

    // ACC-026, ACC-050: adding members needs every permission the group gives.
    roles::assign(
        pool,
        &access,
        role_id(pool, PredefinedRole::AccountCreator).await,
        Holder::Group(devs.id),
        &[None],
    )
    .await
    .unwrap();
    roles::assign(
        pool,
        &access,
        role_id(pool, PredefinedRole::ProjectAdmin).await,
        Holder::User(bob),
        &[None],
    )
    .await
    .unwrap();
    let bob_access = access_of(pool, bob).await;
    assert!(bob_access.may_manage_groups().is_ok());
    let refused = groups::add_members(pool, &bob_access, devs.id, &[carol])
        .await
        .unwrap_err();
    assert_eq!(missing(refused), ["user.create"]);
    assert_eq!(
        groups::add_members(pool, &access, devs.id, &[carol])
            .await
            .unwrap(),
        [carol]
    );
    assert!(
        groups::add_members(pool, &access, devs.id, &[carol])
            .await
            .unwrap()
            .is_empty()
    );
    assert!(access_of(pool, carol).await.has(Permission::UserCreate));
    let members = groups::members(pool, &access, devs.id, "", None, 10)
        .await
        .unwrap();
    assert_eq!(
        members
            .iter()
            .map(|u| u.username.as_str())
            .collect::<Vec<_>>(),
        ["carol", "dave"]
    );
    // Built-in groups list every account they include.
    assert_eq!(
        groups::members(pool, &access, everyone, "", None, 50)
            .await
            .unwrap()
            .len(),
        5
    );

    // ACC-032: deleting moves the ID to the replacement; roles go.
    let qa = groups::create(
        pool,
        &access,
        NewGroup {
            name: "QA".to_owned(),
            ..NewGroup::default()
        },
    )
    .await
    .unwrap();
    let impact = groups::deletion_impact(pool, &access, &[devs.id])
        .await
        .unwrap();
    assert_eq!((impact.memberships, impact.assignments), (2, 1));
    assert!(matches!(
        groups::delete(pool, &access, &[everyone], qa.id).await,
        Err(Error::Conflict(_))
    ));
    groups::delete(pool, &access, &[devs.id], qa.id)
        .await
        .unwrap();
    assert_eq!(
        groups::get(pool, &access, devs.id)
            .await
            .unwrap()
            .listed
            .group
            .id,
        qa.id
    );
    assert!(!access_of(pool, carol).await.has(Permission::UserCreate));
    let listed = groups::list(pool, &access, "", GROUPS, None, 10)
        .await
        .unwrap();
    assert_eq!(
        listed
            .iter()
            .map(|g| g.listed.group.name.as_str())
            .collect::<Vec<_>>(),
        ["All members", "Everyone", "QA"]
    );
    let found = groups::list(pool, &access, "участ", GROUPS, None, 10)
        .await
        .unwrap();
    assert_eq!(found.len(), 1, "built-in names match in every language");
}

#[tokio::test]
async fn acc_046_acc_050_acc_054_acc_057_custom_roles() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = set_up(pool).await.user;
    let access = access_of(pool, admin.id).await;
    let bob = create(pool, &access, "bob", "Bob").await;

    let reviewer = roles::create(
        pool,
        &access,
        NewRole {
            name: "Reviewer".to_owned(),
            description: "Reads and comments".to_owned(),
            permissions: vec!["issue.read".to_owned(), "comment.update.any".to_owned()],
        },
    )
    .await
    .unwrap();
    // ACC-046: implied permissions come along.
    assert_eq!(
        reviewer.permissions.keys(),
        [
            "project.basic",
            "issue.read",
            "comment.read",
            "comment.update.any"
        ]
    );
    let off = roles::set_permission(
        pool,
        &access,
        reviewer.role.id,
        Permission::ProjectBasic,
        false,
    )
    .await
    .unwrap();
    assert_eq!(
        off.permissions.keys(),
        ["comment.read", "comment.update.any"]
    );
    // Predefined roles cannot change.
    let member = role_id(pool, PredefinedRole::ProjectMember).await;
    assert!(matches!(
        roles::set_permission(pool, &access, member, Permission::IssueSilent, true).await,
        Err(Error::Conflict(_))
    ));

    // ACC-050: a holder of system.write without user.delete cannot put it into a role.
    let limited = roles::create(
        pool,
        &access,
        NewRole {
            name: "Operator".to_owned(),
            description: String::new(),
            permissions: vec!["system.write".to_owned()],
        },
    )
    .await
    .unwrap();
    roles::assign(pool, &access, limited.role.id, Holder::User(bob), &[None])
        .await
        .unwrap();
    let bob_access = access_of(pool, bob).await;
    let refused = roles::set_permission(
        pool,
        &bob_access,
        reviewer.role.id,
        Permission::UserDelete,
        true,
    )
    .await
    .unwrap_err();
    // Bob sees the directory through All members; only user.delete is his to lack.
    assert_eq!(missing(refused), ["user.delete"]);

    // ACC-045: a role without project permissions cannot go to a project; projects come in 1.2.
    assert!(matches!(
        roles::assign(
            pool,
            &access,
            reviewer.role.id,
            Holder::User(bob),
            &[Some(1)]
        )
        .await,
        Err(Error::Invalid(_))
    ));

    // ACC-057: deleting a custom role revokes its assignments.
    roles::delete(pool, &access, limited.role.id).await.unwrap();
    assert!(!access_of(pool, bob).await.has(Permission::SystemWrite));
    assert!(matches!(
        roles::delete(pool, &access, member).await,
        Err(Error::Conflict(_))
    ));
    let listed = roles::list(
        pool,
        &access,
        "",
        Some(Permission::CommentUpdateAny),
        None,
        50,
    )
    .await
    .unwrap();
    assert_eq!(
        listed
            .iter()
            .map(|r| r.role.name.as_str())
            .collect::<Vec<_>>(),
        ["System administrator", "Project administrator", "Reviewer"]
    );
}

#[tokio::test]
async fn acc_058_acc_059_acc_061_the_last_administrator_stays() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = set_up(pool).await.user;
    let access = access_of(pool, admin.id).await;
    let bob = create(pool, &access, "bob", "Bob").await;
    let admins = groups::create(
        pool,
        &access,
        NewGroup {
            name: "Admins".to_owned(),
            ..NewGroup::default()
        },
    )
    .await
    .unwrap();
    let system_admin = role_id(pool, PredefinedRole::SystemAdmin).await;
    roles::assign(
        pool,
        &access,
        system_admin,
        Holder::Group(admins.id),
        &[None],
    )
    .await
    .unwrap();
    groups::add_members(pool, &access, admins.id, &[bob, admin.id])
        .await
        .unwrap();

    // ACC-061: the role shows both sources.
    let admin_user = taskmaster_store::users::get(pool, admin.id)
        .await
        .unwrap()
        .unwrap();
    let roles_now = access::effective_roles(pool, &admin_user).await.unwrap();
    let sys = roles_now
        .iter()
        .find(|r| r.role.id == system_admin)
        .unwrap();
    assert_eq!(sys.sources.len(), 2);
    let direct = sys
        .sources
        .iter()
        .find_map(|s| match s {
            Source::Direct { assignment } => Some(*assignment),
            Source::Group { .. } => None,
        })
        .unwrap();

    // Revoking the direct one is fine while the group still makes two administrators…
    roles::revoke(pool, &access, direct).await.unwrap();
    groups::remove_members(pool, &access, admins.id, &[admin.id])
        .await
        .unwrap();
    // …but nobody removes the last one, by any route.
    assert!(matches!(
        groups::remove_members(pool, &access, admins.id, &[bob]).await,
        Err(Error::LastAdministrator)
    ));
    assert!(matches!(
        users::disable(pool, &access, &[bob], "", DisabledLabel::Disabled).await,
        Err(Error::LastAdministrator)
    ));
    let group_assignment = roles::assignments_of_holder(pool, &access, Holder::Group(admins.id))
        .await
        .unwrap()[0]
        .id;
    assert!(matches!(
        roles::revoke(pool, &access, group_assignment).await,
        Err(Error::LastAdministrator)
    ));
    let other = groups::create(
        pool,
        &access,
        NewGroup {
            name: "Other".to_owned(),
            ..NewGroup::default()
        },
    )
    .await
    .unwrap();
    assert!(matches!(
        groups::delete(pool, &access, &[admins.id], other.id).await,
        Err(Error::LastAdministrator)
    ));
    assert!(access_of(pool, bob).await.has(Permission::SystemWrite));
}

#[tokio::test]
async fn acc_064_acc_066_capabilities_follow_groups() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = set_up(pool).await.user;
    let access = access_of(pool, admin.id).await;
    let bob = create(pool, &access, "bob", "Bob").await;
    // ACC-066: on for Everyone by default.
    assert!(
        access_of(pool, bob)
            .await
            .capability(Capability::ShareViews)
    );
    let list = capabilities::list(pool, &access).await.unwrap();
    assert_eq!(list[0].groups.len(), 1);

    let leads: GroupId = groups::create(
        pool,
        &access,
        NewGroup {
            name: "Leads".to_owned(),
            ..NewGroup::default()
        },
    )
    .await
    .unwrap()
    .id;
    capabilities::set(pool, &access, Capability::ShareViews, &[leads])
        .await
        .unwrap();
    assert!(
        !access_of(pool, bob)
            .await
            .capability(Capability::ShareViews)
    );
    groups::add_members(pool, &access, leads, &[bob])
        .await
        .unwrap();
    assert!(
        access_of(pool, bob)
            .await
            .capability(Capability::ShareViews)
    );
    // Switching off for everybody is allowed for this one.
    capabilities::set(pool, &access, Capability::ShareViews, &[])
        .await
        .unwrap();
    assert!(
        !access_of(pool, bob)
            .await
            .capability(Capability::ShareViews)
    );
    let bob_access = access_of(pool, bob).await;
    assert_eq!(
        missing(
            capabilities::set(pool, &bob_access, Capability::ShareViews, &[leads])
                .await
                .unwrap_err()
        ),
        ["system.write"]
    );
}
