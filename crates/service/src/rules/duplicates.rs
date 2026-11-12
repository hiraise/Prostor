//! AUT-071 (step 1.9 §3.8, part 1.9b-1): duplicates keep one root, raise their original's
//! priority and reopen without their links. Our Duplicate type is a hierarchy (D-034): the
//! original is the source of the link, the duplicate the target — so a duplicate has exactly one
//! original and no cycles, and the link layer enforces both. The rules, on the events of the link
//! use case:
//!
//! 1. Collapse to one root: a duplicate of an issue that is a duplicate itself hangs directly
//!    under the chain's root, and so do the duplicates it anchored — the group stays a star.
//! 2. Raise priority: the original takes its new duplicate's priority when that comes earlier in
//!    the one set both projects' priority fields share.
//! 3. Reopen without links: an issue whose state names it a duplicate and whose last
//!    duplicate-of link is gone takes the reopen value (§3.7's rule), silently.
//! 4. State on link: an issue made a duplicate takes the duplicate value when its state is not
//!    there yet and the actor may update it — without the right nothing changes.
//! 5. The state requires a link: the duplicate value without an original is refused where the
//!    form shows field problems, before anything lands.

use sqlx::PgConnection;
use trackflow_domain::fields::FieldKind;
use trackflow_domain::issues::rights;
use trackflow_domain::issues::values::Value;
use trackflow_domain::links::BuiltinType;
use trackflow_domain::live::Change;
use trackflow_domain::rules::{DUPLICATE, PRIORITY, REOPENED, Rule};
use trackflow_domain::validation::{Problem, Rejection};
use trackflow_store::issues::Issue;
use trackflow_store::{issues, links, value_sets};

use crate::Error;
use crate::issues::can_read;
use crate::issues::values::{self, ProjectField, ProjectFields};
use crate::live::Batch;
use crate::rules::{Ctx, Event};

/// How far the root walk follows originals upward (§3.8 rule 1); a cycle cannot exist (D-034).
const ROOT_MAX: i32 = 50;
/// How many duplicates one new link may reattach at once (§3.8 rule 1); more refuses.
const REATTACH_MAX: usize = 500;

pub async fn duplicates(
    conn: &mut PgConnection,
    batch: &mut Batch,
    ctx: &mut Ctx<'_>,
    event: &Event<'_>,
) -> Result<(), Error> {
    match event {
        // Rules 1 and 2: a duplicate link came (§3.8). Drafts run nothing: their links are
        // recorded when they are published (LNK-007).
        Event::LinksAdded {
            issue,
            kind: BuiltinType::Duplicate,
            others,
        } => {
            if issue.is_draft() {
                return Ok(());
            }
            for (other, other_is_source) in others.iter() {
                // The target of a duplicate link is the duplicate, its source the original.
                let (duplicate, original) = if *other_is_source {
                    (issue.id, *other)
                } else {
                    (*other, issue.id)
                };
                linked(&mut *conn, batch, ctx, duplicate, original).await?;
            }
            Ok(())
        }
        // Rule 3: a duplicate-of link left (§3.8). The duplicate is the target of the removed
        // link: the event's issue when the other end was the original.
        Event::LinkRemoved {
            issue,
            kind: BuiltinType::Duplicate,
            other,
            was_source,
            ..
        } => {
            let duplicate = if *was_source { issue.id } else { *other };
            unlinked(&mut *conn, batch, ctx, duplicate).await
        }
        // Rule 5: the state that names a duplicate demands its link (§3.8). A refusal rolls the
        // whole use case back, so nothing of the change lands.
        Event::FieldChanged {
            issue, field, new, ..
        } if field.kind.kind == FieldKind::State => {
            requires_link(&mut *conn, ctx, issue, field, *new).await
        }
        _ => Ok(()),
    }
}

/// Rules 1 and 2 for one new link: `duplicate` was made a duplicate of `original`.
async fn linked(
    conn: &mut PgConnection,
    batch: &mut Batch,
    ctx: &mut Ctx<'_>,
    duplicate: i64,
    original: i64,
) -> Result<(), Error> {
    let Some(x) = issues::get(&mut *conn, duplicate).await? else {
        return Ok(());
    };
    let Some(y) = issues::get(&mut *conn, original).await? else {
        return Ok(());
    };
    let type_id = ctx.duplicate_type(&mut *conn).await?;
    // Rule 1 first: what the pair points at after the collapse decides the rest.
    let target = collapse(&mut *conn, batch, ctx, type_id, &x, &y).await?;
    raise_priority(&mut *conn, batch, ctx, &x, &target).await?;
    set_state(&mut *conn, batch, ctx, &x).await
}

