//! Saved searches (QRY-063…QRY-068, PRJ-019; D-038). A query is stored by IDs
//! (`domain::search::Stored`) and printed with the current names, so renames never break it;
//! sharing follows `domain::sharing` like tags. Running one applies the viewer's own rights
//! (QRY-069), not the owner's.

use serde_json::{Value, json};
use sqlx::PgConnection;
use taskmaster_domain::access::permission::Permission;
use taskmaster_domain::access::{Access, Projects};
use taskmaster_domain::search::saved::{self, Builtin, PER_OWNER_MAX};
use taskmaster_domain::search::stored::Refs;
use taskmaster_domain::search::{self as domain, Catalog, Clock, Ctx, Stored};
use taskmaster_domain::settings::Language;
use taskmaster_domain::sharing::{Audience, Shared};
use taskmaster_domain::validation::{Problem as InputProblem, Problems};
use taskmaster_domain::{Actor, ProjectId, UserId};
use taskmaster_query::{Problem, Query, Severity};
use taskmaster_store::saved_searches::{self as store, Listing, NewSavedSearch, SavedSearchId};
use taskmaster_store::search::catalog;
use taskmaster_store::{PgPool, audit, unique_violation};

use crate::tags::{self, sharing_change};
use crate::{Error, Result};

/// A saved search as its reader sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct SavedSearch {
    pub row: store::SavedSearch,
    /// The query with the current names.
    pub text: String,
    /// `deleted_reference` for each name whose target is gone; such a search does not run.
    pub problems: Vec<Problem>,
}

/// A change of a saved search; `None` keeps a value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchChange {
    pub name: Option<String>,
    /// A new query text, resolved with the editor's names.
    pub query: Option<String>,
    /// New audiences: the owner or a user administrator, with share-views (ACC-066).
    pub sharing: Option<Sharing>,
    /// A new owner: user administrators with share-views.
    pub owner: Option<UserId>,
    pub version: i64,
}

/// Who views (and so runs) and who edits a saved search.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sharing {
    pub view: Audience,
    pub edit: Audience,
}

fn taken(error: sqlx::Error) -> Error {
    match unique_violation(&error) {
        Some(store::NAME_KEY) => Error::field(
            "name",
            InputProblem::Taken,
            "you have a saved search with this name",
        ),
        _ => error.into(),
    }
}

fn not_found() -> Error {
    Error::NotFound("no such saved search")
}

/// Known to the viewer: views, edits or may delete it. Others get "not found".
fn knows(shared: &Shared, viewer: &Access) -> bool {
    shared.can_view(viewer) || shared.can_edit(viewer) || shared.may_delete(viewer)
}

/// The viewer's saved searches and those shared with them (QRY-065); the predefined ones are
/// created on the first call (QRY-068).
pub async fn list(
    pool: &PgPool,
    viewer: &Access,
    language: Language,
    listing: Listing,
    after: Option<(String, SavedSearchId)>,
    limit: i64,
) -> Result<Vec<SavedSearch>> {
    let mut conn = pool.acquire().await?;
    if listing != Listing::Shared {
        ensure_builtins(&mut conn, viewer.user(), language).await?;
    }
    let rows = store::listed(
        &mut *conn,
        viewer.user(),
        viewer.groups(),
        listing,
        after,
        limit,
    )
    .await?;
    described(&mut conn, viewer, rows).await
}

/// One saved search the viewer knows of.
pub async fn get(pool: &PgPool, viewer: &Access, id: SavedSearchId) -> Result<SavedSearch> {
    let mut conn = pool.acquire().await?;
    let row = store::get(&mut *conn, id)
        .await?
        .filter(|r| knows(&r.sharing(), viewer))
        .ok_or_else(not_found)?;
    one(&mut conn, viewer, row).await
}

