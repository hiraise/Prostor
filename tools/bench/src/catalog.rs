//! What the scenarios need to build their queries, read from the database once: field and value
//! IDs by name (as the query compiler will resolve them), busy projects, assignees, sprints and
//! the access of the people who look. The real server keeps this metadata in memory too.

use std::collections::{BTreeSet, HashMap};

use anyhow::Context;
use sqlx::PgConnection;

use crate::dataset::model::names;
use crate::rng::{Rng, Weighted};
use crate::text::{VOCABULARY, Vocabulary};

/// Cards on the board of the board scenario.
pub const BOARD_CARDS: i64 = 500;

#[derive(Debug, Clone)]
pub struct Catalog {
    pub state: i32,
    pub priority: i32,
    pub kind: i32,
    pub assignee: i32,
    pub fix_versions: i32,
    pub sprint: i32,
    /// Values named "Bug" in every type set.
    pub bugs: Vec<i64>,
    /// Values named "Fixed".
    pub fixed: Vec<i64>,
    /// The two highest priorities.
    pub urgent: Vec<i64>,
    /// Priority values.
    pub priorities: Vec<i64>,
    /// Priority values grouped by position in their sets, in sort order.
    pub priority_order: Vec<Vec<i64>>,
    /// States named "Open" and "In progress".
    pub open: Vec<i64>,
    /// States that count as resolved.
    pub resolved_states: Vec<i64>,
    /// Projects by number of issues: busy projects are looked at more often.
    pub projects: Vec<i32>,
    pub project_weights: Weighted,
    /// Issues per project.
    pub project_sizes: HashMap<i32, i64>,
    /// Fix versions of each project.
    pub versions: HashMap<i32, Vec<i64>>,
    /// Names of the fix versions, by value.
    pub version_names: HashMap<i64, String>,
    /// Each project's key, for queries.
    pub keys: HashMap<i32, String>,
    /// Projects whose types include bugs.
    pub bug_projects: Vec<i32>,
    /// Users with assigned issues.
    pub assignees: Vec<i64>,
    pub users: Vec<i64>,
    /// Fields whose values are user IDs.
    pub user_fields: Vec<i32>,
    /// Sprints of about [`BOARD_CARDS`] cards: (board, its project, sprint, cards).
    pub sprints: Vec<(i64, i32, i64, i64)>,
    /// The sprints of projects with the scrum rule on (§3.14): their card moves answer
    /// assign-on-move (AUT-085). Empty when no rule project made the busiest boards.
    pub scrum_sprints: Vec<(i64, i32, i64, i64)>,
    /// The columns of each board, in order.
    pub board_columns: HashMap<i64, Vec<i64>>,
    /// Cards of each sprint in [`Catalog::sprints`] whose issues have no audience.
    pub open_cards: HashMap<i64, Vec<i64>>,
    /// Published issues without an audience, with their projects: every member of the project
    /// sees them.
    pub open_issues: Vec<(i64, i32)>,
    /// Priority values of each project.
    pub project_priorities: HashMap<i32, Vec<i64>>,
    /// Seed of the dataset, from which API tokens are derived.
    pub seed: u64,
    pub vocabulary: Vocabulary,
    /// Members of each project team.
    pub teams: HashMap<i32, Vec<i64>>,
    /// Projects with rule rows on (step 1.9): their write paths run the behaviors.
    pub rule_projects: Vec<i32>,
    /// Living closed chains of subtask links, three parents deep (§3.14): the bottom issue and
    /// its project. Resolving the bottom walks the whole chain; about forty, so the operations
    /// alternate between resolving and reopening.
    pub chains: Vec<(i64, i32)>,
    /// Issues whose audience lists people, one of them a team member (AUT-063): the issue, its
    /// project and a team member who may edit it.
    pub audience_issues: Vec<(i64, i32, i64)>,
    /// Open and resolved state values of each project's state field.
    pub project_states: HashMap<i32, (Vec<i64>, Vec<i64>)>,
    /// People in the most project teams: who looks at lists across projects.
    pub busy_members: Vec<i64>,
    /// Groups of each person, project teams included (built-in groups are implicit).
    groups_of: HashMap<i64, Vec<i64>>,
    /// Projects whose team each person is in: there they read and create issues.
    teams_of: HashMap<i64, Vec<i64>>,
    /// Projects each person reported issues in: there they may create issues.
    reported_in: HashMap<i64, Vec<i64>>,
    /// Projects each person owns: they administer them, so they read their issues.
    owned: HashMap<i64, Vec<i64>>,
    /// Everyone and All members.
    builtin_groups: Vec<i64>,
}

