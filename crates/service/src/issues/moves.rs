//! Moving an issue to another project (ISS-025, FLD-059) and switching a draft's project. A
//! preview shows first which values go, which change and whether the actor keeps access.

use std::collections::BTreeMap;

use sqlx::PgConnection;
use time::OffsetDateTime;
use trackflow_domain::access::Access;
use trackflow_domain::access::visibility::{self, Audience, Item, Kind};
use trackflow_domain::fields::FieldKind;
use trackflow_domain::issues::history::{Origin, Place, Record};
use trackflow_domain::issues::moves::{self, Outcome, TargetField};
use trackflow_domain::issues::readable_id;
use trackflow_domain::issues::rights;
use trackflow_domain::issues::values::{self as rules, Value};
use trackflow_domain::{FieldId, IssueId, ProjectId};
use trackflow_store::PgPool;
use trackflow_store::issue_values;
use trackflow_store::issues::{self, Issue};
use trackflow_store::projects::{self, Project};
use trackflow_store::value_sets;

use super::values::{self, ProjectField, ProjectFields};
use super::{One, author, facts, get, lock_for_change};
use crate::live::Batch;
use crate::{Error, Result};

/// What a move would do (ISS-025).
#[derive(Debug, Clone)]
pub struct MovePreview {
    pub target: Project,
    /// The issue's fields that would change, with what happens to them.
    pub changes: Vec<(ProjectField, Option<Value>, Outcome)>,
    /// The actor could not see the issue afterwards.
    pub loses_access: bool,
}

/// Which references of a value the target's field accepts.
async fn accepted(
    conn: &mut PgConnection,
    target: &ProjectField,
    refs: &[i64],
) -> Result<Vec<i64>> {
    match target.kind.kind {
        FieldKind::Group => Ok(refs.to_vec()),
        FieldKind::User => {
            let Some(set) = target.attached.set_id else {
                return Ok(Vec::new());
            };
            let mut out = Vec::new();
            for id in refs {
                if value_sets::user_allowed(&mut *conn, set, *id).await? {
                    out.push(*id);
                }
            }
            Ok(out)
        }
        _ => {
            let Some(set) = target.attached.set_id else {
                return Ok(Vec::new());
            };
            let found = value_sets::get_values(&mut *conn, refs).await?;
            Ok(refs
                .iter()
                .filter(|id| found.iter().any(|v| v.id == **id && v.set_id == set))
                .copied()
                .collect())
        }
    }
}

async fn plan(
    conn: &mut PgConnection,
    held: &BTreeMap<FieldId, Value>,
    target: &ProjectFields,
) -> Result<Vec<(FieldId, Outcome)>> {
    let mut answers: BTreeMap<FieldId, TargetField> = BTreeMap::new();
    for (field, value) in held {
        if let Some(there) = target.get(*field) {
            answers.insert(
                *field,
                TargetField {
                    accepts: accepted(conn, there, value.refs()).await?,
                    defaults: there.attached.default_values.clone(),
                },
            );
        }
    }
    let current: Vec<(FieldId, Value)> = held.iter().map(|(f, v)| (*f, v.clone())).collect();
    Ok(moves::plan(&current, |field, _| {
        answers.get(&field).cloned()
    }))
}

fn loses_access(actor: &Access, issue: &Issue, target: ProjectId) -> bool {
    !visibility::can_see(
        actor,
        Kind::Issue,
        Item {
            project: target,
            author: Some(issue.reporter_id),
            audience: Audience {
                users: &issue.audience_users,
                groups: &issue.audience_groups,
            },
        },
    )
}

async fn target_project(
    conn: &mut PgConnection,
    actor: &Access,
    issue: &Issue,
    target: ProjectId,
) -> Result<Project> {
    let project = projects::get(&mut *conn, target)
        .await?
        .filter(|p| crate::projects::can_see(actor, p))
        .ok_or(Error::NotFound("no such project"))?;
    if project.id == issue.project_id {
        return Err(Error::Conflict("the issue is in this project already"));
    }
    if issue.is_draft() {
        rights::may_create(actor, project.id)?;
    } else {
        rights::may_move(actor, issue.project_id, project.id)?;
    }
    crate::projects::require_active(&project)?;
    Ok(project)
}

/// What moving the issue to `target` would do (ISS-025).
pub async fn preview(
    pool: &PgPool,
    actor: &Access,
    id: IssueId,
    target: ProjectId,
) -> Result<MovePreview> {
    let mut tx = pool.begin().await?;
    let (issue, _) = lock_for_change(&mut tx, actor, id).await?;
    let project = target_project(&mut tx, actor, &issue, target).await?;
    let source = values::of_project(&mut tx, issue.project_id).await?;
    let fields = values::of_project(&mut tx, project.id).await?;
    let held = values::of_issue(&mut tx, &issue, &source).await?;
    let outcomes = plan(&mut tx, &held, &fields).await?;
    let private_read = rights::may_read_private(actor, issue.project_id);
    let changes = outcomes
        .into_iter()
        .filter(|(_, outcome)| *outcome != Outcome::Kept)
        .filter_map(|(field, outcome)| {
            let f = source.get(field)?;
            (!f.private() || private_read).then(|| (f.clone(), held.get(&field).cloned(), outcome))
        })
        .collect();
    tx.rollback().await?;
    Ok(MovePreview {
        loses_access: loses_access(actor, &issue, project.id),
        target: project,
        changes,
    })
}

