//! Fields of a project (FLD-051…FLD-060): attaching existing or new fields, their settings in the
//! project, the project's order, forking a shared value set, detaching. Needs `project.update`
//! in the project; the project must not be archived.

use serde_json::json;
use sqlx::PgConnection;
use taskmaster_domain::access::Access;
use taskmaster_domain::access::permission::Permission;
use taskmaster_domain::fields::{
    FieldKind, FieldType, PROJECT_FIELDS_MAX, check_empty_label, empty_label, required,
};
use taskmaster_domain::validation::{Problem, Problems};
use taskmaster_domain::{FieldId, ProjectId, SetId, rank};
use taskmaster_store::fields::{self, Attached, AttachedChange, Attachment, Field};
use taskmaster_store::projects::{self, Project};
use taskmaster_store::users::User;
use taskmaster_store::value_sets::{self, Value, ValueSet};
use taskmaster_store::{PgPool, audit, issue_values, teams};

use crate::fields::{NewFieldInput, SetSource, check_default_values, load_defaults, pick};
use crate::issues::jobs::FieldWork;
use crate::projects::{actor_of, lock_visible, require, require_active, target, visible};
use crate::seed::{internal, project_set};
use crate::{Error, Result};

/// Values erased per statement when a field leaves a project.
const ERASE_BATCH: i64 = 10_000;

/// A field of a project with its settings there.
#[derive(Debug, Clone)]
pub struct ProjectFieldRow {
    pub attached: Attached,
    pub field: Field,
    pub set: Option<ValueSet>,
    /// Other projects using the same set.
    pub shared_with: i64,
    /// The set is offered to new attachments of some field.
    pub field_default: bool,
    pub default_values: Vec<Value>,
    pub default_users: Vec<User>,
    /// Must be filled on new issues (FLD-057).
    pub required: bool,
}

async fn rows(
    conn: &mut PgConnection,
    project: ProjectId,
    list: Vec<Attached>,
) -> Result<Vec<ProjectFieldRow>> {
    let field_ids: Vec<FieldId> = list.iter().map(|a| a.field_id).collect();
    let found = fields::get_many(&mut *conn, &field_ids).await?;
    let set_ids: Vec<SetId> = list.iter().filter_map(|a| a.set_id).collect();
    let sets = value_sets::get_many(&mut *conn, &set_ids).await?;
    let usage = value_sets::usage_counts(&mut *conn, &set_ids).await?;
    let lists: Vec<(&str, &[i64])> = list
        .iter()
        .filter_map(|a| {
            let field = found.iter().find(|f| f.id == a.field_id)?;
            Some((field.kind.as_str(), a.default_values.as_slice()))
        })
        .collect();
    let (values, people) = load_defaults(conn, &lists).await?;
    Ok(list
        .into_iter()
        .filter_map(|attached| {
            let field = found.iter().find(|f| f.id == attached.field_id)?.clone();
            let set = attached
                .set_id
                .and_then(|id| sets.iter().find(|s| s.id == id).cloned());
            let (projects_using, fields_using) = attached
                .set_id
                .and_then(|id| usage.iter().find(|(set, _, _)| *set == id))
                .map_or((0, 0), |(_, p, f)| (*p, *f));
            let user = field.kind == FieldKind::User.key();
            Some(ProjectFieldRow {
                shared_with: (projects_using - 1).max(0),
                field_default: fields_using > 0,
                default_values: if user {
                    Vec::new()
                } else {
                    pick(&values, &attached.default_values, |v| v.id)
                },
                default_users: if user {
                    pick(&people, &attached.default_values, |u| u.id)
                } else {
                    Vec::new()
                },
                required: required(attached.can_be_empty, !attached.default_values.is_empty()),
                field,
                set,
                attached,
            })
        })
        .filter(|row| row.attached.project_id == project)
        .collect())
}

/// The fields of a project in its order (FLD-051, FLD-058). Needs `project.update`.
pub async fn list(
    pool: &PgPool,
    viewer: &Access,
    project: ProjectId,
) -> Result<Vec<ProjectFieldRow>> {
    let mut conn = pool.acquire().await?;
    visible(&mut conn, viewer, project).await?;
    require(viewer, Permission::ProjectUpdate, project)?;
    let attached = fields::of_project(&mut *conn, project).await?;
    rows(&mut conn, project, attached).await
}

async fn one(pool: &PgPool, project: ProjectId, field: FieldId) -> Result<ProjectFieldRow> {
    let mut conn = pool.acquire().await?;
    let attached = fields::of_project(&mut *conn, project)
        .await?
        .into_iter()
        .filter(|a| a.field_id == field)
        .collect();
    rows(&mut conn, project, attached)
        .await?
        .pop()
        .ok_or(Error::NotFound("the field is not in the project"))
}

