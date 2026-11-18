//! The engine of the built-in behaviors (AUT-023, step 1.9 §3.5). The write paths name what
//! happened as [`Event`]s; [`on`] runs the behaviors the project turned on, and behaviors change
//! issues only through [`write`], which keeps the triggering actor and speaks of the rule in
//! history. There is no fixpoint loop: each behavior runs at its event and handles its own
//! bounded recursion through `write`'s re-entry.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use prostor_domain::FieldId;
use prostor_domain::IssueId;
use prostor_domain::ProjectId;
use prostor_domain::access::Access;
use prostor_domain::fields::FieldKind;
use prostor_domain::issues::history::{ChangeKind, Origin, Record};
use prostor_domain::issues::values::Value;
use prostor_domain::links::BuiltinType;
use prostor_domain::rules::Rule;
use prostor_domain::settings::{Definition, Language, SYSTEM_LANGUAGE};
use prostor_store::issue_history::Author;
use prostor_store::issues::Issue;
use prostor_store::{issues, links, projects, rules as store_rules, settings, value_sets};
use sqlx::PgConnection;
use time::OffsetDateTime;

use crate::Error;
use crate::issues::finish_change;
use crate::issues::values::{self, ProjectField, ProjectFields};
use crate::live::Batch;

mod behaviors;
pub mod clone;
mod duplicates;
mod fix_versions;
mod fixed_in_build;
mod follows;
mod scrum;
pub mod tab;

/// How deep rule-made changes may re-enter the engine, and how many rule writes one transaction
/// may make (§3.5); over either limit is a bug, not a state to keep.
pub const MAX_DEPTH: usize = 20;
pub const MAX_WRITES: usize = 100;

/// What a behavior tells the actor (§3.11): rides the answer of the use case that fired, and the
/// web turns each into a toast. Codes and params belong to the behaviors; texts live in the web
/// i18n only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub rule: Rule,
    pub code: &'static str,
    pub params: BTreeMap<String, String>,
}

/// What a use case that can fire behaviors answers: its own result and the rules' messages.
#[derive(Debug, Clone)]
pub struct Answer<T> {
    pub done: T,
    pub messages: Vec<Message>,
}

impl<T> Answer<T> {
    /// A use case that fired nothing.
    pub fn plain(done: T) -> Self {
        Self {
            done,
            messages: Vec::new(),
        }
    }
}

/// One step of the engine: who triggers, which language names take, what the rules said. The
/// language is a placeholder until [`on`] fills it from the instance settings; behaviors run
/// after that only.
pub struct Ctx<'a> {
    pub actor: &'a Access,
    pub language: Language,
    pub messages: Vec<Message>,
    /// How deep rule-made changes have re-entered the engine, and how many writes they have made.
    pub depth: usize,
    pub writes: usize,
    /// The issue the current event is about: [`write`] on it defers its records to the caller,
    /// so they join the change that triggered them instead of preceding it (§3.5). Set by
    /// [`on`]; tests of the join mechanics may set it by hand.
    pub focus: Option<IssueId>,
    /// Records deferred for the focus issue, waiting for [`finish`].
    pub(crate) deferred: Vec<Record>,
    /// What the engine has read about a project: the project, its toggles, its fields (§3.14's
    /// budget). One context is one transaction, which sees one consistent state, so a re-read of
    /// the same project would answer the same — and the cascade and one event's several
    /// behaviors walk the same project again and again.
    known: HashMap<ProjectId, Known>,
    /// The values of issues the engine has read or written, keyed by issue: [`write`] keeps each
    /// entry current, so one event's behaviors share one read of the issue they act on.
    held: HashMap<IssueId, BTreeMap<FieldId, Value>>,
    /// The values of value sets the engine has read, keyed by set: the state sets the walk, the
    /// duplicate checks and the resolution settling ask for are the same few rows.
    sets: HashMap<i64, Arc<Vec<value_sets::Value>>>,
    /// The built-in duplicate link type, read once (D-034: present and locked, always).
    duplicate_type: Option<i64>,
    /// The parents one walk is re-entering the engine for (D-049's batched writes): their own
    /// resolution events would make the walk climb a chain it has just written. Set only for
    /// the re-entries themselves, empty before and after.
    pub(crate) just_walked: Vec<IssueId>,
    language_read: bool,
}

