//! The completion model (step 2.1 §5, D-061): one shape of suggestion for the command dialog and
//! the query bar. This module owns what is pure — where the caret sits in the grammar, matching,
//! ranking, quoting and ranges; the service assembles the candidates (names, rights, presence,
//! resolved dates) and passes them through [`shape`]. Ranges are bytes into the text; the API
//! layer converts them to UTF-16.

use serde::{Deserialize, Serialize};

use crate::command::Mode;
use crate::lex::{Token, TokenKind, lex, quote};
use crate::print::is_bare_word;
use crate::syntax::Span;
use crate::words::{self, Keyword, fold};

/// How many suggestions one call answers — long enough that the empty query bar's attributes
/// and keywords all fit, bounded so one answer stays light.
pub const MAX: usize = 100;

/// The longest name run the parser matches ([`crate::command::MAX_RUN_WORDS`]).
const RUN: usize = 8;

/// What a suggestion is about; the web labels and orders by it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    /// A command word: `tag`, `move to`…
    Command,
    /// A field name.
    Attribute,
    /// A value of a set field; `description` names the field.
    Value,
    /// A link direction name.
    Link,
    /// A tag name.
    Tag,
    /// A project; `description` is its key.
    Project,
    /// An account; `own` marks the actor's.
    User,
    /// A group of people.
    Group,
    Board,
    Sprint,
    /// `me` and the other words any field may take.
    Special,
    /// A field's empty value; `description` names the field.
    Empty,
    /// A relative date word; `resolved` is its concrete value.
    Date,
    /// An issue after a link word; `description` is its summary.
    Issue,
    /// A command from the actor's history (CMD-030, step 2.1c).
    Recent,
    /// A query keyword in a value position (`resolved`, `sort`…).
    Keyword,
    /// A key of the sort spec.
    SortKey,
    /// `asc`/`desc` in the sort spec.
    SortDirection,
}

/// One thing the service offers: a display name with everything the list shows. Candidates come
/// in priority order within their group; [`shape`] does the rest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candidate {
    /// The words to insert, as they read (unquoted).
    pub insert: String,
    /// [`fold`] of `insert`, for matching.
    pub folded: String,
    pub group: Group,
    /// Plain data the label builds on: the field of a value, a project's key, an issue's
    /// summary. Empty when the group says it all.
    pub description: String,
    /// Priority tier (QRY-057): lower ranks first — values outside the projects the context
    /// implies rank lower than ones inside.
    pub tier: u8,
    /// The value is already on every target issue (commands, §1); the web marks it.
    pub present: bool,
    /// A relative date's concrete value in the caller's zone, as text.
    pub resolved: Option<String>,
    /// The actor's own account.
    pub own: bool,
}

impl Candidate {
    /// The least a candidate needs; the extras default off.
    pub fn new(insert: impl Into<String>, group: Group) -> Self {
        let insert = insert.into();
        let folded = fold(&insert);
        Self {
            insert,
            folded,
            group,
            description: String::new(),
            tier: 0,
            present: false,
            resolved: None,
            own: false,
        }
    }

    pub fn described(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }
}

/// One ready suggestion: what to insert, where, and how it matched.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Suggestion {
    /// The insert, quoted where the plain word would not read back the same (auto-quoting).
    pub text: String,
    pub description: String,
    pub group: Group,
    /// Replaces this range of the text.
    pub replace: Span,
    /// The matched part inside `text`.
    pub matched: Span,
    /// Where the caret lands in the new text, in bytes.
    pub caret: usize,
    /// What follows the insert: a space, or the separator the position calls for.
    pub suffix: String,
    pub present: bool,
    pub resolved: Option<String>,
    pub own: bool,
}

/// What sits at the caret: the words being typed and the range they replace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Place {
    /// The typed words up to the caret, folded; the empty string when none.
    pub prefix: String,
    /// The same, as typed (for the matched range).
    pub raw: String,
    /// The range the insert replaces: the whole run being typed (so a multi-word insert can
    /// take a partial multi-word run with it), or an empty range at the caret.
    pub replace: Span,
}

