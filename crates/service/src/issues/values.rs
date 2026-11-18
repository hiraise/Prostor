//! Field values of issues inside use cases: the project's fields, the issue's current values,
//! checking references against sets, writing a value with its history record, and the resolved
//! state that follows from state fields (ISS-023).

use std::collections::BTreeMap;

use sqlx::PgConnection;
use taskmaster_domain::access::Access;
use taskmaster_domain::fields::{FieldKind, FieldType};
use taskmaster_domain::issues::history::Record;
use taskmaster_domain::issues::resolution::{self, StateSlot};
use taskmaster_domain::issues::values::Value;
use taskmaster_domain::validation::Problem;
use taskmaster_domain::{FieldId, IssueId, ProjectId};
use taskmaster_store::fields::{self as store_fields, Attached, Field};
use taskmaster_store::groups;
use taskmaster_store::issue_values;
use taskmaster_store::issues::{self, Issue};
use taskmaster_store::users;
use taskmaster_store::value_sets;
use time::OffsetDateTime;

use crate::groups::team_visible;
use crate::{Error, Result};

/// A field attached to a project.
#[derive(Debug, Clone)]
pub struct ProjectField {
    pub field: Field,
    pub attached: Attached,
    pub kind: FieldType,
}

impl ProjectField {
    pub fn id(&self) -> FieldId {
        self.field.id
    }

    pub fn private(&self) -> bool {
        self.attached.private
    }
}

/// The fields of one project in its order.
#[derive(Debug, Clone, Default)]
pub struct ProjectFields(pub Vec<ProjectField>);

impl ProjectFields {
    pub fn get(&self, field: FieldId) -> Option<&ProjectField> {
        self.0.iter().find(|f| f.id() == field)
    }

    pub fn states(&self) -> impl Iterator<Item = &ProjectField> {
        self.0.iter().filter(|f| f.kind.kind == FieldKind::State)
    }
}

fn field_type(field: &Field) -> Option<FieldType> {
    let kind = FieldKind::parse(&field.kind)?;
    FieldType::new(kind, field.multi).ok()
}

/// The fields of several projects, with their definitions: three queries in all.
pub async fn of_projects(
    conn: &mut PgConnection,
    projects: &[ProjectId],
) -> Result<BTreeMap<ProjectId, ProjectFields>> {
    let attached = store_fields::of_projects(&mut *conn, projects).await?;
    let mut ids: Vec<FieldId> = attached.iter().map(|a| a.field_id).collect();
    ids.sort_unstable();
    ids.dedup();
    let defs = store_fields::get_many(&mut *conn, &ids).await?;
    let mut out: BTreeMap<ProjectId, ProjectFields> = projects
        .iter()
        .map(|p| (*p, ProjectFields::default()))
        .collect();
    for a in attached {
        let Some(field) = defs.iter().find(|f| f.id == a.field_id) else {
            continue;
        };
        let Some(kind) = field_type(field) else {
            continue;
        };
        out.entry(a.project_id).or_default().0.push(ProjectField {
            field: field.clone(),
            attached: a,
            kind,
        });
    }
    Ok(out)
}

pub async fn of_project(conn: &mut PgConnection, project: ProjectId) -> Result<ProjectFields> {
    Ok(of_projects(conn, &[project])
        .await?
        .remove(&project)
        .unwrap_or_default())
}

/// The values of issues by issue and field, read with each field's kind in the issue's project.
/// Values of fields the project no longer has are left out.
pub async fn of_issues(
    conn: &mut PgConnection,
    issues: &[(IssueId, ProjectId)],
    fields: &BTreeMap<ProjectId, ProjectFields>,
) -> Result<BTreeMap<IssueId, BTreeMap<FieldId, Value>>> {
    let ids: Vec<IssueId> = issues.iter().map(|(id, _)| *id).collect();
    let rows = issue_values::of_issues(conn, &ids).await?;
    let mut out: BTreeMap<IssueId, BTreeMap<FieldId, Value>> = BTreeMap::new();
    for (issue, project) in issues {
        let Some(project_fields) = fields.get(project) else {
            continue;
        };
        let mine: Vec<&issue_values::Row> = rows.iter().filter(|r| r.issue == *issue).collect();
        let mut values = BTreeMap::new();
        for f in &project_fields.0 {
            let stored: Vec<&issue_values::Stored> = mine
                .iter()
                .filter(|r| r.field == f.id())
                .map(|r| &r.value)
                .collect();
            if let Some(value) = issue_values::to_value(f.kind.kind, &stored) {
                values.insert(f.id(), value);
            }
        }
        out.insert(*issue, values);
    }
    Ok(out)
}

/// The values of one issue.
pub async fn of_issue(
    conn: &mut PgConnection,
    issue: &Issue,
    fields: &ProjectFields,
) -> Result<BTreeMap<FieldId, Value>> {
    let map = BTreeMap::from([(issue.project_id, fields.clone())]);
    Ok(of_issues(conn, &[(issue.id, issue.project_id)], &map)
        .await?
        .remove(&issue.id)
        .unwrap_or_default())
}

