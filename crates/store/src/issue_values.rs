//! Field values on issues (FLD-001…FLD-014, D-022): one table per value type. A page of issues
//! loads its values with one query per table (architecture.md §4).

use prostor_domain::fields::FieldKind;
use prostor_domain::issues::values::Value;
use prostor_domain::{FieldId, IssueId, ProjectId};
use sqlx::{PgConnection, PgExecutor};
use time::{Date, OffsetDateTime};

/// A stored value as it comes from its table; the field's kind tells what it means.
#[derive(Debug, Clone, PartialEq)]
pub enum Stored {
    Ref(i64),
    Int(i64),
    Float(f64),
    Date(Date),
    DateTime(OffsetDateTime),
    String(String),
    Text(String),
}

/// One stored value of one issue.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub issue: IssueId,
    pub field: FieldId,
    pub value: Stored,
}

/// Every value of the issues: one query per table.
pub async fn of_issues(conn: &mut PgConnection, ids: &[IssueId]) -> Result<Vec<Row>, sqlx::Error> {
    let mut rows = Vec::new();
    for r in sqlx::query!(
        "select issue_id, field_id, value from issue_ref_values where issue_id = any($1)
         order by issue_id, field_id, value",
        ids
    )
    .fetch_all(&mut *conn)
    .await?
    {
        rows.push(Row {
            issue: r.issue_id,
            field: r.field_id,
            value: Stored::Ref(r.value),
        });
    }
    for r in sqlx::query!(
        "select issue_id, field_id, value from issue_int_values where issue_id = any($1)",
        ids
    )
    .fetch_all(&mut *conn)
    .await?
    {
        rows.push(Row {
            issue: r.issue_id,
            field: r.field_id,
            value: Stored::Int(r.value),
        });
    }
    for r in sqlx::query!(
        "select issue_id, field_id, value from issue_float_values where issue_id = any($1)",
        ids
    )
    .fetch_all(&mut *conn)
    .await?
    {
        rows.push(Row {
            issue: r.issue_id,
            field: r.field_id,
            value: Stored::Float(r.value),
        });
    }
    for r in sqlx::query!(
        "select issue_id, field_id, value from issue_date_values where issue_id = any($1)",
        ids
    )
    .fetch_all(&mut *conn)
    .await?
    {
        rows.push(Row {
            issue: r.issue_id,
            field: r.field_id,
            value: Stored::Date(r.value),
        });
    }
    for r in sqlx::query!(
        "select issue_id, field_id, value from issue_datetime_values where issue_id = any($1)",
        ids
    )
    .fetch_all(&mut *conn)
    .await?
    {
        rows.push(Row {
            issue: r.issue_id,
            field: r.field_id,
            value: Stored::DateTime(r.value),
        });
    }
    for r in sqlx::query!(
        "select issue_id, field_id, value from issue_string_values where issue_id = any($1)",
        ids
    )
    .fetch_all(&mut *conn)
    .await?
    {
        rows.push(Row {
            issue: r.issue_id,
            field: r.field_id,
            value: Stored::String(r.value),
        });
    }
    for r in sqlx::query!(
        "select issue_id, field_id, value from issue_text_values where issue_id = any($1)",
        ids
    )
    .fetch_all(conn)
    .await?
    {
        rows.push(Row {
            issue: r.issue_id,
            field: r.field_id,
            value: Stored::Text(r.value),
        });
    }
    Ok(rows)
}

