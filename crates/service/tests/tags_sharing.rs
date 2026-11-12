//! Shared tags (ISS-036, ISS-037, ACC-066, HIST-007; plan step 1.5 D1, D8) on a real database.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use time::OffsetDateTime;
use time::macros::datetime;
use trackflow_domain::access::Access;
use trackflow_domain::access::capability::Capability;
use trackflow_domain::access::role::PredefinedRole;
use trackflow_domain::issues::history::Origin;
use trackflow_domain::projects::Template;
use trackflow_domain::settings::Language;
use trackflow_domain::sharing::Audience;
use trackflow_domain::{GroupId, IssueId, ProjectId, TagId, UserId};
use trackflow_query::ProblemCode;
use trackflow_service::groups::NewGroup;
use trackflow_service::issues::{self, NewIssue};
use trackflow_service::projects::{self, NewProjectInput};
use trackflow_service::search::{self, SearchInput};
use trackflow_service::setup::{self, Setup};
use trackflow_service::tags::{self, Sharing, TagChange};
use trackflow_service::users::{self, NewAccount};
use trackflow_service::{Error, Passwords, access, capabilities, groups, roles};
use trackflow_store::PgPool;
use trackflow_store::access::Holder;
use trackflow_store::issue_history::Direction;
use trackflow_store::sessions::Origin as SessionOrigin;
use trackflow_store::tags::{Listing, Tag};
use trackflow_testkit::TestDb;

const T0: OffsetDateTime = datetime!(2026-10-01 09:00 UTC);
const WEB: Origin = Origin::Web;

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
            email: "anna@example.com".to_owned(),
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

