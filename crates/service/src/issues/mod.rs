//! Issues (ISS): drafts and publishing, reading by ID or readable ID, the plain list of a project,
//! edits of texts, fields and audience with their history, deletion, stars and votes. Design:
//! decisions.md D-029.
//!
//! Every read goes through the visibility rule (ACC-049): an issue the viewer may not see is "not
//! found". A draft exists only for its reporter; a deleted issue for nobody until the trash (P2).

pub mod activity;
pub mod audience;
pub mod export;
pub mod jobs;
pub mod moves;
pub mod options;
pub mod page;
pub mod values;

use std::collections::BTreeMap;

use sqlx::PgConnection;
use taskmaster_domain::access::Access;
use taskmaster_domain::access::visibility::Audience;
use taskmaster_domain::fields::FieldKind;
use taskmaster_domain::issues::history::{ChangeKind, Origin, Record};
use taskmaster_domain::issues::rights::{self, Facts, VoteRefusal};
use taskmaster_domain::issues::values::{self as rules, Input, Value};
use taskmaster_domain::issues::{self as domain, ReadableId};
use taskmaster_domain::live::Change;
use taskmaster_domain::validation::{Problem, Problems};
use taskmaster_domain::{FieldId, GroupId, IssueId, ProjectId, TagId, UploadId, UserId};
use taskmaster_markup::Profile;
use taskmaster_store::PgPool;
use taskmaster_store::content::{self, TextValueHtml};
use taskmaster_store::groups::{self, Group};
use taskmaster_store::issue_history::{self, Author};
use taskmaster_store::issues::{self, Issue, NewDraft};
use taskmaster_store::projects::{self, Project};
use taskmaster_store::tags::{self as store_tags, Tag};
use taskmaster_store::users::{self, User};
use taskmaster_store::value_sets::{self, Value as SetValue};
use time::OffsetDateTime;

use self::values::{ProjectField, ProjectFields};
use crate::live::Batch;
use crate::{Error, Result};

/// Most issues on one page of the plain list.
pub const PAGE_MAX: i64 = 100;
/// Most drafts listed.
const DRAFTS_MAX: i64 = 50;
/// Most issues read at once by ID.
pub const BATCH_MAX: usize = 100;
/// Empty drafts untouched this long are swept (ISS-010).
pub const EMPTY_DRAFT_IDLE: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);
/// Empty drafts deleted by one statement of the sweep.
const SWEEP_BATCH: i64 = 500;

/// One field of an issue as the viewer sees it.
#[derive(Debug, Clone)]
pub struct FieldView {
    pub field: ProjectField,
    pub value: Option<Value>,
    /// Must not be empty (FLD-056).
    pub required: bool,
    /// The viewer may change it.
    pub editable: bool,
    /// A text value's rendering (D-031).
    pub rendered: Option<TextValueHtml>,
}

/// What the viewer may do with an issue; the UI hides the rest.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Abilities {
    pub update: bool,
    pub update_private: bool,
    pub delete: bool,
    pub manage_watchers: bool,
    pub vote: bool,
    pub silent: bool,
}

/// An issue in full, as one viewer sees it.
#[derive(Debug, Clone)]
pub struct IssueView {
    pub issue: Issue,
    pub project: Project,
    pub fields: Vec<FieldView>,
    /// The viewer's tags on it.
    pub tags: Vec<Tag>,
    pub starred: bool,
    pub voted: bool,
    /// How many people watch it; `None` without `issue.watchers.read`.
    pub watchers: Option<i64>,
    pub can: Abilities,
    /// Its subtasks the viewer may see (LNK-014).
    pub subtasks: crate::issue_links::Subtasks,
    /// Its parent, when the viewer may see it (LNK-013); filled for one issue in full.
    pub parent: Option<(taskmaster_store::links::Linked, Project)>,
}

impl IssueView {
    /// The readable ID; `None` for a draft.
    pub fn readable_id(&self) -> Option<String> {
        self.issue
            .number
            .map(|n| domain::readable_id(&self.project.key, n))
    }
}

/// Issues with what their answers name: accounts, groups and set values.
#[derive(Debug, Clone, Default)]
pub struct Described {
    pub issues: Vec<IssueView>,
    pub users: Vec<User>,
    pub groups: Vec<Group>,
    pub values: Vec<SetValue>,
    /// What the rendered texts refer to; filled for one issue in full, empty in lists.
    pub refs: crate::text::Refs,
    /// How many files of each issue (not of its comments) the viewer sees; filled for cards only.
    pub files: Vec<(IssueId, i64)>,
}

impl Described {
    pub fn user(&self, id: UserId) -> Option<&User> {
        self.users.iter().find(|u| u.id == id)
    }

    pub fn group(&self, id: GroupId) -> Option<&Group> {
        self.groups.iter().find(|g| g.id == id)
    }

    pub fn value(&self, id: i64) -> Option<&SetValue> {
        self.values.iter().find(|v| v.id == id)
    }

    /// The files of an issue the viewer sees, where `files` is filled.
    pub fn files_of(&self, id: IssueId) -> i64 {
        self.files
            .iter()
            .find(|(issue, _)| *issue == id)
            .map_or(0, |(_, n)| *n)
    }

    fn single(self) -> Result<(IssueView, Self)> {
        let mut rest = self;
        let issue = rest.issues.pop().ok_or(Error::NotFound("no such issue"))?;
        Ok((issue, rest))
    }
}

/// One issue in full with its lookups.
pub type One = (IssueView, Described);

pub(crate) fn facts(issue: &Issue) -> Facts<'_> {
    Facts {
        project: issue.project_id,
        reporter: issue.reporter_id,
        draft: issue.is_draft(),
        audience: Audience {
            users: &issue.audience_users,
            groups: &issue.audience_groups,
        },
    }
}

/// Whether the viewer may see an issue in its project (ACC-049, ISS-002).
pub fn can_read(viewer: &Access, issue: &Issue, project: &Project) -> bool {
    !issue.is_deleted() && !project.is_deleting() && rights::can_read(viewer, &facts(issue))
}

