//! The word diff of text edits (ISS-051 on the web, NTF-060 in emails): what changed between
//! two texts. Tokens are words and runs of whitespace; the shortest edit script by Myers' O(ND)
//! algorithm. Very long or very different texts are shown as one removal and one addition
//! instead. The web app's conflict view (`web/src/lib/features/issues/page/diff.ts`) follows the
//! same rules; both run the cases in `diff_cases.json`.

use serde::{Deserialize, Serialize};

/// Above this many tokens on either side the diff is not computed.
pub const MAX_TOKENS: usize = 20_000;
/// Texts further apart than this many token edits are shown whole.
const MAX_EDITS: usize = 2_000;

/// What happened to a run of tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Op {
    Same,
    Added,
    Removed,
}

impl Op {
    pub fn key(self) -> &'static str {
        match self {
            Op::Same => "same",
            Op::Added => "added",
            Op::Removed => "removed",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "same" => Some(Op::Same),
            "added" => Some(Op::Added),
            "removed" => Some(Op::Removed),
            _ => None,
        }
    }
}

/// One part of a word diff: a run of tokens and what happened to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Part {
    pub op: Op,
    pub text: String,
}

/// The parts of the word diff of two texts.
pub fn diff_words(before: &str, after: &str) -> Vec<Part> {
    if before == after {
        return if before.is_empty() {
            Vec::new()
        } else {
            vec![Part {
                op: Op::Same,
                text: before.to_owned(),
            }]
        };
    }
    let a = tokens(before);
    let b = tokens(after);
    if a.len() > MAX_TOKENS || b.len() > MAX_TOKENS {
        return whole(before, after);
    }

    // The common head and tail are kept out of the search.
    let mut head = 0;
    while head < a.len() && head < b.len() && a[head] == b[head] {
        head += 1;
    }
    let mut tail = 0;
    while tail < a.len() - head
        && tail < b.len() - head
        && a[a.len() - 1 - tail] == b[b.len() - 1 - tail]
    {
        tail += 1;
    }
    let x = &a[head..a.len() - tail];
    let y = &b[head..b.len() - tail];

    let Some(steps) = middle(x, y) else {
        return whole(before, after);
    };
    let mut parts = Vec::new();
    if head > 0 {
        push(&mut parts, Op::Same, a[..head].concat());
    }
    for part in tidy(&steps) {
        push(&mut parts, part.op, part.text);
    }
    if tail > 0 {
        push(&mut parts, Op::Same, a[a.len() - tail..].concat());
    }
    parts
}

