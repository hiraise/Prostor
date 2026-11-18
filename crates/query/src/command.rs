//! The command language (step 2.1, plan §3): what the command dialog and the bulk flow apply
//! to the selected issues. The parser never fails and does no I/O: the service injects the
//! [`Dictionary`] of folded names, an unknown word becomes an [`Command::Error`] node with a
//! problem over its span, and parsing resumes at the next word. Resolving to a plan of typed
//! operations happens in `service::commands` (plan §4.2); the canonical text this module
//! writes is what the dialog's history stores.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::lex::{Token, TokenKind, lex, quote, unescape};
use crate::print::is_bare_word;
use crate::syntax::{Atom, Span};
use crate::words::{self, fold};

/// Longest run of unquoted words tried against a dictionary class; longer names must be quoted.
pub const MAX_RUN_WORDS: usize = 8;

/// The folded names the parser matches against, assembled by the service for the target
/// issues' projects (plan §4.1). Everything is folded ([`fold`]); one assembly per call.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Dictionary {
    /// Field names and their aliases.
    pub attributes: Vec<String>,
    /// Value names per folded attribute name; aliases of one field share its list.
    pub values: HashMap<String, Vec<String>>,
    /// Link names, both directions.
    pub links: Vec<String>,
    pub projects: Vec<String>,
    pub tags: Vec<String>,
    pub users: Vec<String>,
    pub groups: Vec<String>,
    pub boards: Vec<String>,
    pub sprints: Vec<String>,
    /// Special value names beyond the vocabulary's (`me`, dates…), where the projects have any.
    pub special: Vec<String>,
}

/// Every problem the language reports. The parser raises the first two; resolution adds the
/// rest over the same spans (`service::commands`, plan §4.2); the API and the web translate
/// each code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandCode {
    /// A word that starts no command and matches no name of the dictionary.
    CmdUnknownWord,
    /// A command word or attribute whose value is missing.
    CmdValueExpected,
    /// A name that resolves to nothing: a value the field lacks, a tag that is not, a user
    /// nobody matches. May carry the create-value quick fix (plan §5).
    CmdUnknownValue,
    /// A name that fits several things: a bare value naming several fields, a project and a
    /// tag sharing a name. The user qualifies; never a guess.
    CmdAmbiguous,
    /// Clearing a field that may not be empty.
    CmdNotEmpty,
    /// A value of the wrong kind for the field, or a command word the value's shape does not
    /// fit (`add` on a single-value field, removing a project).
    CmdWrongKind,
    /// A markdown field: commands cannot write it.
    CmdTextField,
    /// Voting for an issue one reported oneself.
    CmdOwnIssueVote,
    /// A permission the actor lacks; the problem names it.
    CmdNoPermission,
    /// An issue the command was to apply to is gone or invisible.
    CmdTargetNotFound,
    /// A board command with no sprint to mean (plan §4.5): a query board takes no removals, a
    /// sprint board may hold neither a current nor an auto-add sprint.
    CmdBoardNoSprints,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandProblem {
    pub code: CommandCode,
    pub span: Span,
}

/// How a command touches a list: plain sets — it replaces the whole list on multi-value
/// fields (owner, 2026-11-13) — while `add` and `remove` keep the other values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Set,
    Add,
    Remove,
}

/// One command of the line, in command order, each part with its span into the text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    Comment(Span),
    Delete(Span),
    Clone(Span),
    Move {
        word: Span,
        project: Atom,
    },
    Tag {
        word: Span,
        add: bool,
        name: Atom,
    },
    Star {
        word: Span,
        on: bool,
        user: Option<Atom>,
    },
    Vote {
        word: Span,
        on: bool,
    },
    Visibility {
        word: Span,
        mode: Mode,
        party: Atom,
    },
    Board {
        word: Span,
        mode: Mode,
        board: Atom,
        sprint: Option<Atom>,
    },
    Link {
        name: Atom,
        target: Atom,
        remove: bool,
    },
    /// `field: value` or `field value` — one value, whatever the field's kind.
    Set {
        field: Atom,
        value: Atom,
    },
    /// `add|remove field value`.
    Field {
        word: Span,
        field: Atom,
        value: Atom,
        mode: Mode,
    },
    /// `add|remove value` — the field comes from the bare-value order (plan §4.2).
    BareValue {
        word: Span,
        value: Atom,
        mode: Mode,
    },
    /// A lone name, resolved by the bare-value order (plan §4.2).
    Value(Atom),
    /// An error node; the matching problem says what went wrong.
    Error(Span),
}

impl Command {
    /// The span of everything the command covers.
    pub fn span(&self) -> Span {
        match self {
            Self::Comment(span) | Self::Delete(span) | Self::Clone(span) | Self::Error(span) => {
                *span
            }
            Self::Move { word, project } => word.to(project.span),
            Self::Tag { word, name, .. } => word.to(name.span),
            Self::Visibility { word, party, .. } => word.to(party.span),
            Self::Star { word, user, .. } => user.as_ref().map_or(*word, |u| word.to(u.span)),
            Self::Vote { word, .. } => *word,
            Self::Board {
                word,
                board,
                sprint,
                ..
            } => sprint
                .as_ref()
                .map_or(word.to(board.span), |s| word.to(s.span)),
            Self::Link { name, target, .. } => name.span.to(target.span),
            Self::Set { field, value, .. } => field.span.to(value.span),
            Self::Field { word, value, .. } | Self::BareValue { word, value, .. } => {
                word.to(value.span)
            }
            Self::Value(atom) => atom.span,
        }
    }
}

