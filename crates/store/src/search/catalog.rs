//! Loaders of `domain::search::Catalog`: one query per kind of name. Names are narrowed in SQL by
//! the folded forms of `prostor_query::{fold, fold_value}` (`lower`, `ё` → `е`, and for full
//! names runs of spaces, `-` and `_` as one space); the resolver compares again in Rust.

use prostor_domain::access::Projects;
use prostor_domain::boards::sprints::SprintDates;
use prostor_domain::fields::FieldKind;
use prostor_domain::links::Side;
use prostor_domain::search::Filter;
use prostor_domain::search::catalog::{
    Attached, BoardEntry, FieldEntry, GroupEntry, IssueEntry, LinkSideEntry, ProjectEntry,
    SprintEntry, TagEntry, UserEntry, ValueEntry,
};
use prostor_domain::{BoardId, GroupId, IssueId, ProjectId, SetId, UserId};
use sqlx::{PgConnection, Postgres, QueryBuilder, Row};

use crate::boards::Viewer;

use crate::visibility::Visibility;

/// A project with what the service decides on before the catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRow {
    pub entry: ProjectEntry,
    /// Being deleted (PRJ-065): nobody sees it.
    pub deleting: bool,
}

/// Every project with its former keys.
pub async fn projects(conn: &mut PgConnection) -> Result<Vec<ProjectRow>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"select p.id, p.key, p.name, p.archived_at is not null as "archived!",
                  p.deleting_at is not null as "deleting!",
                  coalesce(array_agg(k.key order by k.key) filter (where k.key <> p.key), '{}')
                      as "former!"
           from projects p left join project_keys k on k.project_id = p.id
           group by p.id
           order by p.id"#
    )
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| ProjectRow {
            entry: ProjectEntry {
                id: r.id,
                key: r.key,
                former_keys: r.former,
                name: r.name,
                archived: r.archived,
            },
            deleting: r.deleting,
        })
        .collect())
}

/// Fields attached to the projects, with their attachments.
pub async fn fields(
    conn: &mut PgConnection,
    projects: &[ProjectId],
) -> Result<(Vec<FieldEntry>, Vec<Attached>), sqlx::Error> {
    let rows = sqlx::query!(
        r#"select pf.project_id, pf.field_id, pf.set_id, pf.can_be_empty, pf.empty_label,
                  pf.private, f.name, f.aliases, f.kind, f.multi, f.builtin
           from project_fields pf join fields f on f.id = pf.field_id
           where pf.project_id = any($1)
           order by f.rank, f.id, pf.project_id"#,
        projects
    )
    .fetch_all(conn)
    .await?;
    let mut fields: Vec<FieldEntry> = Vec::new();
    let mut attached = Vec::with_capacity(rows.len());
    for r in rows {
        if !fields.iter().any(|f| f.id == r.field_id) {
            let Some(kind) = FieldKind::parse(&r.kind) else {
                continue;
            };
            fields.push(FieldEntry {
                id: r.field_id,
                name: r.name,
                aliases: r.aliases,
                kind,
                multi: r.multi,
                builtin: r.builtin,
            });
        }
        attached.push(Attached {
            project: r.project_id,
            can_be_empty: r.can_be_empty,
            field: r.field_id,
            set: r.set_id,
            empty_label: r.empty_label,
            private: r.private,
        });
    }
    Ok((fields, attached))
}

/// Values of `sets` whose folded name is one of `words`, and every value of `whole`, with their
/// positions in the set's order.
pub async fn values(
    conn: &mut PgConnection,
    sets: &[SetId],
    words: &[String],
    whole: &[SetId],
) -> Result<Vec<ValueEntry>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"select id as "id!", set_id as "set_id!", name as "name!", resolved as "resolved!",
                  position as "position!"
           from (select v.id, v.set_id, v.name, v.resolved,
                        row_number() over (partition by v.set_id order by v.rank) - 1 as position,
                        replace(lower(btrim(v.name)), 'ё', 'е') as folded
                 from field_values v where v.set_id = any($1)) x
           where x.folded = any($2) or x.set_id = any($3)
           order by x.set_id, x.position"#,
        sets,
        words,
        whole
    )
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| ValueEntry {
            id: r.id,
            set: r.set_id,
            name: r.name,
            position: u32::try_from(r.position).unwrap_or(u32::MAX),
            resolved: r.resolved,
        })
        .collect())
}

