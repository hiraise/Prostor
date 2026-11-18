//! Notification settings and subscriptions, the watcher list, the creator's project subscription
//! (NTF-015, NTF-017, NTF-021…NTF-027, NTF-030, NTF-034, NTF-066, PRJ-019; plan step 1.10 §6.1)
//! on a real database.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use prostor_domain::access::Access;
use prostor_domain::access::capability::Capability;
use prostor_domain::access::role::PredefinedRole;
use prostor_domain::issues::history::Origin;
use prostor_domain::notifications::{Kind, Settings, Source};
use prostor_domain::projects::Template;
use prostor_domain::settings::Language;
use prostor_domain::sharing::Audience;
use prostor_domain::{FieldId, IssueId, ProjectId, UserId};
use prostor_service::comments;
use prostor_service::groups::{self, NewGroup};
use prostor_service::issues::{self, NewIssue};
use prostor_service::notifications::{self as ntf};
use prostor_service::projects::{self, NewProjectInput};
use prostor_service::roles::NewRole;
use prostor_service::rules;
use prostor_service::saved_searches::{self as saved, SearchChange, Sharing as SearchSharing};
use prostor_service::setup::{self, Setup};
use prostor_service::tags::{self, Sharing as TagSharing, TagChange};
use prostor_service::users::{self, NewAccount};
use prostor_service::{Error, Passwords, access, capabilities, roles};
use prostor_store::PgPool;
use prostor_store::access::Holder;
use prostor_store::saved_searches::Listing;
use prostor_store::sessions::Origin as SessionOrigin;
use prostor_testkit::TestDb;
use time::OffsetDateTime;
use time::macros::datetime;

const T0: OffsetDateTime = datetime!(2026-10-01 09:00 UTC);

fn passwords() -> Passwords {
    Passwords::new(4)
}

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

async fn project(pool: &PgPool, actor: &Access, key: &str, template: Template) -> ProjectId {
    projects::create(
        pool,
        actor,
        NewProjectInput {
            template,
            name: format!("Project {key}"),
            key: Some(key.to_owned()),
            first_number: None,
            description: String::new(),
        },
        Language::En,
    )
    .await
    .unwrap()
    .project
    .id
}

/// A member of the projects.
async fn member(pool: &PgPool, admin: &Access, username: &str, of: &[ProjectId]) -> Access {
    let user = users::create(
        pool,
        &passwords(),
        admin,
        NewAccount {
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
    let role = prostor_store::roles::builtin_id(pool, PredefinedRole::ProjectMember.key())
        .await
        .unwrap()
        .unwrap();
    let scopes: Vec<Option<ProjectId>> = of.iter().map(|p| Some(*p)).collect();
    roles::assign(pool, admin, role, Holder::User(user), &scopes)
        .await
        .unwrap();
    access::load(pool, user).await.unwrap()
}

async fn reload(pool: &PgPool, who: &Access) -> Access {
    access::load(pool, who.user()).await.unwrap()
}

/// A project created in Russian, so its "unassigned" search has a Russian name (PRJ-019).
async fn russian_project(pool: &PgPool, actor: &Access) -> ProjectId {
    projects::create(
        pool,
        actor,
        NewProjectInput {
            template: Template::Scrum,
            name: "Project WEB".to_owned(),
            key: Some("WEB".to_owned()),
            first_number: None,
            description: String::new(),
        },
        Language::Ru,
    )
    .await
    .unwrap()
    .project
    .id
}

fn users_(ids: &[UserId]) -> Audience {
    Audience {
        users: ids.to_vec(),
        groups: Vec::new(),
    }
}

/// Lets `who` share (the capability goes to groups), then reloads them.
async fn sharer(pool: &PgPool, admin: &Access, who: &Access) -> Access {
    static CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let nth = CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let id = groups::create(
        pool,
        admin,
        NewGroup {
            name: format!("Sharers {}", nth),
            description: String::new(),
            auto_join: false,
        },
    )
    .await
    .unwrap()
    .id;
    groups::add_members(pool, admin, id, &[who.user()])
        .await
        .unwrap();
    capabilities::set(pool, admin, Capability::ShareViews, &[id])
        .await
        .unwrap();
    reload(pool, who).await
}

/// The world of the subscription tests: two members of one project.
struct World {
    admin: Access,
    anna: Access,
    bob: Access,
    web: ProjectId,
}

async fn world(pool: &PgPool) -> World {
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB", Template::Scrum).await;
    let anna = member(pool, &admin, "anna", &[web]).await;
    let bob = member(pool, &admin, "bob", &[web]).await;
    World {
        admin,
        anna,
        bob,
        web,
    }
}

/// Anna's tag, shared for viewing with Bob.
async fn shared_tag(pool: &PgPool, w: &World) -> prostor_store::tags::Tag {
    let anna = sharer(pool, &w.admin, &w.anna).await;
    let tag = tags::create(pool, &anna, "Release", None).await.unwrap();
    tags::update(
        pool,
        &anna,
        tag.id,
        TagChange {
            sharing: Some(TagSharing {
                view: users_(&[w.bob.user()]),
                ..TagSharing::default()
            }),
            version: tag.version,
            ..TagChange::default()
        },
    )
    .await
    .unwrap()
}

/// Anna's saved search, shared for viewing with Bob.
async fn shared_search(pool: &PgPool, w: &World) -> prostor_service::saved_searches::SavedSearch {
    let anna = sharer(pool, &w.admin, &w.anna).await;
    let s = saved::create(pool, &anna, "Open ones", "State: Open")
        .await
        .unwrap();
    saved::update(
        pool,
        &anna,
        s.row.id,
        SearchChange {
            sharing: Some(SearchSharing {
                view: users_(&[w.bob.user()]),
                ..SearchSharing::default()
            }),
            version: s.row.version,
            ..SearchChange::default()
        },
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn ntf_001_ntf_017_defaults_and_watch_kinds() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;

    // No row: the defaults. The watch subscription has every kind on (NTF-017), `system` never.
    // The inbox channel has no switch at all (NTF-001): the settings say only email and kinds.
    let before = ntf::get_settings(pool, w.anna.user()).await.unwrap();
    assert_eq!(before, Settings::default());
    for kind in [
        Kind::Created,
        Kind::Updated,
        Kind::Resolved,
        Kind::Comment,
        Kind::Vote,
        Kind::TagAdded,
        Kind::TagRemoved,
    ] {
        assert!(
            before.watch_allows(kind),
            "{kind:?} should be on by default"
        );
    }
    assert!(!before.watch_allows(Kind::System));

    // The first change writes the row; editing the watch kinds changes what watching delivers.
    let changed = ntf::set_settings(
        pool,
        w.anna.user(),
        Settings {
            watch_updated: false,
            watch_tag: false,
            ..before
        },
    )
    .await
    .unwrap();
    assert!(!changed.watch_allows(Kind::Updated));
    assert!(!changed.watch_allows(Kind::TagRemoved));
    assert!(changed.watch_allows(Kind::Comment));
    assert_eq!(
        ntf::get_settings(pool, w.anna.user()).await.unwrap(),
        changed
    );

    // The settings of one person are not the settings of another.
    assert_eq!(
        ntf::get_settings(pool, w.bob.user()).await.unwrap(),
        Settings::default()
    );
}

#[tokio::test]
async fn ntf_030_own_changes_flag() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;

    // Own changes are off by default (NTF-030) and can be switched on.
    let before = ntf::get_settings(pool, w.anna.user()).await.unwrap();
    assert!(!before.own_changes);
    let on = ntf::set_settings(
        pool,
        w.anna.user(),
        Settings {
            own_changes: true,
            ..before
        },
    )
    .await
    .unwrap();
    assert!(on.own_changes);
    assert!(
        ntf::get_settings(pool, w.anna.user())
            .await
            .unwrap()
            .own_changes
    );
}

#[tokio::test]
async fn ntf_034_mentions_flag() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;

    // Mentions are on by default (NTF-034) and can be switched off.
    let before = ntf::get_settings(pool, w.anna.user()).await.unwrap();
    assert!(before.mentions);
    let off = ntf::set_settings(
        pool,
        w.anna.user(),
        Settings {
            mentions: false,
            ..before
        },
    )
    .await
    .unwrap();
    assert!(!off.mentions);
    assert!(
        !ntf::get_settings(pool, w.anna.user())
            .await
            .unwrap()
            .mentions
    );
}

#[tokio::test]
async fn ntf_066_format_flag() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;

    // HTML is the default form (NTF-066); plain text is a choice. The email channel itself
    // stays on either way.
    let before = ntf::get_settings(pool, w.anna.user()).await.unwrap();
    assert!(before.email && before.email_html);
    let plain = ntf::set_settings(
        pool,
        w.anna.user(),
        Settings {
            email_html: false,
            ..before
        },
    )
    .await
    .unwrap();
    assert!(plain.email);
    assert!(!plain.email_html);
}