/// Where the caret sits in the command grammar, from the words before it. The service turns
/// each case into its candidates (plan §5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandContext {
    /// A command may begin: everything the dictionary names is offered.
    Start,
    /// After a command word that takes a name: `tag`, `untag`, `star`, `unstar`, `watcher`,
    /// `visible_to`, `move_to`, `board`, `project`.
    Word { key: &'static str },
    /// After `add` or `remove`.
    AddRemove { remove: bool },
    /// After a field name — plain, or under `add`/`remove` (`multi`).
    Attribute { folded: String, multi: Option<Mode> },
    /// After `board <name>`: the sprints of that board.
    BoardSprints { board: String },
    /// After a link name: issues.
    LinkTarget,
}

/// Where the caret sits in the query grammar (QRY-056).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueryContext {
    /// A name position: attributes and keywords.
    Names,
    /// A value position after an attribute — a custom field (`folded`), or a built-in
    /// (`builtin` carries its key: `tag`, `project`, `reporter`, `created`…).
    Values {
        folded: String,
        builtin: Option<&'static str>,
    },
    /// Inside the sort spec, a key expected.
    SortKeys,
    /// Inside the sort spec, after a key: `asc`/`desc`.
    SortDirections,
}

/// The caret's place in a command line, with what the words before it make (plan §5). The
/// context comes from the words the command has already consumed — those before the caret's
/// run, plus the syntax words at the run's head the parser would take (`add`, the attribute,
/// the link name, a board's name after `board`): what the run replaces is what follows them.
pub fn command_site(
    text: &str,
    caret: usize,
    dict: &crate::command::Dictionary,
) -> (CommandContext, Place) {
    let caret = caret.min(text.len());
    let tokens = lex(text);
    let Some((run, current)) = run_at(&tokens, caret) else {
        return (
            context_of(&words_before(text, Span::new(caret, caret), &tokens), dict),
            empty(caret),
        );
    };
    let mut consumed = words_before(text, run.span, &tokens);
    if let Some(token) = current.filter(|t| matches!(t.kind, TokenKind::Quoted { .. })) {
        // A quoted atom replaces whole; its typed inner text is the prefix.
        return (
            context_of(&consumed, dict),
            quoted_place(text, token, caret),
        );
    }
    let words = run.words(text);
    // Consume the syntax heads the parser would take before the name being typed: the command
    // word (with the name it may take), the link name, the attribute. A command word later in
    // the run starts a fresh command and moves the frontier there, so a second command typed
    // on the line completes on its own words.
    let mut at = 0usize;
    loop {
        if let Some(n) = longest_command(&words[at..], 2) {
            let key = word_key(&words[at..at + n].join(" ")).unwrap_or_default();
            at += n;
            if let Some(class) = name_class(key, dict)
                && let Some(m) = longest_full(&words[at..], class)
                && at + m < words.len()
            {
                at += m;
            }
        } else if let Some(n) = longest_full(&words[at..], &dict.links) {
            at += n;
        } else if let Some(n) = longest_full(&words[at..], &dict.attributes) {
            at += n;
        }
        let fresh = (1..words.len().saturating_sub(at))
            .find(|&j| longest_command(&words[at + j..], 2).is_some());
        match fresh {
            Some(j) => at += j,
            None => break,
        }
    }
    consumed.extend(words[..at].iter().cloned());
    let replace = if at == 0 {
        run.span
    } else if at == words.len() {
        Span::new(run.span.end, run.span.end)
    } else {
        Span::new(tokens[run.first + at].span.start, run.span.end)
    };
    let typed = text.get(replace.start..caret).unwrap_or("");
    (
        context_of(&consumed, dict),
        Place {
            prefix: fold(typed),
            raw: typed.to_owned(),
            replace,
        },
    )
}

/// The command context alone.
pub fn command_context(
    text: &str,
    caret: usize,
    dict: &crate::command::Dictionary,
) -> CommandContext {
    command_site(text, caret, dict).0
}

