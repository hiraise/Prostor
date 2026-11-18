//! Rows of issues: the issue itself, its field values, history, watchers, votes, tags and cards
//! on sprint boards. Each issue gets its own random stream, so the rows do not depend on how the work is split
//! between workers.

use std::fmt::Write as _;
use std::ops::Range;

use time::macros::datetime;

use super::copy::CopyBuf;
use super::model::{Kind, Model, Project, Role, ValueSet};
use crate::rng::{Rng, Weighted};

/// The renderer's version, stored with every text the dataset writes.
const RENDER_VERSION: i16 = prostor_markup::RENDER_VERSION;

/// The dataset's "now": the newest issue is created just before it.
pub const END: i64 = datetime!(2026-10-01 0:00 UTC).unix_timestamp();
/// Eight years of history.
const SPAN: i64 = 8 * 31_557_600;
const HOUR: i64 = 3600;
const DAY: i64 = 24 * HOUR;
/// Estimations in minutes.
const ESTIMATES: [i64; 7] = [30, 60, 120, 240, 480, 960, 2400];
const POINTS: [f64; 7] = [0.5, 1.0, 2.0, 3.0, 5.0, 8.0, 13.0];
/// History records of an issue get IDs `issue * HISTORY_STRIDE + n`, so they are in order: up
/// to about ten changes and [`COMMENTS_MAX`] comments.
pub const HISTORY_STRIDE: i64 = 64;
/// Comments of an issue get IDs `issue * COMMENT_STRIDE + n`.
pub const COMMENT_STRIDE: i64 = 64;
/// Most comments on one issue; their number follows Zipf's law with a mean of about 3.
pub const COMMENTS_MAX: usize = 40;
const COMMENT_EXPONENT: f64 = 1.65;
/// Files of an issue get IDs `issue * ATTACHMENT_STRIDE + n`.
pub const ATTACHMENT_STRIDE: i64 = 4;
/// Share of issues with files; half of those have two, so about 0.3 files per issue (CNT-017).
const WITH_FILES: f64 = 0.2;
/// Names and types of generated files: metadata only, the store holds no bytes.
const FILES: [(&str, &str, i64); 4] = [
    ("screenshot.png", "image/png", 180_000),
    ("server.log", "application/octet-stream", 42_000),
    ("photo.jpg", "image/jpeg", 950_000),
    ("spec.pdf", "application/pdf", 310_000),
];
/// Links of an issue get IDs `issue * LINK_STRIDE + n`.
pub const LINK_STRIDE: i64 = 4;
/// IDs of the built-in link types the dataset creates (LNK-002): related, dependency, duplicate,
/// subtask.
pub const RELATED: i64 = 1;
pub const DEPENDENCY: i64 = 2;
pub const DUPLICATE: i64 = 3;
pub const SUBTASK: i64 = 4;
/// Each issue links to an earlier one with these chances, about 0.6 links per issue: half
/// related, a quarter subtasks, the rest dependencies and duplicates (LNK-003, LNK-013).
const LINKS: [(i64, f64); 4] = [
    (RELATED, 0.3),
    (SUBTASK, 0.15),
    (DEPENDENCY, 0.1),
    (DUPLICATE, 0.05),
];
/// How far back an issue looks for earlier issues of its project to link to.
pub(crate) const LINK_LOOKBACK: u64 = 400;
/// Share of comments limited to the project team (CNT-003).
const RESTRICTED_COMMENTS: f64 = 0.03;
/// Share of issues limited to an audience (ISS-041): a few people or the project team.
const RESTRICTED: f64 = 0.03;
/// 32 zero bytes in COPY's text form of `bytea`: hashes and secrets of generated files.
const ZERO_BYTES: &str = "\\x0000000000000000000000000000000000000000000000000000000000000000";
/// Every `TAGGERS`-th user has [`TAGS_EACH`] personal tags.
pub const TAGGERS: usize = 4;
pub const TAGS_EACH: usize = 3;

