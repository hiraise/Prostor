//! Links between issues (LNK): link types and their administration, adding and removing links
//! with their history on both issues, the links panel, parents and subtask counts. Design:
//! decisions.md D-034.
//!
//! A link the viewer may not follow is invisible: its type is deleted, or the issue at the other
//! end is a draft, deleted, or out of the viewer's reach (LNK-004). Links of one issue are loaded
//! whole (at most `LINKS_PER_ISSUE_MAX`) and filtered by the visibility rule in Rust
//! (architecture.md §4).

use std::collections::BTreeMap;

use serde_json::json;
use sqlx::PgConnection;
use taskmaster_domain::access::Access;
use taskmaster_domain::access::permission::Permission;
use taskmaster_domain::access::visibility::Audience;
use taskmaster_domain::issues::history::{LoggedLink, Origin, Record};
use taskmaster_domain::issues::rights::{self, Facts};
use taskmaster_domain::links::{
    self as domain, ADD_MAX, BuiltinType, DEPTH_MAX, Direction, LINKS_PER_ISSUE_MAX, Refusal, Side,
    TypeNames,
};
use taskmaster_domain::live::Change;
use taskmaster_domain::settings::Language;
use taskmaster_domain::validation::{Problem, Problems};
use taskmaster_domain::{IssueId, LinkId, LinkTypeId, ProjectId};
use taskmaster_store::issue_history::{self, Author};
use taskmaster_store::issues::{self, Issue};
use taskmaster_store::links::{self, End, Link, LinkType, Linked, NewLinkType, TypeChange};
use taskmaster_store::projects::{self, Project};
use taskmaster_store::{PgPool, audit, instance, unique_violation};

use crate::live::Batch;
use crate::{Error, Result};

/// Links each group of the panel shows before "more" (LNK-010).
pub const GROUP_FIRST: usize = 10;
/// Most links on one page of a group.
pub const GROUP_PAGE_MAX: i64 = 100;

// ---------------------------------------------------------------------------------------------
// Link types (LNK-001, LNK-002, LNK-018…LNK-021)

/// A link type with how many links it has; the count only for those who may see the
/// administration of types.
#[derive(Debug, Clone)]
pub struct TypeRow {
    pub link_type: LinkType,
    pub usage: Option<i64>,
}

pub fn direction_of(link_type: &LinkType) -> Direction {
    Direction::parse(&link_type.direction).unwrap_or(Direction::Directed)
}

fn names_of(link_type: &LinkType) -> TypeNames {
    TypeNames {
        name: link_type.name.clone(),
        source: link_type.source_name.clone(),
        target: link_type.target_name.clone(),
    }
}

/// The name a link shows on an issue that is at `side` of it.
pub fn side_name(link_type: &LinkType, side: Side) -> &str {
    match (side.shown(direction_of(link_type)), &link_type.target_name) {
        (Side::Target, Some(target)) => target,
        _ => &link_type.source_name,
    }
}

/// The link types (LNK-001). Everyone signed in sees those in use; holders of `system.read` also
/// see deleted ones and the usage counts (LNK-018).
pub async fn list_types(pool: &PgPool, viewer: &Access) -> Result<Vec<TypeRow>> {
    let admin = viewer.has(Permission::SystemRead);
    let mut conn = pool.acquire().await?;
    let all = links::all_types(&mut *conn).await?;
    let counts = if admin {
        links::usage_counts(&mut *conn).await?
    } else {
        Vec::new()
    };
    Ok(all
        .into_iter()
        .filter(|t| admin || !t.is_deleted())
        .map(|t| TypeRow {
            usage: admin.then(|| {
                counts
                    .iter()
                    .find(|(id, _)| *id == t.id)
                    .map_or(0, |(_, n)| *n)
            }),
            link_type: t,
        })
        .collect())
}

async fn type_row(conn: &mut PgConnection, id: LinkTypeId) -> Result<TypeRow> {
    let link_type = links::get_type(&mut *conn, id)
        .await?
        .ok_or(Error::NotFound("no such link type"))?;
    let usage = links::usage_of(&mut *conn, id).await?;
    Ok(TypeRow {
        link_type,
        usage: Some(usage),
    })
}

/// A new link type as given.
#[derive(Debug, Clone)]
pub struct TypeInput {
    pub name: String,
    pub source_name: String,
    /// Ignored for undirected types.
    pub target_name: Option<String>,
    pub direction: Direction,
}

/// Validates the names a type gets and checks them against the other types that are not deleted.
async fn checked_names(
    conn: &mut PgConnection,
    own: Option<LinkTypeId>,
    name: &str,
    source: &str,
    target: Option<&str>,
    direction: Direction,
) -> Result<TypeNames> {
    let mut problems = Problems::new();
    let name = problems.check("name", domain::name(name));
    let source = problems.check("source_name", domain::name(source));
    let target = if direction.has_target_name() {
        match target {
            Some(raw) => problems.check("target_name", domain::name(raw)).map(Some),
            None => {
                problems.add(
                    "target_name",
                    taskmaster_domain::validation::Rejection::new(
                        Problem::Required,
                        "a directed type needs a name for the target side",
                    ),
                );
                None
            }
        }
    } else {
        Some(None)
    };
    problems.finish().map_err(Error::Invalid)?;
    let (Some(name), Some(source), Some(target)) = (name, source, target) else {
        return Err(Error::Internal("validated values are missing".to_owned()));
    };
    let names = TypeNames {
        name,
        source,
        target,
    };
    let others: Vec<TypeNames> = links::all_types(&mut *conn)
        .await?
        .iter()
        .filter(|t| !t.is_deleted() && Some(t.id) != own)
        .map(names_of)
        .collect();
    domain::check_unique(&names, &others)
        .finish()
        .map_err(Error::Invalid)?;
    Ok(names)
}