pub(crate) async fn checked(
    conn: &mut PgConnection,
    viewer: &Access,
    issue: Option<Issue>,
) -> Result<(Issue, Project)> {
    let issue = issue.ok_or(Error::NotFound("no such issue"))?;
    let project = projects::get(&mut *conn, issue.project_id)
        .await?
        .ok_or(Error::NotFound("no such issue"))?;
    if !can_read(viewer, &issue, &project) {
        return Err(Error::NotFound("no such issue"));
    }
    Ok((issue, project))
}

/// An issue the viewer may see, locked for a change in an active project.
pub(crate) async fn lock_for_change(
    conn: &mut PgConnection,
    viewer: &Access,
    id: IssueId,
) -> Result<(Issue, Project)> {
    let found = issues::lock(&mut *conn, id).await?;
    let (issue, project) = checked(conn, viewer, found).await?;
    crate::projects::require_active(&project)?;
    Ok((issue, project))
}

/// Who writes history: the actor, from where, whether silently (HIST-012). Silence needs
/// `issue.silent`.
pub(crate) fn author(
    actor: &Access,
    origin: Origin,
    silent: bool,
    project: ProjectId,
) -> Result<Author> {
    if silent {
        rights::may_be_silent(actor, project)?;
    }
    Ok(Author {
        user: actor.user(),
        origin,
        silent,
    })
}

/// Writes history for a published issue (drafts have none), marks it changed, records its live
/// event and queues its notification events (plan §3.2). `mentions` are the accounts named by
/// the change's new comment or description edit.
pub(crate) async fn finish_change(
    conn: &mut PgConnection,
    batch: &mut Batch,
    issue: &Issue,
    author: Author,
    records: &[Record],
    mentions: &[UserId],
) -> Result<()> {
    change(conn, batch, issue, author, records, mentions, true).await
}

/// [`finish_change`] without notification events: for records whose events another path already
/// queued — a publication's rule records ride its one `created` event.
pub(crate) async fn finish_change_quiet(
    conn: &mut PgConnection,
    batch: &mut Batch,
    issue: &Issue,
    author: Author,
    records: &[Record],
) -> Result<()> {
    change(conn, batch, issue, author, records, &[], false).await
}

async fn change(
    conn: &mut PgConnection,
    batch: &mut Batch,
    issue: &Issue,
    author: Author,
    records: &[Record],
    mentions: &[UserId],
    events: bool,
) -> Result<()> {
    if records.is_empty() {
        return Ok(());
    }
    if !issue.is_draft() {
        issue_history::record(&mut *conn, issue.id, author, records).await?;
        if events {
            crate::notifications::record(&mut *conn, issue, author.user, records, mentions).await?;
        }
    }
    issues::touch(conn, issue.id, author.user).await?;
    batch.issue(issue.id, Change::Updated);
    Ok(())
}

fn abilities(viewer: &Access, issue: &Issue) -> Abilities {
    let f = facts(issue);
    let update = rights::may_update(viewer, &f).is_ok();
    Abilities {
        update,
        update_private: update && rights::may_update_private(viewer, issue.project_id).is_ok(),
        delete: issue.is_draft() || rights::may_delete(viewer, &f).is_ok(),
        manage_watchers: rights::may_manage_watchers(viewer, issue.project_id).is_ok(),
        vote: rights::may_vote(viewer, &f).is_ok(),
        silent: rights::may_be_silent(viewer, issue.project_id).is_ok(),
    }
}

/// Loads what answers show of issues the viewer may see: one query per relation.
pub(crate) async fn describe(
    conn: &mut PgConnection,
    viewer: &Access,
    list: Vec<Issue>,
) -> Result<Described> {
    if list.is_empty() {
        return Ok(Described::default());
    }
    let ids: Vec<IssueId> = list.iter().map(|i| i.id).collect();
    let mut project_ids: Vec<ProjectId> = list.iter().map(|i| i.project_id).collect();
    project_ids.sort_unstable();
    project_ids.dedup();
    let found_projects = projects::get_many(&mut *conn, &project_ids).await?;
    let fields = values::of_projects(&mut *conn, &project_ids).await?;
    let pairs: Vec<(IssueId, ProjectId)> = list.iter().map(|i| (i.id, i.project_id)).collect();
    let mut all_values = values::of_issues(&mut *conn, &pairs, &fields).await?;
    let tags = store_tags::on_issues(&mut *conn, &ids, viewer.user(), viewer.groups()).await?;
    let starred = issues::watched(&mut *conn, &ids, viewer.user()).await?;
    let voted = issues::voted(&mut *conn, &ids, viewer.user()).await?;
    let counts = issues::watcher_counts(&mut *conn, &ids).await?;
    let mut texts = content::text_values_of(&mut *conn, &ids).await?;
    let subtasks =
        crate::issue_links::subtask_counts(&mut *conn, viewer, &ids, &found_projects).await?;

    let mut user_ids: Vec<UserId> = Vec::new();
    let mut group_ids: Vec<GroupId> = Vec::new();
    let mut value_ids: Vec<i64> = Vec::new();
    let mut views = Vec::with_capacity(list.len());
    for issue in list {
        let Some(project) = found_projects.iter().find(|p| p.id == issue.project_id) else {
            continue;
        };
        user_ids.push(issue.reporter_id);
        user_ids.extend(issue.updated_by);
        user_ids.extend(&issue.audience_users);
        group_ids.extend(&issue.audience_groups);
        let private_read = rights::may_read_private(viewer, issue.project_id);
        let can = abilities(viewer, &issue);
        let mut held = all_values.remove(&issue.id).unwrap_or_default();
        let mut field_views = Vec::new();
        for f in fields
            .get(&issue.project_id)
            .map(|p| p.0.as_slice())
            .unwrap_or_default()
        {
            if f.private() && !private_read {
                continue;
            }
            let value = held.remove(&f.id());
            if let Some(Value::Refs(refs)) = &value {
                match f.kind.kind {
                    FieldKind::User => user_ids.extend(refs),
                    FieldKind::Group => group_ids.extend(refs),
                    _ => value_ids.extend(refs),
                }
            }
            let editable = rules::editable(f.field.builtin.as_deref())
                && if f.private() {
                    can.update_private
                } else {
                    can.update
                };
            let rendered = match &value {
                Some(Value::Text(_)) => texts
                    .iter()
                    .position(|t| t.issue == issue.id && t.field == f.id())
                    .map(|at| texts.swap_remove(at)),
                _ => None,
            };
            field_views.push(FieldView {
                required: !f.attached.can_be_empty,
                editable,
                value,
                rendered,
                field: f.clone(),
            });
        }
        let watchers = rights::may_see_watchers(viewer, issue.project_id).then(|| {
            counts
                .iter()
                .find(|(id, _)| *id == issue.id)
                .map_or(0, |(_, n)| *n)
        });
        views.push(IssueView {
            tags: tags
                .iter()
                .filter(|(id, _)| *id == issue.id)
                .map(|(_, t)| t.clone())
                .collect(),
            starred: starred.contains(&issue.id),
            voted: voted.contains(&issue.id),
            watchers,
            can,
            subtasks: subtasks
                .iter()
                .find(|(id, _)| *id == issue.id)
                .map(|(_, c)| *c)
                .unwrap_or_default(),
            parent: None,
            fields: field_views,
            project: project.clone(),
            issue,
        });
    }
    user_ids.sort_unstable();
    user_ids.dedup();
    group_ids.sort_unstable();
    group_ids.dedup();
    value_ids.sort_unstable();
    value_ids.dedup();
    Ok(Described {
        issues: views,
        users: users::get_many(&mut *conn, &user_ids).await?,
        groups: groups::get_many(&mut *conn, &group_ids).await?,
        values: value_sets::get_values(&mut *conn, &value_ids).await?,
        refs: crate::text::Refs::default(),
        files: Vec::new(),
    })
}

