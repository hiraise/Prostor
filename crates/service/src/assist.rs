//! The assist service (step 2.1 §5, D-061): one service, two surfaces, one suggestion
//! contract. The command half answers the dialog's preview and completion (`CMD-028`,
//! `CMD-029`); the query half answers the query bar's check plus completion (`QRY-056`…
//! `QRY-058`). The pure model — contexts, matching, ranges — lives in
//! [`taskmaster_query::completion`]; this module assembles the candidates from the dictionary
//! of the scope the caret's context implies: names, rights, presence, resolved dates.

use std::collections::{HashMap, HashSet};

use sqlx::PgConnection;
use taskmaster_domain::access::Access;
use taskmaster_domain::fields::FieldKind;
use taskmaster_domain::search::dates::Clock;
use taskmaster_domain::{FieldId, IssueId, ProjectId, TagId};
use taskmaster_query::completion::{
    self, Candidate, CommandContext, Group, Place, QueryContext, Suggestion,
};
use taskmaster_query::words::{self, Kind};
use taskmaster_store::PgPool;

use crate::Result;
use crate::commands::dictionary::{self, Names};
use crate::commands::{self, ApplyInput, Plan};
use crate::search::{self, SearchInput};

/// One command line the dialog is typing (plan §5): its text, the caret, and the targets the
/// line would apply to — named issues, or the query of the bulk flow.
#[derive(Debug, Clone, Default)]
pub struct CommandAssist {
    pub text: String,
    /// In bytes.
    pub caret: usize,
    pub issues: Vec<IssueId>,
    /// The query the bulk dialog applies the line to; with `project`, its scope.
    pub query: Option<String>,
    pub project: Option<ProjectId>,
    pub hide_resolved: bool,
    /// Markdown the `comment` command attaches; the preview says when it is missing.
    pub comment: Option<String>,
    pub time_zone: Option<String>,
}

/// The command half's answer: the plan the preview shows (its operations and problems, quick
/// fixes included), the marks its rows carry (plan §6), and the suggestions at the caret.
#[derive(Debug, Clone)]
pub struct CommandAssistAnswer {
    pub plan: Plan,
    pub marks: Vec<OperationMarks>,
    pub suggestions: Vec<Suggestion>,
}

/// What the preview marks one planned operation with (plan §6): its direction — `set`, `add`,
/// `remove` — and, for a plain set on a multi-value field, the values it takes off every target
/// issue (§1: the preview shows the removals).
#[derive(Debug, Clone, Default)]
pub struct OperationMarks {
    pub mode: Option<&'static str>,
    pub removals: Vec<String>,
}

/// The direction an operation changes things in, for the preview's `+`/`−` marks.
fn mode_of(operation: &commands::Operation) -> Option<&'static str> {
    use commands::Operation;
    use taskmaster_query::command::Mode;
    match operation {
        Operation::SetField { change, .. } => match change {
            crate::issues::FieldChange::Set(_) => Some("set"),
            crate::issues::FieldChange::Add(_) => Some("add"),
            crate::issues::FieldChange::Remove(_) => Some("remove"),
        },
        Operation::Tag { on, .. } | Operation::Star { on, .. } | Operation::Vote { on } => {
            Some(if *on { "add" } else { "remove" })
        }
        Operation::Link { remove, .. } => Some(if *remove { "remove" } else { "add" }),
        Operation::Visibility { mode, .. } | Operation::Board { mode, .. } => match mode {
            Mode::Set => Some("set"),
            Mode::Add => Some("add"),
            Mode::Remove => Some("remove"),
        },
        Operation::Move { .. }
        | Operation::Duplicate { .. }
        | Operation::Comment
        | Operation::Delete
        | Operation::Clone => None,
    }
}