/// What a person may see, compiled as the server compiles access from the dataset's role
/// assignments (architecture.md §4, D-025): none of the people in the dataset is a system
/// administrator, and none holds a role that sees every audience.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Viewer {
    pub user: i64,
    /// Projects where they read issues.
    pub read: Vec<i64>,
    /// Projects where they only create issues, so they see the ones they reported.
    pub create_only: Vec<i64>,
    /// Their groups, the built-in ones and project teams included.
    pub groups: Vec<i64>,
    /// Projects where they see issues of any audience.
    pub bypass: Vec<i64>,
}

impl Catalog {
    pub async fn load(conn: &mut PgConnection) -> anyhow::Result<Self> {
        let seed: i64 = sqlx::query_scalar("select seed from bench.dataset")
            .fetch_one(&mut *conn)
            .await
            .context("no dataset in this database")?;
        let fields: HashMap<String, i32> = sqlx::query_as("select name, id::integer from fields")
            .fetch_all(&mut *conn)
            .await?
            .into_iter()
            .collect();
        let field = |name: &str| {
            fields
                .get(name)
                .copied()
                .with_context(|| format!("no field {name}"))
        };
        let (state, priority, kind) = (
            field(names::STATE)?,
            field(names::PRIORITY)?,
            field(names::TYPE)?,
        );
        let (assignee, fix_versions, sprint) = (
            field(names::ASSIGNEE)?,
            field(names::FIX_VERSIONS)?,
            field(names::SPRINT)?,
        );

        // Values in the sets that projects use for these fields: what names resolve to.
        let values: Vec<(i64, i32, String, bool)> = sqlx::query_as(
            "select v.id, s.field_id::integer, v.name, v.resolved
             from (select distinct field_id, set_id from project_fields
                   where field_id = any($1)) s
             join field_values v on v.set_id = s.set_id
             order by v.set_id, v.rank",
        )
        .bind(vec![i64::from(state), i64::from(priority), i64::from(kind)])
        .fetch_all(&mut *conn)
        .await?;
        let named = |field: i32, wanted: &[&str]| -> Vec<i64> {
            values
                .iter()
                .filter(|v| v.1 == field && wanted.contains(&v.2.as_str()))
                .map(|v| v.0)
                .collect()
        };
        let bugs = named(kind, &[names::BUG]);
        let fixed = named(state, &[names::FIXED]);
        let urgent = named(priority, &names::URGENT);
        let priorities = values
            .iter()
            .filter(|v| v.1 == priority)
            .map(|v| v.0)
            .collect();
        let open = named(state, &names::OPEN);
        let resolved_states = values
            .iter()
            .filter(|v| v.1 == state && v.3)
            .map(|v| v.0)
            .collect();
        // Positions in the sets, from the order of their ranks (D-024).
        let priority_order = sqlx::query_scalar(
            "select array_agg(id order by id)
             from (select v.id, row_number() over (partition by v.set_id order by v.rank) as position
                   from field_values v
                   where v.set_id in (select set_id from project_fields where field_id = $1)) v
             group by position order by position",
        )
        .bind(i64::from(priority))
        .fetch_all(&mut *conn)
        .await?;

        let sizes: Vec<(i32, i64)> = sqlx::query_as(
            "select project_id::integer, count(*) from issues group by project_id order by 1",
        )
        .fetch_all(&mut *conn)
        .await?;
        let projects = sizes.iter().map(|s| s.0).collect();
        let project_sizes = sizes.iter().copied().collect();
        let project_weights = Weighted::new(sizes.iter().map(|s| s.1 as f64));

        let mut versions: HashMap<i32, Vec<i64>> = HashMap::new();
        let mut version_names = HashMap::new();
        let rows: Vec<(i32, i64, String)> = sqlx::query_as(
            "select pf.project_id::integer, v.id, v.name from project_fields pf
             join field_values v on v.set_id = pf.set_id
             where pf.field_id = $1 order by pf.project_id, v.rank",
        )
        .bind(i64::from(fix_versions))
        .fetch_all(&mut *conn)
        .await?;
        for (project, value, name) in rows {
            versions.entry(project).or_default().push(value);
            version_names.insert(value, name);
        }
        let bug_projects = sqlx::query_scalar(
            "select distinct pf.project_id::integer from project_fields pf
             join field_values v on v.set_id = pf.set_id
             where pf.field_id = $1 and v.name = $2 order by 1",
        )
        .bind(i64::from(kind))
        .bind(names::BUG)
        .fetch_all(&mut *conn)
        .await?;
        let keys = sqlx::query_as("select id::integer, key from projects")
            .fetch_all(&mut *conn)
            .await?
            .into_iter()
            .collect();

        let assignees = sqlx::query_scalar(
            "select distinct value from issue_ref_values where field_id = $1 order by 1",
        )
        .bind(i64::from(assignee))
        .fetch_all(&mut *conn)
        .await?;
        let users = sqlx::query_scalar("select id from users order by id")
            .fetch_all(&mut *conn)
            .await?;
        let user_fields = sqlx::query_scalar("select id::integer from fields where kind = 'user'")
            .fetch_all(&mut *conn)
            .await?;
        let sprints = sqlx::query_as(
            "select s.board_id, bp.project_id::integer, s.id, count(*) as cards
             from sprints s
             join board_projects bp on bp.board_id = s.board_id
             join sprint_issues si on si.sprint_id = s.id
             group by s.board_id, bp.project_id, s.id
             order by abs(count(*) - $1), s.id
             limit 50",
        )
        .bind(BOARD_CARDS)
        .fetch_all(&mut *conn)
        .await?;
        let picked: Vec<i64> = sprints.iter().map(|s: &(i64, i32, i64, i64)| s.2).collect();
        // Only cards that sit in a column of their board: the board shows the preset's columns
        // (unresolved values plus the first resolved one), so a card holding another resolved
        // value — «Verified», «Дубликат» — has no column to move from.
        let open_cards: Vec<(i64, Vec<i64>)> = sqlx::query_as(
            "select si.sprint_id, array_agg(si.issue_id order by si.issue_id)
             from sprint_issues si
             join issues i on i.id = si.issue_id
             join sprints s on s.id = si.sprint_id
             join issue_ref_values r on r.issue_id = si.issue_id and r.field_id = $2
             join board_columns c on c.board_id = s.board_id and c.value_id = r.value
             where si.sprint_id = any($1) and i.audience_users = '{}' and i.audience_groups = '{}'
             group by si.sprint_id",
        )
        .bind(&picked)
        .bind(i64::from(state))
        .fetch_all(&mut *conn)
        .await?;
        let board_columns: Vec<(i64, Vec<i64>)> = sqlx::query_as(
            "select board_id, array_agg(id order by rank) from board_columns group by board_id",
        )
        .fetch_all(&mut *conn)
        .await?;
        let open_issues = sqlx::query_as(
            "select id, project_id::integer from issues
             where number is not null and deleted_at is null
               and audience_users = '{}' and audience_groups = '{}'
             order by id",
        )
        .fetch_all(&mut *conn)
        .await?;
        let mut project_priorities: HashMap<i32, Vec<i64>> = HashMap::new();
        let rows: Vec<(i32, i64)> = sqlx::query_as(
            "select pf.project_id::integer, v.id from project_fields pf
             join field_values v on v.set_id = pf.set_id
             where pf.field_id = $1 order by pf.project_id, v.rank",
        )
        .bind(i64::from(priority))
        .fetch_all(&mut *conn)
        .await?;
        for (project, value) in rows {
            project_priorities.entry(project).or_default().push(value);
        }

        // Access: project teams, other groups, reported issues and owned projects.
        let members: Vec<(i64, i64, Option<i64>)> = sqlx::query_as(
            "select m.user_id, m.group_id, g.team_of
             from group_members m join groups g on g.id = m.group_id
             order by m.user_id, m.group_id",
        )
        .fetch_all(&mut *conn)
        .await?;
        let (mut groups_of, mut teams_of, mut teams) = (
            HashMap::new(),
            HashMap::<i64, Vec<i64>>::new(),
            HashMap::new(),
        );
        for (user, group, team_of) in members {
            groups_of.entry(user).or_insert_with(Vec::new).push(group);
            if let Some(project) = team_of {
                teams_of.entry(user).or_default().push(project);
                teams
                    .entry(i32::try_from(project)?)
                    .or_insert_with(Vec::new)
                    .push(user);
            }
        }
        let mut busy: Vec<(usize, i64)> = teams_of.iter().map(|(u, p)| (p.len(), *u)).collect();
        busy.sort_unstable_by(|a, b| b.cmp(a));
        let busy_members = busy.iter().take(100).map(|b| b.1).collect();
        let reported_in = sqlx::query_as::<_, (i64, Vec<i64>)>(
            "select reporter_id, array_agg(distinct project_id) from issues group by reporter_id",
        )
        .fetch_all(&mut *conn)
        .await?
        .into_iter()
        .collect();
        let owned = sqlx::query_as::<_, (i64, Vec<i64>)>(
            "select owner_id, array_agg(id order by id) from projects group by owner_id",
        )
        .fetch_all(&mut *conn)
        .await?
        .into_iter()
        .collect();
        let builtin_groups =
            sqlx::query_scalar("select id from groups where builtin is not null order by id")
                .fetch_all(&mut *conn)
                .await?;

        // Step 1.9 (§3.14): the targets of the rule variants of the change scenario. A chain is
        // closed when no ancestor has another subtask at all — a structural fact, so the list
        // stays true however the runs before have resolved and reopened the chains: a toggle at
        // the bottom reopens every resolved ancestor above and stops at the first open one, so
        // every other operation walks the whole chain whatever state it starts from. An audience
        // issue's actor must be a team member the audience lists, or they could not even see the
        // issue to edit it.
        let rule_projects = sqlx::query_scalar(
            "select distinct project_id::integer from project_rules where enabled order by 1",
        )
        .fetch_all(&mut *conn)
        .await?;
        let scrum_projects = sqlx::query_scalar(
            "select distinct project_id::integer from project_rules
             where enabled and rule = 'scrum' order by 1",
        )
        .fetch_all(&mut *conn)
        .await?;
        // The card-move variant moves scrum cards, so its hook answers (§3.14); the busiest
        // boards decide whether any made the pick.
        let scrum_sprints = sprints
            .iter()
            .filter(|s| scrum_projects.contains(&s.1))
            .copied()
            .collect::<Vec<_>>();
        let chains: Vec<(i64, i32)> = sqlx::query_as(
            "with subtask as (
                 select l.source_id as parent, l.target_id as child
                 from issue_links l
                 join link_types t on t.id = l.type_id and t.builtin = 'subtask'
             ), chain as (
                 select c1.child as bottom
                 from subtask c1
                 join subtask c2 on c2.child = c1.parent
                 join subtask c3 on c3.child = c2.parent
                 where not exists (select 1 from subtask o
                                   where o.parent = c1.parent and o.child <> c1.child)
                   and not exists (select 1 from subtask o
                                   where o.parent = c2.parent and o.child <> c2.child)
                   and not exists (select 1 from subtask o
                                   where o.parent = c3.parent and o.child <> c3.child)
             )
             select c.bottom, i.project_id::integer
             from chain c join issues i on i.id = c.bottom
             where i.number is not null and i.deleted_at is null
               and i.audience_users = '{}' and i.audience_groups = '{}'
               and exists (select 1 from project_rules r where r.project_id = i.project_id
                           and r.enabled and r.rule = 'subtasks_follow')
             order by c.bottom % 37, c.bottom
             limit 40",
        )
        .fetch_all(&mut *conn)
        .await?;
        let audience_issues: Vec<(i64, i32, i64)> = sqlx::query_as(
            "select i.id, i.project_id::integer, min(u.user_id)
             from issues i
             cross join lateral (select unnest(i.audience_users) as user_id) u
             join group_members m on m.user_id = u.user_id
             join groups g on g.id = m.group_id and g.team_of = i.project_id
             where i.number is not null and i.deleted_at is null
               and i.audience_groups = '{}'
               and exists (select 1 from project_rules r where r.project_id = i.project_id
                           and r.enabled and r.rule = 'assignee_visibility')
               and exists (select 1 from project_fields pf
                           where pf.project_id = i.project_id and pf.field_id = $1)
             group by i.id, i.project_id
             order by i.id
             limit 400",
        )
        .bind(i64::from(assignee))
        .fetch_all(&mut *conn)
        .await?;
        let mut project_states: HashMap<i32, (Vec<i64>, Vec<i64>)> = HashMap::new();
        let rows: Vec<(i32, i64, bool)> = sqlx::query_as(
            "select pf.project_id::integer, v.id, v.resolved
             from project_fields pf join field_values v on v.set_id = pf.set_id
             where pf.field_id = $1 order by pf.project_id, v.rank",
        )
        .bind(i64::from(state))
        .fetch_all(&mut *conn)
        .await?;
        for (project, value, resolved) in rows {
            let values = project_states.entry(project).or_default();
            if resolved {
                // The first resolved value only: the one the parent-follows walk writes (§3.7)
                // and the only one the cascade may toggle to — a value named «Дубликат» would
                // be refused for its missing link (AUT-071 rule 5), and the cascade measures
                // commits that land.
                if values.1.is_empty() {
                    values.1.push(value);
                }
            } else {
                values.0.push(value);
            }
        }