/// Creates a link type (LNK-019). Needs `system.write`.
pub async fn create_type(pool: &PgPool, actor: &Access, input: TypeInput) -> Result<TypeRow> {
    domain::may_manage_types(actor)?;
    let mut tx = pool.begin().await?;
    links::lock_types(&mut tx, None).await?;
    let names = checked_names(
        &mut tx,
        None,
        &input.name,
        &input.source_name,
        input.target_name.as_deref(),
        input.direction,
    )
    .await?;
    let id = links::insert_type(
        &mut tx,
        &NewLinkType {
            name: &names.name,
            source_name: &names.source,
            target_name: names.target.as_deref(),
            direction: input.direction.key(),
            builtin: None,
            locked: false,
        },
    )
    .await
    .map_err(name_taken)?;
    audit::record(
        &mut tx,
        actor_of(actor),
        "link_type.created",
        &target(id),
        json!({
            "name": names.name,
            "source_name": names.source,
            "target_name": names.target,
            "direction": input.direction.key(),
        }),
    )
    .await?;
    let row = type_row(&mut tx, id).await?;
    tx.commit().await?;
    Ok(row)
}

/// Changes of a link type; `None` keeps a value.
#[derive(Debug, Clone, Default)]
pub struct TypeUpdate {
    pub name: Option<String>,
    pub source_name: Option<String>,
    pub target_name: Option<String>,
    pub direction: Option<Direction>,
    pub version: i64,
}

/// Changes a link type (LNK-020): renames apply to every link at once; a type that becomes
/// undirected drops its target name and one that becomes directed needs one; a type becomes a
/// hierarchy only when its links already keep the hierarchy rules. Built-in locked types do not
/// change. Needs `system.write`.
pub async fn update_type(
    pool: &PgPool,
    actor: &Access,
    id: LinkTypeId,
    change: TypeUpdate,
) -> Result<TypeRow> {
    domain::may_manage_types(actor)?;
    let mut tx = pool.begin().await?;
    let current = links::lock_types(&mut tx, Some(id))
        .await?
        .ok_or(Error::NotFound("no such link type"))?;
    if current.is_deleted() {
        return Err(Error::Conflict(
            "the link type is deleted; restore it first",
        ));
    }
    if current.locked {
        return Err(Error::Link(Refusal::Locked));
    }
    if current.version != change.version {
        return Err(Error::Conflict("the link type was changed by someone else"));
    }
    let was = direction_of(&current);
    let direction = change.direction.unwrap_or(was);
    let target_name = change
        .target_name
        .as_deref()
        .or(current.target_name.as_deref());
    let names = checked_names(
        &mut tx,
        Some(id),
        change.name.as_deref().unwrap_or(&current.name),
        change
            .source_name
            .as_deref()
            .unwrap_or(&current.source_name),
        target_name,
        direction,
    )
    .await?;
    if direction == Direction::Hierarchy && was != Direction::Hierarchy {
        // Links of the type wait for this change (they share the type's row).
        let pairs = links::pairs_of_type(&mut *tx, id).await?;
        if !domain::is_forest(&pairs) {
            return Err(Error::Link(Refusal::HierarchyConflict));
        }
    }
    links::update_type(
        &mut tx,
        id,
        &TypeChange {
            name: &names.name,
            source_name: &names.source,
            target_name: names.target.as_deref(),
            direction: direction.key(),
        },
    )
    .await
    .map_err(name_taken)?;
    let changed = |old: &str, new: &str| (old != new).then(|| json!({ "old": old, "new": new }));
    audit::record(
        &mut tx,
        actor_of(actor),
        "link_type.updated",
        &target(id),
        json!({
            "name": changed(&current.name, &names.name),
            "source_name": changed(&current.source_name, &names.source),
            "target_name": (current.target_name != names.target)
                .then(|| json!({ "old": current.target_name, "new": names.target })),
            "direction": changed(was.key(), direction.key()),
        }),
    )
    .await?;
    let row = type_row(&mut tx, id).await?;
    tx.commit().await?;
    Ok(row)
}

/// How many links deleting a type hides (LNK-021). Needs `system.write`.
pub async fn deletion_impact(pool: &PgPool, actor: &Access, id: LinkTypeId) -> Result<i64> {
    domain::may_manage_types(actor)?;
    let mut conn = pool.acquire().await?;
    links::get_type(&mut *conn, id)
        .await?
        .ok_or(Error::NotFound("no such link type"))?;
    Ok(links::usage_of(&mut *conn, id).await?)
}