/// The marks of every operation of the plan: the mode, and the removals a plain set on a
/// multi-value field makes — the values every target carries that the set drops, named as they
/// read. A bulk query, whose targets vary, marks no removals.
fn marks_of(
    plan: &Plan,
    names: &dictionary::Names,
    presence: Option<&Presence>,
) -> Vec<OperationMarks> {
    use commands::Operation;
    use taskmaster_domain::issues::values::Input;
    plan.operations
        .iter()
        .map(|planned| {
            let mut marks = OperationMarks {
                mode: mode_of(&planned.operation),
                removals: Vec::new(),
            };
            if let Operation::SetField { field, set, change } = &planned.operation
                && let crate::issues::FieldChange::Set(Some(Input::Ids(keep))) = change
                && let Some(presence) = presence
                && let Some(defined) = names.fields.iter().find(|found| found.id == *field)
                && defined.multi
                && let Some(held) = presence.values.get(field)
            {
                for id in held.iter().filter(|id| !keep.contains(id)) {
                    if let Some(display) = set
                        .and_then(|set| names.values.get(&set))
                        .and_then(|values| values.iter().find(|value| value.id == *id))
                        .map(|value| value.display.clone())
                    {
                        marks.removals.push(display);
                    }
                }
            }
            marks
        })
        .collect()
}

/// The query half's answer (QRY-056, QRY-058): the check — tokens and problems for the
/// markers — plus the suggestions at the caret.
#[derive(Debug, Clone)]
pub struct QueryAssistAnswer {
    pub check: search::Check,
    pub suggestions: Vec<Suggestion>,
}

/// The command dialog's assist (plan §5): the plan over the targets' dictionary, and the
/// completion at the caret — values marked [`present`] when every target already carries them.
pub async fn commands(
    pool: &PgPool,
    actor: &Access,
    input: &CommandAssist,
) -> Result<CommandAssistAnswer> {
    let mut conn = pool.acquire().await?;
    let targets = targets(&mut conn, actor, input).await?;
    let names = dictionary::load_projects(&mut conn, actor, targets).await?;
    let apply = ApplyInput {
        text: input.text.clone(),
        issues: input.issues.clone(),
        comment: input.comment.clone(),
        time_zone: input.time_zone.clone(),
        ..ApplyInput::default()
    };
    let clock = commands::clock(&mut conn, &apply).await?;
    let plan =
        crate::commands::resolve::resolve(&mut conn, &input.text, &names, actor, &apply, &clock)
            .await?;
    let (context, place) = completion::command_site(&input.text, input.caret, &names.parser);
    let presence = load_presence(&mut conn, actor, &input.issues).await?;
    // The person's lately applied lines (CMD-030): the `recent` group, above everything else a
    // fresh command could start from.
    let recents = taskmaster_store::command_history::recent(
        &mut *conn,
        actor.user(),
        taskmaster_store::command_history::KEEP as i64,
    )
    .await?;
    let candidates = if let CommandContext::LinkTarget = context {
        issue_candidates(pool, actor, input, &place).await?
    } else {
        command_candidates(&names, actor, &context, &clock, presence.as_ref(), &recents)
    };
    Ok(CommandAssistAnswer {
        marks: marks_of(&plan, &names, presence.as_ref()),
        plan,
        suggestions: completion::shape(&place, &candidates),
    })
}

/// The query bar's assist (QRY-056…QRY-058): the check's tokens and problems plus completion —
/// attributes and keywords, an attribute's values, the sort spec's keys and directions — with
/// values of the projects the query implies ranked above the rest (QRY-057).
pub async fn query(
    pool: &PgPool,
    viewer: &Access,
    text: &str,
    caret: usize,
    project: Option<ProjectId>,
    time_zone: Option<String>,
) -> Result<QueryAssistAnswer> {
    let check = search::check(pool, viewer, text, project, time_zone.clone()).await?;
    let mut conn = pool.acquire().await?;
    let readable = readable(&mut conn, viewer, project).await?;
    let names = dictionary::load_projects(&mut conn, viewer, readable.clone()).await?;
    let search_input = SearchInput {
        time_zone,
        ..SearchInput::default()
    };
    let clock = search::clock(&mut conn, &search_input).await?;
    let (context, place) = completion::query_site(text, caret, &names.parser);
    // What the query so far implies: values of those projects rank above the rest.
    let implied = match project {
        Some(one) => vec![one],
        None => {
            let q = taskmaster_query::parse(text);
            let entries = readable_entries(&mut conn, viewer).await?;
            let scope = taskmaster_domain::search::scope(&q, &entries, None);
            if scope.narrowed {
                scope.projects
            } else {
                Vec::new()
            }
        }
    };
    let partial = !place.prefix.is_empty();
    let candidates = query_candidates(&names, viewer, &context, &clock, &implied, partial);
    Ok(QueryAssistAnswer {
        check,
        suggestions: completion::shape(&place, &candidates),
    })
}

