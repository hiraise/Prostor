//! Links on a real database (step 1.4c): link types and their administration, links seen from
//! both issues, visibility of the other end, adding and removing with history, links of new issues
//! and drafts, the panel and its pages, hierarchies and subtask counts.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use prostor_domain::access::Access;
use prostor_domain::access::role::PredefinedRole;
use prostor_domain::issues::history::{LoggedLink, Origin};
use prostor_domain::issues::values::Input;
use prostor_domain::links::{Direction, Refusal, Side};
use prostor_domain::projects::Template;
use prostor_domain::settings::Language;
use prostor_domain::validation::Problem;
use prostor_domain::{IssueId, LinkTypeId, ProjectId, UserId};
use prostor_service::auth::SignedIn;
use prostor_service::issue_links::{self, NewLinks, TypeInput, TypeUpdate};
use prostor_service::issues::{self, AudienceInput, FieldChange, IssueChange, IssueView, NewIssue};
use prostor_service::projects::{self, NewProjectInput};
use prostor_service::search::{self, SearchInput};
use prostor_service::setup::{self, Setup};
use prostor_service::users::{self, NewAccount};
use prostor_service::{Error, Passwords, access, roles};
use prostor_store::PgPool;
use prostor_store::access::Holder;
use prostor_store::issue_history::Direction as Way;
use prostor_store::sessions::Origin as SessionOrigin;
use prostor_testkit::TestDb;
use time::OffsetDateTime;
use time::macros::datetime;

/// A project's issues as the list shows them, newest first (the search with a project context).
async fn list(pool: &PgPool, viewer: &Access, project: ProjectId, limit: u32) -> issues::Described {
    search::search(
        pool,
        viewer,
        SearchInput {
            text: "sort: id".to_owned(),
            project: Some(project),
            limit,
            ..SearchInput::default()
        },
    )
    .await
    .unwrap()
    .issues
}

/// How many of a project's issues the viewer sees: all and unresolved.
async fn count(pool: &PgPool, viewer: &Access, project: ProjectId) -> (i64, i64) {
    search::count(
        pool,
        viewer,
        SearchInput {
            project: Some(project),
            limit: 1,
            ..SearchInput::default()
        },
    )
    .await
    .unwrap()
}

const T0: OffsetDateTime = datetime!(2026-10-01 09:00 UTC);
const WEB: Origin = Origin::Web;

fn passwords() -> Passwords {
    Passwords::new(4)
}

async fn set_up(pool: &PgPool, language: &str) -> SignedIn {
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
            system_language: language.to_owned(),
            base_url: None,
            time_zone: "UTC".to_owned(),
        },
        T0,
        &SessionOrigin::default(),
    )
    .await
    .unwrap()
}

async fn admin(pool: &PgPool) -> Access {
    let signed_in = set_up(pool, "en").await;
    access::load(pool, signed_in.user.id).await.unwrap()
}

