//! AUT-092 (step 1.9 §3.7): subtasks inherit the parent's fix versions. A subtask linked to a
//! parent copies the parent's version names onto its own fix-versions field, matched by name in
//! its project's set (the shared default set matches everything, a forked set what it still
//! holds); a change of the parent's field applies the same copy to every open subtask. Silent —
//! the records speak — and always through [`super::write`].

use std::collections::BTreeMap;

use sqlx::PgConnection;
use trackflow_domain::fields::FieldKind;
use trackflow_domain::issues::values::Value;
use trackflow_domain::links::BuiltinType;
use trackflow_domain::rules::{FIX_VERSIONS, Rule};
use trackflow_domain::{IssueId, ProjectId};
use trackflow_store::issues::Issue;
use trackflow_store::{issues, links, value_sets};

use crate::Error;
use crate::issues::values::{self, ProjectField, ProjectFields};
use crate::live::Batch;
use crate::rules::{Ctx, Event};

/// How many subtasks one change of fix versions may write at once (§3.7); more refuses.
const SUBTASKS_MAX: usize = 500;

pub async fn inherit_fix_versions(
    conn: &mut PgConnection,
    batch: &mut Batch,
    ctx: &mut Ctx<'_>,
    event: &Event<'_>,
) -> Result<(), Error> {
    match event {
        // A parent linked to the issue (the child is the issue): the child copies from it at once.
        Event::LinksAdded {
            issue,
            kind: BuiltinType::Subtask,
            others,
        } => {
            if issue.is_draft() || issue.resolved_at.is_some() {
                return Ok(());
            }
            for (other, is_source) in others.iter() {
                if *is_source {
                    copy_from_parent(&mut *conn, batch, ctx, issue, *other).await?;
                }
            }
            Ok(())
        }
        // A change of the project's fix-versions field: every open subtask takes the same copy.
        Event::FieldChanged {
            issue, field, new, ..
        } => {
            if field.kind.kind != FieldKind::Version || !field.kind.multi {
                return Ok(());
            }
            let fields = ctx.fields(&mut *conn, issue.project_id).await?;
            let Some(needle) = find_field(&fields, ctx.language) else {
                return Ok(());
            };
            if field.id() != needle.id() {
                return Ok(());
            }
            let names = match new {
                Some(value) => names_of(&mut *conn, value.refs()).await?,
                None => Vec::new(),
            };
            let children = links::children_of(&mut *conn, &[issue.id]).await?;
            if children.len() > SUBTASKS_MAX {
                return Err(Error::Refused(
                    format!(
                        "the issue has {} subtasks; fix versions reach at most {SUBTASKS_MAX} \
                         of them at once",
                        children.len()
                    )
                    .into(),
                ));
            }
            copy_to_subtasks(&mut *conn, batch, ctx, &children, &names).await
        }
        _ => Ok(()),
    }
}

/// Applies the copy to the open subtasks of a change: the projects batched, the fields batched,
/// the rows batched — one write per subtask in a project that has the field and the rule on.
async fn copy_to_subtasks(
    conn: &mut PgConnection,
    batch: &mut Batch,
    ctx: &mut Ctx<'_>,
    children: &[(IssueId, trackflow_store::links::Linked)],
    names: &[String],
) -> Result<(), Error> {
    let open: Vec<&trackflow_store::links::Linked> = children
        .iter()
        .map(|(_, child)| child)
        .filter(|child| {
            child.number.is_some() && child.deleted_at.is_none() && child.resolved_at.is_none()
        })
        .collect();
    // Which projects take the copy: not archived, the rule on there.
    let mut ids: Vec<ProjectId> = open.iter().map(|child| child.project_id).collect();
    ids.sort_unstable();
    ids.dedup();
    let mut on: Vec<ProjectId> = Vec::new();
    for id in &ids {
        let Some(project) = ctx.project(&mut *conn, *id).await? else {
            continue;
        };
        if !project.is_archived()
            && ctx
                .toggles(&mut *conn, *id)
                .await?
                .iter()
                .any(|toggle| toggle.enabled && toggle.rule == Rule::InheritFixVersions)
        {
            on.push(*id);
        }
    }
    if on.is_empty() {
        return Ok(());
    }
    // The fix-versions fields of those projects, their set values and the subtask rows: batched.
    let fields = values::of_projects(&mut *conn, &on).await?;
    let fix: BTreeMap<_, &ProjectField> = on
        .iter()
        .filter_map(|id| {
            fields
                .get(id)
                .and_then(|f| find_field(f, ctx.language))
                .map(|f| (*id, f))
        })
        .collect();
    let mut sets: Vec<i64> = fix.values().filter_map(|f| f.attached.set_id).collect();
    sets.sort_unstable();
    sets.dedup();
    let set_values = value_sets::values_of(&mut *conn, &sets).await?;
    let mut rows: Vec<_> = open
        .iter()
        .filter(|child| on.contains(&child.project_id))
        .map(|child| child.id)
        .collect();
    rows.sort_unstable();
    rows.dedup();
    for child in issues::get_many(&mut *conn, &rows).await? {
        let Some(field) = fix.get(&child.project_id).copied() else {
            continue;
        };
        let Some(set) = field.attached.set_id else {
            continue;
        };
        let values: Vec<value_sets::Value> = set_values
            .iter()
            .filter(|value| value.set_id == set)
            .cloned()
            .collect();
        copy(&mut *conn, batch, ctx, &child, field, &values, names).await?;
    }
    Ok(())
}

