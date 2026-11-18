//! The shared rendering cases (CNT-040…CNT-045, CNT-054, CNT-056, XSS). The web app's
//! `MarkdownView` tests read the same file, so both sides agree on the HTML.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, HashMap};

use prostor_markup::{Profile, render};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Case {
    name: String,
    text: String,
    #[serde(default)]
    profile: Option<String>,
    #[serde(default)]
    users: HashMap<String, i64>,
    #[serde(default)]
    html: Option<String>,
    #[serde(default)]
    contains: Vec<String>,
    #[serde(default)]
    excludes: Vec<String>,
    #[serde(default)]
    mentions: Option<Vec<i64>>,
    #[serde(default)]
    issue_tokens: Option<Vec<String>>,
    #[serde(default)]
    files: Option<Vec<String>>,
}

fn run(group: &str) {
    let all: BTreeMap<String, Vec<Case>> =
        serde_json::from_str(include_str!("cases.json")).unwrap();
    let cases = all.get(group).unwrap_or_else(|| panic!("no group {group}"));
    assert!(!cases.is_empty());
    for case in cases {
        let profile = match case.profile.as_deref() {
            Some("basic") => Profile::Basic,
            _ => Profile::Full,
        };
        let out = render(&case.text, profile, &case.users);
        let name = format!("{group}: {}", case.name);
        if let Some(html) = &case.html {
            assert_eq!(&out.html, html, "{name}");
        }
        for part in &case.contains {
            assert!(out.html.contains(part.as_str()), "{name}: {}", out.html);
        }
        for part in &case.excludes {
            assert!(!out.html.contains(part.as_str()), "{name}: {}", out.html);
        }
        if let Some(mentions) = &case.mentions {
            assert_eq!(&out.users, mentions, "{name}");
        }
        if let Some(tokens) = &case.issue_tokens {
            assert_eq!(&out.issue_tokens, tokens, "{name}");
        }
        if let Some(files) = &case.files {
            assert_eq!(&out.files, files, "{name}");
        }
    }
}

#[test]
fn cnt_040_core_syntax() {
    run("cnt_040");
}

#[test]
fn cnt_041_lists_and_checklists() {
    run("cnt_041");
}

#[test]
fn cnt_042_tables() {
    run("cnt_042");
}

#[test]
fn cnt_043_links_and_autolinks() {
    run("cnt_043");
}

#[test]
fn cnt_045_code_blocks() {
    run("cnt_045");
}

#[test]
fn cnt_054_mentions() {
    run("cnt_054");
}

#[test]
fn cnt_056_issue_ids() {
    run("cnt_056");
}

#[test]
fn cnt_044_file_references() {
    run("cnt_044");
}

#[test]
fn cnt_039_basic_profile() {
    run("basic_profile");
}

#[test]
fn xss_cases_stay_inert() {
    run("xss");
}

/// Prints every case's output as JSON when `MARKUP_DUMP` names a file: for reviewing changes.
#[test]
fn dump() {
    let Ok(path) = std::env::var("MARKUP_DUMP") else {
        return;
    };
    let all: BTreeMap<String, Vec<Case>> =
        serde_json::from_str(include_str!("cases.json")).unwrap();
    let mut out = BTreeMap::new();
    for (group, cases) in &all {
        for case in cases {
            let profile = match case.profile.as_deref() {
                Some("basic") => Profile::Basic,
                _ => Profile::Full,
            };
            let r = render(&case.text, profile, &case.users);
            out.insert(format!("{group}/{}", case.name), r.html);
        }
    }
    std::fs::write(path, serde_json::to_string_pretty(&out).unwrap()).unwrap();
}