/// Rows of every issue table for a range of issues.
#[derive(Debug)]
pub struct Rows {
    pub issues: CopyBuf,
    pub refs: CopyBuf,
    pub ints: CopyBuf,
    pub floats: CopyBuf,
    pub dates: CopyBuf,
    pub datetimes: CopyBuf,
    pub strings: CopyBuf,
    pub texts: CopyBuf,
    pub history: CopyBuf,
    pub comments: CopyBuf,
    pub attachments: CopyBuf,
    pub links: CopyBuf,
    pub watchers: CopyBuf,
    pub votes: CopyBuf,
    pub tags: CopyBuf,
    /// Cards: the issues of each sprint with their ranks.
    pub cards: CopyBuf,
}

impl Rows {
    pub fn new() -> Self {
        Self {
            issues: CopyBuf::new(
                "copy issues (id, project_id, number, summary, description, reporter_id, \
                 created_at, updated_at, updated_by, resolved_at, audience_users, audience_groups, \
                 votes, version, text_version, description_html, render_version) from stdin",
            ),
            refs: CopyBuf::new("copy issue_ref_values (issue_id, field_id, value) from stdin"),
            ints: CopyBuf::new("copy issue_int_values (issue_id, field_id, value) from stdin"),
            floats: CopyBuf::new("copy issue_float_values (issue_id, field_id, value) from stdin"),
            dates: CopyBuf::new("copy issue_date_values (issue_id, field_id, value) from stdin"),
            datetimes: CopyBuf::new(
                "copy issue_datetime_values (issue_id, field_id, value) from stdin",
            ),
            strings: CopyBuf::new(
                "copy issue_string_values (issue_id, field_id, value) from stdin",
            ),
            texts: CopyBuf::new(
                "copy issue_text_values (issue_id, field_id, value, html, render_version) \
                 from stdin",
            ),
            history: CopyBuf::new(
                "copy issue_history (id, issue_id, at, author_id, origin, kind, field_id, \
                 comment_id, removed, added) from stdin",
            ),
            comments: CopyBuf::new(
                "copy comments (id, issue_id, author_id, text, html, render_version, created_at, \
                 audience_groups) from stdin",
            ),
            attachments: CopyBuf::new(
                "copy attachments (id, issue_id, author_id, name, size, content_type, sha256, \
                 storage_key, width, height, secret, created_at) from stdin",
            ),
            links: CopyBuf::new(
                "copy issue_links (id, type_id, source_id, target_id, created_at, created_by) \
                 from stdin",
            ),
            watchers: CopyBuf::new(
                "copy issue_watchers (issue_id, user_id, created_at) from stdin",
            ),
            votes: CopyBuf::new("copy issue_votes (issue_id, user_id, created_at) from stdin"),
            tags: CopyBuf::new("copy issue_tags (issue_id, tag_id, added_at) from stdin"),
            cards: CopyBuf::new("copy sprint_issues (sprint_id, issue_id, rank) from stdin"),
        }
    }

    pub fn all(&mut self) -> [&mut CopyBuf; 16] {
        [
            &mut self.issues,
            &mut self.refs,
            &mut self.ints,
            &mut self.floats,
            &mut self.dates,
            &mut self.datetimes,
            &mut self.strings,
            &mut self.texts,
            &mut self.history,
            &mut self.comments,
            &mut self.attachments,
            &mut self.links,
            &mut self.watchers,
            &mut self.votes,
            &mut self.tags,
            &mut self.cards,
        ]
    }
}

impl Default for Rows {
    fn default() -> Self {
        Self::new()
    }
}

/// IDs of the personal tags of the user at `index` in [`Model::users`]; empty for most users.
pub fn tags_of(index: usize) -> Range<i64> {
    if !index.is_multiple_of(TAGGERS) {
        return 0..0;
    }
    let first = (index / TAGGERS * TAGS_EACH) as i64 + 1;
    first..first + TAGS_EACH as i64
}

