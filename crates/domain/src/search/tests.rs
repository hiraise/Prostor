//! The resolver on a small catalog: two projects with different state sets, a private field, two
//! fields named "Size", a field named like an attribute, people, a group, a tag, link sides.

use time::macros::{date, datetime};
use trackflow_query::{Direction, ProblemCode, parse};

use super::catalog::{
    Attached, BoardEntry, FieldEntry, GroupEntry, IssueEntry, LinkSideEntry, ProjectEntry,
    SprintEntry, TagEntry, UserEntry, ValueEntry,
};
use super::filter::{Bound, FieldRef, Filter, Range, Target};
use super::sort::{SortBy, SortSpec};
use super::*;
use crate::fields::FieldKind;
use crate::links::Side;

const WEB: i64 = 1;
const APP: i64 = 2;
const ARC: i64 = 3;

fn project(id: i64, key: &str, former: &[&str], name: &str, archived: bool) -> ProjectEntry {
    ProjectEntry {
        id,
        key: key.to_owned(),
        former_keys: former.iter().map(|k| (*k).to_owned()).collect(),
        name: name.to_owned(),
        archived,
    }
}

fn field(id: i64, name: &str, aliases: &[&str], kind: FieldKind, multi: bool) -> FieldEntry {
    FieldEntry {
        id,
        name: name.to_owned(),
        aliases: aliases.iter().map(|a| (*a).to_owned()).collect(),
        kind,
        multi,
    }
}

fn attach(project: i64, field: i64, set: Option<i64>, label: &str, private: bool) -> Attached {
    Attached {
        project,
        field,
        set,
        empty_label: label.to_owned(),
        private,
    }
}

fn value(id: i64, set: i64, name: &str, position: u32, resolved: bool) -> ValueEntry {
    ValueEntry {
        id,
        set,
        name: name.to_owned(),
        position,
        resolved,
    }
}

fn user(id: i64, login: &str, full_name: &str) -> UserEntry {
    UserEntry {
        id,
        login: login.to_owned(),
        full_name: full_name.to_owned(),
    }
}

fn catalog() -> Catalog {
    use FieldKind as K;
    Catalog {
        projects: vec![
            project(WEB, "WEB", &["OLD"], "Web site", false),
            project(APP, "APP", &[], "Mobile app", false),
            project(ARC, "ARC", &[], "Archive", true),
        ],
        fields: vec![
            field(10, "State", &[], K::State, false),
            field(11, "Priority", &[], K::Enum, false),
            field(12, "Assignee", &["owner"], K::User, false),
            field(13, "Estimate", &[], K::Duration, false),
            field(14, "Due date", &[], K::Date, false),
            field(15, "Size", &[], K::Enum, false),
            field(16, "size", &[], K::Integer, false),
            field(17, "Text", &[], K::String, false),
            field(18, "Notes", &[], K::Text, false),
            field(19, "Team", &[], K::Group, false),
            field(20, "Fix versions", &[], K::Version, true),
            field(21, "Started", &[], K::DateTime, false),
            field(22, "Score", &[], K::Float, false),
        ],
        attached: vec![
            attach(WEB, 10, Some(100), "", false),
            attach(APP, 10, Some(101), "", false),
            attach(WEB, 11, Some(110), "", false),
            attach(APP, 11, Some(110), "", true),
            attach(WEB, 12, None, "Unassigned", false),
            attach(APP, 12, None, "", false),
            attach(WEB, 13, None, "", false),
            attach(WEB, 14, None, "", false),
            attach(WEB, 15, Some(120), "", false),
            attach(APP, 16, None, "", false),
            attach(WEB, 17, None, "", false),
            attach(WEB, 18, None, "", false),
            attach(WEB, 19, None, "", false),
            attach(WEB, 20, Some(130), "", false),
            attach(WEB, 21, None, "", false),
            attach(WEB, 22, None, "", false),
            attach(ARC, 10, Some(100), "", false),
        ],
        values: vec![
            value(1000, 100, "Open", 0, false),
            value(1001, 100, "In progress", 1, false),
            value(1002, 100, "Fixed", 2, true),
            value(1003, 100, "Won't fix", 3, true),
            value(1004, 100, "Открыта", 4, false),
            value(1010, 101, "Open", 0, false),
            value(1011, 101, "Fixed", 1, true),
            value(1012, 101, "In progress", 2, false),
            value(1100, 110, "Low", 0, false),
            value(1101, 110, "Normal", 1, false),
            value(1102, 110, "High", 2, false),
            value(1103, 110, "Critical", 3, false),
            value(1200, 120, "Small", 0, false),
            value(1201, 120, "Large", 1, false),
            value(1300, 130, "1.0", 0, false),
            value(1301, 130, "2.0", 1, false),
        ],
        users: vec![
            user(1, "alice", "Alice Smith"),
            user(2, "bob", "Bob Stone"),
            user(3, "bob2", "Bob Stone"),
            user(4, "carol", "Carol King"),
        ],
        groups: vec![GroupEntry {
            id: 50,
            name: "Developers".to_owned(),
            members: vec![2, 4],
        }],
        tags: vec![TagEntry {
            id: 60,
            name: "urgent".to_owned(),
        }],
        link_sides: vec![
            side(70, Side::Source, "blocks"),
            side(70, Side::Target, "blocked by"),
            side(71, Side::Source, "parent of"),
            side(71, Side::Target, "child of"),
        ],
        issues: vec![
            issue("WEB-1", 500),
            issue("OLD-1", 500),
            issue("500", 500),
            issue("APP-2", 501),
        ],
        common_words: ["the".to_owned(), "и the".to_owned()].into(),
        boards: vec![
            board(80, "Team"),
            board(81, "Ops"),
            BoardEntry {
                query: Some(Filter::Projects(vec![1])),
                ..board(82, "Triage")
            },
        ],
        sprints: vec![
            sprint(
                80,
                800,
                "Sprint 1",
                Some((date!(2026 - 09 - 28), date!(2026 - 10 - 10))),
                false,
            ),
            sprint(
                80,
                801,
                "Sprint 2",
                Some((date!(2026 - 10 - 12), date!(2026 - 10 - 25))),
                false,
            ),
            sprint(80, 802, "Old", None, true),
            sprint(81, 810, "Sprint 1", None, false),
        ],
    }
}

