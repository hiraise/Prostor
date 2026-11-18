//! Custom fields of the instance (FLD-015…FLD-027): the list, creating, base properties and the
//! settings new attachments start with, the instance order, deleting. Design: decisions.md D-028.

use serde_json::json;
use sqlx::PgConnection;
use taskmaster_domain::access::Access;
use taskmaster_domain::access::permission::Permission;
use taskmaster_domain::fields::{
    FieldKind, FieldType, aliases, check_defaults, check_empty_label, empty_label, field_name,
    may_edit_base, may_see_fields,
};
use taskmaster_domain::validation::{Problem, Problems};
use taskmaster_domain::{FieldId, ProjectId, SetId, rank};
use taskmaster_store::fields::{self, Field, FieldChange, NewField};
use taskmaster_store::projects::{self, Project};
use taskmaster_store::users::{self, User};
use taskmaster_store::value_sets::{self, Value, ValueSet};
use taskmaster_store::{PgPool, audit, issue_values, unique_violation};

use crate::issues::jobs::FieldWork;
use crate::projects::can_see;
use crate::seed::{free_set_name, internal};
use crate::{Error, Result};

const SEARCH_MAX: usize = 100;

/// A field with its set, default values and how many projects use it.
#[derive(Debug, Clone)]
pub struct FieldRow {
    pub field: Field,
    pub set: Option<ValueSet>,
    pub project_count: i64,
    pub default_values: Vec<Value>,
    pub default_users: Vec<User>,
}

impl FieldRow {
    pub fn kind(&self) -> Option<FieldKind> {
        FieldKind::parse(&self.field.kind)
    }
}

/// Default values of fields, loaded in batches: values, or accounts for user fields.
pub(crate) async fn load_defaults(
    conn: &mut PgConnection,
    lists: &[(&str, &[i64])],
) -> Result<(Vec<Value>, Vec<User>)> {
    let mut value_ids = Vec::new();
    let mut user_ids = Vec::new();
    for (kind, ids) in lists {
        if *kind == FieldKind::User.key() {
            user_ids.extend_from_slice(ids);
        } else {
            value_ids.extend_from_slice(ids);
        }
    }
    let values = value_sets::get_values(&mut *conn, &value_ids).await?;
    let users = users::get_many(&mut *conn, &user_ids).await?;
    Ok((values, users))
}

pub(crate) fn pick<T: Clone>(all: &[T], ids: &[i64], id_of: impl Fn(&T) -> i64) -> Vec<T> {
    ids.iter()
        .filter_map(|id| all.iter().find(|item| id_of(item) == *id).cloned())
        .collect()
}

async fn rows(conn: &mut PgConnection, list: Vec<Field>) -> Result<Vec<FieldRow>> {
    let counts = fields::project_counts(&mut *conn).await?;
    let set_ids: Vec<SetId> = list.iter().filter_map(|f| f.set_id).collect();
    let sets = value_sets::get_many(&mut *conn, &set_ids).await?;
    let lists: Vec<(&str, &[i64])> = list
        .iter()
        .map(|f| (f.kind.as_str(), f.default_values.as_slice()))
        .collect();
    let (values, people) = load_defaults(conn, &lists).await?;
    Ok(list
        .into_iter()
        .map(|field| {
            let user = field.kind == FieldKind::User.key();
            FieldRow {
                set: field
                    .set_id
                    .and_then(|id| sets.iter().find(|s| s.id == id).cloned()),
                project_count: counts
                    .iter()
                    .find(|(id, _)| *id == field.id)
                    .map_or(0, |(_, count)| *count),
                default_values: if user {
                    Vec::new()
                } else {
                    pick(&values, &field.default_values, |v| v.id)
                },
                default_users: if user {
                    pick(&people, &field.default_values, |u| u.id)
                } else {
                    Vec::new()
                },
                field,
            }
        })
        .collect())
}