/// Rule 1: the chain above the original collapses to its root — the link is rewritten to it —
/// and the duplicates the new duplicate anchored hang under the same root. Answers the issue the
/// duplicate points at when the smoke clears: the root, or the original it was linked to.
async fn collapse(
    conn: &mut PgConnection,
    batch: &mut Batch,
    ctx: &mut Ctx<'_>,
    type_id: i64,
    x: &Issue,
    y: &Issue,
) -> Result<Issue, Error> {
    // The originals above the original, nearest first: the last is the root (cap 50, §3.8).
    let chain = links::ancestors(&mut *conn, type_id, y.id, ROOT_MAX).await?;
    let mut target = y.clone();
    if let Some(root_id) = chain.last().copied()
        && let Some(root) = issues::get(&mut *conn, root_id).await?
        && ctx
            .project(&mut *conn, root.project_id)
            .await?
            .is_some_and(|project| !project.is_archived())
    {
        rewrite(&mut *conn, batch, ctx, type_id, x, y, &root).await?;
        target = root;
    }
    // The duplicates X anchored move under the same root, so the group stays a star. An archived
    // project's issues keep their links (PRJ-062: the archive is read-only).
    let children = links::duplicates_of(&mut *conn, &[x.id]).await?;
    let mut reach: Vec<i64> = Vec::new();
    for (_, child) in &children {
        if child.number.is_none() || child.deleted_at.is_some() {
            continue;
        }
        if ctx
            .project(&mut *conn, child.project_id)
            .await?
            .is_some_and(|project| project.is_archived())
        {
            continue;
        }
        reach.push(child.id);
    }
    if reach.len() > REATTACH_MAX {
        return Err(Error::Refused(
            format!(
                "the issue has {} duplicates; the rule reattaches at most {REATTACH_MAX} at once",
                reach.len()
            )
            .into(),
        ));
    }
    for child in issues::get_many(&mut *conn, &reach).await? {
        move_link(&mut *conn, batch, ctx, type_id, x, &child, &target).await?;
    }
    Ok(target)
}

/// Rewrites the just-added link to the root: the original's link to the duplicate goes, the
/// root's takes its place, records on every issue touched, and the actor hears where the link
/// landed — when they may see the root.
async fn rewrite(
    conn: &mut PgConnection,
    batch: &mut Batch,
    ctx: &mut Ctx<'_>,
    type_id: i64,
    x: &Issue,
    y: &Issue,
    root: &Issue,
) -> Result<(), Error> {
    let made = move_link(&mut *conn, batch, ctx, type_id, y, x, root).await?;
    if made.is_empty() {
        return Ok(()); // the link is gone already, or its replacement exists
    }
    if let Some(project) = ctx.project(&mut *conn, root.project_id).await?
        && can_read(ctx.actor, root, &project)
    {
        let issue = format!("{}-{}", project.key, root.number.unwrap_or_default());
        ctx.say(Rule::Duplicates, "link_reattached", &[("issue", issue)]);
    }
    Ok(())
}

/// Moves one duplicate's link from one original to another: the old link goes, the new one takes
/// its place, records on every issue touched (HIST-005). Answers the links made.
async fn move_link(
    conn: &mut PgConnection,
    batch: &mut Batch,
    ctx: &mut Ctx<'_>,
    type_id: i64,
    from: &Issue,
    duplicate: &Issue,
    to: &Issue,
) -> Result<Vec<links::Link>, Error> {
    let Some(old) = links::between(&mut *conn, type_id, from.id, duplicate.id).await? else {
        return Ok(Vec::new());
    };
    // The old link goes first: a duplicate keeps one original (D-034).
    take(&mut *conn, batch, old.id).await?;
    let made = links::insert_many(
        &mut *conn,
        type_id,
        &[(to.id, duplicate.id)],
        ctx.actor.user(),
    )
    .await?;
    for link in &made {
        batch.link(link.id, Change::Created);
    }
    let link_type = links::share_type(&mut *conn, type_id)
        .await?
        .ok_or_else(|| Error::Internal("the built-in duplicate link type is gone".to_owned()))?;
    crate::issue_links::record(
        &mut *conn,
        batch,
        ctx.author(),
        duplicate,
        &link_type,
        &made,
        &[old],
    )
    .await?;
    Ok(made)
}