/// Puts a link type in the trash (LNK-021, D-013): its links are hidden everywhere until it is
/// restored. Built-in locked types stay. Needs `system.write`.
pub async fn delete_type(pool: &PgPool, actor: &Access, id: LinkTypeId) -> Result<()> {
    domain::may_manage_types(actor)?;
    let mut tx = pool.begin().await?;
    let current = links::lock_types(&mut tx, Some(id))
        .await?
        .ok_or(Error::NotFound("no such link type"))?;
    if current.locked {
        return Err(Error::Link(Refusal::Locked));
    }
    if current.is_deleted() {
        return Err(Error::Conflict("the link type is deleted already"));
    }
    let usage = links::usage_of(&mut *tx, id).await?;
    links::mark_type_deleted(&mut tx, id, actor.user()).await?;
    audit::record(
        &mut tx,
        actor_of(actor),
        "link_type.deleted",
        &target(id),
        json!({ "name": current.name, "links": usage }),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Takes a link type out of the trash, with its links. Refused while another type has taken one
/// of its names. Needs `system.write`.
pub async fn restore_type(pool: &PgPool, actor: &Access, id: LinkTypeId) -> Result<TypeRow> {
    domain::may_manage_types(actor)?;
    let mut tx = pool.begin().await?;
    let current = links::lock_types(&mut tx, Some(id))
        .await?
        .ok_or(Error::NotFound("no such link type"))?;
    if !current.is_deleted() {
        return Err(Error::Conflict("the link type is not deleted"));
    }
    checked_names(
        &mut tx,
        Some(id),
        &current.name,
        &current.source_name,
        current.target_name.as_deref(),
        direction_of(&current),
    )
    .await?;
    links::restore_type(&mut tx, id).await?;
    audit::record(
        &mut tx,
        actor_of(actor),
        "link_type.restored",
        &target(id),
        json!({ "name": current.name }),
    )
    .await?;
    let row = type_row(&mut tx, id).await?;
    tx.commit().await?;
    Ok(row)
}

/// Creates the built-in link types once, in the instance language (LNK-002), when the setup is
/// complete and they do not exist yet; `true` when they were created now. Every node calls this at
/// startup; the setup wizard creates them in its own transaction.
pub async fn ensure_default_types(pool: &PgPool, language: Language) -> Result<bool> {
    let mut tx = pool.begin().await?;
    let created = default_types(&mut tx, language).await?;
    tx.commit().await?;
    Ok(created)
}

/// Creates the built-in link types inside the caller's transaction, unless they exist.
pub async fn default_types(conn: &mut PgConnection, language: Language) -> Result<bool> {
    if !instance::lock_unseeded_link_types(conn).await? {
        return Ok(false);
    }
    links::lock_types(conn, None).await?;
    let existing = links::all_types(&mut *conn).await?;
    for builtin in BuiltinType::ALL {
        if existing
            .iter()
            .any(|t| t.builtin.as_deref() == Some(builtin.key()))
        {
            continue;
        }
        let seed = builtin.seed();
        let name = seed.name.get(language);
        // A type someone made with the same internal name keeps it; ours gets the key's name.
        let name = if existing
            .iter()
            .any(|t| !t.is_deleted() && t.name.to_lowercase() == name.to_lowercase())
        {
            builtin.key()
        } else {
            name
        };
        links::insert_type(
            conn,
            &NewLinkType {
                name,
                source_name: seed.source.get(language),
                target_name: seed.target.map(|n| n.get(language)),
                direction: seed.direction.key(),
                builtin: Some(builtin.key()),
                locked: seed.locked,
            },
        )
        .await?;
    }
    instance::link_types_seeded(conn).await?;
    Ok(true)
}

fn name_taken(error: sqlx::Error) -> Error {
    if unique_violation(&error) == Some(links::NAME_KEY) {
        Error::field("name", Problem::Taken, "another link type has this name")
    } else {
        error.into()
    }
}

fn target(id: LinkTypeId) -> String {
    format!("link_type:{id}")
}

fn actor_of(access: &Access) -> taskmaster_domain::Actor {
    taskmaster_domain::Actor::User(access.user())
}

// ---------------------------------------------------------------------------------------------
// Visibility of the other end (LNK-004)

fn linked_facts(l: &Linked) -> Facts<'_> {
    Facts {
        project: l.project_id,
        reporter: l.reporter_id,
        draft: l.number.is_none(),
        audience: Audience {
            users: &l.audience_users,
            groups: &l.audience_groups,
        },
    }
}

/// Whether the viewer may follow a link to this issue: a published issue they may read, not
/// deleted, in a project that is not being deleted.
pub(crate) fn can_follow(viewer: &Access, l: &Linked, projects: &[Project]) -> bool {
    l.number.is_some()
        && l.deleted_at.is_none()
        && projects
            .iter()
            .any(|p| p.id == l.project_id && !p.is_deleting())
        && rights::can_read(viewer, &linked_facts(l))
}

async fn projects_of(conn: &mut PgConnection, issues: &[&Linked]) -> Result<Vec<Project>> {
    let mut ids: Vec<ProjectId> = issues.iter().map(|l| l.project_id).collect();
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    Ok(projects::get_many(&mut *conn, &ids).await?)
}

/// Of some linked issues, those the viewer may follow, with their projects.
pub(crate) async fn followable(
    conn: &mut PgConnection,
    viewer: &Access,
    issues: Vec<Linked>,
) -> Result<(Vec<Linked>, Vec<Project>)> {
    let found = projects_of(conn, &issues.iter().collect::<Vec<_>>()).await?;
    let kept: Vec<Linked> = issues
        .into_iter()
        .filter(|l| can_follow(viewer, l, &found))
        .collect();
    Ok((kept, found))
}

// ---------------------------------------------------------------------------------------------
// Adding and removing links (LNK-003, LNK-005…LNK-008, LNK-013)

/// Links to add from one issue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewLinks {
    pub link_type: LinkTypeId,
    /// The side of the issue the links start from: `Source` makes it the parent of the others
    /// with the subtask type.
    pub side: Side,
    pub issues: Vec<IssueId>,
}

