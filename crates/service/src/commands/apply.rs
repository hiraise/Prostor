//! Apply (plan §4.3): one transaction per issue, the operations in the fixed order — move
//! first, then values, links, tags, visibility, boards, star, vote, the comment, delete and
//! clone last — with the project's built-in behaviors inside the same transaction and one live
//! publish. A failure rolls that issue back whole and becomes its result; across issues each
//! stands alone. Silence skips the subscriber-notify rows (`issue.silent` in the project, the
//! author check), never what the rules sent.

use crate::issues::values::ProjectFields;
use sqlx::PgConnection;
use taskmaster_domain::access::Access;
use taskmaster_domain::issues::history::{ChangeKind, Origin};
use taskmaster_domain::issues::rights::{self, VoteRefusal};
use taskmaster_domain::links::{BuiltinType, Side};
use taskmaster_domain::validation::Problem;
use taskmaster_domain::{FieldId, IssueId, SetId};
use taskmaster_query::command::{CommandCode, Mode};
use taskmaster_store::PgPool;
use taskmaster_store::links;

use super::{ApplyInput, IssueOutcome, IssueProblem, Operation, Party, Planned, QuickFix};
use crate::boards;
use crate::comments::{self, NewComment};
use crate::issue_links;
use crate::issues;
use crate::live::Batch;
use crate::rules::clone::clone_conn;
use crate::rules::{self, Ctx, Event};
use crate::tags;
use crate::{Error, Result};

/// Applies one issue's share of the plan (plan §4.3). The plan is error-free by contract; an
/// operation the model of this step cannot carry yet is an internal refusal, replaced when
/// 2.1a-3 lands (§4.5).
pub(crate) async fn apply_issue(
    pool: &PgPool,
    actor: &Access,
    origin: Origin,
    issue: IssueId,
    operations: &[Planned],
    input: &ApplyInput,
) -> IssueOutcome {
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(error) => return refused(issue, error.into()),
    };
    let applying = Applying {
        actor,
        origin,
        input,
    };
    let mut batch = Batch::new();
    let mut ctx = Ctx::new(actor);
    match within(&mut tx, &mut batch, &mut ctx, &applying, issue, operations).await {
        Ok(()) => {
            if let Err(error) = batch.publish(&mut tx).await {
                return refused(issue, error);
            }
            if let Err(error) = tx.commit().await {
                return refused(issue, error.into());
            }
            IssueOutcome {
                issue,
                ok: true,
                problem: None,
                messages: ctx.messages,
            }
        }
        Err(problem) => IssueOutcome {
            issue,
            ok: false,
            problem: Some(problem),
            messages: Vec::new(),
        },
    }
}

/// Who applies and with what parameters: every operation's share of one call.
struct Applying<'a> {
    actor: &'a Access,
    origin: Origin,
    input: &'a ApplyInput,
}