/// The shares of [`Ctx::known`]: each read once, only when a behavior needs it.
#[derive(Default)]
struct Known {
    project: Option<Arc<projects::Project>>,
    toggles: Option<Arc<Vec<store_rules::Toggle>>>,
    fields: Option<Arc<ProjectFields>>,
}

impl<'a> Ctx<'a> {
    pub fn new(actor: &'a Access) -> Self {
        Self {
            actor,
            language: Language::En,
            messages: Vec::new(),
            depth: 0,
            writes: 0,
            focus: None,
            deferred: Vec::new(),
            known: HashMap::new(),
            held: HashMap::new(),
            sets: HashMap::new(),
            duplicate_type: None,
            just_walked: Vec::new(),
            language_read: false,
        }
    }

    /// The project row, read once per context; `None` while it is gone, as the store answers.
    /// Behaviors keep the share while the engine runs on.
    pub(crate) async fn project(
        &mut self,
        conn: &mut PgConnection,
        id: ProjectId,
    ) -> Result<Option<Arc<projects::Project>>, Error> {
        if let Some(found) = self.known.entry(id).or_default().project.clone() {
            return Ok(Some(found));
        }
        let found = projects::get(&mut *conn, id).await?.map(Arc::new);
        if let Some(found) = &found {
            self.known.entry(id).or_default().project = Some(Arc::clone(found));
        }
        Ok(found)
    }

    /// The project's rule toggles, read once per context.
    pub(crate) async fn toggles(
        &mut self,
        conn: &mut PgConnection,
        id: ProjectId,
    ) -> Result<Arc<Vec<store_rules::Toggle>>, Error> {
        if let Some(found) = self.known.entry(id).or_default().toggles.clone() {
            return Ok(found);
        }
        let found = Arc::new(store_rules::of_project(&mut *conn, id).await?);
        self.known.entry(id).or_default().toggles = Some(Arc::clone(&found));
        Ok(found)
    }

    /// The project's fields with their values, read once per context.
    pub(crate) async fn fields(
        &mut self,
        conn: &mut PgConnection,
        id: ProjectId,
    ) -> Result<Arc<ProjectFields>, Error> {
        if let Some(found) = self.known.entry(id).or_default().fields.clone() {
            return Ok(found);
        }
        let found = Arc::new(values::of_project(&mut *conn, id).await?);
        self.known.entry(id).or_default().fields = Some(Arc::clone(&found));
        Ok(found)
    }

    /// The values an issue holds, read once per context and kept current by [`write`]: one
    /// event's behaviors (the scrum assign and its write, the build copies) act on the same
    /// issue, and the walk preloads its whole chain (D-049's batched writes).
    pub(crate) async fn held_of(
        &mut self,
        conn: &mut PgConnection,
        issue: &issues::Issue,
        fields: &ProjectFields,
    ) -> Result<BTreeMap<FieldId, Value>, Error> {
        if let Some(found) = self.held.get(&issue.id) {
            return Ok(found.clone());
        }
        let found = values::of_issue(&mut *conn, issue, fields).await?;
        self.held.insert(issue.id, found.clone());
        Ok(found)
    }

    /// Keeps [`held_of`]'s entry what the transaction now sees; [`write`] calls this after
    /// storing a value.
    pub(crate) fn remember_held(&mut self, issue: IssueId, held: BTreeMap<FieldId, Value>) {
        self.held.insert(issue, held);
    }

    /// The values of a value set, read once per context: the walk, the duplicate checks and the
    /// settling of resolution ask for the same state sets again and again.
    pub(crate) async fn set_values(
        &mut self,
        conn: &mut PgConnection,
        set: i64,
    ) -> Result<Arc<Vec<value_sets::Value>>, Error> {
        if let Some(found) = self.sets.get(&set) {
            return Ok(Arc::clone(found));
        }
        let found = Arc::new(value_sets::values_of(&mut *conn, &[set]).await?);
        self.sets.insert(set, Arc::clone(&found));
        Ok(found)
    }