/// The projects the assist's dictionary covers: the named issues', or the ones the bulk query
/// implies (its own parse; a query that does not resolve yet covers every readable project).
async fn targets(
    conn: &mut PgConnection,
    actor: &Access,
    input: &CommandAssist,
) -> Result<Vec<ProjectId>> {
    if !input.issues.is_empty() {
        let found = taskmaster_store::issues::get_many(&mut *conn, &input.issues).await?;
        let mut projects: Vec<ProjectId> = found.iter().map(|issue| issue.project_id).collect();
        projects.sort_unstable();
        projects.dedup();
        return Ok(projects);
    }
    let readable = readable(conn, actor, input.project).await?;
    if let Some(text) = input.query.as_deref().filter(|text| !text.is_empty()) {
        let entries = readable_entries(conn, actor).await?;
        let within = input.project.map(|p| vec![p]);
        let scope = taskmaster_domain::search::scope(
            &taskmaster_query::parse(text),
            &entries,
            within.as_deref(),
        );
        if scope.narrowed {
            return Ok(scope.projects);
        }
    }
    Ok(readable)
}

/// Every project the actor can read, minus an absent `project` (the caller checked it exists).
async fn readable(
    conn: &mut PgConnection,
    actor: &Access,
    project: Option<ProjectId>,
) -> Result<Vec<ProjectId>> {
    let entries = readable_entries(conn, actor).await?;
    let mut out: Vec<ProjectId> = entries
        .iter()
        .map(|entry| entry.id)
        .filter(|id| Some(*id) != project)
        .collect();
    if let Some(one) = project
        && entries.iter().any(|entry| entry.id == one)
    {
        out.push(one);
    }
    out.sort_unstable();
    out.dedup();
    Ok(out)
}

/// The readable, living projects as entries, for the scope of a query's parse.
async fn readable_entries(
    conn: &mut PgConnection,
    actor: &Access,
) -> Result<Vec<taskmaster_domain::search::catalog::ProjectEntry>> {
    let rows = taskmaster_store::search::catalog::projects(&mut *conn).await?;
    let vis = search::visibility(actor, &rows);
    Ok(rows
        .into_iter()
        .filter(|row| !row.deleting && search::sees(&vis, row.entry.id))
        .map(|row| row.entry)
        .collect())
}

/// The candidates of one command context, in the order the list shows (plan §5): each group
/// together, the values of a field labelled with it, users only with the directory right (the
/// dictionary already answers that), relative dates with their resolved value. The person's
/// recent lines (CMD-030) lead a fresh command's list.
fn command_candidates(
    names: &Names,
    actor: &Access,
    context: &CommandContext,
    clock: &Clock,
    presence: Option<&Presence>,
    recents: &[String],
) -> Vec<Candidate> {
    let mut out = Vec::new();
    match context {
        CommandContext::Start => {
            for line in recents {
                out.push(Candidate::new(line.trim(), Group::Recent));
            }
            command_words(&mut out);
            attributes(names, &mut out);
            links(names, &mut out);
            values(names, presence, false, &mut out);
            empties(names, false, &mut out);
            projects(names, &mut out);
            tags(names, presence, &mut out);
            users(names, actor, presence, None, &mut out);
            groups(names, &mut out);
            boards(names, &mut out);
            sprints(names, None, &mut out);
            out.push(Candidate::new("me", Group::Special));
        }
        CommandContext::Word { key } => match *key {
            "tag" | "untag" => tags(names, presence, &mut out),
            "move_to" | "project" => projects(names, &mut out),
            "star" | "unstar" | "watcher" => users(names, actor, presence, None, &mut out),
            "visible_to" => {
                users(names, actor, presence, None, &mut out);
                groups(names, &mut out);
            }
            "board" => boards(names, &mut out),
            _ => {}
        },
        CommandContext::AddRemove { .. } => {
            links(names, &mut out);
            sub_command_words(&mut out);
            attributes_multi(names, &mut out);
            values(names, presence, true, &mut out);
            empties(names, true, &mut out);
            users(names, actor, presence, None, &mut out);
            tags(names, presence, &mut out);
        }
        CommandContext::Attribute { folded, multi } => {
            for field in names.fields_named(folded) {
                if multi.is_some() && !field.multi {
                    continue;
                }
                field_candidates(names, actor, field, clock, presence, &mut out);
            }
        }
        CommandContext::BoardSprints { board } => {
            sprints(
                names,
                names.boards_named(board).first().map(|b| b.id),
                &mut out,
            );
        }
        CommandContext::LinkTarget => {}
    }
    out
}