fn board(id: i64, name: &str) -> BoardEntry {
    BoardEntry {
        id,
        name: name.to_owned(),
        query: None,
    }
}

fn sprint(
    board: i64,
    id: i64,
    name: &str,
    dates: Option<(time::Date, time::Date)>,
    archived: bool,
) -> SprintEntry {
    SprintEntry {
        board,
        name: name.to_owned(),
        dates: crate::boards::sprints::SprintDates {
            id,
            starts_on: dates.map(|d| d.0),
            ends_on: dates.map(|d| d.1),
            finished_at: None,
            archived,
            internal: false,
            created_at: datetime!(2026-09-01 0:00 UTC),
        },
    }
}

fn side(link_type: i64, side: Side, name: &str) -> LinkSideEntry {
    LinkSideEntry {
        link_type,
        side,
        name: name.to_owned(),
    }
}

fn issue(text: &str, id: i64) -> IssueEntry {
    IssueEntry {
        text: text.to_owned(),
        id,
    }
}

/// Thursday 2026-10-22, 14:05 in Berlin; the viewer is alice and reads private fields in WEB only.
fn ctx() -> Ctx {
    Ctx {
        me: 1,
        clock: Clock {
            now: "2026-10-22T12:05:00Z".parse().unwrap(),
            zone: jiff::tz::TimeZone::get("Europe/Berlin").unwrap(),
        },
        private_readable: Projects::Only(vec![WEB]),
        scope: vec![],
        hide_resolved: false,
    }
}

fn run_with(text: &str, mut ctx: Ctx) -> Result<Resolved, Vec<trackflow_query::Problem>> {
    let cat = catalog();
    let q = parse(text);
    ctx.scope = scope(&q, &cat.projects, None).projects;
    resolve(&q, &cat, &ctx)
}

fn run(text: &str) -> Result<Resolved, Vec<trackflow_query::Problem>> {
    run_with(text, ctx())
}

fn filter(text: &str) -> Filter {
    match run(text) {
        Ok(r) => r.filter,
        Err(problems) => panic!("{text}: {problems:?}"),
    }
}

/// The codes of the problems of a query that does not run.
fn errors(text: &str) -> Vec<ProblemCode> {
    match run(text) {
        Ok(r) => panic!("{text} runs: {:?}", r.filter),
        Err(problems) => problems.iter().map(|p| p.code).collect(),
    }
}

fn fref(id: i64, kind: FieldKind) -> FieldRef {
    // The catalog above: Fix versions is the only multi-valued set field.
    FieldRef {
        id,
        kind,
        multi: id == 20,
        hidden: vec![],
    }
}

fn refs(id: i64, kind: FieldKind, ids: &[i64], implied: Option<bool>) -> Filter {
    Filter::RefValues {
        field: fref(id, kind),
        ids: ids.to_vec(),
        implied,
    }
}

fn priority() -> FieldRef {
    FieldRef {
        id: 11,
        kind: FieldKind::Enum,
        multi: false,
        hidden: vec![APP],
    }
}

#[test]
fn state_values_in_every_set_of_the_scope() {
    let r = run("state: open").unwrap();
    assert_eq!(
        r.filter,
        refs(10, FieldKind::State, &[1000, 1010], Some(false))
    );
    assert!(r.concerns_resolution);
    // The name, then one value per set.
    let bound: Vec<&Binding> = r.bindings.iter().map(|(_, b)| b).collect();
    assert_eq!(
        bound,
        vec![
            &Binding::Field(10),
            &Binding::Value(1000),
            &Binding::Value(1010)
        ]
    );
}

