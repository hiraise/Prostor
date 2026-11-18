//! AUT-094 (step 1.9 §3.7): a parent follows its subtasks. When a subtask becomes resolved and
//! none of the parent's other open subtasks stays, the parent takes its state field's first
//! resolved value; when a subtask reopens, the parent takes the reopen value. The whole chain
//! is walked in one call, its reads and writes batched (D-049): the parents' rows, children and
//! values load once, and the records and touches of every parent that flip land in one
//! statement each. What a write settles still re-enters the engine once per parent
//! ([`super::on_resolution`]), so the other behaviors of the parents' projects run — only this
//! walk stops there, finding every parent above already in its new state. The scrum rule walks
//! the same way (§3.9): one function, two toggles — `rule` names the one that runs it, and the
//! parent's project must hold that one on.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use prostor_domain::issues::history::{ChangeKind, Record};
use prostor_domain::issues::resolution::{self, StateSlot};
use prostor_domain::issues::values::Value;
use prostor_domain::live::Change;
use prostor_domain::rules::{REOPENED, Rule};
use prostor_domain::{FieldId, IssueId, ProjectId};
use prostor_store::issue_history;
use prostor_store::issue_values::{self, Stored};
use prostor_store::value_sets::Value as SetValue;
use prostor_store::{issues, links, value_sets};
use sqlx::PgConnection;
use time::OffsetDateTime;

use crate::Error;
use crate::issues::can_read;
use crate::issues::values::{ProjectField, ProjectFields};
use crate::live::Batch;
use crate::rules::{Ctx, Event, MAX_DEPTH};

