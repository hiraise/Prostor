//! Text terms and mentions (plan step 1.5 §C2): bare words, quoted forms and phrases, scoped by
//! `summary:`, `description:`, `comments:`, `text:` or a text field; `mentions:` and
//! `mentioned in:`.

use prostor_query::{Atom, Condition, ItemKind, Op, ProblemCode};

use super::filter::{Filter, TextScope, TextTerm};
use super::names::looks_like_id;
use super::resolve::{R, Who, strong_keyword};
use super::{Binding, FieldRef};
use crate::fields::FieldKind;

/// Most text terms in one query.
pub const TEXT_TERMS_MAX: usize = 10;

impl R<'_> {
    /// Every place text is searched in: summary, description, the text fields in scope, comments.
    pub(super) fn all_text(&self) -> TextScope {
        let fields = self
            .cat
            .fields
            .iter()
            .filter(|f| f.kind == FieldKind::Text)
            .filter(|f| !self.cat.attachments(f.id, &self.ctx.scope).is_empty())
            .map(|f| self.field_ref(f))
            .collect();
        TextScope::All { fields }
    }

    /// A bare word or quoted text. A bare issue ID is that issue or the text (QRY-016). `None`:
    /// only common words, left out of the query.
    pub(super) fn text_atom(&mut self, atom: &Atom) -> Option<Filter> {
        let scope = self.all_text();
        if !atom.quoted
            && looks_like_id(&atom.text)
            && let Some(id) = self.cat.issue(&atom.text)
        {
            self.bind(atom.span, Binding::Issue(id));
            self.bare_id = Some(id);
            let issue = Filter::Issues(vec![id]);
            return Some(match self.term(scope, atom, false) {
                Some(text) => Filter::or(vec![issue, text]),
                None => issue,
            });
        }
        let term = self.term(scope, atom, false)?;
        self.bind(atom.span, Binding::Literal(atom.text.clone()));
        Some(term)
    }

    /// The scope a text attribute's condition searches; `None` for `description:` and
    /// `comments:` with only `none` / `any`, which ask about presence.
    pub(super) fn text_scope(&self, key: &str, c: &Condition) -> Option<TextScope> {
        let words = || {
            c.items.iter().any(|item| match &item.kind {
                ItemKind::Value(atom) => strong_keyword(atom).is_none(),
                ItemKind::Range(..) => false,
            })
        };
        match key {
            "summary" => Some(TextScope::Summary),
            "text" => Some(self.all_text()),
            "description" if words() => Some(TextScope::Description),
            "comments" if words() => Some(TextScope::Comments),
            _ => None,
        }
    }

    /// A condition whose values are text terms: any of the positive ones, none of the excluded.
    /// `None` when every term is common words.
    pub(super) fn text_cond(&mut self, scope: &TextScope, c: &Condition) -> Option<Filter> {
        if c.op != Op::Is {
            self.problem(ProblemCode::WrongOperator, c.span);
            return Some(Filter::True);
        }
        let mut positive = Vec::new();
        let mut negative = Vec::new();
        for item in &c.items {
            let filter = match &item.kind {
                ItemKind::Range(..) => {
                    self.problem(ProblemCode::WrongOperator, item.span);
                    Some(Filter::True)
                }
                ItemKind::Value(atom) => self.text_item(scope, atom, item.excluded),
            };
            match filter {
                Some(f) if item.excluded => negative.push(Filter::not(f)),
                Some(f) => positive.push(f),
                None => {}
            }
        }
        if positive.is_empty() && negative.is_empty() {
            return None;
        }
        if !positive.is_empty() {
            negative.insert(0, Filter::or(positive));
        }
        Some(Filter::and(negative))
    }

    fn text_item(&mut self, scope: &TextScope, atom: &Atom, excluded: bool) -> Option<Filter> {
        if let Some(key) = strong_keyword(atom) {
            let has = match (scope, key) {
                (TextScope::Description, "none" | "any") => {
                    Some(Filter::not(Filter::DescriptionEmpty))
                }
                (TextScope::Comments, "none" | "any") => Some(Filter::Comments),
                (TextScope::Field(field), "none" | "any") => {
                    return self.empty_any(field, atom).or(Some(Filter::True));
                }
                _ => None,
            };
            let Some(has) = has else {
                self.problem(ProblemCode::UnknownValue, atom.span);
                return Some(Filter::True);
            };
            let filter = if key == "none" { Filter::not(has) } else { has };
            return Some(self.keyword(atom, key, filter));
        }
        let term = self.term(scope.clone(), atom, excluded)?;
        self.bind(atom.span, Binding::Literal(atom.text.clone()));
        Some(term)
    }

    /// A text field's condition (any value but `none` / `any` is text).
    pub(super) fn text_field(&mut self, field: &FieldRef, c: &Condition) -> Filter {
        self.text_cond(&TextScope::Field(field.clone()), c)
            .unwrap_or(Filter::True)
    }

    /// One term; `None` with the note `ignored_common_word` when its text query is empty.
    fn term(&mut self, scope: TextScope, atom: &Atom, excluded: bool) -> Option<Filter> {
        let text = atom.text.trim();
        if text.is_empty() || self.cat.common_words.contains(text) {
            self.problem(ProblemCode::IgnoredCommonWord, atom.span);
            return None;
        }
        self.text_terms += 1;
        if self.text_terms > TEXT_TERMS_MAX {
            if self.text_terms == TEXT_TERMS_MAX + 1 {
                self.problem(ProblemCode::TooManyTerms, atom.span);
            }
            return Some(Filter::True);
        }
        let term = if !atom.quoted {
            TextTerm::Word(text.to_owned())
        } else if text.split_whitespace().count() == 1 {
            TextTerm::Exact(text.to_owned())
        } else {
            TextTerm::Phrase(text.split_whitespace().collect::<Vec<_>>().join(" "))
        };
        if !(self.negated ^ excluded) {
            self.ranked.push((scope.clone(), term.clone()));
        }
        Some(Filter::Text { scope, term })
    }

    /// `mentions:` a person (or a group's members, or `me`) or an issue (CNT-058).
    pub(super) fn mentions(&mut self, c: &Condition) -> Filter {
        self.values(c, |r, atom| {
            if !atom.quoted
                && looks_like_id(&atom.text)
                && let Some(id) = r.cat.issue(&atom.text)
            {
                r.bind(atom.span, Binding::Issue(id));
                return Filter::MentionsIssue(id);
            }
            match r.who(atom) {
                Some(Who::Users(ids)) => Filter::MentionsUsers(ids),
                Some(Who::None | Who::Any) => {
                    r.problem(ProblemCode::UnknownValue, atom.span);
                    Filter::True
                }
                None => Filter::True,
            }
        })
    }

    /// `mentioned in:` an issue: the issues its description and visible comments mention.
    pub(super) fn mentioned_in(&mut self, c: &Condition) -> Filter {
        self.values(c, |r, atom| match r.cat.issue(&atom.text) {
            Some(id) => {
                r.bind(atom.span, Binding::Issue(id));
                Filter::MentionedIn(id)
            }
            None => {
                r.problem(ProblemCode::UnknownValue, atom.span);
                Filter::True
            }
        })
    }
}
