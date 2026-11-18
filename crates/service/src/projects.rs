//! Projects (PRJ): the list, creating from a template, settings, favorites, archive, ownership
//! and deletion in the background. Design: decisions.md D-027.
//!
//! Who may see a project: whoever holds `project.basic` there (PRJ-009); everyone else gets "not
//! found", so existence never leaks. A project being deleted is visible to nobody but the list of
//! deletions of those who may delete it.

use prostor_domain::access::permission::Permission;
use prostor_domain::access::role::PredefinedRole;
use prostor_domain::access::{Access, Projects, Scope};
use prostor_domain::projects::{
    KEY_MIN, ProjectKey, Template, first_number, project_description, project_name, suggest_key,
};
use prostor_domain::settings::Language;
use prostor_domain::validation::{FieldProblem, Problem, Problems};
use prostor_domain::{Actor, GroupId, ProjectId, RoleId, UserId};
use prostor_store::access::{self as assignments, Holder};
use prostor_store::fields as store_fields;
use prostor_store::groups::{self, Group};
use prostor_store::jobs::{self, Enqueue, Job, JobError, JobKind};
use prostor_store::projects::{self, ListCursor, NewProject, Project, Section};
use prostor_store::users::{self, User};
use prostor_store::{PgPool, audit, roles, rules, teams, unique_violation, value_sets};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgConnection;

use crate::access::changed;
use crate::seed::{self, NewProjectFields};
use crate::{Error, Result};

const SEARCH_MAX: usize = 100;

/// A project with what its pages show: owner, team, former keys, default audience, the viewer's
/// star and, while it is being deleted, the deletion job.
#[derive(Debug, Clone)]
pub struct ProjectRow {
    pub project: Project,
    pub favorite: bool,
    pub owner: Option<User>,
    pub team: Option<Group>,
    pub team_size: i64,
    pub former_keys: Vec<String>,
    pub audience: Option<Group>,
    pub deletion: Option<Job>,
}

/// Whether a person may see a project (PRJ-009).
pub fn can_see(access: &Access, project: &Project) -> bool {
    !project.is_deleting() && access.has_in(Permission::ProjectBasic, project.id)
}

/// Refuses unless the person holds the permission in the project; the project is visible to them.
pub(crate) fn require(access: &Access, permission: Permission, project: ProjectId) -> Result<()> {
    access.require_at(permission, Scope::Project(project))?;
    Ok(())
}

/// Refuses changes to an archived project (PRJ-061): it is read-only until restored.
pub(crate) fn require_active(project: &Project) -> Result<()> {
    if project.is_archived() {
        return Err(Error::Conflict("the project is archived"));
    }
    Ok(())
}

/// A project the viewer may see, or "not found".
pub(crate) async fn visible(
    conn: &mut PgConnection,
    viewer: &Access,
    id: ProjectId,
) -> Result<Project> {
    projects::get(&mut *conn, id)
        .await?
        .filter(|project| can_see(viewer, project))
        .ok_or(Error::NotFound("no such project"))
}

/// The same, locked for a change.
pub(crate) async fn lock_visible(
    conn: &mut PgConnection,
    viewer: &Access,
    id: ProjectId,
) -> Result<Project> {
    projects::lock(conn, id)
        .await?
        .filter(|project| can_see(viewer, project))
        .ok_or(Error::NotFound("no such project"))
}

