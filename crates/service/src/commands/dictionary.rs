//! The dictionary of one command call (plan §4.1): the name classes of the target issues'
//! projects, assembled in one batch of reads — the same catalog `/queries/check` loads through,
//! extended with the command-only entries (boards, sprints, groups, the empty-value names).
//! One assembly per call, nothing cached; every name is folded ([`taskmaster_query::words::fold`]).

use std::collections::{BTreeMap, HashMap};

use sqlx::PgConnection;
use taskmaster_domain::access::Access;
use taskmaster_domain::access::Projects;
use taskmaster_domain::access::permission::Permission;
use taskmaster_domain::fields::FieldKind;
use taskmaster_domain::links::Side;
use taskmaster_domain::search::catalog::{Attached, FieldEntry};
use taskmaster_domain::{
    BoardId, FieldId, GroupId, IssueId, LinkTypeId, ProjectId, SetId, SprintId, TagId, UserId,
    ValueId,
};
use taskmaster_query::command::Dictionary;
use taskmaster_query::words::{self, Kind, fold};
use taskmaster_store::boards::Viewer as BoardViewer;
use taskmaster_store::groups::Kinds;
use taskmaster_store::search::catalog;
use taskmaster_store::tags::Listing;
use taskmaster_store::users::{self as store_users, DirectoryFilter};
use taskmaster_store::{groups as store_groups, issues as store_issues, tags as store_tags};

use crate::Result;

/// A page of directory-style reads; the dictionary walks them all.
const PAGE: i64 = 200;

/// The name classes of a command call, resolved to IDs; [`Names::parser`] holds the same names
/// in the shape the parser matches against.
#[derive(Debug, Clone, Default)]
pub struct Names {
    /// The projects the target issues live in: the scope of fields and checks.
    pub target_projects: Vec<ProjectId>,
    /// Projects the actor can read; moves resolve against them (§4.5 applies).
    pub projects: Vec<NamedProject>,
    /// Fields attached to the target projects, with what resolution needs per attachment.
    pub fields: Vec<NamedField>,
    /// Values of the fields' sets, by set: archived ones included (§4.1).
    pub values: BTreeMap<SetId, Vec<NamedValue>>,
    /// Accounts the actor may name: the directory, or only themselves without
    /// `user.directory` (the assist of §5 answers the same set).
    pub users: Vec<NamedUser>,
    /// Groups the actor may know (teams of projects they may see included).
    pub groups: Vec<NamedGroup>,
    /// Tags the actor may use (§4.4: the tag-use right, not the view right).
    pub tags: Vec<NamedTag>,
    pub boards: Vec<NamedBoard>,
    pub sprints: Vec<NamedSprint>,
    /// Link types with both direction names.
    pub links: Vec<NamedLink>,
    /// The folded classes the parser matches against; aliases repeat their lists.
    pub parser: Dictionary,
}

#[derive(Debug, Clone)]
pub struct NamedProject {
    pub id: ProjectId,
    /// The project's own name, for problems and the preview.
    pub name: String,
    /// The current key, for the assist's descriptions (plan §5).
    pub key: String,
    /// Folded forms a command may write: the name and the current and former keys.
    pub names: Vec<String>,
}

/// One attachment of a field: what a project says about it.
#[derive(Debug, Clone)]
pub struct FieldAttachment {
    pub project: ProjectId,
    /// The set this attachment uses — a copy of the field's own set is its own.
    pub set: Option<SetId>,
    pub can_be_empty: bool,
    /// Folded; empty where the attachment sets none.
    pub empty_label: String,
    /// The label as it reads, for suggestions.
    pub empty_display: String,
    pub private: bool,
}

#[derive(Debug, Clone)]
pub struct NamedField {
    pub id: FieldId,
    pub kind: FieldKind,
    pub multi: bool,
    /// The built-in key (`spent_time` is read-only); `None` for a custom field.
    pub builtin: Option<String>,
    /// The field's own name, for problems and the preview.
    pub name: String,
    /// Folded forms: the name and every alias.
    pub names: Vec<String>,
    pub attachments: Vec<FieldAttachment>,
}