#[tokio::test]
async fn ntf_021_subscription_model() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let tag = shared_tag(pool, &w).await;

    let sub = ntf::subscribe(
        pool,
        &w.anna,
        Source::Tag(tag.id),
        &[Kind::Comment, Kind::Created],
    )
    .await
    .unwrap();
    assert_eq!(sub.name, "Release");
    assert_eq!(sub.row.kinds, vec![Kind::Created, Kind::Comment]);

    // Subscriptions are personal (NTF-021): Bob subscribes to the same shared tag with his own
    // kinds, and each list shows only their own.
    let bob_sub = ntf::subscribe(pool, &w.bob, Source::Tag(tag.id), &[Kind::Vote])
        .await
        .unwrap();
    let anna_list = ntf::subscriptions(pool, w.anna.user()).await.unwrap();
    assert_eq!(anna_list.len(), 1);
    assert_eq!(anna_list[0].row.id, sub.row.id);
    let bob_list = ntf::subscriptions(pool, w.bob.user()).await.unwrap();
    assert_eq!(bob_list.len(), 1);
    assert_eq!(bob_list[0].row.id, bob_sub.row.id);
    assert_eq!(bob_list[0].row.kinds, vec![Kind::Vote]);
}

#[tokio::test]
async fn ntf_022_kinds_per_source() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let tag = tags::create(pool, &w.anna, "Release", None).await.unwrap();
    let s = saved::create(pool, &w.anna, "Mine", "State: Open")
        .await
        .unwrap();

    // A tag takes all seven subscribable kinds, the two tag kinds included.
    ntf::subscribe(
        pool,
        &w.anna,
        Source::Tag(tag.id),
        &[Kind::TagAdded, Kind::TagRemoved],
    )
    .await
    .unwrap();

    // A saved search takes none of the tag kinds (NTF-022), and nothing takes `system`.
    assert!(matches!(
        ntf::subscribe(pool, &w.anna, Source::Search(s.row.id), &[Kind::TagAdded]).await,
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        ntf::subscribe(pool, &w.anna, Source::Search(s.row.id), &[Kind::System]).await,
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        ntf::subscribe(pool, &w.anna, Source::Tag(tag.id), &[Kind::System]).await,
        Err(Error::Invalid(_))
    ));
    let ok = ntf::subscribe(
        pool,
        &w.anna,
        Source::Search(s.row.id),
        &[
            Kind::Created,
            Kind::Updated,
            Kind::Resolved,
            Kind::Comment,
            Kind::Vote,
        ],
    )
    .await
    .unwrap();
    assert_eq!(ok.row.kinds.len(), 5);
}

#[tokio::test]
async fn ntf_023_create_visibility() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let anna = sharer(pool, &w.admin, &w.anna).await;

    // Private sources: Bob is refused, without learning whether they exist.
    let tag = tags::create(pool, &anna, "Release", None).await.unwrap();
    let s = saved::create(pool, &anna, "Mine", "State: Open")
        .await
        .unwrap();
    assert!(matches!(
        ntf::subscribe(pool, &w.bob, Source::Tag(tag.id), &[Kind::Created]).await,
        Err(Error::Refused(_))
    ));
    assert!(matches!(
        ntf::subscribe(pool, &w.bob, Source::Search(s.row.id), &[Kind::Created]).await,
        Err(Error::Refused(_))
    ));
    assert!(matches!(
        ntf::subscribe(pool, &w.bob, Source::Tag(999_999), &[Kind::Created]).await,
        Err(Error::Refused(_))
    ));

    // Shared for viewing: both kinds of source open up (NTF-023).
    tags::update(
        pool,
        &anna,
        tag.id,
        TagChange {
            sharing: Some(TagSharing {
                view: users_(&[w.bob.user()]),
                ..TagSharing::default()
            }),
            version: tag.version,
            ..TagChange::default()
        },
    )
    .await
    .unwrap();
    ntf::subscribe(pool, &w.bob, Source::Tag(tag.id), &[Kind::Created])
        .await
        .unwrap();
    let shared = shared_search(pool, &w).await;
    ntf::subscribe(
        pool,
        &w.bob,
        Source::Search(shared.row.id),
        &[Kind::Created],
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn ntf_024_edit_remove() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let tag = shared_tag(pool, &w).await;

    let sub = ntf::subscribe(
        pool,
        &w.bob,
        Source::Tag(tag.id),
        &[Kind::Created, Kind::Comment],
    )
    .await
    .unwrap();
    let edited = ntf::set_kinds(pool, &w.bob, sub.row.id, &[Kind::Vote])
        .await
        .unwrap();
    assert_eq!(edited.row.kinds, vec![Kind::Vote]);

    // No ticks at all: the subscription stays, delivering nothing (NTF-024).
    let empty = ntf::set_kinds(pool, &w.bob, sub.row.id, &[]).await.unwrap();
    assert!(empty.row.kinds.is_empty());
    assert_eq!(
        ntf::subscriptions(pool, w.bob.user()).await.unwrap().len(),
        1
    );

    // Still mine when the source stops being visible (NTF-025's rule).
    let anna = sharer(pool, &w.admin, &w.anna).await;
    tags::update(
        pool,
        &anna,
        tag.id,
        TagChange {
            sharing: Some(TagSharing::default()),
            version: tag.version,
            ..TagChange::default()
        },
    )
    .await
    .unwrap();
    ntf::set_kinds(pool, &w.bob, sub.row.id, &[Kind::Created])
        .await
        .unwrap();

    // Only the owner of a subscription changes or removes it; anyone else's is "not found".
    assert!(matches!(
        ntf::set_kinds(pool, &w.anna, sub.row.id, &[Kind::Vote]).await,
        Err(Error::NotFound(_))
    ));
    assert!(matches!(
        ntf::remove(pool, &w.anna, sub.row.id).await,
        Err(Error::NotFound(_))
    ));
    ntf::remove(pool, &w.bob, sub.row.id).await.unwrap();
    assert!(
        ntf::subscriptions(pool, w.bob.user())
            .await
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        ntf::remove(pool, &w.bob, sub.row.id).await,
        Err(Error::NotFound(_))
    ));

    // One subscription per person and source: Anna on her own tag, twice.
    ntf::subscribe(pool, &w.anna, Source::Tag(tag.id), &[Kind::Created])
        .await
        .unwrap();
    assert!(matches!(
        ntf::subscribe(pool, &w.anna, Source::Tag(tag.id), &[Kind::Vote]).await,
        Err(Error::Invalid(_))
    ));
}

#[tokio::test]
async fn ntf_025_dialog_data() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let tag = tags::create(pool, &w.anna, "Release", None).await.unwrap();
    let s = saved::create(pool, &w.anna, "Mine", "State: Open")
        .await
        .unwrap();

    // The tag and saved-search dialogs answer the person's subscription state (NTF-025).
    assert!(
        ntf::subscription_on(pool, &w.anna, Source::Tag(tag.id))
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        ntf::subscription_on(pool, &w.anna, Source::Search(s.row.id))
            .await
            .unwrap()
            .is_none()
    );
    let sub = ntf::subscribe(pool, &w.anna, Source::Tag(tag.id), &[Kind::Created])
        .await
        .unwrap();
    let found = ntf::subscription_on(pool, &w.anna, Source::Tag(tag.id))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.row.id, sub.row.id);
    assert_eq!(found.name, "Release");
}

#[tokio::test]
async fn ntf_027_project_search() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = russian_project(pool, &admin).await;

    // A new project has its "unassigned" search, owned by its creator (PRJ-019, NTF-027).
    let searches = saved::list(pool, &admin, Language::Ru, Listing::Mine, None, 200)
        .await
        .unwrap();
    let unassigned = searches
        .iter()
        .find(|s| s.row.project_id == Some(web))
        .unwrap();
    assert_eq!(unassigned.row.name, "Неназначенные в WEB");
    assert_eq!(unassigned.row.owner_id, admin.user());
}