/// Rule 2: the original takes its new duplicate's priority when that comes earlier in the one
/// set both projects' priority fields resolve to — the shared default set qualifies, a forked
/// one does not (§3.8). Both must hold a value.
async fn raise_priority(
    conn: &mut PgConnection,
    batch: &mut Batch,
    ctx: &mut Ctx<'_>,
    x: &Issue,
    target: &Issue,
) -> Result<(), Error> {
    let x_fields = ctx.fields(&mut *conn, x.project_id).await?;
    let Some(x_field) = find_field(&x_fields, ctx.language) else {
        return Ok(());
    };
    let target_fields = ctx.fields(&mut *conn, target.project_id).await?;
    let Some(target_field) = find_field(&target_fields, ctx.language) else {
        return Ok(());
    };
    let (Some(x_set), Some(target_set)) = (x_field.attached.set_id, target_field.attached.set_id)
    else {
        return Ok(());
    };
    if x_set != target_set {
        return Ok(()); // forks do not compare (§3.8)
    }
    let x_held = values::of_issue(&mut *conn, x, &x_fields).await?;
    let Some(x_value) = x_held
        .get(&x_field.id())
        .and_then(|value| value.refs().first().copied())
    else {
        return Ok(());
    };
    let target_held = values::of_issue(&mut *conn, target, &target_fields).await?;
    let Some(target_value) = target_held
        .get(&target_field.id())
        .and_then(|value| value.refs().first().copied())
    else {
        return Ok(());
    };
    // The set's order is its rank: earlier is higher (§3.8).
    let set_values = value_sets::values_of(&mut *conn, &[x_set]).await?;
    let rank_of = |id: i64| set_values.iter().position(|value| value.id == id);
    if x_value == target_value {
        return Ok(());
    }
    let (Some(x_at), Some(target_at)) = (rank_of(x_value), rank_of(target_value)) else {
        return Ok(());
    };
    if x_at >= target_at {
        return Ok(()); // the original is higher already, or as high
    }
    super::write(
        &mut *conn,
        batch,
        ctx,
        target,
        target_field,
        Some(&Value::Refs(vec![x_value])),
        Rule::Duplicates,
    )
    .await?;
    let name = set_values
        .iter()
        .find(|value| value.id == x_value)
        .map(|value| value.name.clone())
        .unwrap_or_default();
    ctx.say(Rule::Duplicates, "priority_raised", &[("value", name)]);
    Ok(())
}

/// Rule 3: the issue's last duplicate-of link is gone; a state that still names it a duplicate
/// takes the reopen value (§3.7's rule: the value named «Открыта повторно»/Reopened when the set
/// holds it, else the set's default). Silent — the records speak.
async fn unlinked(
    conn: &mut PgConnection,
    batch: &mut Batch,
    ctx: &mut Ctx<'_>,
    duplicate: i64,
) -> Result<(), Error> {
    let Some(issue) = issues::get(&mut *conn, duplicate).await? else {
        return Ok(());
    };
    if issue.is_draft() {
        return Ok(());
    }
    let type_id = ctx.duplicate_type(&mut *conn).await?;
    // Another original stays: the issue is still a duplicate (§3.8 names the *last* link).
    if !links::sources_of(&mut *conn, type_id, &[issue.id])
        .await?
        .is_empty()
    {
        return Ok(());
    }
    let fields = ctx.fields(&mut *conn, issue.project_id).await?;
    let held = values::of_issue(&mut *conn, &issue, &fields).await?;
    for field in fields.states() {
        let Some(set) = field.attached.set_id else {
            continue;
        };
        let Some(value) = held
            .get(&field.id())
            .and_then(|value| value.refs().first().copied())
        else {
            continue;
        };
        let set_values = value_sets::values_of(&mut *conn, &[set]).await?;
        let Some(own) = set_values.iter().find(|held| held.id == value) else {
            continue;
        };
        if !names(own.name.as_str(), &DUPLICATE) {
            continue;
        }
        // The reopen value, the way the parent-follows walk picks it (§3.7).
        let reopen = set_values
            .iter()
            .find(|held| names(held.name.as_str(), &REOPENED))
            .map(|held| held.id)
            .or_else(|| field.attached.default_values.first().copied());
        let Some(reopen) = reopen else {
            return Ok(());
        };
        super::write(
            &mut *conn,
            batch,
            ctx,
            &issue,
            field,
            Some(&Value::Refs(vec![reopen])),
            Rule::Duplicates,
        )
        .await?;
        return Ok(());
    }
    Ok(())
}

/// Whether a set value carries one of a name pair, either language, case-insensitively (§3.4).
fn names(held: &str, needle: &trackflow_domain::fields::builtin::Names) -> bool {
    held.to_lowercase() == needle.en.to_lowercase()
        || held.to_lowercase() == needle.ru.to_lowercase()
}