impl NamedField {
    /// The sets of the attachments in scope, without duplicates, in attachment order.
    pub fn sets(&self) -> Vec<SetId> {
        let mut out: Vec<SetId> = Vec::new();
        for attachment in &self.attachments {
            if let Some(set) = attachment.set
                && !out.contains(&set)
            {
                out.push(set);
            }
        }
        out
    }
}

#[derive(Debug, Clone)]
pub struct NamedValue {
    pub id: ValueId,
    /// Folded name.
    pub name: String,
    /// The name as it reads, for suggestions and the preview.
    pub display: String,
    pub resolved: bool,
}

#[derive(Debug, Clone)]
pub struct NamedUser {
    pub id: UserId,
    /// Folded login and full name, in this order.
    pub names: Vec<String>,
    /// As they read: the login inserts, the full name describes (plan §5).
    pub login: String,
    pub full_name: String,
}

#[derive(Debug, Clone)]
pub struct NamedGroup {
    pub id: GroupId,
    /// Folded name.
    pub name: String,
    /// The name as it reads.
    pub display: String,
}

#[derive(Debug, Clone)]
pub struct NamedTag {
    pub id: TagId,
    /// Folded name.
    pub name: String,
    /// The name as it reads.
    pub display: String,
}

#[derive(Debug, Clone)]
pub struct NamedBoard {
    pub id: BoardId,
    /// Folded name.
    pub name: String,
    /// The name as it reads.
    pub display: String,
}

#[derive(Debug, Clone)]
pub struct NamedSprint {
    pub id: SprintId,
    pub board: BoardId,
    /// Folded name.
    pub name: String,
    /// The name as it reads.
    pub display: String,
}

#[derive(Debug, Clone)]
pub struct NamedLink {
    pub link: LinkTypeId,
    pub side: Side,
    /// Folded name of this direction.
    pub name: String,
    /// The direction's name as it reads.
    pub display: String,
}

/// Assembles the dictionary of the target issues' projects (plan §4.1). Issues gone or
/// invisible stay out of scope: their apply answers `cmd_target_not_found` per issue.
pub(crate) async fn load(
    conn: &mut PgConnection,
    actor: &Access,
    issues: &[IssueId],
) -> Result<Names> {
    let found = store_issues::get_many(&mut *conn, issues).await?;
    let mut target_projects: Vec<ProjectId> = found.iter().map(|issue| issue.project_id).collect();
    target_projects.sort_unstable();
    target_projects.dedup();
    load_projects(conn, actor, target_projects).await
}