#[tokio::test]
async fn prj_019_creator_subscription() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = russian_project(pool, &admin).await;

    let searches = saved::list(pool, &admin, Language::Ru, Listing::Mine, None, 200)
        .await
        .unwrap();
    let unassigned = searches
        .iter()
        .find(|s| s.row.project_id == Some(web))
        .unwrap();

    // The creator is subscribed to it, for new items only (PRJ-019, NTF-027).
    let subs = ntf::subscriptions(pool, admin.user()).await.unwrap();
    let sub = subs
        .iter()
        .find(|s| s.row.saved_search_id == Some(unassigned.row.id))
        .unwrap();
    assert_eq!(sub.row.kinds, vec![Kind::Created]);
    assert_eq!(sub.name, "Неназначенные в WEB");

    // Nobody else: a member of the project starts with no subscription.
    let anna = member(pool, &admin, "anna", &[web]).await;
    assert!(
        ntf::subscriptions(pool, anna.user())
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn ntf_015_watchers_list_rights() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB", Template::Scrum).await;
    let anna = member(pool, &admin, "anna", &[web]).await;

    // A reader without `issue.watchers.read` (NTF-015).
    let quiet = users::create(
        pool,
        &passwords(),
        &admin,
        NewAccount {
            username: "quiet".to_owned(),
            full_name: "Quiet Person".to_owned(),
            email: "quiet@example.com".to_owned(),
            password: "correct horse".to_owned(),
            must_change_password: false,
        },
    )
    .await
    .unwrap()
    .id;
    let role = roles::create(
        pool,
        &admin,
        NewRole {
            name: "Quiet reader".to_owned(),
            description: String::new(),
            permissions: vec!["project.basic".to_owned(), "issue.read".to_owned()],
        },
    )
    .await
    .unwrap()
    .role
    .id;
    roles::assign(pool, &admin, role, Holder::User(quiet), &[Some(web)])
        .await
        .unwrap();
    let quiet = access::load(pool, quiet).await.unwrap();

    // Someone with no access to the project at all.
    let outsider = users::create(
        pool,
        &passwords(),
        &admin,
        NewAccount {
            username: "vera".to_owned(),
            full_name: "Vera Person".to_owned(),
            email: "vera@example.com".to_owned(),
            password: "correct horse".to_owned(),
            must_change_password: false,
        },
    )
    .await
    .unwrap()
    .id;
    let outsider = access::load(pool, outsider).await.unwrap();

    let x = issues::create(
        pool,
        &admin,
        Origin::Web,
        NewIssue {
            project: web,
            summary: "Something".to_owned(),
            publish: true,
            watchers: vec![admin.user(), anna.user()],
            ..NewIssue::default()
        },
    )
    .await
    .unwrap()
    .done
    .0
    .issue
    .id;

    // Behind `issue.watchers.read`: the admin and a member see the people in the order they
    // came; a reader without the permission is refused; an outsider gets "not found".
    let seen = ntf::watchers(pool, &admin, x).await.unwrap();
    let ids: Vec<UserId> = seen.iter().map(|u| u.id).collect();
    assert_eq!(ids, vec![admin.user(), anna.user()]);
    let seen = ntf::watchers(pool, &anna, x).await.unwrap();
    assert_eq!(seen.len(), 2);
    assert!(matches!(
        ntf::watchers(pool, &quiet, x).await,
        Err(Error::Refused(_))
    ));
    assert!(matches!(
        ntf::watchers(pool, &outsider, x).await,
        Err(Error::NotFound(_))
    ));
}

/// NTF-016: the request replaces the whole list. Moving anyone but the caller needs
/// `issue.watchers.update`; one's own marker moves without it, as the star moves it. Unknown
/// accounts are refused.
#[tokio::test]
async fn ntf_016_manage_watchers() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB", Template::Scrum).await;
    let anna = member(pool, &admin, "anna", &[web]).await;

    // A reader without `issue.watchers.update` (a project member has it, NTF-015's test shows).
    let quiet = users::create(
        pool,
        &passwords(),
        &admin,
        NewAccount {
            username: "quiet".to_owned(),
            full_name: "Quiet Person".to_owned(),
            email: "quiet@example.com".to_owned(),
            password: "correct horse".to_owned(),
            must_change_password: false,
        },
    )
    .await
    .unwrap()
    .id;
    let role = roles::create(
        pool,
        &admin,
        NewRole {
            name: "Quiet reader".to_owned(),
            description: String::new(),
            permissions: vec!["project.basic".to_owned(), "issue.read".to_owned()],
        },
    )
    .await
    .unwrap()
    .role
    .id;
    roles::assign(pool, &admin, role, Holder::User(quiet), &[Some(web)])
        .await
        .unwrap();
    let quiet = access::load(pool, quiet).await.unwrap();

    let x = watched_issue(pool, &admin, web, "Something", &[admin.user(), anna.user()]).await;

    // The reader cannot move the admin's marker, but may add and drop their own.
    assert!(matches!(
        ntf::set_watchers(pool, &quiet, x, vec![quiet.user()]).await,
        Err(Error::Forbidden(_))
    ));
    let seen = ntf::set_watchers(
        pool,
        &quiet,
        x,
        vec![admin.user(), anna.user(), quiet.user()],
    )
    .await
    .unwrap();
    assert_eq!(
        seen.iter().map(|u| u.id).collect::<Vec<_>>(),
        vec![admin.user(), anna.user(), quiet.user()]
    );
    let seen = ntf::set_watchers(pool, &quiet, x, vec![admin.user(), anna.user()])
        .await
        .unwrap();
    assert_eq!(
        seen.iter().map(|u| u.id).collect::<Vec<_>>(),
        vec![admin.user(), anna.user()]
    );

    // The admin moves anyone; an unknown account is refused and changes nothing.
    let vera = users::create(
        pool,
        &passwords(),
        &admin,
        NewAccount {
            username: "vera".to_owned(),
            full_name: "Vera Person".to_owned(),
            email: "vera@example.com".to_owned(),
            password: "correct horse".to_owned(),
            must_change_password: false,
        },
    )
    .await
    .unwrap()
    .id;
    let seen = ntf::set_watchers(pool, &admin, x, vec![admin.user(), vera])
        .await
        .unwrap();
    assert_eq!(
        seen.iter().map(|u| u.id).collect::<Vec<_>>(),
        vec![admin.user(), vera]
    );
    assert!(matches!(
        ntf::set_watchers(pool, &admin, x, vec![admin.user(), 999_999]).await,
        Err(Error::Invalid(_))
    ));
    assert_eq!(
        ntf::watchers(pool, &admin, x)
            .await
            .unwrap()
            .iter()
            .map(|u| u.id)
            .collect::<Vec<_>>(),
        vec![admin.user(), vera]
    );
}

// ---- Auto-watch (ISS-030, plan step 1.12 §7.1) ----

/// The watchers of an issue straight from the store: these tests are about the rows, not the
/// list's own rights (NTF-015 above).
async fn watchers_of(pool: &PgPool, issue: IssueId) -> Vec<UserId> {
    let mut conn = pool.acquire().await.unwrap();
    prostor_store::notifications::issue_watchers(&mut *conn, issue)
        .await
        .unwrap()
}

/// The issue's Assignee field, as a change takes it.
async fn assignee_field(pool: &PgPool, viewer: &Access, issue: IssueId) -> FieldId {
    issues::get(pool, viewer, issue)
        .await
        .unwrap()
        .0
        .fields
        .iter()
        .find(|f| f.field.field.name == "Assignee")
        .unwrap()
        .field
        .id()
}

#[tokio::test]
async fn iss_030_autowatch_creator() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;

    // Publishing makes the creator a watcher — the switch is on by default.
    let x = watched_issue(pool, &w.anna, w.web, "Mine", &[]).await;
    assert!(watchers_of(pool, x).await.contains(&w.anna.user()));

    // The switch off leaves the creator out, draft and publication both.
    ntf::set_settings(
        pool,
        w.bob.user(),
        Settings {
            autowatch_created: false,
            ..Settings::default()
        },
    )
    .await
    .unwrap();
    let y = issues::create(
        pool,
        &w.bob,
        Origin::Web,
        NewIssue {
            project: w.web,
            summary: "Bob's".to_owned(),
            publish: false,
            ..NewIssue::default()
        },
    )
    .await
    .unwrap()
    .done
    .0
    .issue
    .id;
    assert!(!watchers_of(pool, y).await.contains(&w.bob.user()));
    issues::publish(pool, &w.bob, Origin::Web, y).await.unwrap();
    assert!(!watchers_of(pool, y).await.contains(&w.bob.user()));
}

#[tokio::test]
async fn iss_030_autowatch_comment_first_only() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;

    let x = watched_issue(pool, &w.admin, w.web, "Talk", &[]).await;
    // A first comment makes its author a watcher.
    comments::create(
        pool,
        &w.bob,
        Origin::Web,
        x,
        NewComment {
            text: "first".to_owned(),
            ..NewComment::default()
        },
    )
    .await
    .unwrap();
    assert!(watchers_of(pool, x).await.contains(&w.bob.user()));

    // A star taken off stays off: the second comment does not put it back.
    issues::set_star(pool, &w.bob, x, false).await.unwrap();
    comments::create(
        pool,
        &w.bob,
        Origin::Web,
        x,
        NewComment {
            text: "again".to_owned(),
            ..NewComment::default()
        },
    )
    .await
    .unwrap();
    assert!(!watchers_of(pool, x).await.contains(&w.bob.user()));

    // The switch off keeps even a first comment from watching.
    ntf::set_settings(
        pool,
        w.anna.user(),
        Settings {
            autowatch_commented: false,
            ..Settings::default()
        },
    )
    .await
    .unwrap();
    comments::create(
        pool,
        &w.anna,
        Origin::Web,
        x,
        NewComment {
            text: "mine".to_owned(),
            ..NewComment::default()
        },
    )
    .await
    .unwrap();
    assert!(!watchers_of(pool, x).await.contains(&w.anna.user()));
}

