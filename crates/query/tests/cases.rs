//! The shared language cases (QRY-001…QRY-008, QRY-044, QRY-062, error recovery, the printer,
//! UTF-16 offsets). The web's copy of the language (1.5d) reads the same file. Offsets in the
//! file are UTF-16 code units, as JavaScript counts them.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, HashSet};

use serde::Deserialize;
use taskmaster_query::{Class, ProblemCode, Query, Span, classes, parse, print, utf16};

#[derive(Debug, Deserialize)]
struct Case {
    name: String,
    text: String,
    tree: String,
    #[serde(default)]
    print: Option<String>,
    /// Absent means none.
    #[serde(default)]
    problems: Vec<(ProblemCode, usize, usize)>,
    #[serde(default)]
    classes: Option<Vec<(usize, usize, Class)>>,
}

fn all() -> BTreeMap<String, Vec<Case>> {
    serde_json::from_str(include_str!("cases.json")).unwrap()
}

fn run(group: &str) {
    let all = all();
    let cases = all.get(group).unwrap_or_else(|| panic!("no group {group}"));
    assert!(!cases.is_empty());
    for case in cases {
        check(group, case);
    }
}

fn check(group: &str, case: &Case) {
    let name = format!("{group}: {}", case.name);
    let query = parse(&case.text);
    assert_eq!(query.outline(), case.tree, "{name}: tree");

    let spans: Vec<Span> = query.problems.iter().map(|p| p.span).collect();
    let problems: Vec<(ProblemCode, usize, usize)> = query
        .problems
        .iter()
        .zip(utf16::offsets(&case.text, &spans))
        .map(|(p, s)| (p.code, s.start, s.end))
        .collect();
    assert_eq!(problems, case.problems, "{name}: problems");

    if let Some(expected) = &case.classes {
        let found = classes(&query, &case.text);
        let spans: Vec<Span> = found.iter().map(|(s, _)| *s).collect();
        let found: Vec<(usize, usize, Class)> = utf16::offsets(&case.text, &spans)
            .into_iter()
            .zip(found)
            .map(|(s, (_, class))| (s.start, s.end, class))
            .collect();
        assert_eq!(&found, expected, "{name}: classes");
    }

    let printed = print(&query);
    if let Some(expected) = &case.print {
        assert_eq!(&printed, expected, "{name}: print");
    }
    if query.problems.is_empty() {
        round_trip(&query, &printed, &name);
    }
}

/// The printed text parses to the same tree and prints the same again.
fn round_trip(query: &Query, printed: &str, name: &str) {
    let again = parse(printed);
    assert_eq!(again.problems, [], "{name}: reparsing {printed:?}");
    assert_eq!(
        again.outline(),
        query.outline(),
        "{name}: reparsing {printed:?}"
    );
    assert_eq!(print(&again), printed, "{name}: printing twice");
}

#[test]
fn qry_001_keywords_in_any_case_and_the_empty_query() {
    run("qry_001");
}

#[test]
fn qry_002_names_quoted_names_and_folding() {
    run("qry_002");
}

#[test]
fn qry_003_lists_are_any_of_and_repeats_are_and() {
    run("qry_003");
}

#[test]
fn qry_004_exclusion_and_negative_numbers() {
    run("qry_004");
}

#[test]
fn qry_006_quoting() {
    run("qry_006");
}

#[test]
fn qry_007_precedence_and_grouping() {
    run("qry_007");
}

#[test]
fn qry_008_ranges_and_comparisons() {
    run("qry_008");
}

#[test]
fn qry_044_sort() {
    run("qry_044");
}

#[test]
fn qry_049_board_and_sprint_words() {
    run("qry_049");
}

#[test]
fn qry_062_russian_keywords() {
    run("qry_062");
}

#[test]
fn error_recovery() {
    run("recovery");
}

#[test]
fn printer_round_trips() {
    run("print");
}

#[test]
fn utf16_offsets() {
    run("utf16");
}

/// Every parser code has a case at its span, except the limits, which `parse.rs` tests with
/// generated input.
#[test]
fn every_parser_problem_has_a_case() {
    let seen: HashSet<ProblemCode> = all()
        .values()
        .flatten()
        .flat_map(|c| c.problems.iter().map(|p| p.0))
        .collect();
    for code in ProblemCode::PARSER {
        let limit = matches!(
            code,
            ProblemCode::TooDeep | ProblemCode::TooManyTerms | ProblemCode::QueryTooLong
        );
        assert!(limit || seen.contains(code), "no case for {code:?}");
    }
}

/// Random input from the language's pieces: the parser never panics, spans fall on character
/// boundaries, classes come in order without overlaps, and every query without problems
/// survives printing.
#[test]
fn never_panics_and_round_trips() {
    const PIECES: &[&str] = &[
        "a", "b", "state", "votes", " ", " ", ":", ",", "(", ")", "!", "\"", "..", "<", ">=", "or",
        "and", "not", "или", "не", "sort", "desc", "5", "-", "1.", "14:30", "ё", "🎉", "\\", "me",
        "today", "\"x y\"", "\"or\"", "\"text\"",
    ];
    let mut seed: u64 = 0x2026_1023;
    let mut next = |n: usize| {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((seed >> 33) as usize) % n
    };
    for _ in 0..20_000 {
        let len = next(14);
        let text: String = (0..len).map(|_| PIECES[next(PIECES.len())]).collect();
        let query = parse(&text);
        for p in &query.problems {
            assert!(
                text.is_char_boundary(p.span.start) && text.is_char_boundary(p.span.end),
                "{text:?}"
            );
        }
        let mut end = 0;
        for (span, _) in classes(&query, &text) {
            assert!(
                span.start >= end && span.end > span.start,
                "{text:?}: {span:?}"
            );
            assert!(text.is_char_boundary(span.start) && text.is_char_boundary(span.end));
            end = span.end;
        }
        let printed = print(&query);
        if query.problems.is_empty() {
            round_trip(&query, &printed, &text);
        }
    }
}