async fn create_user(pool: &PgPool, actor: &Access, username: &str) -> UserId {
    users::create(
        pool,
        &passwords(),
        actor,
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
    .id
}

async fn project(pool: &PgPool, actor: &Access, name: &str, key: &str) -> ProjectId {
    projects::create(
        pool,
        actor,
        NewProjectInput {
            template: Template::Blank,
            name: name.to_owned(),
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

async fn person(
    pool: &PgPool,
    actor: &Access,
    username: &str,
    role: PredefinedRole,
    of: &[ProjectId],
) -> Access {
    let user = create_user(pool, actor, username).await;
    let role = prostor_store::roles::builtin_id(pool, role.key())
        .await
        .unwrap()
        .unwrap();
    let scopes: Vec<Option<ProjectId>> = of.iter().copied().map(Some).collect();
    roles::assign(pool, actor, role, Holder::User(user), &scopes)
        .await
        .unwrap();
    access::load(pool, user).await.unwrap()
}

/// A person holding a custom role of these permissions in a project.
async fn custom(
    pool: &PgPool,
    admin: &Access,
    username: &str,
    permissions: &[&str],
    project: ProjectId,
) -> Access {
    let user = create_user(pool, admin, username).await;
    let role = roles::create(
        pool,
        admin,
        roles::NewRole {
            name: format!("Role of {username}"),
            description: String::new(),
            permissions: permissions.iter().map(|p| (*p).to_owned()).collect(),
        },
    )
    .await
    .unwrap();
    roles::assign(
        pool,
        admin,
        role.role.id,
        Holder::User(user),
        &[Some(project)],
    )
    .await
    .unwrap();
    access::load(pool, user).await.unwrap()
}

async fn create(pool: &PgPool, actor: &Access, project: ProjectId, summary: &str) -> IssueId {
    issues::create(
        pool,
        actor,
        WEB,
        NewIssue {
            project,
            summary: summary.to_owned(),
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

async fn view(pool: &PgPool, viewer: &Access, issue: IssueId) -> IssueView {
    issues::get(pool, viewer, issue).await.unwrap().0
}

async fn builtin(pool: &PgPool, key: &str) -> LinkTypeId {
    prostor_store::links::type_by_builtin(pool, key)
        .await
        .unwrap()
        .unwrap()
        .id
}

async fn link(
    pool: &PgPool,
    actor: &Access,
    from: IssueId,
    link_type: LinkTypeId,
    side: Side,
    others: &[IssueId],
) -> Result<issue_links::Added, Error> {
    issue_links::add(
        pool,
        actor,
        WEB,
        from,
        NewLinks {
            link_type,
            side,
            issues: others.to_vec(),
        },
        false,
    )
    .await
    .map(|answer| answer.done)
}

/// The panel as `(type, side, name, [issue IDs])`.
async fn panel(
    pool: &PgPool,
    viewer: &Access,
    issue: IssueId,
) -> Vec<(LinkTypeId, Side, String, Vec<IssueId>)> {
    issue_links::panel(pool, viewer, issue)
        .await
        .unwrap()
        .groups
        .into_iter()
        .map(|g| {
            (
                g.link_type.id,
                g.side,
                issue_links::side_name(&g.link_type, g.side).to_owned(),
                g.rows.iter().map(|r| r.issue.id).collect(),
            )
        })
        .collect()
}

/// Link records of an issue as the viewer's stream shows them: `(removed, added)`.
async fn link_records(
    pool: &PgPool,
    viewer: &Access,
    issue: IssueId,
) -> Vec<(Vec<LoggedLink>, Vec<LoggedLink>)> {
    let page = issues::activity::page(
        pool,
        viewer,
        issue,
        issues::activity::Query {
            comments: false,
            changes: true,
            from: None,
            direction: Way::Newer,
            limit: 100,
            around_comment: None,
        },
    )
    .await
    .unwrap();
    let read = |json: Option<&serde_json::Value>| -> Vec<LoggedLink> {
        json.and_then(serde_json::Value::as_array)
            .map(|a| a.iter().filter_map(LoggedLink::read).collect())
            .unwrap_or_default()
    };
    page.items
        .iter()
        .map(issues::activity::Item::entry)
        .filter(|e| e.kind == "links")
        .map(|e| (read(e.removed.as_ref()), read(e.added.as_ref())))
        .collect()
}

fn problems(error: Error) -> Vec<(String, Problem)> {
    match error {
        Error::Invalid(problems) => problems.into_iter().map(|p| (p.path, p.problem)).collect(),
        other => panic!("expected invalid input, got {other:?}"),
    }
}

async fn resolve(pool: &PgPool, actor: &Access, issue: IssueId) {
    let v = view(pool, actor, issue).await;
    let state = v
        .fields
        .iter()
        .find(|f| f.field.field.name == "State")
        .unwrap();
    let set = state.field.attached.set_id.unwrap();
    let fixed = prostor_store::value_sets::values_of(pool, &[set])
        .await
        .unwrap()
        .into_iter()
        .find(|v| v.resolved)
        .unwrap();
    issues::set_field(
        pool,
        actor,
        WEB,
        issue,
        state.field.id(),
        FieldChange::Set(Some(Input::Integer(fixed.id))),
        false,
    )
    .await
    .unwrap();
}

fn type_input(name: &str, source: &str, target: Option<&str>, direction: Direction) -> TypeInput {
    TypeInput {
        name: name.to_owned(),
        source_name: source.to_owned(),
        target_name: target.map(str::to_owned),
        direction,
    }
}

#[tokio::test]
async fn lnk_001_lnk_002_lnk_018_builtin_types_are_seeded_once_and_locked() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let types = issue_links::list_types(pool, &admin).await.unwrap();
    let names: Vec<(&str, &str, Option<&str>, &str, bool)> = types
        .iter()
        .map(|r| {
            let t = &r.link_type;
            (
                t.name.as_str(),
                t.source_name.as_str(),
                t.target_name.as_deref(),
                t.direction.as_str(),
                t.locked,
            )
        })
        .collect();
    assert_eq!(
        names,
        [
            ("Related", "related to", None, "undirected", false),
            (
                "Dependency",
                "blocks",
                Some("blocked by"),
                "directed",
                false
            ),
            (
                "Duplicate",
                "has duplicate",
                Some("duplicate of"),
                "hierarchy",
                true
            ),
            ("Subtask", "parent of", Some("child of"), "hierarchy", true),
        ]
    );
    assert!(types.iter().all(|r| r.usage == Some(0)));
    // Created once: startup finds them.
    assert!(
        !issue_links::ensure_default_types(pool, Language::En)
            .await
            .unwrap()
    );
    // Locked types neither change nor go (LNK-002).
    let subtask = builtin(pool, "subtask").await;
    assert!(matches!(
        issue_links::update_type(
            pool,
            &admin,
            subtask,
            TypeUpdate {
                name: Some("Children".to_owned()),
                version: 1,
                ..TypeUpdate::default()
            },
        )
        .await,
        Err(Error::Link(Refusal::Locked))
    ));
    assert!(matches!(
        issue_links::delete_type(pool, &admin, subtask).await,
        Err(Error::Link(Refusal::Locked))
    ));
    // Everyone sees the types in use, without counts; managing them needs `system.write`.
    let member = person(pool, &admin, "mia", PredefinedRole::ProjectMember, &[web]).await;
    let seen = issue_links::list_types(pool, &member).await.unwrap();
    assert_eq!(seen.len(), 4);
    assert!(seen.iter().all(|r| r.usage.is_none()));
    assert!(matches!(
        issue_links::create_type(
            pool,
            &member,
            type_input("Causes", "causes", Some("caused by"), Direction::Directed)
        )
        .await,
        Err(Error::Forbidden(_))
    ));
}

#[tokio::test]
async fn lnk_002_builtin_types_speak_the_instance_language() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let signed_in = set_up(pool, "ru").await;
    let admin = access::load(pool, signed_in.user.id).await.unwrap();
    let types = issue_links::list_types(pool, &admin).await.unwrap();
    let subtask = types
        .iter()
        .find(|r| r.link_type.builtin.as_deref() == Some("subtask"))
        .unwrap();
    assert_eq!(subtask.link_type.name, "Подзадача");
    assert_eq!(subtask.link_type.source_name, "родитель для");
    assert_eq!(
        subtask.link_type.target_name.as_deref(),
        Some("подзадача для")
    );
}

#[tokio::test]
async fn lnk_003_hist_005_lnk_008_one_link_seen_from_both_sides() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let app = project(pool, &admin, "App", "APP").await;
    let a = create(pool, &admin, web, "Blocker").await;
    let b = create(pool, &admin, app, "Blocked").await;
    let dependency = builtin(pool, "dependency").await;
    let added = link(pool, &admin, a, dependency, Side::Source, &[b])
        .await
        .unwrap();
    assert_eq!(added.added, [b]);
    assert!(added.existing.is_empty());
    // Across projects, with each side's name.
    assert_eq!(
        panel(pool, &admin, a).await,
        [(dependency, Side::Source, "blocks".to_owned(), vec![b])]
    );
    assert_eq!(
        panel(pool, &admin, b).await,
        [(dependency, Side::Target, "blocked by".to_owned(), vec![a])]
    );
    // One record on each issue, with that issue's side (HIST-005).
    let on_a = link_records(pool, &admin, a).await;
    assert_eq!(on_a.len(), 1);
    assert_eq!(on_a[0].1[0].side, Side::Source);
    assert_eq!(on_a[0].1[0].issue, b);
    let on_b = link_records(pool, &admin, b).await;
    assert_eq!(on_b[0].1[0].side, Side::Target);
    assert_eq!(on_b[0].1[0].issue, a);
    // Removed from the other side, it leaves both (LNK-008).
    let id = issue_links::panel(pool, &admin, b).await.unwrap().groups[0].rows[0].link;
    issue_links::remove(pool, &admin, WEB, b, id, false)
        .await
        .unwrap();
    assert!(panel(pool, &admin, a).await.is_empty());
    assert!(panel(pool, &admin, b).await.is_empty());
    let on_a = link_records(pool, &admin, a).await;
    assert_eq!(on_a.len(), 2);
    assert_eq!(on_a[1].0[0].issue, b);
    assert_eq!(link_records(pool, &admin, b).await.len(), 2);
    // Usage counts follow (LNK-018).
    let types = issue_links::list_types(pool, &admin).await.unwrap();
    assert!(types.iter().all(|r| r.usage == Some(0)));
    // A link of another issue is not found here.
    let c = create(pool, &admin, web, "Third").await;
    link(pool, &admin, a, dependency, Side::Source, &[c])
        .await
        .unwrap();
    let other = issue_links::panel(pool, &admin, a).await.unwrap().groups[0].rows[0].link;
    assert!(matches!(
        issue_links::remove(pool, &admin, WEB, b, other, false).await,
        Err(Error::NotFound(_))
    ));
}

#[tokio::test]
async fn lnk_004_hist_007_links_to_unseen_issues_are_hidden() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let secret = project(pool, &admin, "Secret", "SEC").await;
    let member = person(pool, &admin, "mia", PredefinedRole::ProjectMember, &[web]).await;
    let parent = create(pool, &admin, web, "Parent").await;
    let visible = create(pool, &admin, web, "Visible child").await;
    let hidden = create(pool, &admin, secret, "Hidden child").await;
    let restricted = create(pool, &admin, web, "Restricted child").await;
    issues::update(
        pool,
        &admin,
        WEB,
        restricted,
        IssueChange {
            audience: Some(AudienceInput {
                users: vec![admin.user()],
                groups: vec![],
            }),
            ..IssueChange::default()
        },
    )
    .await
    .unwrap();
    let subtask = builtin(pool, "subtask").await;
    link(
        pool,
        &admin,
        parent,
        subtask,
        Side::Source,
        &[visible, hidden, restricted],
    )
    .await
    .unwrap();
    // The administrator sees all three; the member one (LNK-004, LNK-014).
    assert_eq!(view(pool, &admin, parent).await.subtasks.total, 3);
    let seen = view(pool, &member, parent).await;
    assert_eq!(seen.subtasks.total, 1);
    assert_eq!(seen.subtasks.unresolved, 1);
    assert_eq!(panel(pool, &member, parent).await[0].3, [visible]);
    let groups = issue_links::panel(pool, &member, parent)
        .await
        .unwrap()
        .groups;
    assert_eq!(groups[0].total, 1);
    // The record keeps only what the member may see (HIST-007).
    let records = link_records(pool, &member, parent).await;
    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0].1.iter().map(|l| l.issue).collect::<Vec<_>>(),
        [visible]
    );
    assert_eq!(link_records(pool, &admin, parent).await[0].1.len(), 3);
    // A parent the member cannot see is no parent for them.
    assert_eq!(
        view(pool, &admin, hidden).await.parent.unwrap().0.id,
        parent
    );
    let other_parent = create(pool, &admin, secret, "Secret parent").await;
    let child = create(pool, &admin, web, "Child of a secret").await;
    link(pool, &admin, other_parent, subtask, Side::Source, &[child])
        .await
        .unwrap();
    assert!(view(pool, &member, child).await.parent.is_none());
    assert!(link_records(pool, &member, child).await.is_empty());
    // Nobody links to what they cannot read.
    assert!(matches!(
        link(pool, &member, visible, subtask, Side::Source, &[hidden]).await,
        Err(Error::NotFound(_))
    ));
    // A deleted issue drops out.
    issues::delete(pool, &admin, WEB, visible).await.unwrap();
    assert!(panel(pool, &member, parent).await.is_empty());
    assert_eq!(view(pool, &admin, parent).await.subtasks.total, 2);
}

#[tokio::test]
async fn lnk_005_iss_022_adding_several_links_and_the_rules() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let old = project(pool, &admin, "Old", "OLD").await;
    let a = create(pool, &admin, web, "A").await;
    let b = create(pool, &admin, web, "B").await;
    let c = create(pool, &admin, web, "C").await;
    let archived = create(pool, &admin, old, "Archived").await;
    let related = builtin(pool, "related").await;
    // Several at once; the pair linked already is reported, whichever the direction.
    link(pool, &admin, b, related, Side::Source, &[a])
        .await
        .unwrap();
    let added = link(pool, &admin, a, related, Side::Source, &[b, c, c])
        .await
        .unwrap();
    assert_eq!(added.added, [c]);
    assert_eq!(added.existing, [b]);
    // Undirected: one group, whichever side made the link.
    assert_eq!(
        panel(pool, &admin, a).await,
        [(related, Side::Source, "related to".to_owned(), vec![c, b])]
    );
    // No self links, no empty requests.
    let error = link(pool, &admin, a, related, Side::Source, &[a])
        .await
        .unwrap_err();
    assert_eq!(problems(error), [("issues".to_owned(), Problem::Invalid)]);
    let error = link(pool, &admin, a, related, Side::Source, &[])
        .await
        .unwrap_err();
    assert_eq!(problems(error), [("issues".to_owned(), Problem::Required)]);
    // Archived projects are read-only (PRJ-061).
    projects::set_archived(pool, &admin, old, true)
        .await
        .unwrap();
    assert!(matches!(
        link(pool, &admin, a, related, Side::Source, &[archived]).await,
        Err(Error::Conflict(_))
    ));
    // An unknown or deleted type is not found.
    assert!(matches!(
        link(pool, &admin, a, 9_999, Side::Source, &[c]).await,
        Err(Error::NotFound(_))
    ));
    // Reading is not enough: `issue.link`, or the reporter with `issue.create` (ISS-022).
    let reader = custom(pool, &admin, "rita", &["project.basic", "issue.read"], web).await;
    assert!(matches!(
        link(pool, &reader, a, related, Side::Source, &[b]).await,
        Err(Error::Forbidden(_))
    ));
    let submitter = custom(
        pool,
        &admin,
        "sam",
        &["project.basic", "issue.read", "issue.create"],
        web,
    )
    .await;
    let own = create(pool, &submitter, web, "Mine").await;
    link(pool, &submitter, own, related, Side::Source, &[a])
        .await
        .unwrap();
    assert!(matches!(
        link(pool, &submitter, b, related, Side::Source, &[own]).await,
        Err(Error::Forbidden(_))
    ));
    // Silence needs `issue.silent`.
    assert!(matches!(
        issue_links::add(
            pool,
            &submitter,
            WEB,
            own,
            NewLinks {
                link_type: related,
                side: Side::Source,
                issues: vec![b],
            },
            true
        )
        .await,
        Err(Error::Forbidden(_))
    ));
    // Removing needs the same rights where it starts.
    let link_id = issue_links::panel(pool, &admin, b).await.unwrap().groups[0].rows[0].link;
    assert!(matches!(
        issue_links::remove(pool, &reader, WEB, b, link_id, false).await,
        Err(Error::Forbidden(_))
    ));
}

