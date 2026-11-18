//! Issue cloning (AUT-066, step 1.9 §3.10): a published copy at once, or the same copy as the
//! actor's draft. The `clone_issue` toggle of the source's project gates both; the copy takes
//! the summary with a «(copy)» suffix (the instance language), the description with its
//! rendering, every field value, the original's audience and the actor's own tags — and none of
//! the comments, files, links, votes, watchers or history. The creation record names the
//! original, and the stream renders «created by cloning X».

use sqlx::PgConnection;
use taskmaster_domain::IssueId;
use taskmaster_domain::access::Access;
use taskmaster_domain::issues as domain;
use taskmaster_domain::issues::history::{Origin, Record};
use taskmaster_domain::issues::rights;
use taskmaster_domain::rules::Rule;
use taskmaster_domain::settings::Language;
use taskmaster_domain::validation::Problem;
use taskmaster_markup::Profile;
use taskmaster_store::PgPool;
use taskmaster_store::issues::{self, Issue, NewDraft};
use taskmaster_store::projects::Project;
use taskmaster_store::rules as store_rules;
use taskmaster_store::tags as store_tags;

use super::{Answer, Ctx, instance_language};
use crate::issues::{self as issue_use, Described, One, values};
use crate::live::Batch;
use crate::{Error, Result};

/// Most issues cloned as drafts at once (§3.10).
pub const CLONE_MAX: usize = 50;

/// Clones one issue into a published copy of the same project (§3.10). Needs read of the source,
/// `issue.create` there and the project's `clone_issue` toggle on; the answer's message names
/// the copy so the toast can offer to open it.
pub async fn clone_issue(
    pool: &PgPool,
    actor: &Access,
    origin: Origin,
    source: IssueId,
) -> Result<Answer<One>> {
    let mut tx = pool.begin().await?;
    let found = issues::lock(&mut tx, source)
        .await?
        .ok_or(Error::NotFound("no such issue"))?;
    let (source, project) = issue_use::checked(&mut tx, actor, Some(found)).await?;
    let mut batch = Batch::new();
    let mut ctx = Ctx::new(actor);
    let id = clone_conn(
        &mut tx, &mut batch, &mut ctx, actor, origin, &source, &project,
    )
    .await?;
    batch.publish(&mut tx).await?;
    tx.commit().await?;
    let done = issue_use::get(pool, actor, id).await?;
    Ok(Answer {
        done,
        messages: ctx.messages,
    })
}

/// Clones a checked issue inside the caller's transaction (step 2.1 §4.3): everything
/// [`clone_issue`] does between its begin and its commit — the copy's publication events join
/// the caller's `batch`, its rule message the caller's `ctx`. Answers the copy's ID.
pub(crate) async fn clone_conn(
    tx: &mut PgConnection,
    batch: &mut Batch,
    ctx: &mut Ctx<'_>,
    actor: &Access,
    origin: Origin,
    source: &Issue,
    project: &Project,
) -> Result<IssueId> {
    if source.is_draft() {
        // Drafts are their reporter's alone; the clone menu lives on published issues.
        return Err(Error::NotFound("no such issue"));
    }
    gate(tx, actor, project).await?;
    let language = instance_language(tx).await?;
    let id = copy(tx, actor, source, project, language).await?;
    let draft = issues::get(&mut *tx, id)
        .await?
        .ok_or(Error::NotFound("no such issue"))?;
    issue_use::publish_locked(
        tx,
        batch,
        actor,
        origin,
        &draft,
        ctx,
        Record::created_of(source.id),
    )
    .await?;
    let made = issues::get(&mut *tx, id)
        .await?
        .ok_or(Error::NotFound("no such issue"))?;
    let made_id = made
        .number
        .map(|number| domain::readable_id(&project.key, number))
        .unwrap_or_default();
    ctx.say(Rule::CloneIssue, "clone_created", &[("issue", made_id)]);
    Ok(id)
}

