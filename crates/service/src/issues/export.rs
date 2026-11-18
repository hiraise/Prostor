//! Exporting issues to CSV and XLSX (QRY-084, step 2.2 §9): the selection toolbar's «Экспорт»
//! answered with a file. The columns are fixed — ID, project, summary, reporter, assignee,
//! state, created, updated, resolved, tags, votes, then the union of the exported issues' custom
//! fields (each named by its field), then the description as the Markdown text it is — written in
//! the instance's language, its dates in the instance's format (ADM-016) and zone, 24-hour as
//! the instance patterns are. Reading follows visibility as everywhere (ACC-049): issues the
//! viewer may not see are skipped and still counted. The row count is capped by a constant until
//! the setting of 2.11 (`ADM-005`).

use sqlx::PgConnection;
use taskmaster_domain::access::Access;
use taskmaster_domain::issues::values::Value;
use taskmaster_domain::settings::Language;
use taskmaster_domain::{IssueId, ProjectId};
use taskmaster_store::PgPool;
use taskmaster_store::issues::Issue;
use taskmaster_store::projects;
use taskmaster_store::{issues as store_issues, settings};
use time::OffsetDateTime;

use super::{Described, IssueView, can_read, describe};
use crate::Result;
use crate::search::SearchInput;

/// Most rows one export has: a constant in one place until the setting of 2.11 (`ADM-005`).
pub const EXPORT_CAP: usize = 500;

/// What one export runs on (§9): the chosen issues, or everything a query matches.
pub enum Ask {
    Ids(Vec<IssueId>),
    Query(SearchInput),
}

/// One export, ready for either writer: the header and the rows as cell strings.
pub struct Exported {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    /// How many issues were asked for — the query's count, or the IDs asked for — with the
    /// invisible ones an export skips and the rows a truncation drops included.
    pub total: i64,
}

/// Collects what an export writes (§9). Visibility decides what a row is: issues the viewer may
/// not see are skipped, not errors.
pub async fn collect(pool: &PgPool, viewer: &Access, ask: Ask) -> Result<Exported> {
    let mut conn = pool.acquire().await?;
    let (found, total) = match ask {
        Ask::Ids(ids) => {
            let mut unique = dedup_ordered(ids);
            let total = i64::try_from(unique.len()).unwrap_or(i64::MAX);
            unique.truncate(EXPORT_CAP);
            let found = visible(&mut conn, viewer, &unique).await?;
            (found, total)
        }
        Ask::Query(input) => {
            let (ids, total) =
                crate::search::matching(&mut conn, viewer, input, EXPORT_CAP as u32).await?;
            let mut found = store_issues::get_many(&mut *conn, &ids).await?;
            // The query's order, which `get_many` does not keep.
            found.sort_by_key(|issue| ids.iter().position(|id| *id == issue.id));
            (found, total)
        }
    };
    let language = match setting(&mut conn, "system_language").await? {
        Some(value) if value == "ru" => Language::Ru,
        _ => Language::En,
    };
    let date_format = setting(&mut conn, "default_date_format")
        .await?
        .unwrap_or_else(|| "locale".to_owned());
    let zone = setting(&mut conn, "default_time_zone")
        .await?
        .unwrap_or_else(|| "UTC".to_owned());
    let zone = jiff::tz::TimeZone::get(&zone)
        .map_err(|_| crate::Error::Internal(format!("unknown time zone {zone}")))?;
    let shapes = Shapes::new(language, &date_format, zone);
    let d = describe(&mut conn, viewer, found).await?;
    Ok(build(&d, shapes, total))
}

/// The issues the viewer may see, in the order asked.
async fn visible(conn: &mut PgConnection, viewer: &Access, ids: &[IssueId]) -> Result<Vec<Issue>> {
    let mut found = store_issues::get_many(&mut *conn, ids).await?;
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
    Ok(list)
}

/// A stored setting as a string, trimmed; `None` when unset or not a string.
async fn setting(conn: &mut PgConnection, key: &str) -> Result<Option<String>> {
    Ok(settings::value(&mut *conn, key)
        .await?
        .and_then(|v| v.as_str().map(str::to_owned)))
}

/// Without repeats, in the order first asked.
fn dedup_ordered(mut ids: Vec<IssueId>) -> Vec<IssueId> {
    ids.dedup_by(|a, b| a == b);
    ids
}

/// How the instance spells things (ADM-016, ADM-017): the fixed columns' names, dates and
/// durations. Built once per export.
struct Shapes {
    language: Language,
    date: DateShape,
    zone: jiff::tz::TimeZone,
}