#[test]
fn qry_011_resolved_and_unresolved_per_set() {
    assert_eq!(
        filter("state: unresolved"),
        refs(
            10,
            FieldKind::State,
            &[1000, 1001, 1004, 1010, 1012],
            Some(false)
        )
    );
    // Each project's only state field: resolved values imply a resolved issue.
    assert_eq!(
        filter("state: resolved"),
        refs(10, FieldKind::State, &[1002, 1003, 1011], Some(true))
    );
    // A value named like a keyword wins in the field's own position.
    assert_eq!(
        filter("state: открыта"),
        refs(10, FieldKind::State, &[1004], Some(false))
    );
    assert_eq!(filter("is: resolved"), Filter::Resolved(true));
    assert_eq!(filter("is: открытые"), Filter::Resolved(false));
}

#[test]
fn qry_008_ranges_by_position_per_set() {
    assert_eq!(
        filter("priority: normal..critical"),
        Filter::RefValues {
            field: priority(),
            ids: vec![1101, 1102, 1103],
            implied: None
        }
    );
    // Fixed is third in WEB's set and second in APP's.
    assert_eq!(
        filter("state: open..fixed"),
        refs(10, FieldKind::State, &[1000, 1001, 1002, 1010, 1011], None)
    );
    assert_eq!(
        filter("state < fixed"),
        refs(10, FieldKind::State, &[1000, 1001, 1010], Some(false))
    );
    assert_eq!(
        filter("priority >= high"),
        Filter::RefValues {
            field: priority(),
            ids: vec![1102, 1103],
            implied: None
        }
    );
    assert_eq!(
        errors("priority: high..purple"),
        vec![ProblemCode::UnknownValue]
    );
    assert_eq!(
        errors("assignee: bob..carol"),
        vec![ProblemCode::WrongOperator]
    );
}

#[test]
fn fld_056_empty_labels_per_project() {
    assert_eq!(
        filter("assignee: unassigned"),
        Filter::FieldEmpty {
            field: fref(12, FieldKind::User),
            projects: vec![WEB]
        }
    );
    assert_eq!(
        filter("assignee: none"),
        Filter::FieldEmpty {
            field: fref(12, FieldKind::User),
            projects: vec![WEB, APP]
        }
    );
    assert_eq!(
        filter("is: Unassigned"),
        Filter::FieldEmpty {
            field: fref(12, FieldKind::User),
            projects: vec![WEB]
        }
    );
}

#[test]
fn users_by_login_full_name_group_and_me() {
    let user = |ids: &[i64]| refs(12, FieldKind::User, ids, None);
    assert_eq!(filter("assignee: me"), user(&[1]));
    assert_eq!(filter("assignee: мне"), user(&[1]));
    assert_eq!(filter("owner: BOB"), user(&[2]));
    assert_eq!(filter("assignee: \"carol  king\""), user(&[4]));
    assert_eq!(filter("assignee: developers"), user(&[2, 4]));
    assert_eq!(filter("reporter: bob, carol"), Filter::Reporter(vec![2, 4]));
    assert_eq!(filter("updater: developers"), Filter::Updater(vec![2, 4]));
    assert_eq!(filter("commenter: any"), Filter::Comments);
    assert_eq!(
        errors("assignee: \"Bob Stone\""),
        vec![ProblemCode::AmbiguousUser]
    );
    assert_eq!(errors("reporter: nobody"), vec![ProblemCode::UnknownValue]);
    assert_eq!(errors("reporter > bob"), vec![ProblemCode::WrongOperator]);
    assert_eq!(
        filter("team: developers"),
        refs(19, FieldKind::Group, &[50], None)
    );
}

#[test]
fn qry_013_involves() {
    let involves = Filter::Involves {
        users: vec![1],
        fields: vec![fref(12, FieldKind::User)],
    };
    assert_eq!(filter("involves: me"), involves);
    assert_eq!(filter("is: me"), involves);
    assert_eq!(
        filter("is: bob"),
        Filter::Involves {
            users: vec![2],
            fields: vec![fref(12, FieldKind::User)],
        }
    );
}

