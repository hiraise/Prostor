//! The query language (QRY-001…QRY-008, QRY-044, QRY-062; plan `docs/plans/step-1.5.md` §3):
//! text to a tree with spans, and back.
//!
//! - [`lex`] splits the text into tokens; [`parse`] builds the tree and never fails: broken input
//!   gives `Error` nodes and [`Problem`]s, so highlighting works on any text.
//! - [`words`] holds the vocabulary (keywords in English and Russian) and the folding rules.
//! - [`classes`] colors the tokens before names are resolved; [`print`] writes the canonical text.
//! - [`command`] parses the command language (step 2.1) over dictionary names the service
//!   injects; [`print_commands`] writes its canonical text. [`completion`] is the completion
//!   model the assist service builds on (step 2.1 §5, D-061).
//! - Spans are byte ranges; [`utf16::offsets`] converts them for JavaScript.
//!
//! Pure: no I/O, no clock, no threads, so it builds for `wasm32-unknown-unknown`. Resolving names
//! against fields and values, and compiling to SQL, live in `domain` and `store`.

pub mod command;
pub mod completion;
pub mod highlight;
pub mod lex;
pub mod parse;
pub mod print;
pub mod problems;
pub mod syntax;
pub mod utf16;
pub mod words;

pub use command::{
    Command, CommandCode, CommandList, CommandProblem, Dictionary, Mode, parse_commands,
    print_commands,
};
pub use highlight::{Class, classes};
pub use lex::{Token, TokenKind, lex};
pub use parse::{MAX_CHARS, MAX_DEPTH, MAX_TERMS, parse};
pub use print::print;
pub use problems::{Problem, ProblemCode, Severity};
pub use syntax::{
    Atom, Condition, Direction, Expr, Item, ItemKind, Op, Query, Sort, SortKey, Span,
};
pub use words::{RESERVED_USERNAMES, fold, fold_value};
