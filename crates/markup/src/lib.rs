//! Rich text (CNT-038…CNT-045, CNT-054, CNT-056, D-031): Markdown to safe HTML on the server, one
//! renderer for previews, the issue page, lists, notifications and exports.
//!
//! comrak parses CommonMark with the strikethrough, table, autolink and task list extensions; one
//! pass over the tree turns mentions, issue IDs and file names into links with our own schemes;
//! ammonia sanitizes the output against an allowlist. The markers are ordinary links, so nothing
//! raw is injected:
//!
//! - `<a href="tf-user:57">@jsmith</a>` — a mention, by account ID, resolved when the text is
//!   saved;
//! - `<a href="tf-issue:ABC-12">ABC-12</a>` — an issue ID, resolved for each viewer when read;
//! - `<img src="tf-file:shot.png">`, `<a href="tf-file:log.txt">` — a file of the issue by name.
//!
//! URLs with a `tf-` scheme written by hand are cleared first, so markers come only from here.
//! Pure: no I/O, the caller resolves logins and stores the output.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use ammonia::{Builder, UrlRelative};
use comrak::nodes::{Ast, AstNode, LineColumn, NodeLink, NodeValue};
use comrak::{Arena, Options};

/// Version of the rendering rules; stored with every rendered text. Raise it when the output
/// changes, and a background job renders older texts again.
pub const RENDER_VERSION: i16 = 1;

/// Scheme of mention markers.
pub const USER_SCHEME: &str = "tf-user:";
/// Scheme of issue ID markers.
pub const ISSUE_SCHEME: &str = "tf-issue:";
/// Scheme of file reference markers.
pub const FILE_SCHEME: &str = "tf-file:";

/// What a text may contain (CNT-039).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    /// Issue descriptions, text fields and comments: mentions, issue IDs and files.
    Full,
    /// Project descriptions: Markdown alone.
    Basic,
}

/// A rendered text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Output {
    /// Sanitized HTML with markers.
    pub html: String,
    /// Accounts mentioned, in order of first appearance.
    pub users: Vec<i64>,
    /// Issue ID tokens such as `ABC-12`, in order of first appearance.
    pub issue_tokens: Vec<String>,
    /// File names referenced, percent-decoded, in order of first appearance.
    pub files: Vec<String>,
}

/// Logins after `@` in a text, lowercased, for the caller to look up before [`render`]. Each
/// mention gives its longest form and, when that ends with `.` or `-`, the form without them.
pub fn mentions(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut found: Vec<String> = Vec::new();
    for at in 0..chars.len() {
        if let Some((full, trimmed)) = mention_at(&chars, at) {
            for login in [Some(full), trimmed].into_iter().flatten() {
                let login = login.to_ascii_lowercase();
                if !found.contains(&login) {
                    found.push(login);
                }
            }
        }
    }
    found
}

/// Renders a text. `users` maps lowercased logins to account IDs; only those become mentions.
pub fn render(text: &str, profile: Profile, users: &HashMap<String, i64>) -> Output {
    let options = options();
    let arena = Arena::new();
    let root = comrak::parse_document(&arena, text, &options);
    let mut out = Output::default();
    let mut pass = Pass {
        arena: &arena,
        profile,
        users,
        out: &mut out,
    };
    pass.walk(root);
    let mut html = String::new();
    if comrak::format_html(root, &options, &mut html).is_err() {
        html.clear();
    }
    out.html = sanitizer().clean(&html).to_string();
    out
}

fn options() -> Options<'static> {
    let mut options = Options::default();
    options.extension.strikethrough = true;
    options.extension.table = true;
    options.extension.autolink = true;
    options.extension.tasklist = true;
    // Raw HTML is shown as text until the HTML subset (CNT-047).
    options.render.r#unsafe = false;
    options.render.escape = true;
    // Kept on list items only, for checklist toggles.
    options.render.sourcepos = true;
    options
}