    /// The built-in duplicate link type, read once per context.
    pub(crate) async fn duplicate_type(&mut self, conn: &mut PgConnection) -> Result<i64, Error> {
        if let Some(found) = self.duplicate_type {
            return Ok(found);
        }
        let found = links::type_by_builtin(&mut *conn, BuiltinType::Duplicate.key())
            .await?
            .map(|kind| kind.id)
            .ok_or_else(|| {
                Error::Internal("the built-in duplicate link type is gone".to_owned())
            })?;
        self.duplicate_type = Some(found);
        Ok(found)
    }

    /// The author of rule-made records: the behavior acts, the triggering actor is named (§3.12).
    fn author(&self) -> Author {
        Author {
            user: self.actor.user(),
            origin: Origin::Rule,
            silent: false,
        }
    }

    /// Says a message in a behavior's name (§3.11): it rides the answer of the use case that
    /// fired, and the web turns it into a toast.
    pub(crate) fn say(
        &mut self,
        rule: Rule,
        code: &'static str,
        params: &[(&'static str, String)],
    ) {
        self.messages.push(Message {
            rule,
            code,
            params: params
                .iter()
                .map(|(key, value)| ((*key).to_owned(), value.clone()))
                .collect(),
        });
    }
}

/// What happened on a write path (§3.5): one event per use case fragment. `BecameResolved` and
/// `BecameUnresolved` come from the resolution records a change produced.
pub enum Event<'a> {
    FieldChanged {
        issue: &'a Issue,
        field: &'a ProjectField,
        old: Option<&'a Value>,
        new: Option<&'a Value>,
    },
    BecameResolved {
        issue: &'a Issue,
    },
    BecameUnresolved {
        issue: &'a Issue,
    },
    Published {
        issue: &'a Issue,
    },
    Moved {
        issue: &'a Issue,
    },
    LinksAdded {
        issue: &'a Issue,
        kind: BuiltinType,
        /// The issues linked now, and whether each is the source of its link.
        others: &'a [(IssueId, bool)],
    },
    LinkRemoved {
        issue: &'a Issue,
        kind: BuiltinType,
        other: IssueId,
        /// Whether the source of the removed link was the other issue.
        was_source: bool,
    },
    CommentAdded {
        issue: &'a Issue,
        text: &'a str,
    },
}

impl Event<'_> {
    pub fn issue(&self) -> &Issue {
        match self {
            Event::FieldChanged { issue, .. }
            | Event::BecameResolved { issue }
            | Event::BecameUnresolved { issue }
            | Event::Published { issue }
            | Event::Moved { issue }
            | Event::LinksAdded { issue, .. }
            | Event::LinkRemoved { issue, .. }
            | Event::CommentAdded { issue, .. } => issue,
        }
    }
}

/// Runs the behaviors the issue's project turned on for one event. The records of the issue the
/// event is about come back to join the caller's change ([`finish`]); writes to other issues are
/// complete here, with history of their own. A project with no rows runs nothing beyond the one
/// read of its toggles.
pub async fn on(
    conn: &mut PgConnection,
    batch: &mut Batch,
    ctx: &mut Ctx<'_>,
    event: Event<'_>,
) -> Result<Vec<Record>, Error> {
    let issue = event.issue();
    let toggles = ctx.toggles(&mut *conn, issue.project_id).await?;
    if !toggles.is_empty() {
        read_language(conn, ctx).await?;
    }
    let outer = ctx.focus;
    ctx.focus = Some(issue.id);
    for toggle in toggles.iter().filter(|toggle| toggle.enabled) {
        dispatch(conn, batch, ctx, toggle.rule, &event).await?;
    }
    ctx.focus = outer;
    // Only the outermost step answers the caller; a re-entered step (depth > 0) has written
    // everything already, and must not walk away with records deferred for the caller.
    Ok(if ctx.depth == 0 {
        std::mem::take(&mut ctx.deferred)
    } else {
        Vec::new()
    })
}

/// Fills the context's language from the instance settings, once per context, when a rule is
/// about to run: name lookups and message params speak it (§3.4).
async fn read_language(conn: &mut PgConnection, ctx: &mut Ctx<'_>) -> Result<(), Error> {
    if ctx.language_read {
        return Ok(());
    }
    ctx.language_read = true;
    ctx.language = instance_language(conn).await?;
    Ok(())
}