/// The caret's place in a query (QRY-056). The run the insert replaces is bounded by the
/// query's punctuation: a `:` or comparison ends a name, a comma ends a value.
pub fn query_site(
    text: &str,
    caret: usize,
    dict: &crate::command::Dictionary,
) -> (QueryContext, Place) {
    let caret = caret.min(text.len());
    let tokens = lex(text);
    let place = match run_at(&tokens, caret) {
        Some((_run, Some(token))) if matches!(token.kind, TokenKind::Quoted { .. }) => {
            quoted_place(text, token, caret)
        }
        Some((run, _)) => {
            let typed = text.get(run.span.start..caret).unwrap_or("");
            Place {
                prefix: fold(typed),
                raw: typed.to_owned(),
                replace: run.span,
            }
        }
        None => empty(caret),
    };
    let before: Vec<&Token> = tokens
        .iter()
        .filter(|t| t.span.end <= place.replace.start)
        .collect();

    // The sort spec: `sort:` then only keys, directions and commas up to the caret.
    if let Some(i) = before
        .iter()
        .rposition(|t| t.kind == TokenKind::Word && sort_word(text, t))
        && before
            .get(i + 1)
            .is_some_and(|t| t.kind == TokenKind::Colon)
        && before[i + 2..]
            .iter()
            .all(|t| matches!(t.kind, TokenKind::Word | TokenKind::Comma))
    {
        return match before.last().map(|t| t.kind) {
            Some(TokenKind::Word) => (QueryContext::SortDirections, place),
            _ => (QueryContext::SortKeys, place),
        };
    }

    // A value position: the nearest `:` or comparison, only value-shaped tokens between it and
    // the caret, and a name before it.
    if let Some(op) = before.iter().rposition(|t| {
        matches!(
            t.kind,
            TokenKind::Colon | TokenKind::Lt | TokenKind::Le | TokenKind::Gt | TokenKind::Ge
        )
    }) {
        let name = before
            .get(op.wrapping_sub(1))
            .filter(|t| matches!(t.kind, TokenKind::Word | TokenKind::Quoted { .. }));
        let values_between = before[op + 1..].iter().all(|t| {
            matches!(
                t.kind,
                TokenKind::Word
                    | TokenKind::Quoted { .. }
                    | TokenKind::Comma
                    | TokenKind::Bang
                    | TokenKind::DotDot
            ) && !is_keyword(text, t)
        });
        if let (true, Some(name)) = (values_between, name) {
            let folded = fold(slice(text, name.span));
            if dict.attributes.contains(&folded) {
                return (
                    QueryContext::Values {
                        folded,
                        builtin: None,
                    },
                    place,
                );
            }
            if let Some(word) = words::keyword(&folded, words::NAME) {
                return (
                    QueryContext::Values {
                        folded,
                        builtin: Some(word.key),
                    },
                    place,
                );
            }
        }
    }
    (QueryContext::Names, place)
}

/// The query context alone.
pub fn query_context(text: &str, caret: usize, dict: &crate::command::Dictionary) -> QueryContext {
    query_site(text, caret, dict).0
}

/// Whether a token is the `sort` keyword.
fn sort_word(text: &str, token: &Token) -> bool {
    token.kind == TokenKind::Word
        && words::keyword(&fold(slice(text, token.span)), &[words::Kind::Sort])
            .is_some_and(|w| w.key == "sort")
}

/// Whether a token is an operator or the sort word: a value list never crosses them.
fn is_keyword(text: &str, token: &Token) -> bool {
    if token.kind != TokenKind::Word {
        return false;
    }
    let folded = fold(slice(text, token.span));
    words::is_operator(&folded) || sort_word(text, token)
}

/// Matches the candidates against the place and shapes the suggestions: prefix matches first,
/// then substring matches; a lower tier beats both (QRY-057); the service's order breaks ties.
/// Multi-word inserts arrive quoted, and so do words that would read back as keywords. At most
/// [`MAX`] suggestions, without duplicates.
pub fn shape(place: &Place, candidates: &[Candidate]) -> Vec<Suggestion> {
    let prefix = place.prefix.as_str();
    let mut matched: Vec<(u8, u8, usize, &Candidate)> = candidates
        .iter()
        .enumerate()
        .filter_map(|(i, candidate)| {
            if prefix.is_empty() || candidate.folded.starts_with(prefix) {
                Some((candidate.tier, 0, i, candidate))
            } else if candidate.folded.contains(prefix) {
                Some((candidate.tier, 1, i, candidate))
            } else {
                None
            }
        })
        .collect();
    matched.sort_by_key(|&(tier, kind, order, _)| (tier, kind, order));

    let mut out: Vec<Suggestion> = Vec::new();
    let mut seen: Vec<(Group, String, String)> = Vec::new();
    for (_, _, _, candidate) in matched {
        if out.len() == MAX {
            break;
        }
        let key = (
            candidate.group,
            candidate.folded.clone(),
            candidate.description.clone(),
        );
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        // Only names insert quoted: the parser reads them from a quoted literal. Command
        // words, attributes, link names and the syntax's words parse as bare word runs — a
        // quote there would break the line.
        let name_position = matches!(
            candidate.group,
            Group::Value
                | Group::Tag
                | Group::Project
                | Group::User
                | Group::Group
                | Group::Board
                | Group::Sprint
                | Group::Special
                | Group::Empty
                | Group::Issue
        );
        let quoted = name_position
            && (!is_bare_word(&candidate.insert)
                || words::command_word(&candidate.folded).is_some());
        let text = if quoted {
            quote(&candidate.insert)
        } else {
            candidate.insert.clone()
        };
        let suffix = " ".to_owned();
        let inside = if quoted { 1 } else { 0 };
        let matched_span = match_span(&candidate.insert, &place.raw)
            .map(|(start, end)| Span::new(start + inside, end + inside))
            .unwrap_or(Span::new(inside, inside));
        out.push(Suggestion {
            caret: place.replace.start + text.len() + suffix.len(),
            replace: place.replace,
            matched: matched_span,
            text,
            description: candidate.description.clone(),
            group: candidate.group,
            suffix,
            present: candidate.present,
            resolved: candidate.resolved.clone(),
            own: candidate.own,
        });
    }
    out
}