#[tokio::test]
async fn iss_030_autowatch_assignee() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;

    let x = watched_issue(pool, &w.admin, w.web, "Work", &[]).await;
    let field = assignee_field(pool, &w.admin, x).await;
    // The assignee field draws on the project team; both candidates join it.
    prostor_service::teams::add(pool, &w.admin, w.web, &[w.bob.user(), w.anna.user()], &[])
        .await
        .unwrap();
    // Naming someone in a user field — the assignee here — makes them a watcher.
    issues::set_field(
        pool,
        &w.admin,
        Origin::Web,
        x,
        field,
        issues::FieldChange::Set(Some(prostor_domain::issues::values::Input::Ids(vec![
            w.bob.user(),
        ]))),
        false,
    )
    .await
    .unwrap();
    assert!(watchers_of(pool, x).await.contains(&w.bob.user()));

    // An assignee given on creation watches from the publication on.
    let y = issues::create(
        pool,
        &w.admin,
        Origin::Web,
        NewIssue {
            project: w.web,
            summary: "Carried".to_owned(),
            fields: vec![issues::FieldInput {
                field,
                value: Some(prostor_domain::issues::values::Input::Ids(vec![
                    w.bob.user(),
                ])),
            }],
            publish: true,
            ..NewIssue::default()
        },
    )
    .await
    .unwrap()
    .done
    .0
    .issue
    .id;
    assert!(watchers_of(pool, y).await.contains(&w.bob.user()));

    // Their own switch off leaves the newly assigned out.
    ntf::set_settings(
        pool,
        w.anna.user(),
        Settings {
            autowatch_assigned: false,
            ..Settings::default()
        },
    )
    .await
    .unwrap();
    issues::set_field(
        pool,
        &w.admin,
        Origin::Web,
        x,
        field,
        issues::FieldChange::Set(Some(prostor_domain::issues::values::Input::Ids(vec![
            w.anna.user(),
        ]))),
        false,
    )
    .await
    .unwrap();
    assert!(!watchers_of(pool, x).await.contains(&w.anna.user()));
}

#[tokio::test]
async fn iss_030_autowatch_voted() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;

    let x = watched_issue(pool, &w.admin, w.web, "Pick", &[]).await;
    // A vote makes the voter a watcher.
    issues::set_vote(pool, &w.bob, x, true).await.unwrap();
    assert!(watchers_of(pool, x).await.contains(&w.bob.user()));
    // Withdrawing the vote keeps the star: watching ends by the star, not by the vote.
    issues::set_vote(pool, &w.bob, x, false).await.unwrap();
    assert!(watchers_of(pool, x).await.contains(&w.bob.user()));

    // The switch off: voting does not watch.
    ntf::set_settings(
        pool,
        w.anna.user(),
        Settings {
            autowatch_voted: false,
            ..Settings::default()
        },
    )
    .await
    .unwrap();
    issues::set_vote(pool, &w.anna, x, true).await.unwrap();
    assert!(!watchers_of(pool, x).await.contains(&w.anna.user()));
}

/// NTF-014: a clone takes none of its original's watchers, however many it has — the creator's
/// own auto-watch is the only marker it starts with (ISS-030, plan 1.12 §7.1).
#[tokio::test]
async fn ntf_014_clones_take_no_watchers() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    // The scrum template has the cloning toggle on; the blank one has it off.
    let web = project(pool, &admin, "WEB", Template::Scrum).await;
    let anna = member(pool, &admin, "anna", &[web]).await;
    let x = watched_issue(pool, &admin, web, "Watched", &[admin.user(), anna.user()]).await;

    let done = rules::clone::clone_issue(pool, &admin, Origin::Web, x)
        .await
        .unwrap();
    let view = issues::get(pool, &admin, done.done.0.issue.id)
        .await
        .unwrap()
        .0;
    assert_eq!(view.watchers, Some(1));
    assert!(view.starred);
    // The original's own watchers — anna among them — did not ride along.
    assert_eq!(
        ntf::watchers(pool, &admin, done.done.0.issue.id)
            .await
            .unwrap()
            .iter()
            .map(|u| u.id)
            .collect::<Vec<_>>(),
        vec![admin.user()]
    );
}

// ---------------------------------------------------------------------------
// Deliveries: events, recipients, batching (plan §6.2, 1.10a-2)
//

use std::time::Duration;

use prostor_domain::Actor;
use prostor_domain::issues::values::Input;
use prostor_domain::notifications::{Content, Line};
use prostor_service::comments::NewComment;
use prostor_service::issues::{AudienceInput, FieldChange, IssueChange};
use prostor_store::jobs::{
    self as jobs_queue, Enqueue, Handlers, JobError, JobKind, Runner, RunnerConfig,
};
use prostor_store::notifications as inbox;

/// A published issue watched by the given people.
async fn watched_issue(
    pool: &PgPool,
    author: &Access,
    project: ProjectId,
    summary: &str,
    watchers: &[UserId],
) -> prostor_domain::IssueId {
    issues::create(
        pool,
        author,
        Origin::Web,
        NewIssue {
            project,
            summary: summary.to_owned(),
            publish: true,
            watchers: watchers.to_vec(),
            ..NewIssue::default()
        },
    )
    .await
    .unwrap()
    .done
    .0
    .issue
    .id
}

/// The person's delivered rows, newest first.
async fn delivered(pool: &PgPool, user: UserId) -> Vec<inbox::Notification> {
    inbox::notifications_of(pool, user, None, 100)
        .await
        .unwrap()
}

/// Fires everything due within `ahead` of now (the sweep's clock is a parameter, NTF-044),
/// with email off.
async fn fire(pool: &PgPool, ahead: Duration) -> u64 {
    ntf::sweep(
        pool,
        OffsetDateTime::now_utc() + ahead,
        &prostor_service::mail::Outgoing::default(),
    )
    .await
    .unwrap()
}

/// The rendered content of a row.
fn content(row: &inbox::Notification) -> Content {
    serde_json::from_value(row.content.clone()).unwrap()
}

/// The kinds of the person's rows, in delivery order.
fn kinds(rows: &[inbox::Notification]) -> Vec<&'static str> {
    rows.iter().map(|row| row.kind.key()).collect()
}