/// Turns stored rows of one field into a value of its kind; `None` when there are none or they
/// do not fit the kind.
pub fn to_value(kind: FieldKind, stored: &[&Stored]) -> Option<Value> {
    if kind.set_kind().is_some() || kind == FieldKind::Group {
        let ids: Vec<i64> = stored
            .iter()
            .filter_map(|s| match s {
                Stored::Ref(id) => Some(*id),
                _ => None,
            })
            .collect();
        return (!ids.is_empty()).then_some(Value::Refs(ids));
    }
    let first = stored.first()?;
    match (kind, first) {
        (FieldKind::Integer, Stored::Int(n)) => Some(Value::Integer(*n)),
        (FieldKind::Duration, Stored::Int(n)) => Some(Value::Duration(*n)),
        (FieldKind::Float, Stored::Float(n)) => Some(Value::Float(*n)),
        (FieldKind::Date, Stored::Date(d)) => Some(Value::Date(*d)),
        (FieldKind::DateTime, Stored::DateTime(t)) => Some(Value::DateTime(*t)),
        (FieldKind::String, Stored::String(s)) => Some(Value::String(s.clone())),
        (FieldKind::Text, Stored::Text(s)) => Some(Value::Text(s.clone())),
        _ => None,
    }
}

/// Which table holds a kind's values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Table {
    Ref,
    Int,
    Float,
    Date,
    DateTime,
    String,
    Text,
}

fn table(kind: FieldKind) -> Table {
    match kind {
        FieldKind::String => Table::String,
        FieldKind::Text => Table::Text,
        FieldKind::Integer | FieldKind::Duration => Table::Int,
        FieldKind::Float => Table::Float,
        FieldKind::Date => Table::Date,
        FieldKind::DateTime => Table::DateTime,
        FieldKind::Enum
        | FieldKind::State
        | FieldKind::User
        | FieldKind::Group
        | FieldKind::Version
        | FieldKind::Build
        | FieldKind::Owned => Table::Ref,
    }
}

/// Removes a field's value from an issue.
async fn clear(
    conn: &mut PgConnection,
    issue: IssueId,
    field: FieldId,
    kind: FieldKind,
) -> Result<(), sqlx::Error> {
    match table(kind) {
        Table::Ref => {
            sqlx::query!(
                "delete from issue_ref_values where issue_id = $1 and field_id = $2",
                issue,
                field
            )
            .execute(conn)
            .await?
        }
        Table::Int => {
            sqlx::query!(
                "delete from issue_int_values where issue_id = $1 and field_id = $2",
                issue,
                field
            )
            .execute(conn)
            .await?
        }
        Table::Float => {
            sqlx::query!(
                "delete from issue_float_values where issue_id = $1 and field_id = $2",
                issue,
                field
            )
            .execute(conn)
            .await?
        }
        Table::Date => {
            sqlx::query!(
                "delete from issue_date_values where issue_id = $1 and field_id = $2",
                issue,
                field
            )
            .execute(conn)
            .await?
        }
        Table::DateTime => {
            sqlx::query!(
                "delete from issue_datetime_values where issue_id = $1 and field_id = $2",
                issue,
                field
            )
            .execute(conn)
            .await?
        }
        Table::String => {
            sqlx::query!(
                "delete from issue_string_values where issue_id = $1 and field_id = $2",
                issue,
                field
            )
            .execute(conn)
            .await?
        }
        Table::Text => {
            sqlx::query!(
                "delete from issue_text_values where issue_id = $1 and field_id = $2",
                issue,
                field
            )
            .execute(conn)
            .await?
        }
    };
    Ok(())
}