pub async fn parent_follows_subtasks(
    conn: &mut PgConnection,
    batch: &mut Batch,
    ctx: &mut Ctx<'_>,
    event: &Event<'_>,
    rule: Rule,
) -> Result<(), Error> {
    let resolved = match event {
        Event::BecameResolved { .. } => true,
        Event::BecameUnresolved { .. } => false,
        _ => return Ok(()),
    };
    let issue = event.issue();
    // A walk's own re-entries for the parents it has just written stop here: the chain above them
    // is written, and climbing it again would only find every parent in its new state.
    if ctx.just_walked.contains(&issue.id) {
        return Ok(());
    }
    // The chain of parents the walk may write: one parent per hierarchy type (in P1 the built-in
    // subtask type is the only one), each published and alive, its project not archived with the
    // rule on (PRJ-062). The walk stops at the first parent it may not touch.
    let mut chain: Vec<links::Linked> = Vec::new();
    let mut child = issue.id;
    while chain.len() < MAX_DEPTH {
        let Some(parent) = links::parent_of(&mut *conn, child).await? else {
            break;
        };
        if parent.number.is_none() || parent.deleted_at.is_some() {
            break;
        }
        let Some(project) = ctx.project(&mut *conn, parent.project_id).await? else {
            break;
        };
        if project.is_archived()
            || !ctx
                .toggles(&mut *conn, parent.project_id)
                .await?
                .iter()
                .any(|toggle| toggle.enabled && toggle.rule == rule)
        {
            break;
        }
        child = parent.id;
        chain.push(parent);
    }
    if chain.is_empty() {
        return Ok(());
    }

    // One batch of reads for the whole chain (D-049): the parents' rows, their children, their
    // values and the state sets of their projects.
    let ids: Vec<IssueId> = chain.iter().map(|parent| parent.id).collect();
    let mut fields_of: BTreeMap<ProjectId, ProjectFields> = BTreeMap::new();
    for parent in &chain {
        let fields = ctx.fields(&mut *conn, parent.project_id).await?;
        fields_of
            .entry(parent.project_id)
            .or_insert_with(|| (*fields).clone());
    }
    let rows: HashMap<IssueId, issues::Issue> = issues::get_many(&mut *conn, &ids)
        .await?
        .into_iter()
        .map(|issue| (issue.id, issue))
        .collect();
    if rows.len() != ids.len() {
        return Err(Error::Internal(
            "the parent of a followed subtask is gone".to_owned(),
        ));
    }
    let children = links::children_of(&mut *conn, &ids).await?;
    // The values the walk decides and settles on are the state fields' refs — one query, not
    // the seven typed tables [`values::of_issues`] reads for a whole issue. The map is partial
    // on purpose and stays local: the engine's own cache keeps only whole issues.
    let mut state_fields: Vec<FieldId> = fields_of
        .values()
        .flat_map(|fields| fields.states().map(|field| field.id()))
        .collect();
    state_fields.sort_unstable();
    state_fields.dedup();
    let mut held: HashMap<IssueId, BTreeMap<FieldId, Value>> = HashMap::new();
    for row in issue_values::refs_of(&mut *conn, &ids, &state_fields).await? {
        let values = held.entry(row.issue).or_default();
        let refs = values
            .entry(row.field)
            .or_insert_with(|| Value::Refs(Vec::new()));
        if let Value::Refs(refs) = refs {
            refs.push(match row.value {
                Stored::Ref(value) => value,
                _ => unreachable!("refs_of answers refs"),
            });
        }
    }

    // Walk bottom-up: a parent flips under the same conditions as one step of the step-by-step
    // climb, and the first parent that does not flip ends the walk — the parent above it sees an
    // open subtask, or nothing to reopen.
    let now = OffsetDateTime::now_utc();
    let mut flipped: HashSet<IssueId> = HashSet::new();
    let mut crossed: Vec<(IssueId, bool)> = Vec::new();
    let mut stores: Vec<(IssueId, FieldId, i64)> = Vec::new();
    let mut written: Vec<(issues::Issue, Vec<Record>)> = Vec::new();
    for parent in &chain {
        let issue = &rows[&parent.id];
        let fields = &fields_of[&parent.project_id];
        if resolved {
            if issue.resolved_at.is_some() {
                break;
            }
            let open = children.iter().any(|(owner, child)| {
                *owner == parent.id
                    && child.number.is_some()
                    && child.deleted_at.is_none()
                    && child.resolved_at.is_none()
                    && !flipped.contains(&child.id)
            });
            if open {
                break;
            }
        } else if issue.resolved_at.is_none() {
            break;
        }
        let Some((field, value, set_values)) =
            state_value(&mut *conn, ctx, fields, resolved).await?
        else {
            break;
        };
        let held_values = held.entry(parent.id).or_default();
        let old = held_values.get(&field.id()).cloned();
        let new = Value::Refs(vec![value]);
        // The record with the names the walk already holds (the set's values); the store joins
        // the other parents' in one pair of statements after the loop.
        let names: HashMap<i64, &str> = set_values
            .iter()
            .map(|held| (held.id, held.name.as_str()))
            .collect();
        let Some(record) = Record::field(field.id(), old.as_ref(), Some(&new), |id| {
            names.get(&id).map(|name| (*name).to_owned())
        }) else {
            break;
        };
        stores.push((parent.id, field.id(), value));
        ctx.writes += 1;
        held_values.insert(field.id(), new);
        // The resolution settling of one parent, in memory over the values the walk already
        // holds (ISS-023): the resolved flags come from the sets read for the state values.
        let mut records = vec![record];
        let mut flags = resolved_flags(&mut *conn, ctx, fields, held_values).await?;
        collect_flags(&set_values, &mut flags);
        let at = settle(issue, fields, held_values, &flags, now, &mut records);
        crossed.push((parent.id, issue.resolved_at.is_some() != at.is_some()));
        let mut fresh = issue.clone();
        fresh.resolved_at = at;
        if at.is_some() {
            flipped.insert(parent.id);
        }
        // The message says what the parent became, but only to an actor who may see it.
        if let Some(project) = ctx.project(&mut *conn, parent.project_id).await?
            && can_read(ctx.actor, &fresh, &project)
        {
            let id = format!("{}-{}", project.key, parent.number.unwrap_or_default());
            let code = if resolved {
                "parent_set"
            } else {
                "parent_reopened"
            };
            let mut params = vec![("issue", id)];
            if resolved {
                let name = set_values
                    .iter()
                    .find(|held| held.id == value)
                    .map(|held| held.name.clone())
                    .unwrap_or_default();
                params.push(("value", name));
            }
            ctx.say(rule, code, &params);
        }
        written.push((fresh, records));
    }

    // The chain's writes, records, touches, resolution times and live events in one statement
    // each (D-049). The parents that cross cross in the walk's one direction, so one `at` fits
    // them.
    let (store_issues, store_fields, store_values): (Vec<IssueId>, Vec<FieldId>, Vec<i64>) =
        stores.into_iter().collect();
    issue_values::set_refs_many(&mut *conn, &store_issues, &store_fields, &store_values).await?;
    let crossing: Vec<IssueId> = crossed
        .iter()
        .filter(|(_, crossed)| *crossed)
        .map(|(id, _)| *id)
        .collect();
    issues::set_resolved_at_many(&mut *conn, &crossing, resolved.then_some(now)).await?;
    let (records_of, records): (Vec<IssueId>, Vec<Record>) = written
        .iter()
        .flat_map(|(issue, records)| records.iter().map(move |record| (issue.id, record.clone())))
        .collect();
    let touched: Vec<IssueId> = written.iter().map(|(issue, _)| issue.id).collect();
    issue_history::record_many(&mut *conn, &records_of, ctx.author(), &records).await?;
    issues::touch_many(&mut *conn, &touched, ctx.actor.user()).await?;
    for (issue, _) in &written {
        batch.issue(issue.id, Change::Updated);
    }
    // What the parents' settling changed goes through the engine once per parent, one level
    // deeper (`write`'s climb): the other behaviors of their projects run; this walk itself
    // stops at the first parent, finding the chain already written (`just_walked`).
    ctx.just_walked = written.iter().map(|(issue, _)| issue.id).collect();
    for (issue, records) in &written {
        if !records
            .iter()
            .any(|record| matches!(record.kind, ChangeKind::Resolved | ChangeKind::Reopened))
        {
            continue;
        }
        ctx.depth += 1;
        let result = Box::pin(super::on_resolution(conn, batch, ctx, issue, records)).await;
        ctx.depth -= 1;
        result.map(|_| ())?;
    }
    ctx.just_walked.clear();
    Ok(())
}