/// The same dictionary over the projects named (plan §4.1): the bulk job's scope, the query's
/// projects.
pub(crate) async fn load_projects(
    conn: &mut PgConnection,
    actor: &Access,
    target_projects: Vec<ProjectId>,
) -> Result<Names> {
    // Readable projects, as a query's resolution sees them: moves resolve against them.
    let rows = catalog::projects(conn).await?;
    let vis = crate::search::visibility(actor, &rows);
    let projects: Vec<NamedProject> = rows
        .iter()
        .filter(|row| !row.deleting && crate::search::sees(&vis, row.entry.id))
        .map(|row| NamedProject {
            id: row.entry.id,
            name: row.entry.name.clone(),
            key: row.entry.key.clone(),
            names: project_names(&row.entry.name, &row.entry.key, &row.entry.former_keys),
        })
        .collect();

    let (fields, attached) = catalog::fields(conn, &target_projects).await?;
    let fields = named_fields(&fields, &attached);

    // Every value of every set in scope (archived ones included, §4.1).
    let sets: Vec<SetId> = attached.iter().filter_map(|a| a.set).collect();
    let values = catalog::values(conn, &sets, &[], &sets)
        .await?
        .into_iter()
        .fold(
            BTreeMap::<SetId, Vec<NamedValue>>::new(),
            |mut map, value| {
                map.entry(value.set).or_default().push(NamedValue {
                    id: value.id,
                    name: fold(&value.name),
                    display: value.name,
                    resolved: value.resolved,
                });
                map
            },
        );

    let users = directory(conn, actor).await?;
    let groups = known_groups(conn, actor).await?;
    let tags = usable_tags(conn, actor).await?;
    let read = actor.projects_with(Permission::IssueRead);
    let boards = catalog::boards(
        conn,
        &BoardViewer {
            me: actor.user(),
            groups: actor.groups(),
            read: &read,
        },
        &[],
        true,
    )
    .await?
    .into_iter()
    .map(|b| NamedBoard {
        id: b.id,
        name: fold(&b.name),
        display: b.name,
    })
    .collect::<Vec<_>>();
    let board_ids: Vec<BoardId> = boards.iter().map(|b| b.id).collect();
    let sprints = catalog::sprints(conn, &board_ids)
        .await?
        .into_iter()
        .map(|s| NamedSprint {
            id: s.id(),
            board: s.board,
            name: fold(&s.name),
            display: s.name,
        })
        .collect::<Vec<_>>();
    let links = catalog::link_sides(conn)
        .await?
        .into_iter()
        .map(|side| NamedLink {
            link: side.link_type,
            side: side.side,
            name: fold(&side.name),
            display: side.name,
        })
        .collect::<Vec<_>>();

    let parser = parser_dictionary(
        &attributes_of(&fields),
        &projects,
        &users,
        &groups,
        &tags,
        &boards,
        &sprints,
        &links,
        &values,
        &fields,
    );
    Ok(Names {
        target_projects,
        projects,
        fields,
        values,
        users,
        groups,
        tags,
        boards,
        sprints,
        links,
        parser,
    })
}

fn project_names(name: &str, key: &str, former: &[String]) -> Vec<String> {
    let mut out = vec![fold(name)];
    for form in std::iter::once(key).chain(former.iter().map(String::as_str)) {
        let folded = fold(form);
        if !folded.is_empty() && !out.contains(&folded) {
            out.push(folded);
        }
    }
    out
}

fn named_fields(fields: &[FieldEntry], attached: &[Attached]) -> Vec<NamedField> {
    let mut out: Vec<NamedField> = Vec::new();
    for field in fields {
        let attachments: Vec<FieldAttachment> = attached
            .iter()
            .filter(|a| a.field == field.id)
            .map(|a| FieldAttachment {
                project: a.project,
                set: a.set,
                can_be_empty: a.can_be_empty,
                empty_label: fold(&a.empty_label),
                empty_display: a.empty_label.clone(),
                private: a.private,
            })
            .collect();
        if attachments.is_empty() {
            continue;
        }
        let mut names = vec![fold(&field.name)];
        for alias in &field.aliases {
            let folded = fold(alias);
            if !names.contains(&folded) {
                names.push(folded);
            }
        }
        out.push(NamedField {
            id: field.id,
            kind: field.kind,
            multi: field.multi,
            builtin: field.builtin.clone(),
            name: field.name.clone(),
            names,
            attachments,
        });
    }
    out
}

fn attributes_of(fields: &[NamedField]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for field in fields {
        for name in &field.names {
            if !out.contains(name) {
                out.push(name.clone());
            }
        }
    }
    out
}

/// The accounts the actor may name: the directory (enabled accounts, guests out), or only
/// themselves without `user.directory`.
async fn directory(conn: &mut PgConnection, actor: &Access) -> Result<Vec<NamedUser>> {
    if !actor.has(Permission::UserDirectory) {
        let me = store_users::get(conn, actor.user())
            .await?
            .ok_or(crate::Error::NotFound("no such account"))?;
        return Ok(vec![NamedUser {
            id: me.id,
            names: vec![fold(&me.username), fold(&me.full_name)],
            login: me.username,
            full_name: me.full_name,
        }]);
    }
    let filter = DirectoryFilter {
        text: "",
        enabled: true,
        disabled: false,
    };
    let mut out = Vec::new();
    let mut after = (String::new(), 0);
    loop {
        let page = store_users::directory(conn, &filter, (after.0.as_str(), after.1), PAGE).await?;
        let done = page.len() < usize::try_from(PAGE).unwrap_or(usize::MAX);
        for user in &page {
            after = (user.full_name.clone(), user.id);
            if user.is_guest() {
                continue;
            }
            out.push(NamedUser {
                id: user.id,
                names: vec![fold(&user.username), fold(&user.full_name)],
                login: user.username.clone(),
                full_name: user.full_name.clone(),
            });
        }
        if done {
            return Ok(out);
        }
    }
}

