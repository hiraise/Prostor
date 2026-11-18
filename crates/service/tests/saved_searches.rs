//! Saved searches (QRY-053, QRY-063…QRY-068, PRJ-019; plan step 1.5 D1, D8) on a real database.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use taskmaster_domain::access::Access;
use taskmaster_domain::access::capability::Capability;
use taskmaster_domain::access::role::PredefinedRole;
use taskmaster_domain::issues::history::Origin;
use taskmaster_domain::projects::Template;
use taskmaster_domain::settings::Language;
use taskmaster_domain::sharing::Audience;
use taskmaster_domain::{GroupId, IssueId, ProjectId, UserId};
use taskmaster_query::ProblemCode;
use taskmaster_service::groups::NewGroup;
use taskmaster_service::issues::{self, NewIssue};
use taskmaster_service::projects::{self, NewProjectInput};
use taskmaster_service::saved_searches::{self as saved, SavedSearch, SearchChange, Sharing};
use taskmaster_service::search::{self, SearchInput};
use taskmaster_service::setup::{self, Setup};
use taskmaster_service::users::{self, NewAccount};
use taskmaster_service::{Error, Passwords, access, capabilities, groups, roles, tags};
use taskmaster_store::PgPool;
use taskmaster_store::access::Holder;
use taskmaster_store::saved_searches::Listing;
use taskmaster_store::sessions::Origin as SessionOrigin;
use taskmaster_testkit::TestDb;
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
    let role = taskmaster_store::roles::builtin_id(pool, PredefinedRole::ProjectMember.key())
        .await
        .unwrap()
        .unwrap();
    let scopes: Vec<Option<ProjectId>> = of.iter().map(|p| Some(*p)).collect();
    roles::assign(pool, admin, role, Holder::User(user), &scopes)
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
        Origin::Web,
        NewIssue {
            project,
            summary: "Something".to_owned(),
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

/// Issues of a query within a saved search.
async fn run(
    pool: &PgPool,
    who: &Access,
    s: &SavedSearch,
    text: &str,
) -> Result<Vec<IssueId>, Error> {
    let page = search::search(
        pool,
        who,
        SearchInput {
            text: text.to_owned(),
            saved_search: Some(s.row.id),
            limit: 50,
            ..SearchInput::default()
        },
    )
    .await?;
    let mut ids: Vec<IssueId> = page.issues.issues.iter().map(|v| v.issue.id).collect();
    ids.sort_unstable();
    Ok(ids)
}

async fn change(
    pool: &PgPool,
    who: &Access,
    s: &SavedSearch,
    c: SearchChange,
) -> Result<SavedSearch, Error> {
    saved::update(
        pool,
        who,
        s.row.id,
        SearchChange {
            version: s.row.version,
            ..c
        },
    )
    .await
}

async fn names(pool: &PgPool, who: &Access, listing: Listing) -> Vec<String> {
    saved::list(pool, who, Language::En, listing, None, 100)
        .await
        .unwrap()
        .into_iter()
        .map(|s| s.row.name)
        .collect()
}

async fn sql(pool: &PgPool, statement: &'static str) {
    sqlx::query(statement).execute(pool).await.unwrap();
}

struct World {
    admin: Access,
    web: ProjectId,
    mob: ProjectId,
    olga: Access,
    vera: Access,
    eda: Access,
    editors: GroupId,
}

async fn world(pool: &PgPool) -> World {
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB", Template::Scrum).await;
    let mob = project(pool, &admin, "MOB", Template::Scrum).await;
    let olga = member(pool, &admin, "olga", &[web, mob]).await;
    let vera = member(pool, &admin, "vera", &[web]).await;
    let eda = member(pool, &admin, "eda", &[web]).await;
    let editors = group(pool, &admin, "Editors", &[eda.user()]).await;
    let eda = reload(pool, &eda).await;
    World {
        admin,
        web,
        mob,
        olga,
        vera,
        eda,
        editors,
    }
}

#[tokio::test]
async fn qry_063_stored_by_ids_survives_renames() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let x = issue(pool, &w.olga, w.web).await;
    let s = saved::create(pool, &w.olga, " Open ones ", "State: Open")
        .await
        .unwrap();
    assert_eq!(s.row.name, "Open ones");
    assert_eq!(s.text, "State: Open");
    assert!(s.problems.is_empty());
    assert_eq!(run(pool, &w.olga, &s, "").await.unwrap(), [x]);

    // The field and the value are renamed: the search prints the new words and still matches.
    sql(
        pool,
        "update fields set name = 'Status' where builtin = 'state'",
    )
    .await;
    sql(
        pool,
        "update field_values set name = 'New one' where name = 'Open'",
    )
    .await;
    let s = saved::get(pool, &w.olga, s.row.id).await.unwrap();
    assert_eq!(s.text, r#"Status: "New one""#);
    assert_eq!(run(pool, &w.olga, &s, "").await.unwrap(), [x]);

    // A query with an error and a taken name (any case) are refused.
    assert!(matches!(
        saved::create(pool, &w.olga, "Bad", "nosuchfield: x").await,
        Err(Error::InvalidQuery(_))
    ));
    assert!(matches!(
        saved::create(pool, &w.olga, "OPEN ONES", "").await,
        Err(Error::Invalid(_))
    ));

    // A target that is gone keeps its words, and running answers invalid_query.
    let tag = tags::create(pool, &w.olga, "release", None).await.unwrap();
    let t = saved::create(pool, &w.olga, "Release", "tag: release")
        .await
        .unwrap();
    tags::delete(pool, &w.olga, tag.id).await.unwrap();
    let t = saved::get(pool, &w.olga, t.row.id).await.unwrap();
    assert_eq!(t.text, "tag: release");
    assert_eq!(t.problems.len(), 1);
    assert_eq!(t.problems[0].code, ProblemCode::DeletedReference);
    assert_eq!(
        &t.text[t.problems[0].span.start..t.problems[0].span.end],
        "release"
    );
    assert!(matches!(
        run(pool, &w.olga, &t, "").await,
        Err(Error::InvalidQuery(_))
    ));
}

#[tokio::test]
async fn qry_064_editors_change_but_do_not_delete() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let s = saved::create(pool, &w.olga, "Mine", "State: Open")
        .await
        .unwrap();
    let s = change(
        pool,
        &w.olga,
        &s,
        SearchChange {
            sharing: Some(Sharing {
                edit: Audience {
                    users: Vec::new(),
                    groups: vec![w.editors],
                },
                ..Sharing::default()
            }),
            ..SearchChange::default()
        },
    )
    .await
    .unwrap();
    // Eda edits name and query, by her names.
    let s = change(
        pool,
        &w.eda,
        &s,
        SearchChange {
            name: Some("Done ones".to_owned()),
            query: Some("State: Done".to_owned()),
            ..SearchChange::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        (s.row.name.as_str(), s.text.as_str()),
        ("Done ones", "State: Done")
    );
    assert_eq!(s.row.owner_id, w.olga.user());
    // An old version is refused.
    assert!(matches!(
        saved::update(
            pool,
            &w.eda,
            s.row.id,
            SearchChange {
                name: Some("x".to_owned()),
                version: s.row.version - 1,
                ..SearchChange::default()
            }
        )
        .await,
        Err(Error::Conflict(_))
    ));
    // Not sharing, not deleting.
    assert!(matches!(
        change(
            pool,
            &w.eda,
            &s,
            SearchChange {
                sharing: Some(Sharing::default()),
                ..SearchChange::default()
            }
        )
        .await,
        Err(Error::Refused(_))
    ));
    assert!(matches!(
        saved::delete(pool, &w.eda, s.row.id).await,
        Err(Error::Refused(_))
    ));
    // Vera knows nothing of it; a user administrator deletes it.
    assert!(matches!(
        saved::get(pool, &w.vera, s.row.id).await,
        Err(Error::NotFound(_))
    ));
    saved::delete(pool, &w.admin, s.row.id).await.unwrap();
    assert!(matches!(
        saved::get(pool, &w.olga, s.row.id).await,
        Err(Error::NotFound(_))
    ));
}

#[tokio::test]
async fn qry_065_view_list_transfer_and_capability() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let s = saved::create(pool, &w.olga, "Open", "State: Open")
        .await
        .unwrap();
    let s = change(
        pool,
        &w.olga,
        &s,
        SearchChange {
            sharing: Some(Sharing {
                view: users_(&[w.vera.user(), w.olga.user()]),
                ..Sharing::default()
            }),
            ..SearchChange::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(s.row.view_users, [w.vera.user()]);
    assert_eq!(names(pool, &w.vera, Listing::Shared).await, ["Open"]);
    // Viewing is not editing.
    assert!(matches!(
        change(
            pool,
            &w.vera,
            &s,
            SearchChange {
                name: Some("Mine now".to_owned()),
                ..SearchChange::default()
            }
        )
        .await,
        Err(Error::Refused(_))
    ));
    // Only a user administrator gives it away.
    let to_vera = SearchChange {
        owner: Some(w.vera.user()),
        ..SearchChange::default()
    };
    assert!(matches!(
        change(pool, &w.olga, &s, to_vera.clone()).await,
        Err(Error::Refused(_))
    ));

    // Share-views only for Sharers, which has the administrator.
    let sharers = group(pool, &w.admin, "Sharers", &[w.admin.user()]).await;
    capabilities::set(pool, &w.admin, Capability::ShareViews, &[sharers])
        .await
        .unwrap();
    let olga = reload(pool, &w.olga).await;
    let admin = reload(pool, &w.admin).await;
    assert!(matches!(
        change(
            pool,
            &olga,
            &s,
            SearchChange {
                sharing: Some(Sharing::default()),
                ..SearchChange::default()
            }
        )
        .await,
        Err(Error::Refused(_))
    ));
    let s = change(pool, &admin, &s, to_vera).await.unwrap();
    assert_eq!(s.row.owner_id, w.vera.user());
    assert!(s.row.view_users.is_empty());
    assert!(
        names(pool, &w.vera, Listing::Mine)
            .await
            .contains(&"Open".to_owned())
    );
}

#[tokio::test]
async fn qry_065_deleted_accounts_and_groups() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let s = saved::create(pool, &w.olga, "Open", "State: Open")
        .await
        .unwrap();
    let s = change(
        pool,
        &w.olga,
        &s,
        SearchChange {
            sharing: Some(Sharing {
                view: Audience {
                    users: vec![w.vera.user(), w.admin.user()],
                    groups: vec![w.editors],
                },
                ..Sharing::default()
            }),
            ..SearchChange::default()
        },
    )
    .await
    .unwrap();
    // Both have predefined searches: the heir keeps theirs.
    names(pool, &w.olga, Listing::Mine).await;
    names(pool, &w.admin, Listing::Mine).await;
    saved::create(pool, &w.admin, "open", "").await.unwrap();

    users::delete(pool, &w.admin, &[w.olga.user()], w.admin.user())
        .await
        .unwrap();
    let moved = saved::get(pool, &w.admin, s.row.id).await.unwrap();
    assert_eq!(moved.row.owner_id, w.admin.user());
    assert_eq!(moved.row.name, format!("Open #{}", s.row.id));
    assert_eq!(moved.row.view_users, [w.vera.user()]);
    let mine = names(pool, &w.admin, Listing::Mine).await;
    assert_eq!(mine.iter().filter(|n| *n == "All issues").count(), 1);

    // Vera leaves: her place in the audience passes to Eda.
    users::delete(pool, &w.admin, &[w.vera.user()], w.eda.user())
        .await
        .unwrap();
    let moved = saved::get(pool, &w.admin, s.row.id).await.unwrap();
    assert_eq!(moved.row.view_users, [w.eda.user()]);

    // A deleted group's entries pass to the replacement group.
    let others = group(pool, &w.admin, "Others", &[]).await;
    groups::delete(pool, &w.admin, &[w.editors], others)
        .await
        .unwrap();
    let moved = saved::get(pool, &w.admin, s.row.id).await.unwrap();
    assert_eq!(moved.row.view_groups, [others]);
}

#[tokio::test]
async fn qry_068_predefined_once_in_the_instance_language() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let x = issue(pool, &w.olga, w.web).await;
    let first = names(pool, &w.olga, Listing::Mine).await;
    assert_eq!(
        first,
        [
            "All issues",
            "Assigned to me",
            "Commented by me",
            "Reported by me"
        ]
    );
    assert_eq!(names(pool, &w.olga, Listing::Mine).await, first);
    let ru: Vec<String> = saved::list(pool, &w.vera, Language::Ru, Listing::Mine, None, 100)
        .await
        .unwrap()
        .into_iter()
        .map(|s| s.row.name)
        .collect();
    assert!(ru.contains(&"Все задачи".to_owned()), "{ru:?}");

    let all = saved::list(pool, &w.olga, Language::En, Listing::Mine, None, 100)
        .await
        .unwrap();
    for s in &all {
        assert!(s.problems.is_empty(), "{}: {:?}", s.row.name, s.problems);
        run(pool, &w.olga, s, "").await.unwrap();
    }
    let reported = all.iter().find(|s| s.row.name == "Reported by me").unwrap();
    assert_eq!(run(pool, &w.olga, reported, "").await.unwrap(), [x]);
    let assigned = all.iter().find(|s| s.row.name == "Assigned to me").unwrap();
    assert_eq!(assigned.text, "Assignee: me is: unresolved");

    // Deleting one hides it for good.
    saved::delete(pool, &w.olga, assigned.row.id).await.unwrap();
    let left = names(pool, &w.olga, Listing::Mine).await;
    assert_eq!(left, ["All issues", "Commented by me", "Reported by me"]);
}