/// Sets a field's value on an issue; `None` clears it. The value must fit the kind.
pub async fn set(
    conn: &mut PgConnection,
    issue: IssueId,
    field: FieldId,
    kind: FieldKind,
    value: Option<&Value>,
) -> Result<(), sqlx::Error> {
    clear(&mut *conn, issue, field, kind).await?;
    let Some(value) = value else {
        return Ok(());
    };
    match value {
        Value::Refs(ids) => {
            sqlx::query!(
                "insert into issue_ref_values (issue_id, field_id, value)
                 select $1, $2, unnest($3::bigint[]) on conflict do nothing",
                issue,
                field,
                ids
            )
            .execute(conn)
            .await?;
        }
        Value::Integer(n) | Value::Duration(n) => {
            sqlx::query!(
                "insert into issue_int_values (issue_id, field_id, value) values ($1, $2, $3)",
                issue,
                field,
                n
            )
            .execute(conn)
            .await?;
        }
        Value::Float(n) => {
            sqlx::query!(
                "insert into issue_float_values (issue_id, field_id, value) values ($1, $2, $3)",
                issue,
                field,
                n
            )
            .execute(conn)
            .await?;
        }
        Value::Date(d) => {
            sqlx::query!(
                "insert into issue_date_values (issue_id, field_id, value) values ($1, $2, $3)",
                issue,
                field,
                d
            )
            .execute(conn)
            .await?;
        }
        Value::DateTime(t) => {
            sqlx::query!(
                "insert into issue_datetime_values (issue_id, field_id, value)
                 values ($1, $2, $3)",
                issue,
                field,
                t
            )
            .execute(conn)
            .await?;
        }
        Value::String(s) => {
            sqlx::query!(
                "insert into issue_string_values (issue_id, field_id, value) values ($1, $2, $3)",
                issue,
                field,
                s
            )
            .execute(conn)
            .await?;
        }
        Value::Text(s) => {
            sqlx::query!(
                "insert into issue_text_values (issue_id, field_id, value) values ($1, $2, $3)",
                issue,
                field,
                s
            )
            .execute(conn)
            .await?;
        }
    }
    Ok(())
}

/// Erases up to `limit` values of a field, in one project or everywhere (FLD-023, FLD-054).
/// Returns how many rows went; repeat until none.
pub async fn erase_batch(
    conn: &mut PgConnection,
    field: FieldId,
    kind: FieldKind,
    project: Option<ProjectId>,
    limit: i64,
) -> Result<u64, sqlx::Error> {
    let done = match (table(kind), project) {
        (Table::Ref, Some(p)) => {
            sqlx::query!(
                "delete from issue_ref_values where (issue_id, field_id, value) in (
                   select v.issue_id, v.field_id, v.value from issue_ref_values v
                   join issues i on i.id = v.issue_id
                   where v.field_id = $1 and i.project_id = $2 limit $3)",
                field,
                p,
                limit
            )
            .execute(conn)
            .await?
        }
        (Table::Ref, None) => {
            sqlx::query!(
                "delete from issue_ref_values where (issue_id, field_id, value) in (
                   select issue_id, field_id, value from issue_ref_values
                   where field_id = $1 limit $2)",
                field,
                limit
            )
            .execute(conn)
            .await?
        }
        (Table::Int, Some(p)) => {
            sqlx::query!(
                "delete from issue_int_values where (issue_id, field_id) in (
                   select v.issue_id, v.field_id from issue_int_values v
                   join issues i on i.id = v.issue_id
                   where v.field_id = $1 and i.project_id = $2 limit $3)",
                field,
                p,
                limit
            )
            .execute(conn)
            .await?
        }
        (Table::Int, None) => {
            sqlx::query!(
                "delete from issue_int_values where (issue_id, field_id) in (
                   select issue_id, field_id from issue_int_values where field_id = $1 limit $2)",
                field,
                limit
            )
            .execute(conn)
            .await?
        }
        (Table::Float, Some(p)) => {
            sqlx::query!(
                "delete from issue_float_values where (issue_id, field_id) in (
                   select v.issue_id, v.field_id from issue_float_values v
                   join issues i on i.id = v.issue_id
                   where v.field_id = $1 and i.project_id = $2 limit $3)",
                field,
                p,
                limit
            )
            .execute(conn)
            .await?
        }
        (Table::Float, None) => {
            sqlx::query!(
                "delete from issue_float_values where (issue_id, field_id) in (
                   select issue_id, field_id from issue_float_values where field_id = $1 limit $2)",
                field,
                limit
            )
            .execute(conn)
            .await?
        }
        (Table::Date, Some(p)) => {
            sqlx::query!(
                "delete from issue_date_values where (issue_id, field_id) in (
                   select v.issue_id, v.field_id from issue_date_values v
                   join issues i on i.id = v.issue_id
                   where v.field_id = $1 and i.project_id = $2 limit $3)",
                field,
                p,
                limit
            )
            .execute(conn)
            .await?
        }
        (Table::Date, None) => {
            sqlx::query!(
                "delete from issue_date_values where (issue_id, field_id) in (
                   select issue_id, field_id from issue_date_values where field_id = $1 limit $2)",
                field,
                limit
            )
            .execute(conn)
            .await?
        }
        (Table::DateTime, Some(p)) => {
            sqlx::query!(
                "delete from issue_datetime_values where (issue_id, field_id) in (
                   select v.issue_id, v.field_id from issue_datetime_values v
                   join issues i on i.id = v.issue_id
                   where v.field_id = $1 and i.project_id = $2 limit $3)",
                field,
                p,
                limit
            )
            .execute(conn)
            .await?
        }
        (Table::DateTime, None) => {
            sqlx::query!(
                "delete from issue_datetime_values where (issue_id, field_id) in (
                   select issue_id, field_id from issue_datetime_values
                   where field_id = $1 limit $2)",
                field,
                limit
            )
            .execute(conn)
            .await?
        }
        (Table::String, Some(p)) => {
            sqlx::query!(
                "delete from issue_string_values where (issue_id, field_id) in (
                   select v.issue_id, v.field_id from issue_string_values v
                   join issues i on i.id = v.issue_id
                   where v.field_id = $1 and i.project_id = $2 limit $3)",
                field,
                p,
                limit
            )
            .execute(conn)
            .await?
        }
        (Table::String, None) => {
            sqlx::query!(
                "delete from issue_string_values where (issue_id, field_id) in (
                   select issue_id, field_id from issue_string_values
                   where field_id = $1 limit $2)",
                field,
                limit
            )
            .execute(conn)
            .await?
        }
        (Table::Text, Some(p)) => {
            sqlx::query!(
                "delete from issue_text_values where (issue_id, field_id) in (
                   select v.issue_id, v.field_id from issue_text_values v
                   join issues i on i.id = v.issue_id
                   where v.field_id = $1 and i.project_id = $2 limit $3)",
                field,
                p,
                limit
            )
            .execute(conn)
            .await?
        }
        (Table::Text, None) => {
            sqlx::query!(
                "delete from issue_text_values where (issue_id, field_id) in (
                   select issue_id, field_id from issue_text_values where field_id = $1 limit $2)",
                field,
                limit
            )
            .execute(conn)
            .await?
        }
    };
    Ok(done.rows_affected())
}