/// Changes the summary of an issue (an `updated` event, NTF-045).
async fn retitle(pool: &PgPool, actor: &Access, issue: prostor_domain::IssueId, summary: &str) {
    let version = issues::get(pool, actor, issue)
        .await
        .unwrap()
        .0
        .issue
        .text_version;
    issues::update(
        pool,
        actor,
        Origin::Web,
        issue,
        IssueChange {
            summary: Some(summary.to_owned()),
            text_version: Some(version),
            ..IssueChange::default()
        },
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn ntf_045_kinds_of_update() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let web = w.web;
    let carol = member(pool, &w.admin, "carol", &[web]).await;
    let issue = watched_issue(pool, &w.bob, web, "Crash", &[w.anna.user()]).await;

    // One `created` on publication, due at once.
    let fired = fire(pool, Duration::from_secs(2)).await;
    assert!(fired >= 1);
    let rows = delivered(pool, w.anna.user()).await;
    assert_eq!(kinds(&rows), ["created"]);

    // A comment, a vote and a tag change are kinds of their own; every one is due at once.
    prostor_service::comments::create(
        pool,
        &carol,
        Origin::Web,
        issue,
        NewComment {
            text: "Look here".to_owned(),
            ..NewComment::default()
        },
    )
    .await
    .unwrap();
    issues::set_vote(pool, &carol, issue, true).await.unwrap();
    let tag = tags::create(pool, &carol, "Bug", None).await.unwrap();
    tags::set_on_issue(pool, &carol, Origin::Web, issue, tag.id, true)
        .await
        .unwrap();
    fire(pool, Duration::from_secs(2)).await;
    let mut so_far = kinds(&delivered(pool, w.anna.user()).await);
    so_far.sort_unstable();
    assert_eq!(so_far, ["comment", "created", "tag_added", "vote"]);

    // Field values, texts and audience changes are `updated` (NTF-045), held one minute.
    let view = issues::get(pool, &carol, issue).await.unwrap().0;
    let state = view
        .fields
        .iter()
        .find(|f| f.field.field.name == "State")
        .expect("the scrum template has a State field");
    let values = prostor_store::value_sets::values_of(
        pool,
        &[state.field.attached.set_id.expect("state has a set")],
    )
    .await
    .unwrap();
    // A plain state to move to, the resolving one kept apart ("Duplicate" needs a link).
    let plain = values
        .iter()
        .find(|v| v.name != "Open" && v.name != "Duplicate")
        .expect("a third state");
    let resolving = values
        .iter()
        .rev()
        .find(|v| v.name != "Duplicate")
        .unwrap()
        .clone();
    prostor_service::value_sets::update_value(
        pool,
        &w.admin,
        resolving.set_id,
        resolving.id,
        prostor_service::value_sets::ValueInput {
            resolved: Some(true),
            ..prostor_service::value_sets::ValueInput::default()
        },
        resolving.version,
    )
    .await
    .unwrap();
    issues::set_field(
        pool,
        &carol,
        Origin::Web,
        issue,
        state.field.id(),
        FieldChange::Set(Some(Input::Ids(vec![plain.id]))),
        false,
    )
    .await
    .unwrap();
    retitle(pool, &carol, issue, "Crash harder").await;
    fire(pool, Duration::from_secs(2)).await;
    let mut waiting = kinds(&delivered(pool, w.anna.user()).await);
    waiting.sort_unstable();
    assert_eq!(
        waiting,
        ["comment", "created", "tag_added", "vote"],
        "the updated batch waits out its window"
    );
    fire(pool, Duration::from_secs(70)).await;
    let rows = delivered(pool, w.anna.user()).await;
    let mut all = kinds(&rows);
    all.sort_unstable();
    assert_eq!(all, ["comment", "created", "tag_added", "updated", "vote"]);
    // The field line and the summary line joined one batch.
    let updated = rows.iter().find(|row| row.kind.key() == "updated").unwrap();
    let lines = content(updated).lines;
    assert_eq!(lines.len(), 2, "{lines:?}");

    // A state value flagged resolved resolves: `resolved`, not `updated` (§3.2).
    issues::set_field(
        pool,
        &carol,
        Origin::Web,
        issue,
        state.field.id(),
        FieldChange::Set(Some(Input::Ids(vec![resolving.id]))),
        false,
    )
    .await
    .unwrap();
    fire(pool, Duration::from_secs(2)).await;
    // The resolution fires at once, and with it the field's own `updated` batch flushes
    // (NTF-044) — the newest row is that flush, the resolution sits just under it.
    let rows = delivered(pool, w.anna.user()).await;
    assert_eq!(rows[0].kind, prostor_domain::notifications::Kind::Updated);
    assert_eq!(rows[1].kind, prostor_domain::notifications::Kind::Resolved);
}

#[tokio::test]
async fn ntf_042_reasons_union() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let web = w.web;
    // Anna watches the issue and subscribes to her own tag on it (NTF-021).
    let tag = tags::create(pool, &w.anna, "Release", None).await.unwrap();
    tags::update(
        pool,
        &w.anna,
        tag.id,
        TagChange {
            sharing: Some(TagSharing {
                use_: users_(&[w.bob.user()]),
                ..TagSharing::default()
            }),
            version: tag.version,
            ..TagChange::default()
        },
    )
    .await
    .unwrap();
    ntf::subscribe(
        pool,
        &w.anna,
        Source::Tag(tag.id),
        &[prostor_domain::notifications::Kind::Updated],
    )
    .await
    .unwrap();
    let issue = watched_issue(pool, &w.bob, web, "Crash", &[w.anna.user()]).await;
    tags::set_on_issue(pool, &w.bob, Origin::Web, issue, tag.id, true)
        .await
        .unwrap();
    fire(pool, Duration::from_secs(2)).await;
    retitle(pool, &w.bob, issue, "Crash harder").await;
    fire(pool, Duration::from_secs(70)).await;

    // One notification, both reasons on it (NTF-042).
    let rows = delivered(pool, w.anna.user()).await;
    let updated = rows.iter().find(|row| row.kind.key() == "updated").unwrap();
    let mut reasons = updated.reasons.clone();
    reasons.sort_unstable();
    assert_eq!(reasons, [format!("tag:{}", tag.id), "watch".to_owned()]);
}

#[tokio::test]
async fn ntf_042_actor_dropped() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let web = w.web;
    let issue = watched_issue(pool, &w.bob, web, "Crash", &[w.anna.user(), w.bob.user()]).await;

    // Own changes are off by default (NTF-030): the author hears nothing, the watcher does.
    fire(pool, Duration::from_secs(2)).await;
    retitle(pool, &w.bob, issue, "Crash harder").await;
    fire(pool, Duration::from_secs(70)).await;
    assert_eq!(
        kinds(&delivered(pool, w.anna.user()).await),
        ["updated", "created"]
    );
    assert!(delivered(pool, w.bob.user()).await.is_empty());

    // With the flag on, the author hears their own change too.
    ntf::set_settings(
        pool,
        w.bob.user(),
        Settings {
            own_changes: true,
            ..Settings::default()
        },
    )
    .await
    .unwrap();
    retitle(pool, &w.bob, issue, "Crash hardest").await;
    fire(pool, Duration::from_secs(70)).await;
    let rows = delivered(pool, w.bob.user()).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].reasons, ["watch"]);
}

#[tokio::test]
async fn ntf_042_search_subscriber() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let web = w.web;
    // Anna's stored search: open issues. She subscribes to updates of what it matches.
    let search = saved::create(pool, &w.anna, "Open ones", "State: Open")
        .await
        .unwrap();
    ntf::subscribe(
        pool,
        &w.anna,
        Source::Search(search.row.id),
        &[prostor_domain::notifications::Kind::Updated],
    )
    .await
    .unwrap();
    let open = watched_issue(pool, &w.bob, web, "Crash", &[w.admin.user()]).await;
    let other = watched_issue(pool, &w.bob, web, "Distractor", &[w.admin.user()]).await;
    // The distractor leaves the search: its state value resolves it (FLD-046).
    let view = issues::get(pool, &w.anna, other).await.unwrap().0;
    let state = view
        .fields
        .iter()
        .find(|f| f.field.field.name == "State")
        .expect("the scrum template has a State field");
    let values = prostor_store::value_sets::values_of(
        pool,
        &[state.field.attached.set_id.expect("state has a set")],
    )
    .await
    .unwrap();
    let resolving = values
        .iter()
        .rev()
        .find(|v| v.name != "Duplicate")
        .unwrap()
        .clone();
    prostor_service::value_sets::update_value(
        pool,
        &w.admin,
        resolving.set_id,
        resolving.id,
        prostor_service::value_sets::ValueInput {
            resolved: Some(true),
            ..prostor_service::value_sets::ValueInput::default()
        },
        resolving.version,
    )
    .await
    .unwrap();
    issues::set_field(
        pool,
        &w.anna,
        Origin::Web,
        other,
        state.field.id(),
        FieldChange::Set(Some(Input::Ids(vec![resolving.id]))),
        false,
    )
    .await
    .unwrap();
    fire(pool, Duration::from_secs(2)).await;
    assert!(
        delivered(pool, w.anna.user()).await.is_empty(),
        "the resolved change is not an update she subscribed to"
    );

    // The matching issue notifies; the other one does not (`holds_each` against the stored
    // search, plan §3.3).
    retitle(pool, &w.bob, open, "Crash harder").await;
    retitle(pool, &w.bob, other, "Distractor more").await;
    fire(pool, Duration::from_secs(70)).await;
    let rows = delivered(pool, w.anna.user()).await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].reasons, [format!("search:{}", search.row.id)]);
    assert_eq!(content(&rows[0]).issue.summary, "Crash harder");
}

#[tokio::test]
async fn ntf_035_mention_without_access_dropped() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let web = w.web;
    let issue = watched_issue(pool, &w.bob, web, "Crash", &[w.anna.user()]).await;
    // A person with no access to the project at all.
    let outsider = users::create(
        pool,
        &passwords(),
        &w.admin,
        NewAccount {
            username: "stranger".to_owned(),
            full_name: "Stranger Things".to_owned(),
            email: "stranger@example.com".to_owned(),
            password: "correct horse".to_owned(),
            must_change_password: false,
        },
    )
    .await
    .unwrap()
    .id;

    prostor_service::comments::create(
        pool,
        &w.bob,
        Origin::Web,
        issue,
        NewComment {
            text: "@anna and @stranger, look".to_owned(),
            ..NewComment::default()
        },
    )
    .await
    .unwrap();
    fire(pool, Duration::from_secs(2)).await;

    // Anna: the mention is one more reason on the comment's notification (NTF-042).
    let rows = delivered(pool, w.anna.user()).await;
    let comment = rows
        .iter()
        .find(|row| row.kind.key() == "comment")
        .expect("the comment row");
    let mut reasons = comment.reasons.clone();
    reasons.sort_unstable();
    assert_eq!(reasons, ["mention".to_owned(), "watch".to_owned()]);
    // The mention without read access does not notify at all (NTF-035, §1).
    assert!(delivered(pool, outsider).await.is_empty());
}