/// Loads what the pages show next to projects: one query per relation.
pub(crate) async fn describe(
    pool: &PgPool,
    viewer: &Access,
    list: Vec<(Project, bool)>,
) -> Result<Vec<ProjectRow>> {
    let mut conn = pool.acquire().await?;
    let ids: Vec<ProjectId> = list.iter().map(|(p, _)| p.id).collect();
    let owner_ids: Vec<UserId> = list.iter().map(|(p, _)| p.owner_id).collect();
    let owners = users::get_many(&mut *conn, &owner_ids).await?;
    let team_ids = teams::of_projects(&mut *conn, &ids).await?;
    let only_teams: Vec<GroupId> = team_ids.iter().map(|(_, team)| *team).collect();
    let sizes = teams::sizes(&mut *conn, &only_teams).await?;
    let mut group_ids = only_teams.clone();
    group_ids.extend(list.iter().filter_map(|(p, _)| p.default_audience_group_id));
    let found_groups = groups::get_many(&mut *conn, &group_ids).await?;
    let former = projects::former_keys(&mut *conn, &ids).await?;
    drop(conn);
    let mut rows = Vec::with_capacity(list.len());
    for (project, favorite) in list {
        let deletion = match project.deletion_job_id {
            Some(job) if viewer.has_in(Permission::ProjectDelete, project.id) => {
                jobs::get(pool, job).await?
            }
            _ => None,
        };
        let team_id = team_ids
            .iter()
            .find(|(p, _)| *p == project.id)
            .map(|(_, team)| *team);
        rows.push(ProjectRow {
            owner: owners.iter().find(|u| u.id == project.owner_id).cloned(),
            team: team_id.and_then(|id| found_groups.iter().find(|g| g.id == id).cloned()),
            team_size: team_id
                .and_then(|id| sizes.iter().find(|(team, _)| *team == id))
                .map_or(0, |(_, size)| *size),
            former_keys: former
                .iter()
                .filter(|(p, _)| *p == project.id)
                .map(|(_, key)| key.clone())
                .collect(),
            audience: project
                .default_audience_group_id
                .and_then(|id| found_groups.iter().find(|g| g.id == id).cloned()),
            deletion,
            favorite,
            project,
        });
    }
    Ok(rows)
}

fn search_text(text: &str) -> String {
    text.trim().chars().take(SEARCH_MAX).collect()
}

/// A page of projects (PRJ-010, PRJ-012, PRJ-013): the active ones with the viewer's favorites
/// first, the archived ones, or the ones being deleted (only those the viewer may delete).
pub async fn list(
    pool: &PgPool,
    viewer: &Access,
    section: Section,
    text: &str,
    after: Option<ListCursor>,
    limit: i64,
) -> Result<Vec<ProjectRow>> {
    list_filtered(pool, viewer, section, text, false, after, limit).await
}

/// `list`; with `in_teams` only projects whose team includes the viewer, directly or through a
/// group («Все мои проекты» of a new board, BRD-003).
pub async fn list_filtered(
    pool: &PgPool,
    viewer: &Access,
    section: Section,
    text: &str,
    in_teams: bool,
    after: Option<ListCursor>,
    limit: i64,
) -> Result<Vec<ProjectRow>> {
    let permission = match section {
        Section::Deleting => Permission::ProjectDelete,
        Section::Active | Section::Archived => Permission::ProjectBasic,
    };
    let mut visible = viewer.projects_with(permission);
    if in_teams {
        let teams: Vec<ProjectId> = teams::projects_of_users(pool, &[viewer.user()])
            .await?
            .into_iter()
            .map(|(_, project)| project)
            .collect();
        visible = match visible {
            Projects::All => Projects::Only(teams),
            Projects::Only(ids) => {
                Projects::Only(ids.into_iter().filter(|id| teams.contains(id)).collect())
            }
        };
    }
    if visible == Projects::Only(Vec::new()) {
        return Ok(Vec::new());
    }
    let ids = match &visible {
        Projects::All => None,
        Projects::Only(ids) => Some(ids.as_slice()),
    };
    let after = after.unwrap_or((-1, String::new(), 0));
    let mut conn = pool.acquire().await?;
    let rows = projects::list(
        &mut conn,
        viewer.user(),
        ids,
        section,
        &search_text(text),
        &after,
        limit,
    )
    .await?;
    drop(conn);
    describe(
        pool,
        viewer,
        rows.into_iter().map(|r| (r.project, r.favorite)).collect(),
    )
    .await
}

