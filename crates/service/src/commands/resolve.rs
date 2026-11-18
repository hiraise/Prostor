//! Resolution (plan §4.2): the parse of a command line becomes a [`Plan`] of typed operations,
//! each name resolved to an ID against the dictionary, every failure a problem over a range of
//! the text — `cmd_*` codes, never exceptions. Bare values resolve in one fixed order:
//! command word (the parser's), attribute (the parser's), link name (the parser's), project
//! (moves), tag (adds), username (assigns), field value; a project and a tag colliding is
//! `cmd_ambiguous` — never a guess.

use std::collections::BTreeMap;

use prostor_domain::access::Access;
use prostor_domain::duration::Schedule;
use prostor_domain::fields::{FieldKind, FieldType};
use prostor_domain::issues::values::{self as rules, Input};
use prostor_domain::links::Side;
use prostor_domain::search::dates::Clock;
use prostor_domain::{ProjectId, SetId};
use prostor_query::command::{self, Command, CommandCode, Mode};
use prostor_query::words::{self, fold};
use prostor_query::{Atom, Span};
use prostor_store::links;
use prostor_store::value_sets;
use sqlx::PgConnection;

use super::dictionary::{NamedField, Names};
use super::{Operation, Party, Plan, PlanProblem, Planned, QuickFix};
use crate::issues::FieldChange;
use crate::{Error, Result};

/// The value keyword that names the actor (`me`, «я»).
const ME: &str = "me";

/// What every command of one line resolves against (plan §4.2).
struct Resolving<'a> {
    plan: &'a mut Plan,
    names: &'a Names,
    actor: &'a Access,
    clock: &'a Clock,
}

/// Resolves the line. Pure over its arguments except one read: the create-value quick fix
/// names the projects sharing the set, which the dictionary's scope does not cover.
pub(crate) async fn resolve(
    conn: &mut PgConnection,
    text: &str,
    names: &Names,
    actor: &Access,
    input: &super::ApplyInput,
    clock: &Clock,
) -> Result<Plan> {
    let list = command::parse_commands(text, &names.parser);
    let mut plan = Plan::default();
    for problem in &list.problems {
        plan.problems.push(PlanProblem {
            code: problem.code,
            span: problem.span,
            params: BTreeMap::new(),
            quick_fix: None,
        });
    }
    for parsed in &list.commands {
        let mut resolving = Resolving {
            plan: &mut plan,
            names,
            actor,
            clock,
        };
        resolve_command(&mut resolving, parsed, input, conn).await?;
    }
    permissions(&mut plan, names, actor, input);
    Ok(plan)
}

fn problem(plan: &mut Plan, code: CommandCode, span: Span) {
    plan.problems.push(PlanProblem {
        code,
        span,
        params: BTreeMap::new(),
        quick_fix: None,
    });
}

fn problem_with(plan: &mut Plan, code: CommandCode, span: Span, params: BTreeMap<String, String>) {
    plan.problems.push(PlanProblem {
        code,
        span,
        params,
        quick_fix: None,
    });
}

fn one_param(key: &str, value: impl Into<String>) -> BTreeMap<String, String> {
    BTreeMap::from([(key.to_owned(), value.into())])
}