/// One issue in full, with what its description and text values refer to.
async fn one(conn: &mut PgConnection, viewer: &Access, issue: Issue) -> Result<One> {
    let (mut view, mut described) = describe(conn, viewer, vec![issue]).await?.single()?;
    view.parent = crate::issue_links::parent(conn, viewer, view.issue.id).await?;
    let mut mentioned = view.issue.mentioned_users.clone();
    let mut tokens = view.issue.issue_tokens.clone();
    let mut names = view.issue.file_names.clone();
    for f in view.fields.iter().filter_map(|f| f.rendered.as_ref()) {
        mentioned.extend(&f.users);
        tokens.extend(f.issue_tokens.iter().cloned());
        names.extend(f.files.iter().cloned());
    }
    let scope = crate::text::FileScope {
        issue: Some(&view.issue),
        names: &names,
        uploads: &[],
    };
    described.refs = crate::text::refs(conn, viewer, &mentioned, &tokens, Some(scope)).await?;
    Ok((view, described))
}

/// An issue by internal ID (ISS-001).
pub async fn get(pool: &PgPool, viewer: &Access, id: IssueId) -> Result<One> {
    let mut conn = pool.acquire().await?;
    let found = issues::get(&mut *conn, id).await?;
    let (issue, _) = checked(&mut conn, viewer, found).await?;
    one(&mut conn, viewer, issue).await
}

/// Finds an issue by a readable ID, current or former (ISS-002, ISS-003, PRJ-005): `KEY-12` with
/// any key the project ever had, or a number the issue had before it moved. Answers the issue's
/// internal ID and whether the ID given is its current one.
pub async fn resolve(pool: &PgPool, viewer: &Access, readable: &str) -> Result<(IssueId, bool)> {
    let mut conn = pool.acquire().await?;
    resolve_conn(&mut conn, viewer, readable).await
}

/// The conn half of [`resolve`] (step 2.1 §4.5): a command's link and duplicate targets read
/// this way inside the issue's own transaction.
pub(crate) async fn resolve_conn(
    conn: &mut PgConnection,
    viewer: &Access,
    readable: &str,
) -> Result<(IssueId, bool)> {
    let id = ReadableId::parse(readable).ok_or(Error::NotFound("no such issue"))?;
    let project = projects::key_owner(&mut *conn, &id.key)
        .await?
        .ok_or(Error::NotFound("no such issue"))?;
    let issue = issues::by_number(&mut *conn, project, id.number)
        .await?
        .ok_or(Error::NotFound("no such issue"))?;
    let found = issues::get(&mut *conn, issue).await?;
    let (issue, project) = checked(&mut *conn, viewer, found).await?;
    let current = issue.number == Some(id.number) && project.key == id.key;
    Ok((issue.id, current))
}

/// The viewer's drafts, newest first (ISS-002).
pub async fn drafts(pool: &PgPool, viewer: &Access) -> Result<Described> {
    let mut conn = pool.acquire().await?;
    let found = issues::drafts_of(&mut *conn, viewer.user(), DRAFTS_MAX).await?;
    let ids: Vec<ProjectId> = found.iter().map(|i| i.project_id).collect();
    let found_projects = projects::get_many(&mut *conn, &ids).await?;
    let mine = found
        .into_iter()
        .filter(|issue| {
            found_projects
                .iter()
                .find(|p| p.id == issue.project_id)
                .is_some_and(|p| can_read(viewer, issue, p))
        })
        .collect();
    describe(&mut conn, viewer, mine).await
}

/// Issues by internal ID in the order asked (ISS-001): published ones the viewer may read and
/// their own drafts; the others, and repeats, are left out. One query per kind of data.
pub async fn describe_many(pool: &PgPool, viewer: &Access, ids: &[IssueId]) -> Result<Described> {
    if ids.is_empty() {
        return Err(Error::field(
            "ids",
            Problem::Required,
            "ask for at least one issue",
        ));
    }
    if ids.len() > BATCH_MAX {
        return Err(Error::field(
            "ids",
            Problem::TooLong,
            format!("at most {BATCH_MAX} issues at once"),
        ));
    }
    let mut conn = pool.acquire().await?;
    let mut found = issues::get_many(&mut *conn, ids).await?;
    let mut project_ids: Vec<ProjectId> = found.iter().map(|i| i.project_id).collect();
    project_ids.sort_unstable();
    project_ids.dedup();
    let found_projects = projects::get_many(&mut *conn, &project_ids).await?;
    let mut list = Vec::with_capacity(found.len());
    for id in ids {
        let Some(at) = found.iter().position(|i| i.id == *id) else {
            continue;
        };
        let issue = found.swap_remove(at);
        let readable = found_projects
            .iter()
            .find(|p| p.id == issue.project_id)
            .is_some_and(|p| can_read(viewer, &issue, p));
        if readable {
            list.push(issue);
        }
    }
    describe(&mut conn, viewer, list).await
}