#[test]
fn dates_in_the_zone() {
    // Today in Berlin (summer time, UTC+2).
    let today = Range::half_open(
        Some(datetime!(2026-10-21 22:00 UTC)),
        Some(datetime!(2026-10-22 22:00 UTC)),
    );
    assert_eq!(filter("created: today"), Filter::Created(today));
    assert_eq!(filter("изменена: сегодня"), Filter::Updated(today));
    assert_eq!(
        filter("created < 2026-10-22"),
        Filter::Created(Range {
            low: Bound::Open,
            high: Bound::Excluded(datetime!(2026-10-21 22:00 UTC))
        })
    );
    assert_eq!(
        filter("created <= 2026-10-22"),
        Filter::Created(Range {
            low: Bound::Open,
            high: Bound::Excluded(datetime!(2026-10-22 22:00 UTC))
        })
    );
    assert_eq!(
        filter("due-date: 2026-10..2026-11"),
        Filter::Date {
            field: fref(14, FieldKind::Date),
            range: Range::half_open(Some(date!(2026 - 10 - 01)), Some(date!(2026 - 12 - 01)))
        }
    );
    assert_eq!(
        filter("due_date: 2026-10-22T14:30"),
        Filter::Date {
            field: fref(14, FieldKind::Date),
            range: Range::half_open(Some(date!(2026 - 10 - 22)), Some(date!(2026 - 10 - 23)))
        }
    );
    assert_eq!(
        filter("started > yesterday"),
        Filter::DateTime {
            field: fref(21, FieldKind::DateTime),
            range: Range {
                low: Bound::Included(datetime!(2026-10-21 22:00 UTC)),
                high: Bound::Open
            }
        }
    );
    // Nothing comes before the open start of "older".
    assert_eq!(filter("created < older"), Filter::False);
    assert_eq!(filter("resolved: none"), Filter::Resolved(false));
    assert!(run("resolved: today").unwrap().concerns_resolution);
    assert_eq!(filter("commented: none"), Filter::not(Filter::Comments));
    assert_eq!(errors("created: 2026-02-30"), vec![ProblemCode::BadDate]);
    assert_eq!(errors("created > none"), vec![ProblemCode::WrongOperator]);
}

#[test]
fn numbers_and_durations() {
    assert_eq!(
        filter("estimate: 1d..2d"),
        Filter::Int {
            field: fref(13, FieldKind::Duration),
            range: Range::closed(480, 960)
        }
    );
    assert_eq!(
        filter("votes > 3"),
        Filter::Votes(Range {
            low: Bound::Excluded(3),
            high: Bound::Open
        })
    );
    assert_eq!(filter("votes: none"), Filter::Votes(Range::closed(0, 0)));
    assert_eq!(
        filter("score >= -1.5"),
        Filter::Float {
            field: fref(22, FieldKind::Float),
            range: Range {
                low: Bound::Included(-1.5),
                high: Bound::Open
            }
        }
    );
    assert_eq!(errors("votes: many"), vec![ProblemCode::BadNumber]);
    assert_eq!(errors("score: NaN"), vec![ProblemCode::BadNumber]);
}

#[test]
fn fld_016_two_fields_with_one_name() {
    assert_eq!(
        filter("size: large"),
        refs(15, FieldKind::Enum, &[1201], None)
    );
    assert_eq!(
        filter("size: 5"),
        Filter::Int {
            field: fref(16, FieldKind::Integer),
            range: Range::closed(5, 5)
        }
    );
    assert_eq!(
        filter("size: none"),
        Filter::Or(vec![
            Filter::FieldEmpty {
                field: fref(15, FieldKind::Enum),
                projects: vec![WEB]
            },
            Filter::FieldEmpty {
                field: fref(16, FieldKind::Integer),
                projects: vec![APP]
            },
        ])
    );
    // Neither reads it: the first field's problem.
    assert_eq!(errors("size: huge"), vec![ProblemCode::UnknownValue]);
}

#[test]
fn fld_017_aliases_and_folding() {
    assert_eq!(filter("OWNER: me"), filter("assignee: me"));
    assert_eq!(filter("Fix_Versions: 2.0"), filter("\"fix versions\": 2.0"));
}

#[test]
fn shadowed_fields_need_quotes() {
    // `text` is the attribute; the field named Text answers to its quoted name.
    let notes = run("text: foo").unwrap().notes;
    assert_eq!(notes[0].code, ProblemCode::FieldShadowed);
    assert_eq!(
        filter("\"text\": \"Hello World\""),
        Filter::Str {
            field: fref(17, FieldKind::String),
            value: "Hello World".to_owned()
        }
    );
}

#[test]
fn qry_002_unknown_names_and_values_are_errors() {
    assert_eq!(errors("colour: red"), vec![ProblemCode::UnknownAttribute]);
    assert_eq!(errors("state: purple"), vec![ProblemCode::UnknownValue]);
    assert_eq!(errors("project: none"), vec![ProblemCode::UnknownValue]);
    assert_eq!(errors("tag: missing"), vec![ProblemCode::UnknownValue]);
    assert_eq!(errors("is: nothing"), vec![ProblemCode::UnknownValue]);
    assert_eq!(errors("tag: a..b"), vec![ProblemCode::WrongOperator]);
    assert_eq!(errors("state: me"), vec![ProblemCode::UnknownValue]);
    // Parser problems come back too, in order.
    assert_eq!(
        errors("colour: red (state: open"),
        vec![ProblemCode::UnknownAttribute, ProblemCode::UnclosedGroup]
    );
}