/// One field's own candidates: its values (marked when the targets already carry them), its
/// accounts or groups, its date words with their resolved value, its empty value.
fn field_candidates(
    names: &Names,
    actor: &Access,
    field: &dictionary::NamedField,
    clock: &Clock,
    presence: Option<&Presence>,
    out: &mut Vec<Candidate>,
) {
    match field.kind {
        FieldKind::Enum
        | FieldKind::State
        | FieldKind::Version
        | FieldKind::Build
        | FieldKind::Owned => {
            for set in field.sets() {
                if let Some(set_values) = names.values.get(&set) {
                    for value in set_values {
                        let mut candidate =
                            Candidate::new(&value.display, Group::Value).described(&field.name);
                        candidate.present =
                            presence.is_some_and(|p| p.has_value(field.id, value.id));
                        out.push(candidate);
                    }
                }
            }
        }
        FieldKind::User => {
            users(names, actor, presence, Some(field), out);
            out.push(Candidate::new("me", Group::Special).described(&field.name));
        }
        FieldKind::Group => {
            for group in &names.groups {
                out.push(Candidate::new(&group.display, Group::Group));
            }
        }
        FieldKind::Date | FieldKind::DateTime => dates(clock, out),
        FieldKind::Text
        | FieldKind::String
        | FieldKind::Integer
        | FieldKind::Float
        | FieldKind::Duration => {}
    }
    empty_of(field, out);
}