/// Deletes all of the viewer's drafts with their files (ISS-011).
pub async fn delete_drafts(pool: &PgPool, viewer: &Access) -> Result<()> {
    let mut tx = pool.begin().await?;
    let me = [viewer.user()];
    let drafts = issues::draft_ids_of(&mut *tx, &me).await?;
    let keys = taskmaster_store::attachments::keys_of_issues(&mut *tx, &drafts).await?;
    crate::files::queue_removal(&mut tx, keys).await?;
    issues::delete_drafts_of(&mut tx, &me).await?;
    tx.commit().await?;
    Ok(())
}

/// Deletes empty drafts untouched for `idle`, in batches (ISS-010): what closing a form could not
/// delete, such as after a crash. Answers how many went.
pub async fn sweep_drafts(pool: &PgPool, idle: std::time::Duration) -> Result<u64> {
    let mut total = 0;
    loop {
        let mut conn = pool.acquire().await?;
        let done = issues::sweep_empty_drafts(&mut conn, idle, SWEEP_BATCH).await?;
        total += done;
        if done < u64::try_from(SWEEP_BATCH).unwrap_or(u64::MAX) {
            return Ok(total);
        }
    }
}

/// An audience as given: accounts and groups (ISS-041).
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AudienceInput {
    pub users: Vec<UserId>,
    pub groups: Vec<GroupId>,
}

/// Checks an audience: accounts that exist, groups the actor may know, the size limit.
pub(crate) async fn check_audience(
    conn: &mut PgConnection,
    actor: &Access,
    input: &AudienceInput,
) -> Result<(Vec<UserId>, Vec<GroupId>)> {
    let mut users_list = input.users.clone();
    users_list.sort_unstable();
    users_list.dedup();
    let mut groups_list = input.groups.clone();
    groups_list.sort_unstable();
    groups_list.dedup();
    domain::check_audience(users_list.len(), groups_list.len())
        .map_err(|r| Error::rejected("audience", r))?;
    let found = users::get_many(&mut *conn, &users_list).await?;
    if users_list.iter().any(|id| {
        !found
            .iter()
            .any(|u| u.id == *id && !u.is_deleted() && !u.is_guest())
    }) {
        return Err(Error::field(
            "audience.users",
            Problem::Invalid,
            "no such account",
        ));
    }
    let found = groups::get_many(&mut *conn, &groups_list).await?;
    if groups_list.iter().any(|id| {
        !found
            .iter()
            .any(|g| g.id == *id && crate::groups::team_visible(actor, g))
    }) {
        return Err(Error::field(
            "audience.groups",
            Problem::Invalid,
            "no such group",
        ));
    }
    Ok((users_list, groups_list))
}

/// Sets an issue's audience inside the caller's transaction (ISS-041, step 2.1 §4.3): the
/// checks and the write of [`update`]'s audience half, the record joining the caller's change.
/// `None`: the audience is this already.
pub(crate) async fn set_visibility_conn(
    conn: &mut PgConnection,
    actor: &Access,
    issue: &Issue,
    audience: &AudienceInput,
) -> Result<Option<Record>> {
    let (users_list, groups_list) = check_audience(&mut *conn, actor, audience).await?;
    if let Some(record) = Record::visibility(
        (&issue.audience_users, &issue.audience_groups),
        (&users_list, &groups_list),
    ) {
        issues::set_audience(&mut *conn, issue.id, &users_list, &groups_list).await?;
        return Ok(Some(record));
    }
    Ok(None)
}

/// A field value on a new issue or draft.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldInput {
    pub field: FieldId,
    pub value: Option<Input>,
}

/// A new issue or draft (ISS-005…ISS-007).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NewIssue {
    pub project: ProjectId,
    pub summary: String,
    pub description: String,
    /// Values that replace the project's defaults.
    pub fields: Vec<FieldInput>,
    /// `None` takes the project's default audience (PRJ-039).
    pub audience: Option<AudienceInput>,
    /// The actor's tags to put on it.
    pub tags: Vec<TagId>,
    /// People to add as watchers; others than the actor need `issue.watchers.update`.
    pub watchers: Vec<UserId>,
    /// Publish at once; otherwise a draft is kept.
    pub publish: bool,
    /// Staged uploads to attach (ISS-007, CNT-017); needs `attachment.create`.
    pub uploads: Vec<UploadId>,
    /// Links to existing issues (ISS-009, LNK-006, LNK-007): a new subtask of an issue is linked
    /// with the subtask type from the target side.
    pub links: Vec<crate::issue_links::NewLinks>,
}

/// Values of a new issue: the project's defaults (FLD-055), then what the actor gave.
async fn initial_values(
    conn: &mut PgConnection,
    actor: &Access,
    fields: &ProjectFields,
    given: &[FieldInput],
    private_update: bool,
    problems: &mut Problems,
) -> Result<BTreeMap<FieldId, Value>> {
    let mut out = BTreeMap::new();
    for f in &fields.0 {
        if let Some(value) = rules::default_value(&f.attached.default_values) {
            out.insert(f.id(), value);
        }
    }
    let schedule = taskmaster_domain::duration::Schedule::default();
    for input in given {
        let path = format!("fields.{}", input.field);
        let Some(f) = fields.get(input.field) else {
            problems.add(
                &path,
                taskmaster_domain::validation::Rejection::new(
                    Problem::Invalid,
                    "the project has no such field",
                ),
            );
            continue;
        };
        if (f.private() && !private_update) || !rules::editable(f.field.builtin.as_deref()) {
            problems.add(
                &path,
                taskmaster_domain::validation::Rejection::new(
                    Problem::Invalid,
                    "you may not set this field",
                ),
            );
            continue;
        }
        let Some(value) =
            problems.check(&path, rules::parse(f.kind, input.value.clone(), schedule))
        else {
            continue;
        };
        if let Some(v) = &value
            && !v.refs().is_empty()
        {
            values::check_refs(conn, actor, f, v.refs(), &[], &path).await?;
        }
        match value {
            Some(v) => out.insert(f.id(), v),
            None => out.remove(&f.id()),
        };
    }
    Ok(out)
}