/// Locks a project for a change of its fields.
async fn lock_project(conn: &mut PgConnection, actor: &Access, id: ProjectId) -> Result<Project> {
    let project = lock_visible(conn, actor, id).await?;
    require(actor, Permission::ProjectUpdate, id)?;
    require_active(&project)?;
    Ok(project)
}

/// How a project takes a field's values when attaching it (FLD-030, FLD-053).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetUse {
    /// The field's set, shared with every project sharing it.
    Shared,
    /// A copy of it, owned by the project.
    Copy,
}

/// What to attach.
#[derive(Debug, Clone)]
pub enum AttachInput {
    /// An existing field (FLD-053).
    Existing { field: FieldId, set: SetUse },
    /// A new field, created for this project (FLD-052): its values are the project's own set.
    New {
        name: String,
        kind: FieldKind,
        multi: bool,
        aliases: Vec<String>,
    },
}

/// Attaches a field (FLD-052, FLD-053) at the end of the project's order. Its settings start from
/// the field's defaults (FLD-019). A project holds one field per name (D-028).
pub async fn attach(
    pool: &PgPool,
    actor: &Access,
    project_id: ProjectId,
    input: AttachInput,
) -> Result<ProjectFieldRow> {
    let mut tx = pool.begin().await?;
    let project = lock_project(&mut tx, actor, project_id).await?;
    let present = fields::of_project(&mut *tx, project_id).await?;
    if present.len() >= PROJECT_FIELDS_MAX {
        return Err(Error::Conflict(
            "the project has the most fields it can have",
        ));
    }
    let present_fields = fields::get_many(
        &mut *tx,
        &present.iter().map(|a| a.field_id).collect::<Vec<_>>(),
    )
    .await?;
    let (field, set, defaults) = match input {
        AttachInput::Existing { field, set } => {
            let field = fields::get(&mut *tx, field)
                .await?
                .ok_or_else(|| Error::field("field", Problem::Invalid, "no such field"))?;
            let (set, defaults) = match (field.set_id, set) {
                (None, _) => (None, Vec::new()),
                (Some(id), SetUse::Shared) => (Some(id), field.default_values.clone()),
                (Some(id), SetUse::Copy) => {
                    let source = value_sets::get(&mut *tx, id)
                        .await?
                        .ok_or_else(|| Error::Internal("the field's set is gone".to_owned()))?;
                    let (copy, defaults, _) = crate::value_sets::fork(
                        &mut tx,
                        &source,
                        &project.name,
                        &field.default_values,
                    )
                    .await?;
                    (Some(copy), defaults)
                }
            };
            (field, set, defaults)
        }
        AttachInput::New {
            name,
            kind,
            multi,
            aliases,
        } => {
            // The new field's values are this project's set; a user set starts with the team.
            let set = match kind.set_kind() {
                Some(set_kind) => {
                    let own =
                        project_set(&mut tx, set_kind.key(), &project.name, name.trim()).await?;
                    if kind == FieldKind::User
                        && let Some(team) = teams::of_project(&mut *tx, project_id).await?
                    {
                        value_sets::add_groups(&mut tx, own, &[team]).await?;
                    }
                    Some(own)
                }
                None => None,
            };
            let field_id = crate::fields::insert(
                &mut tx,
                &NewFieldInput {
                    name,
                    kind,
                    multi,
                    aliases,
                    set: set.map_or(SetSource::New { name: None }, SetSource::Existing),
                    can_be_empty: true,
                    empty_label: String::new(),
                    private: false,
                    default_values: Vec::new(),
                    auto_attach: false,
                },
                true,
            )
            .await?;
            let field = fields::get(&mut *tx, field_id)
                .await?
                .ok_or_else(|| Error::Internal("the new field is gone".to_owned()))?;
            audit::record(
                &mut tx,
                actor_of(actor),
                "field.created",
                &format!("field:{field_id}"),
                json!({ "name": field.name, "kind": field.kind, "multi": field.multi, "project": project_id }),
            )
            .await?;
            (field, set, Vec::new())
        }
    };
    if present.iter().any(|a| a.field_id == field.id) {
        return Err(Error::Conflict("the field is in the project already"));
    }
    if present_fields
        .iter()
        .any(|f| f.name.to_lowercase() == field.name.to_lowercase())
    {
        return Err(Error::field(
            "field",
            Problem::Taken,
            "the project has a field with this name",
        ));
    }
    let last = fields::last_attached_rank(&mut *tx, project_id).await?;
    let rank = rank::between(last.as_deref(), None).map_err(internal)?;
    fields::attach(
        &mut tx,
        &Attachment {
            project_id,
            field_id: field.id,
            set_id: set,
            rank: &rank,
            can_be_empty: field.can_be_empty,
            empty_label: &field.empty_label,
            default_values: &defaults,
            private: field.private,
        },
    )
    .await?;
    projects::touch(&mut tx, &[project_id]).await?;
    audit::record(
        &mut tx,
        actor_of(actor),
        "project.field_attached",
        &target(project_id),
        json!({ "field": field.id, "set": set }),
    )
    .await?;
    if field.kind == FieldKind::State.key() {
        // Every state field must be resolved now (ISS-023).
        crate::value_sets::enqueue_work(
            &mut tx,
            actor,
            FieldWork::ResolveProject {
                project: project_id,
                actor: actor.user(),
            },
        )
        .await?;
    }
    tx.commit().await?;
    one(pool, project_id, field.id).await
}