/// Accounts (not deleted) by login among `words` or by folded full name among `people`.
pub async fn users(
    conn: &mut PgConnection,
    words: &[String],
    people: &[String],
) -> Result<Vec<UserEntry>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"select id, username, full_name from users
           where deleted_at is null
             and (lower(username) = any($1)
                  or btrim(regexp_replace(replace(lower(full_name), 'ё', 'е'), '[[:space:]_-]+', ' ', 'g'))
                     = any($2))
           order by id"#,
        words,
        people
    )
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| UserEntry {
            id: r.id,
            login: r.username,
            full_name: r.full_name,
        })
        .collect())
}

/// Groups (teams included) by folded name, with their members. A built-in group's members are
/// every account (`everyone`) or every account but the guest (`all_members`).
pub async fn groups(
    conn: &mut PgConnection,
    words: &[String],
) -> Result<Vec<GroupEntry>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"select g.id, g.name, g.builtin,
                  coalesce((select array_agg(m.user_id order by m.user_id) from group_members m
                            join users u on u.id = m.user_id and u.deleted_at is null
                            where m.group_id = g.id), '{}') as "members!"
           from groups g
           where replace(lower(btrim(g.name)), 'ё', 'е') = any($1)
           order by g.id"#,
        words
    )
    .fetch_all(&mut *conn)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let members = match r.builtin.as_deref() {
            Some(builtin) => {
                sqlx::query_scalar!(
                    "select id from users where deleted_at is null
                       and ($1 or builtin is distinct from 'guest') order by id",
                    builtin == "everyone"
                )
                .fetch_all(&mut *conn)
                .await?
            }
            None => r.members,
        };
        out.push(GroupEntry {
            id: r.id,
            name: r.name,
            members,
        });
    }
    Ok(out)
}

/// Tags the viewer sees by folded name: their own and those shared for viewing (ISS-037).
pub async fn tags(
    conn: &mut PgConnection,
    me: UserId,
    groups: &[GroupId],
    words: &[String],
) -> Result<Vec<TagEntry>, sqlx::Error> {
    let rows = sqlx::query!(
        "select id, name from tags
         where (owner_id = $1 or $1 = any(view_users) or view_groups && $2)
           and replace(lower(btrim(name)), 'ё', 'е') = any($3)
         order by id",
        me,
        groups,
        words
    )
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| TagEntry {
            id: r.id,
            name: r.name,
        })
        .collect())
}

/// Boards the viewer can view (the rule of `boards::listed`, QRY-049): every one when `all`, else
/// those whose folded name is one of `words`. A board in query mode comes with `query: False`.
pub async fn boards(
    conn: &mut PgConnection,
    viewer: &Viewer<'_>,
    words: &[String],
    all: bool,
) -> Result<Vec<BoardEntry>, sqlx::Error> {
    if !all && words.is_empty() {
        return Ok(Vec::new());
    }
    let (read_all, read) = match viewer.read {
        Projects::All => (true, Vec::new()),
        Projects::Only(ids) => (false, ids.clone()),
    };
    let rows = sqlx::query!(
        r#"select b.id, b.name, b.mode = 'query' as "query!" from boards b
           where ($5 or replace(lower(btrim(b.name)), 'ё', 'е') = any($6))
             and (b.owner_id = $1 or $1 = any(b.view_users) or b.view_groups && $2
                  or $1 = any(b.edit_users) or b.edit_groups && $2
                  or (b.view_readers
                      and exists (select from board_projects bp where bp.board_id = b.id)
                      and ($3 or not exists (select from board_projects bp
                                             where bp.board_id = b.id
                                               and bp.project_id <> all($4)))))
           order by b.id"#,
        viewer.me,
        viewer.groups,
        read_all,
        &read,
        all,
        words
    )
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| BoardEntry {
            id: r.id,
            name: r.name,
            // The service puts in what a query board's query matches.
            query: r.query.then_some(Filter::False),
        })
        .collect())
}