/// Permissions knowable before applying (§4.4): a command no target project allows is a plan
/// problem — the actor could not apply it anywhere — while anything that depends on the issue
/// (its reporter, its audience, a draft) fails per issue at the writer's own checks.
fn permissions(plan: &mut Plan, names: &Names, actor: &Access, input: &super::ApplyInput) {
    use prostor_domain::access::permission::Permission;
    let targets = names.target_projects.clone();
    // The update right, or the reporter's own path to it (ISS-022, ACC-047).
    let may_update = |project: ProjectId| {
        actor.has_in(Permission::IssueUpdate, project)
            || actor.has_in(Permission::IssueCreate, project)
    };
    for Planned {
        span, operation, ..
    } in plan.operations.clone()
    {
        match operation {
            Operation::SetField { field, .. } => {
                let Some(found) = names.fields.iter().find(|f| f.id == field) else {
                    continue;
                };
                if targets.iter().all(|p| !may_update(*p)) {
                    problem_with(
                        plan,
                        CommandCode::CmdNoPermission,
                        span,
                        one_param("permission", Permission::IssueUpdate.key()),
                    );
                } else if found.attachments.iter().all(|a| a.private)
                    && targets
                        .iter()
                        .all(|p| !actor.has_in(Permission::IssuePrivateUpdate, *p))
                {
                    problem_with(
                        plan,
                        CommandCode::CmdNoPermission,
                        span,
                        one_param("permission", Permission::IssuePrivateUpdate.key()),
                    );
                }
            }
            Operation::Comment
                if targets
                    .iter()
                    .all(|p| !actor.has_in(Permission::CommentCreate, *p)) =>
            {
                problem_with(
                    plan,
                    CommandCode::CmdNoPermission,
                    span,
                    one_param("permission", Permission::CommentCreate.key()),
                );
            }
            _ => {}
        }
    }
    if input.silent
        && targets
            .iter()
            .all(|p| !actor.has_in(Permission::IssueSilent, *p))
    {
        problem_with(
            plan,
            CommandCode::CmdNoPermission,
            Span::default(),
            one_param("permission", Permission::IssueSilent.key()),
        );
    }
}

/// One operation of the plan, over the command that said it, with what it names as it reads.
fn push_op(plan: &mut Plan, span: Span, operation: Operation, names: Vec<String>) {
    plan.operations.push(Planned {
        span,
        operation,
        names,
    });
}

fn join(parts: &[String]) -> String {
    parts.join(", ")
}