async fn project(pool: &PgPool, actor: &Access) -> ProjectId {
    projects::create(
        pool,
        actor,
        NewProjectInput {
            template: Template::Blank,
            name: "Web".to_owned(),
            key: Some("WEB".to_owned()),
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

/// A project member.
async fn member(pool: &PgPool, admin: &Access, username: &str, web: ProjectId) -> Access {
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
    let role = trackflow_store::roles::builtin_id(pool, PredefinedRole::ProjectMember.key())
        .await
        .unwrap()
        .unwrap();
    roles::assign(pool, admin, role, Holder::User(user), &[Some(web)])
        .await
        .unwrap();
    access::load(pool, user).await.unwrap()
}

async fn group(pool: &PgPool, admin: &Access, name: &str, members: &[UserId]) -> GroupId {
    let id = groups::create(
        pool,
        admin,
        NewGroup {
            name: name.to_owned(),
            description: String::new(),
            auto_join: false,
        },
    )
    .await
    .unwrap()
    .id;
    if !members.is_empty() {
        groups::add_members(pool, admin, id, members).await.unwrap();
    }
    id
}

async fn reload(pool: &PgPool, who: &Access) -> Access {
    access::load(pool, who.user()).await.unwrap()
}

async fn issue(pool: &PgPool, actor: &Access, project: ProjectId) -> IssueId {
    issues::create(
        pool,
        actor,
        WEB,
        NewIssue {
            project,
            summary: "Tagged".to_owned(),
            publish: true,
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

fn users_(ids: &[UserId]) -> Audience {
    Audience {
        users: ids.to_vec(),
        groups: Vec::new(),
    }
}

async fn share(pool: &PgPool, who: &Access, tag: &Tag, sharing: Sharing) -> Result<Tag, Error> {
    tags::update(
        pool,
        who,
        tag.id,
        TagChange {
            sharing: Some(sharing),
            version: tag.version,
            ..TagChange::default()
        },
    )
    .await
}

async fn rename(pool: &PgPool, who: &Access, tag: &Tag, name: &str) -> Result<Tag, Error> {
    tags::update(
        pool,
        who,
        tag.id,
        TagChange {
            name: Some(name.to_owned()),
            version: tag.version,
            ..TagChange::default()
        },
    )
    .await
}

async fn tag_names(pool: &PgPool, who: &Access, id: IssueId) -> Vec<String> {
    issues::get(pool, who, id)
        .await
        .unwrap()
        .0
        .tags
        .iter()
        .map(|t| t.name.clone())
        .collect()
}

async fn find(pool: &PgPool, who: &Access, text: &str) -> Result<Vec<IssueId>, Error> {
    let page = search::search(
        pool,
        who,
        SearchInput {
            text: text.to_owned(),
            limit: 50,
            ..SearchInput::default()
        },
    )
    .await?;
    Ok(page.issues.issues.iter().map(|v| v.issue.id).collect())
}

async fn listed(pool: &PgPool, who: &Access, listing: Listing) -> Vec<TagId> {
    tags::list(pool, who, listing, "", None, 50)
        .await
        .unwrap()
        .iter()
        .map(|t| t.id)
        .collect()
}

/// Tag records an issue's history shows the viewer.
async fn tag_records(pool: &PgPool, who: &Access, id: IssueId) -> usize {
    let page = issues::activity::page(
        pool,
        who,
        id,
        issues::activity::Query {
            comments: false,
            changes: true,
            from: None,
            direction: Direction::Older,
            limit: 50,
            around_comment: None,
        },
    )
    .await
    .unwrap();
    page.items
        .iter()
        .filter(|i| i.entry().kind == "tags")
        .count()
}

struct World {
    admin: Access,
    web: ProjectId,
    olga: Access,
    vera: Access,
    ulf: Access,
    eda: Access,
    sam: Access,
    editors: GroupId,
}

async fn world(pool: &PgPool) -> World {
    let admin = admin(pool).await;
    let web = project(pool, &admin).await;
    let olga = member(pool, &admin, "olga", web).await;
    let vera = member(pool, &admin, "vera", web).await;
    let ulf = member(pool, &admin, "ulf", web).await;
    let eda = member(pool, &admin, "eda", web).await;
    let sam = member(pool, &admin, "sam", web).await;
    let editors = group(pool, &admin, "Editors", &[eda.user()]).await;
    let eda = reload(pool, &eda).await;
    World {
        admin,
        web,
        olga,
        vera,
        ulf,
        eda,
        sam,
        editors,
    }
}

fn sharing(w: &World) -> Sharing {
    Sharing {
        view: users_(&[w.vera.user()]),
        use_: users_(&[w.ulf.user()]),
        edit: Audience {
            users: Vec::new(),
            groups: vec![w.editors],
        },
    }
}

#[tokio::test]
async fn iss_037_view_use_edit_are_independent() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let x = issue(pool, &w.admin, w.web).await;
    let tag = tags::create(pool, &w.olga, "release", None).await.unwrap();
    assert!(matches!(
        tags::set_on_issue(pool, &w.ulf, WEB, x, tag.id, true).await,
        Err(Error::NotFound(_))
    ));
    let tag = share(pool, &w.olga, &tag, sharing(&w)).await.unwrap();
    assert_eq!(tag.view_users, vec![w.vera.user()]);

    // Use without view: Ulf puts it on, then does not see it.
    tags::set_on_issue(pool, &w.ulf, WEB, x, tag.id, true)
        .await
        .unwrap();
    assert!(tag_names(pool, &w.ulf, x).await.is_empty());
    assert_eq!(tag_names(pool, &w.vera, x).await, ["release"]);
    assert_eq!(tag_names(pool, &w.olga, x).await, ["release"]);
    assert!(tag_names(pool, &w.eda, x).await.is_empty());
    // Vera sees it but may not use it.
    assert!(matches!(
        tags::set_on_issue(pool, &w.vera, WEB, x, tag.id, false).await,
        Err(Error::NotFound(_))
    ));

    // In queries only where visible.
    assert_eq!(find(pool, &w.vera, "tag: release").await.unwrap(), [x]);
    assert!(find(pool, &w.vera, "tag: none").await.unwrap().is_empty());
    assert_eq!(find(pool, &w.ulf, "tag: none").await.unwrap(), [x]);
    match find(pool, &w.ulf, "tag: release").await {
        Err(Error::InvalidQuery(p)) => assert_eq!(p[0].code, ProblemCode::UnknownValue),
        other => panic!("{other:?}"),
    }
    let in_tag = |who: Access| async move {
        search::search(
            pool,
            &who,
            SearchInput {
                tag: Some(tag.id),
                limit: 50,
                ..SearchInput::default()
            },
        )
        .await
    };
    assert_eq!(in_tag(w.vera.clone()).await.unwrap().issues.issues.len(), 1);
    assert!(matches!(
        in_tag(w.ulf.clone()).await,
        Err(Error::NotFound(_))
    ));

    // Listings: shared ones are those with any audience the viewer is in.
    assert_eq!(listed(pool, &w.vera, Listing::Shared).await, [tag.id]);
    assert_eq!(listed(pool, &w.ulf, Listing::Shared).await, [tag.id]);
    assert_eq!(listed(pool, &w.eda, Listing::All).await, [tag.id]);
    assert!(listed(pool, &w.vera, Listing::Mine).await.is_empty());
    assert!(listed(pool, &w.sam, Listing::All).await.is_empty());
    assert_eq!(listed(pool, &w.olga, Listing::Mine).await, [tag.id]);
    assert!(listed(pool, &w.olga, Listing::Shared).await.is_empty());
}

#[tokio::test]
async fn iss_036_non_owners_edit_content_only() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let tag = tags::create(pool, &w.olga, "release", None).await.unwrap();
    let tag = share(pool, &w.olga, &tag, sharing(&w)).await.unwrap();

    let tag = rename(pool, &w.eda, &tag, "release 1").await.unwrap();
    assert_eq!(tag.name, "release 1");
    let tag = tags::update(
        pool,
        &w.eda,
        tag.id,
        TagChange {
            color: Some(Some(4)),
            version: tag.version,
            ..TagChange::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(tag.color, Some(4));
    assert!(matches!(
        share(pool, &w.eda, &tag, Sharing::default()).await,
        Err(Error::Refused(_))
    ));
    assert!(matches!(
        tags::delete(pool, &w.eda, tag.id).await,
        Err(Error::Refused(_))
    ));
    assert!(matches!(
        tags::usage(pool, &w.eda, tag.id).await,
        Err(Error::Refused(_))
    ));
    // Viewers and users do not edit; strangers do not know the tag.
    assert!(matches!(
        rename(pool, &w.vera, &tag, "mine now").await,
        Err(Error::Refused(_))
    ));
    assert!(matches!(
        rename(pool, &w.ulf, &tag, "mine now").await,
        Err(Error::Refused(_))
    ));
    assert!(matches!(
        rename(pool, &w.sam, &tag, "mine now").await,
        Err(Error::NotFound(_))
    ));
    assert!(matches!(
        rename(
            pool,
            &w.eda,
            &Tag {
                version: tag.version - 1,
                ..tag.clone()
            },
            "stale"
        )
        .await,
        Err(Error::Conflict(_))
    ));
    // The owner may delete.
    tags::delete(pool, &w.olga, tag.id).await.unwrap();
}

#[tokio::test]
async fn acc_066_sharing_needs_share_views() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let tag = tags::create(pool, &w.olga, "release", None).await.unwrap();
    // Unknown accounts and the owner in a list.
    assert!(matches!(
        share(
            pool,
            &w.olga,
            &tag,
            Sharing {
                view: users_(&[999_999]),
                ..Sharing::default()
            }
        )
        .await,
        Err(Error::Invalid(_))
    ));
    let tag = share(
        pool,
        &w.olga,
        &tag,
        Sharing {
            view: users_(&[w.olga.user(), w.vera.user(), w.vera.user()]),
            ..Sharing::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(tag.view_users, vec![w.vera.user()]);
    // A member cannot give the tag away.
    assert!(matches!(
        tags::update(
            pool,
            &w.olga,
            tag.id,
            TagChange {
                owner: Some(w.vera.user()),
                version: tag.version,
                ..TagChange::default()
            },
        )
        .await,
        Err(Error::Refused(_))
    ));

    // Share-views only for the group Sharers, which has the administrator.
    let sharers = group(pool, &w.admin, "Sharers", &[w.admin.user()]).await;
    capabilities::set(pool, &w.admin, Capability::ShareViews, &[sharers])
        .await
        .unwrap();
    let olga = reload(pool, &w.olga).await;
    let admin = reload(pool, &w.admin).await;
    assert!(!olga.capability(Capability::ShareViews));
    assert!(matches!(
        share(pool, &olga, &tag, Sharing::default()).await,
        Err(Error::Refused(_))
    ));
    // Editing the name needs no capability.
    let tag = rename(pool, &olga, &tag, "release 2").await.unwrap();
    // A user administrator with the capability gives it to Vera; she leaves the view list.
    let tag = tags::update(
        pool,
        &admin,
        tag.id,
        TagChange {
            owner: Some(w.vera.user()),
            version: tag.version,
            ..TagChange::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(tag.owner_id, w.vera.user());
    assert!(tag.view_users.is_empty());
    assert_eq!(listed(pool, &w.vera, Listing::Mine).await, [tag.id]);
}

#[tokio::test]
async fn hist_007_tag_records_follow_who_sees_the_tag() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let x = issue(pool, &w.admin, w.web).await;
    let tag = tags::create(pool, &w.olga, "release", None).await.unwrap();
    let tag = share(pool, &w.olga, &tag, sharing(&w)).await.unwrap();
    tags::set_on_issue(pool, &w.ulf, WEB, x, tag.id, true)
        .await
        .unwrap();
    assert_eq!(tag_records(pool, &w.olga, x).await, 1);
    assert_eq!(tag_records(pool, &w.vera, x).await, 1);
    assert_eq!(tag_records(pool, &w.ulf, x).await, 0);
    assert_eq!(tag_records(pool, &w.sam, x).await, 0);
    // Viewing given through a group counts too.
    let tag = share(
        pool,
        &w.olga,
        &tag,
        Sharing {
            view: Audience {
                users: Vec::new(),
                groups: vec![w.editors],
            },
            ..Sharing::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(tag.view_groups, vec![w.editors]);
    assert_eq!(tag_records(pool, &w.eda, x).await, 1);
    assert_eq!(tag_records(pool, &w.vera, x).await, 0);
}

#[tokio::test]
async fn iss_037_deleted_accounts_and_groups() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let shared = tags::create(pool, &w.olga, "release", None).await.unwrap();
    let shared = share(
        pool,
        &w.olga,
        &shared,
        Sharing {
            view: Audience {
                users: vec![w.vera.user(), w.admin.user()],
                groups: vec![w.editors],
            },
            ..Sharing::default()
        },
    )
    .await
    .unwrap();
    let private = tags::create(pool, &w.olga, "private", None).await.unwrap();
    tags::create(pool, &w.admin, "release", None).await.unwrap();
    let veras = tags::create(pool, &w.vera, "vera's", None).await.unwrap();
    let veras = share(
        pool,
        &w.vera,
        &veras,
        Sharing {
            use_: users_(&[w.sam.user()]),
            ..Sharing::default()
        },
    )
    .await
    .unwrap();

    // Olga leaves: her shared tag passes to the administrator, renamed; the unshared one goes.
    users::delete(pool, &w.admin, &[w.olga.user()], w.admin.user())
        .await
        .unwrap();
    let get = |id| trackflow_store::tags::get(pool, id);
    assert!(get(private.id).await.unwrap().is_none());
    let moved = get(shared.id).await.unwrap().unwrap();
    assert_eq!(moved.owner_id, w.admin.user());
    assert_eq!(moved.name, format!("release #{}", shared.id));
    assert_eq!(moved.view_users, vec![w.vera.user()]);

    // Sam leaves: audience entries pass to the replacement.
    users::delete(pool, &w.admin, &[w.sam.user()], w.ulf.user())
        .await
        .unwrap();
    assert_eq!(
        get(veras.id).await.unwrap().unwrap().use_users,
        vec![w.ulf.user()]
    );

    // A deleted group's entries pass to the replacement group.
    let others = group(pool, &w.admin, "Others", &[]).await;
    groups::delete(pool, &w.admin, &[w.editors], others)
        .await
        .unwrap();
    assert_eq!(
        get(shared.id).await.unwrap().unwrap().view_groups,
        vec![others]
    );
}
