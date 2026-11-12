//! AUT-085 (step 1.9 §3.9, part 1.9b-3): the scrum rules. Three behaviors on the `scrum`
//! toggle:
//!
//! - **Assign on move.** A state change on a published issue nobody is assigned to — by the
//!   field use case or a board card move or cell create, which go through the same hook (§3.5) —
//!   gives the issue to the actor, when the actor belongs to the assignee field's set. The card
//!   that lands in a column is the mover's to work on; the record says it, so no message.
//! - **Epics and tasks.** An epic counts no ideal days and a task no story points: the field
//!   change that would set one is refused where the form shows field problems, unless it clears
//!   the value. A project without the field, the type value or the type itself is inert.
//! - **Parent follows.** The walk of AUT-094 under this toggle (§3.7's values: the first
//!   resolved value of the scrum set is «Готово»/Done) — the same function, so a scrum project
//!   needs no `subtasks_follow` of its own.

use sqlx::PgConnection;
use trackflow_domain::fields::FieldKind;
use trackflow_domain::issues::values::Value;
use trackflow_domain::rules::{ASSIGNEE, EPIC, IDEAL_DAYS, Rule, STORY_POINTS, TASK, TYPE};
use trackflow_domain::validation::{Problem, Rejection};
use trackflow_store::value_sets;

use crate::Error;
use crate::issues::values::{self, ProjectField, ProjectFields};
use crate::live::Batch;
use crate::rules::{Ctx, Event, follows};

pub async fn scrum(
    conn: &mut PgConnection,
    batch: &mut Batch,
    ctx: &mut Ctx<'_>,
    event: &Event<'_>,
) -> Result<(), Error> {
    match event {
        Event::FieldChanged { field, .. } => match field.kind.kind {
            FieldKind::State => assign_on_move(&mut *conn, batch, ctx, event).await,
            FieldKind::Integer => estimate_blocked(&mut *conn, ctx, event).await,
            _ => Ok(()),
        },
        // The walk of AUT-094 under the scrum toggle (§3.9), once: where `subtasks_follow` is on
        // too, its own dispatch has already run it.
        Event::BecameResolved { .. } | Event::BecameUnresolved { .. } => {
            let issue = event.issue();
            if ctx
                .toggles(&mut *conn, issue.project_id)
                .await?
                .iter()
                .any(|toggle| toggle.enabled && toggle.rule == Rule::ParentFollowsSubtasks)
            {
                return Ok(());
            }
            follows::parent_follows_subtasks(&mut *conn, batch, ctx, event, Rule::Scrum).await
        }
        _ => Ok(()),
    }
}

/// Assign on move (§3.9): a state change on a published, unassigned issue gives it to the actor
/// — the one who moved the card — when the actor is assignable in the assignee field's set
/// (FLD-008). Silent: the record names the rule and the person.
async fn assign_on_move(
    conn: &mut PgConnection,
    batch: &mut Batch,
    ctx: &mut Ctx<'_>,
    event: &Event<'_>,
) -> Result<(), Error> {
    let issue = event.issue();
    if issue.is_draft() {
        return Ok(());
    }
    let fields = ctx.fields(&mut *conn, issue.project_id).await?;
    let Some(assignee) = find_field(&fields, &ASSIGNEE, ctx.language) else {
        return Ok(());
    };
    let held = ctx.held_of(&mut *conn, issue, &fields).await?;
    if held
        .get(&assignee.id())
        .is_some_and(|value| !value.refs().is_empty())
    {
        return Ok(()); // the change leaves an assignee in place: the issue is someone's already
    }
    // The actor must be assignable: an account of the assignee field's set (FLD-008).
    if values::check_refs(
        &mut *conn,
        ctx.actor,
        assignee,
        &[ctx.actor.user()],
        &[],
        "value",
    )
    .await
    .is_err()
    {
        return Ok(());
    }
    super::write(
        &mut *conn,
        batch,
        ctx,
        issue,
        assignee,
        Some(&Value::Refs(vec![ctx.actor.user()])),
        Rule::Scrum,
    )
    .await?;
    Ok(())
}

/// The estimation blocks (§3.9): an epic counts no ideal days, a task no story points — the
/// change that would set one is refused where the form shows field problems, unless it clears
/// the value. Missing field, type value or type itself: the rule is inert.
async fn estimate_blocked(
    conn: &mut PgConnection,
    ctx: &mut Ctx<'_>,
    event: &Event<'_>,
) -> Result<(), Error> {
    let Event::FieldChanged {
        issue, field, new, ..
    } = event
    else {
        return Ok(());
    };
    if new.is_none() {
        return Ok(()); // clearing is always allowed
    }
    let fields = ctx.fields(&mut *conn, issue.project_id).await?;
    // Each estimation field is the rule's business on its own: a project may hold one without
    // the other (§3.9's "missing field … inert").
    let days = find_field(&fields, &IDEAL_DAYS, ctx.language);
    let points = find_field(&fields, &STORY_POINTS, ctx.language);
    let (needle, what) = if days.is_some_and(|found| found.id() == field.id()) {
        (&EPIC, "epics do not count ideal days")
    } else if points.is_some_and(|found| found.id() == field.id()) {
        (&TASK, "tasks do not count story points")
    } else {
        return Ok(()); // another integer field: none of the rule's business
    };
    // The issue's type: without the field, a held value or the value the block names, inert.
    let Some(type_field) = find_field(&fields, &TYPE, ctx.language) else {
        return Ok(());
    };
    let Some(set) = type_field.attached.set_id else {
        return Ok(());
    };
    let held = ctx.held_of(&mut *conn, issue, &fields).await?;
    let Some(held_type) = held
        .get(&type_field.id())
        .and_then(|value| value.refs().first().copied())
    else {
        return Ok(());
    };
    let set_values = value_sets::values_of(&mut *conn, &[set]).await?;
    let is_type = set_values.iter().any(|value| {
        value.id == held_type
            && (value.name.to_lowercase() == needle.en.to_lowercase()
                || value.name.to_lowercase() == needle.ru.to_lowercase())
    });
    if !is_type {
        return Ok(());
    }
    Err(Error::rejected(
        "value",
        Rejection::new(Problem::Invalid, what),
    ))
}

/// The project's field a rule looks up (§3.4), in the engine's shared way.
fn find_field<'a>(
    fields: &'a ProjectFields,
    needle: &trackflow_domain::rules::FieldRef,
    language: trackflow_domain::settings::Language,
) -> Option<&'a ProjectField> {
    super::find_field(fields, needle, language)
}