/// Clones issues into the actor's drafts (§3.10): the same copies without numbers, one message
/// per source. Sources the actor cannot read are skipped, as everywhere issues are read in
/// bulk; a readable source whose project refuses cloning refuses the whole request.
pub async fn clone_drafts(
    pool: &PgPool,
    actor: &Access,
    sources: &[IssueId],
) -> Result<Answer<Described>> {
    let mut list = sources.to_vec();
    list.sort_unstable();
    list.dedup();
    if list.is_empty() {
        return Err(Error::field(
            "issues",
            Problem::Required,
            "ask for at least one issue",
        ));
    }
    if list.len() > CLONE_MAX {
        return Err(Error::field(
            "issues",
            Problem::TooLong,
            format!("at most {CLONE_MAX} issues at once"),
        ));
    }
    let mut tx = pool.begin().await?;
    let language = instance_language(&mut tx).await?;
    let mut ctx = Ctx::new(actor);
    let mut made: Vec<IssueId> = Vec::with_capacity(list.len());
    for id in &list {
        let Some(found) = issues::lock(&mut tx, *id).await? else {
            continue;
        };
        let (source, project) = match issue_use::checked(&mut tx, actor, Some(found)).await {
            Ok(pair) => pair,
            Err(Error::NotFound(_)) => continue,
            Err(other) => return Err(other),
        };
        if source.is_draft() {
            continue;
        }
        gate(&mut tx, actor, &project).await?;
        made.push(copy(&mut tx, actor, &source, &project, language).await?);
        let readable = source
            .number
            .map(|n| domain::readable_id(&project.key, n))
            .unwrap_or_default();
        ctx.say(Rule::CloneIssue, "draft_saved", &[("issue", readable)]);
    }
    tx.commit().await?;
    let done = if made.is_empty() {
        Described::default()
    } else {
        issue_use::describe_many(pool, actor, &made).await?
    };
    Ok(Answer {
        done,
        messages: ctx.messages,
    })
}

/// Whether the actor may clone in the project (§3.10): create right, an active project and the
/// `clone_issue` toggle on.
async fn gate(conn: &mut PgConnection, actor: &Access, project: &Project) -> Result<()> {
    rights::may_create(actor, project.id)?;
    crate::projects::require_active(project)?;
    let toggles = store_rules::of_project(&mut *conn, project.id).await?;
    if !toggles
        .iter()
        .any(|t| t.rule == Rule::CloneIssue && t.enabled)
    {
        return Err(Error::Refused(
            "issue cloning is off in this project".into(),
        ));
    }
    Ok(())
}

/// Makes the copy as a draft: values, audience and the actor's own tags, no watchers, files or
/// links. Publishing it — with a creation record naming the original — is the caller's step.
async fn copy(
    tx: &mut PgConnection,
    actor: &Access,
    source: &Issue,
    project: &Project,
    language: Language,
) -> Result<IssueId> {
    let fields = values::of_project(tx, project.id).await?;
    let held = values::of_issue(tx, source, &fields).await?;
    let suffix = match language {
        Language::Ru => " (копия)",
        Language::En => " (copy)",
    };
    let summary = domain::summary(&format!("{}{}", source.summary.trim(), suffix), true)
        .map_err(|r| Error::rejected("summary", r))?;
    // The actor's own tags on the original (ISS-037): shared ones belong to their owners.
    let own_tags: Vec<_> = store_tags::on_issues(&mut *tx, &[source.id], actor.user(), &[])
        .await?
        .into_iter()
        .filter(|(_, tag)| tag.owner_id == actor.user())
        .map(|(_, tag)| tag.id)
        .collect();
    let rendered = crate::text::prepare(tx, &source.description, Profile::Full).await?;
    let id = issues::insert_draft(
        tx,
        &NewDraft {
            project: project.id,
            reporter: actor.user(),
            summary: &summary,
            description: &source.description,
            rendered: &rendered,
            audience_users: &source.audience_users,
            audience_groups: &source.audience_groups,
        },
    )
    .await?;
    for (field, value) in &held {
        if let Some(f) = fields.get(*field) {
            values::store(tx, id, *field, f.kind.kind, Some(value)).await?;
        }
    }
    for tag in own_tags {
        store_tags::add(tx, id, tag).await?;
    }
    Ok(id)
}