/// Changes of a field's settings in a project; `None` keeps one.
#[derive(Debug, Clone, Default)]
pub struct SettingsChange {
    pub can_be_empty: Option<bool>,
    pub empty_label: Option<String>,
    pub default_values: Option<Vec<i64>>,
    pub private: Option<bool>,
    pub version: i64,
}

/// Changes empty handling, defaults and privacy of a field in a project (FLD-055, FLD-056,
/// FLD-060). Defaults apply to new issues; existing ones keep their values.
pub async fn update(
    pool: &PgPool,
    actor: &Access,
    project_id: ProjectId,
    field_id: FieldId,
    change: SettingsChange,
) -> Result<ProjectFieldRow> {
    let mut tx = pool.begin().await?;
    lock_project(&mut tx, actor, project_id).await?;
    let attached = fields::lock_attached(&mut tx, project_id, field_id)
        .await?
        .ok_or(Error::NotFound("the field is not in the project"))?;
    if attached.version != change.version {
        return Err(Error::Conflict(
            "the field's settings were changed by someone else",
        ));
    }
    let field = fields::get(&mut *tx, field_id)
        .await?
        .ok_or(Error::NotFound("no such field"))?;
    let field_type = FieldKind::parse(&field.kind)
        .map(|kind| FieldType {
            kind,
            multi: field.multi,
        })
        .ok_or_else(|| Error::Internal("unknown field kind".to_owned()))?;
    let mut problems = Problems::new();
    let label = change
        .empty_label
        .as_deref()
        .and_then(|raw| problems.check("empty_label", empty_label(raw)));
    let can_be_empty = change.can_be_empty.unwrap_or(attached.can_be_empty);
    let resulting_label = match &label {
        Some(label) => label.clone(),
        None if !can_be_empty => String::new(),
        None => attached.empty_label.clone(),
    };
    problems.check(
        "empty_label",
        check_empty_label(can_be_empty, &resulting_label),
    );
    problems.finish().map_err(Error::Invalid)?;
    let defaults = match &change.default_values {
        Some(list) => Some(check_default_values(&mut tx, field_type, attached.set_id, list).await?),
        None => None,
    };
    let label_change =
        (resulting_label != attached.empty_label).then_some(resulting_label.as_str());
    fields::update_attached(
        &mut tx,
        project_id,
        field_id,
        &AttachedChange {
            can_be_empty: change.can_be_empty,
            empty_label: label_change,
            default_values: defaults.as_deref(),
            private: change.private,
            ..AttachedChange::default()
        },
    )
    .await?;
    projects::touch(&mut tx, &[project_id]).await?;
    audit::record(
        &mut tx,
        actor_of(actor),
        "project.field_changed",
        &target(project_id),
        json!({
            "field": field_id,
            "can_be_empty": change.can_be_empty,
            "empty_label": label_change,
            "default_values": defaults,
            "private": change.private,
        }),
    )
    .await?;
    tx.commit().await?;
    one(pool, project_id, field_id).await
}

/// Moves a field in the project's order (FLD-058) right after another one, or first.
pub async fn move_after(
    pool: &PgPool,
    actor: &Access,
    project_id: ProjectId,
    field_id: FieldId,
    after: Option<FieldId>,
) -> Result<Vec<ProjectFieldRow>> {
    let mut tx = pool.begin().await?;
    lock_project(&mut tx, actor, project_id).await?;
    fields::lock_attached(&mut tx, project_id, field_id)
        .await?
        .ok_or(Error::NotFound("the field is not in the project"))?;
    let rank = match after {
        Some(after) if after == field_id => {
            return Err(Error::field(
                "after",
                Problem::Invalid,
                "a field cannot follow itself",
            ));
        }
        Some(after) => {
            let previous = fields::lock_attached(&mut tx, project_id, after)
                .await?
                .ok_or_else(|| Error::field("after", Problem::Invalid, "no such field here"))?;
            let next = fields::attached_rank_after(&mut *tx, project_id, &previous.rank).await?;
            rank::between(Some(&previous.rank), next.as_deref()).map_err(internal)?
        }
        None => {
            let first = fields::first_attached_rank(&mut *tx, project_id).await?;
            rank::between(None, first.as_deref()).map_err(internal)?
        }
    };
    fields::update_attached(
        &mut tx,
        project_id,
        field_id,
        &AttachedChange {
            rank: Some(&rank),
            ..AttachedChange::default()
        },
    )
    .await?;
    projects::touch(&mut tx, &[project_id]).await?;
    tx.commit().await?;
    list(pool, actor, project_id).await
}