async fn row(pool: &PgPool, viewer: &Access, project: Project) -> Result<ProjectRow> {
    let favorite = !projects::favorites_among(pool, viewer.user(), &[project.id])
        .await?
        .is_empty();
    describe(pool, viewer, vec![(project, favorite)])
        .await?
        .pop()
        .ok_or_else(|| Error::Internal("the project is gone".to_owned()))
}

/// One project (PRJ-014).
pub async fn get(pool: &PgPool, viewer: &Access, id: ProjectId) -> Result<ProjectRow> {
    let mut conn = pool.acquire().await?;
    let project = visible(&mut conn, viewer, id).await?;
    drop(conn);
    row(pool, viewer, project).await
}

/// The project with a key, current or former (PRJ-005): the caller compares the keys and
/// redirects to the current one.
pub async fn by_key(pool: &PgPool, viewer: &Access, key: &str) -> Result<ProjectRow> {
    let project = projects::by_key(pool, key.trim())
        .await?
        .filter(|project| can_see(viewer, project))
        .ok_or(Error::NotFound("no such project"))?;
    row(pool, viewer, project).await
}

/// What the wizard shows before creating (PRJ-018): a key suggested for the name and the problems
/// of the name and key as typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub suggested_key: Option<String>,
    pub problems: Vec<FieldProblem>,
}

/// Suggests a free key for a name: every key some project has or had counts as taken.
async fn suggestion(conn: &mut PgConnection, name: &str) -> Result<String> {
    let base = suggest_key(name, |_| false);
    let prefix: String = base.chars().take(KEY_MIN).collect();
    let taken = projects::keys_starting_with(&mut *conn, &prefix).await?;
    Ok(suggest_key(name, |key| taken.iter().any(|t| t == key)))
}

/// Checks a name and a key without creating anything (PRJ-018): for the creation wizard and for
/// renaming a project. Needs `project.create`, or `project.update` in some project.
pub async fn check(
    pool: &PgPool,
    actor: &Access,
    name: Option<&str>,
    key: Option<&str>,
) -> Result<Check> {
    if !actor.has_anywhere(Permission::ProjectUpdate) {
        actor.require(Permission::ProjectCreate)?;
    }
    let mut problems = Problems::new();
    let mut conn = pool.acquire().await?;
    let mut suggested_key = None;
    if let Some(raw) = name
        && let Some(name) = problems.check("name", project_name(raw))
    {
        if projects::with_name(&mut *conn, &name).await?.is_some() {
            problems.add(
                "name",
                prostor_domain::validation::Rejection::new(
                    Problem::Taken,
                    "another project has this name",
                ),
            );
        }
        suggested_key = Some(suggestion(&mut conn, &name).await?);
    }
    if let Some(raw) = key
        && let Some(key) = problems.check("key", ProjectKey::parse(raw))
        && !projects::keys_taken(&mut *conn, &[key.as_str().to_owned()])
            .await?
            .is_empty()
    {
        problems.add(
            "key",
            prostor_domain::validation::Rejection::new(
                Problem::Taken,
                "another project has or had this key",
            ),
        );
    }
    Ok(Check {
        suggested_key,
        problems: problems.into_vec(),
    })
}

/// A project to create (PRJ-017).
#[derive(Debug, Clone)]
pub struct NewProjectInput {
    pub template: Template,
    pub name: String,
    /// `None`: the suggested key.
    pub key: Option<String>,
    pub first_number: Option<i64>,
    pub description: String,
}