/// The instance's language, as the rules and their tab speak it (§3.4): the setting, else English.
pub(crate) async fn instance_language(conn: &mut PgConnection) -> Result<Language, Error> {
    if let Some(stored) = settings::value(&mut *conn, SYSTEM_LANGUAGE.key).await?
        && let Ok(normalized) = SYSTEM_LANGUAGE.parse(&stored)
        && let Ok(language) = serde_json::from_value::<Language>(normalized)
    {
        return Ok(language);
    }
    Ok(Language::En)
}

/// One rule against one event (§3.6–§3.9). Each behavior looks at the events it owns and skips
/// the rest; CloneIssue, a use case rather than an event behavior (§3.10), answers nothing.
async fn dispatch(
    conn: &mut PgConnection,
    batch: &mut Batch,
    ctx: &mut Ctx<'_>,
    rule: Rule,
    event: &Event<'_>,
) -> Result<(), Error> {
    match rule {
        Rule::AssigneeVisibility => behaviors::assignee_visibility(&mut *conn, ctx, event).await?,
        Rule::PlusOneVote => behaviors::plus_one_vote(&mut *conn, ctx, event).await?,
        Rule::ComponentOwner => behaviors::component_owner(&mut *conn, batch, ctx, event).await?,
        Rule::ParentFollowsSubtasks => {
            follows::parent_follows_subtasks(&mut *conn, batch, ctx, event, rule).await?
        }
        Rule::InheritFixVersions => {
            fix_versions::inherit_fix_versions(&mut *conn, batch, ctx, event).await?
        }
        Rule::FixedInBuild => fixed_in_build::fixed_in_build(&mut *conn, batch, ctx, event).await?,
        Rule::Duplicates => duplicates::duplicates(&mut *conn, batch, ctx, event).await?,
        Rule::Scrum => scrum::scrum(&mut *conn, batch, ctx, event).await?,
        // CloneIssue is a use case, not an event behavior (§3.10).
        _ => {}
    }
    Ok(())
}

/// Changes one field of an issue the way a behavior does (§3.5): the same store calls as the
/// field use case, but with no actor-rights check — behaviors act for the project, not the
/// person — and with the rule, not the person, as the origin. A field that may not be empty is
/// never cleared: the behavior skips. Resolution records the write produces re-enter [`on`] for
/// that issue, so a behavior that climbs (parent follows subtasks) runs on.
pub async fn write(
    conn: &mut PgConnection,
    batch: &mut Batch,
    ctx: &mut Ctx<'_>,
    issue: &Issue,
    field: &ProjectField,
    new: Option<&Value>,
    rule: Rule,
) -> Result<Vec<Record>, Error> {
    if ctx.depth >= MAX_DEPTH || ctx.writes >= MAX_WRITES {
        return Err(Error::Internal(format!(
            "the rule {} reached depth {} with {} writes",
            rule.key(),
            ctx.depth,
            ctx.writes
        )));
    }
    // A guard, not a refusal: a behavior skips what it may not do.
    if new.is_none() && !field.attached.can_be_empty {
        return Ok(Vec::new());
    }
    // PRJ-062: rule writes never touch issues of archived projects — the archive is read-only
    // (D-027). A guard in `write`, so no behavior can work around it.
    if ctx
        .project(&mut *conn, issue.project_id)
        .await?
        .is_some_and(|project| project.is_archived())
    {
        return Ok(Vec::new());
    }
    let fields = ctx.fields(&mut *conn, issue.project_id).await?;
    let mut held = ctx.held_of(&mut *conn, issue, &fields).await?;
    let old = held.get(&field.id()).cloned();
    let mut records = Vec::new();
    let Some(record) = values::write(&mut *conn, issue.id, field, old.as_ref(), new).await? else {
        return Ok(records);
    };
    records.push(record);
    match new {
        Some(value) => {
            held.insert(field.id(), value.clone());
        }
        None => {
            held.remove(&field.id());
        }
    }
    ctx.remember_held(issue.id, held.clone());
    if field.kind.kind == FieldKind::State {
        records.extend(
            values::settle_resolution(&mut *conn, issue, &fields, &held, OffsetDateTime::now_utc())
                .await?,
        );
    }
    ctx.writes += 1;
    // The focus issue's records join the change that triggered them (§3.5); every other issue
    // is complete here and now.
    if ctx.depth == 0 && ctx.focus == Some(issue.id) {
        ctx.deferred.extend(records.iter().cloned());
        return Ok(records);
    }
    finish(conn, batch, ctx, issue, &records).await?;
    climb(conn, batch, ctx, issue, &records).await?;
    Ok(records)
}

