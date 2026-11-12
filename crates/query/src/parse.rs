//! The parser (plan §3.2): recursive descent over the tokens, never failing. Broken input gives
//! `Error` nodes and problems; a query with an error-severity problem does not run.

use crate::lex::{Token, TokenKind, lex, unescape};
use crate::problems::{Problem, ProblemCode};
use crate::syntax::{
    Atom, Condition, Direction, Expr, Item, ItemKind, Op, Query, Sort, SortKey, Span,
};
use crate::words::{self, Kind, fold};

/// Longest query, in characters; the rest is not parsed (`query_too_long`).
pub const MAX_CHARS: usize = 4096;
/// Deepest nesting of groups and negations (`too_deep`).
pub const MAX_DEPTH: usize = 32;
/// Most conditions and text terms (`too_many_terms`).
pub const MAX_TERMS: usize = 50;

pub fn parse(text: &str) -> Query {
    let mut problems = Vec::new();
    let text = match text.char_indices().nth(MAX_CHARS) {
        Some((cut, _)) => {
            problems.push(Problem::new(
                ProblemCode::QueryTooLong,
                Span::new(cut, text.len()),
            ));
            text.get(..cut).unwrap_or(text)
        }
        None => text,
    };
    let tokens = lex(text);
    for token in &tokens {
        if token.kind == (TokenKind::Quoted { closed: false }) {
            problems.push(Problem::new(ProblemCode::UnclosedQuote, token.span));
        }
    }
    let mut parser = Parser {
        text,
        tokens,
        pos: 0,
        depth: 0,
        terms: 0,
        problems,
        sort: None,
    };
    let mut parts = Vec::new();
    loop {
        if let Some(expr) = parser.or() {
            parts.push(expr);
        }
        match parser.peek() {
            None => break,
            Some(TokenKind::RParen) => {
                let span = parser.bump();
                parser.problem(ProblemCode::UnexpectedClose, span);
            }
            Some(_) => {
                let span = parser.bump();
                parser.problem(ProblemCode::UnexpectedSymbol, span);
            }
        }
    }
    let mut problems = parser.problems;
    problems.sort_by_key(|p| p.span.start);
    Query {
        expr: join(parts, false),
        sort: parser.sort,
        problems,
    }
}

struct Parser<'a> {
    text: &'a str,
    tokens: Vec<Token>,
    pos: usize,
    /// Groups and negations open around the current position.
    depth: usize,
    terms: usize,
    problems: Vec<Problem>,
    sort: Option<Sort>,
}