async fn resolve_command(
    resolving: &mut Resolving<'_>,
    parsed: &Command,
    input: &super::ApplyInput,
    conn: &mut PgConnection,
) -> Result<()> {
    let (plan, names) = (&mut *resolving.plan, resolving.names);
    match parsed {
        Command::Error(_) => {} // the parse problems already carry it
        Command::Comment(span) => {
            if input
                .comment
                .as_deref()
                .is_none_or(|text| text.trim().is_empty())
            {
                problem(plan, CommandCode::CmdValueExpected, *span);
            }
            plan.operations.push(Planned {
                span: parsed.span(),
                operation: Operation::Comment,
                names: Vec::new(),
            });
        }
        Command::Delete(_) => plan.operations.push(Planned {
            span: parsed.span(),
            operation: Operation::Delete,
            names: Vec::new(),
        }),
        Command::Clone(_) => plan.operations.push(Planned {
            span: parsed.span(),
            operation: Operation::Clone,
            names: Vec::new(),
        }),
        Command::Move { project, .. } => match unique(names.projects_named(&fold(&project.text))) {
            Unique::One(found) => plan.operations.push(Planned {
                span: parsed.span(),
                operation: Operation::Move { project: found.id },
                names: vec![found.name.clone()],
            }),
            Unique::None_ => problem(plan, CommandCode::CmdUnknownValue, project.span),
            Unique::Many => problem_with(
                plan,
                CommandCode::CmdAmbiguous,
                project.span,
                one_param("matches", "projects"),
            ),
        },
        Command::Tag { name, add, .. } => match unique(names.tags_named(&fold(&name.text))) {
            Unique::One(found) => plan.operations.push(Planned {
                span: parsed.span(),
                operation: Operation::Tag {
                    tag: found.id,
                    on: *add,
                },
                names: vec![found.display.clone()],
            }),
            Unique::None_ => problem(plan, CommandCode::CmdUnknownValue, name.span),
            Unique::Many => problem_with(
                plan,
                CommandCode::CmdAmbiguous,
                name.span,
                one_param("matches", "tags"),
            ),
        },
        Command::Star { user, on, .. } => {
            let found = match user {
                Some(atom) => match unique(names.users_named(&atom.text)) {
                    Unique::One(found) => Some(found.id),
                    Unique::None_ => {
                        problem(plan, CommandCode::CmdUnknownValue, atom.span);
                        None
                    }
                    Unique::Many => {
                        problem_with(
                            plan,
                            CommandCode::CmdAmbiguous,
                            atom.span,
                            one_param("matches", "accounts"),
                        );
                        None
                    }
                },
                None => None,
            };
            plan.operations.push(Planned {
                span: parsed.span(),
                operation: Operation::Star {
                    user: found,
                    on: *on,
                },
                names: found
                    .and_then(|id| user_display(resolving.names, id))
                    .into_iter()
                    .collect(),
            });
        }
        Command::Vote { on, .. } => plan.operations.push(Planned {
            span: parsed.span(),
            operation: Operation::Vote { on: *on },
            names: Vec::new(),
        }),
        Command::Visibility { party, mode, .. } => match party_of(names, party) {
            Some(party) => plan.operations.push(Planned {
                span: parsed.span(),
                operation: Operation::Visibility { party, mode: *mode },
                names: party_names(names, &party).into_iter().collect(),
            }),
            None => problem(plan, CommandCode::CmdUnknownValue, party.span),
        },
        Command::Board {
            board,
            sprint,
            mode,
            ..
        } => match unique(names.boards_named(&fold(&board.text))) {
            Unique::One(found) => {
                let sprint_id = match sprint {
                    Some(atom) => {
                        let folded = fold(&atom.text);
                        let sprint = names
                            .sprints_named(&folded)
                            .into_iter()
                            .find(|s| s.board == found.id)
                            .map(|s| s.id);
                        if sprint.is_none() {
                            problem(plan, CommandCode::CmdUnknownValue, atom.span);
                        }
                        sprint
                    }
                    None => None,
                };
                plan.operations.push(Planned {
                    span: parsed.span(),
                    operation: Operation::Board {
                        board: found.id,
                        sprint: sprint_id,
                        mode: *mode,
                    },
                    names: [
                        vec![found.display.clone()],
                        sprint_id
                            .and_then(|id| {
                                names
                                    .sprints
                                    .iter()
                                    .find(|s| s.id == id)
                                    .map(|s| s.display.clone())
                            })
                            .into_iter()
                            .collect(),
                    ]
                    .concat(),
                });
            }
            Unique::None_ => problem(plan, CommandCode::CmdUnknownValue, board.span),
            Unique::Many => problem_with(
                plan,
                CommandCode::CmdAmbiguous,
                board.span,
                one_param("matches", "boards"),
            ),
        },
        Command::Link {
            name,
            target,
            remove,
        } => match unique(names.links_named(&fold(&name.text))) {
            Unique::One(found) => {
                // The built-in duplicate word is its own command (§4.2): the «Дубликат» state
                // and the link in one — this issue is the duplicate, the target-side name
                // («дубликат», "duplicate of").
                let builtin = links::type_by_builtin(
                    conn,
                    prostor_domain::links::BuiltinType::Duplicate.key(),
                )
                .await
                .map_err(Error::from)?
                .map(|found_type| found_type.id);
                let operation =
                    if builtin == Some(found.link) && found.side == Side::Target && !*remove {
                        Operation::Duplicate {
                            target: target.text.clone(),
                        }
                    } else {
                        Operation::Link {
                            link: found.link,
                            side: found.side,
                            target: target.text.clone(),
                            remove: *remove,
                        }
                    };
                let link_names = match &operation {
                    Operation::Duplicate { target } => vec![target.clone()],
                    Operation::Link {
                        link, side, target, ..
                    } => [
                        names
                            .links
                            .iter()
                            .find(|l| l.link == *link && l.side == *side)
                            .map(|l| l.display.clone()),
                        Some(target.clone()),
                    ]
                    .into_iter()
                    .flatten()
                    .collect(),
                    _ => Vec::new(),
                };
                plan.operations.push(Planned {
                    span: parsed.span(),
                    operation,
                    names: link_names,
                });
            }
            Unique::None_ => problem(plan, CommandCode::CmdUnknownValue, name.span),
            Unique::Many => problem_with(
                plan,
                CommandCode::CmdAmbiguous,
                name.span,
                one_param("matches", "link names"),
            ),
        },
        Command::Set { field, value } => {
            set_command(resolving, conn, field, value, Mode::Set).await?;
        }
        Command::Field {
            field, value, mode, ..
        } => {
            set_command(resolving, conn, field, value, *mode).await?;
        }
        Command::BareValue { value, mode, .. } => bare_value(
            resolving.plan,
            resolving.names,
            resolving.actor,
            value,
            *mode,
        ),
        Command::Value(atom) => bare_value(
            resolving.plan,
            resolving.names,
            resolving.actor,
            atom,
            Mode::Set,
        ),
    }
    Ok(())
}

