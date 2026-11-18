//! Value sets (FLD-029…FLD-049): the list, one set with its values or members and who uses it,
//! and changes of values. A set shared by several projects changes for all of them, so changing it
//! needs `project.update` in every one (or `system.write`); otherwise a project forks its own copy
//! (FLD-030, FLD-031).

use serde_json::json;
use sqlx::PgConnection;
use taskmaster_domain::access::Access;
use taskmaster_domain::access::permission::Permission;
use taskmaster_domain::fields::{
    SET_GROUPS_MAX, SET_USERS_MAX, SetKind, VALUES_MAX, may_edit_set, may_see_fields, set_name,
    value_color, value_description, value_name,
};
use taskmaster_domain::validation::{Problem, Problems};
use taskmaster_domain::{GroupId, SetId, UserId, ValueId, rank};
use taskmaster_store::groups::{self, Group};
use taskmaster_store::projects::{self, Project};
use taskmaster_store::users::{self, User};
use taskmaster_store::value_sets::{self, NewValue, Usage, Value, ValueChange, ValueSet};
use taskmaster_store::{PgPool, audit, fields, unique_violation};
use time::Date;

use crate::issues::jobs::FieldWork;
use crate::projects::can_see;
use crate::seed::internal;
use crate::{Error, Result};

const SEARCH_MAX: usize = 100;

/// A set of the list with how many projects and fields use it.
#[derive(Debug, Clone)]
pub struct SetSummary {
    pub set: ValueSet,
    pub projects: i64,
    pub fields: i64,
}

/// One set with everything its editor shows.
#[derive(Debug, Clone)]
pub struct SetRow {
    pub set: ValueSet,
    /// Values in order (every kind but user).
    pub values: Vec<Value>,
    /// Owners of values with an owner.
    pub owners: Vec<User>,
    /// Accounts and groups of a user set.
    pub users: Vec<User>,
    pub groups: Vec<Group>,
    /// Projects using it that the viewer may see, and how many others.
    pub projects: Vec<Project>,
    pub hidden_projects: i64,
    /// Fields that offer it to new attachments.
    pub fields: Vec<i64>,
    /// Whether the viewer may change it.
    pub editable: bool,
}

/// Sets by name (FLD-025), optionally of one kind.
pub async fn list(
    pool: &PgPool,
    viewer: &Access,
    kind: Option<SetKind>,
    text: &str,
    after: Option<(String, SetId)>,
    limit: i64,
) -> Result<Vec<SetSummary>> {
    may_see_fields(viewer)?;
    let text: String = text.trim().chars().take(SEARCH_MAX).collect();
    let (name, id) = after.unwrap_or_default();
    let mut conn = pool.acquire().await?;
    let sets = match kind {
        Some(kind) => {
            value_sets::list_of_kind(&mut conn, kind.key(), &text, (&name, id), limit).await?
        }
        None => value_sets::list(&mut conn, &text, (&name, id), limit).await?,
    };
    let ids: Vec<SetId> = sets.iter().map(|s| s.id).collect();
    let counts = value_sets::usage_counts(&mut *conn, &ids).await?;
    Ok(sets
        .into_iter()
        .map(|set| {
            let (projects, fields) = counts
                .iter()
                .find(|(id, _, _)| *id == set.id)
                .map_or((0, 0), |(_, p, f)| (*p, *f));
            SetSummary {
                set,
                projects,
                fields,
            }
        })
        .collect())
}

/// One set (FLD-031: the editor shows which projects a change affects).
pub async fn get(pool: &PgPool, viewer: &Access, id: SetId) -> Result<SetRow> {
    may_see_fields(viewer)?;
    let mut conn = pool.acquire().await?;
    let set = value_sets::get(&mut *conn, id)
        .await?
        .ok_or(Error::NotFound("no such value set"))?;
    let usage = value_sets::usage(&mut *conn, id).await?;
    row(&mut conn, viewer, set, usage).await
}