#[test]
fn fld_062_private_fields() {
    // Priority is private in APP, where alice may not read private fields.
    assert_eq!(
        filter("priority: high"),
        Filter::RefValues {
            field: priority(),
            ids: vec![1102],
            implied: None
        }
    );
    assert_eq!(
        filter("priority: none"),
        Filter::FieldEmpty {
            field: priority(),
            projects: vec![WEB]
        }
    );
    let mut all = ctx();
    all.private_readable = Projects::All;
    assert_eq!(
        run_with("priority: high", all).unwrap().filter,
        refs(11, FieldKind::Enum, &[1102], None)
    );
}

#[test]
fn scope_from_context_project_conditions_or_everything_readable() {
    let cat = catalog();
    let scope_of = |text: &str| scope(&parse(text), &cat.projects, None).projects;
    // Archived projects are left out unless named.
    assert_eq!(scope_of("state: open"), vec![WEB, APP]);
    assert_eq!(scope_of("project: app, old state: open"), vec![WEB, APP]);
    assert_eq!(scope_of("project: \"Mobile App\""), vec![APP]);
    assert_eq!(scope_of("project: arc"), vec![ARC]);
    // Only top-level, positive conditions narrow it.
    assert_eq!(scope_of("project: app or state: open"), vec![WEB, APP]);
    assert_eq!(scope_of("project: !app"), vec![WEB, APP]);
    assert_eq!(
        scope(&parse("project: app"), &cat.projects, Some(&[ARC])),
        Scope {
            projects: vec![ARC],
            narrowed: true
        }
    );
    let narrowed = |text: &str| scope(&parse(text), &cat.projects, None).narrowed;
    assert!(narrowed("project: app"));
    assert!(!narrowed("state: open"));
    assert!(!narrowed("project: !app"));
    // Names outside the scope are unknown.
    assert_eq!(
        errors("project: app due-date: today"),
        vec![ProblemCode::UnknownAttribute]
    );
    assert_eq!(
        filter("project: app size: 3"),
        Filter::And(vec![
            Filter::Projects(vec![APP]),
            Filter::Int {
                field: fref(16, FieldKind::Integer),
                range: Range::closed(3, 3)
            }
        ])
    );
}

#[test]
fn standalone_values() {
    assert_eq!(filter("is: urgent"), Filter::Tags(vec![60]));
    assert_eq!(filter("is: starred"), Filter::Starred);
    assert_eq!(filter("это: отмечена"), Filter::Starred);
    assert_eq!(filter("is: web"), Filter::Projects(vec![WEB]));
    let r = run("is: fixed").unwrap();
    assert_eq!(
        r.filter,
        refs(10, FieldKind::State, &[1002, 1011], Some(true))
    );
    assert!(r.concerns_resolution);
    assert_eq!(errors("is: 1.0..2.0"), vec![ProblemCode::WrongOperator]);
    assert_eq!(errors("is: none"), vec![ProblemCode::UnknownValue]);
}

#[test]
fn links_tags_and_presence() {
    assert_eq!(
        filter("blocks: web-1"),
        Filter::Linked {
            sides: Some(vec![(70, Side::Source)]),
            target: Target::Issues(vec![500])
        }
    );
    assert_eq!(
        filter("\"child of\": any"),
        Filter::Linked {
            sides: Some(vec![(71, Side::Target)]),
            target: Target::Any
        }
    );
    let any_link = Filter::Linked {
        sides: None,
        target: Target::Any,
    };
    assert_eq!(filter("linked: none"), Filter::not(any_link.clone()));
    assert_eq!(filter("links: any"), any_link);
    assert_eq!(
        filter("linked: OLD-1, app-2"),
        Filter::Or(vec![
            Filter::Linked {
                sides: None,
                target: Target::Issues(vec![500])
            },
            Filter::Linked {
                sides: None,
                target: Target::Issues(vec![501])
            },
        ])
    );
    assert_eq!(errors("linked: WEB-99"), vec![ProblemCode::UnknownValue]);
    assert_eq!(filter("tag: none"), Filter::NoTags);
    assert_eq!(filter("tag: any"), Filter::not(Filter::NoTags));
    assert_eq!(filter("tag: Urgent"), Filter::Tags(vec![60]));
    assert_eq!(filter("attachments: any"), Filter::Attachments);
    assert_eq!(filter("description: none"), Filter::DescriptionEmpty);
    assert_eq!(filter("comments: none"), Filter::not(Filter::Comments));
    assert_eq!(filter("id: old-1, 500"), Filter::Issues(vec![500]));
}

fn all_text() -> TextScope {
    TextScope::All {
        fields: vec![fref(18, FieldKind::Text)],
    }
}