/// Fields in the instance order (FLD-022, FLD-025) whose name or an alias contains `text`;
/// `unused` keeps only fields no project uses (or only used ones with `Some(false)`). Fields are
/// few, so the list is filtered and paged in memory; `after` is the previous page's last field.
pub async fn list(
    pool: &PgPool,
    viewer: &Access,
    text: &str,
    unused: Option<bool>,
    after: Option<FieldId>,
    limit: usize,
) -> Result<Vec<FieldRow>> {
    may_see_fields(viewer)?;
    let text: String = text
        .trim()
        .to_lowercase()
        .chars()
        .take(SEARCH_MAX)
        .collect();
    let mut conn = pool.acquire().await?;
    let all = fields::all(&mut *conn).await?;
    let all = rows(&mut conn, all).await?;
    let filtered: Vec<FieldRow> = all
        .into_iter()
        .filter(|row| {
            text.is_empty()
                || row.field.name.to_lowercase().contains(&text)
                || row
                    .field
                    .aliases
                    .iter()
                    .any(|alias| alias.to_lowercase().contains(&text))
        })
        .filter(|row| unused.is_none_or(|unused| (row.project_count == 0) == unused))
        .collect();
    let start = after
        .and_then(|after| filtered.iter().position(|row| row.field.id == after))
        .map_or(0, |index| index + 1);
    Ok(filtered.into_iter().skip(start).take(limit).collect())
}

/// One field.
pub async fn get(pool: &PgPool, viewer: &Access, id: FieldId) -> Result<FieldRow> {
    may_see_fields(viewer)?;
    let mut conn = pool.acquire().await?;
    let field = fields::get(&mut *conn, id)
        .await?
        .ok_or(Error::NotFound("no such field"))?;
    rows(&mut conn, vec![field])
        .await?
        .pop()
        .ok_or(Error::NotFound("no such field"))
}

/// The projects using a field that the viewer may see, and how many others there are (FLD-025).
pub async fn projects_of(
    pool: &PgPool,
    viewer: &Access,
    id: FieldId,
) -> Result<(Vec<Project>, i64)> {
    may_see_fields(viewer)?;
    let mut conn = pool.acquire().await?;
    fields::get(&mut *conn, id)
        .await?
        .ok_or(Error::NotFound("no such field"))?;
    let ids = fields::projects_of(&mut *conn, id).await?;
    let mut found = projects::get_many(&mut *conn, &ids).await?;
    let total = i64::try_from(found.len()).unwrap_or(i64::MAX);
    found.retain(|project| can_see(viewer, project));
    found.sort_by_key(|project| project.name.to_lowercase());
    let hidden = total - i64::try_from(found.len()).unwrap_or(i64::MAX);
    Ok((found, hidden))
}

/// Where a new field takes its values from (FLD-015).
#[derive(Debug, Clone)]
pub enum SetSource {
    /// A new empty set; named after the field when no name is given.
    New { name: Option<String> },
    /// The set of an existing field or any set of the same kind.
    Existing(SetId),
}

/// A field to create.
#[derive(Debug, Clone)]
pub struct NewFieldInput {
    pub name: String,
    pub kind: FieldKind,
    pub multi: bool,
    pub aliases: Vec<String>,
    /// For kinds with values; ignored otherwise.
    pub set: SetSource,
    pub can_be_empty: bool,
    pub empty_label: String,
    pub private: bool,
    /// Only with an existing set (FLD-015): a new set has no values yet.
    pub default_values: Vec<i64>,
    pub auto_attach: bool,
}

/// Checks default values against a field's set: values of the set that are not archived, or
/// accounts the user set offers.
pub(crate) async fn check_default_values(
    conn: &mut PgConnection,
    field: FieldType,
    set: Option<SetId>,
    defaults: &[i64],
) -> Result<Vec<i64>> {
    let mut ids = Vec::new();
    for id in defaults {
        if !ids.contains(id) {
            ids.push(*id);
        }
    }
    check_defaults(field, ids.len()).map_err(|r| Error::rejected("default_values", r))?;
    if ids.is_empty() {
        return Ok(ids);
    }
    let Some(set) = set else {
        return Err(Error::field(
            "default_values",
            Problem::Invalid,
            "this field has no values",
        ));
    };
    if field.kind == FieldKind::User {
        for user in &ids {
            if !value_sets::user_allowed(&mut *conn, set, *user).await? {
                return Err(Error::field(
                    "default_values",
                    Problem::Invalid,
                    "a default account is not in the field's set",
                ));
            }
        }
    } else {
        let found = value_sets::get_values(&mut *conn, &ids).await?;
        if found.len() != ids.len() || found.iter().any(|v| v.set_id != set || v.archived) {
            return Err(Error::field(
                "default_values",
                Problem::Invalid,
                "a default value is not in the field's set",
            ));
        }
    }
    Ok(ids)
}