#[tokio::test]
async fn prj_019_unassigned_search_of_a_new_project() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let x = issue(pool, &w.olga, w.web).await;
    let all = saved::list(pool, &w.admin, Language::En, Listing::Mine, None, 100)
        .await
        .unwrap();
    let web = all
        .iter()
        .find(|s| s.row.name == "Unassigned in WEB")
        .unwrap();
    assert_eq!(web.row.project_id, Some(w.web));
    assert_eq!(web.text, "project: WEB Assignee: none is: unresolved");
    assert_eq!(run(pool, &w.admin, web, "").await.unwrap(), [x]);
    assert!(all.iter().any(|s| s.row.name == "Unassigned in MOB"));

    // A project without the assignee field gets none.
    sql(
        pool,
        "update fields set auto_attach = false where builtin = 'assignee'",
    )
    .await;
    let plain = project(pool, &w.admin, "PLN", Template::Blank).await;
    assert!(
        taskmaster_store::saved_searches::assignee_field(pool, plain)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        !names(pool, &w.admin, Listing::Mine)
            .await
            .contains(&"Unassigned in PLN".to_owned())
    );
}

#[tokio::test]
async fn qry_053_saved_search_context_follows_the_viewer() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let web = issue(pool, &w.olga, w.web).await;
    let mob = issue(pool, &w.olga, w.mob).await;
    let s = saved::create(pool, &w.olga, "Open", "State: Open")
        .await
        .unwrap();
    let s = change(
        pool,
        &w.olga,
        &s,
        SearchChange {
            sharing: Some(Sharing {
                view: users_(&[w.vera.user()]),
                ..Sharing::default()
            }),
            ..SearchChange::default()
        },
    )
    .await
    .unwrap();
    // The owner sees both; Vera only what she may read (QRY-069).
    assert_eq!(run(pool, &w.olga, &s, "").await.unwrap(), [web, mob]);
    assert_eq!(run(pool, &w.vera, &s, "").await.unwrap(), [web]);
    // The query narrows the saved search.
    assert_eq!(run(pool, &w.olga, &s, "project: MOB").await.unwrap(), [mob]);
    // Eda cannot run what she does not see.
    assert!(matches!(
        run(pool, &w.eda, &s, "").await,
        Err(Error::NotFound(_))
    ));
    // "Hide resolved" is ignored when the saved search is about resolution.
    let resolved = saved::create(pool, &w.olga, "Resolved", "is: resolved")
        .await
        .unwrap();
    let page = search::search(
        pool,
        &w.olga,
        SearchInput {
            saved_search: Some(resolved.row.id),
            hide_resolved: true,
            limit: 50,
            ..SearchInput::default()
        },
    )
    .await
    .unwrap();
    assert!(
        page.notes
            .iter()
            .any(|n| n.code == ProblemCode::HideResolvedIgnored)
    );
}