#[tokio::test]
async fn ntf_044_batch_window() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let web = w.web;
    let issue = watched_issue(pool, &w.bob, web, "Crash", &[w.anna.user()]).await;
    fire(pool, Duration::from_secs(2)).await;
    assert_eq!(kinds(&delivered(pool, w.anna.user()).await), ["created"]);

    retitle(pool, &w.bob, issue, "Crash harder").await;
    fire(pool, Duration::from_secs(5)).await;
    assert_eq!(
        kinds(&delivered(pool, w.anna.user()).await),
        ["created"],
        "the window has not passed"
    );

    // A second change by the same author merges and restarts the window (NTF-044).
    let version = issues::get(pool, &w.bob, issue)
        .await
        .unwrap()
        .0
        .issue
        .text_version;
    issues::update(
        pool,
        &w.bob,
        Origin::Web,
        issue,
        IssueChange {
            description: Some("It broke twice".to_owned()),
            text_version: Some(version),
            ..IssueChange::default()
        },
    )
    .await
    .unwrap();
    fire(pool, Duration::from_secs(70)).await;

    let rows = delivered(pool, w.anna.user()).await;
    assert_eq!(kinds(&rows), ["updated", "created"]);
    let lines = content(&rows[0]).lines;
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(matches!(&lines[0], Line::Field { name, .. } if name == "summary"));
    assert!(matches!(&lines[1], Line::Field { name, .. } if name == "description"));
}

#[tokio::test]
async fn ntf_044_other_author_flushed() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let web = w.web;
    let carol = member(pool, &w.admin, "carol", &[web]).await;
    let issue = watched_issue(pool, &w.bob, web, "Crash", &[carol.user()]).await;
    fire(pool, Duration::from_secs(2)).await;
    assert_eq!(kinds(&delivered(pool, carol.user()).await), ["created"]);

    retitle(pool, &w.bob, issue, "By Bob").await;
    // A second author's update starts its own window, later than Bob's.
    tokio::time::sleep(Duration::from_secs(1)).await;
    retitle(pool, &w.anna, issue, "By Anna").await;

    // Bob's batch is due; Anna's is not, but fires with it, each as its own notification
    // (NTF-044).
    fire(pool, Duration::from_millis(60_500)).await;
    let rows = delivered(pool, carol.user()).await;
    assert_eq!(kinds(&rows), ["updated", "updated", "created"], "{rows:?}");
    let updated: Vec<&inbox::Notification> = rows
        .iter()
        .filter(|row| row.kind.key() == "updated")
        .collect();
    let mut authors: Vec<_> = updated.iter().map(|row| row.author_id).collect();
    authors.sort_unstable();
    let mut expected = vec![w.anna.user(), w.bob.user()];
    expected.sort_unstable();
    assert_eq!(authors, expected);
}

#[tokio::test]
async fn ntf_043_private_fields_filtered() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let web = w.web;
    let carol = member(pool, &w.admin, "carol", &[web]).await;
    // Dana reads the project's issues through a custom role without private-field read (a
    // reporter's view); Anna is a full member.
    let dana = users::create(
        pool,
        &passwords(),
        &w.admin,
        NewAccount {
            username: "dana".to_owned(),
            full_name: "Dana Quiet".to_owned(),
            email: "dana@example.com".to_owned(),
            password: "correct horse".to_owned(),
            must_change_password: false,
        },
    )
    .await
    .unwrap()
    .id;
    let role = roles::create(
        pool,
        &w.admin,
        NewRole {
            name: "Quiet reader".to_owned(),
            description: String::new(),
            permissions: vec!["project.basic".to_owned(), "issue.read".to_owned()],
        },
    )
    .await
    .unwrap()
    .role
    .id;
    roles::assign(pool, &w.admin, role, Holder::User(dana), &[Some(web)])
        .await
        .unwrap();
    let dana = access::load(pool, dana).await.unwrap();
    let issue = watched_issue(pool, &w.bob, web, "Crash", &[dana.user(), w.anna.user()]).await;

    // The State field becomes private (FLD-060); the change is Carol's, who may write private
    // fields. Dana cannot read them; Anna can.
    let fields = prostor_service::project_fields::list(pool, &w.admin, web)
        .await
        .unwrap();
    let state = fields
        .iter()
        .find(|f| f.field.name == "State")
        .expect("the scrum template has a State field");
    prostor_service::project_fields::update(
        pool,
        &w.admin,
        web,
        state.field.id,
        prostor_service::project_fields::SettingsChange {
            private: Some(true),
            version: state.attached.version,
            ..prostor_service::project_fields::SettingsChange::default()
        },
    )
    .await
    .unwrap();
    let values = prostor_store::value_sets::values_of(
        pool,
        &[state.attached.set_id.expect("state has a set")],
    )
    .await
    .unwrap();
    let plain = values
        .iter()
        .find(|v| v.name != "Open" && v.name != "Duplicate")
        .expect("a third state");
    issues::set_field(
        pool,
        &carol,
        Origin::Web,
        issue,
        state.field.id,
        FieldChange::Set(Some(Input::Ids(vec![plain.id]))),
        false,
    )
    .await
    .unwrap();
    fire(pool, Duration::from_secs(70)).await;

    // The private line is filtered from Dana's notification and kept for Anna (FLD-062).
    fn of_kind<'a>(rows: &'a [inbox::Notification], kind: &str) -> &'a inbox::Notification {
        rows.iter()
            .find(|row| row.kind.key() == kind)
            .unwrap_or_else(|| panic!("no {kind} row"))
    }
    let dana_rows = delivered(pool, dana.user()).await;
    assert!(
        content(of_kind(&dana_rows, "updated")).lines.is_empty(),
        "{:?}",
        content(of_kind(&dana_rows, "updated")).lines
    );
    let anna_rows = delivered(pool, w.anna.user()).await;
    let lines = content(of_kind(&anna_rows, "updated")).lines;
    assert!(
        matches!(&lines[0], Line::Field { name, .. } if name == "State"),
        "{lines:?}"
    );
}

#[tokio::test]
async fn ntf_043_last_notification_once() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let web = w.web;
    let issue = watched_issue(pool, &w.bob, web, "Crash", &[w.anna.user()]).await;

    // The audience closes on Anna: one final row says watching stopped (NTF-043).
    issues::update(
        pool,
        &w.bob,
        Origin::Web,
        issue,
        IssueChange {
            audience: Some(AudienceInput {
                users: vec![w.bob.user()],
                groups: Vec::new(),
            }),
            ..IssueChange::default()
        },
    )
    .await
    .unwrap();
    fire(pool, Duration::from_secs(70)).await;
    let rows = delivered(pool, w.anna.user()).await;
    assert_eq!(rows.len(), 1);
    assert!(rows[0].last);
    assert_eq!(
        content(&rows[0]).lines,
        [Line::Text {
            text: "watching stopped".to_owned()
        }]
    );

    // While access stays away, later changes reach her not at all.
    retitle(pool, &w.bob, issue, "Still crashing").await;
    fire(pool, Duration::from_secs(70)).await;
    assert_eq!(delivered(pool, w.anna.user()).await.len(), 1);

    // Access back: an ordinary row again, and the final-notice flag clears.
    issues::update(
        pool,
        &w.bob,
        Origin::Web,
        issue,
        IssueChange {
            audience: Some(AudienceInput::default()),
            ..IssueChange::default()
        },
    )
    .await
    .unwrap();
    fire(pool, Duration::from_secs(70)).await;
    retitle(pool, &w.bob, issue, "Crash harder").await;
    fire(pool, Duration::from_secs(70)).await;
    let rows = delivered(pool, w.anna.user()).await;
    assert_eq!(rows.len(), 3);
    assert!(!rows.iter().any(|row| row.last), "the flag cleared");

    // Losing access again writes a fresh final row.
    issues::update(
        pool,
        &w.bob,
        Origin::Web,
        issue,
        IssueChange {
            audience: Some(AudienceInput {
                users: vec![w.bob.user()],
                groups: Vec::new(),
            }),
            ..IssueChange::default()
        },
    )
    .await
    .unwrap();
    fire(pool, Duration::from_secs(70)).await;
    let rows = delivered(pool, w.anna.user()).await;
    assert_eq!(rows.len(), 4);
    assert!(rows[0].last);
}

#[tokio::test]
async fn ntf_041_drafts_silent() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let web = w.web;
    let draft = issues::create(
        pool,
        &w.bob,
        Origin::Web,
        NewIssue {
            project: web,
            summary: "Half-baked".to_owned(),
            publish: false,
            watchers: vec![w.anna.user()],
            ..NewIssue::default()
        },
    )
    .await
    .unwrap()
    .done
    .0
    .issue
    .id;
    retitle(pool, &w.bob, draft, "Half-baked still").await;

    // Nothing pends and nothing arrives, however far the clock runs (NTF-041).
    fire(pool, Duration::from_secs(600)).await;
    assert!(delivered(pool, w.anna.user()).await.is_empty());
    let mut conn = pool.acquire().await.unwrap();
    let due = inbox::due_issues(
        &mut conn,
        OffsetDateTime::now_utc() + Duration::from_secs(600),
    )
    .await
    .unwrap();
    let locked = inbox::lock_issues(&mut conn, &due).await.unwrap();
    let pending = inbox::due_batches(
        &mut conn,
        OffsetDateTime::now_utc() + Duration::from_secs(600),
        &locked,
        100,
    )
    .await
    .unwrap();
    drop(conn);
    assert!(pending.is_empty(), "{pending:?}");
}

