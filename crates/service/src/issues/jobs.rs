//! Background work on issues that follows an administrator's change of fields (architecture.md
//! §9.11): erasing the values of a deleted field (FLD-023), replacing a deleted value (FLD-032),
//! recomputing the resolved state when a value's flag or a project's state fields change
//! (ISS-023). Each batch is its own transaction and the work is idempotent, so a job interrupted
//! anywhere runs again safely.

use serde::{Deserialize, Serialize};
use serde_json::json;
use taskmaster_domain::fields::FieldKind;
use taskmaster_domain::issues::history::{Origin, Record};
use taskmaster_domain::issues::values::Value;
use taskmaster_domain::live::Change;
use taskmaster_domain::{FieldId, IssueId, ProjectId, UserId, ValueId};
use taskmaster_store::issue_history::{self, Author};
use taskmaster_store::issue_values;
use taskmaster_store::issues;
use taskmaster_store::jobs::{JobError, JobKind};
use taskmaster_store::{PgConnection, PgPool, value_sets};
use time::OffsetDateTime;

use super::values;
use crate::live::Batch;

/// Rows or issues one transaction handles.
const BATCH: i64 = 500;
/// Field kinds whose values live in sets.
const SET_KINDS: [&str; 5] = ["enum", "state", "version", "build", "owned"];

/// Work on issues after a change of fields (`issues.fields`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "work", rename_all = "snake_case")]
pub enum FieldWork {
    /// Erases every value of a deleted field (FLD-023).
    Erase { field: FieldId, kind: String },
    /// Replaces a deleted value with another of its set, or clears it, in every issue holding it
    /// (FLD-032). `name` is the deleted value's, for the history.
    Replace {
        value: ValueId,
        name: String,
        replacement: Option<ValueId>,
        actor: UserId,
    },
    /// Recomputes the resolved state of the issues holding a value whose resolved flag changed.
    ResolveValue { value: ValueId, actor: UserId },
    /// Recomputes the resolved state of a project's issues after its state fields changed.
    ResolveProject { project: ProjectId, actor: UserId },
}

impl JobKind for FieldWork {
    const KIND: &'static str = "issues.fields";
    const CANCELLABLE: bool = false;
    const MAX_ATTEMPTS: i32 = 10;
}

fn system(actor: UserId) -> Author {
    Author {
        user: actor,
        origin: Origin::System,
        silent: true,
    }
}

/// Runs the work to the end.
pub async fn run(pool: &PgPool, work: FieldWork) -> Result<(), JobError> {
    match work {
        FieldWork::Erase { field, kind } => {
            let Some(kind) = FieldKind::parse(&kind) else {
                return Err(JobError::Fail(format!("unknown field kind {kind}")));
            };
            loop {
                let mut tx = pool.begin().await?;
                let done = issue_values::erase_batch(&mut tx, field, kind, None, BATCH).await?;
                tx.commit().await?;
                if done == 0 {
                    return Ok(());
                }
            }
        }
        FieldWork::Replace {
            value,
            name,
            replacement,
            actor,
        } => {
            let kinds: Vec<&str> = SET_KINDS.to_vec();
            let mut after = 0;
            loop {
                let batch =
                    issue_values::issues_holding(pool, &[value], &kinds, after, BATCH).await?;
                let Some(last) = batch.last().copied() else {
                    return Ok(());
                };
                let mut tx = pool.begin().await?;
                let mut events = Batch::new();
                for issue in batch {
                    replace_in(
                        &mut tx,
                        &mut events,
                        issue,
                        value,
                        &name,
                        replacement,
                        actor,
                    )
                    .await?;
                }
                events.publish(&mut tx).await.map_err(retry)?;
                tx.commit().await?;
                after = last;
            }
        }
        FieldWork::ResolveValue { value, actor } => {
            let mut after = 0;
            loop {
                let batch =
                    issue_values::issues_holding(pool, &[value], &["state"], after, BATCH).await?;
                let Some(last) = batch.last().copied() else {
                    return Ok(());
                };
                let mut tx = pool.begin().await?;
                for issue in batch {
                    resolve(&mut tx, issue, actor).await?;
                }
                tx.commit().await?;
                after = last;
            }
        }
        FieldWork::ResolveProject { project, actor } => {
            let mut after = 0;
            loop {
                let batch = issues::ids_of_project(pool, project, after, BATCH).await?;
                let Some(last) = batch.last().copied() else {
                    return Ok(());
                };
                let mut tx = pool.begin().await?;
                for issue in batch {
                    resolve(&mut tx, issue, actor).await?;
                }
                tx.commit().await?;
                after = last;
            }
        }
    }
}