/// What adding links did: the issues linked now and those that were linked already.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Added {
    pub added: Vec<IssueId>,
    pub existing: Vec<IssueId>,
    /// The built-in kind of the type and whether the starting issue is the source, for the
    /// behaviors of step 1.9 (§3.5); `None` for a type of the instance's own.
    pub builtin: Option<(BuiltinType, bool)>,
}

/// Adds links from one issue (LNK-005): `may_link` on it, reading the others, both projects
/// active; no self links; pairs linked already are reported, not refused. In a hierarchy each
/// target keeps one source and no loop forms (LNK-013). Records go to both issues, unless the
/// starting issue is a draft: its links are recorded on the others when it is published.
pub async fn add(
    pool: &PgPool,
    actor: &Access,
    origin: Origin,
    issue: IssueId,
    input: NewLinks,
    silent: bool,
) -> Result<crate::rules::Answer<Added>> {
    let mut tx = pool.begin().await?;
    // Every issue involved, in ID order, so concurrent links never deadlock.
    let mut all = input.issues.clone();
    all.push(issue);
    all.sort_unstable();
    all.dedup();
    if all.len() > ADD_MAX + 1 {
        return Err(Error::field(
            "issues",
            Problem::TooLong,
            format!("link at most {ADD_MAX} issues at once"),
        ));
    }
    let locked = issues::lock_many(&mut tx, &all).await?;
    let found = locked.into_iter().find(|i| i.id == issue);
    let (from, project) = crate::issues::checked(&mut tx, actor, found).await?;
    crate::projects::require_active(&project)?;
    let mut batch = Batch::new();
    let added = add_locked(&mut tx, &mut batch, actor, origin, &from, input, silent).await?;
    // The behaviors hear the links of a built-in type before the change's own records (§3.5).
    let mut ctx = crate::rules::Ctx::new(actor);
    let mut rule_records = Vec::new();
    if let Some((kind, issue_is_source)) = added.builtin
        && !added.added.is_empty()
    {
        let others: Vec<(IssueId, bool)> = added
            .added
            .iter()
            .map(|other| (*other, !issue_is_source))
            .collect();
        rule_records.extend(
            crate::rules::on(
                &mut tx,
                &mut batch,
                &mut ctx,
                crate::rules::Event::LinksAdded {
                    issue: &from,
                    kind,
                    others: &others,
                },
            )
            .await?,
        );
        crate::rules::finish(&mut tx, &mut batch, &ctx, &from, &rule_records).await?;
    }
    batch.publish(&mut tx).await?;
    tx.commit().await?;
    Ok(crate::rules::Answer {
        done: added,
        messages: ctx.messages,
    })
}

/// Adds links to an issue the caller locked and may see, inside its transaction (step 2.1
/// §4.3): what [`add`] does between its begin and its commit — the built-in behaviors hear the
/// links on the caller's `ctx`, their events joining `batch`.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn add_conn(
    conn: &mut PgConnection,
    batch: &mut Batch,
    ctx: &mut crate::rules::Ctx<'_>,
    actor: &Access,
    origin: Origin,
    from: &Issue,
    input: NewLinks,
    silent: bool,
) -> Result<()> {
    let added = add_locked(conn, batch, actor, origin, from, input, silent).await?;
    let mut rule_records = Vec::new();
    if let Some((kind, issue_is_source)) = added.builtin
        && !added.added.is_empty()
    {
        let others: Vec<(IssueId, bool)> = added
            .added
            .iter()
            .map(|other| (*other, !issue_is_source))
            .collect();
        rule_records.extend(
            crate::rules::on(
                conn,
                batch,
                ctx,
                crate::rules::Event::LinksAdded {
                    issue: from,
                    kind,
                    others: &others,
                },
            )
            .await?,
        );
        crate::rules::finish(conn, batch, ctx, from, &rule_records).await?;
    }
    Ok(())
}