/// Creates a project from a template (PRJ-017, PRJ-019, PRJ-021). The creator becomes its owner
/// and project administrator and joins its team, whose members are project members by default;
/// the template's fields are attached in the instance language.
pub async fn create(
    pool: &PgPool,
    actor: &Access,
    input: NewProjectInput,
    language: Language,
) -> Result<ProjectRow> {
    actor.require(Permission::ProjectCreate)?;
    let mut problems = Problems::new();
    let name = problems.check("name", project_name(&input.name));
    let key = input
        .key
        .as_deref()
        .filter(|raw| !raw.trim().is_empty())
        .and_then(|raw| problems.check("key", ProjectKey::parse(raw)));
    let number = problems.check(
        "first_number",
        first_number(input.first_number.unwrap_or(1)),
    );
    let description = problems.check("description", project_description(&input.description));
    problems.finish().map_err(Error::Invalid)?;
    let (Some(name), Some(number), Some(description)) = (name, number, description) else {
        return Err(Error::Internal("validated values are missing".to_owned()));
    };
    let mut tx = pool.begin().await?;
    let key = match key {
        Some(key) => key.into_string(),
        None => suggestion(&mut tx, &name).await?,
    };
    let rendered =
        crate::text::prepare(&mut tx, &description, prostor_markup::Profile::Basic).await?;
    let id = projects::insert(
        &mut tx,
        &NewProject {
            key: &key,
            name: &name,
            description: &description,
            description_html: &rendered.html,
            render_version: rendered.version,
            template: input.template.key(),
            owner: actor.user(),
            next_number: number,
            created_by: Some(actor.user()),
        },
    )
    .await
    .map_err(taken)?;
    let team = teams::create(&mut tx, id, &name).await?;
    groups::add_members(&mut tx, team, &[actor.user()]).await?;
    let admin = predefined(&mut tx, PredefinedRole::ProjectAdmin).await?;
    let member = predefined(&mut tx, PredefinedRole::ProjectMember).await?;
    assignments::insert(
        &mut tx,
        admin,
        Holder::User(actor.user()),
        Some(id),
        Some(actor.user()),
    )
    .await?;
    assignments::insert(
        &mut tx,
        member,
        Holder::Group(team),
        Some(id),
        Some(actor.user()),
    )
    .await?;
    seed::attach_template(
        &mut tx,
        &NewProjectFields {
            project: id,
            project_name: &name,
            team,
            template: input.template,
            language,
        },
    )
    .await?;
    rules::seed_project(&mut tx, id, input.template).await?;
    // PRJ-019, NTF-027: the creator is subscribed to the project's "unassigned" search, for new
    // items only.
    if let Some(search) =
        crate::saved_searches::create_unassigned(&mut tx, actor.user(), id, &key, language).await?
    {
        crate::notifications::subscribe_unassigned(&mut tx, actor.user(), search).await?;
    }
    template_board(
        &mut tx,
        actor.user(),
        input.template,
        id,
        &name,
        team,
        language,
    )
    .await?;
    audit::record(
        &mut tx,
        actor_of(actor),
        "project.created",
        &target(id),
        json!({ "key": key, "name": name, "template": input.template.key() }),
    )
    .await?;
    changed(&mut tx).await?;
    tx.commit().await?;
    // The creator's access changed; compile it again to show the new project.
    let access = crate::access::load(pool, actor.user()).await?;
    get(pool, &access, id).await
}

/// The board of a scrum or kanban project (PRJ-023, PRJ-024; D-044): the preset of the
/// template over the new project, named after it, seen by its team, with its backlog.
async fn template_board(
    conn: &mut PgConnection,
    owner: UserId,
    template: Template,
    project: ProjectId,
    name: &str,
    team: GroupId,
    language: Language,
) -> Result<()> {
    use prostor_domain::boards::Preset;
    use prostor_domain::sharing::Audience;
    let preset = match template {
        Template::Scrum => Preset::Scrum,
        Template::Kanban => Preset::Kanban,
        Template::Blank => return Ok(()),
    };
    let name: String = name
        .chars()
        .take(prostor_domain::boards::NAME_MAX)
        .collect();
    let team = Audience {
        users: Vec::new(),
        groups: vec![team],
    };
    crate::boards::found(
        conn,
        crate::boards::Founding {
            owner,
            preset,
            name,
            projects: vec![project],
            sharing: crate::boards::Sharing {
                view: team.clone(),
                edit: Audience::default(),
                view_readers: false,
                edit_updaters: false,
            },
            backlog_view: team,
            language,
        },
    )
    .await?;
    Ok(())
}