/// The sprints of `boards` but the internal ones, archived included, in creation order.
pub async fn sprints(
    conn: &mut PgConnection,
    boards: &[BoardId],
) -> Result<Vec<SprintEntry>, sqlx::Error> {
    if boards.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query!(
        "select id, board_id, name, starts_on, ends_on, finished_at, archived_at, created_at
         from sprints where board_id = any($1) and not internal
         order by created_at, id",
        boards
    )
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| SprintEntry {
            board: r.board_id,
            name: r.name,
            dates: SprintDates {
                id: r.id,
                starts_on: r.starts_on,
                ends_on: r.ends_on,
                finished_at: r.finished_at,
                archived: r.archived_at.is_some(),
                internal: false,
                created_at: r.created_at,
            },
        })
        .collect())
}

/// The side names of link types that are not deleted.
pub async fn link_sides(conn: &mut PgConnection) -> Result<Vec<LinkSideEntry>, sqlx::Error> {
    let rows = sqlx::query!(
        "select id, source_name, target_name from link_types where deleted_at is null order by id"
    )
    .fetch_all(conn)
    .await?;
    let mut out = Vec::new();
    for r in rows {
        out.push(LinkSideEntry {
            link_type: r.id,
            side: Side::Source,
            name: r.source_name,
        });
        if let Some(name) = r.target_name {
            out.push(LinkSideEntry {
                link_type: r.id,
                side: Side::Target,
                name,
            });
        }
    }
    Ok(out)
}

/// Published issues the viewer may see, by readable IDs (current and former keys, numbers of
/// issues that moved away) or internal IDs in digits, as `names::issue_key` writes them.
pub async fn issues(
    conn: &mut PgConnection,
    keys: &[String],
    vis: &Visibility,
) -> Result<Vec<IssueEntry>, sqlx::Error> {
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    let (digits, readable): (Vec<&String>, Vec<&String>) = keys
        .iter()
        .partition(|k| k.bytes().all(|b| b.is_ascii_digit()));
    let readable: Vec<String> = readable.into_iter().cloned().collect();
    let mut found: Vec<(String, IssueId)> =
        crate::issues::resolve_tokens(&mut *conn, &readable).await?;
    found.extend(
        digits
            .into_iter()
            .filter_map(|d| d.parse::<IssueId>().ok().map(|id| (d.clone(), id))),
    );
    let ids: Vec<IssueId> = found.iter().map(|(_, id)| *id).collect();
    let vis = Visibility {
        within: None,
        ..vis.clone()
    };
    let mut qb = QueryBuilder::<Postgres>::new("select i.id from issues i where i.id = any(");
    qb.push_bind(ids);
    qb.push(") and ");
    super::compile::push_base(&mut qb, &vis);
    let visible: Vec<IssueId> = qb
        .build()
        .persistent(false)
        .fetch_all(&mut *conn)
        .await?
        .iter()
        .map(|row| row.try_get("id"))
        .collect::<Result<_, _>>()?;
    Ok(found
        .into_iter()
        .filter(|(_, id)| visible.contains(id))
        .map(|(text, id)| IssueEntry { text, id })
        .collect())
}

/// The texts whose text query is empty: only common words or signs (QRY-041).
pub async fn common_words(
    conn: &mut PgConnection,
    texts: &[String],
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"select t as "t!" from unnest($1::text[]) t
           where numnode(phraseto_tsquery('prostor_text', t)) = 0"#,
        texts
    )
    .fetch_all(conn)
    .await
}