/// Adds links from an issue the caller locked and may see, inside its transaction.
pub(crate) async fn add_locked(
    tx: &mut PgConnection,
    batch: &mut Batch,
    actor: &Access,
    origin: Origin,
    from: &Issue,
    input: NewLinks,
    silent: bool,
) -> Result<Added> {
    domain::may_link(actor, &crate::issues::facts(from))?;
    let author = crate::issues::author(actor, origin, silent, from.project_id)?;
    let mut others = input.issues.clone();
    others.sort_unstable();
    others.dedup();
    if others.is_empty() {
        return Err(Error::field(
            "issues",
            Problem::Required,
            "name the issues to link",
        ));
    }
    if others.len() > ADD_MAX {
        return Err(Error::field(
            "issues",
            Problem::TooLong,
            format!("link at most {ADD_MAX} issues at once"),
        ));
    }
    if others.contains(&from.id) {
        return Err(Error::field(
            "issues",
            Problem::Invalid,
            "an issue cannot be linked to itself",
        ));
    }
    // Locks go issues first (in ID order; the caller holds them already or the starting issue is
    // a new draft nobody else sees), then the type, then its hierarchy.
    let locked = issues::lock_many(&mut *tx, &others).await?;
    let link_type = links::share_type(&mut *tx, input.link_type)
        .await?
        .filter(|t| !t.is_deleted())
        .ok_or(Error::NotFound("no such link type"))?;
    let direction = direction_of(&link_type);
    if direction == Direction::Hierarchy {
        links::lock_hierarchy(&mut *tx, link_type.id).await?;
    }
    let mut project_ids: Vec<ProjectId> = locked.iter().map(|i| i.project_id).collect();
    project_ids.sort_unstable();
    project_ids.dedup();
    let found_projects = projects::get_many(&mut *tx, &project_ids).await?;
    for id in &others {
        let readable = locked.iter().find(|i| i.id == *id).and_then(|i| {
            let p = found_projects.iter().find(|p| p.id == i.project_id)?;
            (!i.is_draft() && crate::issues::can_read(actor, i, p)).then_some(p)
        });
        let Some(p) = readable else {
            return Err(Error::NotFound("no such issue"));
        };
        crate::projects::require_active(p)?;
    }
    let pairs: Vec<(IssueId, IssueId)> = others
        .iter()
        .map(|other| match input.side {
            Side::Source => (from.id, *other),
            Side::Target => (*other, from.id),
        })
        .collect();
    if direction == Direction::Hierarchy {
        check_hierarchy(&mut *tx, link_type.id, &pairs).await?;
    }
    let mut counted = others.clone();
    counted.push(from.id);
    let counts = links::counts_of(&mut *tx, &counted).await?;
    let over = |id: IssueId, more: usize| {
        let has = counts
            .iter()
            .find(|(i, _)| *i == id)
            .map_or(0, |(_, n)| usize::try_from(*n).unwrap_or(usize::MAX));
        has.saturating_add(more) > LINKS_PER_ISSUE_MAX
    };
    if over(from.id, others.len()) || others.iter().any(|id| over(*id, 1)) {
        return Err(Error::Conflict("an issue has too many links"));
    }
    let made = links::insert_many(&mut *tx, link_type.id, &pairs, actor.user()).await?;
    let other_of = |l: &Link| {
        if l.source_id == from.id {
            l.target_id
        } else {
            l.source_id
        }
    };
    let added: Vec<IssueId> = made.iter().map(other_of).collect();
    let existing: Vec<IssueId> = others
        .iter()
        .copied()
        .filter(|id| !added.contains(id))
        .collect();
    for link in &made {
        batch.link(link.id, Change::Created);
    }
    if !made.is_empty() {
        record(&mut *tx, batch, author, from, &link_type, &made, &[]).await?;
    }
    Ok(Added {
        added,
        existing,
        builtin: link_type
            .builtin
            .as_deref()
            .and_then(BuiltinType::parse)
            .map(|kind| (kind, input.side == Side::Source)),
    })
}

/// Refuses hierarchy links that would give a target a second source or close a loop (D-013).
/// The pairs share one end, so this takes two queries whatever their number.
async fn check_hierarchy(
    conn: &mut PgConnection,
    link_type: LinkTypeId,
    pairs: &[(IssueId, IssueId)],
) -> Result<()> {
    let mut sources: Vec<IssueId> = pairs.iter().map(|p| p.0).collect();
    sources.sort_unstable();
    sources.dedup();
    let targets: Vec<IssueId> = pairs.iter().map(|p| p.1).collect();
    let Some(source) = sources.first().copied() else {
        return Ok(());
    };
    // Several sources for one target: it can have only one.
    if sources.len() > 1 {
        return Err(Error::Link(Refusal::ParentTaken));
    }
    let taken = links::sources_of(&mut *conn, link_type, &targets).await?;
    if taken.iter().any(|(_, s)| *s != source) {
        return Err(Error::Link(Refusal::ParentTaken));
    }
    let above = links::ancestors(&mut *conn, link_type, source, DEPTH_MAX).await?;
    if targets.iter().any(|t| above.contains(t)) {
        return Err(Error::Link(Refusal::Cycle));
    }
    if i32::try_from(above.len()).unwrap_or(i32::MAX) >= DEPTH_MAX {
        return Err(Error::Conflict("the hierarchy is too deep"));
    }
    Ok(())
}

/// Records links that came or left on both of their issues (HIST-005) and marks the issues
/// changed. A draft gets no records and the issues it links to are left alone: they get their
/// record when it is published. The duplicates rule reuses it for the links it rewrites (§3.8).
pub(crate) async fn record(
    conn: &mut PgConnection,
    batch: &mut Batch,
    author: Author,
    from: &Issue,
    link_type: &LinkType,
    added: &[Link],
    removed: &[Link],
) -> Result<()> {
    if from.is_draft() {
        issues::touch(conn, from.id, author.user).await?;
        batch.issue(from.id, Change::Updated);
        return Ok(());
    }
    let end = |l: &Link, issue: IssueId| LoggedLink {
        link_type: link_type.id,
        side: if l.source_id == issue {
            Side::Source
        } else {
            Side::Target
        },
        issue: if l.source_id == issue {
            l.target_id
        } else {
            l.source_id
        },
    };
    let mut ids: Vec<IssueId> = Vec::new();
    let mut records: Vec<Record> = Vec::new();
    let own_added: Vec<LoggedLink> = added.iter().map(|l| end(l, from.id)).collect();
    let own_removed: Vec<LoggedLink> = removed.iter().map(|l| end(l, from.id)).collect();
    if let Some(r) = Record::links(&own_removed, &own_added) {
        ids.push(from.id);
        records.push(r);
    }
    for (list, is_added) in [(added, true), (removed, false)] {
        for l in list {
            let other = end(l, from.id).issue;
            let theirs = [end(l, other)];
            let r = if is_added {
                Record::links(&[], &theirs)
            } else {
                Record::links(&theirs, &[])
            };
            if let Some(r) = r {
                ids.push(other);
                records.push(r);
            }
        }
    }
    issue_history::record_many(&mut *conn, &ids, author, &records).await?;
    // Both ends changed visibly: each issue's watchers hear its own links line (plan §3.2).
    let found = issues::get_many(&mut *conn, &ids).await?;
    for (id, record) in ids.iter().zip(&records) {
        if let Some(issue) = found.iter().find(|i| i.id == *id) {
            crate::notifications::record(
                &mut *conn,
                issue,
                author.user,
                std::slice::from_ref(record),
                &[],
            )
            .await?;
        }
    }
    let mut touched = ids;
    touched.sort_unstable();
    touched.dedup();
    issues::touch_many(conn, &touched, author.user).await?;
    for id in touched {
        batch.issue(id, Change::Updated);
    }
    Ok(())
}