#[tokio::test]
async fn ntf_027_created_fires() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let web = w.web;

    // The creator's subscription on the project's "unassigned" search (PRJ-019) hears a new
    // unassigned issue from someone else (NTF-027).
    let issue = watched_issue(pool, &w.anna, web, "Crash", &[]).await;
    fire(pool, Duration::from_secs(2)).await;
    let rows = delivered(pool, w.admin.user()).await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].kind, prostor_domain::notifications::Kind::Created);
    assert!(rows[0].reasons[0].starts_with("search:"));

    // The admin's own new issue is their own change: no notification (NTF-030).
    watched_issue(pool, &w.admin, web, "Own", &[]).await;
    fire(pool, Duration::from_secs(2)).await;
    assert_eq!(delivered(pool, w.admin.user()).await.len(), 1);

    // An assigned issue does not match the "unassigned" search — but the assignee watches it
    // from the assignment now (ISS-030 auto-watch), so it still arrives, by watch.
    let view = issues::get(pool, &w.anna, issue).await.unwrap().0;
    let assignee = view
        .fields
        .iter()
        .find(|f| f.field.field.name == "Assignee")
        .expect("the scrum template has an Assignee field");
    issues::create(
        pool,
        &w.anna,
        Origin::Web,
        NewIssue {
            project: web,
            summary: "Taken".to_owned(),
            publish: true,
            fields: vec![prostor_service::issues::FieldInput {
                field: assignee.field.id(),
                value: Some(Input::Ids(vec![w.admin.user()])),
            }],
            ..NewIssue::default()
        },
    )
    .await
    .unwrap();
    fire(pool, Duration::from_secs(2)).await;
    let rows = delivered(pool, w.admin.user()).await;
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_eq!(rows[0].kind, prostor_domain::notifications::Kind::Created);
    assert!(rows[0].reasons.iter().any(|reason| reason == "watch"));
}

/// A probe job for the report test: succeeds or fails on demand.
#[derive(serde::Serialize, serde::Deserialize)]
struct Probe {
    ok: bool,
}

impl JobKind for Probe {
    const KIND: &'static str = "ntf.probe";
    const CANCELLABLE: bool = false;
    const MAX_ATTEMPTS: i32 = 3;
}

#[tokio::test]
async fn adm_031_job_report() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;

    let mut handlers = Handlers::new();
    handlers.add::<Probe, _, _>(|_ctx, job| async move {
        if job.ok {
            Ok(())
        } else {
            Err(JobError::Fail("it broke".to_owned()))
        }
    });
    let config = RunnerConfig {
        workers: 1,
        poll_interval: Duration::from_millis(50),
        lease: Duration::from_secs(5),
        ..RunnerConfig::default()
    };
    let runner = Runner::start(pool.clone(), handlers, config, None);

    let mut conn = pool.acquire().await.unwrap();
    for ok in [true, false] {
        jobs_queue::enqueue(
            &mut conn,
            &Probe { ok },
            Enqueue {
                initiator: Actor::User(admin.user()),
                ..Enqueue::default()
            },
        )
        .await
        .unwrap();
    }
    drop(conn);

    // The runner takes the jobs; each end writes its report (ADM-031).
    for _ in 0..100 {
        if delivered(pool, admin.user()).await.len() == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    runner.shutdown().await;

    let rows = delivered(pool, admin.user()).await;
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert!(rows.iter().all(|row| row.kind.key() == "system"));
    assert!(rows.iter().all(|row| row.issue_id.is_none()));
    let states: Vec<(&str, Option<&str>)> = rows
        .iter()
        .map(|row| {
            let job = row.content.get("job").and_then(|v| v.as_str()).unwrap();
            let state = row.content.get("state").and_then(|v| v.as_str()).unwrap();
            let message = row
                .content
                .get("message")
                .and_then(|v| v.as_str())
                .filter(|m| !m.is_empty());
            assert_eq!(job, "ntf.probe");
            (state, message)
        })
        .collect();
    assert!(states.contains(&("finished", None)));
    assert!(states.contains(&("failed", Some("it broke"))));
}

// ---------------------------------------------------------------------------
// The emails of fired notifications (plan §6.4, 1.10c-2)
//

use prostor_domain::settings::BASE_URL;
use prostor_service::mail::{self, EnvMail, Mail, Outgoing};
use prostor_service::settings as instance_settings;

/// The outgoing email state of an instance whose transport comes from the environment (D-026):
/// on, and the sender address carries no display name, so NTF-008's case is the default here.
fn email_on() -> Outgoing {
    mail::outgoing(
        &prostor_domain::settings::Snapshot::default(),
        Some(&EnvMail {
            url: "smtp://127.0.0.1:1025".to_owned(),
            from: "tracker@example.com".to_owned(),
        }),
    )
}

/// Fires with email on and links in place (NTF-010): the instance's base URL points somewhere.
async fn fire_email(pool: &PgPool, admin: &prostor_domain::access::Access, ahead: Duration) {
    instance_settings::set(
        pool,
        BASE_URL.key,
        Some(&serde_json::json!("https://tracker.example.com")),
        Actor::User(admin.user()),
        None,
    )
    .await
    .unwrap();
    ntf::sweep(pool, OffsetDateTime::now_utc() + ahead, &email_on())
        .await
        .unwrap();
}

/// The queued emails of one person, oldest first. Other people share the queue (the project's
/// creator hears about every new issue through their «Unassigned» search, PRJ-019), so the
/// tests pick their own recipient's letters.
fn of<'a>(emails: &'a [Mail], address: &str) -> Vec<&'a Mail> {
    emails.iter().filter(|m| m.to == address).collect()
}

/// The queued notification emails, oldest first.
async fn queued_mail(pool: &PgPool) -> Vec<Mail> {
    let payloads: Vec<serde_json::Value> =
        sqlx::query_scalar("select payload from jobs where kind = 'mail.send' order by id")
            .fetch_all(pool)
            .await
            .unwrap();
    payloads
        .into_iter()
        .map(|payload| serde_json::from_value(payload).unwrap())
        .collect()
}

#[tokio::test]
async fn ntf_059_new_item_email() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let carol = member(pool, &w.admin, "carol", &[w.web]).await;
    // Anna wants both bodies; Carol asked for plain text only (NTF-066).
    ntf::set_settings(
        pool,
        carol.user(),
        Settings {
            email_html: false,
            ..Settings::default()
        },
    )
    .await
    .unwrap();
    issues::create(
        pool,
        &w.bob,
        Origin::Web,
        NewIssue {
            project: w.web,
            summary: "Crash on save".to_owned(),
            description: "It **crashes** when saving".to_owned(),
            publish: true,
            watchers: vec![w.anna.user(), carol.user()],
            ..NewIssue::default()
        },
    )
    .await
    .unwrap();
    fire_email(pool, &w.admin, Duration::from_secs(2)).await;

    // One email per recipient (§5.2), queued by the system's hand; the two watchers each get
    // theirs (the creator's own letter rides their «Unassigned» subscription, PRJ-019).
    let emails = queued_mail(pool).await;
    let mut to: Vec<&str> = emails.iter().map(|m| m.to.as_str()).collect();
    to.sort_unstable();
    assert_eq!(
        to,
        ["anna@example.com", "carol@example.com", "chief@example.com"],
        "{emails:?}"
    );
    let initiator: Vec<Option<i64>> =
        sqlx::query_scalar("select initiator_id from jobs where kind = 'mail.send' order by id")
            .fetch_all(pool)
            .await
            .unwrap();
    assert!(initiator.iter().all(|id| id.is_none()), "the system queues");

    // The new item carries everything visible: the reference, the description, the reporter
    // (§5.2) and the footer's link (NTF-063, NTF-010).
    let watchers = of(&emails, "anna@example.com");
    let anna = watchers[0];
    assert!(
        anna.subject.starts_with("created: WEB-1 "),
        "{}",
        anna.subject
    );
    assert!(anna.text.contains("WEB-1 — Crash on save"));
    assert!(anna.text.contains("Reported by: bob Person"));
    assert!(anna.text.contains("It **crashes** when saving"));
    assert!(anna.html.contains("crashes"));
    assert!(
        anna.text
            .contains("https://tracker.example.com/issues/WEB-1")
    );
    assert!(
        anna.text
            .contains("Notification settings: https://tracker.example.com/account")
    );
    // Plain text only when the person asked for it (NTF-066).
    let plain = of(&emails, "carol@example.com");
    assert_eq!(plain[0].text, anna.text, "the same content per recipient");
    assert!(plain[0].html.is_empty());
}