/// The issue's change, all of it or none: every writer runs inside the caller's transaction,
/// the records of the operations join one history change, and the behaviors' records follow as
/// their own (plan §3.5's join, one change per issue).
async fn within(
    conn: &mut PgConnection,
    batch: &mut Batch,
    ctx: &mut Ctx<'_>,
    call: &Applying<'_>,
    issue: IssueId,
    operations: &[Planned],
) -> std::result::Result<(), IssueProblem> {
    let actor = call.actor;
    let input = call.input;
    let (mut issue, mut project) = issues::lock_for_change(conn, actor, issue)
        .await
        .map_err(failure)?;
    let mut who =
        issues::author(actor, call.origin, input.silent, issue.project_id).map_err(failure)?;
    let mut fields = issues::values::of_project(conn, issue.project_id)
        .await
        .map_err(failure)?;
    let mut own = Vec::new();
    let mut rules_said = Vec::new();
    let mut mentions = Vec::new();
    for Planned { operation: op, .. } in ordered(operations) {
        match op {
            Operation::SetField { field, set, change } => {
                if !applies_here(&fields, *field, *set) {
                    continue; // another target project's attachment of the same field name
                }
                let written = issues::set_field_locked(conn, actor, &issue, *field, change.clone())
                    .await
                    .map_err(failure)?;
                if !written.is_empty() {
                    own.extend(written.records.iter().cloned());
                    rules_said.extend(
                        rules::on(
                            conn,
                            batch,
                            ctx,
                            Event::FieldChanged {
                                issue: &issue,
                                field: &written.field,
                                old: written.old.as_ref(),
                                new: written.new.as_ref(),
                            },
                        )
                        .await
                        .map_err(failure)?,
                    );
                    rules_said.extend(
                        rules::on_resolution(conn, batch, ctx, &issue, &written.records)
                            .await
                            .map_err(failure)?,
                    );
                }
            }
            Operation::Tag { tag, on } => {
                if let Some(record) = tags::set_on_issue_conn(conn, actor, &issue, *tag, *on)
                    .await
                    .map_err(failure)?
                {
                    own.push(record);
                }
            }
            Operation::Comment => {
                let planted = comments::create_conn(
                    conn,
                    batch,
                    ctx,
                    actor,
                    &issue,
                    NewComment {
                        text: input.comment.clone().unwrap_or_default(),
                        audience: input.comment_visibility.clone(),
                        silent: input.silent,
                        uploads: Vec::new(),
                    },
                )
                .await
                .map_err(failure)?;
                own.extend(planted.records);
                mentions.extend(planted.mentions);
                rules_said.extend(planted.rule_records);
            }
            Operation::Move { project: target } => {
                issues::moves::move_locked(
                    conn,
                    batch,
                    ctx,
                    actor,
                    call.origin,
                    (&issue, &project),
                    *target,
                    input.silent,
                )
                .await
                .map_err(failure)?;
                // The later operations apply to the moved issue (§4.3): its project, its
                // fields and the author's silence check read anew.
                (issue, project) = issues::lock_for_change(conn, actor, issue.id)
                    .await
                    .map_err(failure)?;
                who = issues::author(actor, call.origin, input.silent, issue.project_id)
                    .map_err(failure)?;
                fields = issues::values::of_project(conn, issue.project_id)
                    .await
                    .map_err(failure)?;
            }
            Operation::Link {
                link,
                side,
                target,
                remove,
            } => {
                let (other, _) = issues::resolve_conn(conn, actor, target)
                    .await
                    .map_err(failure)?;
                if *remove {
                    let removed = issue_links::remove_conn(
                        conn,
                        batch,
                        ctx,
                        actor,
                        call.origin,
                        &issue,
                        *link,
                        other,
                        input.silent,
                    )
                    .await
                    .map_err(failure)?;
                    if !removed {
                        return Err(failure(Error::NotFound("no such link")));
                    }
                } else {
                    issue_links::add_conn(
                        conn,
                        batch,
                        ctx,
                        actor,
                        call.origin,
                        &issue,
                        issue_links::NewLinks {
                            link_type: *link,
                            side: *side,
                            issues: vec![other],
                        },
                        input.silent,
                    )
                    .await
                    .map_err(failure)?;
                }
            }
            Operation::Duplicate { target } => {
                let (other, _) = issues::resolve_conn(conn, actor, target)
                    .await
                    .map_err(failure)?;
                // The built-in duplicate link alone (§4.2): this issue is the duplicate, the
                // other end its original, and the built-in handling of AUT-071 — listening on
                // the same event — raises the priority and sets the «Дубликат» state inside
                // this transaction (its rule 5 refuses the state without the link).
                let duplicate = links::type_by_builtin(&mut *conn, BuiltinType::Duplicate.key())
                    .await
                    .map_err(|error| failure(Error::from(error)))?
                    .map(|found| found.id)
                    .ok_or_else(|| failure(Error::NotFound("no such link type")))?;
                issue_links::add_conn(
                    conn,
                    batch,
                    ctx,
                    actor,
                    call.origin,
                    &issue,
                    issue_links::NewLinks {
                        link_type: duplicate,
                        side: Side::Target,
                        issues: vec![other],
                    },
                    input.silent,
                )
                .await
                .map_err(failure)?;
            }
            Operation::Visibility { party, mode } => {
                let mut users = issue.audience_users.clone();
                let mut groups = issue.audience_groups.clone();
                match mode {
                    // A plain `visible to` replaces the whole audience (owner, 2026-11-13).
                    Mode::Set => {
                        users.clear();
                        groups.clear();
                        match party {
                            Party::User(id) => users.push(*id),
                            Party::Group(id) => groups.push(*id),
                        }
                    }
                    Mode::Add => match party {
                        Party::User(id) => {
                            if !users.contains(id) {
                                users.push(*id);
                            }
                        }
                        Party::Group(id) => {
                            if !groups.contains(id) {
                                groups.push(*id);
                            }
                        }
                    },
                    Mode::Remove => match party {
                        Party::User(id) => users.retain(|user| user != id),
                        Party::Group(id) => groups.retain(|group| group != id),
                    },
                }
                let audience = issues::AudienceInput { users, groups };
                if let Some(record) = issues::set_visibility_conn(conn, actor, &issue, &audience)
                    .await
                    .map_err(failure)?
                {
                    own.push(record);
                    issue.audience_users = audience.users;
                    issue.audience_groups = audience.groups;
                }
            }
            Operation::Board {
                board,
                sprint,
                mode,
            } => {
                let outcome = boards::cards::membership(
                    conn,
                    batch,
                    ctx,
                    actor,
                    call.origin,
                    *board,
                    *sprint,
                    &issue,
                    *mode != Mode::Remove,
                )
                .await
                .map_err(failure)?;
                if let boards::cards::Membership::NoSprints = outcome {
                    return Err(IssueProblem {
                        code: CommandCode::CmdBoardNoSprints,
                        detail: "the board has no sprint this command could mean".to_owned(),
                    });
                }
            }
            Operation::Star { user, on } => {
                // For oneself, watching needs only reading it; another account needs
                // `issue.watchers.update` (§4.4).
                let watcher = match user {
                    Some(named) => {
                        rights::may_manage_watchers(actor, issue.project_id)
                            .map_err(Error::from)
                            .map_err(failure)?;
                        *named
                    }
                    None => actor.user(),
                };
                issues::set_star_conn(conn, &issue, *on, watcher)
                    .await
                    .map_err(failure)?;
            }
            Operation::Vote { on } => {
                match rights::may_vote(actor, &issues::facts(&issue)) {
                    Err(VoteRefusal::OwnIssue) => {
                        return Err(IssueProblem {
                            code: CommandCode::CmdOwnIssueVote,
                            detail: "you cannot vote for an issue you reported".to_owned(),
                        });
                    }
                    Err(VoteRefusal::Draft) => {
                        return Err(failure(Error::Conflict("a draft takes no votes")));
                    }
                    Ok(()) => {}
                }
                issues::set_vote_conn(conn, actor, &issue, *on)
                    .await
                    .map_err(failure)?;
            }
            Operation::Delete => {
                issues::delete_conn(conn, batch, actor, call.origin, &issue)
                    .await
                    .map_err(failure)?;
            }
            Operation::Clone => {
                clone_conn(conn, batch, ctx, actor, call.origin, &issue, &project)
                    .await
                    .map_err(failure)?;
            }
        }
    }
    issues::finish_change(conn, batch, &issue, who, &own, &mentions)
        .await
        .map_err(failure)?;
    rules::finish(conn, batch, ctx, &issue, &rules_said)
        .await
        .map_err(failure)?;
    // A behavior's own write may have settled the issue's resolution (the mirror of
    // `set_field`); its record joined `rules_said`, so the event fires for it here.
    if rules_said
        .iter()
        .any(|record| matches!(record.kind, ChangeKind::Resolved | ChangeKind::Reopened))
    {
        rules::on_resolution(conn, batch, ctx, &issue, &rules_said)
            .await
            .map_err(failure)?;
    }
    Ok(())
}