/// Where the typed prefix sits inside the insert, case-insensitively, in bytes of the insert.
fn match_span(insert: &str, raw: &str) -> Option<(usize, usize)> {
    if raw.is_empty() {
        return Some((0, 0));
    }
    let typed: Vec<char> = raw.chars().flat_map(char::to_lowercase).collect();
    // The insert's lowercase characters, each remembering the char (and its byte) it came from.
    let chars: Vec<(usize, char)> = insert.char_indices().collect();
    let mut flat: Vec<char> = Vec::new();
    let mut marks: Vec<usize> = Vec::new();
    for (i, (_, c)) in chars.iter().enumerate() {
        for lower in c.to_lowercase() {
            flat.push(lower);
            marks.push(i);
        }
    }
    let at = (0..=flat.len().checked_sub(typed.len())?)
        .find(|&start| flat[start..start + typed.len()] == typed[..])?;
    let first = marks[at];
    let last = marks[at + typed.len() - 1];
    Some((chars[first].0, chars[last].0 + chars[last].1.len_utf8()))
}

/// A run of consecutive plain words — what a multi-word name may be typed as.
struct Run {
    /// The whole run, from its first word's start to its last word's end.
    span: Span,
    /// Indexes of the first and last word token in the token list; `usize::MAX` for none.
    first: usize,
    last: usize,
}

impl Run {
    /// The run's folded words.
    fn words(&self, text: &str) -> Vec<String> {
        if self.first == usize::MAX {
            return Vec::new();
        }
        lex(text)[self.first..=self.last]
            .iter()
            .map(|t| fold(slice(text, t.span)))
            .collect()
    }
}

/// A quoted atom under the caret: it replaces whole, its typed inner text is the prefix.
fn quoted_place(text: &str, token: &Token, caret: usize) -> Place {
    let TokenKind::Quoted { closed } = token.kind else {
        return empty(caret);
    };
    let raw = slice(text, token.span);
    let inner = raw.get(1..).unwrap_or(raw);
    let inner = if closed && !inner.is_empty() {
        inner.get(..inner.len() - 1).unwrap_or(inner)
    } else {
        inner
    };
    let cut = caret.saturating_sub(1).saturating_sub(token.span.start);
    let mut end = cut.min(inner.len());
    while end > 0 && !inner.is_char_boundary(end) {
        end -= 1;
    }
    let partial = inner.get(..end).unwrap_or("");
    Place {
        prefix: fold(partial),
        raw: partial.to_owned(),
        replace: token.span,
    }
}

/// The word run the caret sits in, and the token at the caret (a quoted atom replaces whole).
/// `None` when nothing is being typed — the caret sits past every word after a space.
fn run_at(tokens: &[Token], caret: usize) -> Option<(Run, Option<&Token>)> {
    let i = tokens.iter().rposition(|t| {
        t.span.start <= caret && matches!(t.kind, TokenKind::Word | TokenKind::Quoted { .. })
    })?;
    let token = &tokens[i];
    if token.span.end < caret {
        return None;
    }
    if matches!(token.kind, TokenKind::Quoted { .. }) {
        return Some((
            Run {
                span: token.span,
                first: usize::MAX,
                last: usize::MAX,
            },
            Some(token),
        ));
    }
    let mut first = i;
    while first > 0 && tokens[first - 1].kind == TokenKind::Word {
        first -= 1;
    }
    Some((
        Run {
            span: Span::new(tokens[first].span.start, token.span.end),
            first,
            last: i,
        },
        Some(token),
    ))
}