/// Moves an issue to another project (ISS-025, FLD-059): it gets the target's next number and the
/// old ID keeps leading to it (ISS-003); values follow the plan; required checks are skipped;
/// the default audience of the target does not apply (D-029). A draft just changes project and
/// takes the target's defaults for fields it has no value in. `None`: the issue moved where the
/// actor cannot see it.
pub async fn move_issue(
    pool: &PgPool,
    actor: &Access,
    origin: Origin,
    id: IssueId,
    target: ProjectId,
    silent: bool,
) -> Result<crate::rules::Answer<Option<One>>> {
    let mut tx = pool.begin().await?;
    let (issue, source_project) = lock_for_change(&mut tx, actor, id).await?;
    rights::may_update(actor, &facts(&issue))?;
    let project = target_project(&mut tx, actor, &issue, target).await?;
    // Two moves into the same project take numbers under the project's lock.
    projects::lock(&mut tx, project.id).await?;
    let who = author(actor, origin, silent, issue.project_id)?;
    let source = values::of_project(&mut tx, issue.project_id).await?;
    let fields = values::of_project(&mut tx, project.id).await?;
    let mut held = values::of_issue(&mut tx, &issue, &source).await?;
    let outcomes = plan(&mut tx, &held, &fields).await?;
    let mut records = Vec::new();
    if let Some(old_number) = issue.number {
        let number = issues::move_to(&mut tx, id, issue.project_id, old_number, project.id).await?;
        let from_id = readable_id(&source_project.key, old_number);
        let to_id = readable_id(&project.key, number);
        records.push(Record::project(
            Place {
                project: issue.project_id,
                number: old_number,
                id: &from_id,
            },
            Place {
                project: project.id,
                number,
                id: &to_id,
            },
        ));
    } else {
        issues::set_draft_project(&mut tx, id, project.id).await?;
    }
    for (field, outcome) in outcomes {
        let Some(f) = source.get(field) else {
            continue;
        };
        let old = held.get(&field).cloned();
        match outcome {
            Outcome::Kept => {}
            Outcome::Erased => {
                issue_values::set(&mut tx, id, field, f.kind.kind, None).await?;
                records.extend(Record::field(field, old.as_ref(), None, |_| None));
                held.remove(&field);
            }
            Outcome::Replaced(new) => {
                if let Some(there) = fields.get(field) {
                    records.extend(
                        values::write(&mut tx, id, there, old.as_ref(), new.as_ref()).await?,
                    );
                }
                match new {
                    Some(v) => held.insert(field, v),
                    None => held.remove(&field),
                };
            }
        }
    }
    if issue.is_draft() {
        for f in &fields.0 {
            if held.contains_key(&f.id()) {
                continue;
            }
            if let Some(value) = rules::default_value(&f.attached.default_values) {
                values::store(&mut tx, id, f.id(), f.kind.kind, Some(&value)).await?;
                held.insert(f.id(), value);
            }
        }
    }
    let moved = issues::get(&mut *tx, id)
        .await?
        .ok_or(Error::NotFound("no such issue"))?;
    let now = OffsetDateTime::now_utc();
    records.extend(values::settle_resolution(&mut tx, &moved, &fields, &held, now).await?);
    let mut batch = Batch::new();
    batch.issue_moved(id, issue.project_id);
    // The behaviors of the target project hear the move before its own records (§3.5).
    let mut ctx = crate::rules::Ctx::new(actor);
    let mut rule_records = crate::rules::on(
        &mut tx,
        &mut batch,
        &mut ctx,
        crate::rules::Event::Moved { issue: &moved },
    )
    .await?;
    rule_records.extend(
        crate::rules::on_resolution(&mut tx, &mut batch, &mut ctx, &moved, &records).await?,
    );
    super::finish_change(&mut tx, &mut batch, &moved, who, &records, &[]).await?;
    crate::rules::finish(&mut tx, &mut batch, &ctx, &moved, &rule_records).await?;
    batch.publish(&mut tx).await?;
    let can_see_after = super::can_read(actor, &moved, &project);
    let messages = ctx.messages;
    tx.commit().await?;
    if !can_see_after {
        return Ok(crate::rules::Answer {
            done: None,
            messages,
        });
    }
    let done = get(pool, actor, id).await?;
    Ok(crate::rules::Answer {
        done: Some(done),
        messages,
    })
}