/// Saves a query under a name for the viewer (QRY-063). A query with an error is refused.
pub async fn create(pool: &PgPool, viewer: &Access, name: &str, text: &str) -> Result<SavedSearch> {
    let mut problems = Problems::new();
    let name = problems.check("name", saved::name(name));
    problems.finish().map_err(Error::Invalid)?;
    let Some(name) = name else {
        return Err(Error::Internal("validated values are missing".to_owned()));
    };
    let mut tx = pool.begin().await?;
    if store::count_of_owner(&mut *tx, viewer.user()).await?
        >= i64::try_from(PER_OWNER_MAX).unwrap_or(i64::MAX)
    {
        return Err(Error::Conflict("you have too many saved searches"));
    }
    let stored = crate::search::bind(&mut tx, viewer, text).await?;
    let id = store::insert(
        &mut tx,
        &NewSavedSearch {
            owner: viewer.user(),
            name: &name,
            query: &encode(&stored)?,
            builtin: None,
            project: None,
        },
    )
    .await
    .map_err(taken)?;
    audit::record(
        &mut tx,
        Actor::User(viewer.user()),
        "saved_search.created",
        &target(id),
        json!({ "name": name, "query": text }),
    )
    .await?;
    tx.commit().await?;
    get(pool, viewer, id).await
}

/// Changes a saved search (QRY-064, QRY-065). Editors change the name and query only.
pub async fn update(
    pool: &PgPool,
    viewer: &Access,
    id: SavedSearchId,
    change: SearchChange,
) -> Result<SavedSearch> {
    let mut problems = Problems::new();
    let name = change
        .name
        .as_deref()
        .and_then(|n| problems.check("name", saved::name(n)));
    problems.finish().map_err(Error::Invalid)?;
    let mut tx = pool.begin().await?;
    let row = store::lock(&mut tx, id)
        .await?
        .filter(|r| knows(&r.sharing(), viewer))
        .ok_or_else(not_found)?;
    let shared = row.sharing();
    if row.version != change.version {
        return Err(Error::Conflict(
            "the saved search was changed by someone else",
        ));
    }
    if (change.name.is_some() || change.query.is_some()) && !shared.can_edit(viewer) {
        return Err(Error::Refused("you may not edit this saved search".into()));
    }
    if change.name.is_some() || change.query.is_some() {
        let query = match &change.query {
            Some(text) => encode(&crate::search::bind(&mut tx, viewer, text).await?)?,
            None => row.query.clone(),
        };
        store::update(&mut tx, id, name.as_deref().unwrap_or(&row.name), &query)
            .await
            .map_err(taken)?;
        audit::record(
            &mut tx,
            Actor::User(viewer.user()),
            "saved_search.changed",
            &target(id),
            json!({ "name": name, "query": change.query, "owner": row.owner_id }),
        )
        .await?;
    }
    if change.sharing.is_some() || change.owner.is_some() {
        let sharing = change.sharing.as_ref().map(|s| tags::Sharing {
            view: s.view.clone(),
            use_: Audience::default(),
            edit: s.edit.clone(),
        });
        let next = sharing_change(&mut tx, viewer, &shared, sharing.as_ref(), change.owner).await?;
        if next != shared {
            store::set_sharing(&mut tx, id, &next)
                .await
                .map_err(taken)?;
            audit::record(
                &mut tx,
                Actor::User(viewer.user()),
                "saved_search.shared",
                &target(id),
                json!({
                    "owner": next.owner,
                    "view": { "users": next.view.users, "groups": next.view.groups },
                    "edit": { "users": next.edit.users, "groups": next.edit.groups },
                }),
            )
            .await?;
        }
    }
    tx.commit().await?;
    get(pool, viewer, id).await
}