/// The longest run of the words (up to `max`) that names a command word.
fn longest_command(words: &[String], max: usize) -> Option<usize> {
    (1..=max.min(words.len()))
        .rev()
        .find(|&n| word_key(&words[..n].join(" ")).is_some())
}

/// The longest run of the words that a dictionary class knows in full.
fn longest_full(words: &[String], names: &[String]) -> Option<usize> {
    (1..=RUN.min(words.len()))
        .rev()
        .find(|&n| names.contains(&words[..n].join(" ")))
}

/// The name class a command word takes after it, when it takes one at its head.
fn name_class<'a>(key: &str, dict: &'a crate::command::Dictionary) -> Option<&'a Vec<String>> {
    match key {
        "tag" | "untag" => Some(&dict.tags),
        "board" => Some(&dict.boards),
        "move_to" | "project" => Some(&dict.projects),
        "visible_to" => None, // users or groups: not consumed, the party is the run
        "star" | "unstar" | "watcher" => None,
        _ => None,
    }
}

fn empty(caret: usize) -> Place {
    Place {
        prefix: String::new(),
        raw: String::new(),
        replace: Span::new(caret, caret),
    }
}

fn slice(text: &str, span: Span) -> &str {
    text.get(span.start..span.end).unwrap_or("")
}

/// The folded words immediately before `end`, oldest first, stopping at anything that is not a
/// plain word (a quoted name, a colon, the start of the text).
fn words_before(text: &str, end: Span, tokens: &[Token]) -> Vec<String> {
    let mut out = Vec::new();
    for token in tokens.iter().rev() {
        if token.span.end > end.start {
            continue;
        }
        if token.kind != TokenKind::Word {
            break;
        }
        out.push(fold(slice(text, token.span)));
        if out.len() == RUN {
            break;
        }
    }
    out.reverse();
    out
}

/// Classifies the words before the caret into a command context.
fn context_of(before: &[String], dict: &crate::command::Dictionary) -> CommandContext {
    let joins = |from: usize| before[from..].join(" ");
    let run = |n: usize| before.len().checked_sub(n).map(joins);

    // `add|remove <link name>`: the issue the link names.
    for n in (1..RUN).rev() {
        let Some(from) = before.len().checked_sub(n + 1) else {
            continue;
        };
        if matches!(word_key(&before[from]), Some("add") | Some("remove"))
            && run(n).is_some_and(|name| dict.links.contains(&name))
        {
            return CommandContext::LinkTarget;
        }
    }
    // `board <name>`: the sprints of that board.
    if before.len() >= 2 && word_key(&before[0]) == Some("board") && dict.boards.contains(&joins(1))
    {
        return CommandContext::BoardSprints { board: joins(1) };
    }
    // `add|remove tag|board|visible to`: the sub-command's own candidates.
    if before.len() >= 2 {
        let head = word_key(&before[before.len() - 2]);
        let tail = word_key(&before[before.len() - 1]);
        if matches!(head, Some("add") | Some("remove"))
            && let Some(key @ ("tag" | "board" | "visible_to")) = tail
        {
            return CommandContext::Word { key };
        }
    }
    match before.last().and_then(|w| word_key(w)) {
        Some("add") => return CommandContext::AddRemove { remove: false },
        Some("remove") => return CommandContext::AddRemove { remove: true },
        _ => {}
    }
    // A command word that expects a name right at the caret.
    for n in (1..=2).rev() {
        if let Some(words) = run(n)
            && let Some(key) = word_key(&words)
            && expects_name(key)
        {
            return CommandContext::Word { key };
        }
    }
    // A field name: longest run first, with `add`/`remove` before it when present.
    for n in (1..=RUN).rev() {
        let Some(name) = run(n) else {
            continue;
        };
        if !dict.attributes.contains(&name) {
            continue;
        }
        let multi = before
            .len()
            .checked_sub(n + 1)
            .and_then(|from| word_key(&before[from]))
            .and_then(|key| match key {
                "add" => Some(Mode::Add),
                "remove" => Some(Mode::Remove),
                _ => None,
            });
        return CommandContext::Attribute {
            folded: name,
            multi,
        };
    }
    // A link name alone: the issue it names.
    for n in (1..=RUN).rev() {
        if run(n).is_some_and(|name| dict.links.contains(&name)) {
            return CommandContext::LinkTarget;
        }
    }
    CommandContext::Start
}