        Ok(Self {
            teams,
            busy_members,
            groups_of,
            teams_of,
            reported_in,
            owned,
            builtin_groups,
            state,
            priority,
            kind,
            assignee,
            fix_versions,
            sprint,
            bugs,
            fixed,
            urgent,
            priorities,
            priority_order,
            open,
            resolved_states,
            projects,
            project_weights,
            project_sizes,
            versions,
            version_names,
            keys,
            bug_projects,
            assignees,
            users,
            user_fields,
            sprints,
            scrum_sprints,
            board_columns: board_columns.into_iter().collect(),
            open_cards: open_cards.into_iter().collect(),
            open_issues,
            project_priorities,
            rule_projects,
            chains,
            audience_issues,
            project_states,
            seed: u64::try_from(seed)?,
            vocabulary: Vocabulary::new(u64::try_from(seed)?),
        })
    }

    /// The access of `user`: they read the projects whose team they are in or that they own, and
    /// create issues in those they reported in.
    pub fn viewer(&self, user: i64) -> Viewer {
        let read: BTreeSet<i64> = self
            .teams_of
            .get(&user)
            .into_iter()
            .chain(self.owned.get(&user))
            .flatten()
            .copied()
            .collect();
        let create_only = self
            .reported_in
            .get(&user)
            .into_iter()
            .flatten()
            .filter(|p| !read.contains(p))
            .copied()
            .collect();
        let mut groups = self.builtin_groups.clone();
        groups.extend(self.groups_of.get(&user).into_iter().flatten());
        Viewer {
            user,
            read: read.into_iter().collect(),
            create_only,
            groups,
            bypass: Vec::new(),
        }
    }

    /// How many issues `viewer` may read, in `project` or in all projects (the server keeps
    /// counts per project); issues they only reported are left out.
    pub fn readable(&self, viewer: &Viewer, project: Option<i32>) -> i64 {
        let size = |p: i64| {
            i32::try_from(p)
                .ok()
                .and_then(|p| self.project_sizes.get(&p))
                .copied()
                .unwrap_or(0)
        };
        match project {
            Some(p) if viewer.read.contains(&i64::from(p)) => size(i64::from(p)),
            Some(_) => 0,
            None => viewer.read.iter().map(|p| size(*p)).sum(),
        }
    }

    /// A member of the team of `project`, busy ones more likely.
    pub fn member(&self, project: i32, rng: &mut Rng) -> i64 {
        let team = self.teams.get(&project).map_or(&[][..], Vec::as_slice);
        // Teams are listed by user ID; weigh the first ones more, as the dataset does.
        team.get((rng.unit().powi(2) * team.len() as f64) as usize)
            .copied()
            .unwrap_or(1)
    }

    /// A project, busy ones more likely.
    pub fn project(&self, rng: &mut Rng) -> i32 {
        self.projects
            .get(self.project_weights.sample(rng))
            .copied()
            .unwrap_or(1)
    }

    pub fn pick<T: Copy + Default>(items: &[T], rng: &mut Rng) -> T {
        items
            .get(rng.index(items.len()))
            .copied()
            .unwrap_or_default()
    }

    /// What a filter on these states says about resolution: the query compiler adds it as a
    /// condition on the issue row, which has an index for unresolved issues.
    pub fn implied_resolution(&self, states: &[i64]) -> Option<bool> {
        let resolved = |s: &i64| self.resolved_states.contains(s);
        if states.is_empty() {
            None
        } else if states.iter().all(resolved) {
            Some(true)
        } else if !states.iter().any(resolved) {
            Some(false)
        } else {
            None
        }
    }

    /// User IDs among reference values `(issue, field, value)`.
    pub fn people<'a>(&'a self, refs: &'a [(i64, i32, i64)]) -> impl Iterator<Item = i64> + 'a {
        refs.iter()
            .filter(|r| self.user_fields.contains(&r.1))
            .map(|r| r.2)
    }

    /// An issue without an audience and a member of its project's team who looks at it.
    pub fn issue_and_member(&self, rng: &mut Rng) -> (i64, i32, i64) {
        for _ in 0..20 {
            let (issue, project) = Self::pick(&self.open_issues, rng);
            if self
                .teams
                .get(&project)
                .is_some_and(|team| !team.is_empty())
            {
                return (issue, project, self.member(project, rng));
            }
        }
        (0, 0, 0)
    }

    /// The API token of `user` (`dataset::api_token`).
    pub fn token(&self, user: i64) -> String {
        crate::dataset::api_token(self.seed, user)
    }

    /// A word whose frequency rank is in `ranks` (0 is the most frequent).
    pub fn word(&self, rng: &mut Rng, ranks: std::ops::Range<usize>) -> &str {
        let span = ranks.end.min(VOCABULARY).saturating_sub(ranks.start);
        self.vocabulary.at(ranks.start + rng.index(span))
    }
}