async fn row(
    conn: &mut PgConnection,
    viewer: &Access,
    set: ValueSet,
    usage: Usage,
) -> Result<SetRow> {
    let values = value_sets::values_of(&mut *conn, &[set.id]).await?;
    let owner_ids: Vec<UserId> = values.iter().filter_map(|v| v.owner_id).collect();
    let owners = users::get_many(&mut *conn, &owner_ids).await?;
    let members = value_sets::members(conn, &[set.id]).await?;
    let user_ids: Vec<UserId> = members.users.iter().map(|(_, u)| *u).collect();
    let group_ids: Vec<GroupId> = members.groups.iter().map(|(_, g)| *g).collect();
    let mut people = users::get_many(&mut *conn, &user_ids).await?;
    people.sort_by_key(|u| user_ids.iter().position(|id| *id == u.id));
    let mut found_groups = groups::get_many(&mut *conn, &group_ids).await?;
    found_groups.sort_by_key(|g| group_ids.iter().position(|id| *id == g.id));
    let mut found_projects = projects::get_many(&mut *conn, &usage.projects).await?;
    let total = i64::try_from(found_projects.len()).unwrap_or(i64::MAX);
    found_projects.retain(|p| can_see(viewer, p));
    found_projects.sort_by_key(|p| p.name.to_lowercase());
    Ok(SetRow {
        editable: may_edit_set(viewer, &usage.projects).is_ok(),
        hidden_projects: total - i64::try_from(found_projects.len()).unwrap_or(i64::MAX),
        projects: found_projects,
        fields: usage.fields,
        users: people,
        groups: found_groups,
        values,
        owners,
        set,
    })
}

/// Locks a set and checks the actor may change it (FLD-031).
async fn lock_editable(conn: &mut PgConnection, actor: &Access, id: SetId) -> Result<ValueSet> {
    let set = value_sets::lock(conn, id)
        .await?
        .ok_or(Error::NotFound("no such value set"))?;
    if !actor.has(Permission::SystemRead) && !actor.has_anywhere(Permission::ProjectUpdate) {
        return Err(Error::NotFound("no such value set"));
    }
    let usage = value_sets::usage(&mut *conn, id).await?;
    may_edit_set(actor, &usage.projects)?;
    Ok(set)
}

fn kind_of(set: &ValueSet) -> Result<SetKind> {
    SetKind::parse(&set.kind).ok_or_else(|| Error::Internal("unknown set kind".to_owned()))
}

/// Creates an empty set (FLD-029). Needs `system.write`.
pub async fn create(pool: &PgPool, actor: &Access, name: &str, kind: SetKind) -> Result<SetRow> {
    actor.require(Permission::SystemWrite)?;
    let name = set_name(name).map_err(|r| Error::rejected("name", r))?;
    let mut tx = pool.begin().await?;
    let id = value_sets::insert(&mut tx, &name, kind.key())
        .await
        .map_err(set_name_taken)?;
    audit::record(
        &mut tx,
        actor_of(actor),
        "value_set.created",
        &target(id),
        json!({ "name": name, "kind": kind.key() }),
    )
    .await?;
    tx.commit().await?;
    get(pool, actor, id).await
}

/// Renames a set.
pub async fn rename(
    pool: &PgPool,
    actor: &Access,
    id: SetId,
    name: &str,
    version: i64,
) -> Result<SetRow> {
    let name = set_name(name).map_err(|r| Error::rejected("name", r))?;
    let mut tx = pool.begin().await?;
    let set = lock_editable(&mut tx, actor, id).await?;
    if set.version != version {
        return Err(Error::Conflict("the value set was changed by someone else"));
    }
    value_sets::rename(&mut tx, id, &name)
        .await
        .map_err(set_name_taken)?;
    audit::record(
        &mut tx,
        actor_of(actor),
        "value_set.renamed",
        &target(id),
        json!({ "old": set.name, "new": name }),
    )
    .await?;
    tx.commit().await?;
    get(pool, actor, id).await
}