/// Gives the project its own copy of the set a field uses there (FLD-030): the same values in the
/// same order; defaults follow their copies. Other projects keep the shared set. Needs
/// `project.update` only here.
pub async fn fork_set(
    pool: &PgPool,
    actor: &Access,
    project_id: ProjectId,
    field_id: FieldId,
) -> Result<ProjectFieldRow> {
    let mut tx = pool.begin().await?;
    let project = lock_project(&mut tx, actor, project_id).await?;
    let attached = fields::lock_attached(&mut tx, project_id, field_id)
        .await?
        .ok_or(Error::NotFound("the field is not in the project"))?;
    let Some(set_id) = attached.set_id else {
        return Err(Error::Conflict("the field has no value set"));
    };
    let set = value_sets::lock(&mut tx, set_id)
        .await?
        .ok_or_else(|| Error::Internal("the set is gone".to_owned()))?;
    let (copy, defaults, pairs) =
        crate::value_sets::fork(&mut tx, &set, &project.name, &attached.default_values).await?;
    // The project's issues hold the copies from now on.
    let (old, new): (Vec<i64>, Vec<i64>) = pairs.into_iter().unzip();
    issue_values::remap_project(&mut tx, project_id, field_id, &old, &new).await?;
    fields::update_attached(
        &mut tx,
        project_id,
        field_id,
        &AttachedChange {
            set_id: Some(copy),
            default_values: Some(&defaults),
            ..AttachedChange::default()
        },
    )
    .await?;
    value_sets::delete_unused(&mut tx, &[set_id]).await?;
    projects::touch(&mut tx, &[project_id]).await?;
    audit::record(
        &mut tx,
        actor_of(actor),
        "project.field_set_forked",
        &target(project_id),
        json!({ "field": field_id, "from": set_id, "to": copy }),
    )
    .await?;
    tx.commit().await?;
    one(pool, project_id, field_id).await
}

/// How many of the project's issues hold a value of the field: shown before detaching it
/// (FLD-054).
pub async fn detach_impact(
    pool: &PgPool,
    actor: &Access,
    project_id: ProjectId,
    field_id: FieldId,
) -> Result<i64> {
    let mut tx = pool.begin().await?;
    lock_project(&mut tx, actor, project_id).await?;
    fields::lock_attached(&mut tx, project_id, field_id)
        .await?
        .ok_or(Error::NotFound("the field is not in the project"))?;
    let field = fields::get(&mut *tx, field_id)
        .await?
        .ok_or(Error::NotFound("no such field"))?;
    let kind = FieldKind::parse(&field.kind).ok_or_else(|| internal("unknown field kind"))?;
    Ok(issue_values::issues_with_field(&mut tx, field_id, kind, Some(project_id)).await?)
}

/// Detaches a field from a project (FLD-054). Its values in the project's issues go with it at
/// once, so attaching it again brings none back; a set only this project used is deleted.
pub async fn detach(
    pool: &PgPool,
    actor: &Access,
    project_id: ProjectId,
    field_id: FieldId,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    lock_project(&mut tx, actor, project_id).await?;
    let attached = fields::lock_attached(&mut tx, project_id, field_id)
        .await?
        .ok_or(Error::NotFound("the field is not in the project"))?;
    let field = fields::get(&mut *tx, field_id)
        .await?
        .ok_or(Error::NotFound("no such field"))?;
    let kind = FieldKind::parse(&field.kind).ok_or_else(|| internal("unknown field kind"))?;
    while issue_values::erase_batch(&mut tx, field_id, kind, Some(project_id), ERASE_BATCH).await?
        > 0
    {}
    fields::detach(&mut tx, project_id, field_id).await?;
    if let Some(set) = attached.set_id {
        value_sets::delete_unused(&mut tx, &[set]).await?;
    }
    projects::touch(&mut tx, &[project_id]).await?;
    audit::record(
        &mut tx,
        actor_of(actor),
        "project.field_detached",
        &target(project_id),
        json!({ "field": field_id }),
    )
    .await?;
    if kind == FieldKind::State {
        crate::value_sets::enqueue_work(
            &mut tx,
            actor,
            FieldWork::ResolveProject {
                project: project_id,
                actor: actor.user(),
            },
        )
        .await?;
    }
    tx.commit().await?;
    Ok(())
}