#[tokio::test]
async fn lnk_006_lnk_007_iss_009_links_made_with_a_new_issue_or_draft() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let parent = create(pool, &admin, web, "Epic").await;
    let subtask = builtin(pool, "subtask").await;
    // A new subtask of an issue, published at once (ISS-009, LNK-006).
    let (child, _) = issues::create(
        pool,
        &admin,
        WEB,
        NewIssue {
            project: web,
            summary: "Part one".to_owned(),
            publish: true,
            links: vec![NewLinks {
                link_type: subtask,
                side: Side::Target,
                issues: vec![parent],
            }],
            ..NewIssue::default()
        },
    )
    .await
    .unwrap()
    .done;
    assert_eq!(child.parent.as_ref().unwrap().0.id, parent);
    assert_eq!(view(pool, &admin, parent).await.subtasks.total, 1);
    // The parent's history says so; the child's creation covers its links.
    assert_eq!(link_records(pool, &admin, parent).await.len(), 1);
    assert!(link_records(pool, &admin, child.issue.id).await.is_empty());

    // A draft's links wait for publishing (LNK-007).
    let (draft, _) = issues::create(
        pool,
        &admin,
        WEB,
        NewIssue {
            project: web,
            summary: "Part two".to_owned(),
            publish: false,
            links: vec![NewLinks {
                link_type: subtask,
                side: Side::Target,
                issues: vec![parent],
            }],
            ..NewIssue::default()
        },
    )
    .await
    .unwrap()
    .done;
    assert_eq!(view(pool, &admin, parent).await.subtasks.total, 1);
    assert_eq!(panel(pool, &admin, parent).await[0].3, [child.issue.id]);
    assert_eq!(link_records(pool, &admin, parent).await.len(), 1);
    // Its own panel shows them.
    assert_eq!(panel(pool, &admin, draft.issue.id).await[0].3, [parent]);
    issues::publish(pool, &admin, WEB, draft.issue.id)
        .await
        .unwrap();
    assert_eq!(view(pool, &admin, parent).await.subtasks.total, 2);
    let records = link_records(pool, &admin, parent).await;
    assert_eq!(records.len(), 2);
    assert_eq!(records[1].1[0].issue, draft.issue.id);
    assert_eq!(records[1].1[0].side, Side::Source);
    // A second parent is refused with the new issue, which is not created.
    let other = create(pool, &admin, web, "Other epic").await;
    let before = count(pool, &admin, web).await;
    assert!(matches!(
        issues::create(
            pool,
            &admin,
            WEB,
            NewIssue {
                project: web,
                summary: "Twice".to_owned(),
                publish: true,
                links: vec![
                    NewLinks {
                        link_type: subtask,
                        side: Side::Target,
                        issues: vec![parent],
                    },
                    NewLinks {
                        link_type: subtask,
                        side: Side::Target,
                        issues: vec![other],
                    },
                ],
                ..NewIssue::default()
            },
        )
        .await,
        Err(Error::Link(Refusal::ParentTaken))
    ));
    assert_eq!(count(pool, &admin, web).await, before);
}

