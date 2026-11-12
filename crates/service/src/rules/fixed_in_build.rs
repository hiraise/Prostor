//! AUT-098 (step 1.9 §3.7): the build an issue was fixed in follows its state and its
//! duplicates. A reopened issue loses the build it held; an issue whose state names it a
//! duplicate takes the build of its original — the last original that has one; a build value set
//! by hand spreads to the issue's duplicates. Silent — the records speak — and always through
//! [`super::write`].

use sqlx::PgConnection;
use trackflow_domain::fields::FieldKind;
use trackflow_domain::issues::values::Value;
use trackflow_domain::rules::{DUPLICATE, FIXED_IN_BUILD, Rule};
use trackflow_store::issues::Issue;
use trackflow_store::{issues, links};

use crate::Error;
use crate::issues::values::{self, ProjectField, ProjectFields};
use crate::live::Batch;
use crate::rules::{Ctx, Event};

pub async fn fixed_in_build(
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
    let Some(build) = find_field(&fields, ctx.language) else {
        return Ok(());
    };
    match event {
        // Reopened: the build it was fixed in says nothing anymore.
        Event::BecameUnresolved { .. } => {
            super::write(
                &mut *conn,
                batch,
                ctx,
                issue,
                build,
                None,
                Rule::FixedInBuild,
            )
            .await?;
            Ok(())
        }
        // The state names it a duplicate of its original (§3.8): it takes the original's build.
        Event::FieldChanged { field, new, .. } if field.kind.kind == FieldKind::State => {
            if !is_duplicate_state(&mut *conn, ctx, field, *new).await? {
                return Ok(());
            }
            copy_from_originals(&mut *conn, batch, ctx, issue, &fields, build).await
        }
        // A build value set by hand spreads to the issue's duplicates of the same project.
        Event::FieldChanged {
            field,
            new: Some(value),
            ..
        } if field.id() == build.id() => {
            spread_to_duplicates(&mut *conn, batch, ctx, issue, build, value).await
        }
        _ => Ok(()),
    }
}

/// Whether a state change names the duplicate value «Дубликат»/Duplicate (§3.4): by name, in
/// either language, case-insensitively.
async fn is_duplicate_state(
    conn: &mut PgConnection,
    ctx: &mut Ctx<'_>,
    field: &ProjectField,
    new: Option<&Value>,
) -> Result<bool, Error> {
    let Some(value) = new.and_then(|value| value.refs().first().copied()) else {
        return Ok(false);
    };
    let Some(set) = field.attached.set_id else {
        return Ok(false);
    };
    Ok(ctx
        .set_values(&mut *conn, set)
        .await?
        .iter()
        .find(|held| held.id == value)
        .is_some_and(|held| {
            held.name.to_lowercase() == DUPLICATE.en.to_lowercase()
                || held.name.to_lowercase() == DUPLICATE.ru.to_lowercase()
        }))
}

/// Copies the build value of the issue's originals onto it (§3.7): the sources of its duplicate
/// links in the same project, published and alive; of those that hold a build value, the last
/// one wins. None does: nothing to copy.
async fn copy_from_originals(
    conn: &mut PgConnection,
    batch: &mut Batch,
    ctx: &mut Ctx<'_>,
    issue: &Issue,
    fields: &ProjectFields,
    build: &ProjectField,
) -> Result<(), Error> {
    let type_id = ctx.duplicate_type(&mut *conn).await?;
    let all = links::of_issue(&mut *conn, issue.id).await?;
    // Newest last: of the originals that hold a build value, the last one wins.
    let mut originals: Vec<&links::End> = all
        .iter()
        .filter(|end| end.type_id == type_id && !end.is_source)
        .filter(|end| alive_in(end, issue))
        .collect();
    originals.sort_by_key(|end| end.created_at);
    for end in originals.iter().rev() {
        let Some(original) = issues::get(&mut *conn, end.other.id).await? else {
            continue;
        };
        let held = values::of_issue(&mut *conn, &original, fields).await?;
        if let Some(value) = held.get(&build.id()) {
            super::write(
                &mut *conn,
                batch,
                ctx,
                issue,
                build,
                Some(value),
                Rule::FixedInBuild,
            )
            .await?;
            return Ok(());
        }
    }
    Ok(())
}

/// Spreads a build value to the issue's duplicates (§3.7): the targets of its duplicate links in
/// the same project, published and alive, each through one write.
async fn spread_to_duplicates(
    conn: &mut PgConnection,
    batch: &mut Batch,
    ctx: &mut Ctx<'_>,
    issue: &Issue,
    build: &ProjectField,
    value: &Value,
) -> Result<(), Error> {
    let type_id = ctx.duplicate_type(&mut *conn).await?;
    let mut targets: Vec<_> = links::of_issue(&mut *conn, issue.id)
        .await?
        .iter()
        .filter(|end| end.type_id == type_id && end.is_source)
        .filter(|end| alive_in(end, issue))
        .map(|end| end.other.id)
        .collect();
    targets.sort_unstable();
    targets.dedup();
    for duplicate in issues::get_many(&mut *conn, &targets).await? {
        super::write(
            &mut *conn,
            batch,
            ctx,
            &duplicate,
            build,
            Some(value),
            Rule::FixedInBuild,
        )
        .await?;
    }
    Ok(())
}

/// The other end of a duplicate link the rule reaches: published, alive and in the same project
/// — the rule is the issue's project's, and cross-project copies are not ours to make.
fn alive_in(end: &links::End, issue: &Issue) -> bool {
    end.other.project_id == issue.project_id
        && end.other.number.is_some()
        && end.other.deleted_at.is_none()
}

/// The project's build field (§3.4): by built-in key first, else by name.
fn find_field(
    fields: &ProjectFields,
    language: trackflow_domain::settings::Language,
) -> Option<&ProjectField> {
    super::find_field(fields, &FIXED_IN_BUILD, language)
}