/// How many issues hold a value of the field, in one project or everywhere: the impact shown
/// before detaching or deleting it (FLD-023, FLD-054). Deleted issues count too.
pub async fn issues_with_field(
    db: &mut PgConnection,
    field: FieldId,
    kind: FieldKind,
    project: Option<ProjectId>,
) -> Result<i64, sqlx::Error> {
    // One query per table shape; the kind picks the table.
    let count = match (table(kind), project) {
        (Table::Ref, Some(p)) => {
            sqlx::query_scalar!(
                r#"select count(distinct v.issue_id) as "n!" from issue_ref_values v
               join issues i on i.id = v.issue_id where v.field_id = $1 and i.project_id = $2"#,
                field,
                p
            )
            .fetch_one(db)
            .await?
        }
        (Table::Ref, None) => sqlx::query_scalar!(
            r#"select count(distinct issue_id) as "n!" from issue_ref_values where field_id = $1"#,
            field
        )
        .fetch_one(db)
        .await?,
        (Table::Int, Some(p)) => {
            sqlx::query_scalar!(
                r#"select count(*) as "n!" from issue_int_values v
               join issues i on i.id = v.issue_id where v.field_id = $1 and i.project_id = $2"#,
                field,
                p
            )
            .fetch_one(db)
            .await?
        }
        (Table::Int, None) => {
            sqlx::query_scalar!(
                r#"select count(*) as "n!" from issue_int_values where field_id = $1"#,
                field
            )
            .fetch_one(db)
            .await?
        }
        (Table::Float, Some(p)) => {
            sqlx::query_scalar!(
                r#"select count(*) as "n!" from issue_float_values v
               join issues i on i.id = v.issue_id where v.field_id = $1 and i.project_id = $2"#,
                field,
                p
            )
            .fetch_one(db)
            .await?
        }
        (Table::Float, None) => {
            sqlx::query_scalar!(
                r#"select count(*) as "n!" from issue_float_values where field_id = $1"#,
                field
            )
            .fetch_one(db)
            .await?
        }
        (Table::Date, Some(p)) => {
            sqlx::query_scalar!(
                r#"select count(*) as "n!" from issue_date_values v
               join issues i on i.id = v.issue_id where v.field_id = $1 and i.project_id = $2"#,
                field,
                p
            )
            .fetch_one(db)
            .await?
        }
        (Table::Date, None) => {
            sqlx::query_scalar!(
                r#"select count(*) as "n!" from issue_date_values where field_id = $1"#,
                field
            )
            .fetch_one(db)
            .await?
        }
        (Table::DateTime, Some(p)) => {
            sqlx::query_scalar!(
                r#"select count(*) as "n!" from issue_datetime_values v
               join issues i on i.id = v.issue_id where v.field_id = $1 and i.project_id = $2"#,
                field,
                p
            )
            .fetch_one(db)
            .await?
        }
        (Table::DateTime, None) => {
            sqlx::query_scalar!(
                r#"select count(*) as "n!" from issue_datetime_values where field_id = $1"#,
                field
            )
            .fetch_one(db)
            .await?
        }
        (Table::String, Some(p)) => {
            sqlx::query_scalar!(
                r#"select count(*) as "n!" from issue_string_values v
               join issues i on i.id = v.issue_id where v.field_id = $1 and i.project_id = $2"#,
                field,
                p
            )
            .fetch_one(db)
            .await?
        }
        (Table::String, None) => {
            sqlx::query_scalar!(
                r#"select count(*) as "n!" from issue_string_values where field_id = $1"#,
                field
            )
            .fetch_one(db)
            .await?
        }
        (Table::Text, Some(p)) => {
            sqlx::query_scalar!(
                r#"select count(*) as "n!" from issue_text_values v
               join issues i on i.id = v.issue_id where v.field_id = $1 and i.project_id = $2"#,
                field,
                p
            )
            .fetch_one(db)
            .await?
        }
        (Table::Text, None) => {
            sqlx::query_scalar!(
                r#"select count(*) as "n!" from issue_text_values where field_id = $1"#,
                field
            )
            .fetch_one(db)
            .await?
        }
    };
    Ok(count)
}