fn text(scope: TextScope, term: TextTerm) -> Filter {
    Filter::Text { scope, term }
}

fn word(w: &str) -> TextTerm {
    TextTerm::Word(w.to_owned())
}

#[test]
fn qry_036_text_terms_and_scopes() {
    assert_eq!(filter("crash"), text(all_text(), word("crash")));
    assert_eq!(
        filter("crash or hang"),
        Filter::or(vec![
            text(all_text(), word("crash")),
            text(all_text(), word("hang"))
        ])
    );
    assert_eq!(
        filter("summary: crash"),
        text(TextScope::Summary, word("crash"))
    );
    assert_eq!(
        filter("comments: crash"),
        text(TextScope::Comments, word("crash"))
    );
    assert_eq!(
        filter("description: crash, !hang"),
        Filter::and(vec![
            text(TextScope::Description, word("crash")),
            Filter::not(text(TextScope::Description, word("hang")))
        ])
    );
    assert_eq!(
        filter("notes: crash"),
        text(TextScope::Field(fref(18, FieldKind::Text)), word("crash"))
    );
    assert_eq!(filter("text: crash"), filter("crash"));
    assert_eq!(
        filter("notes: any"),
        Filter::FieldAny {
            field: fref(18, FieldKind::Text)
        }
    );
    assert_eq!(filter("comments: none"), Filter::not(Filter::Comments));
    assert_eq!(errors("summary: me"), vec![ProblemCode::UnknownValue]);
    assert_eq!(errors("summary: a..b"), vec![ProblemCode::WrongOperator]);
}

#[test]
fn qry_037_quoted_word_is_exact_and_words_a_phrase() {
    assert_eq!(
        filter("\"crashed\""),
        text(all_text(), TextTerm::Exact("crashed".to_owned()))
    );
    assert_eq!(
        filter("summary: \"  login   page \""),
        text(
            TextScope::Summary,
            TextTerm::Phrase("login page".to_owned())
        )
    );
}

#[test]
fn qry_041_common_words_are_left_out() {
    let r = run("the crash").unwrap();
    assert_eq!(r.filter, text(all_text(), word("crash")));
    assert_eq!(r.notes[0].code, ProblemCode::IgnoredCommonWord);
    assert_eq!(filter("the"), Filter::True);
    assert_eq!(filter("crash or the"), filter("crash"));
    assert_eq!(filter("not the"), Filter::True);
    assert_eq!(filter("\"и the\""), Filter::True);
}

#[test]
fn qry_036_at_most_ten_terms() {
    let ten = "a b c d e f g h i j";
    assert!(run(ten).is_ok());
    assert_eq!(errors(&format!("{ten} k")), vec![ProblemCode::TooManyTerms]);
}

#[test]
fn qry_045_relevance_ranks_positive_terms() {
    let r = run("crash not hang").unwrap();
    assert_eq!(
        r.sort[0].by,
        SortBy::Relevance(vec![(all_text(), word("crash"))])
    );
    assert_eq!(r.sort[1].by, SortBy::Id);
    assert_eq!(run("crash sort: votes").unwrap().sort[0].by, SortBy::Votes);
    assert_eq!(run("not crash").unwrap().sort[0].by, SortBy::Updated);
}

#[test]
fn cnt_058_mentions() {
    assert_eq!(filter("mentions: me"), Filter::MentionsUsers(vec![1]));
    assert_eq!(filter("mentions: WEB-1"), Filter::MentionsIssue(500));
    assert_eq!(filter("mentioned-in: app-2"), Filter::MentionedIn(501));
    assert_eq!(errors("mentions: none"), vec![ProblemCode::UnknownValue]);
    assert_eq!(
        errors("mentioned-in: WEB-99"),
        vec![ProblemCode::UnknownValue]
    );
}

#[test]
fn qry_016_single_id() {
    let r = run("WEB-1").unwrap();
    assert_eq!(
        (r.filter, r.single_id),
        (
            Filter::or(vec![
                Filter::Issues(vec![500]),
                text(all_text(), word("WEB-1"))
            ]),
            Some(500)
        )
    );
    assert_eq!(run("id: old-1").unwrap().single_id, Some(500));
    assert_eq!(run("id: 500").unwrap().single_id, Some(500));
    assert_eq!(run("WEB-1 state: open").unwrap().single_id, None);
    assert_eq!(run("id: web-1, app-2").unwrap().single_id, None);
    // An ID-shaped word that names no readable issue is text; quoted, it is text only.
    assert_eq!(filter("WEB-99"), text(all_text(), word("WEB-99")));
    assert_eq!(
        filter("\"WEB-1\""),
        text(all_text(), TextTerm::Exact("WEB-1".to_owned()))
    );
}

