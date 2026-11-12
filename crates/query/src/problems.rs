//! Problems found in a query: by the parser here, by the resolver in `domain::search`.

use serde::{Deserialize, Serialize};

use crate::syntax::Span;

/// An error stops the query from running; a note only informs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Note,
}

/// Every problem the language can report. Closed: the API and the web translate each code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProblemCode {
    // Parser.
    UnclosedQuote,
    UnclosedGroup,
    UnexpectedClose,
    /// A `,`, `:`, `..` or comparison where no condition can hold it.
    UnexpectedSymbol,
    MissingValue,
    MissingOperand,
    EmptyGroup,
    SortTwice,
    SortInGroup,
    BadRange,
    TooDeep,
    TooManyTerms,
    QueryTooLong,
    // Resolver: errors.
    UnknownAttribute,
    UnknownValue,
    AmbiguousUser,
    WrongOperator,
    BadDate,
    BadNumber,
    NotSortable,
    /// More than three sort keys.
    TooManySortKeys,
    DeletedReference,
    // Resolver: notes.
    SameFieldTwice,
    FieldShadowed,
    HideResolvedIgnored,
    IgnoredCommonWord,
}

impl ProblemCode {
    /// The codes the parser itself reports.
    pub const PARSER: &[ProblemCode] = &[
        Self::UnclosedQuote,
        Self::UnclosedGroup,
        Self::UnexpectedClose,
        Self::UnexpectedSymbol,
        Self::MissingValue,
        Self::MissingOperand,
        Self::EmptyGroup,
        Self::SortTwice,
        Self::SortInGroup,
        Self::BadRange,
        Self::TooDeep,
        Self::TooManyTerms,
        Self::QueryTooLong,
    ];

    pub const fn severity(self) -> Severity {
        match self {
            Self::SameFieldTwice
            | Self::FieldShadowed
            | Self::HideResolvedIgnored
            | Self::IgnoredCommonWord => Severity::Note,
            _ => Severity::Error,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Problem {
    pub code: ProblemCode,
    pub span: Span,
    pub severity: Severity,
}

impl Problem {
    pub const fn new(code: ProblemCode, span: Span) -> Self {
        Self {
            code,
            span,
            severity: code.severity(),
        }
    }
}