/// Creates a field of the instance (FLD-015). Needs `system.write`; a project administrator
/// creates fields from their project (FLD-052).
pub async fn create(pool: &PgPool, actor: &Access, input: NewFieldInput) -> Result<FieldRow> {
    actor.require(Permission::SystemWrite)?;
    let mut tx = pool.begin().await?;
    let id = insert(&mut tx, &input, false).await?;
    audit::record(
        &mut tx,
        actor_of(actor),
        "field.created",
        &target(id),
        json!({ "name": input.name.trim(), "kind": input.kind.key(), "multi": input.multi }),
    )
    .await?;
    tx.commit().await?;
    get(pool, actor, id).await
}

/// Validates and stores a new field at the end of the instance order. `in_project` leaves the set
/// to the caller (it is the project's set) and refuses defaults.
pub(crate) async fn insert(
    conn: &mut PgConnection,
    input: &NewFieldInput,
    in_project: bool,
) -> Result<FieldId> {
    let mut problems = Problems::new();
    let name = problems.check("name", field_name(&input.name));
    let field_type = problems.check("multi", FieldType::new(input.kind, input.multi));
    let alias_list = name
        .as_deref()
        .and_then(|name| problems.check("aliases", aliases(&input.aliases, name)));
    let label = problems.check("empty_label", empty_label(&input.empty_label));
    if let Some(label) = &label {
        problems.check("empty_label", check_empty_label(input.can_be_empty, label));
    }
    problems.finish().map_err(Error::Invalid)?;
    let (Some(name), Some(field_type), Some(alias_list), Some(label)) =
        (name, field_type, alias_list, label)
    else {
        return Err(Error::Internal("validated values are missing".to_owned()));
    };
    if fields::find(&mut *conn, &name, input.kind.key(), input.multi)
        .await?
        .is_some()
    {
        return Err(Error::field(
            "name",
            Problem::Taken,
            "a field of this type has this name",
        ));
    }
    let set = match (input.kind.set_kind(), &input.set) {
        (None, _) => None,
        (Some(kind), SetSource::Existing(id)) => {
            let set = value_sets::get(&mut *conn, *id)
                .await?
                .ok_or_else(|| Error::field("set", Problem::Invalid, "no such value set"))?;
            if set.kind != kind.key() {
                return Err(Error::field(
                    "set",
                    Problem::Invalid,
                    "the value set holds another kind of values",
                ));
            }
            Some(set.id)
        }
        (Some(kind), SetSource::New { name: set_name }) => {
            let base = match set_name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
                Some(raw) => taskmaster_domain::fields::set_name(raw)
                    .map_err(|r| Error::rejected("set.name", r))?,
                None => name.clone(),
            };
            let free = free_set_name(conn, kind.key(), &base).await?;
            Some(value_sets::insert(conn, &free, kind.key()).await?)
        }
    };
    let defaults = if in_project {
        Vec::new()
    } else {
        check_default_values(conn, field_type, set, &input.default_values).await?
    };
    let last = fields::last_rank(&mut *conn).await?;
    let rank = rank::between(last.as_deref(), None).map_err(internal)?;
    let id = fields::insert(
        conn,
        &NewField {
            name: &name,
            kind: input.kind.key(),
            multi: input.multi,
            aliases: &alias_list,
            builtin: None,
            auto_attach: input.auto_attach,
            rank: &rank,
            set_id: set,
            can_be_empty: input.can_be_empty,
            empty_label: &label,
            private: input.private,
            default_values: &defaults,
        },
    )
    .await
    .map_err(name_taken)?;
    Ok(id)
}

/// Changes of a field; `None` keeps a value.
#[derive(Debug, Clone, Default)]
pub struct FieldUpdate {
    pub name: Option<String>,
    pub aliases: Option<Vec<String>>,
    pub auto_attach: Option<bool>,
    pub can_be_empty: Option<bool>,
    pub empty_label: Option<String>,
    pub private: Option<bool>,
    pub default_values: Option<Vec<i64>>,
    pub version: i64,
}