/// One of the four ways a date is shown (ADM-016), as the export writes it; `Locale` follows the
/// instance language, with 24-hour times as the explicit patterns have.
enum DateShape {
    Locale,
    DmyDot,
    DmyName,
    Iso,
}

impl Shapes {
    fn new(language: Language, format: &str, zone: jiff::tz::TimeZone) -> Self {
        let date = match format {
            "dmy_dot" => DateShape::DmyDot,
            "dmy_name" => DateShape::DmyName,
            "iso" => DateShape::Iso,
            _ => DateShape::Locale,
        };
        Self {
            language,
            date,
            zone,
        }
    }

    /// A header of the fixed columns (§9), in the instance's language.
    fn columns(&self) -> Vec<String> {
        match self.language {
            Language::En => [
                "ID",
                "Project",
                "Summary",
                "Reporter",
                "Assignee",
                "State",
                "Created",
                "Updated",
                "Resolved",
                "Tags",
                "Votes",
                "Description",
            ],
            Language::Ru => [
                "ID",
                "Проект",
                "Сводка",
                "Автор",
                "Исполнитель",
                "Состояние",
                "Создана",
                "Обновлена",
                "Решена",
                "Теги",
                "Голоса",
                "Описание",
            ],
        }
        .iter()
        .map(|name| (*name).to_owned())
        .collect()
    }

    /// A full date with time, the shape a list shows (ADM-016): `31.12.2000, 23:59`,
    /// `31 дек. 2000, 23:59`, `2000-12-31 23:59`; `Locale` joins the language's order —
    /// `Dec 31, 2000, 23:59` or `31 дек. 2000, 23:59`.
    fn datetime(&self, at: OffsetDateTime) -> String {
        let Some((year, month, day, hour, minute)) = self.parts(at) else {
            return String::new();
        };
        match self.date {
            DateShape::DmyDot => format!("{day:02}.{month:02}.{year}, {hour:02}:{minute:02}"),
            DateShape::Iso => format!("{year}-{month:02}-{day:02} {hour:02}:{minute:02}"),
            DateShape::DmyName => format!(
                "{} {} {}, {:02}:{:02}",
                day,
                self.month_name(month),
                year,
                hour,
                minute
            ),
            DateShape::Locale => match self.language {
                Language::En => format!(
                    "{} {day}, {year}, {hour:02}:{minute:02}",
                    self.month_name(month)
                ),
                Language::Ru => format!(
                    "{} {} {}, {:02}:{:02}",
                    day,
                    self.month_name(month),
                    year,
                    hour,
                    minute
                ),
            },
        }
    }

    /// A calendar date (a date field's value), the same shapes without the time.
    fn date_only(&self, year: i32, month: u8, day: u8) -> String {
        match self.date {
            DateShape::DmyDot => format!("{day:02}.{month:02}.{year}"),
            DateShape::Iso => format!("{year}-{month:02}-{day:02}"),
            DateShape::DmyName => format!("{day} {} {year}", self.month_name(month)),
            DateShape::Locale => match self.language {
                Language::En => format!("{} {day}, {year}", self.month_name(month)),
                Language::Ru => format!("{day} {} {year}", self.month_name(month)),
            },
        }
    }

    /// Minutes the way people write them, as the command line reads them (`1h 30m`).
    fn duration(&self, minutes: i64) -> String {
        let _ = self.language;
        let hours = minutes / 60;
        let rest = minutes % 60;
        match (hours, rest) {
            (0, 0) => "0m".to_owned(),
            (0, m) => format!("{m}m"),
            (h, 0) => format!("{h}h"),
            (h, m) => format!("{h}h {m}m"),
        }
    }

    fn month_name(&self, month: u8) -> &'static str {
        let names = match self.language {
            Language::En => [
                "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
            ],
            Language::Ru => [
                "янв.",
                "февр.",
                "мар.",
                "апр.",
                "мая",
                "июн.",
                "июл.",
                "авг.",
                "сент.",
                "окт.",
                "нояб.",
                "дек.",
            ],
        };
        names[(month as usize - 1).min(11)]
    }

    /// An instant in the instance's zone, split into calendar parts; `None` when the instant
    /// does not convert (never for a stored timestamp).
    fn parts(&self, at: OffsetDateTime) -> Option<(i32, u8, u8, u8, u8)> {
        let stamp = jiff::Timestamp::new(
            at.unix_timestamp(),
            i32::try_from(at.nanosecond()).unwrap_or(0),
        )
        .ok()?;
        let civil = stamp.to_zoned(self.zone.clone()).datetime();
        Some((
            i32::from(civil.year()),
            u8::try_from(civil.month()).unwrap_or(1),
            civil.day().try_into().ok()?,
            civil.hour().try_into().ok()?,
            civil.minute().try_into().ok()?,
        ))
    }
}