/// Recomputes one issue's resolved state and records the change.
async fn resolve(conn: &mut PgConnection, id: IssueId, actor: UserId) -> Result<(), JobError> {
    let Some(issue) = issues::lock(&mut *conn, id).await? else {
        return Ok(());
    };
    let fields = values::of_project(&mut *conn, issue.project_id)
        .await
        .map_err(retry)?;
    let held = values::of_issue(&mut *conn, &issue, &fields)
        .await
        .map_err(retry)?;
    let record = values::settle_resolution(
        &mut *conn,
        &issue,
        &fields,
        &held,
        OffsetDateTime::now_utc(),
    )
    .await
    .map_err(retry)?;
    if let Some(record) = record
        && !issue.is_draft()
    {
        let one = [record];
        issue_history::record(&mut *conn, id, system(actor), &one).await?;
        crate::notifications::record(&mut *conn, &issue, actor, &one, &[])
            .await
            .map_err(retry)?;
    }
    Ok(())
}

/// Replaces a deleted value in every field of one issue that holds it.
async fn replace_in(
    conn: &mut PgConnection,
    events: &mut Batch,
    id: IssueId,
    value: ValueId,
    name: &str,
    replacement: Option<ValueId>,
    actor: UserId,
) -> Result<(), JobError> {
    let Some(issue) = issues::lock(&mut *conn, id).await? else {
        return Ok(());
    };
    let fields = values::of_project(&mut *conn, issue.project_id)
        .await
        .map_err(retry)?;
    // Values of the issue as stored, also of fields whose value is no longer in a set.
    let rows = issue_values::of_issues(&mut *conn, &[id]).await?;
    let replacement_value = match replacement {
        Some(r) => value_sets::get_values(&mut *conn, &[r])
            .await?
            .into_iter()
            .next(),
        None => None,
    };
    let mut records = Vec::new();
    let mut state_changed = false;
    for f in &fields.0 {
        let refs: Vec<i64> = rows
            .iter()
            .filter(|r| r.field == f.id())
            .filter_map(|r| match r.value {
                issue_values::Stored::Ref(id) => Some(id),
                _ => None,
            })
            .collect();
        if !refs.contains(&value) || f.kind.kind.set_kind().is_none() {
            continue;
        }
        let mut new: Vec<i64> = refs.iter().filter(|v| **v != value).copied().collect();
        if let Some(r) = &replacement_value
            && Some(r.set_id) == f.attached.set_id
            && !new.contains(&r.id)
        {
            if f.kind.multi {
                new.push(r.id);
            } else {
                new = vec![r.id];
            }
        }
        let new_value = (!new.is_empty()).then(|| Value::Refs(new.clone()));
        issue_values::set(&mut *conn, id, f.id(), f.kind.kind, new_value.as_ref()).await?;
        let added: Vec<serde_json::Value> = replacement_value
            .iter()
            .filter(|r| new.contains(&r.id) && !refs.contains(&r.id))
            .map(|r| json!({ "id": r.id, "name": r.name }))
            .collect();
        records.push(Record {
            kind: taskmaster_domain::issues::history::ChangeKind::Field,
            field: Some(f.id()),
            comment: None,
            removed: Some(json!([{ "id": value, "name": name }])),
            added: Some(serde_json::Value::Array(added)),
        });
        state_changed |= f.kind.kind == FieldKind::State;
    }
    if records.is_empty() {
        return Ok(());
    }
    if state_changed {
        let held = values::of_issue(&mut *conn, &issue, &fields)
            .await
            .map_err(retry)?;
        records.extend(
            values::settle_resolution(
                &mut *conn,
                &issue,
                &fields,
                &held,
                OffsetDateTime::now_utc(),
            )
            .await
            .map_err(retry)?,
        );
    }
    if !issue.is_draft() {
        issue_history::record(&mut *conn, id, system(actor), &records).await?;
        crate::notifications::record(&mut *conn, &issue, actor, &records, &[])
            .await
            .map_err(retry)?;
    }
    issues::touch(conn, id, actor).await?;
    events.issue(id, Change::Updated);
    Ok(())
}

fn retry(error: crate::Error) -> JobError {
    JobError::Retry(error.to_string())
}