/// Checks that new references are allowed in the field (FLD-008…FLD-014): values of the
/// project's set, accounts of its user set, groups the actor may know. References the issue
/// holds already pass: an account that left a set or a value that was archived stays (FLD-045).
pub async fn check_refs(
    conn: &mut PgConnection,
    actor: &Access,
    field: &ProjectField,
    ids: &[i64],
    held: &[i64],
    path: &str,
) -> Result<()> {
    let new: Vec<i64> = ids
        .iter()
        .filter(|id| !held.contains(id))
        .copied()
        .collect();
    if new.is_empty() {
        return Ok(());
    }
    let bad = || Error::field(path, Problem::Invalid, "not a value of this field");
    match field.kind.kind {
        FieldKind::User => {
            let set = field.attached.set_id.ok_or_else(bad)?;
            let found = users::get_many(&mut *conn, &new).await?;
            for id in &new {
                let ok = found
                    .iter()
                    .any(|u| u.id == *id && !u.is_deleted() && !u.is_guest());
                if !ok || !value_sets::user_allowed(&mut *conn, set, *id).await? {
                    return Err(bad());
                }
            }
        }
        FieldKind::Group => {
            let found = groups::get_many(&mut *conn, &new).await?;
            let ok = new
                .iter()
                .all(|id| found.iter().any(|g| g.id == *id && team_visible(actor, g)));
            if !ok {
                return Err(bad());
            }
        }
        _ => {
            let set = field.attached.set_id.ok_or_else(bad)?;
            let found = value_sets::get_values(&mut *conn, &new).await?;
            let ok = new
                .iter()
                .all(|id| found.iter().any(|v| v.id == *id && v.set_id == set));
            if !ok {
                return Err(bad());
            }
        }
    }
    Ok(())
}

/// Names of set values for history records (HIST-004); accounts and groups go by ID.
pub async fn value_names(
    conn: &mut PgConnection,
    kind: FieldKind,
    values: &[Option<&Value>],
) -> Result<BTreeMap<i64, String>> {
    if matches!(kind, FieldKind::User | FieldKind::Group) || kind.set_kind().is_none() {
        return Ok(BTreeMap::new());
    }
    let ids: Vec<i64> = values
        .iter()
        .flatten()
        .flat_map(|v| v.refs().iter().copied())
        .collect();
    Ok(value_sets::get_values(conn, &ids)
        .await?
        .into_iter()
        .map(|v| (v.id, v.name))
        .collect())
}

/// Stores a field's value on an issue; text values are rendered with it (D-031). `None` clears.
pub async fn store(
    conn: &mut PgConnection,
    issue: IssueId,
    field: FieldId,
    kind: FieldKind,
    value: Option<&Value>,
) -> Result<()> {
    issue_values::set(&mut *conn, issue, field, kind, value).await?;
    if let Some(Value::Text(text)) = value {
        let rendered =
            crate::text::prepare(&mut *conn, text, taskmaster_markup::Profile::Full).await?;
        taskmaster_store::content::set_text_value(conn, issue, field, &rendered).await?;
    }
    Ok(())
}

/// Writes a field's new value and returns its history record (`None` when nothing changed).
pub async fn write(
    conn: &mut PgConnection,
    issue: IssueId,
    field: &ProjectField,
    old: Option<&Value>,
    new: Option<&Value>,
) -> Result<Option<Record>> {
    let names = value_names(&mut *conn, field.kind.kind, &[old, new]).await?;
    let record = Record::field(field.id(), old, new, |id| names.get(&id).cloned());
    if record.is_some() {
        store(conn, issue, field.id(), field.kind.kind, new).await?;
        // Auto-watch (ISS-030): an account a user field newly names — the assignee, or any other
        // place people are put — starts watching by their own switch's word.
        if field.kind.kind == FieldKind::User {
            let before = old.map(|value| value.refs()).unwrap_or_default();
            for user in new.map(|value| value.refs()).unwrap_or_default() {
                if !before.contains(user) {
                    crate::notifications::auto_watch(
                        conn,
                        *user,
                        issue,
                        crate::notifications::AutoWatch::Assigned,
                    )
                    .await?;
                }
            }
        }
    }
    Ok(record)
}

/// Sets the resolution time after a change of state fields or project (ISS-023) and returns the
/// record of the change, if the issue became resolved or reopened.
pub async fn settle_resolution(
    conn: &mut PgConnection,
    issue: &Issue,
    fields: &ProjectFields,
    values: &BTreeMap<FieldId, Value>,
    now: OffsetDateTime,
) -> Result<Option<Record>> {
    let slots: Vec<StateSlot> = fields
        .states()
        .map(|f| StateSlot {
            field: f.id(),
            value: values.get(&f.id()).and_then(|v| v.refs().first().copied()),
        })
        .collect();
    let held: Vec<i64> = slots.iter().filter_map(|s| s.value).collect();
    let resolved_values: Vec<i64> = value_sets::get_values(&mut *conn, &held)
        .await?
        .into_iter()
        .filter(|v| v.resolved)
        .map(|v| v.id)
        .collect();
    let resolved = resolution::is_resolved(&slots, |v| resolved_values.contains(&v));
    let at = resolution::resolved_at(issue.resolved_at, resolved, now);
    if at == issue.resolved_at {
        return Ok(None);
    }
    issues::set_resolved_at(conn, issue.id, at).await?;
    Ok((issue.resolved_at.is_some() != at.is_some()).then(|| Record::resolved(resolved)))
}