/// Records the links of a draft being published on the issues at their other ends (LNK-007):
/// the draft's own record of creation covers what it started with.
pub(crate) async fn record_published(
    conn: &mut PgConnection,
    batch: &mut Batch,
    author: Author,
    issue: IssueId,
) -> Result<()> {
    let ends = links::of_issue(&mut *conn, issue).await?;
    let mut ids = Vec::new();
    let mut records = Vec::new();
    for e in ends.iter().filter(|e| e.other.number.is_some()) {
        // Hidden while the issue was a draft, so new to everyone else.
        batch.link(e.link, Change::Created);
        let theirs = [LoggedLink {
            link_type: e.type_id,
            side: if e.is_source {
                Side::Target
            } else {
                Side::Source
            },
            issue,
        }];
        if let Some(r) = Record::links(&[], &theirs) {
            ids.push(e.other.id);
            records.push(r);
        }
    }
    issue_history::record_many(&mut *conn, &ids, author, &records).await?;
    let mut touched = ids.clone();
    touched.sort_unstable();
    touched.dedup();
    issues::touch_many(conn, &touched, author.user).await?;
    for id in touched {
        batch.issue(id, Change::Updated);
    }
    Ok(())
}

/// Removes a link from one of its issues (LNK-008): `may_link` there, the other end visible, both
/// projects active; recorded on both issues.
pub async fn remove(
    pool: &PgPool,
    actor: &Access,
    origin: Origin,
    issue: IssueId,
    link: LinkId,
    silent: bool,
) -> Result<Vec<crate::rules::Message>> {
    let mut tx = pool.begin().await?;
    let found = links::get(&mut *tx, link)
        .await?
        .filter(|l| l.source_id == issue || l.target_id == issue)
        .ok_or(Error::NotFound("no such link"))?;
    let other = if found.source_id == issue {
        found.target_id
    } else {
        found.source_id
    };
    let mut both = vec![issue, other];
    both.sort_unstable();
    let locked = issues::lock_many(&mut tx, &both).await?;
    let from = locked.iter().find(|i| i.id == issue).cloned();
    let (from, project) = crate::issues::checked(&mut tx, actor, from).await?;
    crate::projects::require_active(&project)?;
    let mut batch = Batch::new();
    remove_checked(&mut tx, &mut batch, actor, origin, &from, &found, silent).await?;
    // The behaviors hear a removed link of a built-in type (§3.5); duplicates reopen on it (1.9b).
    let mut ctx = crate::rules::Ctx::new(actor);
    let mut rule_records = Vec::new();
    if let Some(kind) = links::share_type(&mut tx, found.type_id)
        .await?
        .and_then(|kind| kind.builtin)
        .as_deref()
        .and_then(BuiltinType::parse)
    {
        rule_records.extend(
            crate::rules::on(
                &mut tx,
                &mut batch,
                &mut ctx,
                crate::rules::Event::LinkRemoved {
                    issue: &from,
                    kind,
                    other,
                    was_source: found.source_id == other,
                },
            )
            .await?,
        );
        crate::rules::finish(&mut tx, &mut batch, &ctx, &from, &rule_records).await?;
    }
    batch.publish(&mut tx).await?;
    tx.commit().await?;
    Ok(ctx.messages)
}

/// Unlinks `target` from `from` by one link type inside the caller's transaction (step 2.1
/// §4.5): the unlink and the built-in behaviors of [`remove`]; `false` when no such link is
/// there. The other end is locked the way [`remove`] locks it.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn remove_conn(
    conn: &mut PgConnection,
    batch: &mut Batch,
    ctx: &mut crate::rules::Ctx<'_>,
    actor: &Access,
    origin: Origin,
    from: &Issue,
    link_type: LinkTypeId,
    target: IssueId,
    silent: bool,
) -> Result<bool> {
    let found = links::of_issue(&mut *conn, from.id)
        .await?
        .into_iter()
        .find(|end| end.type_id == link_type && end.other.id == target);
    let Some(end) = found else {
        return Ok(false);
    };
    issues::lock_many(&mut *conn, &[target]).await?;
    let Some(link) = links::get(&mut *conn, end.link).await? else {
        return Ok(false);
    };
    remove_checked(conn, batch, actor, origin, from, &link, silent).await?;
    let mut rule_records = Vec::new();
    if let Some(kind) = links::share_type(&mut *conn, link.type_id)
        .await?
        .and_then(|kind| kind.builtin)
        .as_deref()
        .and_then(BuiltinType::parse)
    {
        rule_records.extend(
            crate::rules::on(
                conn,
                batch,
                ctx,
                crate::rules::Event::LinkRemoved {
                    issue: from,
                    kind,
                    other: target,
                    was_source: link.source_id == target,
                },
            )
            .await?,
        );
        crate::rules::finish(conn, batch, ctx, from, &rule_records).await?;
    }
    Ok(true)
}

