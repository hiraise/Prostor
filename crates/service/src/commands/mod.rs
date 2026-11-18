//! The command engine (step 2.1, plan §4): a command line parses against a dictionary of the
//! target projects' names (`dictionary`), resolves to a plan of typed operations with problems
//! over ranges of the text (`resolve`), and applies per issue in one transaction each (`apply`)
//! — a partial application within one issue never happens, and across issues each stands alone
//! with its own result. Every writer the apply reuses runs the project's built-in behaviors
//! inside the same transaction; problems ride the answers as `cmd_*` codes, never HTTP errors.

pub mod apply;
pub mod bulk;
pub mod dictionary;
pub mod resolve;

use std::collections::BTreeMap;

use prostor_domain::access::Access;
use prostor_domain::issues::history::Origin;
use prostor_domain::links::Side;
use prostor_domain::search::dates::Clock;
use prostor_domain::{
    BoardId, FieldId, GroupId, IssueId, LinkTypeId, ProjectId, SetId, SprintId, TagId, UserId,
};
use prostor_query::Span;
use prostor_query::command::{CommandCode, Mode};
use prostor_store::PgPool;
use sqlx::PgConnection;

use crate::Result;
use crate::issues::FieldChange;

/// How many issues one explicit apply takes (plan §4.7); more goes through the bulk job.
pub const ISSUES_MAX: usize = 50;

/// One command line to apply (plan §4.8); the endpoint of 2.1a-3 wraps this.
#[derive(Debug, Clone, Default)]
pub struct ApplyInput {
    pub text: String,
    pub issues: Vec<IssueId>,
    /// Markdown the `comment` command attaches (plan §6: the dialog's comment field).
    pub comment: Option<String>,
    /// The comment's audience; `None` is the issue's own.
    pub comment_visibility: Option<crate::issues::AudienceInput>,
    /// Without subscriber notifications; needs `issue.silent` in every affected project (§4.4).
    pub silent: bool,
    /// Make the create-value quick fixes before resolving the plan again (§4.2).
    pub create_missing: bool,
    /// The zone relative dates resolve in (§4.2); `None` is the instance's.
    pub time_zone: Option<String>,
    /// Apply as another account (§4.6, CMD-031): `system.write`, audited — only the permission
    /// checks answer as the named account, history keeps the acting user.
    pub run_as: Option<UserId>,
}

/// A party a visibility command names (§4.5): one account or a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Party {
    User(UserId),
    Group(GroupId),
}

/// One typed operation of the plan (plan §4.2), in command order. `SetField` carries user
/// fields too — a username resolves to the account and rides the same write, which is what
/// §4.2's separate `SetUser` names. The operations of §4.5 (move, links, duplicate,
/// visibility, boards, star, votes, delete, clone) resolve here and apply from step 2.1a-3.
#[derive(Debug, Clone, PartialEq)]
pub enum Operation {
    /// `field: value` and its `add`/`remove` forms; a plain set replaces the whole list on
    /// multi-value fields (owner, 2026-11-13). `set` is the attachment the value resolved
    /// against: one field attached with copied sets in several target projects resolves to one
    /// operation per set, and an issue runs only the one its project attaches.
    SetField {
        field: FieldId,
        set: Option<SetId>,
        change: FieldChange,
    },
    /// `tag`/`untag`, and a bare tag name.
    Tag {
        tag: TagId,
        on: bool,
    },
    Move {
        project: ProjectId,
    },
    /// The target stays text until 2.1a-3 resolves readable IDs (§4.5).
    Link {
        link: LinkTypeId,
        side: Side,
        target: String,
        remove: bool,
    },
    Duplicate {
        target: String,
    },
    Visibility {
        party: Party,
        mode: Mode,
    },
    Board {
        board: BoardId,
        sprint: Option<SprintId>,
        mode: Mode,
    },
    Star {
        user: Option<UserId>,
        on: bool,
    },
    Vote {
        on: bool,
    },
    Comment,
    Delete,
    Clone,
}

/// Creating a value the command named and the field lacks (plan §4.2, §5): the assist offers
/// it, `create_missing` makes it before the plan resolves again. Needs `project.update` in
/// every project sharing the set (§4.4), which `value_sets::add_value` checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickFix {
    /// The name as typed, trimmed: the value's title.
    pub name: String,
    pub field: FieldId,
    pub set: SetId,
    /// The projects sharing the set — those a creation updates.
    pub projects: Vec<ProjectId>,
}

/// A problem over a range of the command text: parse and resolution problems share this shape
/// (plan §4.2); the assist and the dialog translate the codes, the params say what each code
/// needs — the fields a bare value fits (`fields`), the classes a collision matched
/// (`matches`).
#[derive(Debug, Clone, PartialEq)]
pub struct PlanProblem {
    pub code: CommandCode,
    pub span: Span,
    pub params: BTreeMap<String, String>,
    pub quick_fix: Option<QuickFix>,
}

/// One operation of the plan with its source range (plan §4.2): where the command line
/// says it, for problems that name it and the preview's hover. `names` carries what the
/// operation names as they read — the field and its values, the tag, the project — for the
/// preview (plan §5).
#[derive(Debug, Clone, PartialEq)]
pub struct Planned {
    pub span: Span,
    pub operation: Operation,
    pub names: Vec<String>,
}

/// A command line resolved against the dictionary: typed operations in command order and the
/// problems that stop the apply (an apply with problems answers them and touches nothing).
#[derive(Debug, Clone, Default)]
pub struct Plan {
    pub operations: Vec<Planned>,
    pub problems: Vec<PlanProblem>,
}

impl Plan {
    pub fn has_errors(&self) -> bool {
        !self.problems.is_empty()
    }
}