pub(crate) async fn predefined(conn: &mut PgConnection, role: PredefinedRole) -> Result<RoleId> {
    roles::builtin_id(&mut *conn, role.key())
        .await?
        .ok_or_else(|| Error::Internal(format!("the role {} is missing", role.key())))
}

fn taken(error: sqlx::Error) -> Error {
    match unique_violation(&error) {
        Some(projects::KEY_KEY | projects::CURRENT_KEY_KEY) => {
            Error::field("key", Problem::Taken, "another project has or had this key")
        }
        Some(projects::NAME_KEY) => {
            Error::field("name", Problem::Taken, "another project has this name")
        }
        _ => error.into(),
    }
}

/// Changes of the general settings (PRJ-038, PRJ-039); `None` keeps a value.
#[derive(Debug, Clone, Default)]
pub struct ProjectChange {
    pub name: Option<String>,
    pub key: Option<String>,
    pub description: Option<String>,
    /// `Some(None)` clears the default audience.
    pub audience: Option<Option<GroupId>>,
    pub version: i64,
}

/// Changes name, key, description and default audience (PRJ-002, PRJ-005, PRJ-006, PRJ-039).
/// A new key keeps the old one reserved for the project, so old IDs still lead to it.
pub async fn update(
    pool: &PgPool,
    actor: &Access,
    id: ProjectId,
    change: ProjectChange,
) -> Result<ProjectRow> {
    let mut problems = Problems::new();
    let name = change
        .name
        .as_deref()
        .and_then(|raw| problems.check("name", project_name(raw)));
    let key = change
        .key
        .as_deref()
        .and_then(|raw| problems.check("key", ProjectKey::parse(raw)));
    let description = change
        .description
        .as_deref()
        .and_then(|raw| problems.check("description", project_description(raw)));
    problems.finish().map_err(Error::Invalid)?;
    let mut tx = pool.begin().await?;
    let project = lock_visible(&mut tx, actor, id).await?;
    require(actor, Permission::ProjectUpdate, id)?;
    require_active(&project)?;
    if project.version != change.version {
        return Err(Error::Conflict("the project was changed by someone else"));
    }
    let mut details = serde_json::Map::new();
    if name.is_some() || description.is_some() {
        // The project description has the basic profile (PRJ-006, CNT-039).
        let rendered = match &description {
            Some(text) => {
                Some(crate::text::prepare(&mut tx, text, prostor_markup::Profile::Basic).await?)
            }
            None => None,
        };
        let described = description
            .as_deref()
            .zip(rendered.as_ref())
            .map(|(text, r)| projects::Description {
                text,
                html: &r.html,
                render_version: r.version,
            });
        projects::update(&mut tx, id, name.as_deref(), described)
            .await
            .map_err(taken)?;
        if let Some(name) = &name {
            teams::rename(&mut tx, id, name).await?;
            details.insert("name".into(), json!({ "old": project.name, "new": name }));
        }
        if description.is_some() {
            details.insert("description".into(), json!(true));
        }
    }
    if let Some(key) = key
        && key.as_str() != project.key
    {
        match projects::key_owner(&mut tx, key.as_str()).await? {
            Some(owner) if owner != id => {
                return Err(Error::field(
                    "key",
                    Problem::Taken,
                    "another project has or had this key",
                ));
            }
            _ => {}
        }
        projects::set_key(&mut tx, id, key.as_str()).await?;
        details.insert(
            "key".into(),
            json!({ "old": project.key, "new": key.as_str() }),
        );
    }
    if let Some(audience) = change.audience
        && audience != project.default_audience_group_id
    {
        if let Some(group) = audience
            && groups::get(&mut *tx, group).await?.is_none()
        {
            return Err(Error::field("audience", Problem::Invalid, "no such group"));
        }
        projects::set_audience(&mut tx, id, audience).await?;
        details.insert(
            "audience".into(),
            json!({ "old": project.default_audience_group_id, "new": audience }),
        );
    }
    if !details.is_empty() {
        audit::record(
            &mut tx,
            actor_of(actor),
            "project.updated",
            &target(id),
            serde_json::Value::Object(details),
        )
        .await?;
    }
    tx.commit().await?;
    get(pool, actor, id).await
}