#[tokio::test]
async fn lnk_010_groups_order_and_pages() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let hub = create(pool, &admin, web, "Hub").await;
    let mut spokes = Vec::new();
    for n in 0..12 {
        spokes.push(create(pool, &admin, web, &format!("Spoke {n}")).await);
    }
    let related = builtin(pool, "related").await;
    let dependency = builtin(pool, "dependency").await;
    for spoke in &spokes {
        link(pool, &admin, hub, related, Side::Source, &[*spoke])
            .await
            .unwrap();
    }
    link(pool, &admin, hub, dependency, Side::Target, &[spokes[0]])
        .await
        .unwrap();
    resolve(pool, &admin, spokes[11]).await;
    let p = issue_links::panel(pool, &admin, hub).await.unwrap();
    // Groups in the order of the types; counts of the whole group.
    assert_eq!(p.groups.len(), 2);
    let g = &p.groups[0];
    assert_eq!(g.link_type.id, related);
    assert_eq!((g.total, g.unresolved), (12, 11));
    // Unresolved first, newest first; the first ten.
    let shown: Vec<IssueId> = g.rows.iter().map(|r| r.issue.id).collect();
    let expected: Vec<IssueId> = spokes[..11].iter().rev().take(10).copied().collect();
    assert_eq!(shown, expected);
    let next = g.next.unwrap();
    let (rest, _) =
        issue_links::group_page(pool, &admin, hub, related, Side::Target, Some(next), 10)
            .await
            .unwrap();
    assert_eq!(
        rest.rows.iter().map(|r| r.issue.id).collect::<Vec<_>>(),
        [spokes[0], spokes[11]]
    );
    assert!(rest.next.is_none());
    assert_eq!(p.groups[1].side, Side::Target);
    assert!(p.can_link);
}