/// A user or a group a visibility command names; accounts first, groups second — a name both
/// carry is the account's (deterministic, like every collision order here).
fn party_of(names: &Names, atom: &Atom) -> Option<Party> {
    if let Unique::One(user) = unique(names.users_named(&atom.text)) {
        return Some(Party::User(user.id));
    }
    if let Unique::One(group) = unique(names.groups_named(&fold(&atom.text))) {
        return Some(Party::Group(group.id));
    }
    None
}

/// `field: value` and its `add`/`remove` forms (plan §4.2). A field name shared by several
/// fields — one per project — resolves for each; a project holding two of them is ambiguous.
async fn set_command(
    resolving: &mut Resolving<'_>,
    conn: &mut PgConnection,
    field: &Atom,
    value: &Atom,
    mode: Mode,
) -> Result<()> {
    let (plan, names) = (&mut *resolving.plan, resolving.names);
    let folded = fold(&field.text);
    let candidates = names.fields_named(&folded);
    if candidates.is_empty() {
        problem(plan, CommandCode::CmdUnknownValue, field.span);
        return Ok(());
    }
    if let Some(collisions) = field_collision(names, &folded) {
        problem_with(
            plan,
            CommandCode::CmdAmbiguous,
            field.span,
            one_param("fields", join(&collisions)),
        );
        return Ok(());
    }
    for candidate in candidates {
        field_value(resolving, conn, candidate, value, mode).await?;
    }
    Ok(())
}

/// Whether one target project attaches two fields the name fits: then the user must qualify.
fn field_collision(names: &Names, folded: &str) -> Option<Vec<String>> {
    for project in &names.target_projects {
        let here: Vec<&NamedField> = names
            .fields_named(folded)
            .into_iter()
            .filter(|f| f.attachments.iter().any(|a| a.project == *project))
            .collect();
        if here.len() > 1 {
            return Some(here.iter().map(|f| f.name.clone()).collect());
        }
    }
    None
}