/// A JSON string.
fn json_text(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// The HTML the renderer (crate `markup`) makes of generated text: one paragraph of plain words
/// and full stops, escaped. A test checks it against the renderer.
pub fn paragraph_html(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let mut out = String::with_capacity(text.len() + 9);
    out.push_str("<p>");
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out.push_str("</p>\n");
    out
}

/// What a field record keeps of reference values (`crates/domain/src/issues/history.rs`): IDs of
/// people and groups, `{"id", "name"}` of set values.
fn logged(set: Option<&ValueSet>, kind: Kind, ids: &[i64]) -> String {
    let mut out = String::from("[");
    for (n, id) in ids.iter().enumerate() {
        if n > 0 {
            out.push(',');
        }
        let value = set.and_then(|s| s.values.iter().find(|v| v.id == *id));
        match value {
            Some(value) if kind.set_kind().is_some() && kind != Kind::User => {
                let _ = write!(out, r#"{{"id":{id},"name":"#);
                json_text(&mut out, &value.name);
                out.push('}');
            }
            _ => {
                let _ = write!(out, "{id}");
            }
        }
    }
    out.push(']');
    out
}

/// Rows of the issues with IDs in `ids`.
pub fn generate(model: &Model, ids: Range<u64>) -> Rows {
    let mut rows = Rows::new();
    let comments = Weighted::zipf(COMMENTS_MAX + 1, COMMENT_EXPONENT);
    for id in ids {
        issue(model, id, &comments, &mut rows);
    }
    rows
}

/// One record of an issue's history before it gets its place in time order.
struct Event {
    at: i64,
    author: i64,
    kind: &'static str,
    field: Option<i64>,
    comment: Option<i64>,
    change: Option<(String, String)>,
}

/// An earlier issue of the same project, among the nearest ones; `None` for a project's first.
fn earlier_in_project(model: &Model, id: u64, rng: &mut Rng) -> Option<i64> {
    let project = model.issue_project.get((id - 1) as usize)?;
    let mut found = [0_u64; 8];
    let mut count = 0;
    let mut at = id - 1;
    while at >= 1 && id - at <= LINK_LOOKBACK && count < found.len() {
        if model.issue_project.get((at - 1) as usize) == Some(project) {
            found[count] = at;
            count += 1;
        }
        at -= 1;
    }
    (count > 0).then(|| found[rng.index(count)] as i64)
}

/// A duration between `low` and `high` seconds, short ones more likely (log-uniform).
fn duration(rng: &mut Rng, low: i64, high: i64) -> i64 {
    let (low, high) = (low.max(1) as f64, high.max(1) as f64);
    (low * (high / low).powf(rng.unit())) as i64
}

fn team_member(project: &Project, rng: &mut Rng) -> i64 {
    project
        .team
        .get(project.team_weights.sample(rng))
        .copied()
        .unwrap_or(1)
}

/// One to three different values of a set, or one for a single-valued field.
fn sample_values(set: &ValueSet, rng: &mut Rng, multi: bool) -> ([i64; 3], usize) {
    let mut picked = [0_i64; 3];
    let wanted = if multi { 1 + rng.index(3) } else { 1 };
    let mut len = 0;
    for _ in 0..wanted * 2 {
        if len == wanted {
            break;
        }
        let Some(value) = set.values.get(set.weights.sample(rng)) else {
            break;
        };
        if !picked[..len].contains(&value.id) {
            picked[len] = value.id;
            len += 1;
        }
    }
    (picked, len)
}

/// The value at `progress` (0..1) of an ordered set such as versions, moved by `shift`.
fn value_at(set: &ValueSet, progress: f64, shift: i64) -> Option<i64> {
    let count = set.values.len();
    let at = (progress * count as f64) as i64 + shift;
    let at = usize::try_from(at.max(0))
        .unwrap_or(0)
        .min(count.checked_sub(1)?);
    set.values.get(at).map(|value| value.id)
}

fn issue(model: &Model, id: u64, comment_counts: &Weighted, rows: &mut Rows) {
    let index = (id - 1) as usize;
    let Some(project) = model
        .issue_project
        .get(index)
        .and_then(|&p| model.projects.get(p as usize))
    else {
        return;
    };
    let number = model.issue_number.get(index).copied().unwrap_or(1);
    let issue = id as i64;
    let mut rng = Rng::for_item(model.plan.seed, "issue", id);

    // About 2% of issues are duplicates with a build value (§3.14): the duplicate link is forced
    // below when the random one does not happen, and the build field always gets a value.
    let duplicate_with_build = rng.chance(0.02);

    // Time: issues arrive faster every year; old ones are mostly resolved.
    let age = (id as f64 - 0.5) / model.issue_count() as f64;
    let created = END - SPAN + (SPAN as f64 * age.sqrt()) as i64;
    let resolved = rng.chance(0.95 - 0.75 * age.powi(4));
    let resolved_at = resolved.then(|| (created + duration(&mut rng, HOUR, 180 * DAY)).min(END));
    let active_until =
        resolved_at.unwrap_or_else(|| (created + duration(&mut rng, HOUR, 90 * DAY)).min(END));
    // Position of the issue in its project's life, for versions, builds and sprints.
    let progress = f64::from(number - 1) / f64::from(project.issues.max(1));

    let reporter = if rng.chance(0.7) {
        team_member(project, &mut rng)
    } else {
        model
            .users
            .get(rng.index(model.users.len()))
            .copied()
            .unwrap_or(1)
    };
    let mut summary = String::new();
    let words = 3 + rng.index(8);
    model.vocabulary.phrase(&mut rng, words, &mut summary);
    let description = rng.chance(0.8).then(|| {
        let words = 5 + (rng.unit().powi(3) * 300.0) as usize;
        model.vocabulary.text(&mut rng, words)
    });

    let mut state = None;
    let mut assignee = None;
    for attached in &project.fields {
        let Some(field) = model.fields.get(attached.field) else {
            continue;
        };
        let set = attached.set.and_then(|s| model.sets.get(s));
        let fill = match field.role {
            Role::FixVersions if !resolved => 0.2,
            Role::FixVersions => 0.65,
            Role::FixedInBuild if duplicate_with_build => 1.0,
            Role::FixedInBuild if !resolved => 0.0,
            Role::SpentTime if !resolved => 0.1,
            _ => field.fill,
        };
        if !rng.chance(fill) {
            continue;
        }
        let mut reference = |value: i64| {
            rows.refs.int(issue).int(field.id.into()).int(value).end();
        };
        match (field.role, set) {
            (Role::State, Some(set)) => {
                let weights = if resolved {
                    &set.resolved_weights
                } else {
                    &set.unresolved_weights
                };
                if let Some(value) = set.values.get(weights.sample(&mut rng)) {
                    reference(value.id);
                    state = Some((field.id, set, value.id));
                }
            }
            (Role::Assignee, _) => {
                let user = team_member(project, &mut rng);
                reference(user);
                assignee = Some((field.id, user));
            }
            (Role::FixVersions, Some(set)) => {
                let first = value_at(set, progress, 0);
                let more = [
                    rng.chance(0.3)
                        .then(|| value_at(set, progress, 1))
                        .flatten(),
                    rng.chance(0.1)
                        .then(|| value_at(set, progress, 2))
                        .flatten(),
                ];
                let mut seen = [0_i64; 3];
                for (n, value) in [first, more[0], more[1]].into_iter().flatten().enumerate() {
                    if !seen.contains(&value) {
                        reference(value);
                        seen[n] = value;
                    }
                }
            }
            (Role::AffectedVersions, Some(set)) => {
                let shift = -(rng.below(2) as i64);
                if let Some(value) = value_at(set, progress, shift) {
                    reference(value);
                }
            }
            (Role::FixedInBuild, Some(set)) => {
                if let Some(value) = value_at(set, progress, rng.below(3) as i64) {
                    reference(value);
                }
            }
            (Role::Sprint, Some(set)) => {
                let sprint = (number - 1) / project.sprint_size.max(1);
                if let Some(value) = set.values.get(sprint as usize) {
                    reference(value.id);
                    // On the project's board, in creation order within the sprint.
                    if let Some(board) =
                        project.board.filter(|b| Some(b.sprint_set) == attached.set)
                    {
                        let place = (number - 1) % project.sprint_size.max(1);
                        rows.cards
                            .int(board.first_sprint + i64::from(sprint))
                            .int(issue)
                            .text(&model.ranks[place as usize])
                            .end();
                    }
                }
            }
            (Role::SpentTime, _) => {
                let minutes = rng.between(15, 4800);
                rows.ints.int(issue).int(field.id.into()).int(minutes).end();
            }
            (_, Some(set)) => {
                let (values, len) = sample_values(set, &mut rng, field.multi);
                for &value in &values[..len] {
                    reference(value);
                }
            }
            (_, None) => {
                let field_id = i64::from(field.id);
                match field.kind {
                    Kind::User => {
                        let user = model
                            .users
                            .get(model.user_weights.sample(&mut rng))
                            .copied()
                            .unwrap_or(1);
                        reference(user);
                    }
                    Kind::Integer => {
                        let value = rng.between(1, 100);
                        rows.ints.int(issue).int(field_id).int(value).end();
                    }
                    Kind::Duration => {
                        let value = ESTIMATES[rng.index(ESTIMATES.len())];
                        rows.ints.int(issue).int(field_id).int(value).end();
                    }
                    Kind::Float => {
                        let value = POINTS[rng.index(POINTS.len())];
                        rows.floats.int(issue).int(field_id).float(value).end();
                    }
                    Kind::Date => {
                        let at = created + rng.between(1, 90) * DAY;
                        rows.dates.int(issue).int(field_id).date(at).end();
                    }
                    Kind::DateTime => {
                        let at = created + duration(&mut rng, HOUR, 60 * DAY);
                        rows.datetimes.int(issue).int(field_id).time(at).end();
                    }
                    Kind::String => {
                        let word = model.vocabulary.at(200 + rng.index(1000));
                        let value = format!("{word}-{}", rng.below(20));
                        rows.strings.int(issue).int(field_id).text(&value).end();
                    }
                    Kind::Text => {
                        let words = 20 + rng.index(100);
                        let value = model.vocabulary.text(&mut rng, words);
                        rows.texts
                            .int(issue)
                            .int(field_id)
                            .text(&value)
                            .text(&paragraph_html(&value))
                            .int(RENDER_VERSION.into())
                            .end();
                    }
                    _ => {}
                }
            }
        }
    }

    // History: created, then changes; resolved issues went through more changes, and the last
    // one resolved them. Records follow `crates/domain/src/issues/history.rs`.
    let entries = if resolved {
        1 + rng.below(7)
    } else {
        rng.below(4)
    };
    let mut events: Vec<Event> = Vec::with_capacity(16);
    let mut record = |at: i64,
                      author: i64,
                      kind: &'static str,
                      field: Option<i64>,
                      change: Option<(&str, &str)>| {
        events.push(Event {
            at,
            author,
            kind,
            field,
            comment: None,
            change: change.map(|(a, b)| (a.to_owned(), b.to_owned())),
        });
    };
    record(created, reporter, "created", None, None);
    let mut text_changes = 0;
    for change in 0..entries {
        let at = created + (active_until - created) * (change as i64 + 1) / entries as i64;
        let author = team_member(project, &mut rng);
        let last = change + 1 == entries;
        match (state, assignee) {
            (Some((field, set, to)), _) if resolved && last => {
                let from = set
                    .values
                    .get(set.unresolved_weights.sample(&mut rng))
                    .map_or(0, |value| value.id);
                let (removed, added) = (
                    logged(Some(set), Kind::State, &[from]),
                    logged(Some(set), Kind::State, &[to]),
                );
                record(
                    at,
                    author,
                    "field",
                    Some(field.into()),
                    Some((&removed, &added)),
                );
                record(at, author, "resolved", None, None);
            }
            (_, Some((field, to))) if rng.chance(0.25) => {
                let from = team_member(project, &mut rng);
                let (removed, added) = (format!("[{from}]"), format!("[{to}]"));
                record(
                    at,
                    author,
                    "field",
                    Some(field.into()),
                    Some((&removed, &added)),
                );
            }
            _ if rng.chance(0.4) => {
                // Both texts, old and new; descriptions are shortened, so the dataset fits in the
                // reference server's memory.
                let (kind, words) = if rng.chance(0.5) {
                    ("summary", 3 + rng.index(8))
                } else {
                    ("description", 5 + (rng.unit().powi(3) * 20.0) as usize)
                };
                let mut texts = [String::new(), String::new()];
                for text in &mut texts {
                    let mut plain = String::new();
                    model.vocabulary.phrase(&mut rng, words, &mut plain);
                    json_text(text, &plain);
                }
                record(at, author, kind, None, Some((&texts[0], &texts[1])));
                text_changes += 1;
            }
            _ => {
                // Any field of the project that has a list of values.
                let attached = project.fields.get(rng.index(project.fields.len()));
                let set = attached.and_then(|a| a.set).and_then(|s| model.sets.get(s));
                let field = attached.and_then(|a| model.fields.get(a.field));
                match (field, set) {
                    (Some(field), Some(set)) if !set.values.is_empty() => {
                        let from = set.values.get(rng.index(set.values.len()));
                        let to = set.values.get(rng.index(set.values.len()));
                        let (from, to) = (from.map_or(0, |v| v.id), to.map_or(0, |v| v.id));
                        let (removed, added) = (
                            logged(Some(set), field.kind, &[from]),
                            logged(Some(set), field.kind, &[to]),
                        );
                        record(
                            at,
                            author,
                            "field",
                            Some(field.id.into()),
                            Some((&removed, &added)),
                        );
                    }
                    _ => {
                        let mut texts = [String::new(), String::new()];
                        for text in &mut texts {
                            let mut plain = String::new();
                            model.vocabulary.phrase(&mut rng, 5, &mut plain);
                            json_text(text, &plain);
                        }
                        record(at, author, "summary", None, Some((&texts[0], &texts[1])));
                        text_changes += 1;
                    }
                }
            }
        }
    }

    // Comments (CNT-001): their number by Zipf's law, 5 to 80 words, mostly short, by the
    // reporter or the team; a few limited to the team. Each has its record in the history.
    let count = comment_counts.sample(&mut rng);
    for k in 0..count {
        let at = created + duration(&mut rng, 60, (active_until - created).max(120));
        let author = if rng.chance(0.3) {
            reporter
        } else {
            team_member(project, &mut rng)
        };
        let words = 5 + (rng.unit().powi(3) * 75.0) as usize;
        let text = model.vocabulary.text(&mut rng, words);
        let comment = issue * COMMENT_STRIDE + k as i64;
        let audience: Vec<i64> = if rng.chance(RESTRICTED_COMMENTS) {
            vec![project.team_group]
        } else {
            Vec::new()
        };
        rows.comments
            .int(comment)
            .int(issue)
            .int(author)
            .text(&text)
            .text(&paragraph_html(&text))
            .int(RENDER_VERSION.into())
            .time(at)
            .ints(&audience)
            .end();
        events.push(Event {
            at,
            author,
            kind: "comment",
            field: None,
            comment: Some(comment),
            change: None,
        });
    }

    // Files (CNT-017): a fifth of the issues has one or two, attached by the team while active.
    if rng.chance(WITH_FILES) {
        let count = if rng.chance(0.5) { 2 } else { 1 };
        for k in 0..count {
            let (name, content_type, size) = FILES[rng.index(FILES.len())];
            let picture = content_type.starts_with("image/");
            rows.attachments
                .int(issue * ATTACHMENT_STRIDE + k)
                .int(issue)
                .int(team_member(project, &mut rng))
                .text(if k == 0 { name } else { "notes.txt" })
                .int(size)
                .text(content_type)
                .text(ZERO_BYTES)
                .text(&format!("bench/{issue}/{k}"))
                .opt_int(picture.then_some(1280))
                .opt_int(picture.then_some(720))
                .text(ZERO_BYTES)
                .time(created + duration(&mut rng, 60, (active_until - created).max(120)))
                .end();
        }
    }

    // Links (LNK-003, LNK-013): to an earlier issue, mostly of the same project, so every target
    // has at most one parent and one original and no loop forms. The record is on this issue only:
    // the generator writes one issue at a time. A chain member's subtask link is its chain parent
    // (§3.14), always, instead of the random roll.
    let chained = model.chains.get(&id).copied();
    for (k, (link_type, chance)) in LINKS.iter().enumerate() {
        // A chain member always takes its chain parent; a forced duplicate always gets a link.
        let forced = (*link_type == SUBTASK && chained.is_some())
            || (*link_type == DUPLICATE && duplicate_with_build);
        if !forced && !rng.chance(*chance) {
            continue;
        }
        let other = if *link_type == SUBTASK && chained.is_some() {
            chained
        } else if *link_type == RELATED && rng.chance(0.2) {
            (id > 1).then(|| 1 + rng.below(id - 1) as i64)
        } else {
            earlier_in_project(model, id, &mut rng)
        };
        let Some(other) = other else { continue };
        // This issue is the child of a subtask link and the duplicate of a duplicate link.
        let (source, target, side) = if *link_type == SUBTASK || *link_type == DUPLICATE {
            (other, issue, "target")
        } else {
            (issue, other, "source")
        };
        let at = created + duration(&mut rng, 60, (active_until - created).max(120));
        let author = team_member(project, &mut rng);
        rows.links
            .int(issue * LINK_STRIDE + k as i64)
            .int(*link_type)
            .int(source)
            .int(target)
            .time(at)
            .int(author)
            .end();
        events.push(Event {
            at,
            author,
            kind: "links",
            field: None,
            comment: None,
            change: Some((
                "[]".to_owned(),
                format!(r#"[{{"type": {link_type}, "side": "{side}", "issue": {other}}}]"#),
            )),
        });
    }

    // Records in time order; the creation stays first.
    events[1..].sort_by_key(|e| e.at);
    let (mut updated, mut updated_by) = (created, reporter);
    for (n, event) in events.iter().enumerate() {
        rows.history
            .int(issue * HISTORY_STRIDE + n as i64)
            .int(issue)
            .time(event.at)
            .int(event.author)
            .text("web")
            .text(event.kind)
            .opt_int(event.field)
            .opt_int(event.comment);
        match &event.change {
            Some((removed, added)) => rows.history.text(removed).text(added),
            None => rows.history.null().null(),
        };
        rows.history.end();
        if event.at >= updated {
            (updated, updated_by) = (event.at, event.author);
        }
    }
    let n = events.len() as i64;

    // Audience: most issues are open to everyone who reads the project.
    let (audience_users, audience_groups) = if rng.chance(RESTRICTED) {
        if rng.chance(0.5) {
            let other = team_member(project, &mut rng);
            let mut people = vec![reporter];
            if other != reporter {
                people.push(other);
            }
            (people, Vec::new())
        } else {
            (Vec::new(), vec![project.team_group])
        }
    } else {
        (Vec::new(), Vec::new())
    };

    // Watchers: mostly the reporter, often the assignee, sometimes another team member.
    let mut watchers: Vec<i64> = Vec::with_capacity(3);
    if rng.chance(0.8) {
        watchers.push(reporter);
    }
    if let Some((_, user)) = assignee.filter(|_| rng.chance(0.5)) {
        watchers.push(user);
    }
    if rng.chance(0.2) {
        watchers.push(team_member(project, &mut rng));
    }
    watchers.sort_unstable();
    watchers.dedup();
    for user in watchers {
        rows.watchers.int(issue).int(user).time(created).end();
    }

    let mut voters: Vec<i64> = Vec::new();
    if rng.chance(0.05) {
        for _ in 0..1 + rng.index(5) {
            let user = model
                .users
                .get(model.user_weights.sample(&mut rng))
                .copied()
                .unwrap_or(1);
            if !voters.contains(&user) {
                voters.push(user);
            }
        }
    }
    for &user in &voters {
        rows.votes.int(issue).int(user).time(created).end();
    }

    // Personal tags of a team member.
    if rng.chance(0.08) {
        let member = team_member(project, &mut rng);
        let index = model.users.binary_search(&member).unwrap_or(0);
        let tags = tags_of(index - index % TAGGERS);
        let count = tags.end - tags.start;
        if count > 0 {
            let tag = tags.start + rng.below(count as u64) as i64;
            rows.tags.int(issue).int(tag).time(created).end();
        }
    }

    rows.issues
        .int(issue)
        .int(project.id.into())
        .int(number.into())
        .text(&summary)
        .text(description.as_deref().unwrap_or_default())
        .int(reporter)
        .time(created)
        .time(updated)
        .int(updated_by)
        .opt_time(resolved_at)
        .ints(&audience_users)
        .ints(&audience_groups)
        .int(voters.len() as i64)
        .int(n)
        .int(1 + text_changes)
        .text(&paragraph_html(description.as_deref().unwrap_or_default()))
        .int(RENDER_VERSION.into())
        .end();
}

#[cfg(test)]
mod tests {
    use super::super::Plan;
    use super::*;

    fn model() -> Model {
        let plan = Plan::new(0.002, 3);
        let users = (1..=i64::try_from(plan.users()).unwrap()).collect();
        Model::build(plan, users, 3)
    }

    #[test]
    fn rows_do_not_depend_on_how_work_is_split() {
        let model = model();
        let total = model.issue_count();
        let mut whole = generate(&model, 1..total + 1);
        let mut first = generate(&model, 1..total / 2);
        let mut second = generate(&model, total / 2..total + 1);
        for ((a, b), c) in whole.all().iter().zip(first.all()).zip(second.all()) {
            assert_eq!(a.rows(), b.rows() + c.rows());
        }
        assert_eq!(whole.issues.rows(), total);
    }

    #[test]
    fn issues_have_history_and_values() {
        let model = model();
        let rows = generate(&model, 1..model.issue_count() + 1);
        let issues = rows.issues.rows();
        // Every issue has a state, a priority and a type; most have more.
        assert!(rows.refs.rows() > 4 * issues, "{}", rows.refs.rows());
        // About four changes and three comments per issue.
        let per_issue = rows.history.rows() as f64 / issues as f64;
        assert!((5.5..9.5).contains(&per_issue), "{per_issue}");
        let comments = rows.comments.rows() as f64 / issues as f64;
        assert!((2.0..4.5).contains(&comments), "{comments}");
        let files = rows.attachments.rows() as f64 / issues as f64;
        assert!((0.2..0.4).contains(&files), "{files}");
        // About 0.6 links per issue (LNK-003), plus the chain subtask links of §3.14 and the
        // forced duplicates with builds.
        let links = rows.links.rows() as f64 / issues as f64;
        assert!((0.6..0.9).contains(&links), "{links}");
        assert!(rows.watchers.rows() > issues / 2 && rows.tags.rows() > 0);
        assert!(rows.cards.rows() > 0);
        assert!(rows.texts.rows() > 0 && rows.ints.rows() > 0 && rows.dates.rows() > 0);
    }

    #[test]
    fn generated_html_is_what_the_renderer_makes() {
        let model = model();
        let mut rng = Rng::new(5);
        for words in [1, 5, 40, 300] {
            let text = model.vocabulary.text(&mut rng, words);
            let rendered = prostor_markup::render(
                &text,
                prostor_markup::Profile::Full,
                &std::collections::HashMap::new(),
            );
            assert_eq!(paragraph_html(&text), rendered.html, "{text}");
        }
        assert_eq!(paragraph_html(""), "");
    }
}