#[tokio::test]
async fn lnk_013_lnk_015_hierarchies_have_one_parent_and_no_loops() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let a = create(pool, &admin, web, "A").await;
    let b = create(pool, &admin, web, "B").await;
    let c = create(pool, &admin, web, "C").await;
    let subtask = builtin(pool, "subtask").await;
    link(pool, &admin, a, subtask, Side::Source, &[b])
        .await
        .unwrap();
    link(pool, &admin, b, subtask, Side::Source, &[c])
        .await
        .unwrap();
    // C under A as well: one parent.
    assert!(matches!(
        link(pool, &admin, a, subtask, Side::Source, &[c]).await,
        Err(Error::Link(Refusal::ParentTaken))
    ));
    // A under C: a loop.
    assert!(matches!(
        link(pool, &admin, c, subtask, Side::Source, &[a]).await,
        Err(Error::Link(Refusal::Cycle))
    ));
    // Linking the same parent again is no change.
    let again = link(pool, &admin, c, subtask, Side::Target, &[b])
        .await
        .unwrap();
    assert_eq!(again.existing, [b]);
    // Concurrent parents for one child: exactly one wins.
    let d = create(pool, &admin, web, "D").await;
    let only_d = [d];
    let (first, second) = tokio::join!(
        link(pool, &admin, a, subtask, Side::Source, &only_d),
        link(pool, &admin, c, subtask, Side::Source, &only_d),
    );
    assert!(first.is_ok() != second.is_ok());
    // Duplicates: the duplicate points at its original (LNK-015).
    let duplicate = builtin(pool, "duplicate").await;
    let original = create(pool, &admin, web, "Original").await;
    let copy = create(pool, &admin, web, "Copy").await;
    link(pool, &admin, copy, duplicate, Side::Target, &[original])
        .await
        .unwrap();
    assert_eq!(
        panel(pool, &admin, original).await,
        [(
            duplicate,
            Side::Source,
            "has duplicate".to_owned(),
            vec![copy]
        )]
    );
    assert_eq!(panel(pool, &admin, copy).await[0].2, "duplicate of");
}