/// Fields that must be filled before publishing (FLD-057): those that cannot be empty, among the
/// fields the actor may see. Private fields do not block someone who cannot read them (D-029).
fn check_required(
    fields: &ProjectFields,
    values: &BTreeMap<FieldId, Value>,
    private_read: bool,
) -> Result<()> {
    let mut problems = Problems::new();
    for f in &fields.0 {
        if f.private() && !private_read {
            continue;
        }
        if let Err(rejection) = rules::check_not_empty(f.attached.can_be_empty, values.get(&f.id()))
        {
            problems.add(&format!("fields.{}", f.id()), rejection);
        }
    }
    problems.finish().map_err(Error::Invalid)
}

/// Publishes a locked draft inside a transaction (ISS-002): the next number, the record of
/// creation and the resolved state. The project's behaviors hear the publication (step 1.9).
/// `created` is the creation record to write: [`Record::created`], or one naming the original
/// for a clone (AUT-066).
pub(crate) async fn publish_locked(
    tx: &mut PgConnection,
    batch: &mut Batch,
    actor: &Access,
    origin: Origin,
    issue: &Issue,
    ctx: &mut crate::rules::Ctx<'_>,
    created: Record,
) -> Result<()> {
    domain::summary(&issue.summary, true).map_err(|r| Error::rejected("summary", r))?;
    let fields = values::of_project(tx, issue.project_id).await?;
    let held = values::of_issue(tx, issue, &fields).await?;
    check_required(
        &fields,
        &held,
        rights::may_read_private(actor, issue.project_id),
    )?;
    issues::publish(tx, issue.id).await?;
    // Auto-watch (ISS-030): the creator by their own switch, and every account a user field
    // already names at publication — an assignee chosen in the form or a project default.
    crate::notifications::auto_watch(
        tx,
        actor.user(),
        issue.id,
        crate::notifications::AutoWatch::Created,
    )
    .await?;
    for field in &fields.0 {
        if field.kind.kind == FieldKind::User
            && let Some(value) = held.get(&field.id())
        {
            for user in value.refs() {
                crate::notifications::auto_watch(
                    tx,
                    *user,
                    issue.id,
                    crate::notifications::AutoWatch::Assigned,
                )
                .await?;
            }
        }
    }
    let published = issues::get(&mut *tx, issue.id)
        .await?
        .ok_or(Error::NotFound("no such issue"))?;
    let now = published.created_at;
    let mut records = vec![created];
    // A new issue that is resolved at once has its resolution time now.
    if let Some(record) = values::settle_resolution(tx, &published, &fields, &held, now).await?
        && record.kind == ChangeKind::Resolved
    {
        records.push(record);
    }
    records.extend(crate::boards::membership::on_published(tx, batch, &published).await?);
    // The behaviors hear the publication before its records, so what they change on this issue
    // joins them as one change (§3.5).
    let rule_records = crate::rules::on(
        tx,
        batch,
        ctx,
        crate::rules::Event::Published { issue: &published },
    )
    .await?;
    let author = Author {
        user: actor.user(),
        origin,
        silent: false,
    };
    issue_history::record(tx, issue.id, author, &records).await?;
    // One `created` event carries everything the publication did, its rules' records included
    // (plan §3.2); their history write stays quiet about events of its own.
    let mut whole = records.clone();
    whole.extend(rule_records.iter().cloned());
    crate::notifications::record_created(tx, &published, author.user, &whole).await?;
    crate::rules::finish_quiet(tx, batch, ctx, &published, &rule_records).await?;
    crate::issue_links::record_published(tx, batch, author, issue.id).await?;
    batch.issue(issue.id, Change::Created);
    // A draft's files were hidden with it: now they are new to everyone else.
    for file in taskmaster_store::attachments::of_issue(&mut *tx, issue.id).await? {
        batch.attachment(file.id, Change::Created);
    }
    Ok(())
}

/// Creates an issue, or a draft when `publish` is off (ISS-002, ISS-005…ISS-007). Needs
/// `issue.create` in an active project. Defaults fill the fields; required fields must be filled
/// to publish; the audience defaults to the project's (PRJ-039).
pub async fn create(
    pool: &PgPool,
    actor: &Access,
    origin: Origin,
    input: NewIssue,
) -> Result<crate::rules::Answer<One>> {
    let mut problems = Problems::new();
    if input.links.len() > taskmaster_domain::links::ADD_MAX {
        problems.add(
            "links",
            taskmaster_domain::validation::Rejection::new(
                Problem::TooLong,
                "too many links for one issue at once",
            ),
        );
    }
    let summary = problems.check("summary", domain::summary(&input.summary, input.publish));
    let description = problems.check("description", domain::description(&input.description));
    let mut tx = pool.begin().await?;
    let project = projects::lock(&mut tx, input.project)
        .await?
        .filter(|p| crate::projects::can_see(actor, p))
        .ok_or(Error::NotFound("no such project"))?;
    rights::may_create(actor, project.id)?;
    crate::projects::require_active(&project)?;
    let fields = values::of_project(&mut tx, project.id).await?;
    let private_update = rights::may_update_private(actor, project.id).is_ok();
    let initial = initial_values(
        &mut tx,
        actor,
        &fields,
        &input.fields,
        private_update,
        &mut problems,
    )
    .await?;
    problems.finish().map_err(Error::Invalid)?;
    let (Some(summary), Some(description)) = (summary, description) else {
        return Err(Error::Internal("validated values are missing".to_owned()));
    };
    let (audience_users, audience_groups) = match &input.audience {
        Some(audience) => check_audience(&mut tx, actor, audience).await?,
        None => (
            Vec::new(),
            project.default_audience_group_id.into_iter().collect(),
        ),
    };
    let watchers = check_watchers(&mut tx, actor, project.id, &input.watchers).await?;
    let tags = usable_tags(&mut tx, actor, &input.tags).await?;
    let rendered = crate::text::prepare(&mut tx, &description, Profile::Full).await?;
    // A draft's changes publish nothing; publishing it is `issue created`.
    let mut batch = Batch::new();
    let id = issues::insert_draft(
        &mut tx,
        &NewDraft {
            project: project.id,
            reporter: actor.user(),
            summary: &summary,
            description: &description,
            rendered: &rendered,
            audience_users: &audience_users,
            audience_groups: &audience_groups,
        },
    )
    .await?;
    for (field, value) in &initial {
        if let Some(f) = fields.get(*field) {
            values::store(&mut tx, id, *field, f.kind.kind, Some(value)).await?;
        }
    }
    for tag in &tags {
        store_tags::add(&mut tx, id, *tag).await?;
    }
    issues::watch(&mut tx, id, &watchers).await?;
    if !input.uploads.is_empty() {
        let draft = issues::get(&mut *tx, id)
            .await?
            .ok_or(Error::NotFound("no such issue"))?;
        taskmaster_domain::attachments::may_attach(actor, &facts(&draft))?;
        // Files of a draft have no history; publishing records the issue as it is.
        crate::attachments::attach_locked(&mut tx, &mut batch, actor, &draft, None, &input.uploads)
            .await?;
    }
    if !input.links.is_empty() {
        let draft = issues::get(&mut *tx, id)
            .await?
            .ok_or(Error::NotFound("no such issue"))?;
        // A draft's links have no history; publishing records them on the issues they reach.
        for links in input.links {
            crate::issue_links::add_locked(
                &mut tx, &mut batch, actor, origin, &draft, links, false,
            )
            .await?;
        }
    }
    if input.publish {
        let draft = issues::get(&mut *tx, id)
            .await?
            .ok_or(Error::NotFound("no such issue"))?;
        let mut ctx = crate::rules::Ctx::new(actor);
        publish_locked(
            &mut tx,
            &mut batch,
            actor,
            origin,
            &draft,
            &mut ctx,
            Record::created(),
        )
        .await?;
        batch.publish(&mut tx).await?;
        tx.commit().await?;
        let done = get(pool, actor, id).await?;
        return Ok(crate::rules::Answer {
            done,
            messages: ctx.messages,
        });
    }
    batch.publish(&mut tx).await?;
    tx.commit().await?;
    let done = get(pool, actor, id).await?;
    Ok(crate::rules::Answer::plain(done))
}