/// Changes a field (FLD-016…FLD-020). Name and aliases are base properties shared by every
/// project: `system.write`, or `project.update` in every project using a field that is not
/// attached automatically (FLD-018). Auto-attaching and the settings of new attachments are
/// instance settings: `system.write`.
pub async fn update(
    pool: &PgPool,
    actor: &Access,
    id: FieldId,
    change: FieldUpdate,
) -> Result<FieldRow> {
    let mut tx = pool.begin().await?;
    let field = fields::lock(&mut tx, id)
        .await?
        .ok_or(Error::NotFound("no such field"))?;
    let field_type = FieldKind::parse(&field.kind)
        .map(|kind| FieldType {
            kind,
            multi: field.multi,
        })
        .ok_or_else(|| Error::Internal("unknown field kind".to_owned()))?;
    if field.version != change.version {
        return Err(Error::Conflict("the field was changed by someone else"));
    }
    if change.name.is_some() || change.aliases.is_some() {
        let used_in = fields::projects_of(&mut *tx, id).await?;
        may_edit_base(actor, field.auto_attach, &used_in)?;
    }
    if change.auto_attach.is_some()
        || change.can_be_empty.is_some()
        || change.empty_label.is_some()
        || change.private.is_some()
        || change.default_values.is_some()
    {
        actor.require(Permission::SystemWrite)?;
    }
    let mut problems = Problems::new();
    let name = change
        .name
        .as_deref()
        .and_then(|raw| problems.check("name", field_name(raw)));
    let alias_list = change.aliases.as_ref().and_then(|raw| {
        problems.check(
            "aliases",
            aliases(raw, name.as_deref().unwrap_or(&field.name)),
        )
    });
    let label = change
        .empty_label
        .as_deref()
        .and_then(|raw| problems.check("empty_label", empty_label(raw)));
    let can_be_empty = change.can_be_empty.unwrap_or(field.can_be_empty);
    let resulting_label = match &label {
        Some(label) => label.clone(),
        None if !can_be_empty => String::new(),
        None => field.empty_label.clone(),
    };
    problems.check(
        "empty_label",
        check_empty_label(can_be_empty, &resulting_label),
    );
    problems.finish().map_err(Error::Invalid)?;
    let defaults = match &change.default_values {
        Some(list) => Some(check_default_values(&mut tx, field_type, field.set_id, list).await?),
        None => None,
    };
    let label_change = (resulting_label != field.empty_label).then_some(resulting_label.as_str());
    fields::update(
        &mut tx,
        id,
        &FieldChange {
            name: name.as_deref(),
            aliases: alias_list.as_deref(),
            auto_attach: change.auto_attach,
            can_be_empty: change.can_be_empty,
            empty_label: label_change,
            private: change.private,
            default_values: defaults.as_deref(),
            rank: None,
        },
    )
    .await
    .map_err(name_taken)?;
    audit::record(
        &mut tx,
        actor_of(actor),
        "field.updated",
        &target(id),
        json!({
            "name": name.as_ref().map(|new| json!({ "old": field.name, "new": new })),
            "aliases": alias_list,
            "auto_attach": change.auto_attach,
            "can_be_empty": change.can_be_empty,
            "empty_label": label_change,
            "private": change.private,
            "default_values": defaults,
        }),
    )
    .await?;
    tx.commit().await?;
    get(pool, actor, id).await
}

/// Moves a field in the instance order (FLD-022) to right after another one, or first. Needs
/// `system.write`.
pub async fn move_after(
    pool: &PgPool,
    actor: &Access,
    id: FieldId,
    after: Option<FieldId>,
) -> Result<FieldRow> {
    actor.require(Permission::SystemWrite)?;
    let mut tx = pool.begin().await?;
    fields::lock(&mut tx, id)
        .await?
        .ok_or(Error::NotFound("no such field"))?;
    let rank = match after {
        Some(after) if after == id => {
            return Err(Error::field(
                "after",
                Problem::Invalid,
                "a field cannot follow itself",
            ));
        }
        Some(after) => {
            let previous = fields::get(&mut *tx, after)
                .await?
                .ok_or_else(|| Error::field("after", Problem::Invalid, "no such field"))?;
            let next = fields::rank_after(&mut *tx, &previous.rank).await?;
            rank::between(Some(&previous.rank), next.as_deref()).map_err(internal)?
        }
        None => {
            let first = fields::first_rank(&mut *tx).await?;
            rank::between(None, first.as_deref()).map_err(internal)?
        }
    };
    fields::update(
        &mut tx,
        id,
        &FieldChange {
            rank: Some(&rank),
            ..FieldChange::default()
        },
    )
    .await?;
    tx.commit().await?;
    get(pool, actor, id).await
}