/// One field and the value the atom names, for every set the field's attachments use.
async fn field_value(
    resolving: &mut Resolving<'_>,
    conn: &mut PgConnection,
    field: &NamedField,
    value: &Atom,
    mode: Mode,
) -> Result<()> {
    let (plan, names, actor, clock) = (
        &mut *resolving.plan,
        resolving.names,
        resolving.actor,
        resolving.clock,
    );
    // Computed and markdown fields no command writes.
    if !rules::editable(field.builtin.as_deref()) {
        problem(plan, CommandCode::CmdWrongKind, value.span);
        return Ok(());
    }
    if field.kind == FieldKind::Text {
        problem(plan, CommandCode::CmdTextField, value.span);
        return Ok(());
    }
    let folded = fold(&value.text);
    let cleared = clears(field, &folded);
    match field.kind {
        FieldKind::Enum
        | FieldKind::State
        | FieldKind::Version
        | FieldKind::Build
        | FieldKind::Owned => {
            for set in field.sets() {
                if let Some(value_id) = names.values_named(set, &folded).first().map(|v| v.id) {
                    push_ref(plan, names, field, Some(set), value_id, mode, value.span);
                } else if cleared {
                    push_clear(plan, field, Some(set), value.span);
                } else {
                    unknown_value(conn, plan, value, field, set).await?;
                }
            }
        }
        FieldKind::User => {
            if folded == ME {
                push_ref(plan, names, field, None, actor.user(), mode, value.span);
            } else if let Unique::One(user) = unique(names.users_named(&value.text)) {
                push_ref(plan, names, field, None, user.id, mode, value.span);
            } else if cleared {
                push_clear(plan, field, None, value.span);
            } else if names.users_named(&value.text).len() > 1 {
                problem_with(
                    plan,
                    CommandCode::CmdAmbiguous,
                    value.span,
                    one_param("matches", "accounts"),
                );
            } else {
                problem(plan, CommandCode::CmdUnknownValue, value.span);
            }
        }
        FieldKind::Group => {
            if let Unique::One(group) = unique(names.groups_named(&folded)) {
                push_ref(plan, names, field, None, group.id, mode, value.span);
            } else if cleared {
                push_clear(plan, field, None, value.span);
            } else {
                problem(plan, CommandCode::CmdUnknownValue, value.span);
            }
        }
        FieldKind::Text => {} // refused above, before any resolution
        FieldKind::Date
        | FieldKind::DateTime
        | FieldKind::String
        | FieldKind::Integer
        | FieldKind::Float
        | FieldKind::Duration => {
            if cleared {
                push_clear(plan, field, None, value.span);
                return Ok(());
            }
            match literal(field.kind, value, clock) {
                Some(text) => {
                    let shown = text.clone();
                    let input = Input::Text(text);
                    if rules::parse(field_type(field), Some(input.clone()), Schedule::default())
                        .is_err()
                    {
                        problem(plan, CommandCode::CmdWrongKind, value.span);
                    } else if mode == Mode::Set {
                        push_op(
                            plan,
                            value.span,
                            Operation::SetField {
                                field: field.id,
                                set: None,
                                change: FieldChange::Set(Some(input.clone())),
                            },
                            vec![field.name.clone(), shown],
                        );
                    } else {
                        // These fields hold one value; only a plain set writes them.
                        problem(plan, CommandCode::CmdWrongKind, value.span);
                    }
                }
                None => problem(plan, CommandCode::CmdWrongKind, value.span),
            }
        }
    }
    Ok(())
}

/// A reference onto a field: the mode against the field's shape, knowable before applying
/// (§4.4) — `add` and `remove` need a multi-value field.
fn push_ref(
    plan: &mut Plan,
    names: &Names,
    field: &NamedField,
    set: Option<SetId>,
    id: i64,
    mode: Mode,
    span: Span,
) {
    let change = match mode {
        Mode::Set => FieldChange::Set(Some(Input::Ids(vec![id]))),
        Mode::Add | Mode::Remove if !field.multi => {
            problem(plan, CommandCode::CmdWrongKind, span);
            return;
        }
        Mode::Add => FieldChange::Add(vec![id]),
        Mode::Remove => FieldChange::Remove(vec![id]),
    };
    push_op(
        plan,
        span,
        Operation::SetField {
            field: field.id,
            set,
            change,
        },
        [
            vec![field.name.clone()],
            ref_display(names, field, set, id).into_iter().collect(),
        ]
        .concat(),
    );
}

/// What one reference names, as it reads: a set value, an account, a group.
fn ref_display(names: &Names, field: &NamedField, set: Option<SetId>, id: i64) -> Option<String> {
    match field.kind {
        FieldKind::User => user_display(names, id),
        FieldKind::Group => names
            .groups
            .iter()
            .find(|g| g.id == id)
            .map(|g| g.display.clone()),
        _ => set.and_then(|set| {
            names
                .values
                .get(&set)
                .and_then(|values| values.iter().find(|v| v.id == id))
                .map(|v| v.display.clone())
        }),
    }
}