/// The command word a folded text names, if any ([`words::command_word`]).
fn word_key(folded: &str) -> Option<&'static str> {
    words::command_word(folded).map(|w| w.key)
}

/// The command words that take a name after them; the rest (`vote`, `comment`, `delete`…) end
/// at themselves, and what follows starts fresh.
fn expects_name(key: &str) -> bool {
    matches!(
        key,
        "tag"
            | "untag"
            | "star"
            | "unstar"
            | "watcher"
            | "visible_to"
            | "move_to"
            | "project"
            | "board"
    )
}

/// The forms a keyword is offered in: its first English form and every Russian one; secondary
/// English aliases stay accepted but are never suggested (QRY-057).
pub fn offered_forms(word: &Keyword) -> Vec<String> {
    let mut out = vec![word.english.first().copied().unwrap_or_default().to_owned()];
    for form in word.russian {
        if !out.iter().any(|offered| offered == form) {
            out.push((*form).to_owned());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn dict() -> crate::command::Dictionary {
        crate::command::Dictionary {
            attributes: vec!["state".into(), "fix versions".into(), "assignee".into()],
            values: HashMap::from([(
                "state".to_owned(),
                vec!["in progress".into(), "fixed".into()],
            )]),
            links: vec!["relates to".into(), "blocks".into()],
            projects: vec!["mars".into()],
            tags: vec!["web".into()],
            users: vec!["bob".into()],
            groups: vec!["team".into()],
            boards: vec!["demo".into()],
            sprints: vec!["sprint 1".into()],
            special: vec!["me".into(), "today".into()],
        }
    }

    #[test]
    fn cmd_029_the_run_being_typed() {
        // A value after its attribute replaces only its own words.
        let (_, p) = command_site("state in pr", 11, &dict());
        assert_eq!((p.replace.start, p.replace.end), (6, 11));
        assert_eq!(p.prefix, "in pr");
        // A partial multi-word attribute is the run; the insert takes it whole.
        let (_, p) = command_site("add fix ver", 11, &dict());
        assert_eq!((p.replace.start, p.replace.end), (4, 11));
        assert_eq!(p.prefix, "fix ver");
        // A complete attribute leaves nothing typed yet.
        let (_, p) = command_site("state", 5, &dict());
        assert_eq!((p.replace.start, p.replace.end), (5, 5));
        assert_eq!(p.prefix, "");
        // A second command on the line completes on its own words.
        let (context, p) = command_site("state Fixed tag web", 19, &dict());
        assert_eq!(context, CommandContext::Word { key: "tag" });
        assert_eq!((p.replace.start, p.replace.end), (16, 19));
        assert_eq!(p.prefix, "web");
        // Nothing being typed: an empty range at the caret.
        let (_, p) = command_site("state ", 6, &dict());
        assert_eq!((p.replace.start, p.replace.end), (6, 6));
        assert_eq!(p.prefix, "");
        // The caret before a word still replaces that word.
        let (_, p) = command_site("state Fixed", 6, &dict());
        assert_eq!((p.replace.start, p.replace.end), (6, 11));
        assert_eq!(p.prefix, "");
        // A quoted atom replaces whole, its typed part the prefix.
        let (_, p) = command_site("state \"In Pr", 12, &dict());
        assert_eq!((p.replace.start, p.replace.end), (6, 12));
        assert_eq!(p.prefix, "in pr");
    }

    #[test]
    fn cmd_029_command_contexts() {
        let of = |text: &str, caret: usize| command_context(text, caret, &dict());
        assert_eq!(of("", 0), CommandContext::Start);
        assert_eq!(of("state Fixed ", 12), CommandContext::Start);
        assert_eq!(of("tag ", 4), CommandContext::Word { key: "tag" });
        assert_eq!(
            of("перенести в ", 22),
            CommandContext::Word { key: "move_to" }
        );
        assert_eq!(of("project ", 8), CommandContext::Word { key: "project" });
        assert_eq!(of("add ", 4), CommandContext::AddRemove { remove: false });
        assert_eq!(of("remove ", 7), CommandContext::AddRemove { remove: true });
        assert_eq!(of("add tag ", 8), CommandContext::Word { key: "tag" });
        assert_eq!(
            of("state ", 6),
            CommandContext::Attribute {
                folded: "state".into(),
                multi: None
            }
        );
        assert_eq!(
            of("add Fix versions ", 17),
            CommandContext::Attribute {
                folded: "fix versions".into(),
                multi: Some(Mode::Add)
            }
        );
        assert_eq!(
            of("board demo ", 11),
            CommandContext::BoardSprints {
                board: "demo".into()
            }
        );
        assert_eq!(of("relates to ", 11), CommandContext::LinkTarget);
        assert_eq!(of("remove relates to ", 18), CommandContext::LinkTarget);
        // A command word that ends at itself: what follows starts fresh.
        assert_eq!(of("vote ", 5), CommandContext::Start);
    }

    #[test]
    fn cmd_029_shape_matches_ranks_and_quotes() {
        let place = Place {
            prefix: "in".into(),
            raw: "in".into(),
            replace: Span::new(6, 8),
        };
        let suggestions = shape(
            &place,
            &[
                Candidate::new("Beginning", Group::Value).described("State"),
                Candidate::new("In Progress", Group::Value).described("State"),
                Candidate::new("Fixed", Group::Value).described("State"),
                Candidate::new("in progress", Group::Recent),
            ],
        );
        // Prefix first, substring second, the non-match out; the recent repeats only itself.
        assert_eq!(suggestions.len(), 3);
        assert_eq!(suggestions[0].text, "\"In Progress\"");
        assert_eq!(suggestions[0].matched, Span::new(1, 3));
        assert_eq!(suggestions[0].replace, Span::new(6, 8));
        assert_eq!(suggestions[0].caret, 6 + "\"In Progress\"".len() + 1);
        assert_eq!(suggestions[1].group, Group::Recent);
        assert_eq!(suggestions[2].text, "Beginning");
        assert_eq!(suggestions[2].matched.start, 3);

        // A word that would read back as a keyword is inserted quoted.
        let place = Place {
            prefix: String::new(),
            raw: String::new(),
            replace: Span::new(0, 0),
        };
        let suggestions = shape(&place, &[Candidate::new("star", Group::Value)]);
        assert_eq!(suggestions[0].text, "\"star\"");

        // A tier above a match kind (QRY-057): out-of-scope never beats in-scope.
        let suggestions = shape(
            &place,
            &[
                Candidate {
                    tier: 1,
                    ..Candidate::new("Far", Group::Value)
                },
                Candidate {
                    tier: 0,
                    ..Candidate::new("Near", Group::Value)
                },
            ],
        );
        assert_eq!(suggestions[0].text, "Near");
    }

    #[test]
    fn qry_056_query_contexts() {
        let dict = dict();
        let of = |text: &str, caret: usize| query_context(text, caret, &dict);

        assert_eq!(of("", 0), QueryContext::Names);
        assert_eq!(
            of("state: ", 7),
            QueryContext::Values {
                folded: "state".into(),
                builtin: None
            }
        );
        assert_eq!(
            of("state: Fi", 9),
            QueryContext::Values {
                folded: "state".into(),
                builtin: None
            }
        );
        assert_eq!(
            of("state: Fixed, ", 14),
            QueryContext::Values {
                folded: "state".into(),
                builtin: None
            }
        );
        assert_eq!(
            of("tag: ", 5),
            QueryContext::Values {
                folded: "tag".into(),
                builtin: Some("tag")
            }
        );
        assert_eq!(
            of("reporter: ", 10),
            QueryContext::Values {
                folded: "reporter".into(),
                builtin: Some("reporter")
            }
        );
        // An operator between values ends the value position.
        assert_eq!(of("state: Fixed and ", 17), QueryContext::Names);
        assert_eq!(of("sort: ", 6), QueryContext::SortKeys);
        assert_eq!(of("sort: updated ", 14), QueryContext::SortDirections);
        assert_eq!(of("sort: updated, ", 15), QueryContext::SortKeys);
        assert_eq!(of("sort: upd", 9), QueryContext::SortKeys);
    }
}