struct Pass<'a, 'o> {
    arena: &'a Arena<'a>,
    profile: Profile,
    users: &'o HashMap<String, i64>,
    out: &'o mut Output,
}

impl<'a> Pass<'a, '_> {
    fn walk(&mut self, node: &'a AstNode<'a>) {
        merge_texts(node);
        let children: Vec<&'a AstNode<'a>> = node.children().collect();
        for child in children {
            let kind = {
                let ast = child.data.borrow();
                match &ast.value {
                    NodeValue::Link(_) | NodeValue::Image(_) => Kind::Link,
                    NodeValue::Text(_) => Kind::Text,
                    NodeValue::Code(_) | NodeValue::CodeBlock(_) => Kind::Skip,
                    _ => Kind::Other,
                }
            };
            match kind {
                Kind::Link => self.link(child),
                Kind::Text => self.text(child),
                Kind::Skip => {}
                Kind::Other => self.walk(child),
            }
        }
    }

    /// Clears hand-written markers and turns names into file references; text inside stays as
    /// written.
    fn link(&mut self, node: &'a AstNode<'a>) {
        let mut ast = node.data.borrow_mut();
        let (NodeValue::Link(link) | NodeValue::Image(link)) = &mut ast.value else {
            return;
        };
        let url = link.url.trim();
        if url.to_ascii_lowercase().starts_with("tf-") {
            link.url.clear();
            return;
        }
        if self.profile == Profile::Full && is_file_name(url) {
            let name = percent_decode(url);
            if !self.out.files.contains(&name) {
                self.out.files.push(name);
            }
            link.url = format!("{FILE_SCHEME}{url}");
        }
    }

    fn text(&mut self, node: &'a AstNode<'a>) {
        if self.profile != Profile::Full {
            return;
        }
        let text = match &node.data.borrow().value {
            NodeValue::Text(text) => text.to_string(),
            _ => return,
        };
        let pieces = self.split(&text);
        if pieces.iter().all(|p| matches!(p, Piece::Text(_))) {
            return;
        }
        for piece in pieces {
            let new = match piece {
                Piece::Text(text) => self.node(NodeValue::Text(Cow::Owned(text))),
                Piece::Marker { url, label } => {
                    let link = self.node(NodeValue::Link(Box::new(NodeLink {
                        url,
                        title: String::new(),
                    })));
                    link.append(self.node(NodeValue::Text(Cow::Owned(label))));
                    link
                }
            };
            node.insert_before(new);
        }
        node.detach();
    }

    fn node(&self, value: NodeValue) -> &'a AstNode<'a> {
        self.arena.alloc(AstNode::new(RefCell::new(Ast::new(
            value,
            LineColumn::default(),
        ))))
    }

    /// Splits a text into plain parts, mentions of known logins and issue IDs.
    fn split(&mut self, text: &str) -> Vec<Piece> {
        let chars: Vec<char> = text.chars().collect();
        let mut pieces = Vec::new();
        let mut plain = String::new();
        let mut i = 0;
        while i < chars.len() {
            if let Some((len, id)) = self.known_mention(&chars, i) {
                let label: String = chars[i..i + len].iter().collect();
                push_plain(&mut pieces, &mut plain);
                if !self.out.users.contains(&id) {
                    self.out.users.push(id);
                }
                pieces.push(Piece::Marker {
                    url: format!("{USER_SCHEME}{id}"),
                    label,
                });
                i += len;
                continue;
            }
            if let Some(len) = issue_token_at(&chars, i) {
                let token: String = chars[i..i + len].iter().collect();
                push_plain(&mut pieces, &mut plain);
                if !self.out.issue_tokens.contains(&token) {
                    self.out.issue_tokens.push(token.clone());
                }
                pieces.push(Piece::Marker {
                    url: format!("{ISSUE_SCHEME}{token}"),
                    label: token,
                });
                i += len;
                continue;
            }
            plain.push(chars[i]);
            i += 1;
        }
        push_plain(&mut pieces, &mut plain);
        pieces
    }

    /// A mention of a known login at `at`: its length with the `@` and the account.
    fn known_mention(&self, chars: &[char], at: usize) -> Option<(usize, i64)> {
        let (full, trimmed) = mention_at(chars, at)?;
        for login in [Some(full), trimmed].into_iter().flatten() {
            if let Some(id) = self.users.get(&login.to_ascii_lowercase()) {
                return Some((login.chars().count() + 1, *id));
            }
        }
        None
    }
}