/// What deleting a field takes away (FLD-023): the projects that use it, the issues holding a
/// value of it and the boards using it (BRD-022).
pub async fn deletion_impact(
    pool: &PgPool,
    actor: &Access,
    id: FieldId,
) -> Result<(i64, i64, i64)> {
    actor.require(Permission::SystemWrite)?;
    let mut conn = pool.acquire().await?;
    let field = fields::get(&mut *conn, id)
        .await?
        .ok_or(Error::NotFound("no such field"))?;
    let projects =
        i64::try_from(fields::projects_of(&mut *conn, id).await?.len()).unwrap_or(i64::MAX);
    let issues = match FieldKind::parse(&field.kind) {
        Some(kind) => issue_values::issues_with_field(&mut conn, id, kind, None).await?,
        None => 0,
    };
    let boards = crate::boards::health::field_boards(&mut conn, id).await?;
    Ok((projects, issues, boards))
}

/// Deletes a field from every project and the instance (FLD-023). Value sets that nobody else
/// uses go with it; its values on issues are erased in the background and are never shown again
/// meanwhile, since no project has the field. Needs `system.write`.
pub async fn delete(pool: &PgPool, actor: &Access, id: FieldId) -> Result<()> {
    actor.require(Permission::SystemWrite)?;
    let mut tx = pool.begin().await?;
    let field = fields::lock(&mut tx, id)
        .await?
        .ok_or(Error::NotFound("no such field"))?;
    let attachments = fields::attachments_of(&mut *tx, id).await?;
    let mut sets: Vec<SetId> = attachments.iter().filter_map(|a| a.set_id).collect();
    sets.extend(field.set_id);
    let used_in: Vec<ProjectId> = attachments.iter().map(|a| a.project_id).collect();
    taskmaster_store::boards::remove_field(&mut tx, id).await?;
    fields::delete(&mut tx, id).await?;
    value_sets::delete_unused(&mut tx, &sets).await?;
    projects::touch(&mut tx, &used_in).await?;
    audit::record(
        &mut tx,
        actor_of(actor),
        "field.deleted",
        &target(id),
        json!({ "name": field.name, "kind": field.kind, "projects": used_in }),
    )
    .await?;
    if !used_in.is_empty() {
        crate::value_sets::enqueue_work(
            &mut tx,
            actor,
            FieldWork::Erase {
                field: id,
                kind: field.kind.clone(),
            },
        )
        .await?;
        if field.kind == FieldKind::State.key() {
            for project in &used_in {
                crate::value_sets::enqueue_work(
                    &mut tx,
                    actor,
                    FieldWork::ResolveProject {
                        project: *project,
                        actor: actor.user(),
                    },
                )
                .await?;
            }
        }
    }
    tx.commit().await?;
    Ok(())
}

pub(crate) fn name_taken(error: sqlx::Error) -> Error {
    if unique_violation(&error) == Some(fields::NAME_KEY) {
        Error::field("name", Problem::Taken, "a field of this type has this name")
    } else {
        error.into()
    }
}

fn target(id: FieldId) -> String {
    format!("field:{id}")
}

fn actor_of(access: &Access) -> taskmaster_domain::Actor {
    taskmaster_domain::Actor::User(access.user())
}

/// The fields of the projects the viewer reads issues in, or of `project` alone (QRY-078), with
/// how many of those projects use each, most used first. Private attachments count only where
/// the viewer reads private fields (FLD-062). The list's column chooser and default columns.
pub async fn of_readable(
    pool: &PgPool,
    viewer: &Access,
    project: Option<ProjectId>,
) -> Result<Vec<(Field, i64)>> {
    use taskmaster_domain::access::Projects;
    let mut conn = pool.acquire().await?;
    if let Some(id) = project {
        let found = projects::get(&mut *conn, id).await?;
        if !found.is_some_and(|p| can_see(viewer, &p) && viewer.has_in(Permission::IssueRead, id)) {
            return Err(Error::NotFound("no such project"));
        }
    }
    let read = viewer.projects_with(Permission::IssueRead);
    if read == Projects::Only(Vec::new()) {
        return Ok(Vec::new());
    }
    let private = viewer.projects_with(Permission::IssuePrivateRead);
    let ids = |p: &Projects| match p {
        Projects::All => None,
        Projects::Only(ids) => Some(ids.clone()),
    };
    let (read, private) = (ids(&read), ids(&private));
    Ok(
        fields::of_readable_projects(&mut *conn, read.as_deref(), private.as_deref(), project)
            .await?,
    )
}