/// Deletes a set nobody uses. Needs `system.write`.
pub async fn delete(pool: &PgPool, actor: &Access, id: SetId) -> Result<()> {
    actor.require(Permission::SystemWrite)?;
    let mut tx = pool.begin().await?;
    let set = value_sets::lock(&mut tx, id)
        .await?
        .ok_or(Error::NotFound("no such value set"))?;
    if value_sets::delete_unused(&mut tx, &[id]).await?.is_empty() {
        return Err(Error::Conflict("a field or a project uses the value set"));
    }
    audit::record(
        &mut tx,
        actor_of(actor),
        "value_set.deleted",
        &target(id),
        json!({ "name": set.name }),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Properties of a value; `None` keeps one (or uses the default for a new value). Dates and the
/// owner take `Some(None)` to clear.
#[derive(Debug, Clone, Default)]
pub struct ValueInput {
    pub name: Option<String>,
    pub description: Option<String>,
    pub color: Option<Option<i16>>,
    pub archived: Option<bool>,
    pub resolved: Option<bool>,
    pub released: Option<bool>,
    pub release_date: Option<Option<Date>>,
    pub start_date: Option<Option<Date>>,
    pub assembled_on: Option<Option<Date>>,
    pub owner: Option<Option<UserId>>,
}

/// Checks the properties against the kind of set: each kind has its own (FLD-045…FLD-049).
async fn check_value(
    conn: &mut PgConnection,
    kind: SetKind,
    input: &ValueInput,
) -> Result<(Option<String>, Option<String>, Option<Option<i16>>)> {
    let mut problems = Problems::new();
    let name = input
        .name
        .as_deref()
        .and_then(|raw| problems.check("name", value_name(raw)));
    let description = input
        .description
        .as_deref()
        .and_then(|raw| problems.check("description", value_description(raw)));
    let color = input
        .color
        .and_then(|color| problems.check("color", value_color(color)));
    let not_for = |path: &str, problems: &mut Problems| {
        problems.add(
            path,
            taskmaster_domain::validation::Rejection::new(
                Problem::Invalid,
                format!("values of a {} set have no {path}", kind.key()),
            ),
        );
    };
    if input.resolved.is_some() && kind != SetKind::State {
        not_for("resolved", &mut problems);
    }
    if (input.released.is_some() || input.release_date.is_some() || input.start_date.is_some())
        && kind != SetKind::Version
    {
        not_for("release", &mut problems);
    }
    if input.assembled_on.is_some() && kind != SetKind::Build {
        not_for("assembled_on", &mut problems);
    }
    if input.owner.is_some() && kind != SetKind::Owned {
        not_for("owner", &mut problems);
    }
    if input.archived.is_some() && kind == SetKind::User {
        not_for("archived", &mut problems);
    }
    for date in [input.release_date, input.start_date, input.assembled_on]
        .into_iter()
        .flatten()
        .flatten()
    {
        if !(taskmaster_domain::fields::YEAR_MIN..=taskmaster_domain::fields::YEAR_MAX)
            .contains(&date.year())
        {
            problems.add(
                "date",
                taskmaster_domain::validation::Rejection::new(
                    Problem::OutOfRange,
                    "dates are within years 1–9999",
                ),
            );
        }
    }
    problems.finish().map_err(Error::Invalid)?;
    if let Some(Some(owner)) = input.owner {
        let user = users::get(&mut *conn, owner).await?;
        if user.is_none_or(|u| u.is_deleted() || u.is_guest()) {
            return Err(Error::field("owner", Problem::Invalid, "no such account"));
        }
    }
    Ok((name, description, color))
}

/// Adds a value (FLD-032) right after another one, or at the end.
pub async fn add_value(
    pool: &PgPool,
    actor: &Access,
    set_id: SetId,
    input: ValueInput,
    after: Option<ValueId>,
) -> Result<SetRow> {
    let mut tx = pool.begin().await?;
    let set = lock_editable(&mut tx, actor, set_id).await?;
    let kind = kind_of(&set)?;
    if kind == SetKind::User {
        return Err(Error::Conflict("a user set lists accounts and groups"));
    }
    if input.name.is_none() {
        return Err(Error::field("name", Problem::Required, "name is required"));
    }
    let (name, description, color) = check_value(&mut tx, kind, &input).await?;
    let count = value_sets::value_count(&mut *tx, set_id).await?;
    if usize::try_from(count).unwrap_or(usize::MAX) >= VALUES_MAX {
        return Err(Error::field(
            "name",
            Problem::TooLong,
            format!("a set holds at most {VALUES_MAX} values"),
        ));
    }
    let rank = rank_for(&mut tx, set_id, after).await?;
    let name = name.unwrap_or_default();
    let id = value_sets::insert_value(
        &mut tx,
        set_id,
        &NewValue {
            name: &name,
            description: description.as_deref().unwrap_or_default(),
            color: color.flatten(),
            rank: &rank,
            resolved: input.resolved.unwrap_or(false),
            released: input.released.unwrap_or(false),
            release_date: input.release_date.flatten(),
            start_date: input.start_date.flatten(),
            assembled_on: input.assembled_on.flatten(),
            owner_id: input.owner.flatten(),
        },
    )
    .await
    .map_err(value_name_taken)?;
    if input.archived == Some(true) {
        value_sets::update_value(
            &mut tx,
            id,
            &ValueChange {
                archived: Some(true),
                ..ValueChange::default()
            },
        )
        .await?;
    }
    value_sets::touch(&mut tx, &[set_id]).await?;
    audit::record(
        &mut tx,
        actor_of(actor),
        "value_set.value_added",
        &target(set_id),
        json!({ "value": id, "name": name }),
    )
    .await?;
    tx.commit().await?;
    get(pool, actor, set_id).await
}

/// The rank right after a value of the set, or after the last one.
async fn rank_for(conn: &mut PgConnection, set: SetId, after: Option<ValueId>) -> Result<String> {
    match after {
        Some(after) => {
            let previous = value_sets::get_values(&mut *conn, &[after])
                .await?
                .pop()
                .filter(|v| v.set_id == set)
                .ok_or_else(|| Error::field("after", Problem::Invalid, "no such value"))?;
            let next = value_sets::rank_after(&mut *conn, set, &previous.rank).await?;
            rank::between(Some(&previous.rank), next.as_deref()).map_err(internal)
        }
        None => {
            let (_, last) = value_sets::rank_bounds(&mut *conn, set).await?;
            rank::between(last.as_deref(), None).map_err(internal)
        }
    }
}

async fn lock_value_of(conn: &mut PgConnection, set: SetId, id: ValueId) -> Result<Value> {
    value_sets::lock_value(conn, id)
        .await?
        .filter(|v| v.set_id == set)
        .ok_or(Error::NotFound("no such value"))
}

/// Changes a value (FLD-032, FLD-043…FLD-049): name, description, color, archived, resolved,
/// release, assembly date, owner.
pub async fn update_value(
    pool: &PgPool,
    actor: &Access,
    set_id: SetId,
    id: ValueId,
    input: ValueInput,
    version: i64,
) -> Result<SetRow> {
    let mut tx = pool.begin().await?;
    let set = lock_editable(&mut tx, actor, set_id).await?;
    let kind = kind_of(&set)?;
    let value = lock_value_of(&mut tx, set_id, id).await?;
    if value.version != version {
        return Err(Error::Conflict("the value was changed by someone else"));
    }
    let (name, description, color) = check_value(&mut tx, kind, &input).await?;
    value_sets::update_value(
        &mut tx,
        id,
        &ValueChange {
            name: name.as_deref(),
            description: description.as_deref(),
            color,
            archived: input.archived,
            resolved: input.resolved,
            released: input.released,
            release_date: input.release_date,
            start_date: input.start_date,
            assembled_on: input.assembled_on,
            owner_id: input.owner,
            rank: None,
        },
    )
    .await
    .map_err(value_name_taken)?;
    if input.archived == Some(true) {
        // An archived value is offered nowhere, so it is no default either (FLD-045).
        fields::forget_default_value(&mut tx, id).await?;
    }
    if let Some(resolved) = input.resolved
        && resolved != value.resolved
    {
        // Issues holding it may become resolved or reopen (ISS-023).
        enqueue_work(
            &mut tx,
            actor,
            FieldWork::ResolveValue {
                value: id,
                actor: actor.user(),
            },
        )
        .await?;
    }
    value_sets::touch(&mut tx, &[set_id]).await?;
    audit::record(
        &mut tx,
        actor_of(actor),
        "value_set.value_changed",
        &target(set_id),
        json!({
            "value": id,
            "name": name.as_ref().map(|new| json!({ "old": value.name, "new": new })),
            "archived": input.archived,
            "resolved": input.resolved,
            "released": input.released,
        }),
    )
    .await?;
    tx.commit().await?;
    get(pool, actor, set_id).await
}

/// Moves a value right after another one, or first (FLD-037).
pub async fn move_value(
    pool: &PgPool,
    actor: &Access,
    set_id: SetId,
    id: ValueId,
    after: Option<ValueId>,
) -> Result<SetRow> {
    let mut tx = pool.begin().await?;
    lock_editable(&mut tx, actor, set_id).await?;
    lock_value_of(&mut tx, set_id, id).await?;
    if after == Some(id) {
        return Err(Error::field(
            "after",
            Problem::Invalid,
            "a value cannot follow itself",
        ));
    }
    let rank = match after {
        Some(_) => rank_for(&mut tx, set_id, after).await?,
        None => {
            let (first, _) = value_sets::rank_bounds(&mut *tx, set_id).await?;
            rank::between(None, first.as_deref()).map_err(internal)?
        }
    };
    value_sets::update_value(
        &mut tx,
        id,
        &ValueChange {
            rank: Some(&rank),
            ..ValueChange::default()
        },
    )
    .await?;
    value_sets::touch(&mut tx, &[set_id]).await?;
    tx.commit().await?;
    get(pool, actor, set_id).await
}

/// How many issues hold a value (FLD-032): shown before deleting it.
pub async fn value_usage(pool: &PgPool, actor: &Access, set_id: SetId, id: ValueId) -> Result<i64> {
    let mut tx = pool.begin().await?;
    lock_editable(&mut tx, actor, set_id).await?;
    lock_value_of(&mut tx, set_id, id).await?;
    Ok(taskmaster_store::issue_values::issues_with_value(&mut *tx, id).await?)
}

/// Deletes a value (FLD-032); it stops being anyone's default. Issues holding it get the
/// `replacement` — another value of the same set — or lose it, in the background with a record in
/// their history (D-029).
pub async fn delete_value(
    pool: &PgPool,
    actor: &Access,
    set_id: SetId,
    id: ValueId,
    replacement: Option<ValueId>,
) -> Result<SetRow> {
    let mut tx = pool.begin().await?;
    lock_editable(&mut tx, actor, set_id).await?;
    let value = lock_value_of(&mut tx, set_id, id).await?;
    if let Some(other) = replacement {
        if other == id {
            return Err(Error::field(
                "replacement",
                Problem::Invalid,
                "a value cannot replace itself",
            ));
        }
        lock_value_of(&mut tx, set_id, other).await.map_err(|_| {
            Error::field("replacement", Problem::Invalid, "not a value of this set")
        })?;
    }
    fields::forget_default_value(&mut tx, id).await?;
    value_sets::delete_value(&mut tx, id).await?;
    value_sets::touch(&mut tx, &[set_id]).await?;
    if taskmaster_store::issue_values::issues_with_value(&mut *tx, id).await? > 0 {
        enqueue_work(
            &mut tx,
            actor,
            FieldWork::Replace {
                value: id,
                name: value.name.clone(),
                replacement,
                actor: actor.user(),
            },
        )
        .await?;
    }
    audit::record(
        &mut tx,
        actor_of(actor),
        "value_set.value_deleted",
        &target(set_id),
        json!({ "value": id, "name": value.name, "replacement": replacement }),
    )
    .await?;
    tx.commit().await?;
    get(pool, actor, set_id).await
}

/// Queues work on issues after a change of fields (architecture.md §12).
pub(crate) async fn enqueue_work(
    conn: &mut PgConnection,
    actor: &Access,
    work: FieldWork,
) -> Result<()> {
    taskmaster_store::jobs::enqueue(
        conn,
        &work,
        taskmaster_store::jobs::Enqueue {
            initiator: actor_of(actor),
            ..taskmaster_store::jobs::Enqueue::default()
        },
    )
    .await?;
    Ok(())
}

/// Adds accounts and groups to a user set (FLD-040); groups bring all their current members.
pub async fn add_members(
    pool: &PgPool,
    actor: &Access,
    set_id: SetId,
    user_ids: &[UserId],
    group_ids: &[GroupId],
) -> Result<SetRow> {
    change_members(pool, actor, set_id, user_ids, group_ids, true).await
}

/// Removes accounts and groups from a user set (FLD-040).
pub async fn remove_members(
    pool: &PgPool,
    actor: &Access,
    set_id: SetId,
    user_ids: &[UserId],
    group_ids: &[GroupId],
) -> Result<SetRow> {
    change_members(pool, actor, set_id, user_ids, group_ids, false).await
}

async fn change_members(
    pool: &PgPool,
    actor: &Access,
    set_id: SetId,
    user_ids: &[UserId],
    group_ids: &[GroupId],
    add: bool,
) -> Result<SetRow> {
    // Picking accounts needs the directory (FLD-040: user-read and project update).
    actor.require(Permission::UserDirectory)?;
    let mut tx = pool.begin().await?;
    let set = lock_editable(&mut tx, actor, set_id).await?;
    if kind_of(&set)? != SetKind::User {
        return Err(Error::Conflict("only user sets list accounts and groups"));
    }
    let mut user_ids = user_ids.to_vec();
    user_ids.sort_unstable();
    user_ids.dedup();
    let mut group_ids = group_ids.to_vec();
    group_ids.sort_unstable();
    group_ids.dedup();
    let (changed_users, changed_groups) = if add {
        let found = users::get_many(&mut *tx, &user_ids).await?;
        if found.len() != user_ids.len() || found.iter().any(|u| u.is_deleted() || u.is_guest()) {
            return Err(Error::NotFound("no such account"));
        }
        if groups::get_many(&mut *tx, &group_ids).await?.len() != group_ids.len() {
            return Err(Error::NotFound("no such group"));
        }
        let (users_now, groups_now) = value_sets::member_counts(&mut *tx, set_id).await?;
        let too_many = |now: i64, more: usize, max: usize| {
            usize::try_from(now)
                .unwrap_or(usize::MAX)
                .saturating_add(more)
                > max
        };
        if too_many(users_now, user_ids.len(), SET_USERS_MAX)
            || too_many(groups_now, group_ids.len(), SET_GROUPS_MAX)
        {
            return Err(Error::field(
                "users",
                Problem::TooLong,
                format!("a set lists at most {SET_USERS_MAX} accounts and {SET_GROUPS_MAX} groups"),
            ));
        }
        (
            value_sets::add_users(&mut tx, set_id, &user_ids).await?,
            value_sets::add_groups(&mut tx, set_id, &group_ids).await?,
        )
    } else {
        (
            value_sets::remove_users(&mut tx, set_id, &user_ids).await?,
            value_sets::remove_groups(&mut tx, set_id, &group_ids).await?,
        )
    };
    if !changed_users.is_empty() || !changed_groups.is_empty() {
        if !add {
            // Accounts that are no longer offered stop being defaults.
            fields::forget_default_users_of_set(&mut tx, set_id, &changed_users).await?;
        }
        value_sets::touch(&mut tx, &[set_id]).await?;
        audit::record(
            &mut tx,
            actor_of(actor),
            if add {
                "value_set.members_added"
            } else {
                "value_set.members_removed"
            },
            &target(set_id),
            json!({ "users": changed_users, "groups": changed_groups }),
        )
        .await?;
    }
    tx.commit().await?;
    get(pool, actor, set_id).await
}

/// A project's own copy of a set (FLD-030): the same values (or accounts and groups), in the same
/// order. Returns the new set and the copies of `defaults`.
pub(crate) async fn fork(
    conn: &mut PgConnection,
    set: &ValueSet,
    project_name: &str,
    defaults: &[i64],
) -> Result<(SetId, Vec<i64>, Vec<(ValueId, ValueId)>)> {
    let kind = kind_of(set)?;
    let base: String = format!("{project_name}: {}", set.name)
        .chars()
        .take(taskmaster_domain::fields::SET_NAME_MAX - 6)
        .collect();
    let name = crate::seed::free_set_name(conn, kind.key(), &base).await?;
    let id = value_sets::insert(conn, &name, kind.key()).await?;
    if kind == SetKind::User {
        value_sets::copy_members(conn, set.id, id).await?;
        return Ok((id, defaults.to_vec(), Vec::new()));
    }
    let pairs = value_sets::copy_values(conn, set.id, id).await?;
    let copied = defaults
        .iter()
        .filter_map(|old| pairs.iter().find(|(o, _)| o == old).map(|(_, new)| *new))
        .collect();
    Ok((id, copied, pairs))
}

fn set_name_taken(error: sqlx::Error) -> Error {
    if unique_violation(&error) == Some(value_sets::SET_NAME_KEY) {
        Error::field(
            "name",
            Problem::Taken,
            "another set of this kind has this name",
        )
    } else {
        error.into()
    }
}

fn value_name_taken(error: sqlx::Error) -> Error {
    if unique_violation(&error) == Some(value_sets::VALUE_NAME_KEY) {
        Error::field("name", Problem::Taken, "the set has a value with this name")
    } else {
        error.into()
    }
}

fn target(id: SetId) -> String {
    format!("value_set:{id}")
}

fn actor_of(access: &Access) -> taskmaster_domain::Actor {
    taskmaster_domain::Actor::User(access.user())
}