/// Deletes a saved search: its owner or a user administrator (QRY-064). A predefined one is
/// hidden, so it is not created again (QRY-068).
pub async fn delete(pool: &PgPool, viewer: &Access, id: SavedSearchId) -> Result<()> {
    let mut tx = pool.begin().await?;
    let row = store::lock(&mut tx, id)
        .await?
        .filter(|r| knows(&r.sharing(), viewer))
        .ok_or_else(not_found)?;
    if !row.sharing().may_delete(viewer) {
        return Err(Error::Refused(
            "only the owner or a user administrator deletes a saved search".into(),
        ));
    }
    if row.builtin.is_some() {
        store::hide(&mut tx, id).await?;
    } else {
        store::delete(&mut tx, id).await?;
        audit::record(
            &mut tx,
            Actor::User(viewer.user()),
            "saved_search.deleted",
            &target(id),
            json!({ "name": row.name, "owner": row.owner_id }),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// The query of a saved search the viewer sees, with the current names, parsed again: the
/// context of a search (QRY-053). A gone target answers `invalid_query`.
pub(crate) async fn query_for(
    conn: &mut PgConnection,
    viewer: &Access,
    id: SavedSearchId,
) -> Result<Query> {
    let row = store::get(&mut *conn, id)
        .await?
        .filter(|r| r.sharing().can_view(viewer))
        .ok_or_else(not_found)?;
    let view = one(conn, viewer, row).await?;
    if !view.problems.is_empty() {
        return Err(Error::InvalidQuery(view.problems));
    }
    let q = taskmaster_query::parse(&view.text);
    if q.problems.iter().any(|p| p.severity == Severity::Error) {
        return Err(Error::InvalidQuery(q.problems));
    }
    Ok(q)
}

/// The text of a stored query (a board's, BRD-029) by the viewer's names, with what is lost.
pub(crate) async fn text_of(
    conn: &mut PgConnection,
    viewer: &Access,
    query: &Value,
) -> Result<(String, Vec<taskmaster_query::Problem>)> {
    let stored = decode(query)?;
    let read = viewer.projects_with(Permission::IssueRead);
    let reader = taskmaster_store::boards::Viewer {
        me: viewer.user(),
        groups: viewer.groups(),
        read: &read,
    };
    let names = store::names(conn, &stored.refs(), &reader).await?;
    Ok(stored.text(&names))
}

/// The "unassigned" search of a new project (PRJ-019), owned by its creator; `None` when the
/// project has no assignee field. Runs in the project's creating transaction; answers the search
/// created, for the creator's subscription on it.
pub(crate) async fn create_unassigned(
    conn: &mut PgConnection,
    owner: UserId,
    project: ProjectId,
    key: &str,
    language: Language,
) -> Result<Option<SavedSearchId>> {
    let Some((_, field)) = store::assignee_field(&mut *conn, project).await? else {
        return Ok(None);
    };
    let text = saved::unassigned_query(key, &field);
    let stored = bind_system(conn, owner, &text, Some(vec![project])).await?;
    let name = saved::unassigned_name(language, key);
    let id = store::insert(
        conn,
        &NewSavedSearch {
            owner,
            name: &name,
            query: &encode(&stored)?,
            builtin: None,
            project: Some(project),
        },
    )
    .await
    .map_err(taken)?;
    Ok(Some(id))
}

/// The predefined searches the person has never had (QRY-068).
async fn ensure_builtins(conn: &mut PgConnection, me: UserId, language: Language) -> Result<()> {
    let had = store::builtin_keys(&mut *conn, me).await?;
    let missing: Vec<Builtin> = Builtin::ALL
        .into_iter()
        .filter(|b| !had.iter().any(|k| k == b.key()))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    let assignee = store::assignee_field_name(&mut *conn)
        .await?
        .unwrap_or_else(|| "assignee".to_owned());
    for builtin in missing {
        let stored = bind_system(conn, me, &builtin.query(&assignee), None).await?;
        store::ensure_builtin(
            conn,
            me,
            builtin.key(),
            builtin.name(language),
            &encode(&stored)?,
        )
        .await?;
    }
    Ok(())
}

/// Binds a query the system writes against every project's names, whoever sees them (the
/// reader's rights apply when it runs). A query that does not resolve is kept as written.
async fn bind_system(
    conn: &mut PgConnection,
    me: UserId,
    text: &str,
    scope: Option<Vec<ProjectId>>,
) -> Result<Stored> {
    bind_with(conn, me, text, scope, Vec::new()).await
}

/// The stored form of a system query that names one board (a backlog, BRD-089): the board is
/// known by its name even before anyone could see it.
pub(crate) async fn bind_board(
    conn: &mut PgConnection,
    me: UserId,
    text: &str,
    projects: &[ProjectId],
    board: (taskmaster_domain::BoardId, &str),
) -> Result<Stored> {
    let entry = taskmaster_domain::search::catalog::BoardEntry {
        id: board.0,
        name: board.1.to_owned(),
        query: None,
    };
    bind_with(conn, me, text, Some(projects.to_vec()), vec![entry]).await
}

async fn bind_with(
    conn: &mut PgConnection,
    me: UserId,
    text: &str,
    scope: Option<Vec<ProjectId>>,
    boards: Vec<taskmaster_domain::search::catalog::BoardEntry>,
) -> Result<Stored> {
    let q = taskmaster_query::parse(text);
    let projects: Vec<_> = catalog::projects(&mut *conn)
        .await?
        .into_iter()
        .filter(|r| !r.deleting)
        .map(|r| r.entry)
        .collect();
    let scope = scope.unwrap_or_else(|| domain::scope(&q, &projects, None).projects);
    let (fields, attached) = catalog::fields(&mut *conn, &scope).await?;
    let cat = Catalog {
        projects,
        fields,
        attached,
        boards,
        ..Catalog::default()
    };
    let ctx = Ctx {
        me,
        clock: Clock {
            now: jiff::Timestamp::now(),
            zone: jiff::tz::TimeZone::UTC,
        },
        private_readable: Projects::All,
        scope,
        hide_resolved: false,
    };
    let bindings = domain::resolve(&q, &cat, &ctx)
        .map(|r| r.bindings)
        .unwrap_or_default();
    Ok(Stored::new(&q, bindings))
}

async fn one(
    conn: &mut PgConnection,
    viewer: &Access,
    row: store::SavedSearch,
) -> Result<SavedSearch> {
    let mut all = described(conn, viewer, vec![row]).await?;
    all.pop().ok_or_else(not_found)
}

/// Texts of several saved searches with one load of names per kind.
async fn described(
    conn: &mut PgConnection,
    viewer: &Access,
    rows: Vec<store::SavedSearch>,
) -> Result<Vec<SavedSearch>> {
    let stored: Vec<Stored> = rows
        .iter()
        .map(|r| decode(&r.query))
        .collect::<Result<_>>()?;
    let mut refs = Refs::default();
    for s in &stored {
        let r = s.refs();
        refs.fields.extend(r.fields);
        refs.values.extend(r.values);
        refs.users.extend(r.users);
        refs.groups.extend(r.groups);
        refs.projects.extend(r.projects);
        refs.tags.extend(r.tags);
        refs.link_types.extend(r.link_types);
        refs.issues.extend(r.issues);
        refs.boards.extend(r.boards);
        refs.sprints.extend(r.sprints);
    }
    let read = viewer.projects_with(Permission::IssueRead);
    let reader = taskmaster_store::boards::Viewer {
        me: viewer.user(),
        groups: viewer.groups(),
        read: &read,
    };
    let names = store::names(conn, &refs, &reader).await?;
    Ok(rows
        .into_iter()
        .zip(stored)
        .map(|(row, s)| {
            let (text, problems) = s.text(&names);
            SavedSearch {
                row,
                text,
                problems,
            }
        })
        .collect())
}

pub(crate) fn encode(stored: &Stored) -> Result<Value> {
    serde_json::to_value(stored).map_err(|e| Error::Internal(e.to_string()))
}

pub(crate) fn decode(value: &Value) -> Result<Stored> {
    serde_json::from_value(value.clone())
        .map_err(|e| Error::Internal(format!("a stored query does not read: {e}")))
}

fn target(id: SavedSearchId) -> String {
    format!("saved_search:{id}")
}
