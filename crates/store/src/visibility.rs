//! The visibility predicate of every cross-issue list (ACC-049, architecture.md §4): issues of
//! projects the viewer reads, or that they reported where they may only create; then only issues
//! without an audience or whose audience includes them, unless they see every audience in the
//! project. The same rule as `domain::access::visibility::can_see`; a service test runs both.
//!
//! A list within one project gets the predicate for that project: its part is known in advance,
//! so it is left out or becomes `reporter = me`. Repeating `project = any(…)` next to
//! `project = …` made the planner multiply the two selectivities (D-035).

use prostor_domain::access::Projects;
use prostor_domain::{GroupId, ProjectId, UserId};
use sqlx::{Postgres, QueryBuilder};

/// What one viewer may see of issues and of what is inside them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Visibility {
    pub me: UserId,
    /// `issue.read`.
    pub read: Projects,
    /// `issue.create` without `issue.read`: only their own issues (ACC-047).
    pub create_only: Vec<ProjectId>,
    /// `visibility.bypass`: every audience.
    pub bypass: Projects,
    /// `comment.read`.
    pub comment_read: Projects,
    /// The viewer's groups, teams included.
    pub groups: Vec<GroupId>,
    /// The one project the list is limited to, if any.
    pub within: Option<ProjectId>,
}

/// `{column} = any(…)` for a set of projects; `true` for all, `false` for none.
pub(crate) fn push_projects(qb: &mut QueryBuilder<Postgres>, column: &str, projects: &Projects) {
    match projects {
        Projects::All => {
            qb.push("true");
        }
        Projects::Only(ids) if ids.is_empty() => {
            qb.push("false");
        }
        Projects::Only(ids) => {
            qb.push(format!("{column} = any("));
            qb.push_bind(ids.clone());
            qb.push(")");
        }
    }
}

/// Whether `projects` include `id`.
pub fn holds(projects: &Projects, id: ProjectId) -> bool {
    match projects {
        Projects::All => true,
        Projects::Only(ids) => ids.contains(&id),
    }
}

impl Visibility {
    /// Pushes the conditions on issue row `i`, each starting with ` and`. An empty audience is
    /// compared with `'{}'`: the planner estimates an equality from the column's statistics, but
    /// gives `cardinality(…) = 0` a default selectivity (D-024).
    pub fn push_sql(&self, qb: &mut QueryBuilder<Postgres>, i: &str) {
        let (read, create_only, every_audience) = match self.within {
            None => (Some(self.read.clone()), self.create_only.clone(), false),
            Some(p) if holds(&self.read, p) => (None, Vec::new(), holds(&self.bypass, p)),
            Some(p) if self.create_only.contains(&p) => (
                Some(Projects::Only(Vec::new())),
                vec![p],
                holds(&self.bypass, p),
            ),
            Some(_) => {
                qb.push(" and false");
                return;
            }
        };
        match (&read, create_only.is_empty()) {
            (None, _) => {}
            (Some(Projects::All), _) => {}
            (Some(read), true) => {
                qb.push(" and ");
                push_projects(qb, &format!("{i}.project_id"), read);
            }
            (Some(read), false) => {
                qb.push(" and (");
                push_projects(qb, &format!("{i}.project_id"), read);
                qb.push(format!(" or ({i}.reporter_id = "));
                qb.push_bind(self.me);
                qb.push(format!(" and {i}.project_id = any("));
                qb.push_bind(create_only);
                qb.push(")))");
            }
        }
        if every_audience || (self.within.is_none() && self.bypass == Projects::All) {
            return;
        }
        qb.push(format!(
            " and (({i}.audience_users = '{{}}' and {i}.audience_groups = '{{}}') or {i}.reporter_id = "
        ));
        qb.push_bind(self.me);
        qb.push(" or ");
        qb.push_bind(self.me);
        qb.push(format!(
            " = any({i}.audience_users) or {i}.audience_groups && "
        ));
        qb.push_bind(self.groups.clone());
        if self.within.is_none() && self.bypass != Projects::Only(Vec::new()) {
            qb.push(" or ");
            push_projects(qb, &format!("{i}.project_id"), &self.bypass);
        }
        qb.push(")");
    }

    /// Whether the viewer reads every project (no project condition across projects).
    pub fn reads_all(&self) -> bool {
        self.read == Projects::All
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vis(within: Option<ProjectId>) -> Visibility {
        Visibility {
            me: 5,
            read: Projects::Only(vec![1, 2]),
            create_only: vec![3],
            bypass: Projects::Only(vec![2]),
            comment_read: Projects::Only(vec![1, 2]),
            groups: vec![7],
            within,
        }
    }

    fn sql(vis: &Visibility) -> String {
        let mut qb = QueryBuilder::<Postgres>::new("");
        vis.push_sql(&mut qb, "i");
        qb.sql().as_str().to_owned()
    }

    #[test]
    fn across_projects() {
        let text = sql(&vis(None));
        assert!(
            text.starts_with(
                " and (i.project_id = any($1) or (i.reporter_id = $2 and i.project_id = any($3)))"
            ),
            "{text}"
        );
        assert!(
            text.ends_with("i.audience_groups && $6 or i.project_id = any($7))"),
            "{text}"
        );
    }

    #[test]
    fn within_one_project() {
        // Reads it: only audiences; sees every audience: nothing.
        assert!(!sql(&vis(Some(1))).contains("project_id"));
        assert_eq!(sql(&vis(Some(2))), "");
        let own = sql(&vis(Some(3)));
        assert!(
            own.starts_with(" and (false or (i.reporter_id = $1"),
            "{own}"
        );
        assert_eq!(sql(&vis(Some(4))), " and false");
    }
}