/// The copy itself (§3.7): the child's field takes the parent's version names matched by name in
/// the child's project's set, in the parent's order; names the set does not hold are skipped. No
/// name matches and the field may be empty: it is cleared; it may not: the copy is skipped.
#[allow(clippy::too_many_arguments)]
async fn copy(
    conn: &mut PgConnection,
    batch: &mut Batch,
    ctx: &mut Ctx<'_>,
    child: &Issue,
    field: &ProjectField,
    values: &[value_sets::Value],
    names: &[String],
) -> Result<(), Error> {
    let matched: Vec<i64> = names
        .iter()
        .filter_map(|name| {
            values
                .iter()
                .find(|value| value.name.to_lowercase() == name.to_lowercase())
                .map(|value| value.id)
        })
        .collect();
    let new = if matched.is_empty() {
        if !field.attached.can_be_empty {
            return Ok(());
        }
        None
    } else {
        Some(Value::Refs(matched))
    };
    super::write(
        conn,
        batch,
        ctx,
        child,
        field,
        new.as_ref(),
        Rule::InheritFixVersions,
    )
    .await?;
    Ok(())
}

/// Copies the parent's fix versions onto the child, by name in the child's project's set.
async fn copy_from_parent(
    conn: &mut PgConnection,
    batch: &mut Batch,
    ctx: &mut Ctx<'_>,
    child: &Issue,
    parent: trackflow_domain::IssueId,
) -> Result<(), Error> {
    let child_fields = ctx.fields(&mut *conn, child.project_id).await?;
    let Some(child_field) = find_field(&child_fields, ctx.language) else {
        return Ok(());
    };
    let Some(parent) = issues::get(&mut *conn, parent).await? else {
        return Ok(());
    };
    let parent_fields = ctx.fields(&mut *conn, parent.project_id).await?;
    let Some(parent_field) = find_field(&parent_fields, ctx.language) else {
        return Ok(());
    };
    let held = values::of_issue(&mut *conn, &parent, &parent_fields).await?;
    let refs = held
        .get(&parent_field.id())
        .map(|value| value.refs().to_vec())
        .unwrap_or_default();
    let names = names_of(&mut *conn, &refs).await?;
    copy_to_one(&mut *conn, batch, ctx, child, child_field, &names).await
}

/// The child's field values of its set, then the copy.
async fn copy_to_one(
    conn: &mut PgConnection,
    batch: &mut Batch,
    ctx: &mut Ctx<'_>,
    child: &Issue,
    field: &ProjectField,
    names: &[String],
) -> Result<(), Error> {
    let Some(set) = field.attached.set_id else {
        return Ok(());
    };
    let values = value_sets::values_of(&mut *conn, &[set]).await?;
    copy(conn, batch, ctx, child, field, &values, names).await
}

/// The names of set values, in the order the references hold them.
async fn names_of(conn: &mut PgConnection, refs: &[i64]) -> Result<Vec<String>, Error> {
    let found = value_sets::get_values(&mut *conn, refs).await?;
    let names: BTreeMap<i64, String> = found
        .into_iter()
        .map(|value| (value.id, value.name))
        .collect();
    Ok(refs
        .iter()
        .filter_map(|id| names.get(id).cloned())
        .collect())
}

/// The project's fix-versions field (§3.4): by built-in key first, else by name.
fn find_field(
    fields: &ProjectFields,
    language: trackflow_domain::settings::Language,
) -> Option<&ProjectField> {
    super::find_field(fields, &FIX_VERSIONS, language)
}