/// An account's full name where the plan names one.
fn user_display(names: &Names, id: i64) -> Option<String> {
    names
        .users
        .iter()
        .find(|u| u.id == id)
        .map(|u| u.full_name.clone())
}

/// A visibility party's name.
fn party_names(names: &Names, party: &Party) -> Option<String> {
    match party {
        Party::User(id) => user_display(names, *id),
        Party::Group(id) => names
            .groups
            .iter()
            .find(|g| g.id == *id)
            .map(|g| g.display.clone()),
    }
}

/// Clearing a field (§4.2): refused where the field may not be empty.
fn push_clear(plan: &mut Plan, field: &NamedField, set: Option<SetId>, span: Span) {
    let refused = field
        .attachments
        .iter()
        .filter(|a| set.is_none_or(|s| a.set == Some(s)))
        .all(|a| !a.can_be_empty);
    if refused {
        problem(plan, CommandCode::CmdNotEmpty, span);
        return;
    }
    push_op(
        plan,
        span,
        Operation::SetField {
            field: field.id,
            set,
            change: FieldChange::Set(None),
        },
        vec![field.name.clone()],
    );
}

/// A value the field lacks (plan §4.2, §5): the problem, and — for set kinds — the quick fix
/// that creates it, naming the projects sharing the set.
async fn unknown_value(
    conn: &mut PgConnection,
    plan: &mut Plan,
    atom: &Atom,
    field: &NamedField,
    set: SetId,
) -> Result<()> {
    let quick_fix = value_sets::usage(conn, set)
        .await
        .map_err(Error::from)
        .map(|usage| QuickFix {
            name: atom.text.trim().to_owned(),
            field: field.id,
            set,
            projects: usage.projects,
        })
        .ok();
    plan.problems.push(PlanProblem {
        code: CommandCode::CmdUnknownValue,
        span: atom.span,
        params: one_param("field", field.name.clone()),
        quick_fix,
    });
    Ok(())
}

/// The text a date, date-time, number, duration or string field stores: absolute, or — for
/// dates — the first moment of the word's range in the caller's zone (plan §4.2).
fn literal(kind: FieldKind, atom: &Atom, clock: &Clock) -> Option<String> {
    let text = atom.text.trim().to_owned();
    if matches!(kind, FieldKind::Date | FieldKind::DateTime)
        && let Some(word) = words::keyword(&fold(&text), &[words::Kind::Date])
        && let Some(period) = clock.word(word.key)
    {
        let start = period.start?;
        return Some(match kind {
            FieldKind::Date => start.date().to_string(),
            _ => clock.instant(start)?.to_string(),
        });
    }
    let parsed = rules::parse(
        FieldType { kind, multi: false },
        Some(Input::Text(text.clone())),
        Schedule::default(),
    );
    parsed.ok().map(|_| text)
}