/// The groups the actor may know: what a group list shows, teams of projects they may see
/// included; without the right to see groups at all, none.
async fn known_groups(conn: &mut PgConnection, actor: &Access) -> Result<Vec<NamedGroup>> {
    if actor.may_see_groups().is_err() {
        return Ok(Vec::new());
    }
    let teams_of: Option<Vec<ProjectId>> = match actor.projects_with(Permission::ProjectBasic) {
        Projects::All => None,
        Projects::Only(ids) => Some(ids),
    };
    let mut out = Vec::new();
    let mut after: (i32, String, GroupId) = (-1, String::new(), 0);
    loop {
        let page = store_groups::list(
            conn,
            "",
            &[],
            Kinds {
                groups: true,
                teams: true,
            },
            teams_of.as_deref(),
            (after.0, &after.1, after.2),
            PAGE,
        )
        .await?;
        let done = page.len() < usize::try_from(PAGE).unwrap_or(usize::MAX);
        for listed in &page {
            // The cursor's rank: built-in groups first (ACC-022).
            after = (
                i32::from(listed.group.builtin.is_none()),
                listed.group.name.clone(),
                listed.group.id,
            );
            out.push(NamedGroup {
                id: listed.group.id,
                name: fold(&listed.group.name),
                display: listed.group.name.clone(),
            });
        }
        if done {
            return Ok(out);
        }
    }
}

/// The tags the actor may use (§4.4): their own and others' shared for use — what
/// `set_on_issue` would accept.
async fn usable_tags(conn: &mut PgConnection, actor: &Access) -> Result<Vec<NamedTag>> {
    let mut out = Vec::new();
    let mut after: Option<(String, i64)> = None;
    loop {
        let page = store_tags::listed(
            &mut *conn,
            actor.user(),
            actor.groups(),
            Listing::All,
            "",
            after.clone(),
            PAGE,
        )
        .await?;
        let done = page.len() < usize::try_from(PAGE).unwrap_or(usize::MAX);
        for tag in &page {
            after = Some((tag.name.clone(), tag.id));
            if !tag.sharing().can_use(actor) {
                continue;
            }
            out.push(NamedTag {
                id: tag.id,
                name: fold(&tag.name),
                display: tag.name.clone(),
            });
        }
        if done {
            return Ok(out);
        }
    }
}

/// The parser's dictionary from the resolved names (plan §3): folded, one flat class each;
/// values are keyed by every folded field name — aliases repeat their field's list — and the
/// empty-value names join where the field may be empty.
#[allow(clippy::too_many_arguments)]
fn parser_dictionary(
    attributes: &[String],
    projects: &[NamedProject],
    users: &[NamedUser],
    groups: &[NamedGroup],
    tags: &[NamedTag],
    boards: &[NamedBoard],
    sprints: &[NamedSprint],
    links: &[NamedLink],
    values: &BTreeMap<SetId, Vec<NamedValue>>,
    fields: &[NamedField],
) -> Dictionary {
    let mut value_map: HashMap<String, Vec<String>> = HashMap::new();
    for field in fields {
        let mut list: Vec<String> = Vec::new();
        for attachment in &field.attachments {
            if let Some(set) = attachment.set
                && let Some(set_values) = values.get(&set)
            {
                for value in set_values {
                    if !list.contains(&value.name) {
                        list.push(value.name.clone());
                    }
                }
            }
            if attachment.can_be_empty && !attachment.empty_label.is_empty() {
                let label = attachment.empty_label.clone();
                if !list.contains(&label) {
                    list.push(label);
                }
            }
        }
        // A user field's values are the accounts the actor may name, plus «me» (§4.2); a group
        // field's, the groups they may know.
        match field.kind {
            FieldKind::User => {
                for user in users {
                    for name in &user.names {
                        if !list.contains(name) {
                            list.push(name.clone());
                        }
                    }
                }
                if !list.contains(&"me".to_owned()) {
                    list.push("me".into());
                }
            }
            FieldKind::Group => {
                for group in groups {
                    if !list.contains(&group.name) {
                        list.push(group.name.clone());
                    }
                }
            }
            _ => {}
        }
        for name in &field.names {
            value_map.insert(name.clone(), list.clone());
        }
    }
    fn flat(names: impl Iterator<Item = String>) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for name in names {
            if !out.contains(&name) {
                out.push(name);
            }
        }
        out
    }
    Dictionary {
        attributes: attributes.to_vec(),
        values: value_map,
        links: flat(links.iter().map(|l| l.name.clone())),
        projects: flat(projects.iter().flat_map(|p| p.names.clone())),
        tags: flat(tags.iter().map(|t| t.name.clone())),
        users: flat(users.iter().flat_map(|u| u.names.clone())),
        groups: flat(groups.iter().map(|g| g.name.clone())),
        boards: flat(boards.iter().map(|b| b.name.clone())),
        sprints: flat(sprints.iter().map(|s| s.name.clone())),
        special: special_names(),
    }
}