#[tokio::test]
async fn lnk_014_subtask_counts_in_lists() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let parent = create(pool, &admin, web, "Parent").await;
    let one = create(pool, &admin, web, "One").await;
    let two = create(pool, &admin, web, "Two").await;
    let subtask = builtin(pool, "subtask").await;
    link(pool, &admin, parent, subtask, Side::Source, &[one, two])
        .await
        .unwrap();
    resolve(pool, &admin, one).await;
    let list = list(pool, &admin, web, 50).await;
    let row = list.issues.iter().find(|v| v.issue.id == parent).unwrap();
    assert_eq!((row.subtasks.total, row.subtasks.unresolved), (2, 1));
    let leaf = list.issues.iter().find(|v| v.issue.id == two).unwrap();
    assert_eq!(leaf.subtasks.total, 0);
    // Parents come with one issue in full only.
    assert!(leaf.parent.is_none());
    assert_eq!(view(pool, &admin, two).await.parent.unwrap().0.id, parent);
}

#[tokio::test]
async fn lnk_019_lnk_020_lnk_021_managing_link_types() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    // Validation: names taken across types, a target name for directed types.
    let error = issue_links::create_type(
        pool,
        &admin,
        type_input("Blocking", "Blocks", Some("waits for"), Direction::Directed),
    )
    .await
    .unwrap_err();
    assert_eq!(
        problems(error),
        [("source_name".to_owned(), Problem::Taken)]
    );
    let error = issue_links::create_type(
        pool,
        &admin,
        type_input("Causality", "causes", None, Direction::Directed),
    )
    .await
    .unwrap_err();
    assert_eq!(
        problems(error),
        [("target_name".to_owned(), Problem::Required)]
    );
    // An undirected type ignores a target name.
    let mirror = issue_links::create_type(
        pool,
        &admin,
        type_input("Mirror", "mirrors", Some("ignored"), Direction::Undirected),
    )
    .await
    .unwrap();
    assert_eq!(mirror.link_type.target_name, None);
    let causality = issue_links::create_type(
        pool,
        &admin,
        type_input(
            "Causality",
            "causes",
            Some("caused by"),
            Direction::Directed,
        ),
    )
    .await
    .unwrap();
    let id = causality.link_type.id;
    let a = create(pool, &admin, web, "A").await;
    let b = create(pool, &admin, web, "B").await;
    let c = create(pool, &admin, web, "C").await;
    link(pool, &admin, a, id, Side::Source, &[b]).await.unwrap();
    link(pool, &admin, c, id, Side::Source, &[b]).await.unwrap();

    // Renames apply to every link at once (LNK-020).
    let renamed = issue_links::update_type(
        pool,
        &admin,
        id,
        TypeUpdate {
            source_name: Some("leads to".to_owned()),
            version: 1,
            ..TypeUpdate::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(panel(pool, &admin, a).await[0].2, "leads to");
    // A stale version is a conflict.
    assert!(matches!(
        issue_links::update_type(
            pool,
            &admin,
            id,
            TypeUpdate {
                name: Some("Late".to_owned()),
                version: 1,
                ..TypeUpdate::default()
            },
        )
        .await,
        Err(Error::Conflict(_))
    ));
    // B has two sources: the type cannot become a hierarchy.
    let version = renamed.link_type.version;
    assert!(matches!(
        issue_links::update_type(
            pool,
            &admin,
            id,
            TypeUpdate {
                direction: Some(Direction::Hierarchy),
                version,
                ..TypeUpdate::default()
            },
        )
        .await,
        Err(Error::Link(Refusal::HierarchyConflict))
    ));
    // Undirected drops the target name; B's links show under the one name.
    let undirected = issue_links::update_type(
        pool,
        &admin,
        id,
        TypeUpdate {
            direction: Some(Direction::Undirected),
            version,
            ..TypeUpdate::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(undirected.link_type.target_name, None);
    assert_eq!(
        panel(pool, &admin, b).await,
        [(id, Side::Source, "leads to".to_owned(), vec![c, a])]
    );
    // Directed again needs a target name; the original sources keep the source name.
    let error = issue_links::update_type(
        pool,
        &admin,
        id,
        TypeUpdate {
            direction: Some(Direction::Directed),
            version: undirected.link_type.version,
            ..TypeUpdate::default()
        },
    )
    .await
    .unwrap_err();
    assert_eq!(
        problems(error),
        [("target_name".to_owned(), Problem::Required)]
    );
    let directed = issue_links::update_type(
        pool,
        &admin,
        id,
        TypeUpdate {
            direction: Some(Direction::Directed),
            target_name: Some("follows".to_owned()),
            version: undirected.link_type.version,
            ..TypeUpdate::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(panel(pool, &admin, b).await[0].2, "follows");

    // Deleting hides the links and says how many (LNK-021); restoring brings them back.
    assert_eq!(
        issue_links::deletion_impact(pool, &admin, id)
            .await
            .unwrap(),
        2
    );
    issue_links::delete_type(pool, &admin, id).await.unwrap();
    assert!(panel(pool, &admin, a).await.is_empty());
    assert!(matches!(
        link(pool, &admin, a, id, Side::Source, &[c]).await,
        Err(Error::NotFound(_))
    ));
    let member = person(pool, &admin, "mia", PredefinedRole::ProjectMember, &[web]).await;
    assert!(
        issue_links::list_types(pool, &member)
            .await
            .unwrap()
            .iter()
            .all(|r| r.link_type.id != id)
    );
    // While it is deleted its names are free; then it cannot come back.
    let taker = issue_links::create_type(
        pool,
        &admin,
        type_input(
            "Causality",
            "precedes",
            Some("succeeds"),
            Direction::Directed,
        ),
    )
    .await
    .unwrap();
    let error = issue_links::restore_type(pool, &admin, id)
        .await
        .unwrap_err();
    assert_eq!(problems(error), [("name".to_owned(), Problem::Taken)]);
    let version = taker.link_type.version;
    issue_links::update_type(
        pool,
        &admin,
        taker.link_type.id,
        TypeUpdate {
            name: Some("Order".to_owned()),
            version,
            ..TypeUpdate::default()
        },
    )
    .await
    .unwrap();
    let restored = issue_links::restore_type(pool, &admin, id).await.unwrap();
    assert!(restored.link_type.deleted_at.is_none());
    assert_eq!(restored.usage, Some(2));
    assert_eq!(panel(pool, &admin, a).await[0].3, [b]);
    assert!(directed.link_type.version < restored.link_type.version);
}