enum Kind {
    Link,
    Text,
    Skip,
    Other,
}

enum Piece {
    Text(String),
    Marker { url: String, label: String },
}

fn push_plain(pieces: &mut Vec<Piece>, plain: &mut String) {
    if !plain.is_empty() {
        pieces.push(Piece::Text(std::mem::take(plain)));
    }
}

/// Joins neighbouring text nodes: the parser splits text at characters that might have started
/// emphasis or links, which would cut `@john_doe` in two.
fn merge_texts<'a>(node: &'a AstNode<'a>) {
    let mut child = node.first_child();
    while let Some(current) = child {
        let next = current.next_sibling();
        if let Some(following) = next {
            let joined = {
                let first = current.data.borrow();
                let second = following.data.borrow();
                match (&first.value, &second.value) {
                    (NodeValue::Text(a), NodeValue::Text(b)) => Some(format!("{a}{b}")),
                    _ => None,
                }
            };
            if let Some(text) = joined {
                current.data.borrow_mut().value = NodeValue::Text(Cow::Owned(text));
                following.detach();
                continue;
            }
        }
        child = next;
    }
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn is_login_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')
}

/// A mention at `at` (CNT-054): `@` not after a word character, `@`, `.` or `/` (so `a@b.c` is
/// not one), then a login of the username alphabet starting with a letter or digit. Returns the
/// longest login and, when it ends with `.` or `-`, the login without them.
fn mention_at(chars: &[char], at: usize) -> Option<(String, Option<String>)> {
    if chars.get(at) != Some(&'@') {
        return None;
    }
    if at > 0 {
        let before = chars[at - 1];
        if is_word(before) || matches!(before, '@' | '.' | '/') {
            return None;
        }
    }
    let first = *chars.get(at + 1)?;
    if !first.is_ascii_alphanumeric() {
        return None;
    }
    let len = chars[at + 1..]
        .iter()
        .take_while(|c| is_login_char(**c))
        .count();
    let full: String = chars[at + 1..at + 1 + len].iter().collect();
    let trimmed = full.trim_end_matches(['.', '-']);
    let trimmed = (trimmed.len() < full.len() && !trimmed.is_empty()).then(|| trimmed.to_owned());
    Some((full, trimmed))
}

/// An issue ID at `at` (CNT-056): `\b[A-Z][A-Z0-9_]{1,9}-[1-9][0-9]{0,17}\b`. Returns its length.
fn issue_token_at(chars: &[char], at: usize) -> Option<usize> {
    if !chars.get(at)?.is_ascii_uppercase() {
        return None;
    }
    if at > 0 && is_word(chars[at - 1]) {
        return None;
    }
    let key = chars[at..]
        .iter()
        .take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || **c == '_')
        .count();
    if !(2..=10).contains(&key) || chars.get(at + key) != Some(&'-') {
        return None;
    }
    let start = at + key + 1;
    if !matches!(chars.get(start), Some('1'..='9')) {
        return None;
    }
    let digits = chars[start..]
        .iter()
        .take_while(|c| c.is_ascii_digit())
        .count();
    if digits > 18 {
        return None;
    }
    let end = start + digits;
    if chars.get(end).is_some_and(|c| is_word(*c)) {
        return None;
    }
    Some(end - at)
}

/// Whether a link target names a file of the issue (CNT-044): no scheme and not a path, anchor,
/// query or relative path.
fn is_file_name(url: &str) -> bool {
    if url.is_empty() || url.starts_with(['/', '#', '?', '.']) {
        return false;
    }
    !has_scheme(url)
}