/// A lone name, or one after `add`/`remove`: the fixed order of plan §4.2 — project (moves),
/// tag (adds), username (assigns), field value.
fn bare_value(plan: &mut Plan, names: &Names, actor: &Access, atom: &Atom, mode: Mode) {
    let folded = fold(&atom.text);
    let projects = names.projects_named(&folded);
    let tags = names.tags_named(&folded);
    if !projects.is_empty() && !tags.is_empty() {
        problem_with(
            plan,
            CommandCode::CmdAmbiguous,
            atom.span,
            one_param("matches", "a project and a tag"),
        );
        return;
    }
    if !projects.is_empty() {
        if projects.len() > 1 {
            problem_with(
                plan,
                CommandCode::CmdAmbiguous,
                atom.span,
                one_param("matches", "projects"),
            );
        } else if mode != Mode::Set {
            problem(plan, CommandCode::CmdWrongKind, atom.span);
        } else {
            push_op(
                plan,
                atom.span,
                Operation::Move {
                    project: projects[0].id,
                },
                vec![projects[0].name.clone()],
            );
        }
        return;
    }
    if !tags.is_empty() {
        if tags.len() > 1 {
            problem_with(
                plan,
                CommandCode::CmdAmbiguous,
                atom.span,
                one_param("matches", "tags"),
            );
        } else {
            push_op(
                plan,
                atom.span,
                Operation::Tag {
                    tag: tags[0].id,
                    on: mode != Mode::Remove,
                },
                vec![tags[0].display.clone()],
            );
        }
        return;
    }
    if folded == ME {
        assign(plan, names, atom, actor.user(), mode);
        return;
    }
    let users = names.users_named(&atom.text);
    if users.len() > 1 {
        problem_with(
            plan,
            CommandCode::CmdAmbiguous,
            atom.span,
            one_param("matches", "accounts"),
        );
        return;
    }
    if let Some(user) = users.first() {
        assign(plan, names, atom, user.id, mode);
        return;
    }
    // A value of any set-kind field in scope: exactly one field, or the user qualifies.
    let mut found: Vec<(&NamedField, SetId, i64)> = Vec::new();
    for field in &names.fields {
        if matches!(
            field.kind,
            FieldKind::Enum | FieldKind::State | FieldKind::Version | FieldKind::Build
        ) || field.kind == FieldKind::Owned
        {
            for set in field.sets() {
                if let Some(value) = names.values_named(set, &folded).first() {
                    found.push((field, set, value.id));
                }
            }
        }
    }
    let mut fields: Vec<&NamedField> = Vec::new();
    for (field, _, _) in &found {
        if !fields.iter().any(|f| f.id == field.id) {
            fields.push(field);
        }
    }
    let field_names: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
    match fields.len() {
        0 => problem(plan, CommandCode::CmdUnknownValue, atom.span),
        1 => {
            for (field, set, id) in found {
                push_ref(plan, names, field, Some(set), id, mode, atom.span);
            }
        }
        _ => problem_with(
            plan,
            CommandCode::CmdAmbiguous,
            atom.span,
            one_param("fields", join(&field_names)),
        ),
    }
}

/// Assigning an account by name (§4.2's username): the one user field of the target projects,
/// or the user qualifies with it.
fn assign(plan: &mut Plan, names: &Names, atom: &Atom, user: i64, mode: Mode) {
    let mut fields: Vec<&NamedField> = Vec::new();
    for field in &names.fields {
        if field.kind == FieldKind::User && !fields.iter().any(|f| f.id == field.id) {
            fields.push(field);
        }
    }
    match fields.len() {
        0 => problem(plan, CommandCode::CmdWrongKind, atom.span),
        1 => push_ref(plan, names, fields[0], None, user, mode, atom.span),
        _ => {
            let field_names: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
            problem_with(
                plan,
                CommandCode::CmdAmbiguous,
                atom.span,
                one_param("fields", join(&field_names)),
            );
        }
    }
}

/// Whether this text clears the field: the `none` keyword, or the attachment's empty-value
/// name where the field may be empty (FLD-056).
fn clears(field: &NamedField, folded: &str) -> bool {
    if words::keyword(folded, words::VALUE).is_some_and(|word| word.key == "none") {
        return true;
    }
    field
        .attachments
        .iter()
        .any(|a| a.can_be_empty && !a.empty_label.is_empty() && a.empty_label == folded)
}

fn field_type(field: &NamedField) -> FieldType {
    FieldType {
        kind: field.kind,
        multi: field.multi,
    }
}

enum Unique<T> {
    One(T),
    Many,
    None_,
}

fn unique<T>(found: Vec<&T>) -> Unique<&T> {
    let mut found = found.into_iter();
    match (found.next(), found.next()) {
        (Some(one), None) => Unique::One(one),
        (Some(_), Some(_)) => Unique::Many,
        _ => Unique::None_,
    }
}