/// The parse of one command line.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandList {
    pub commands: Vec<Command>,
    /// Ordered by position.
    pub problems: Vec<CommandProblem>,
}

impl CommandList {
    pub fn has_errors(&self) -> bool {
        !self.problems.is_empty()
    }
}

/// Parses a line of commands against the dictionary. Never fails: words that start no
/// command and name nothing get an error node and a `cmd_unknown_word` problem each, and
/// parsing resumes at the next word.
pub fn parse_commands(text: &str, dictionary: &Dictionary) -> CommandList {
    let mut parser = Parser {
        text,
        tokens: lex(text),
        pos: 0,
        dict: dictionary,
        commands: Vec::new(),
        problems: Vec::new(),
    };
    while parser.pos < parser.tokens.len() {
        let before = parser.pos;
        parser.command();
        debug_assert!(parser.pos > before, "a command must consume tokens");
        if parser.pos == before {
            parser.pos += 1; // unreachable; keeps broken input from looping
        }
    }
    parser.problems.sort_by_key(|problem| problem.span.start);
    CommandList {
        commands: parser.commands,
        problems: parser.problems,
    }
}

/// The canonical text: English command words, the colon form for attributes, single spaces,
/// and quotes only where the plain word would not read back the same. The dialog's history
/// stores it (plan §3, §6); error nodes drop out.
pub fn print_commands(list: &CommandList) -> String {
    let mut out = String::new();
    for command in &list.commands {
        if matches!(command, Command::Error(_)) {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        print_command(command, &mut out);
    }
    out
}

fn print_command(command: &Command, out: &mut String) {
    let prefix = |out: &mut String, mode: Mode| match mode {
        Mode::Set => {}
        Mode::Add => out.push_str("add "),
        Mode::Remove => out.push_str("remove "),
    };
    match command {
        Command::Comment(_) => out.push_str("comment"),
        Command::Delete(_) => out.push_str("delete"),
        Command::Clone(_) => out.push_str("clone"),
        Command::Move { project, .. } => {
            out.push_str("move to ");
            push_atom(project, out);
        }
        Command::Tag { add, name, .. } => {
            out.push_str(if *add { "tag " } else { "untag " });
            push_atom(name, out);
        }
        Command::Star { on, user, .. } => {
            out.push_str(if *on { "star" } else { "unstar" });
            if let Some(user) = user {
                out.push(' ');
                push_atom(user, out);
            }
        }
        Command::Vote { on, .. } => out.push_str(if *on { "vote" } else { "unvote" }),
        Command::Visibility { mode, party, .. } => {
            prefix(out, *mode);
            out.push_str("visible to ");
            push_atom(party, out);
        }
        Command::Board {
            mode,
            board,
            sprint,
            ..
        } => {
            if *mode == Mode::Remove {
                out.push_str("remove ");
            }
            out.push_str("board ");
            push_atom(board, out);
            if let Some(sprint) = sprint {
                out.push(' ');
                push_atom(sprint, out);
            }
        }
        Command::Link {
            name,
            target,
            remove,
        } => {
            if *remove {
                out.push_str("remove ");
            }
            push_atom(name, out);
            out.push(' ');
            push_atom(target, out);
        }
        Command::Set { field, value } => {
            push_atom(field, out);
            out.push_str(": ");
            push_atom(value, out);
        }
        Command::Field {
            field, value, mode, ..
        } => {
            prefix(out, *mode);
            push_atom(field, out);
            out.push_str(": ");
            push_atom(value, out);
        }
        Command::BareValue { value, mode, .. } => {
            prefix(out, *mode);
            push_atom(value, out);
        }
        Command::Value(atom) => push_atom(atom, out),
        Command::Error(_) => {}
    }
}

/// Quoting only when the plain word would not read back: a name that folds to a command
/// word, and a quoted multi-word literal (a dictionary name may stay plain).
fn push_atom(atom: &Atom, out: &mut String) {
    let quoted = if is_bare_word(&atom.text) {
        words::command_word(&fold(&atom.text)).is_some()
    } else {
        atom.quoted
    };
    if quoted {
        out.push_str(&quote(&atom.text));
    } else {
        out.push_str(&atom.text);
    }
}

struct Parser<'a> {
    text: &'a str,
    tokens: Vec<Token>,
    pos: usize,
    dict: &'a Dictionary,
    commands: Vec<Command>,
    problems: Vec<CommandProblem>,
}