impl Parser<'_> {
    fn kind(&self, i: usize) -> Option<TokenKind> {
        self.tokens.get(i).map(|t| t.kind)
    }

    fn peek(&self) -> Option<TokenKind> {
        self.kind(self.pos)
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

    fn last_end(&self) -> usize {
        self.pos.checked_sub(1).map_or(0, |i| self.span(i).end)
    }

    fn problem(&mut self, code: ProblemCode, span: Span) {
        self.problems.push(Problem::new(code, span));
    }

    fn slice(&self, i: usize) -> &str {
        let span = self.span(i);
        self.text.get(span.start..span.end).unwrap_or("")
    }

    fn is_atom(&self, i: usize) -> bool {
        matches!(
            self.kind(i),
            Some(TokenKind::Word | TokenKind::Quoted { .. })
        )
    }

    /// A word or quoted string followed by `:` or a comparison.
    fn is_name(&self, i: usize) -> bool {
        self.is_atom(i)
            && matches!(
                self.kind(i + 1),
                Some(
                    TokenKind::Colon
                        | TokenKind::Lt
                        | TokenKind::Le
                        | TokenKind::Gt
                        | TokenKind::Ge
                )
            )
    }

    fn is_value(&self, i: usize) -> bool {
        self.is_atom(i) && !self.is_name(i)
    }

    fn starts_operand(&self, i: usize) -> bool {
        matches!(
            self.kind(i),
            Some(TokenKind::Word | TokenKind::Quoted { .. } | TokenKind::LParen | TokenKind::Bang)
        )
    }

    /// An unquoted word that is the operator `key`. Whether it acts as one depends on its place.
    fn is_operator(&self, i: usize, key: &str) -> bool {
        self.kind(i) == Some(TokenKind::Word)
            && words::keyword(&fold(self.slice(i)), &[Kind::Operator]).is_some_and(|w| w.key == key)
    }

    fn sort_word(&self, i: usize) -> Option<&'static str> {
        if self.kind(i) != Some(TokenKind::Word) {
            return None;
        }
        words::keyword(&fold(self.slice(i)), &[Kind::Sort]).map(|w| w.key)
    }

    fn atom(&mut self) -> Atom {
        let quoted = self.peek();
        let span = self.bump();
        let raw = self.text.get(span.start..span.end).unwrap_or("");
        match quoted {
            Some(TokenKind::Quoted { closed }) => {
                let end = if closed {
                    raw.len().saturating_sub(1)
                } else {
                    raw.len()
                };
                Atom {
                    text: unescape(raw.get(1..end).unwrap_or("")),
                    quoted: true,
                    span,
                }
            }
            _ => Atom {
                text: raw.to_owned(),
                quoted: false,
                span,
            },
        }
    }

    fn count_term(&mut self, span: Span) {
        self.terms += 1;
        if self.terms == MAX_TERMS + 1 {
            self.problem(ProblemCode::TooManyTerms, span);
        }
    }

    fn enter(&mut self) -> bool {
        if self.depth >= MAX_DEPTH {
            return false;
        }
        self.depth += 1;
        true
    }

    /// `or = and { OR and }`. `or` is an operator only between two operands.
    fn or(&mut self) -> Option<Expr> {
        let mut list: Vec<Expr> = self.and().into_iter().collect();
        while self.is_operator(self.pos, "or") && self.starts_operand(self.pos + 1) {
            self.bump();
            // `None` only when the operand was a sort clause, which stands apart.
            list.extend(self.and());
        }
        join(list, true)
    }

    /// `and = unary { [ AND ] unary }`: juxtaposition is AND.
    fn and(&mut self) -> Option<Expr> {
        let mut list = Vec::new();
        let mut consumed = false;
        loop {
            match self.peek() {
                None | Some(TokenKind::RParen) => break,
                Some(TokenKind::Word) if consumed && self.starts_operand(self.pos + 1) => {
                    if self.is_operator(self.pos, "or") {
                        break;
                    }
                    if self.is_operator(self.pos, "and") {
                        self.bump();
                    }
                }
                _ => {}
            }
            let before = self.pos;
            list.extend(self.unary());
            consumed = true;
            if self.pos == before {
                break;
            }
        }
        join(list, false)
    }

    fn is_not(&self) -> bool {
        match self.peek() {
            Some(TokenKind::Bang) => true,
            Some(TokenKind::Word) => {
                self.is_operator(self.pos, "not") && self.starts_operand(self.pos + 1)
            }
            _ => false,
        }
    }

    fn unary(&mut self) -> Option<Expr> {
        if !self.is_not() {
            return self.primary();
        }
        let op = self.bump();
        if !self.starts_operand(self.pos) {
            self.problem(ProblemCode::MissingOperand, op);
            return Some(Expr::Error(op));
        }
        if !self.enter() {
            self.problem(ProblemCode::TooDeep, op);
            while self.is_not() {
                self.bump();
            }
            let inner = self.primary();
            return Some(Expr::Error(inner.map_or(op, |e| op.to(e.span()))));
        }
        let inner = self.unary();
        self.depth -= 1;
        match inner {
            Some(inner) => Some(Expr::Not(Box::new(inner), op)),
            None => {
                self.problem(ProblemCode::MissingOperand, op);
                Some(Expr::Error(op))
            }
        }
    }

    fn primary(&mut self) -> Option<Expr> {
        match self.peek()? {
            TokenKind::LParen => self.group(),
            TokenKind::Word | TokenKind::Quoted { .. } => {
                if self.is_name(self.pos) {
                    if self.sort_word(self.pos) == Some("sort")
                        && self.kind(self.pos + 1) == Some(TokenKind::Colon)
                    {
                        return self.sort_clause();
                    }
                    return Some(self.condition());
                }
                let atom = self.atom();
                self.count_term(atom.span);
                Some(Expr::Text(atom))
            }
            TokenKind::RParen => None,
            _ => {
                let span = self.bump();
                self.problem(ProblemCode::UnexpectedSymbol, span);
                Some(Expr::Error(span))
            }
        }
    }

    fn group(&mut self) -> Option<Expr> {
        let open = self.bump();
        if !self.enter() {
            self.problem(ProblemCode::TooDeep, open);
            let mut level = 1;
            let mut end = open.end;
            while let Some(kind) = self.peek() {
                end = self.bump().end;
                match kind {
                    TokenKind::LParen => level += 1,
                    TokenKind::RParen => {
                        level -= 1;
                        if level == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
            }
            return Some(Expr::Error(Span::new(open.start, end)));
        }
        let inner = self.or();
        self.depth -= 1;
        let close = if self.peek() == Some(TokenKind::RParen) {
            Some(self.bump())
        } else {
            self.problem(ProblemCode::UnclosedGroup, open);
            None
        };
        match (inner, close) {
            (Some(inner), _) => Some(inner),
            (None, None) => Some(Expr::Error(open)),
            (None, Some(close)) => {
                let span = open.to(close);
                self.problem(ProblemCode::EmptyGroup, span);
                Some(Expr::Error(span))
            }
        }
    }

    /// `name ":" item { "," item }` or `name <op> value`.
    fn condition(&mut self) -> Expr {
        let name = self.atom();
        let op = match self.peek() {
            Some(TokenKind::Lt) => Op::Lt,
            Some(TokenKind::Le) => Op::Le,
            Some(TokenKind::Gt) => Op::Gt,
            Some(TokenKind::Ge) => Op::Ge,
            _ => Op::Is,
        };
        let op_span = self.bump();
        self.count_term(name.span);
        let mut items = Vec::new();
        if op == Op::Is {
            self.items(&mut items, op_span);
        } else if self.is_value(self.pos) {
            let value = self.atom();
            items.push(Item {
                excluded: false,
                span: value.span,
                kind: ItemKind::Value(value),
            });
        } else {
            self.problem(ProblemCode::MissingValue, op_span);
        }
        let span = Span::new(name.span.start, self.last_end());
        Expr::Cond(Condition {
            name,
            op,
            items,
            span,
        })
    }

    fn items(&mut self, items: &mut Vec<Item>, mut after: Span) {
        loop {
            let start = self.pos;
            match self.item() {
                Some(item) => items.push(item),
                None => {
                    if self.pos == start {
                        self.problem(ProblemCode::MissingValue, after);
                    }
                    return;
                }
            }
            if self.peek() != Some(TokenKind::Comma) {
                return;
            }
            after = self.bump();
        }
    }

    /// `[ "!" ] ( value | value ".." [ value ] | ".." value )`.
    fn item(&mut self) -> Option<Item> {
        let bang = (self.peek() == Some(TokenKind::Bang)).then(|| self.bump());
        let first = self.span(self.pos);
        let kind = if self.peek() == Some(TokenKind::DotDot) {
            let dots = self.bump();
            let to = self.range_end(dots);
            if to.is_none() {
                self.problem(ProblemCode::BadRange, dots);
            }
            ItemKind::Range(None, to)
        } else if self.is_value(self.pos) {
            let value = self.atom();
            if self.peek() == Some(TokenKind::DotDot) {
                let dots = self.bump();
                ItemKind::Range(Some(value), self.range_end(dots))
            } else {
                ItemKind::Value(value)
            }
        } else {
            if let Some(bang) = bang {
                self.problem(ProblemCode::MissingValue, bang);
            }
            return None;
        };
        let start = bang.map_or(first.start, |b| b.start);
        Some(Item {
            excluded: bang.is_some(),
            kind,
            span: Span::new(start, self.last_end()),
        })
    }

    /// The end of a range is the value right after `..`, with no space between: in
    /// `created: 2026-01.. foo` the range is open and `foo` is a separate term.
    fn range_end(&mut self, dots: Span) -> Option<Atom> {
        (self.is_value(self.pos) && self.span(self.pos).start == dots.end).then(|| self.atom())
    }

    /// `SORT ":" key { "," key }`, only at the top level and only once.
    fn sort_clause(&mut self) -> Option<Expr> {
        let start = self.bump();
        let mut after = self.bump();
        let mut keys = Vec::new();
        loop {
            if !self.is_value(self.pos) {
                self.problem(ProblemCode::MissingValue, after);
                break;
            }
            let name = self.atom();
            let direction = if self.is_value(self.pos) {
                match self.sort_word(self.pos) {
                    Some("asc") => Some(Direction::Asc),
                    Some("desc") => Some(Direction::Desc),
                    _ => None,
                }
            } else {
                None
            };
            if direction.is_some() {
                self.bump();
            }
            keys.push(SortKey { name, direction });
            if self.peek() != Some(TokenKind::Comma) {
                break;
            }
            after = self.bump();
        }
        let span = Span::new(start.start, self.last_end());
        if self.depth > 0 {
            self.problem(ProblemCode::SortInGroup, span);
            return Some(Expr::Error(span));
        }
        if self.sort.is_some() {
            self.problem(ProblemCode::SortTwice, span);
            return Some(Expr::Error(span));
        }
        if keys.is_empty() {
            return Some(Expr::Error(span));
        }
        self.sort = Some(Sort { keys, span });
        None
    }
}

/// Joins terms into one AND (or OR), flattening nested ones of the same kind.
fn join(list: Vec<Expr>, or: bool) -> Option<Expr> {
    let mut out = Vec::with_capacity(list.len());
    for expr in list {
        match expr {
            Expr::And(inner) if !or => out.extend(inner),
            Expr::Or(inner) if or => out.extend(inner),
            expr => out.push(expr),
        }
    }
    match out.len() {
        0 => None,
        1 => out.pop(),
        _ => Some(if or { Expr::Or(out) } else { Expr::And(out) }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codes(text: &str) -> Vec<ProblemCode> {
        parse(text).problems.into_iter().map(|p| p.code).collect()
    }

    #[test]
    fn depth_limit() {
        let ok = format!("{}a{}", "(".repeat(MAX_DEPTH), ")".repeat(MAX_DEPTH));
        assert_eq!(codes(&ok), []);
        let deep = format!(
            "{}a{} b",
            "(".repeat(MAX_DEPTH + 1),
            ")".repeat(MAX_DEPTH + 1)
        );
        let query = parse(&deep);
        assert_eq!(
            query.problems.iter().map(|p| p.code).collect::<Vec<_>>(),
            [ProblemCode::TooDeep]
        );
        assert_eq!(query.problems[0].span, Span::new(MAX_DEPTH, MAX_DEPTH + 1));
        assert_eq!(query.outline(), "(and (error) 'b')");
        assert_eq!(codes(&"!".repeat(MAX_DEPTH)).len(), 1); // only the missing operand
        assert_eq!(codes(&format!("{}a", "!".repeat(MAX_DEPTH))), []);
        assert_eq!(
            codes(&format!("{}a", "!".repeat(4000))),
            [ProblemCode::TooDeep]
        );
        let balanced = format!("{}a{}", "(".repeat(2000), ")".repeat(2000));
        assert_eq!(codes(&balanced), [ProblemCode::TooDeep]);
    }

    #[test]
    fn term_limit() {
        let ok = vec!["a"; MAX_TERMS].join(" ");
        assert_eq!(codes(&ok), []);
        let many = format!("{ok} state: open");
        let query = parse(&many);
        assert_eq!(query.problems.len(), 1);
        assert_eq!(query.problems[0].code, ProblemCode::TooManyTerms);
        assert_eq!(
            query.problems[0].span,
            Span::new(ok.len() + 1, ok.len() + 6)
        );
    }

    #[test]
    fn length_limit() {
        let fits = "я".repeat(MAX_CHARS);
        assert_eq!(codes(&fits), []);
        let long = format!("{fits} b");
        let query = parse(&long);
        assert_eq!(query.problems.len(), 1);
        assert_eq!(query.problems[0].code, ProblemCode::QueryTooLong);
        assert_eq!(query.problems[0].span, Span::new(fits.len(), long.len()));
        assert_eq!(query.outline(), format!("'{fits}'"));
    }
}
