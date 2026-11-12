//! The behaviors of step 1.9 §3.6: assignee visibility (AUT-063), the `+1` vote (AUT-079) and
//! the component owner (AUT-090, FLD-049). Each looks at the events it owns and changes issues
//! only through [`super::write`], which speaks of the rule in history; the messages go to the
//! actor through [`Ctx::say`].

use sqlx::PgConnection;
use trackflow_domain::fields::FieldKind;
use trackflow_domain::issues::rights;
use trackflow_domain::issues::values::Value;
use trackflow_domain::rules::{ASSIGNEE, COMPONENT, FieldRef, Rule};
use trackflow_store::issues;
use trackflow_store::{groups, users, value_sets};

use crate::Error;
use crate::issues::values::{self, ProjectField, ProjectFields};
use crate::live::Batch;
use crate::rules::{Ctx, Event};

/// AUT-063: a change of the assignee to someone the issue's audience does not list is told to
/// the actor — the assignee cannot see the issue they are to work on. Never blocks, never
/// changes anything.
pub async fn assignee_visibility(
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
    // The assignee holds one account; a cleared value says nothing.
    let Some(assignee) = new.and_then(|value| value.refs().first().copied()) else {
        return Ok(());
    };
    if field.kind.kind != FieldKind::User || field.kind.multi {
        return Ok(());
    }
    // Only a published issue with an audience can hide its assignee.
    if issue.is_draft() || (issue.audience_users.is_empty() && issue.audience_groups.is_empty()) {
        return Ok(());
    }
    let fields = ctx.fields(&mut *conn, issue.project_id).await?;
    let Some(assignee_field) = find_field(&fields, &ASSIGNEE, ctx.language) else {
        return Ok(());
    };
    if field.id() != assignee_field.id() {
        return Ok(());
    }
    // The reporter, a listed account and a member of a listed group all see the issue.
    if assignee == issue.reporter_id
        || issue.audience_users.contains(&assignee)
        || groups::memberships(&mut *conn, &[assignee])
            .await?
            .iter()
            .any(|(_, group)| issue.audience_groups.contains(group))
    {
        return Ok(());
    }
    let name = users::get(&mut *conn, assignee)
        .await?
        .map(|user| user_label(&user))
        .unwrap_or_else(|| assignee.to_string());
    ctx.say(
        Rule::AssigneeVisibility,
        "assignee_not_visible",
        &[("assignee", name)],
    );
    Ok(())
}

/// AUT-079: a comment that says `+1` among its words adds the commenter's vote, the way the vote
/// button does — inside the same transaction. A second `+1` changes nothing.
pub async fn plus_one_vote(
    conn: &mut PgConnection,
    ctx: &mut Ctx<'_>,
    event: &Event<'_>,
) -> Result<(), Error> {
    let Event::CommentAdded { issue, text } = event else {
        return Ok(());
    };
    let said = text
        .split(|c: char| c.is_whitespace() || ",.;!?".contains(c))
        .any(|word| word == "+1");
    if !said || rights::may_vote(ctx.actor, &crate::issues::facts(issue)).is_err() {
        return Ok(());
    }
    if issues::vote(&mut *conn, issue.id, ctx.actor.user()).await? {
        ctx.say(Rule::PlusOneVote, "vote_added", &[]);
        // Auto-watch (ISS-030): this vote makes the voter a watcher like the button does.
        crate::notifications::auto_watch(
            conn,
            ctx.actor.user(),
            issue.id,
            crate::notifications::AutoWatch::Voted,
        )
        .await?;
    }
    Ok(())
}

/// AUT-090, FLD-049: an issue that takes a component value — by a change of the owned field, a
/// move or a publication — and has nobody assigned gets the value's owner as its assignee, when
/// the owner belongs to the assignee field's set; an owner outside it is told to the actor. An
/// issue that keeps an assignee is left alone.
pub async fn component_owner(
    conn: &mut PgConnection,
    batch: &mut Batch,
    ctx: &mut Ctx<'_>,
    event: &Event<'_>,
) -> Result<(), Error> {
    match event {
        Event::FieldChanged { field, .. } if field.kind.kind != FieldKind::Owned => return Ok(()),
        Event::FieldChanged { .. } | Event::Moved { .. } | Event::Published { .. } => {}
        _ => return Ok(()),
    }
    let issue = event.issue();
    let fields = ctx.fields(&mut *conn, issue.project_id).await?;
    let Some(assignee_field) = find_field(&fields, &ASSIGNEE, ctx.language) else {
        return Ok(());
    };
    let Some(owned_field) = find_field(&fields, &COMPONENT, ctx.language) else {
        return Ok(());
    };
    if let Event::FieldChanged { field, .. } = event
        && field.id() != owned_field.id()
    {
        return Ok(());
    }
    let held = values::of_issue(&mut *conn, issue, &fields).await?;
    if held
        .get(&assignee_field.id())
        .is_some_and(|value| !value.refs().is_empty())
    {
        return Ok(());
    }
    let Some(value) = held
        .get(&owned_field.id())
        .and_then(|value| value.refs().first().copied())
    else {
        return Ok(());
    };
    let found = value_sets::get_values(&mut *conn, &[value]).await?;
    let Some(held) = found.iter().find(|v| v.id == value) else {
        return Ok(());
    };
    let Some(owner) = held.owner_id else {
        return Ok(());
    };
    let owner_label = users::get(&mut *conn, owner)
        .await?
        .map(|user| user_label(&user))
        .unwrap_or_else(|| owner.to_string());
    // The owner must be assignable: an account of the assignee field's set (FLD-008).
    if values::check_refs(
        &mut *conn,
        ctx.actor,
        assignee_field,
        &[owner],
        &[],
        "value",
    )
    .await
    .is_ok()
    {
        super::write(
            &mut *conn,
            batch,
            ctx,
            issue,
            assignee_field,
            Some(&Value::Refs(vec![owner])),
            Rule::ComponentOwner,
        )
        .await?;
        ctx.say(
            Rule::ComponentOwner,
            "assigned_to_owner",
            &[("assignee", owner_label), ("value", held.name.clone())],
        );
    } else {
        ctx.say(
            Rule::ComponentOwner,
            "owner_not_assignable",
            &[("owner", owner_label), ("value", held.name.clone())],
        );
    }
    Ok(())
}

/// The project's field a rule looks up (§3.4), in the engine's shared way.
fn find_field<'a>(
    fields: &'a ProjectFields,
    needle: &FieldRef,
    language: trackflow_domain::settings::Language,
) -> Option<&'a ProjectField> {
    super::find_field(fields, needle, language)
}

/// An account as messages name people: the full name, or the login when there is none.
fn user_label(user: &users::User) -> String {
    if user.full_name.is_empty() {
        user.username.clone()
    } else {
        user.full_name.clone()
    }
}