/// The candidates of one query context (QRY-056, QRY-057).
fn query_candidates(
    names: &Names,
    viewer: &Access,
    context: &QueryContext,
    clock: &Clock,
    implied: &[ProjectId],
    partial: bool,
) -> Vec<Candidate> {
    let mut out = Vec::new();
    // A partial word matches attributes *and* values (QRY-056); an empty one shows the
    // attributes and keywords alone.
    match context {
        QueryContext::Names => {
            // The scope's fields first — the closest context — then the built-ins and the
            // structural keywords; the list caps at the model's limit.
            attributes(names, &mut out);
            builtin_attributes(&mut out);
            if partial {
                values(names, None, false, &mut out);
            }
            for word in words::VOCABULARY
                .iter()
                .filter(|w| w.kind == Kind::Operator)
            {
                for form in completion::offered_forms(word) {
                    out.push(Candidate::new(form, Group::Keyword));
                }
            }
            for word in words::VOCABULARY.iter().filter(|w| w.key == "sort") {
                for form in completion::offered_forms(word) {
                    out.push(Candidate::new(form, Group::Keyword));
                }
            }
        }
        QueryContext::Values { folded, builtin } => match builtin {
            Some(key) => match *key {
                "tag" => tags(names, None, &mut out),
                "project" => projects(names, &mut out),
                "board" => boards(names, &mut out),
                "sprint" => sprints(names, None, &mut out),
                "reporter" | "updater" | "commenter" | "involves" => {
                    users(names, viewer, None, None, &mut out)
                }
                "created" | "updated" | "resolved_date" | "commented" => dates(clock, &mut out),
                "is" => {
                    for word in words::VOCABULARY.iter().filter(|w| {
                        w.kind == Kind::Value && w.key != "me" && w.key != "any" && w.key != "none"
                    }) {
                        for form in completion::offered_forms(word) {
                            out.push(Candidate::new(form, Group::Keyword));
                        }
                    }
                }
                _ => {}
            },
            None => {
                for field in names.fields_named(folded) {
                    if !taskmaster_domain::issues::values::editable(field.builtin.as_deref()) {
                        continue;
                    }
                    let tier = if field_in(field, implied) { 0 } else { 1 };
                    match field.kind {
                        FieldKind::User => users(names, viewer, None, None, &mut out),
                        FieldKind::Group => {
                            for group in &names.groups {
                                out.push(Candidate::new(&group.display, Group::Group));
                            }
                        }
                        FieldKind::Date | FieldKind::DateTime => dates(clock, &mut out),
                        _ => {}
                    }
                    for set in field.sets() {
                        if let Some(set_values) = names.values.get(&set) {
                            for value in set_values {
                                let mut candidate = Candidate::new(&value.display, Group::Value)
                                    .described(&field.name);
                                candidate.tier = tier;
                                out.push(candidate);
                            }
                        }
                    }
                }
            }
        },
        QueryContext::SortKeys => {
            for key in [
                "updated", "created", "resolved", "id", "votes", "comments", "project", "reporter",
                "summary", "starred",
            ] {
                if let Some(word) = words::VOCABULARY.iter().find(|w| w.key == key) {
                    for form in completion::offered_forms(word) {
                        out.push(Candidate::new(form, Group::SortKey));
                    }
                }
            }
            for field in &names.fields {
                out.push(Candidate::new(&field.name, Group::SortKey));
            }
        }
        QueryContext::SortDirections => {
            for word in words::VOCABULARY
                .iter()
                .filter(|w| w.key == "asc" || w.key == "desc")
            {
                for form in completion::offered_forms(word) {
                    out.push(Candidate::new(form, Group::SortDirection));
                }
            }
        }
    }
    out
}

/// Which values every target issue already carries (§1): the assist marks them.
#[derive(Debug, Clone, Default)]
pub struct Presence {
    /// By field: the reference ids every target issue holds.
    values: HashMap<FieldId, Vec<i64>>,
    /// Tags on every target issue.
    tags: HashSet<TagId>,
}

impl Presence {
    fn has_value(&self, field: FieldId, id: i64) -> bool {
        self.values.get(&field).is_some_and(|ids| ids.contains(&id))
    }

    fn has_tag(&self, tag: TagId) -> bool {
        self.tags.contains(&tag)
    }
}

/// Loads what every target issue carries (§1): the reference ids each field holds on all of
/// them together, and the tags on all of them — what the suggestions mark `present`. `None`
/// when there are no explicit targets (the bulk flow's query).
async fn load_presence(
    conn: &mut PgConnection,
    actor: &Access,
    issues: &[IssueId],
) -> Result<Option<Presence>> {
    if issues.is_empty() {
        return Ok(None);
    }
    let found = taskmaster_store::issues::get_many(&mut *conn, issues).await?;
    let mut projects: Vec<ProjectId> = found.iter().map(|issue| issue.project_id).collect();
    projects.sort_unstable();
    projects.dedup();
    let project_fields = crate::issues::values::of_projects(&mut *conn, &projects).await?;
    let pairs: Vec<(IssueId, ProjectId)> = found
        .iter()
        .map(|issue| (issue.id, issue.project_id))
        .collect();
    let values = crate::issues::values::of_issues(&mut *conn, &pairs, &project_fields).await?;
    let tag_rows =
        taskmaster_store::tags::on_issues(&mut *conn, issues, actor.user(), actor.groups()).await?;

    let mut presence = Presence::default();
    let mut first = true;
    for issue in issues {
        let mine: HashMap<FieldId, Vec<i64>> = values
            .get(issue)
            .map(|fields| {
                fields
                    .iter()
                    .map(|(field, value)| (*field, value.refs().to_vec()))
                    .collect()
            })
            .unwrap_or_default();
        if first {
            presence.values = mine;
            first = false;
        } else {
            presence.values.retain(|field, ids| {
                mine.get(field)
                    .is_some_and(|theirs| ids.iter().all(|id| theirs.contains(id)))
            });
        }
    }
    let mut first = true;
    for issue in issues {
        let mine: HashSet<TagId> = tag_rows
            .iter()
            .filter(|(id, _)| id == issue)
            .map(|(_, tag)| tag.id)
            .collect();
        if first {
            presence.tags = mine;
            first = false;
        } else {
            presence.tags.retain(|tag| mine.contains(tag));
        }
    }
    Ok(Some(presence))
}