/// How many issues hold a value of a set (FLD-032): the impact shown before deleting it.
pub async fn issues_with_value(db: impl PgExecutor<'_>, value: i64) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"select count(distinct v.issue_id) as "n!" from issue_ref_values v
           join fields f on f.id = v.field_id
           where v.value = $1 and f.kind in ('enum', 'state', 'version', 'build', 'owned')"#,
        value
    )
    .fetch_one(db)
    .await
}

/// Up to `limit` issues holding any of these references in fields of these kinds, after `after`:
/// for jobs that walk them.
pub async fn issues_holding(
    db: impl PgExecutor<'_>,
    values: &[i64],
    kinds: &[&str],
    after: IssueId,
    limit: i64,
) -> Result<Vec<IssueId>, sqlx::Error> {
    let kinds: Vec<String> = kinds.iter().map(|k| (*k).to_owned()).collect();
    sqlx::query_scalar!(
        r#"select distinct v.issue_id as "issue_id!" from issue_ref_values v
           join fields f on f.id = v.field_id
           where v.value = any($1) and f.kind = any($2) and v.issue_id > $3
           order by v.issue_id limit $4"#,
        values,
        &kinds,
        after,
        limit
    )
    .fetch_all(db)
    .await
}

/// Points references of a project's issues in one field at other values (a forked set's copies,
/// FLD-030).
pub async fn remap_project(
    conn: &mut PgConnection,
    project: ProjectId,
    field: FieldId,
    from: &[i64],
    to: &[i64],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update issue_ref_values v set value = m.new
         from unnest($3::bigint[], $4::bigint[]) as m(old, new), issues i
         where v.issue_id = i.id and i.project_id = $1 and v.field_id = $2 and v.value = m.old",
        project,
        field,
        from,
        to
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Puts the replacement of deleted groups into group fields that held them (ACC-032).
pub async fn replace_groups(
    conn: &mut PgConnection,
    groups: &[i64],
    replacement: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "insert into issue_ref_values (issue_id, field_id, value)
         select v.issue_id, v.field_id, $2 from issue_ref_values v join fields f on f.id = v.field_id
         where f.kind = 'group' and v.value = any($1)
         on conflict do nothing",
        groups,
        replacement
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "delete from issue_ref_values v using fields f
         where f.id = v.field_id and f.kind = 'group' and v.value = any($1)",
        groups
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Values of user fields held by deleted accounts (ACC-006): where the issue's project says the
/// field cannot be empty they pass to the heir (or stay with the deleted account, shown as a
/// placeholder, when there is no heir); elsewhere they are removed.
pub async fn replace_users(
    conn: &mut PgConnection,
    users: &[i64],
    heir: Option<i64>,
) -> Result<(), sqlx::Error> {
    if let Some(heir) = heir {
        sqlx::query!(
            "insert into issue_ref_values (issue_id, field_id, value)
             select v.issue_id, v.field_id, $2
             from issue_ref_values v
             join fields f on f.id = v.field_id
             join issues i on i.id = v.issue_id
             join project_fields pf on pf.project_id = i.project_id and pf.field_id = v.field_id
             where f.kind = 'user' and v.value = any($1) and not pf.can_be_empty
             on conflict do nothing",
            users,
            heir
        )
        .execute(&mut *conn)
        .await?;
        sqlx::query!(
            "delete from issue_ref_values v using fields f
             where f.id = v.field_id and f.kind = 'user' and v.value = any($1)",
            users
        )
        .execute(conn)
        .await?;
    } else {
        sqlx::query!(
            "delete from issue_ref_values v
             using fields f, issues i, project_fields pf
             where f.id = v.field_id and f.kind = 'user' and v.value = any($1)
               and i.id = v.issue_id and pf.project_id = i.project_id
               and pf.field_id = v.field_id and pf.can_be_empty",
            users
        )
        .execute(conn)
        .await?;
    }
    Ok(())
}

/// The ref values of the given fields of the given issues, one query: what the parents' walk
/// reads to decide and settle (D-049) — only the state fields matter to it, so it does not pay
/// for the seven tables [`of_issues`] reads.
pub async fn refs_of(
    conn: &mut PgConnection,
    issues: &[IssueId],
    fields: &[FieldId],
) -> Result<Vec<Row>, sqlx::Error> {
    if issues.is_empty() || fields.is_empty() {
        return Ok(Vec::new());
    }
    Ok(sqlx::query!(
        "select issue_id, field_id, value from issue_ref_values
         where issue_id = any($1) and field_id = any($2)
         order by issue_id, field_id, value",
        issues,
        fields
    )
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|r| Row {
        issue: r.issue_id,
        field: r.field_id,
        value: Stored::Ref(r.value),
    })
    .collect())
}

/// Writes one ref value on each (issue, field) pair in two statements: the pairs' rows go, the
/// new one lands. The parents' walk of one chain writes in this shape (D-049).
pub async fn set_refs_many(
    conn: &mut PgConnection,
    issues: &[IssueId],
    fields: &[FieldId],
    values: &[i64],
) -> Result<(), sqlx::Error> {
    if issues.is_empty() {
        return Ok(());
    }
    sqlx::query!(
        "delete from issue_ref_values
         where (issue_id, field_id) in (select * from unnest($1::bigint[], $2::bigint[]))",
        issues,
        fields
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "insert into issue_ref_values (issue_id, field_id, value)
         select * from unnest($1::bigint[], $2::bigint[], $3::bigint[]) on conflict do nothing",
        issues,
        fields,
        values
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}