/// Lays the described issues out as columns and rows: the fixed columns of §9, then the custom
/// fields in the order they first appear, then the description.
fn build(d: &Described, shapes: Shapes, total: i64) -> Exported {
    // The fixed columns, with the description last; the custom fields slot in before it.
    let mut columns = shapes.columns();
    let description_at = columns.len() - 1;

    // Which fields get their own column: everything except the state and assignee of the fixed
    // columns, in first-seen order.
    let mut field_columns: Vec<i64> = Vec::new();
    for view in &d.issues {
        for field in &view.fields {
            let builtin = field.field.field.builtin.as_deref();
            if builtin != Some("state")
                && builtin != Some("assignee")
                && !field_columns.contains(&field.field.id())
            {
                field_columns.push(field.field.id());
            }
        }
    }
    let name_of = |id: i64| -> String {
        d.issues
            .iter()
            .flat_map(|view| view.fields.iter())
            .find(|field| field.field.id() == id)
            .map(|field| field.field.field.name.clone())
            .unwrap_or_default()
    };
    let custom: Vec<String> = field_columns.iter().map(|id| name_of(*id)).collect();
    columns.splice(description_at..description_at, custom.to_vec());

    let mut rows = Vec::with_capacity(d.issues.len());
    for view in &d.issues {
        let mut row = Vec::with_capacity(columns.len());
        row.push(
            view.readable_id()
                .unwrap_or_else(|| view.issue.id.to_string()),
        );
        row.push(view.project.name.clone());
        row.push(view.issue.summary.clone());
        row.push(name(d, view.issue.reporter_id));
        row.push(builtin(d, view, "assignee", &shapes));
        row.push(builtin(d, view, "state", &shapes));
        row.push(shapes.datetime(view.issue.created_at));
        row.push(shapes.datetime(view.issue.updated_at));
        row.push(
            view.issue
                .resolved_at
                .map(|at| shapes.datetime(at))
                .unwrap_or_default(),
        );
        row.push(
            view.tags
                .iter()
                .map(|tag| tag.name.clone())
                .collect::<Vec<_>>()
                .join(", "),
        );
        row.push(view.issue.votes.to_string());
        for id in &field_columns {
            let value = view
                .fields
                .iter()
                .find(|field| field.field.id() == *id)
                .and_then(|field| field.value.as_ref());
            row.push(
                value
                    .map(|value| cell(d, value, &shapes))
                    .unwrap_or_default(),
            );
        }
        row.push(view.issue.description.clone());
        rows.push(row);
    }
    Exported {
        columns,
        rows,
        total,
    }
}

/// The value of the assignee or state column: the field the built-in key names, written as any
/// other field of its kind, empty when the issue has none.
fn builtin(d: &Described, view: &IssueView, key: &str, shapes: &Shapes) -> String {
    view.fields
        .iter()
        .find(|field| field.field.field.builtin.as_deref() == Some(key))
        .and_then(|field| field.value.as_ref())
        .map(|value| cell(d, value, shapes))
        .unwrap_or_default()
}