#[test]
fn qry_003_repeating_a_field_is_and() {
    let r = run("state: open state: fixed").unwrap();
    assert!(matches!(r.filter, Filter::And(ref parts) if parts.len() == 2));
    assert_eq!(
        r.notes.iter().map(|p| p.code).collect::<Vec<_>>(),
        vec![ProblemCode::SameFieldTwice]
    );
    // Multi-valued fields and lists are fine.
    assert!(
        run("fix-versions: 1.0 fix-versions: 2.0")
            .unwrap()
            .notes
            .is_empty()
    );
    assert!(run("state: open, fixed").unwrap().notes.is_empty());
    assert!(run("state: open state: !fixed").unwrap().notes.is_empty());
}

#[test]
fn lists_with_exclusions() {
    assert_eq!(
        filter("fix-versions: 2.0, !1.0"),
        Filter::And(vec![
            refs(20, FieldKind::Version, &[1301], None),
            Filter::not(refs(20, FieldKind::Version, &[1300], None)),
        ])
    );
    assert_eq!(
        filter("priority: !low, !high"),
        Filter::And(vec![
            Filter::not(Filter::RefValues {
                field: priority(),
                ids: vec![1100],
                implied: None
            }),
            Filter::not(Filter::RefValues {
                field: priority(),
                ids: vec![1102],
                implied: None
            }),
        ])
    );
    assert_eq!(
        filter("state: open, fixed"),
        refs(10, FieldKind::State, &[1000, 1002, 1010, 1011], None)
    );
}

#[test]
fn qry_054_hide_resolved() {
    let mut hide = ctx();
    hide.hide_resolved = true;
    assert_eq!(
        run_with("tag: urgent", hide.clone()).unwrap().filter,
        Filter::And(vec![Filter::Tags(vec![60]), Filter::Resolved(false)])
    );
    assert_eq!(
        run_with("", hide.clone()).unwrap().filter,
        Filter::Resolved(false)
    );
    let r = run_with("state: fixed", hide.clone()).unwrap();
    assert_eq!(
        r.filter,
        refs(10, FieldKind::State, &[1002, 1011], Some(true))
    );
    assert_eq!(r.notes[0].code, ProblemCode::HideResolvedIgnored);
    // A single ID opens even when resolved.
    assert_eq!(run_with("WEB-1", hide).unwrap().filter, filter("WEB-1"));
}

#[test]
fn qry_062_russian_keywords() {
    assert_eq!(
        filter("проект: WEB автор: я"),
        Filter::And(vec![Filter::Projects(vec![WEB]), Filter::Reporter(vec![1])])
    );
    assert_eq!(filter("тег: нет"), Filter::NoTags);
    assert_eq!(filter("не тег: нет"), Filter::not(Filter::NoTags));
    assert_eq!(
        run("сортировка: голоса возр").unwrap().sort[0],
        SortSpec {
            by: SortBy::Votes,
            direction: Direction::Asc
        }
    );
}

#[test]
fn sorting() {
    let sort = |text: &str| run(text).unwrap().sort;
    let by = |by: SortBy, direction| SortSpec { by, direction };
    use Direction::{Asc, Desc};
    assert_eq!(
        sort(""),
        vec![by(SortBy::Updated, Desc), by(SortBy::Id, Desc)]
    );
    assert_eq!(sort("sort: id asc"), vec![by(SortBy::Id, Asc)]);
    assert_eq!(
        sort("sort: priority, votes, starred"),
        vec![
            by(
                SortBy::Positions {
                    field: priority(),
                    positions: vec![(1100, 0), (1101, 1), (1102, 2), (1103, 3)]
                },
                Asc
            ),
            by(SortBy::Votes, Desc),
            by(SortBy::Starred, Asc),
            by(SortBy::Id, Desc),
        ]
    );
    assert_eq!(
        sort("sort: assignee desc")[0],
        by(
            SortBy::Users {
                field: fref(12, FieldKind::User)
            },
            Desc
        )
    );
    assert_eq!(
        sort("sort: estimate")[0],
        by(
            SortBy::Scalar {
                field: fref(13, FieldKind::Duration)
            },
            Asc
        )
    );
    let errors_of = |text: &str| -> Vec<ProblemCode> {
        run(text).unwrap_err().iter().map(|p| p.code).collect()
    };
    assert_eq!(errors_of("sort: tag"), vec![ProblemCode::NotSortable]);
    assert_eq!(errors_of("sort: notes"), vec![ProblemCode::NotSortable]);
    assert_eq!(errors_of("sort: blocks"), vec![ProblemCode::NotSortable]);
    assert_eq!(
        errors_of("sort: colour"),
        vec![ProblemCode::UnknownAttribute]
    );
    assert_eq!(
        errors_of("sort: votes, id, created, updated"),
        vec![ProblemCode::TooManySortKeys]
    );
}