/// The value the parent's state takes (§3.7): on resolving, the first resolved value of its
/// project's state field, by set rank («Fixed» in the default set, «Done» in scrum); on
/// reopening, the value named «Открыта повторно»/Reopened when the set holds it, else the
/// attachment's default. The state field of a project: its first attached state field that has
/// a resolved value — `None` when there is none, and the walk stops. The set's values come back
/// with the pick: the walk has read them once, and the message and the settling use them again.
async fn state_value(
    conn: &mut PgConnection,
    ctx: &mut Ctx<'_>,
    fields: &ProjectFields,
    resolved: bool,
) -> Result<Option<(ProjectField, i64, Arc<Vec<SetValue>>)>, Error> {
    for field in fields.states() {
        let Some(set) = field.attached.set_id else {
            continue;
        };
        let values = ctx.set_values(&mut *conn, set).await?;
        let Some(first) = values.iter().find(|value| value.resolved) else {
            continue;
        };
        if resolved {
            return Ok(Some((field.clone(), first.id, values)));
        }
        // The reopen value: by name in either language, else the set's default value.
        let named = values.iter().find(|value| {
            value.name.to_lowercase() == REOPENED.en.to_lowercase()
                || value.name.to_lowercase() == REOPENED.ru.to_lowercase()
        });
        let value = named
            .map(|value| value.id)
            .or_else(|| field.attached.default_values.first().copied());
        if let Some(value) = value {
            return Ok(Some((field.clone(), value, values)));
        }
    }
    Ok(None)
}

/// The value ids flagged resolved of every state field's set of a project, from the sets the
/// context has already read; ids no set explains (a value a forked set no longer holds, FLD-045)
/// are looked up in one query.
async fn resolved_flags(
    conn: &mut PgConnection,
    ctx: &mut Ctx<'_>,
    fields: &ProjectFields,
    held: &BTreeMap<FieldId, Value>,
) -> Result<HashSet<i64>, Error> {
    let mut flags = HashSet::new();
    let mut unknown: Vec<i64> = Vec::new();
    for field in fields.states() {
        let Some(set) = field.attached.set_id else {
            continue;
        };
        let values = ctx.set_values(&mut *conn, set).await?;
        let known: HashSet<i64> = values.iter().map(|value| value.id).collect();
        collect_flags(&values, &mut flags);
        if let Some(Value::Refs(refs)) = held.get(&field.id()) {
            unknown.extend(refs.iter().copied().filter(|id| !known.contains(id)));
        }
    }
    if !unknown.is_empty() {
        collect_flags(
            &Arc::new(value_sets::get_values(&mut *conn, &unknown).await?),
            &mut flags,
        );
    }
    Ok(flags)
}

fn collect_flags(values: &[SetValue], flags: &mut HashSet<i64>) {
    flags.extend(
        values
            .iter()
            .filter(|value| value.resolved)
            .map(|value| value.id),
    );
}

/// The in-memory half of `values::settle_resolution`: the resolution time after the parent's
/// new state (ISS-023) and the record of the crossing. Returns the new `resolved_at`.
fn settle(
    issue: &issues::Issue,
    fields: &ProjectFields,
    held: &BTreeMap<FieldId, Value>,
    flags: &HashSet<i64>,
    now: OffsetDateTime,
    records: &mut Vec<Record>,
) -> Option<OffsetDateTime> {
    let slots: Vec<StateSlot> = fields
        .states()
        .map(|f| StateSlot {
            field: f.id(),
            value: held.get(&f.id()).and_then(|v| v.refs().first().copied()),
        })
        .collect();
    let resolved = resolution::is_resolved(&slots, |value| flags.contains(&value));
    let at = resolution::resolved_at(issue.resolved_at, resolved, now);
    if issue.resolved_at.is_some() != at.is_some() {
        records.push(Record::resolved(resolved));
    }
    at
}