/// The plan's order of application (§4.3): move first — later operations apply to the moved
/// issue — then values, links, tags, visibility, boards, star, vote, the comment, and delete
/// and clone last. Within one class, command order.
fn ordered(operations: &[Planned]) -> Vec<&Planned> {
    fn rank(planned: &Planned) -> u8 {
        let operation = &planned.operation;
        match operation {
            Operation::Move { .. } => 0,
            Operation::SetField { .. } => 1,
            Operation::Link { .. } => 2,
            Operation::Tag { .. } => 3,
            Operation::Visibility { .. } => 4,
            Operation::Board { .. } => 5,
            Operation::Star { .. } => 6,
            Operation::Vote { .. } => 7,
            Operation::Comment => 8,
            Operation::Delete | Operation::Duplicate { .. } => 9,
            Operation::Clone => 10,
        }
    }
    let mut out: Vec<&Planned> = operations.iter().collect();
    out.sort_by_key(|planned| rank(planned));
    out
}

/// Whether this issue's project attaches the field — and, when the operation resolved against
/// one set copy, that set.
fn applies_here(fields: &ProjectFields, field: FieldId, set: Option<SetId>) -> bool {
    fields
        .get(field)
        .is_some_and(|f| set.is_none_or(|s| f.attached.set_id == Some(s)))
}

/// A service error becomes the issue's result: the same `cmd_*` codes the plan uses (§4.4's
/// "per-issue results"), with the error's own words as the detail.
fn failure(error: Error) -> IssueProblem {
    let code = match &error {
        Error::NotFound(_) => CommandCode::CmdTargetNotFound,
        Error::Forbidden(_) | Error::Refused(_) => CommandCode::CmdNoPermission,
        Error::Invalid(problems) => {
            let not_empty = problems
                .iter()
                .any(|problem| problem.path == "value" && problem.problem == Problem::Required);
            if not_empty {
                CommandCode::CmdNotEmpty
            } else {
                CommandCode::CmdWrongKind
            }
        }
        _ => CommandCode::CmdWrongKind,
    };
    IssueProblem {
        code,
        detail: error.to_string(),
    }
}

fn refused(issue: IssueId, error: Error) -> IssueOutcome {
    IssueOutcome {
        issue,
        ok: false,
        problem: Some(failure(error)),
        messages: Vec::new(),
    }
}

/// Makes one create-value quick fix (§4.2): the same use case the dialog's button runs, with
/// its own permission check — `project.update` in every project sharing the set (§4.4).
pub(crate) async fn create_value(pool: &PgPool, actor: &Access, fix: &QuickFix) -> Result<()> {
    crate::value_sets::add_value(
        pool,
        actor,
        fix.set,
        crate::value_sets::ValueInput {
            name: Some(fix.name.clone()),
            ..crate::value_sets::ValueInput::default()
        },
        None,
    )
    .await
    .map(|_| ())
}