/// Rule 4: the state follows the link (§3.8): an issue made a duplicate takes the value named
/// «Дубликат»/Duplicate of its first state field that holds one, when its state is not there yet
/// and the actor may update it — without the right nothing changes, the way the reference's guard
/// leaves the issue alone. The write resolves the issue (`settle_resolution` runs inside
/// [`super::write`]), so the parent-follows walk climbs from it.
async fn set_state(
    conn: &mut PgConnection,
    batch: &mut Batch,
    ctx: &mut Ctx<'_>,
    duplicate: &Issue,
) -> Result<(), Error> {
    let fields = ctx.fields(&mut *conn, duplicate.project_id).await?;
    let held = values::of_issue(&mut *conn, duplicate, &fields).await?;
    for field in fields.states() {
        let Some(set) = field.attached.set_id else {
            continue;
        };
        let set_values = value_sets::values_of(&mut *conn, &[set]).await?;
        let Some(value) = set_values
            .iter()
            .find(|held| names(held.name.as_str(), &DUPLICATE))
            .map(|held| held.id)
        else {
            continue; // the set names no duplicate: the rule is inert here (§3.4)
        };
        if held
            .get(&field.id())
            .and_then(|held| held.refs().first().copied())
            == Some(value)
        {
            return Ok(()); // the state already names the duplicate
        }
        if rights::may_update(ctx.actor, &crate::issues::facts(duplicate)).is_err() {
            return Ok(()); // the actor may not write the issue: nothing changes
        }
        let old = held.get(&field.id()).cloned();
        let written = super::write(
            &mut *conn,
            batch,
            ctx,
            duplicate,
            field,
            Some(&Value::Refs(vec![value])),
            Rule::Duplicates,
        )
        .await?;
        if written.is_empty() {
            return Ok(());
        }
        // The rule's change is a field change like any other, so the behaviors of the issue's
        // project see it: AUT-098 copies the original's build onto the new duplicate in this same
        // transaction (§3.8 and §3.7's interplay), and rule 5 passes — the link exists by then.
        // The cycle (`on` → here → `on`) is boxed once, the way `climb` does.
        let fresh = issues::get(&mut *conn, duplicate.id)
            .await?
            .ok_or_else(|| Error::Internal("a rule wrote to a gone issue".to_owned()))?;
        ctx.depth += 1;
        let joined = Box::pin(super::on(
            &mut *conn,
            batch,
            ctx,
            Event::FieldChanged {
                issue: &fresh,
                field,
                old: old.as_ref(),
                new: Some(&Value::Refs(vec![value])),
            },
        ))
        .await?;
        ctx.depth -= 1;
        debug_assert!(joined.is_empty(), "a re-entered step answers no records");
        return Ok(());
    }
    Ok(())
}

/// Rule 5: the state that names a duplicate demands its link (§3.8): a state change to the value
/// named «Дубликат»/Duplicate on an issue without an original is refused where the form shows
/// field problems — the whole change rolls back. A rule-made write never comes through here:
/// rule writes fire no [`Event::FieldChanged`], and rule 4 runs once the link exists.
async fn requires_link(
    conn: &mut PgConnection,
    ctx: &mut Ctx<'_>,
    issue: &Issue,
    field: &ProjectField,
    new: Option<&Value>,
) -> Result<(), Error> {
    let Some(value) = new.and_then(|value| value.refs().first().copied()) else {
        return Ok(());
    };
    let Some(set) = field.attached.set_id else {
        return Ok(());
    };
    let becomes_duplicate = ctx
        .set_values(&mut *conn, set)
        .await?
        .iter()
        .any(|held| held.id == value && names(held.name.as_str(), &DUPLICATE));
    if !becomes_duplicate {
        return Ok(());
    }
    let type_id = ctx.duplicate_type(&mut *conn).await?;
    if links::sources_of(&mut *conn, type_id, &[issue.id])
        .await?
        .is_empty()
    {
        return Err(Error::rejected(
            "value",
            Rejection::new(Problem::Required, "add a link to the duplicate issue"),
        ));
    }
    Ok(())
}

/// Removes a link row and tells the live channel, before its replacement goes in.
async fn take(conn: &mut PgConnection, batch: &mut Batch, link: i64) -> Result<(), Error> {
    for facts in trackflow_store::live::link_facts(&mut *conn, &[link]).await? {
        batch.link_removed(facts);
    }
    links::delete(&mut *conn, link).await?;
    Ok(())
}

/// The project's priority field (§3.4): by built-in key first, else by name.
fn find_field(
    fields: &ProjectFields,
    language: trackflow_domain::settings::Language,
) -> Option<&ProjectField> {
    super::find_field(fields, &PRIORITY, language)
}