fn has_scheme(url: &str) -> bool {
    let Some((scheme, _)) = url.split_once(':') else {
        return false;
    };
    scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// Decodes `%XX` sequences; invalid ones stay as written.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let (Some(high), Some(low)) = (
                bytes.get(i + 1).and_then(|b| hex(*b)),
                bytes.get(i + 2).and_then(|b| hex(*b)),
            )
        {
            out.push(high * 16 + low);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn is_language_class(value: &str) -> bool {
    value.strip_prefix("language-").is_some_and(|name| {
        !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '+' | '#' | '.' | '-'))
    })
}

fn is_sourcepos(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, ':' | '-'))
}

/// The allowlist (architecture.md §8): what comrak emits for our extensions, nothing else.
fn sanitizer() -> Builder<'static> {
    let tags: HashSet<&str> = [
        "p",
        "br",
        "hr",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "strong",
        "em",
        "del",
        "code",
        "pre",
        "blockquote",
        "ul",
        "ol",
        "li",
        "a",
        "img",
        "table",
        "thead",
        "tbody",
        "tr",
        "th",
        "td",
        "input",
    ]
    .into_iter()
    .collect();
    let attributes: HashMap<&str, HashSet<&str>> = [
        ("a", &["href", "title"][..]),
        ("img", &["src", "alt", "title"]),
        ("ol", &["start"]),
        ("li", &["data-sourcepos"]),
        ("code", &["class"]),
        ("th", &["align"]),
        ("td", &["align"]),
        ("input", &["type", "checked", "disabled"]),
    ]
    .into_iter()
    .map(|(tag, names)| (tag, names.iter().copied().collect()))
    .collect();
    let schemes: HashSet<&str> = ["http", "https", "mailto", "tf-user", "tf-issue", "tf-file"]
        .into_iter()
        .collect();
    let mut builder = Builder::empty();
    builder
        .tags(tags)
        .tag_attributes(attributes)
        .url_schemes(schemes)
        .url_relative(UrlRelative::PassThrough)
        .link_rel(Some("noopener noreferrer nofollow"))
        .strip_comments(true)
        .attribute_filter(|element, attribute, value| {
            let keep = match (element, attribute) {
                ("code", "class") => is_language_class(value),
                ("th" | "td", "align") => matches!(value, "left" | "center" | "right"),
                ("ol", "start") => !value.is_empty() && value.chars().all(|c| c.is_ascii_digit()),
                ("li", "data-sourcepos") => is_sourcepos(value),
                ("input", "type") => value == "checkbox",
                _ => true,
            };
            keep.then_some(Cow::Borrowed(value))
        });
    builder
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mentions_list_long_and_trimmed_logins() {
        assert_eq!(
            mentions("hi @Ann.Lee. and @bob- a@b.c @@x /@y `@z`"),
            ["ann.lee.", "ann.lee", "bob-", "bob", "z"]
        );
    }

    #[test]
    fn issue_tokens_follow_the_pattern() {
        let at = |text: &str| issue_token_at(&text.chars().collect::<Vec<_>>(), 0);
        assert_eq!(at("ABC-12"), Some(6));
        assert_eq!(at("A_1-7."), Some(5));
        assert_eq!(at("A-1"), None);
        assert_eq!(at("ABCDEFGHIJK-1"), None);
        assert_eq!(at("ABC-0"), None);
        assert_eq!(at("ABC-12x"), None);
        assert_eq!(at("Abc-12"), None);
        assert_eq!(at("ABC-1234567890123456789"), None);
    }

    #[test]
    fn file_names_and_decoding() {
        assert!(is_file_name("shot.png"));
        assert!(is_file_name("my%20file.txt"));
        assert!(!is_file_name("https://x"));
        assert!(!is_file_name("/a"));
        assert!(!is_file_name("#a"));
        assert!(!is_file_name("./a"));
        assert!(!is_file_name("mailto:a@b.c"));
        assert_eq!(percent_decode("my%20file%zz.txt"), "my file%zz.txt");
    }
}