/// Publishes a draft (ISS-002).
pub async fn publish(
    pool: &PgPool,
    actor: &Access,
    origin: Origin,
    id: IssueId,
) -> Result<crate::rules::Answer<One>> {
    let mut tx = pool.begin().await?;
    let (issue, _) = lock_for_change(&mut tx, actor, id).await?;
    if !issue.is_draft() {
        return Err(Error::Conflict("the issue is published already"));
    }
    let mut batch = Batch::new();
    let mut ctx = crate::rules::Ctx::new(actor);
    publish_locked(
        &mut tx,
        &mut batch,
        actor,
        origin,
        &issue,
        &mut ctx,
        Record::created(),
    )
    .await?;
    batch.publish(&mut tx).await?;
    tx.commit().await?;
    let done = get(pool, actor, id).await?;
    Ok(crate::rules::Answer {
        done,
        messages: ctx.messages,
    })
}

/// The accounts a watcher list names: sorted, deduped, each an existing, non-deleted, non-guest
/// account.
pub(crate) async fn check_accounts(
    conn: &mut PgConnection,
    watchers: &[UserId],
) -> Result<Vec<UserId>> {
    let mut list = watchers.to_vec();
    list.sort_unstable();
    list.dedup();
    let found = users::get_many(&mut *conn, &list).await?;
    if list.iter().any(|id| {
        !found
            .iter()
            .any(|u| u.id == *id && !u.is_deleted() && !u.is_guest())
    }) {
        return Err(Error::field(
            "watchers",
            Problem::Invalid,
            "no such account",
        ));
    }
    Ok(list)
}

pub(crate) async fn check_watchers(
    conn: &mut PgConnection,
    actor: &Access,
    project: ProjectId,
    watchers: &[UserId],
) -> Result<Vec<UserId>> {
    let list = check_accounts(conn, watchers).await?;
    if list.iter().any(|id| *id != actor.user()) {
        rights::may_manage_watchers(actor, project)?;
    }
    Ok(list)
}

async fn usable_tags(conn: &mut PgConnection, actor: &Access, ids: &[TagId]) -> Result<Vec<TagId>> {
    let mut list = ids.to_vec();
    list.sort_unstable();
    list.dedup();
    if list.len() > domain::TAGS_PER_ISSUE_MAX {
        return Err(Error::field("tags", Problem::TooLong, "too many tags"));
    }
    let found = store_tags::get_many(&mut *conn, &list).await?;
    // Tags the actor may use (ISS-037): own, or shared for use.
    if list.iter().any(|id| {
        !found
            .iter()
            .any(|t| t.id == *id && t.sharing().can_use(actor))
    }) {
        return Err(Error::field("tags", Problem::Invalid, "no such tag"));
    }
    Ok(list)
}

/// Changes of the summary, description and audience (ISS-018, ISS-041); `None` keeps a value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IssueChange {
    pub summary: Option<String>,
    pub description: Option<String>,
    /// The text version the edit started from; needed with a summary or description (ISS-051).
    pub text_version: Option<i64>,
    pub audience: Option<AudienceInput>,
    pub silent: bool,
}