// ---- The candidate groups, shared by both surfaces ----

/// The command words in both languages (plan §3): every [`Kind::Command`] entry, and the query
/// words that double as command words (`tag`, `board`, `project`).
fn command_words(out: &mut Vec<Candidate>) {
    for word in words::VOCABULARY.iter().filter(|w| w.kind == Kind::Command) {
        for form in completion::offered_forms(word) {
            out.push(Candidate::new(form, Group::Command));
        }
    }
    for key in ["tag", "board", "project"] {
        if let Some(word) = words::VOCABULARY.iter().find(|w| w.key == key) {
            for form in completion::offered_forms(word) {
                out.push(Candidate::new(form, Group::Command));
            }
        }
    }
}

/// The sub-command words `add`/`remove` take (plan §3): `tag`, `board`, `visible to`.
fn sub_command_words(out: &mut Vec<Candidate>) {
    for word in words::VOCABULARY
        .iter()
        .filter(|w| matches!(w.key, "tag" | "board" | "visible_to"))
    {
        for form in completion::offered_forms(word) {
            out.push(Candidate::new(form, Group::Command));
        }
    }
}

/// The built-in attributes of the query language, in both languages.
fn builtin_attributes(out: &mut Vec<Candidate>) {
    for word in words::VOCABULARY
        .iter()
        .filter(|w| w.kind == Kind::Attribute)
    {
        for form in completion::offered_forms(word) {
            out.push(Candidate::new(form, Group::Attribute));
        }
    }
}

/// The fields of the scope, by their own names.
fn attributes(names: &Names, out: &mut Vec<Candidate>) {
    for field in &names.fields {
        out.push(Candidate::new(&field.name, Group::Attribute));
    }
}

/// The multi-value fields alone (only they take `add`/`remove`).
fn attributes_multi(names: &Names, out: &mut Vec<Candidate>) {
    for field in &names.fields {
        if field.multi {
            out.push(Candidate::new(&field.name, Group::Attribute));
        }
    }
}

/// The link directions, both ways.
fn links(names: &Names, out: &mut Vec<Candidate>) {
    for link in &names.links {
        out.push(Candidate::new(&link.display, Group::Link));
    }
}

/// Every set field's values, labelled with their field; `multi` keeps the multi-value fields.
fn values(names: &Names, presence: Option<&Presence>, multi: bool, out: &mut Vec<Candidate>) {
    for field in &names.fields {
        if multi && !field.multi {
            continue;
        }
        if !matches!(
            field.kind,
            FieldKind::Enum
                | FieldKind::State
                | FieldKind::Version
                | FieldKind::Build
                | FieldKind::Owned
        ) {
            continue;
        }
        for set in field.sets() {
            if let Some(set_values) = names.values.get(&set) {
                for value in set_values {
                    let mut candidate =
                        Candidate::new(&value.display, Group::Value).described(&field.name);
                    if let Some(presence) = presence {
                        candidate.present = presence.has_value(field.id, value.id);
                    }
                    out.push(candidate);
                }
            }
        }
    }
}

/// The empty values of the fields that may be empty, labelled with their field.
fn empties(names: &Names, multi: bool, out: &mut Vec<Candidate>) {
    for field in &names.fields {
        if multi && !field.multi {
            continue;
        }
        empty_of(field, out);
    }
}