/// Words and runs of whitespace, the pieces a diff moves.
fn tokens(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut space = false;
    for (index, c) in text.char_indices() {
        let this = c.is_whitespace();
        if index > start && this != space {
            out.push(&text[start..index]);
            start = index;
        }
        space = this;
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}

/// Adds a part, merging it with the previous one of the same kind.
fn push(parts: &mut Vec<Part>, op: Op, text: String) {
    if let Some(last) = parts.last_mut()
        && last.op == op
    {
        last.text.push_str(&text);
    } else {
        parts.push(Part { op, text });
    }
}

/// One removal and one addition, when a diff is not worth computing.
fn whole(before: &str, after: &str) -> Vec<Part> {
    let mut parts = Vec::new();
    if !before.is_empty() {
        parts.push(Part {
            op: Op::Removed,
            text: before.to_owned(),
        });
    }
    if !after.is_empty() {
        parts.push(Part {
            op: Op::Added,
            text: after.to_owned(),
        });
    }
    parts
}

/// Whitespace kept between two changes joins them, and each change reads as all that was removed,
/// then all that was added: «-b c +x b» rather than «-b +x · -c +b».
fn tidy(steps: &[Part]) -> Vec<Part> {
    let mut parts = Vec::new();
    let mut removed = String::new();
    let mut added = String::new();
    for (index, step) in steps.iter().enumerate() {
        let inside = step.op == Op::Same
            && step.text.chars().all(char::is_whitespace)
            && (!removed.is_empty() || !added.is_empty())
            && steps.get(index + 1).is_some_and(|next| next.op != Op::Same);
        if step.op == Op::Removed || inside {
            removed.push_str(&step.text);
        }
        if step.op == Op::Added || inside {
            added.push_str(&step.text);
        }
        if step.op == Op::Same && !inside {
            flush(&mut parts, &mut removed, &mut added);
            parts.push(Part {
                op: Op::Same,
                text: step.text.clone(),
            });
        }
    }
    flush(&mut parts, &mut removed, &mut added);
    parts
}

fn flush(parts: &mut Vec<Part>, removed: &mut String, added: &mut String) {
    if !removed.is_empty() {
        parts.push(Part {
            op: Op::Removed,
            text: std::mem::take(removed),
        });
    }
    if !added.is_empty() {
        parts.push(Part {
            op: Op::Added,
            text: std::mem::take(added),
        });
    }
}

/// Myers' greedy forward search; each round keeps its furthest points (k from -d to d) for the
/// way back. `None` past `MAX_EDITS`, so memory stays small (about `MAX_EDITS`² numbers).
fn middle(a: &[&str], b: &[&str]) -> Option<Vec<Part>> {
    let n = a.len();
    let m = b.len();
    if n == 0 {
        return Some(
            b.iter()
                .map(|text| Part {
                    op: Op::Added,
                    text: (*text).to_owned(),
                })
                .collect(),
        );
    }
    if m == 0 {
        return Some(
            a.iter()
                .map(|text| Part {
                    op: Op::Removed,
                    text: (*text).to_owned(),
                })
                .collect(),
        );
    }
    let at = |round: &[i32], d: i32, k: i32| -> i32 {
        round.get((k + d) as usize).copied().unwrap_or(0)
    };
    let mut rounds: Vec<Vec<i32>> = Vec::new();
    let max_d = (n + m).min(MAX_EDITS) as i32;
    let mut d = 0;
    while d <= max_d {
        // The previous round is only read while this one is built.
        let previous: &[i32] = rounds.last().map(Vec::as_slice).unwrap_or(&[]);
        let mut current = vec![0i32; (2 * d + 1) as usize];
        let mut k = -d;
        while k <= d {
            let mut i = if d == 0 {
                0
            } else if k == -d || (k != d && at(previous, d - 1, k - 1) < at(previous, d - 1, k + 1))
            {
                at(previous, d - 1, k + 1)
            } else {
                at(previous, d - 1, k - 1) + 1
            };
            let mut j = i - k;
            while (i as usize) < n && (j as usize) < m && a[i as usize] == b[j as usize] {
                i += 1;
                j += 1;
            }
            current[(k + d) as usize] = i;
            if i as usize >= n && j as usize >= m {
                rounds.push(current);
                return Some(back(a, b, &rounds));
            }
            k += 2;
        }
        rounds.push(current);
        d += 1;
    }
    None
}

/// The path from the end back to the start: each round is one removal or addition after a run of
/// equal tokens.
fn back(a: &[&str], b: &[&str], rounds: &[Vec<i32>]) -> Vec<Part> {
    let mut steps: Vec<Part> = Vec::new();
    let mut i = a.len() as i32;
    let mut j = b.len() as i32;
    for d in (1..rounds.len() as i32).rev() {
        let previous = &rounds[(d - 1) as usize];
        let at = |k: i32| -> i32 { previous.get((k + d - 1) as usize).copied().unwrap_or(0) };
        let k = i - j;
        let down = k == -d || (k != d && at(k - 1) < at(k + 1));
        let previous_k = if down { k + 1 } else { k - 1 };
        let previous_i = at(previous_k);
        let previous_j = previous_i - previous_k;
        while i > previous_i && j > previous_j {
            i -= 1;
            j -= 1;
            steps.push(Part {
                op: Op::Same,
                text: a[i as usize].to_owned(),
            });
        }
        if down {
            j -= 1;
            steps.push(Part {
                op: Op::Added,
                text: b[j as usize].to_owned(),
            });
        } else {
            i -= 1;
            steps.push(Part {
                op: Op::Removed,
                text: a[i as usize].to_owned(),
            });
        }
    }
    while i > 0 && j > 0 {
        i -= 1;
        j -= 1;
        steps.push(Part {
            op: Op::Same,
            text: a[i as usize].to_owned(),
        });
    }
    steps.reverse();
    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Deserialize)]
    struct Case {
        name: String,
        before: String,
        after: String,
        parts: Vec<(String, String)>,
    }

    #[test]
    fn shared_cases_match_the_web() {
        let cases: Vec<Case> = serde_json::from_str(include_str!("diff_cases.json")).unwrap();
        assert!(!cases.is_empty(), "the shared cases exist");
        for case in &cases {
            let parts = diff_words(&case.before, &case.after);
            let shown: Vec<(&str, &str)> = parts
                .iter()
                .map(|part| (part.op.key(), part.text.as_str()))
                .collect();
            let expected: Vec<(&str, &str)> = case
                .parts
                .iter()
                .map(|(op, text)| (Op::parse(op).unwrap_or(Op::Same).key(), text.as_str()))
                .collect();
            assert_eq!(shown, expected, "{}", case.name);
        }
    }

    #[test]
    fn parts_put_the_texts_back_together() {
        for (before, after) in [
            (
                "The login button does nothing",
                "The submit button does nothing",
            ),
            ("один два три", "один три"),
            ("", "новый текст"),
            ("same", "same"),
            ("a b c d", "a x b d"),
        ] {
            let parts = diff_words(before, after);
            let removed: String = parts
                .iter()
                .filter(|p| p.op != Op::Added)
                .map(|p| p.text.as_str())
                .collect();
            let added: String = parts
                .iter()
                .filter(|p| p.op != Op::Removed)
                .map(|p| p.text.as_str())
                .collect();
            assert_eq!(removed, before, "{before:?} → {after:?}");
            assert_eq!(added, after, "{before:?} → {after:?}");
        }
    }

    #[test]
    fn very_different_texts_are_shown_whole() {
        let before = "слово ".repeat(3_000);
        let after = "иное ".repeat(3_000);
        let parts = diff_words(&before, &after);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].op, Op::Removed);
        assert_eq!(parts[1].op, Op::Added);
    }
}