/// Removes a link of an issue the caller locked, may see and whose project is active, inside its
/// transaction (a board re-parents a card, BRD-051): the other end is locked here.
pub(crate) async fn remove_locked(
    tx: &mut PgConnection,
    batch: &mut Batch,
    actor: &Access,
    origin: Origin,
    from: &Issue,
    found: &Link,
) -> Result<()> {
    let other = if found.source_id == from.id {
        found.target_id
    } else {
        found.source_id
    };
    issues::lock_many(&mut *tx, &[other]).await?;
    remove_checked(tx, batch, actor, origin, from, found, false).await
}

async fn remove_checked(
    tx: &mut PgConnection,
    batch: &mut Batch,
    actor: &Access,
    origin: Origin,
    from: &Issue,
    found: &Link,
    silent: bool,
) -> Result<()> {
    let link = found.id;
    let other = if found.source_id == from.id {
        found.target_id
    } else {
        found.source_id
    };
    domain::may_link(actor, &crate::issues::facts(from))?;
    let author = crate::issues::author(actor, origin, silent, from.project_id)?;
    let link_type = links::share_type(&mut *tx, found.type_id)
        .await?
        .filter(|t| !t.is_deleted())
        .ok_or(Error::NotFound("no such link"))?;
    let other_linked = links::linked_many(&mut *tx, &[other]).await?;
    let (visible, other_projects) = followable(&mut *tx, actor, other_linked).await?;
    if visible.is_empty() {
        return Err(Error::NotFound("no such link"));
    }
    if let Some(p) = other_projects.first() {
        crate::projects::require_active(p)?;
    }
    if direction_of(&link_type) == Direction::Hierarchy {
        links::lock_hierarchy(&mut *tx, link_type.id).await?;
    }
    // Gone meanwhile: nothing to do.
    if links::get(&mut *tx, link).await?.is_none() {
        return Ok(());
    }
    // The facts of a removed link are taken before it goes.
    for facts in taskmaster_store::live::link_facts(&mut *tx, &[link]).await? {
        batch.link_removed(facts);
    }
    links::delete(&mut *tx, link).await?;
    record(
        &mut *tx,
        batch,
        author,
        from,
        &link_type,
        &[],
        std::slice::from_ref(found),
    )
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// The links panel (LNK-010) and subtasks (LNK-013, LNK-014)

/// A linked issue as a row shows it.
#[derive(Debug, Clone)]
pub struct LinkRow {
    pub link: LinkId,
    pub created_at: time::OffsetDateTime,
    pub issue: Linked,
}

/// The links of one name on an issue: one type seen from one side.
#[derive(Debug, Clone)]
pub struct Group {
    pub link_type: LinkType,
    /// The side this issue is on; `Source` for undirected types.
    pub side: Side,
    pub total: i64,
    pub unresolved: i64,
    /// Unresolved first, then newest.
    pub rows: Vec<LinkRow>,
    /// Where the next page of the group starts.
    pub next: Option<GroupCursor>,
}

/// Where a page of a group starts: after this row in the group's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GroupCursor {
    pub resolved: bool,
    pub link: LinkId,
}

/// An issue's links as the viewer sees them, with the projects of the linked issues.
#[derive(Debug, Clone, Default)]
pub struct Panel {
    pub groups: Vec<Group>,
    pub projects: Vec<Project>,
    /// Whether the viewer may add and remove links here.
    pub can_link: bool,
}

/// Every link of an issue the viewer may follow, by group (type, shown side), in group order.
async fn visible_groups(
    conn: &mut PgConnection,
    viewer: &Access,
    issue: IssueId,
) -> Result<(
    BTreeMap<(LinkTypeId, Side), Vec<LinkRow>>,
    Vec<LinkType>,
    Vec<Project>,
)> {
    let ends: Vec<End> = links::of_issue(&mut *conn, issue).await?;
    let types: Vec<LinkType> = links::all_types(&mut *conn)
        .await?
        .into_iter()
        .filter(|t| !t.is_deleted())
        .collect();
    let found = projects_of(conn, &ends.iter().map(|e| &e.other).collect::<Vec<_>>()).await?;
    let mut groups: BTreeMap<(LinkTypeId, Side), Vec<LinkRow>> = BTreeMap::new();
    for e in ends {
        let Some(t) = types.iter().find(|t| t.id == e.type_id) else {
            continue;
        };
        if !can_follow(viewer, &e.other, &found) {
            continue;
        }
        let side = if e.is_source {
            Side::Source
        } else {
            Side::Target
        }
        .shown(direction_of(t));
        groups.entry((t.id, side)).or_default().push(LinkRow {
            link: e.link,
            created_at: e.created_at,
            issue: e.other,
        });
    }
    for rows in groups.values_mut() {
        rows.sort_by_key(|r| (r.issue.resolved_at.is_some(), std::cmp::Reverse(r.link)));
    }
    Ok((groups, types, found))
}

/// The links panel of an issue (LNK-004, LNK-010): groups by type and side, unresolved first and
/// then newest, totals and unresolved counts, the first rows of each.
pub async fn panel(pool: &PgPool, viewer: &Access, issue: IssueId) -> Result<Panel> {
    let mut conn = pool.acquire().await?;
    let found = issues::get(&mut *conn, issue).await?;
    let (from, _) = crate::issues::checked(&mut conn, viewer, found).await?;
    panel_of(&mut conn, viewer, &from).await
}