impl Parser<'_> {
    fn kind(&self, i: usize) -> Option<TokenKind> {
        self.tokens.get(i).map(|t| t.kind)
    }

    fn span(&self, i: usize) -> Span {
        self.tokens
            .get(i)
            .map_or(Span::new(self.text.len(), self.text.len()), |t| t.span)
    }

    fn bump(&mut self) -> Span {
        let span = self.span(self.pos);
        self.pos += 1;
        span
    }

    fn problem(&mut self, code: CommandCode, span: Span) {
        self.problems.push(CommandProblem { code, span });
    }

    fn slice(&self, i: usize) -> &str {
        let span = self.tokens[i].span;
        self.text.get(span.start..span.end).unwrap_or("")
    }

    fn remaining_words(&self) -> usize {
        self.tokens[self.pos..]
            .iter()
            .take_while(|t| t.kind == TokenKind::Word)
            .count()
    }

    /// `n` unquoted words from the position, folded; `None` if one is missing or quoted.
    fn run_fold(&self, n: usize) -> Option<String> {
        let mut text = String::new();
        for i in 0..n {
            if self.kind(self.pos + i) != Some(TokenKind::Word) {
                return None;
            }
            if i > 0 {
                text.push(' ');
            }
            text.push_str(self.slice(self.pos + i));
        }
        Some(fold(&text))
    }

    fn run_span(&self, n: usize) -> Span {
        Span::new(self.span(self.pos).start, self.span(self.pos + n - 1).end)
    }

    fn run_text(&self, n: usize) -> String {
        (0..n)
            .map(|i| self.slice(self.pos + i))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// One command. The alternatives follow the grammar's order (plan §3): command words,
    /// link names, attributes, then a lone value; anything else is an unknown word.
    fn command(&mut self) {
        if let Some((key, span, n)) = self.command_word() {
            self.pos += n;
            match key {
                "comment" => self.commands.push(Command::Comment(span)),
                "delete" => self.commands.push(Command::Delete(span)),
                "clone" => self.commands.push(Command::Clone(span)),
                "move_to" | "project" => self.move_to(span),
                "tag" => self.tag(span, true),
                "untag" => self.tag(span, false),
                "star" | "watcher" => self.star(span, true),
                "unstar" => self.star(span, false),
                "vote" => self.commands.push(Command::Vote {
                    word: span,
                    on: true,
                }),
                "unvote" => self.commands.push(Command::Vote {
                    word: span,
                    on: false,
                }),
                "visible_to" => self.visibility(span, Mode::Set),
                "board" => self.board(span, Mode::Add),
                "add" => self.multiple(span, Mode::Add),
                "remove" => self.multiple(span, Mode::Remove),
                _ => unreachable!("unknown command word key {key}"),
            }
            return;
        }
        if self.link(false) || self.attribute(None) {
            return;
        }
        if let Some(value) = self.single_value() {
            self.commands.push(Command::Value(value));
            return;
        }
        let span = self.bump();
        self.problem(CommandCode::CmdUnknownWord, span);
        self.commands.push(Command::Error(span));
    }

    /// The longest command word at the position: the two-word ones first (`move to`,
    /// `visible to`), then one word. `перенести_в` folds to a two-word form at one token.
    fn command_word(&self) -> Option<(&'static str, Span, usize)> {
        for n in (1..=2).rev() {
            if let Some(folded) = self.run_fold(n)
                && let Some(word) = words::command_word(&folded)
            {
                return Some((word.key, self.run_span(n), n));
            }
        }
        None
    }

    fn move_to(&mut self, word: Span) {
        let dict = self.dict;
        if let Some(project) = self.name_required(&[&dict.projects], word) {
            self.commands.push(Command::Move { word, project });
        }
    }

    fn tag(&mut self, word: Span, add: bool) {
        let dict = self.dict;
        if let Some(name) = self.name_required(&[&dict.tags], word) {
            self.commands.push(Command::Tag { word, add, name });
        }
    }

    fn star(&mut self, word: Span, on: bool) {
        let dict = self.dict;
        let user = self.name_optional(&[&dict.users]);
        self.commands.push(Command::Star { word, on, user });
    }

    fn visibility(&mut self, word: Span, mode: Mode) {
        let dict = self.dict;
        if let Some(party) = self.name_required(&[&dict.users, &dict.groups], word) {
            self.commands
                .push(Command::Visibility { word, mode, party });
        }
    }

    fn board(&mut self, word: Span, mode: Mode) {
        let dict = self.dict;
        if let Some(board) = self.name_required(&[&dict.boards], word) {
            let sprint = self.name_optional(&[&dict.sprints]);
            self.commands.push(Command::Board {
                word,
                mode,
                board,
                sprint,
            });
        }
    }

    /// `("add"|"remove"), …` (plan §3). The word alternatives come first, so a value that
    /// happens to be named `tag` cannot capture `add tag x`.
    fn multiple(&mut self, word: Span, mode: Mode) {
        if let Some((key, span, n)) = self.command_word() {
            match key {
                "tag" => {
                    self.pos += n;
                    self.tag(word.to(span), mode == Mode::Add);
                    return;
                }
                "visible_to" => {
                    self.pos += n;
                    self.visibility(word.to(span), mode);
                    return;
                }
                "board" => {
                    self.pos += n;
                    self.board(word.to(span), mode);
                    return;
                }
                _ => {}
            }
        }
        if self.link(mode == Mode::Remove) || self.attribute(Some((word, mode))) {
            return;
        }
        if let Some(value) = self.value_any() {
            self.commands.push(Command::BareValue { word, value, mode });
            return;
        }
        self.problem(CommandCode::CmdValueExpected, word);
        self.commands.push(Command::Error(word));
    }

    /// `LinkName IssueId`: link-name candidates longest first, backtracking to a shorter
    /// name when the issue ID is missing after the longer one (plan §3).
    fn link(&mut self, remove: bool) -> bool {
        let dict = self.dict;
        let candidates = self.candidates(&dict.links);
        let Some(&longest) = candidates.first() else {
            return false;
        };
        for &n in &candidates {
            let name = Atom {
                text: self.run_text(n),
                quoted: false,
                span: self.run_span(n),
            };
            self.pos += n;
            if let Some(target) = self.literal_atom() {
                self.commands.push(Command::Link {
                    name,
                    target,
                    remove,
                });
                return true;
            }
            self.pos -= n;
        }
        let span = self.run_span(longest);
        self.pos += longest;
        self.problem(CommandCode::CmdValueExpected, span);
        self.commands.push(Command::Error(span));
        true
    }

    /// `AttributeName (":" | WS) Value` (plan §3): attribute candidates longest first with
    /// backtracking. Outside `add`/`remove` the value sets the field (replacing a
    /// multi-value list); inside, `mode` says add or remove.
    fn attribute(&mut self, multi: Option<(Span, Mode)>) -> bool {
        let dict = self.dict;
        let candidates = self.candidates(&dict.attributes);
        let Some(&longest) = candidates.first() else {
            return false;
        };
        for &n in &candidates {
            let field = Atom {
                text: self.run_text(n),
                quoted: false,
                span: self.run_span(n),
            };
            let saved = self.pos;
            self.pos += n;
            let colon = self.kind(self.pos) == Some(TokenKind::Colon);
            if colon {
                self.pos += 1;
            }
            if let Some(value) = self.value_for(&fold(&field.text)) {
                let command = match multi {
                    Some((word, mode)) => Command::Field {
                        word,
                        field,
                        value,
                        mode,
                    },
                    None => Command::Set { field, value },
                };
                self.commands.push(command);
                return true;
            }
            self.pos = saved;
        }
        let mut span = self.run_span(longest);
        self.pos += longest;
        if self.kind(self.pos) == Some(TokenKind::Colon) {
            span = span.to(self.span(self.pos));
            self.pos += 1;
        }
        self.problem(CommandCode::CmdValueExpected, span);
        self.commands.push(Command::Error(span));
        true
    }

    /// The word counts (longest first) whose folded run is one of `names`.
    fn candidates(&self, names: &[String]) -> Vec<usize> {
        let runs = self.remaining_words().min(MAX_RUN_WORDS);
        (1..=runs)
            .rev()
            .filter(|&n| self.run_fold(n).is_some_and(|f| names.contains(&f)))
            .collect()
    }

    /// A name from the classes, longest first; a quoted literal or, when the dictionary does
    /// not know the name, one bare word, which resolution then reports. Nothing matches: a
    /// problem over `word` and an error node.
    fn name_required(&mut self, classes: &[&[String]], word: Span) -> Option<Atom> {
        if let Some(atom) = self.class_atom(classes, false) {
            return Some(atom);
        }
        match self.literal_atom() {
            Some(atom) => Some(atom),
            None => {
                self.problem(CommandCode::CmdValueExpected, word);
                self.commands.push(Command::Error(word));
                None
            }
        }
    }

    /// A name only on a positive match: a dictionary class or a quoted literal, never a
    /// bare fallback — after `star` the next command must still be able to start.
    fn name_optional(&mut self, classes: &[&[String]]) -> Option<Atom> {
        if let Some(atom) = self.class_atom(classes, false) {
            return Some(atom);
        }
        self.quoted_atom()
    }

    /// A lone name of any dictionary class (plan §4.2 resolves which one it is); a quoted
    /// literal. A free word is none: it becomes an unknown word at the caller.
    fn single_value(&mut self) -> Option<Atom> {
        let dict = self.dict;
        let mut classes: Vec<&[String]> = vec![
            &dict.projects,
            &dict.tags,
            &dict.users,
            &dict.groups,
            &dict.boards,
            &dict.sprints,
            &dict.special,
        ];
        classes.extend(dict.values.values().map(|v| v.as_slice()));
        self.class_atom(&classes, false)
            .or_else(|| self.quoted_atom())
    }

    /// One value after an attribute: its field's names and the special ones, longest first;
    /// a quoted literal; then one bare word, which resolution judges (`cmd_unknown_value`).
    /// The vocabulary's value and date keywords (`me`, `today`…) count as values too.
    fn value_for(&mut self, field: &str) -> Option<Atom> {
        let dict = self.dict;
        let mut classes: Vec<&[String]> = dict
            .values
            .get(field)
            .map(|values| vec![values.as_slice()])
            .unwrap_or_default();
        classes.push(&dict.special);
        self.class_atom(&classes, true)
            .or_else(|| self.literal_atom())
    }

    /// One value without a field (`add 2025.3`): every field's names and the special ones.
    fn value_any(&mut self) -> Option<Atom> {
        let dict = self.dict;
        let mut classes: Vec<&[String]> = dict.values.values().map(|v| v.as_slice()).collect();
        classes.push(&dict.special);
        self.class_atom(&classes, true)
            .or_else(|| self.literal_atom())
    }

    /// The longest run of unquoted words whose fold names something: a dictionary class
    /// and — in value positions — the vocabulary's value and date keywords.
    fn class_atom(&mut self, classes: &[&[String]], vocabulary: bool) -> Option<Atom> {
        let runs = self.remaining_words().min(MAX_RUN_WORDS);
        for n in (1..=runs).rev() {
            let Some(folded) = self.run_fold(n) else {
                continue;
            };
            let hit = classes.iter().any(|c| c.contains(&folded))
                || (vocabulary && words::keyword(&folded, words::VALUE).is_some());
            if hit {
                let atom = Atom {
                    text: self.run_text(n),
                    quoted: false,
                    span: self.run_span(n),
                };
                self.pos += n;
                return Some(atom);
            }
        }
        None
    }

    /// A quoted literal (an unclosed quote runs to the end) or one bare word.
    fn literal_atom(&mut self) -> Option<Atom> {
        let text = self.text;
        match self.kind(self.pos)? {
            TokenKind::Quoted { closed } => {
                let span = self.bump();
                let raw = text.get(span.start..span.end).unwrap_or("");
                let end = if closed { raw.len() - 1 } else { raw.len() };
                Some(Atom {
                    text: unescape(raw.get(1..end).unwrap_or("")),
                    quoted: true,
                    span,
                })
            }
            TokenKind::Word => {
                let span = self.bump();
                Some(Atom {
                    text: text.get(span.start..span.end).unwrap_or("").to_owned(),
                    quoted: false,
                    span,
                })
            }
            _ => None,
        }
    }

    fn quoted_atom(&mut self) -> Option<Atom> {
        matches!(self.kind(self.pos), Some(TokenKind::Quoted { .. }))
            .then(|| self.literal_atom())
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dict() -> Dictionary {
        let field_values = |names: &[&str]| names.iter().map(|n| n.to_string()).collect();
        Dictionary {
            attributes: vec![
                "state".into(),
                "priority".into(),
                "приоритет".into(),
                "fix versions".into(),
            ],
            values: HashMap::from([
                ("state".into(), field_values(&["in progress", "fixed"])),
                ("priority".into(), field_values(&["critical", "major"])),
                ("приоритет".into(), field_values(&["critical", "major"])),
                ("fix versions".into(), field_values(&["2025.1", "2025.3"])),
            ]),
            links: vec![
                "depends".into(),
                "depends on".into(),
                "relates to".into(),
                "duplicates".into(),
            ],
            projects: vec!["demo".into(), "new mars".into()],
            tags: vec!["web".into(), "urgent".into()],
            users: vec!["ann".into(), "bob lee".into()],
            groups: vec!["team x".into()], // folded, like every dictionary name
            boards: vec!["board one".into()],
            sprints: vec!["sprint 1".into()],
            special: vec!["unassigned".into()],
        }
    }

    /// An atom whose text appears in `text`, with its span there.
    fn a(text: &str, needle: &str, quoted: bool) -> Atom {
        let start = text
            .find(needle)
            .unwrap_or_else(|| panic!("{needle} in {text}"));
        Atom {
            text: needle.to_owned(),
            quoted,
            span: Span::new(start, start + needle.len()),
        }
    }

    /// A quoted atom: the span covers the quotes, the text is the inside of them.
    fn qa(text: &str, needle: &str) -> Atom {
        let start = text
            .find(needle)
            .unwrap_or_else(|| panic!("{needle} in {text}"));
        Atom {
            text: needle[1..needle.len() - 1].to_owned(),
            quoted: true,
            span: Span::new(start, start + needle.len()),
        }
    }

    fn sp(text: &str, needle: &str) -> Span {
        a(text, needle, false).span
    }

    fn parse(text: &str) -> CommandList {
        parse_commands(text, &dict())
    }

    #[test]
    fn cmd_002_simple_forms() {
        let text = "comment delete clone";
        assert_eq!(
            parse(text).commands,
            [
                Command::Comment(sp(text, "comment")),
                Command::Delete(sp(text, "delete")),
                Command::Clone(sp(text, "clone")),
            ]
        );
        assert!(parse(text).problems.is_empty());
        assert_eq!(
            parse("Комментарий").commands,
            [Command::Comment(sp("Комментарий", "Комментарий"))]
        );
        assert_eq!(parse(""), CommandList::default());
    }

    #[test]
    fn cmd_002_project_moves() {
        let text = "move to New Mars";
        assert_eq!(
            parse(text).commands,
            [Command::Move {
                word: sp(text, "move to"),
                project: a(text, "New Mars", false),
            }]
        );
        assert_eq!(
            parse("project demo").commands,
            [Command::Move {
                word: sp("project demo", "project"),
                project: a("project demo", "demo", false),
            }]
        );
        assert_eq!(
            parse("перенести в demo").commands,
            [Command::Move {
                word: sp("перенести в demo", "перенести в"),
                project: a("перенести в demo", "demo", false),
            }]
        );
        let quoted = "move to \"Somewhere Else\"";
        assert_eq!(
            parse(quoted).commands,
            [Command::Move {
                word: sp(quoted, "move to"),
                project: qa(quoted, "\"Somewhere Else\""),
            }]
        );
        assert!(parse("move to nowhere").problems.is_empty()); // resolution reports it
        let cut = parse("move to");
        assert_eq!(
            cut.problems,
            [CommandProblem {
                code: CommandCode::CmdValueExpected,
                span: sp("move to", "move to"),
            }]
        );
        assert_eq!(cut.commands, [Command::Error(sp("move to", "move to"))]);
    }

    #[test]
    fn cmd_002_tags() {
        let text = "tag web";
        assert_eq!(
            parse(text).commands,
            [Command::Tag {
                word: sp(text, "tag"),
                add: true,
                name: a(text, "web", false),
            }]
        );
        assert_eq!(
            parse("снять тег urgent").commands,
            [Command::Tag {
                word: sp("снять тег urgent", "снять тег"),
                add: false,
                name: a("снять тег urgent", "urgent", false),
            }]
        );
        let quoted = "тег \"two words\"";
        assert_eq!(
            parse(quoted).commands,
            [Command::Tag {
                word: sp(quoted, "тег"),
                add: true,
                name: qa(quoted, "\"two words\""),
            }]
        );
    }

    #[test]
    fn cmd_002_star_and_vote() {
        assert_eq!(
            parse("star").commands,
            [Command::Star {
                word: sp("star", "star"),
                on: true,
                user: None,
            }]
        );
        let text = "watcher Bob Lee";
        assert_eq!(
            parse(text).commands,
            [Command::Star {
                word: sp(text, "watcher"),
                on: true,
                user: Some(a(text, "Bob Lee", false)),
            }]
        );
        assert_eq!(
            parse("не следить ann").commands,
            [Command::Star {
                word: sp("не следить ann", "не следить"),
                on: false,
                user: Some(a("не следить ann", "ann", false)),
            }]
        );
        assert_eq!(
            parse("vote +1 unvote").commands,
            [
                Command::Vote {
                    word: sp("vote +1 unvote", "vote"),
                    on: true,
                },
                Command::Vote {
                    word: sp("vote +1 unvote", "+1"),
                    on: true,
                },
                Command::Vote {
                    word: sp("vote +1 unvote", "unvote"),
                    on: false,
                },
            ]
        );
        assert_eq!(
            parse("голос снять голос").commands,
            [
                Command::Vote {
                    word: sp("голос снять голос", "голос"),
                    on: true,
                },
                Command::Vote {
                    word: sp("голос снять голос", "снять голос"),
                    on: false,
                },
            ]
        );
        // An optional name takes only positive matches: the next word stays a command's.
        let cut = parse("star nosuchone comment");
        assert_eq!(
            cut.commands[0],
            Command::Star {
                word: sp("star nosuchone comment", "star"),
                on: true,
                user: None,
            }
        );
        assert_eq!(
            cut.problems,
            [CommandProblem {
                code: CommandCode::CmdUnknownWord,
                span: sp("star nosuchone comment", "nosuchone"),
            }]
        );
    }

    #[test]
    fn cmd_002_visibility() {
        let text = "видимо для Bob Lee";
        assert_eq!(
            parse(text).commands,
            [Command::Visibility {
                word: sp(text, "видимо для"),
                mode: Mode::Set,
                party: a(text, "Bob Lee", false),
            }]
        );
        let text = "remove visible to team-x";
        assert_eq!(
            parse(text).commands,
            [Command::Visibility {
                word: sp(text, "remove visible to"),
                mode: Mode::Remove,
                party: a(text, "team-x", false),
            }]
        );
        let cut = parse("visible to");
        assert_eq!(cut.problems[0].code, CommandCode::CmdValueExpected);
    }

    #[test]
    fn cmd_002_boards() {
        let text = "board Board One Sprint 1";
        assert_eq!(
            parse(text).commands,
            [Command::Board {
                word: sp(text, "board"),
                mode: Mode::Add,
                board: a(text, "Board One", false),
                sprint: Some(a(text, "Sprint 1", false)),
            }]
        );
        let text = "remove board Board One";
        assert_eq!(
            parse(text).commands,
            [Command::Board {
                word: sp(text, "remove board"),
                mode: Mode::Remove,
                board: a(text, "Board One", false),
                sprint: None,
            }]
        );
        let text = "доска Board One";
        assert_eq!(
            parse(text).commands,
            [Command::Board {
                word: sp(text, "доска"),
                mode: Mode::Add,
                board: a(text, "Board One", false),
                sprint: None,
            }]
        );
    }

    #[test]
    fn cmd_002_links() {
        let text = "depends on DEMO-12";
        assert_eq!(
            parse(text).commands,
            [Command::Link {
                name: a(text, "depends on", false),
                target: a(text, "DEMO-12", false),
                remove: false,
            }]
        );
        let text = "remove duplicates DEMO-3";
        assert_eq!(
            parse(text).commands,
            [Command::Link {
                name: a(text, "duplicates", false),
                target: a(text, "DEMO-3", false),
                remove: true,
            }]
        );
        // Longest first with backtracking: without an ID after `depends on`, the link is
        // `depends` and `on` is the target (plan §3).
        let text = "depends on";
        assert_eq!(
            parse(text).commands,
            [Command::Link {
                name: a(text, "depends", false),
                target: a(text, "on", false),
                remove: false,
            }]
        );
        let cut = parse("relates to");
        assert_eq!(
            cut.problems,
            [CommandProblem {
                code: CommandCode::CmdValueExpected,
                span: sp("relates to", "relates to"),
            }]
        );
    }

    #[test]
    fn cmd_002_attributes() {
        let text = "state In Progress";
        assert_eq!(
            parse(text).commands,
            [Command::Set {
                field: a(text, "state", false),
                value: a(text, "In Progress", false),
            }]
        );
        let text = "priority: critical";
        assert_eq!(
            parse(text).commands,
            [Command::Set {
                field: a(text, "priority", false),
                value: a(text, "critical", false),
            }]
        );
        let text = "приоритет major";
        assert_eq!(
            parse(text).commands,
            [Command::Set {
                field: a(text, "приоритет", false),
                value: a(text, "major", false),
            }]
        );
        let text = "state \"in progress\"";
        assert_eq!(
            parse(text).commands,
            [Command::Set {
                field: a(text, "state", false),
                value: qa(text, "\"in progress\""),
            }]
        );
        // The vocabulary's value keywords and the special ones count as values.
        let text = "state unassigned";
        assert_eq!(
            parse(text).commands,
            [Command::Set {
                field: a(text, "state", false),
                value: a(text, "unassigned", false),
            }]
        );
        let text = "state none";
        assert_eq!(
            parse(text).commands,
            [Command::Set {
                field: a(text, "state", false),
                value: a(text, "none", false),
            }]
        );
        // A value the dictionary lacks stays a word for resolution to judge.
        assert_eq!(
            parse("fix versions 2025.4").commands,
            [Command::Set {
                field: a("fix versions 2025.4", "fix versions", false),
                value: a("fix versions 2025.4", "2025.4", false),
            }]
        );
        assert!(parse("fix versions 2025.4").problems.is_empty());
        let cut = parse("state:");
        assert_eq!(
            cut.problems,
            [CommandProblem {
                code: CommandCode::CmdValueExpected,
                span: sp("state:", "state:"),
            }]
        );
    }

    #[test]
    fn cmd_002_multiple() {
        let text = "add 2025.3";
        assert_eq!(
            parse(text).commands,
            [Command::BareValue {
                word: sp(text, "add"),
                value: a(text, "2025.3", false),
                mode: Mode::Add,
            }]
        );
        let text = "remove tag web";
        assert_eq!(
            parse(text).commands,
            [Command::Tag {
                word: sp(text, "remove tag"),
                add: false,
                name: a(text, "web", false),
            }]
        );
        let text = "add state in progress";
        assert_eq!(
            parse(text).commands,
            [Command::Field {
                word: sp(text, "add"),
                field: a(text, "state", false),
                value: a(text, "in progress", false),
                mode: Mode::Add,
            }]
        );
        let text = "remove fix versions 2025.1";
        assert_eq!(
            parse(text).commands,
            [Command::Field {
                word: sp(text, "remove"),
                field: a(text, "fix versions", false),
                value: a(text, "2025.1", false),
                mode: Mode::Remove,
            }]
        );
        let text = "add depends on DEMO-2";
        assert_eq!(
            parse(text).commands,
            [Command::Link {
                name: a(text, "depends on", false),
                target: a(text, "DEMO-2", false),
                remove: false,
            }]
        );
        let text = "убрать \"brand new\"";
        assert_eq!(
            parse(text).commands,
            [Command::BareValue {
                word: sp(text, "убрать"),
                value: qa(text, "\"brand new\""),
                mode: Mode::Remove,
            }]
        );
        let cut = parse("add");
        assert_eq!(
            cut.problems,
            [CommandProblem {
                code: CommandCode::CmdValueExpected,
                span: sp("add", "add"),
            }]
        );
    }

    #[test]
    fn cmd_002_single_values() {
        for (text, needle) in [
            ("demo", "demo"),
            ("New Mars", "New Mars"),
            ("web", "web"),
            ("ann", "ann"),
            ("2025.3", "2025.3"),
            ("team-x", "team-x"),
        ] {
            assert_eq!(
                parse(text).commands,
                [Command::Value(a(text, needle, false))],
                "{text}"
            );
        }
        let text = "\"free text\"";
        assert_eq!(
            parse(text).commands,
            [Command::Value(qa(text, "\"free text\""))]
        );
    }

    #[test]
    fn cmd_002_unknown_words_carry_ranges() {
        let text = "xyz state fixed";
        let list = parse(text);
        assert_eq!(
            list.problems,
            [CommandProblem {
                code: CommandCode::CmdUnknownWord,
                span: sp(text, "xyz"),
            }]
        );
        assert_eq!(list.commands[0], Command::Error(sp(text, "xyz")));
        assert_eq!(
            list.commands[1],
            Command::Set {
                field: a(text, "state", false),
                value: a(text, "fixed", false),
            }
        );
        assert!(list.has_errors());
        // Symbols and every unknown word, one error node each; parsing resumes.
        let text = "foo , bar";
        assert_eq!(
            parse(text).problems,
            [
                CommandProblem {
                    code: CommandCode::CmdUnknownWord,
                    span: sp(text, "foo"),
                },
                CommandProblem {
                    code: CommandCode::CmdUnknownWord,
                    span: sp(text, ","),
                },
                CommandProblem {
                    code: CommandCode::CmdUnknownWord,
                    span: sp(text, "bar"),
                },
            ]
        );
    }

    #[test]
    fn cmd_002_command_list_in_order() {
        let text = "state fixed comment tag web move to demo";
        let list = parse(text);
        assert!(list.problems.is_empty());
        assert_eq!(list.commands.len(), 4);
        assert_eq!(list.commands[0].span(), sp(text, "state fixed"));
        assert_eq!(list.commands[3].span(), sp(text, "move to demo"));
    }

    #[test]
    fn cmd_002_canonical_printer() {
        let cases = [
            ("project demo", "move to demo"),
            ("Перенести_в demo", "move to demo"),
            ("+1", "vote"),
            ("watcher ann", "star ann"),
            ("state: fixed", "state: fixed"),
            ("state \"fixed\"", "state: fixed"),
            ("STATE Fixed", "STATE: Fixed"),
            ("приоритет critical", "приоритет: critical"),
            ("add state in progress", "add state: in progress"),
            ("remove tag web", "untag web"),
            ("add visible to team-x", "add visible to team-x"),
            ("remove board Board One", "remove board Board One"),
            ("board Board One Sprint 1", "board Board One Sprint 1"),
            ("remove duplicates DEMO-3", "remove duplicates DEMO-3"),
            ("move to New  Mars", "move to New Mars"),
            ("state comment", "state: \"comment\""),
            ("xyz comment", "comment"),
            ("снять голос", "unvote"),
        ];
        for (input, expected) in cases {
            assert_eq!(print_commands(&parse(input)), expected, "{input}");
        }
    }

    #[test]
    fn cmd_002_print_round_trips() {
        let cases = [
            "comment",
            "move to New Mars",
            "tag web",
            "untag web",
            "star Bob Lee",
            "unstar",
            "vote",
            "unvote",
            "visible to team-x",
            "add visible to team-x",
            "board Board One Sprint 1",
            "remove board Board One",
            "depends on DEMO-12",
            "remove duplicates DEMO-3",
            "state In Progress",
            "priority: critical",
            "fix versions 2025.3",
            "add state in progress",
            "remove fix versions 2025.1",
            "add 2025.3",
            "add \"brand new\"",
            "demo",
            "New Mars",
            "\"free text\"",
            "state: \"comment\"",
            "state fixed comment tag web move to demo",
        ];
        for input in cases {
            let printed = print_commands(&parse(input));
            let again = parse(&printed);
            assert!(again.problems.is_empty(), "{input} -> {printed}");
            assert_eq!(print_commands(&again), printed, "{input} -> {printed}");
        }
    }

    #[test]
    fn cmd_002_russian_keywords() {
        let text = "комментарий метка web следить голос перенести в demo";
        let list = parse(text);
        assert!(list.problems.is_empty());
        assert_eq!(list.commands.len(), 5);
        assert_eq!(list.commands[0], Command::Comment(sp(text, "комментарий")));
        assert_eq!(
            list.commands[1],
            Command::Tag {
                word: sp(text, "метка"),
                add: true,
                name: a(text, "web", false),
            }
        );
        assert_eq!(
            list.commands[4],
            Command::Move {
                word: sp(text, "перенести в"),
                project: a(text, "demo", false),
            }
        );
    }

    #[test]
    fn cmd_002_case_and_folding() {
        let text = "STATE: FIXED";
        assert_eq!(
            parse(text).commands,
            [Command::Set {
                field: a(text, "STATE", false),
                value: a(text, "FIXED", false),
            }]
        );
        assert_eq!(
            parse("Move  To demo").commands,
            [Command::Move {
                word: sp("Move  To demo", "Move  To"),
                project: a("Move  To demo", "demo", false),
            }]
        );
    }
}