/// One field value as a cell (§9): set values, accounts and groups by name in the set's order,
/// dates in the instance's format, durations as people write them.
fn cell(d: &Described, value: &Value, shapes: &Shapes) -> String {
    match value {
        Value::String(text) | Value::Text(text) => text.clone(),
        Value::Integer(n) => n.to_string(),
        Value::Float(n) => n.to_string(),
        Value::Date(day) => shapes.date_only(day.year(), u8::from(day.month()), day.day()),
        Value::DateTime(at) => shapes.datetime(*at),
        Value::Duration(minutes) => shapes.duration(*minutes),
        Value::Refs(ids) => {
            // Set values keep the set's order; accounts and groups the given order.
            let mut held: Vec<&taskmaster_store::value_sets::Value> =
                ids.iter().filter_map(|id| d.value(*id)).collect();
            if held.is_empty() {
                ids.iter()
                    .filter_map(|id| {
                        d.user(*id)
                            .map(|user| user.full_name.clone())
                            .or_else(|| d.group(*id).map(|group| group.name.clone()))
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            } else {
                held.sort_by(|a, b| a.rank.cmp(&b.rank));
                held.into_iter()
                    .map(|value| value.name.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        }
    }
}

/// A person's full name, their login when the account is gone.
fn name(d: &Described, id: taskmaster_domain::UserId) -> String {
    d.user(id)
        .map(|user| user.full_name.clone())
        .unwrap_or_default()
}

/// The export as CSV (§9): UTF-8 with a BOM, cells quoted when they hold a comma, a quote or a
/// newline, rows closed RFC 4180-style.
pub fn csv(out: &Exported) -> Vec<u8> {
    let mut text = String::from("\u{feff}");
    write_row(&mut text, &out.columns);
    for row in &out.rows {
        write_row(&mut text, row);
    }
    text.into_bytes()
}

fn write_row(text: &mut String, row: &[String]) {
    for (index, cell) in row.iter().enumerate() {
        if index > 0 {
            text.push(',');
        }
        if cell.contains([',', '"', '\n', '\r']) {
            text.push('"');
            text.push_str(&cell.replace('"', "\"\""));
            text.push('"');
        } else {
            text.push_str(cell);
        }
    }
    text.push_str("\r\n");
}

/// The export as XLSX (§9): one sheet named in the instance's language, the header frozen.
pub fn xlsx(out: &Exported) -> Result<Vec<u8>> {
    let russian = out.columns.first().is_some_and(|column| column == "Проект");
    let mut book = rust_xlsxwriter::Workbook::new();
    let sheet = book.add_worksheet();
    sheet
        .set_name(if russian { "Задачи" } else { "Issues" })
        .map_err(internal)?;
    for (column, header) in out.columns.iter().enumerate() {
        sheet
            .write(0, u16::try_from(column).unwrap_or(u16::MAX), header)
            .map_err(internal)?;
    }
    for (row_index, row) in out.rows.iter().enumerate() {
        let row_index = u32::try_from(row_index + 1).unwrap_or(u32::MAX);
        for (column, cell) in row.iter().enumerate() {
            sheet
                .write(
                    row_index,
                    u16::try_from(column).unwrap_or(u16::MAX),
                    cell.as_str(),
                )
                .map_err(internal)?;
        }
    }
    sheet.set_freeze_panes(1, 0).map_err(internal)?;
    book.save_to_buffer().map_err(internal)
}

fn internal(error: rust_xlsxwriter::XlsxError) -> crate::Error {
    crate::Error::Internal(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_quotes_and_bom() {
        let out = Exported {
            columns: vec!["ID".to_owned(), "Сводка".to_owned()],
            rows: vec![vec![
                "WEB-1".to_owned(),
                "Fix \"login\", now\nwith two lines".to_owned(),
            ]],
            total: 1,
        };
        let bytes = csv(&out);
        assert_eq!(&bytes[..3], "\u{feff}".as_bytes(), "the BOM");
        let text = String::from_utf8(bytes).unwrap();
        assert_eq!(
            text,
            "\u{feff}ID,Сводка\r\nWEB-1,\"Fix \"\"login\"\", now\nwith two lines\"\r\n"
        );
    }

    #[test]
    fn durations_read_as_written() {
        let shapes = Shapes::new(Language::En, "iso", jiff::tz::TimeZone::UTC);
        assert_eq!(shapes.duration(0), "0m");
        assert_eq!(shapes.duration(45), "45m");
        assert_eq!(shapes.duration(60), "1h");
        assert_eq!(shapes.duration(90), "1h 30m");
    }

    #[test]
    fn dates_follow_the_instance_pattern() {
        let utc = jiff::tz::TimeZone::UTC;
        let at = OffsetDateTime::parse(
            "2000-12-31T23:59:00Z",
            &time::format_description::well_known::Rfc3339,
        )
        .unwrap();
        let en = Shapes::new(Language::En, "dmy_dot", utc.clone());
        assert_eq!(en.datetime(at), "31.12.2000, 23:59");
        assert_eq!(en.date_only(2000, 12, 31), "31.12.2000");
        let ru = Shapes::new(Language::Ru, "dmy_name", utc.clone());
        assert_eq!(ru.datetime(at), "31 дек. 2000, 23:59");
        assert_eq!(ru.date_only(2000, 1, 9), "9 янв. 2000");
        let iso = Shapes::new(Language::En, "iso", utc.clone());
        assert_eq!(iso.datetime(at), "2000-12-31 23:59");
        let locale = Shapes::new(Language::En, "locale", utc.clone());
        assert_eq!(locale.datetime(at), "Dec 31, 2000, 23:59");
        let locale_ru = Shapes::new(Language::Ru, "locale", utc);
        assert_eq!(locale_ru.datetime(at), "31 дек. 2000, 23:59");
    }
}