/// The links panel of an issue already loaded and checked for the viewer.
pub(crate) async fn panel_of(
    conn: &mut PgConnection,
    viewer: &Access,
    from: &Issue,
) -> Result<Panel> {
    let (groups, types, found_projects) = visible_groups(conn, viewer, from.id).await?;
    let mut out = Vec::with_capacity(groups.len());
    for ((type_id, side), rows) in groups {
        let Some(t) = types.iter().find(|t| t.id == type_id) else {
            continue;
        };
        let total = i64::try_from(rows.len()).unwrap_or(i64::MAX);
        let unresolved = i64::try_from(
            rows.iter()
                .filter(|r| r.issue.resolved_at.is_none())
                .count(),
        )
        .unwrap_or(i64::MAX);
        let next = (rows.len() > GROUP_FIRST).then(|| cursor_of(&rows[GROUP_FIRST - 1]));
        out.push(Group {
            link_type: t.clone(),
            side,
            total,
            unresolved,
            rows: rows.into_iter().take(GROUP_FIRST).collect(),
            next,
        });
    }
    // Groups in the order of the types, the source side first.
    Ok(Panel {
        groups: out,
        projects: found_projects,
        can_link: domain::may_link(viewer, &crate::issues::facts(from)).is_ok(),
    })
}

fn cursor_of(row: &LinkRow) -> GroupCursor {
    GroupCursor {
        resolved: row.issue.resolved_at.is_some(),
        link: row.link,
    }
}

/// A page of one group of the panel (LNK-010): the rows after `after`.
pub async fn group_page(
    pool: &PgPool,
    viewer: &Access,
    issue: IssueId,
    link_type: LinkTypeId,
    side: Side,
    after: Option<GroupCursor>,
    limit: i64,
) -> Result<(Group, Vec<Project>)> {
    let mut conn = pool.acquire().await?;
    let found = issues::get(&mut *conn, issue).await?;
    crate::issues::checked(&mut conn, viewer, found).await?;
    let (mut groups, types, found_projects) = visible_groups(&mut conn, viewer, issue).await?;
    let t = types
        .iter()
        .find(|t| t.id == link_type)
        .ok_or(Error::NotFound("no such link type"))?;
    let side = side.shown(direction_of(t));
    let rows = groups.remove(&(link_type, side)).unwrap_or_default();
    let total = i64::try_from(rows.len()).unwrap_or(i64::MAX);
    let unresolved = i64::try_from(
        rows.iter()
            .filter(|r| r.issue.resolved_at.is_none())
            .count(),
    )
    .unwrap_or(i64::MAX);
    let start = match after {
        None => 0,
        Some(c) => rows
            .iter()
            .position(|r| {
                (r.issue.resolved_at.is_some(), std::cmp::Reverse(r.link))
                    > (c.resolved, std::cmp::Reverse(c.link))
            })
            .unwrap_or(rows.len()),
    };
    let limit = usize::try_from(limit.clamp(1, GROUP_PAGE_MAX)).unwrap_or(1);
    let page: Vec<LinkRow> = rows.into_iter().skip(start).collect();
    let next = (page.len() > limit).then(|| cursor_of(&page[limit - 1]));
    Ok((
        Group {
            link_type: t.clone(),
            side,
            total,
            unresolved,
            rows: page.into_iter().take(limit).collect(),
            next,
        },
        found_projects,
    ))
}

/// How many subtasks an issue has that the viewer may see, and how many are unresolved (LNK-014).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Subtasks {
    pub total: i64,
    pub unresolved: i64,
}

/// Subtask counts of a page of issues (LNK-014): one query for the links, one for the projects of
/// subtasks outside the page's projects. Issues without visible subtasks are left out.
pub(crate) async fn subtask_counts(
    conn: &mut PgConnection,
    viewer: &Access,
    parents: &[IssueId],
    known: &[Project],
) -> Result<Vec<(IssueId, Subtasks)>> {
    if parents.is_empty() {
        return Ok(Vec::new());
    }
    let children = links::children_of(&mut *conn, parents).await?;
    if children.is_empty() {
        return Ok(Vec::new());
    }
    let mut missing: Vec<ProjectId> = children
        .iter()
        .map(|(_, c)| c.project_id)
        .filter(|id| !known.iter().any(|p| p.id == *id))
        .collect();
    missing.sort_unstable();
    missing.dedup();
    let mut all_projects: Vec<Project> = known.to_vec();
    if !missing.is_empty() {
        all_projects.extend(projects::get_many(&mut *conn, &missing).await?);
    }
    let mut counts: BTreeMap<IssueId, Subtasks> = BTreeMap::new();
    for (parent, child) in &children {
        if !can_follow(viewer, child, &all_projects) {
            continue;
        }
        let c = counts.entry(*parent).or_default();
        c.total += 1;
        if child.resolved_at.is_none() {
            c.unresolved += 1;
        }
    }
    Ok(counts.into_iter().collect())
}

/// The parent of an issue through the subtask type, when the viewer may see it (LNK-013).
pub(crate) async fn parent(
    conn: &mut PgConnection,
    viewer: &Access,
    child: IssueId,
) -> Result<Option<(Linked, Project)>> {
    let Some(found) = links::parent_of(&mut *conn, child).await? else {
        return Ok(None);
    };
    let (mut kept, found_projects) = followable(conn, viewer, vec![found]).await?;
    let Some(l) = kept.pop() else {
        return Ok(None);
    };
    let project = found_projects
        .into_iter()
        .find(|p| p.id == l.project_id)
        .ok_or_else(|| Error::Internal("the parent's project is missing".to_owned()))?;
    Ok(Some((l, project)))
}