#[test]
fn stored_bindings_round_trip_as_json() {
    let binding = Binding::LinkSide(70, Side::Target);
    let json = serde_json::to_string(&binding).unwrap();
    assert_eq!(json, r#"{"t":"link_side","v":[70,"target"]}"#);
    assert_eq!(serde_json::from_str::<Binding>(&json).unwrap(), binding);
}

#[test]
fn needs_feed_the_catalog() {
    let n = needs(&parse(
        "assignee: \"Carol  King\" priority: low..high blocks: web-1",
    ));
    assert!(n.names.contains("assignee") && n.names.contains("blocks"));
    assert!(n.whole_sets.contains("priority"));
    assert!(n.people.contains("carol king"));
    assert!(n.issue_ids.contains("WEB-1"));
}

#[test]
fn qry_049_board_conditions() {
    assert_eq!(filter("board: team"), Filter::OnBoards(vec![80]));
    assert_eq!(filter("доска: Team, Ops"), Filter::OnBoards(vec![80, 81]));
    assert_eq!(filter("board: any"), Filter::OnBoards(vec![80, 81]));
    assert_eq!(
        filter("board: none"),
        Filter::not(Filter::OnBoards(vec![80, 81]))
    );
    assert_eq!(
        filter("!board: Ops"),
        Filter::not(Filter::OnBoards(vec![81]))
    );
    assert_eq!(errors("board: Hidden"), vec![ProblemCode::UnknownValue]);
    let bound = run("board: Team").unwrap().bindings;
    assert_eq!(bound[1].1, Binding::Board(80));
    // A query board by its query; `any`, `none` and sprints count boards of cards.
    assert_eq!(filter("board: Triage"), Filter::Projects(vec![1]));
    assert_eq!(
        filter("board: Team, Triage"),
        Filter::or(vec![Filter::OnBoards(vec![80]), Filter::Projects(vec![1])])
    );
    assert_eq!(
        filter("!board: Triage"),
        Filter::not(Filter::Projects(vec![1]))
    );
    assert_eq!(
        errors("sprint: \"Triage/current\""),
        vec![ProblemCode::UnknownValue]
    );
}

#[test]
fn qry_049_sprint_conditions() {
    // The union over boards; archived sprints resolve.
    assert_eq!(
        filter("sprint: \"sprint 1\""),
        Filter::Sprints(vec![800, 810])
    );
    assert_eq!(filter("sprint: Old"), Filter::Sprints(vec![802]));
    assert_eq!(
        filter("sprint: \"Team/Sprint 1\""),
        Filter::Sprints(vec![800])
    );
    // Thursday 2026-10-22: Sprint 2 runs on Team; Ops has one unscheduled sprint.
    assert_eq!(filter("sprint: current"), Filter::Sprints(vec![801, 810]));
    assert_eq!(
        filter("спринт: \"ops/текущий\""),
        Filter::Sprints(vec![810])
    );
    assert_eq!(
        filter("sprint: none"),
        Filter::not(Filter::OnBoards(vec![80, 81]))
    );
    assert_eq!(
        errors("sprint: \"Team/Nope\""),
        vec![ProblemCode::UnknownValue]
    );
    assert_eq!(
        errors("sprint: \"Nope/Sprint 1\""),
        vec![ProblemCode::UnknownValue]
    );
    let bound: Vec<Binding> = run("sprint: \"Team/Sprint 1\" sprint: current")
        .unwrap()
        .bindings
        .into_iter()
        .map(|(_, b)| b)
        .collect();
    assert_eq!(
        bound,
        vec![
            Binding::Attribute("sprint".to_owned()),
            Binding::Board(80),
            Binding::Sprint(800),
            Binding::Attribute("sprint".to_owned()),
            Binding::CurrentSprint(None),
        ]
    );
}

#[test]
fn qry_049_stored_board_and_sprint_names() {
    use super::stored::Names;
    let text = "board: Team sprint: \"Team/Sprint 1\" sprint: \"Team/текущий\" sprint: Old";
    let q = parse(text);
    let r = run(text).unwrap();
    let stored = Stored::new(&q, r.bindings);
    let refs = stored.refs();
    assert_eq!((refs.boards, refs.sprints), (vec![80], vec![800, 802]));
    let mut names = Names::default();
    names.boards.insert(80, "Core".to_owned());
    names.sprints.insert(800, "First".to_owned());
    names.sprints.insert(802, "Old".to_owned());
    let (printed, problems) = stored.text(&names);
    assert_eq!(
        printed,
        "board: Core sprint: Core/First sprint: Core/текущий sprint: Old"
    );
    assert!(problems.is_empty());
    names.sprints.remove(&800);
    let (printed, problems) = stored.text(&names);
    assert_eq!(
        printed,
        "board: Core sprint: \"Team/Sprint 1\" sprint: Core/текущий sprint: Old"
    );
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].code, ProblemCode::DeletedReference);
}