#[tokio::test]
async fn ntf_060_change_summary_diff() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let issue = watched_issue(pool, &w.bob, w.web, "Crash on save", &[w.anna.user()]).await;
    fire_email(pool, &w.admin, Duration::from_secs(2)).await;
    retitle(pool, &w.bob, issue, "Crash on submit").await;
    fire_email(pool, &w.admin, Duration::from_secs(70)).await;

    let queued = queued_mail(pool).await;
    let change = of(&queued, "anna@example.com")[1];
    assert!(
        change.subject.starts_with("changed: "),
        "{}",
        change.subject
    );
    // The plain text keeps «old → new»; the HTML highlights the changed words (NTF-060).
    assert!(
        change
            .text
            .contains("summary: Crash on save → Crash on submit")
    );
    assert!(change.html.contains("<del"), "{}", change.html);
    assert!(change.html.contains("<ins"), "{}", change.html);
    assert!(
        change
            .html
            .contains("<del style=\"color:#b42318\">save</del>"),
        "{}",
        change.html
    );
    assert!(
        change
            .html
            .contains("<ins style=\"color:#067647;text-decoration-thickness:2px\">submit</ins>"),
        "{}",
        change.html
    );
}

#[tokio::test]
async fn ntf_062_subject_kinds() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let carol = member(pool, &w.admin, "carol", &[w.web]).await;
    let issue = watched_issue(pool, &w.bob, w.web, "Crash", &[w.anna.user()]).await;
    fire_email(pool, &w.admin, Duration::from_secs(2)).await;

    prostor_service::comments::create(
        pool,
        &carol,
        Origin::Web,
        issue,
        NewComment {
            text: "Look here".to_owned(),
            ..NewComment::default()
        },
    )
    .await
    .unwrap();
    fire_email(pool, &w.admin, Duration::from_secs(2)).await;
    issues::set_vote(pool, &carol, issue, true).await.unwrap();
    let tag = tags::create(pool, &carol, "Bug", None).await.unwrap();
    tags::set_on_issue(pool, &carol, Origin::Web, issue, tag.id, true)
        .await
        .unwrap();
    fire_email(pool, &w.admin, Duration::from_secs(2)).await;

    // A resolving state makes the issue resolved (§3.2); its subject says so.
    let view = issues::get(pool, &carol, issue).await.unwrap().0;
    let state = view
        .fields
        .iter()
        .find(|f| f.field.field.name == "State")
        .expect("the scrum template has a State field");
    let values = prostor_store::value_sets::values_of(
        pool,
        &[state.field.attached.set_id.expect("state has a set")],
    )
    .await
    .unwrap();
    let resolving = values
        .iter()
        .rev()
        .find(|v| v.name != "Duplicate")
        .unwrap()
        .clone();
    prostor_service::value_sets::update_value(
        pool,
        &w.admin,
        resolving.set_id,
        resolving.id,
        prostor_service::value_sets::ValueInput {
            resolved: Some(true),
            ..prostor_service::value_sets::ValueInput::default()
        },
        resolving.version,
    )
    .await
    .unwrap();
    issues::set_field(
        pool,
        &carol,
        Origin::Web,
        issue,
        state.field.id(),
        FieldChange::Set(Some(Input::Ids(vec![resolving.id]))),
        false,
    )
    .await
    .unwrap();
    fire_email(pool, &w.admin, Duration::from_secs(2)).await;
    retitle(pool, &carol, issue, "Crash harder").await;
    fire_email(pool, &w.admin, Duration::from_secs(70)).await;

    // One stable word per kind (NTF-062). The extra `changed` is the State edit's own batch,
    // flushed together with the resolution (NTF-044).
    let queued = queued_mail(pool).await;
    let subjects: Vec<&str> = of(&queued, "anna@example.com")
        .iter()
        .map(|m| m.subject.as_str())
        .collect();
    assert_eq!(
        subjects,
        [
            "created: WEB-1 Crash",
            "comment: WEB-1 Crash",
            "vote: WEB-1 Crash",
            "tag: WEB-1 Crash",
            "resolved: WEB-1 Crash",
            "changed: WEB-1 Crash",
            "changed: WEB-1 Crash harder",
        ],
        "{subjects:?}"
    );
}

#[tokio::test]
async fn ntf_063_footer_reasons_no_mute() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    // Anna watches, subscribes to her own tag (shared for use) and gets mentioned: three reasons
    // on one notification (NTF-042), all in the footer's words (NTF-063).
    let tag = tags::create(pool, &w.anna, "Release", None).await.unwrap();
    tags::update(
        pool,
        &w.anna,
        tag.id,
        TagChange {
            sharing: Some(TagSharing {
                use_: users_(&[w.bob.user()]),
                ..TagSharing::default()
            }),
            version: tag.version,
            ..TagChange::default()
        },
    )
    .await
    .unwrap();
    ntf::subscribe(pool, &w.anna, Source::Tag(tag.id), &[Kind::Comment])
        .await
        .unwrap();
    let issue = watched_issue(pool, &w.bob, w.web, "Crash", &[w.anna.user()]).await;
    tags::set_on_issue(pool, &w.bob, Origin::Web, issue, tag.id, true)
        .await
        .unwrap();
    fire_email(pool, &w.admin, Duration::from_secs(2)).await;
    prostor_service::comments::create(
        pool,
        &w.bob,
        Origin::Web,
        issue,
        NewComment {
            text: "@anna look".to_owned(),
            ..NewComment::default()
        },
    )
    .await
    .unwrap();
    fire_email(pool, &w.admin, Duration::from_secs(2)).await;

    let emails = queued_mail(pool).await;
    let comment = &emails[emails.len() - 1];
    assert!(comment.text.contains("You received this email because:"));
    assert!(comment.text.contains("you watch this issue"));
    assert!(comment.text.contains("you subscribe to the tag Release"));
    assert!(comment.text.contains("you were mentioned"));
    // No mute link in 1.10 (§1): the settings link is the only way out.
    assert!(!comment.text.to_lowercase().contains("mute"));
    assert!(!comment.html.to_lowercase().contains("mute"));
    assert!(
        comment
            .html
            .contains("href=\"https://tracker.example.com/account\"")
    );
}

#[tokio::test]
async fn ntf_064_thread_headers() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let issue = watched_issue(pool, &w.bob, w.web, "Crash", &[w.anna.user()]).await;
    fire_email(pool, &w.admin, Duration::from_secs(2)).await;
    retitle(pool, &w.bob, issue, "Crash harder").await;
    fire_email(pool, &w.admin, Duration::from_secs(70)).await;

    // The first email of a thread has only its own ID; later ones point back at the first
    // (NTF-064), so all emails about one issue thread in clients.
    let queued = queued_mail(pool).await;
    let emails = of(&queued, "anna@example.com");
    assert_eq!(emails.len(), 2);
    let mut rows = delivered(pool, w.anna.user()).await;
    rows.reverse();
    let [first_row, second_row] = &rows[..] else {
        panic!("two rows: {rows:?}")
    };
    let host = "@tracker.example.com>";
    assert_eq!(
        emails[0].threading.as_ref().unwrap().message_id,
        format!("<notification-{}{host}", first_row.id)
    );
    assert_eq!(emails[0].threading.as_ref().unwrap().first, None);
    assert_eq!(
        emails[1].threading.as_ref().unwrap().message_id,
        format!("<notification-{}{host}", second_row.id)
    );
    assert_eq!(
        emails[1].threading.as_ref().unwrap().first.as_deref(),
        Some(format!("<notification-{}{host}", first_row.id).as_str())
    );

    // On the wire the headers ride the message.
    let outgoing = email_on();
    let mailer = outgoing.transport.as_ref().unwrap();
    let wire =
        String::from_utf8_lossy(&mailer.message(emails[1]).unwrap().formatted()).into_owned();
    assert!(wire.contains("Message-ID: <notification-"), "{wire}");
    assert!(
        wire.contains(&format!(
            "In-Reply-To: <notification-{}{host}",
            first_row.id
        )),
        "{wire}"
    );
    assert!(
        wire.contains(&format!("References: <notification-{}{host}", first_row.id)),
        "{wire}"
    );
}

#[tokio::test]
async fn ntf_008_actor_display_name() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    watched_issue(pool, &w.bob, w.web, "Crash", &[w.anna.user()]).await;
    fire_email(pool, &w.admin, Duration::from_secs(2)).await;

    let queued = queued_mail(pool).await;
    let mail = of(&queued, "anna@example.com")[0];
    assert_eq!(mail.sender_name.as_deref(), Some("bob Person"));

    // A nameless service address shows the actor's name (NTF-008); a named one keeps its own.
    let wire = |outgoing: &Outgoing| {
        String::from_utf8_lossy(
            &outgoing
                .transport
                .as_ref()
                .unwrap()
                .message(mail)
                .unwrap()
                .formatted(),
        )
        .into_owned()
    };
    let unnamed = wire(&email_on());
    assert!(
        unnamed.contains("From: \"bob Person\" <tracker@example.com>"),
        "{unnamed}"
    );
    let named = mail::outgoing(
        &prostor_domain::settings::Snapshot::default(),
        Some(&EnvMail {
            url: "smtp://127.0.0.1:1025".to_owned(),
            from: "prostor <tracker@example.com>".to_owned(),
        }),
    );
    let named = wire(&named);
    assert!(
        named.contains("From: prostor <tracker@example.com>"),
        "{named}"
    );
}