/// Stars or unstars a project for the viewer (PRJ-012).
pub async fn set_favorite(pool: &PgPool, viewer: &Access, id: ProjectId, on: bool) -> Result<()> {
    let mut tx = pool.begin().await?;
    visible(&mut tx, viewer, id).await?;
    if on {
        projects::add_favorite(&mut tx, viewer.user(), id).await?;
    } else {
        projects::remove_favorite(&mut tx, viewer.user(), id).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Archives (PRJ-061) or restores (PRJ-063) a project. Needs `project.update`.
pub async fn set_archived(
    pool: &PgPool,
    actor: &Access,
    id: ProjectId,
    archived: bool,
) -> Result<ProjectRow> {
    let mut tx = pool.begin().await?;
    let project = lock_visible(&mut tx, actor, id).await?;
    require(actor, Permission::ProjectUpdate, id)?;
    if project.is_archived() == archived {
        return Err(Error::Conflict(if archived {
            "the project is archived already"
        } else {
            "the project is not archived"
        }));
    }
    projects::set_archived(&mut tx, id, archived).await?;
    audit::record(
        &mut tx,
        actor_of(actor),
        if archived {
            "project.archived"
        } else {
            "project.restored"
        },
        &target(id),
        json!({ "key": project.key }),
    )
    .await?;
    tx.commit().await?;
    get(pool, actor, id).await
}

/// Gives the project a new owner (PRJ-053), who also becomes its administrator. The previous
/// owner keeps their roles. Needs `project.update` and the permissions of the administrator role
/// there (ACC-050).
pub async fn transfer(
    pool: &PgPool,
    actor: &Access,
    id: ProjectId,
    owner: UserId,
) -> Result<ProjectRow> {
    let mut tx = pool.begin().await?;
    let project = lock_visible(&mut tx, actor, id).await?;
    require(actor, Permission::ProjectUpdate, id)?;
    require_active(&project)?;
    actor.may_pass_on(
        PredefinedRole::ProjectAdmin.permissions(),
        Scope::Project(id),
    )?;
    let user = users::lock(&mut tx, owner)
        .await?
        .filter(|user| !user.is_deleted() && !user.is_guest())
        .ok_or_else(|| Error::field("owner", Problem::Invalid, "no such account"))?;
    if user.disabled_at.is_some() {
        return Err(Error::field(
            "owner",
            Problem::Invalid,
            "a disabled account cannot own a project",
        ));
    }
    if user.id != project.owner_id {
        let admin = predefined(&mut tx, PredefinedRole::ProjectAdmin).await?;
        assignments::insert(
            &mut tx,
            admin,
            Holder::User(user.id),
            Some(id),
            Some(actor.user()),
        )
        .await?;
        projects::set_owner(&mut tx, id, user.id).await?;
        users::touch(&mut tx, &[user.id]).await?;
        audit::record(
            &mut tx,
            actor_of(actor),
            "project.owner_changed",
            &target(id),
            json!({ "old": project.owner_id, "new": user.id }),
        )
        .await?;
        changed(&mut tx).await?;
    }
    tx.commit().await?;
    get(pool, actor, id).await
}

/// Deletes a project's content and the project in the background (PRJ-064, PRJ-065).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectDeletion {
    pub project: ProjectId,
}

impl JobKind for ProjectDeletion {
    const KIND: &'static str = "project.delete";
    const CANCELLABLE: bool = false;
    const MAX_ATTEMPTS: i32 = 10;
}

/// Starts deleting a project (PRJ-064, PRJ-065): from now on nobody can use it, and a job removes
/// its content and then the project. Needs `project.delete`. Asking again while it is being
/// deleted starts the job again when the last one failed.
pub async fn delete(pool: &PgPool, actor: &Access, id: ProjectId) -> Result<()> {
    let mut tx = pool.begin().await?;
    let project = projects::lock(&mut tx, id)
        .await?
        .filter(|project| {
            actor.has_in(Permission::ProjectBasic, project.id)
                || actor.has_in(Permission::ProjectDelete, project.id)
        })
        .ok_or(Error::NotFound("no such project"))?;
    require(actor, Permission::ProjectDelete, id)?;
    let job = jobs::enqueue(
        &mut tx,
        &ProjectDeletion { project: id },
        Enqueue {
            initiator: actor_of(actor),
            unique_key: Some(format!("project.delete:{id}")),
            ..Enqueue::default()
        },
    )
    .await?;
    projects::mark_deleting(&mut tx, id, job.id).await?;
    if !project.is_deleting() {
        audit::record(
            &mut tx,
            actor_of(actor),
            "project.deletion_started",
            &target(id),
            json!({ "key": project.key, "name": project.name, "job": job.id }),
        )
        .await?;
    }
    changed(&mut tx).await?;
    tx.commit().await?;
    Ok(())
}

/// The deletion job (PRJ-065). Idempotent: a project already gone is done. Content (issues, and
/// articles in P2) is removed first in batches; then the project with its keys, team,
/// role assignments, favorites and fields, and the value sets nobody else uses.
pub async fn run_deletion(pool: &PgPool, job: ProjectDeletion) -> Result<(), JobError> {
    // Issues first, each batch its own transaction (PRJ-064).
    loop {
        let mut tx = pool.begin().await?;
        let Some(project) = projects::lock(&mut tx, job.project).await? else {
            return Ok(());
        };
        if !project.is_deleting() {
            return Err(JobError::Fail(
                "the project is not marked for deletion".to_owned(),
            ));
        }
        let batch = prostor_store::issues::ids_of_project(&mut *tx, project.id, 0, 500).await?;
        if batch.is_empty() {
            break;
        }
        // The batch's files leave the file store after the rows (D-033).
        let keys = prostor_store::attachments::keys_of_issues(&mut *tx, &batch).await?;
        crate::files::queue_removal(&mut tx, keys)
            .await
            .map_err(|e| JobError::Retry(e.to_string()))?;
        prostor_store::issues::delete_ids(&mut tx, &batch).await?;
        tx.commit().await?;
    }
    let mut tx = pool.begin().await?;
    let Some(project) = projects::lock(&mut tx, job.project).await? else {
        return Ok(());
    };
    if !project.is_deleting() {
        return Err(JobError::Fail(
            "the project is not marked for deletion".to_owned(),
        ));
    }
    let sets = store_fields::sets_of_project(&mut *tx, project.id).await?;
    crate::files::queue_removal(&mut tx, project.logo_key.clone().into_iter().collect())
        .await
        .map_err(|e| JobError::Retry(e.to_string()))?;
    projects::delete(&mut tx, project.id).await?;
    value_sets::delete_unused(&mut tx, &sets).await?;
    audit::record(
        &mut tx,
        Actor::System,
        "project.deleted",
        &target(project.id),
        json!({ "key": project.key, "name": project.name }),
    )
    .await?;
    prostor_store::bus::notify(&mut tx, prostor_store::bus::ACCESS, "").await?;
    tx.commit().await?;
    Ok(())
}

pub(crate) fn target(id: ProjectId) -> String {
    format!("project:{id}")
}

pub(crate) fn actor_of(access: &Access) -> Actor {
    Actor::User(access.user())
}