/// Writes a rule's records of one issue with the rule as their origin, marks the issue changed
/// and records its live event — what [`finish_change`] does, in the rule's name. Entry points
/// call it for the records [`on`] answered, after their own `finish_change`.
pub async fn finish(
    conn: &mut PgConnection,
    batch: &mut Batch,
    ctx: &Ctx<'_>,
    issue: &Issue,
    records: &[Record],
) -> Result<(), Error> {
    finish_change(conn, batch, issue, ctx.author(), records, &[]).await
}

/// [`finish`] without notification events: a publication's rule records ride its one `created`
/// event (plan §3.2), which the publisher records together with its own records.
pub(crate) async fn finish_quiet(
    conn: &mut PgConnection,
    batch: &mut Batch,
    ctx: &Ctx<'_>,
    issue: &Issue,
    records: &[Record],
) -> Result<(), Error> {
    crate::issues::finish_change_quiet(conn, batch, issue, ctx.author(), records).await
}

/// The project's field a rule looks up (§3.4): by built-in key first — the whole list, before
/// any name match — else by name in the instance language, kind and multi-value.
pub(crate) fn find_field<'a>(
    fields: &'a ProjectFields,
    needle: &prostor_domain::rules::FieldRef,
    language: Language,
) -> Option<&'a ProjectField> {
    if let Some(key) = needle.builtin_key()
        && let Some(found) = fields
            .0
            .iter()
            .find(|f| f.field.builtin.as_deref() == Some(key))
    {
        return Some(found);
    }
    fields.0.iter().find(|f| {
        needle.matches(
            language,
            f.field.builtin.as_deref(),
            &f.field.name,
            f.kind.kind,
            f.kind.multi,
        )
    })
}

/// Fires [`Event::BecameResolved`] and [`Event::BecameUnresolved`] for the resolution records a
/// change produced (§3.5). Answers what the behaviors answered for the same issue.
pub async fn on_resolution(
    conn: &mut PgConnection,
    batch: &mut Batch,
    ctx: &mut Ctx<'_>,
    issue: &Issue,
    records: &[Record],
) -> Result<Vec<Record>, Error> {
    let mut joined = Vec::new();
    for record in records {
        match record.kind {
            ChangeKind::Resolved => {
                joined.extend(on(conn, batch, ctx, Event::BecameResolved { issue }).await?);
            }
            ChangeKind::Reopened => {
                joined.extend(on(conn, batch, ctx, Event::BecameUnresolved { issue }).await?);
            }
            _ => {}
        }
    }
    Ok(joined)
}

/// The re-entry of a rule's own resolution records (§3.5): the issue as it is now — its
/// resolution time changed — goes through [`on`] again, one level deeper.
async fn climb(
    conn: &mut PgConnection,
    batch: &mut Batch,
    ctx: &mut Ctx<'_>,
    issue: &Issue,
    records: &[Record],
) -> Result<(), Error> {
    if !records
        .iter()
        .any(|record| matches!(record.kind, ChangeKind::Resolved | ChangeKind::Reopened))
    {
        return Ok(());
    }
    let fresh = issues::get(&mut *conn, issue.id)
        .await?
        .ok_or_else(|| Error::Internal("a rule wrote to a gone issue".to_owned()))?;
    ctx.depth += 1;
    // The re-entry closes the engine's cycle (`on` → behavior → `write` → here → `on`); boxing
    // it once per rule-made resolution keeps the future a fixed size.
    let result = Box::pin(on_resolution(conn, batch, ctx, &fresh, records)).await;
    ctx.depth -= 1;
    result.map(|_| ())
}