/// Changes texts and audience (ISS-018, ISS-041, ISS-051). An edit of the texts carries the text
/// version it started from: if someone changed the texts meanwhile it is refused with a conflict,
/// and the client shows both versions. Audience changes are last-write-wins.
pub async fn update(
    pool: &PgPool,
    actor: &Access,
    origin: Origin,
    id: IssueId,
    change: IssueChange,
) -> Result<crate::rules::Answer<One>> {
    let mut tx = pool.begin().await?;
    let (issue, _) = lock_for_change(&mut tx, actor, id).await?;
    rights::may_update(actor, &facts(&issue))?;
    let who = author(actor, origin, change.silent, issue.project_id)?;
    let mut problems = Problems::new();
    let summary = change
        .summary
        .as_deref()
        .and_then(|raw| problems.check("summary", domain::summary(raw, !issue.is_draft())));
    let description = change
        .description
        .as_deref()
        .and_then(|raw| problems.check("description", domain::description(raw)));
    problems.finish().map_err(Error::Invalid)?;
    let mut records = Vec::new();
    let mut description_mentions: Option<Vec<UserId>> = None;
    if summary.is_some() || description.is_some() {
        match change.text_version {
            None => {
                return Err(Error::field(
                    "text_version",
                    Problem::Required,
                    "send the text version the edit started from",
                ));
            }
            Some(version) if version != issue.text_version => {
                return Err(Error::Conflict(
                    "the summary or description was changed by someone else",
                ));
            }
            Some(_) => {}
        }
        let new_summary = summary.unwrap_or_else(|| issue.summary.clone());
        let new_description = description.unwrap_or_else(|| issue.description.clone());
        records.extend(Record::text(
            ChangeKind::Summary,
            &issue.summary,
            &new_summary,
        ));
        records.extend(Record::text(
            ChangeKind::Description,
            &issue.description,
            &new_description,
        ));
        let mut new_mentions: Option<Vec<UserId>> = None;
        if !records.is_empty() {
            let rendered = crate::text::prepare(&mut tx, &new_description, Profile::Full).await?;
            // Accounts newly named by the description are mentions of this change (NTF-035).
            new_mentions = Some(
                rendered
                    .users
                    .iter()
                    .copied()
                    .filter(|user| !issue.mentioned_users.contains(user))
                    .collect(),
            );
            issues::set_texts(&mut tx, id, &new_summary, &new_description, &rendered).await?;
        }
        description_mentions = new_mentions;
    }
    if let Some(audience) = &change.audience
        && let Some(record) = set_visibility_conn(&mut tx, actor, &issue, audience).await?
    {
        records.push(record);
    }
    let mentions = description_mentions.unwrap_or_default();
    let mut batch = Batch::new();
    finish_change(&mut tx, &mut batch, &issue, who, &records, &mentions).await?;
    batch.publish(&mut tx).await?;
    tx.commit().await?;
    // Texts and audience fire no behavior in P1 (§3.5); the answer carries the field anyway.
    let done = get(pool, actor, id).await?;
    Ok(crate::rules::Answer::plain(done))
}

/// A change of one field (ISS-019, FLD-001).
#[derive(Debug, Clone, PartialEq)]
pub enum FieldChange {
    /// Sets the value; `None` clears it.
    Set(Option<Input>),
    /// Adds references to a multi-valued field.
    Add(Vec<i64>),
    /// Removes references from a multi-valued field.
    Remove(Vec<i64>),
}

/// What [`set_field_locked`] did: the records to write, and the change itself for the behaviors
/// of step 1.9 (§3.5).
pub(crate) struct FieldWritten {
    pub records: Vec<Record>,
    pub field: values::ProjectField,
    pub old: Option<Value>,
    pub new: Option<Value>,
}