/// Why one issue's apply failed: the same codes the plan uses, with the detail the web says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueProblem {
    pub code: CommandCode,
    pub detail: String,
}

/// One issue's result (plan §4.3): applied whole, or not at all.
#[derive(Debug, Clone)]
pub struct IssueOutcome {
    pub issue: IssueId,
    pub ok: bool,
    pub problem: Option<IssueProblem>,
    /// What the project's behaviors said (plan §3.11); the web turns each into a toast.
    pub messages: Vec<crate::rules::Message>,
}

/// The answer of [`apply`]: the plan the line resolved to — after any quick fixes — and one
/// result per issue, in the input's order.
#[derive(Debug, Clone)]
pub struct ApplyAnswer {
    pub plan: Plan,
    pub issues: Vec<IssueOutcome>,
}

/// Resolves a command line against the dictionary of the targets' projects: the plan the
/// dialog's preview shows (the assist of 2.1b reuses this).
pub async fn plan(pool: &PgPool, actor: &Access, input: &ApplyInput) -> Result<Plan> {
    check(input)?;
    let acting = acting_as(pool, actor, input).await?;
    let mut conn = pool.acquire().await?;
    let names = dictionary::load(&mut conn, &acting, &input.issues).await?;
    let clock = clock(&mut conn, input).await?;
    resolve::resolve(&mut conn, &input.text, &names, &acting, input, &clock).await
}

/// The access the call runs under (§4.6): `run_as` needs `system.write` and writes one audit
/// row naming both users; the named account's rights answer every permission check, while
/// history and notifications keep the acting user (`Access::reattributed`).
pub(crate) async fn acting_as(pool: &PgPool, actor: &Access, input: &ApplyInput) -> Result<Access> {
    use prostor_domain::access::permission::Permission;
    let Some(named) = input.run_as else {
        return Ok(actor.clone());
    };
    if named == actor.user() {
        return Ok(actor.clone());
    }
    actor.require(Permission::SystemWrite)?;
    let mut conn = pool.acquire().await?;
    prostor_store::audit::record(
        &mut conn,
        prostor_domain::Actor::User(actor.user()),
        "commands.run_as",
        &format!("user:{named}"),
        serde_json::json!({
            "acting": actor.user(),
            "as": named,
            "text": input.text,
        }),
    )
    .await
    .map_err(crate::Error::from)?;
    let as_if = crate::access::load(pool, named).await?;
    Ok(as_if.reattributed(actor.user()))
}

/// The clock relative dates resolve in (§4.2): the caller's zone, else the instance's.
pub(crate) async fn clock(conn: &mut PgConnection, input: &ApplyInput) -> Result<Clock> {
    let search = crate::search::SearchInput {
        time_zone: input.time_zone.clone(),
        ..crate::search::SearchInput::default()
    };
    crate::search::clock(conn, &search).await
}

/// Applies a command line to its issues (plan §4.3): the plan first — with problems it answers
/// them and touches nothing — then, when asked, the create-value quick fixes and a fresh plan,
/// and one transaction per issue.
pub async fn apply(
    pool: &PgPool,
    actor: &Access,
    origin: Origin,
    input: &ApplyInput,
) -> Result<ApplyAnswer> {
    check(input)?;
    let acting = acting_as(pool, actor, input).await?;
    let mut conn = pool.acquire().await?;
    let mut names = dictionary::load(&mut conn, &acting, &input.issues).await?;
    let clock = clock(&mut conn, input).await?;
    let mut resolved =
        resolve::resolve(&mut conn, &input.text, &names, &acting, input, &clock).await?;
    if input.create_missing && resolved.has_errors() {
        // Every quick fix whose creation succeeds turns its problem away; one that fails (the
        // name taken, the permission lacking) leaves the problem standing.
        let fixes: Vec<QuickFix> = resolved
            .problems
            .iter()
            .filter_map(|problem| problem.quick_fix.clone())
            .collect();
        if !fixes.is_empty() {
            for fix in &fixes {
                // A creation that fails (the name taken, the permission lacking, §4.4) leaves
                // its problem standing; the apply then answers the problems.
                let _ = apply::create_value(pool, &acting, fix).await;
            }
            names = dictionary::load(&mut conn, &acting, &input.issues).await?;
            resolved =
                resolve::resolve(&mut conn, &input.text, &names, &acting, input, &clock).await?;
        }
    }
    let issues = if resolved.has_errors() {
        input
            .issues
            .iter()
            .map(|issue| IssueOutcome {
                issue: *issue,
                ok: false,
                problem: None,
                messages: Vec::new(),
            })
            .collect()
    } else {
        let mut out = Vec::with_capacity(input.issues.len());
        for issue in &input.issues {
            out.push(
                apply::apply_issue(pool, &acting, origin, *issue, &resolved.operations, input)
                    .await,
            );
        }
        out
    };
    // The history keeps the lines that did something (plan §6): at least one issue got the
    // command. The acting user, not `run_as` (§4.6).
    if issues.iter().any(|outcome| outcome.ok) {
        let mut conn = pool.acquire().await?;
        prostor_store::command_history::record(&mut conn, actor.user(), input.text.trim())
            .await
            .map_err(crate::Error::from)?;
    }
    Ok(ApplyAnswer {
        plan: resolved,
        issues,
    })
}

fn check(input: &ApplyInput) -> Result<()> {
    if input.issues.is_empty() {
        return Err(crate::Error::field(
            "issues",
            prostor_domain::validation::Problem::Required,
            "name the issues to apply to",
        ));
    }
    if input.issues.len() > ISSUES_MAX {
        return Err(crate::Error::field(
            "issues",
            prostor_domain::validation::Problem::OutOfRange,
            format!("at most {ISSUES_MAX} issues in one command"),
        ));
    }
    Ok(())
}