fn empty_of(field: &dictionary::NamedField, out: &mut Vec<Candidate>) {
    let mut seen = Vec::new();
    for attachment in &field.attachments {
        if attachment.can_be_empty
            && !attachment.empty_display.is_empty()
            && !seen.contains(&attachment.empty_display)
        {
            seen.push(attachment.empty_display.clone());
            out.push(
                Candidate::new(&attachment.empty_display, Group::Empty).described(&field.name),
            );
        }
    }
}

fn projects(names: &Names, out: &mut Vec<Candidate>) {
    for project in &names.projects {
        out.push(Candidate::new(&project.name, Group::Project).described(&project.key));
    }
}

fn tags(names: &Names, presence: Option<&Presence>, out: &mut Vec<Candidate>) {
    for tag in &names.tags {
        let mut candidate = Candidate::new(&tag.display, Group::Tag);
        if let Some(presence) = presence {
            candidate.present = presence.has_tag(tag.id);
        }
        out.push(candidate);
    }
}

/// The accounts the actor may name; `field` marks presence through one user field. The
/// dictionary already answers the directory right: without it, only the actor's own account.
fn users(
    names: &Names,
    actor: &Access,
    presence: Option<&Presence>,
    field: Option<&dictionary::NamedField>,
    out: &mut Vec<Candidate>,
) {
    for user in &names.users {
        let mut candidate =
            Candidate::new(&user.login, Group::User).described(user.full_name.clone());
        candidate.own = user.id == actor.user();
        if let (Some(presence), Some(field)) = (presence, field) {
            candidate.present = presence.has_value(field.id, user.id);
        }
        out.push(candidate);
    }
}

fn groups(names: &Names, out: &mut Vec<Candidate>) {
    for group in &names.groups {
        out.push(Candidate::new(&group.display, Group::Group));
    }
}

fn boards(names: &Names, out: &mut Vec<Candidate>) {
    for board in &names.boards {
        out.push(Candidate::new(&board.display, Group::Board));
    }
}

fn sprints(names: &Names, of_board: Option<taskmaster_domain::BoardId>, out: &mut Vec<Candidate>) {
    for sprint in &names.sprints {
        if of_board.is_some_and(|board| sprint.board != board) {
            continue;
        }
        let described = names
            .boards
            .iter()
            .find(|board| board.id == sprint.board)
            .map(|board| board.display.clone());
        let mut candidate = Candidate::new(&sprint.display, Group::Sprint);
        if let Some(described) = described {
            candidate = candidate.described(described);
        }
        out.push(candidate);
    }
}

/// The relative date words, each with the concrete day it means in the caller's zone.
fn dates(clock: &Clock, out: &mut Vec<Candidate>) {
    for word in words::VOCABULARY.iter().filter(|w| w.kind == Kind::Date) {
        for form in completion::offered_forms(word) {
            let mut candidate = Candidate::new(form, Group::Date);
            if let Some(period) = clock.word(word.key)
                && let Some(start) = period.start
            {
                candidate.resolved = Some(start.date().to_string());
            }
            out.push(candidate);
        }
    }
}

/// Issues after a link word (plan §5): IDs with summaries; typed words search issues through
/// the existing search.
async fn issue_candidates(
    pool: &PgPool,
    actor: &Access,
    input: &CommandAssist,
    place: &Place,
) -> Result<Vec<Candidate>> {
    let text = place.raw.trim().to_owned();
    let page = search::search(
        pool,
        actor,
        SearchInput {
            text,
            limit: completion::MAX as u32,
            time_zone: input.time_zone.clone(),
            ..SearchInput::default()
        },
    )
    .await?
    .issues;
    let mut out = Vec::new();
    for view in &page.issues {
        if let Some(id) = view.readable_id() {
            out.push(Candidate::new(id, Group::Issue).described(view.issue.summary.clone()));
        }
    }
    Ok(out)
}

/// Whether a field attaches in one of the projects (QRY-057's tiering).
fn field_in(field: &dictionary::NamedField, projects: &[ProjectId]) -> bool {
    field
        .attachments
        .iter()
        .any(|attachment| projects.contains(&attachment.project))
}