impl FieldWritten {
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

/// Changes one field of an issue at once (ISS-019): last write wins, both writes in history
/// (ISS-051). Private fields need `issue.private.update` (FLD-060); a field that cannot be empty
/// cannot be cleared (FLD-057); spent time is read-only. The project's behaviors run on the
/// change (step 1.9), and what they said rides the answer.
pub async fn set_field(
    pool: &PgPool,
    actor: &Access,
    origin: Origin,
    id: IssueId,
    field: FieldId,
    change: FieldChange,
    silent: bool,
) -> Result<crate::rules::Answer<One>> {
    let mut tx = pool.begin().await?;
    let (issue, _) = lock_for_change(&mut tx, actor, id).await?;
    let written = set_field_locked(&mut tx, actor, &issue, field, change).await?;
    let who = author(actor, origin, silent, issue.project_id)?;
    let mut batch = Batch::new();
    // The behaviors run before the change's own records, so what they change on this issue joins
    // them as one change (§3.5).
    let mut ctx = crate::rules::Ctx::new(actor);
    let mut rule_records = Vec::new();
    if !written.is_empty() {
        rule_records.extend(
            crate::rules::on(
                &mut tx,
                &mut batch,
                &mut ctx,
                crate::rules::Event::FieldChanged {
                    issue: &issue,
                    field: &written.field,
                    old: written.old.as_ref(),
                    new: written.new.as_ref(),
                },
            )
            .await?,
        );
        rule_records.extend(
            crate::rules::on_resolution(&mut tx, &mut batch, &mut ctx, &issue, &written.records)
                .await?,
        );
    }
    finish_change(&mut tx, &mut batch, &issue, who, &written.records, &[]).await?;
    crate::rules::finish(&mut tx, &mut batch, &ctx, &issue, &rule_records).await?;
    // A behavior's own write may have settled the resolution of this issue (1.9b does it).
    if rule_records
        .iter()
        .any(|record| matches!(record.kind, ChangeKind::Resolved | ChangeKind::Reopened))
    {
        crate::rules::on_resolution(&mut tx, &mut batch, &mut ctx, &issue, &rule_records).await?;
    }
    batch.publish(&mut tx).await?;
    tx.commit().await?;
    let done = get(pool, actor, id).await?;
    Ok(crate::rules::Answer {
        done,
        messages: ctx.messages,
    })
}

/// Writes one field of a locked issue with the rules of [`set_field`] and settles the resolution;
/// answers the history records, which the caller writes with [`finish_change`] (a board move uses
/// it too, BRD-068), and the change itself for the behaviors.
pub(crate) async fn set_field_locked(
    conn: &mut PgConnection,
    actor: &Access,
    issue: &Issue,
    field: FieldId,
    change: FieldChange,
) -> Result<FieldWritten> {
    let f_issue = facts(issue);
    let fields = values::of_project(&mut *conn, issue.project_id).await?;
    let f = fields
        .get(field)
        .filter(|f| !f.private() || rights::may_read_private(actor, issue.project_id))
        .ok_or(Error::NotFound("the project has no such field"))?
        .clone();
    rights::may_update(actor, &f_issue)?;
    if f.private() {
        rights::may_update_private(actor, issue.project_id)?;
    }
    if !rules::editable(f.field.builtin.as_deref()) {
        return Err(Error::Conflict("this field is computed"));
    }
    let mut held = values::of_issue(&mut *conn, issue, &fields).await?;
    let old = held.get(&field).cloned();
    let new = match change {
        FieldChange::Set(input) => rules::parse(
            f.kind,
            input,
            taskmaster_domain::duration::Schedule::default(),
        )
        .map_err(|r| Error::rejected("value", r))?,
        FieldChange::Add(ids) | FieldChange::Remove(ids) if !f.kind.multi => {
            let _ = ids;
            return Err(Error::field(
                "value",
                Problem::Invalid,
                "this field holds one value",
            ));
        }
        FieldChange::Add(ids) => rules::add(old.as_ref(), &ids),
        FieldChange::Remove(ids) => rules::remove(old.as_ref(), &ids),
    };
    if let Some(v) = &new {
        let current = old.as_ref().map(Value::refs).unwrap_or_default();
        values::check_refs(&mut *conn, actor, &f, v.refs(), current, "value").await?;
    }
    if !issue.is_draft() {
        rules::check_not_empty(f.attached.can_be_empty, new.as_ref())
            .map_err(|r| Error::rejected("value", r))?;
    }
    let mut records = Vec::new();
    records.extend(values::write(&mut *conn, issue.id, &f, old.as_ref(), new.as_ref()).await?);
    if !records.is_empty() && f.kind.kind == FieldKind::State {
        match &new {
            Some(v) => held.insert(field, v.clone()),
            None => held.remove(&field),
        };
        let now = OffsetDateTime::now_utc();
        records.extend(values::settle_resolution(&mut *conn, issue, &fields, &held, now).await?);
    }
    Ok(FieldWritten {
        records,
        field: f,
        old,
        new,
    })
}

/// Deletes an issue (ISS-028): it disappears from every list and stays whole for the trash (P2).
/// A draft goes for good.
pub async fn delete(pool: &PgPool, actor: &Access, origin: Origin, id: IssueId) -> Result<()> {
    let mut tx = pool.begin().await?;
    let (issue, _) = lock_for_change(&mut tx, actor, id).await?;
    let mut batch = Batch::new();
    delete_conn(&mut tx, &mut batch, actor, origin, &issue).await?;
    if !batch.is_empty() {
        batch.publish(&mut tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Deletes a locked issue inside the caller's transaction (step 2.1 §4.3): both halves of
/// [`delete`], the live event joining the caller's `batch`.
pub(crate) async fn delete_conn(
    conn: &mut PgConnection,
    batch: &mut Batch,
    actor: &Access,
    origin: Origin,
    issue: &Issue,
) -> Result<()> {
    if issue.is_draft() {
        let keys = taskmaster_store::attachments::keys_of_issues(&mut *conn, &[issue.id]).await?;
        crate::files::queue_removal(&mut *conn, keys).await?;
        issues::delete_draft(&mut *conn, issue.id).await?;
    } else {
        rights::may_delete(actor, &facts(issue))?;
        issues::mark_deleted(&mut *conn, issue.id, actor.user()).await?;
        issue_history::record(
            &mut *conn,
            issue.id,
            author(actor, origin, false, issue.project_id)?,
            &[Record::deleted()],
        )
        .await?;
        batch.issue(issue.id, Change::Deleted);
    }
    Ok(())
}

/// Counters in the issue's shape changed (stars, votes): its version grows and its topic hears.
pub(crate) async fn bump(conn: &mut PgConnection, id: IssueId) -> Result<()> {
    issues::bump(&mut *conn, id).await?;
    let mut batch = Batch::new();
    batch.issue(id, Change::Updated);
    batch.publish(conn).await
}

/// Stars or unstars an issue for the viewer (ISS-030): watching needs only reading it.
pub async fn set_star(pool: &PgPool, viewer: &Access, id: IssueId, on: bool) -> Result<()> {
    let mut tx = pool.begin().await?;
    let found = issues::get(&mut *tx, id).await?;
    let (issue, _) = checked(&mut tx, viewer, found).await?;
    set_star_conn(&mut tx, &issue, on, viewer.user()).await?;
    tx.commit().await?;
    Ok(())
}

/// The star write of a checked issue inside the caller's transaction (step 2.1 §4.3): a
/// command stars for itself (any signed-in reader) or names another account, which the apply
/// checks `issue.watchers.update` for before it gets here.
pub(crate) async fn set_star_conn(
    conn: &mut PgConnection,
    issue: &Issue,
    on: bool,
    user: UserId,
) -> Result<()> {
    let changed = if on {
        issues::watch(&mut *conn, issue.id, &[user]).await? > 0
    } else {
        issues::unwatch(&mut *conn, issue.id, user).await?
    };
    if changed {
        bump(conn, issue.id).await?;
    }
    Ok(())
}

/// Votes for an issue or withdraws the vote (ISS-032): anyone who reads it, but not its reporter.
pub async fn set_vote(pool: &PgPool, viewer: &Access, id: IssueId, on: bool) -> Result<()> {
    let mut tx = pool.begin().await?;
    let found = issues::lock(&mut tx, id).await?;
    let (issue, _) = checked(&mut tx, viewer, found).await?;
    match rights::may_vote(viewer, &facts(&issue)) {
        Err(VoteRefusal::OwnIssue) => {
            return Err(Error::Conflict("you cannot vote for an issue you reported"));
        }
        Err(VoteRefusal::Draft) => return Err(Error::Conflict("a draft takes no votes")),
        Ok(()) => {}
    }
    set_vote_conn(&mut tx, viewer, &issue, on).await?;
    tx.commit().await?;
    Ok(())
}

/// The vote write of a checked issue inside the caller's transaction (step 2.1 §4.3); the
/// apply runs `may_vote` itself so its refusal can name the reason (`cmd_own_issue_vote`).
pub(crate) async fn set_vote_conn(
    conn: &mut PgConnection,
    viewer: &Access,
    issue: &Issue,
    on: bool,
) -> Result<()> {
    let changed = if on {
        issues::vote(&mut *conn, issue.id, viewer.user()).await?
    } else {
        issues::unvote(&mut *conn, issue.id, viewer.user()).await?
    };
    if changed {
        bump(conn, issue.id).await?;
        crate::notifications::record_vote(conn, issue, viewer.user(), on).await?;
        // Auto-watch (ISS-030): a vote makes the voter a watcher by their own switch's word.
        if on {
            crate::notifications::auto_watch(
                conn,
                viewer.user(),
                issue.id,
                crate::notifications::AutoWatch::Voted,
            )
            .await?;
        }
    }
    Ok(())
}