/// Names every field may take beyond its own values (§4.2): «me» for user fields and the date
/// words. One list: the parser only decides a token *is* a value.
fn special_names() -> Vec<String> {
    let mut out: Vec<String> = vec!["me".into()];
    for word in words::VOCABULARY
        .iter()
        .filter(|word| word.kind == Kind::Date)
    {
        for form in word.english.iter().chain(word.russian) {
            let folded = fold(form);
            if !out.contains(&folded) {
                out.push(folded);
            }
        }
    }
    out
}

impl Names {
    /// The fields whose folded names include `folded`, attached in scope.
    pub fn fields_named(&self, folded: &str) -> Vec<&NamedField> {
        self.fields
            .iter()
            .filter(|f| f.names.iter().any(|n| n == folded))
            .collect()
    }

    /// The values of `set` matching `folded`.
    pub fn values_named(&self, set: SetId, folded: &str) -> Vec<&NamedValue> {
        self.values
            .get(&set)
            .map(|values| values.iter().filter(|v| v.name == folded).collect())
            .unwrap_or_default()
    }

    /// Accounts matching a name: login first, then the full name; several full names the same
    /// answer every match, and the caller says `cmd_ambiguous` (plan §4.2).
    pub fn users_named(&self, text: &str) -> Vec<&NamedUser> {
        let folded = fold(text);
        let by_login: Vec<&NamedUser> = self
            .users
            .iter()
            .filter(|u| u.names.first().is_some_and(|login| login == &folded))
            .collect();
        if !by_login.is_empty() {
            return by_login;
        }
        self.users
            .iter()
            .filter(|u| u.names.get(1).is_some_and(|name| name == &folded))
            .collect()
    }

    pub fn groups_named(&self, folded: &str) -> Vec<&NamedGroup> {
        self.groups.iter().filter(|g| g.name == folded).collect()
    }

    pub fn tags_named(&self, folded: &str) -> Vec<&NamedTag> {
        self.tags.iter().filter(|t| t.name == folded).collect()
    }

    pub fn boards_named(&self, folded: &str) -> Vec<&NamedBoard> {
        self.boards.iter().filter(|b| b.name == folded).collect()
    }

    pub fn sprints_named(&self, folded: &str) -> Vec<&NamedSprint> {
        self.sprints.iter().filter(|s| s.name == folded).collect()
    }

    pub fn links_named(&self, folded: &str) -> Vec<&NamedLink> {
        self.links.iter().filter(|l| l.name == folded).collect()
    }

    /// Projects matching a folded name or key.
    pub fn projects_named(&self, folded: &str) -> Vec<&NamedProject> {
        self.projects
            .iter()
            .filter(|p| p.names.iter().any(|n| n == folded))
            .collect()
    }
}
