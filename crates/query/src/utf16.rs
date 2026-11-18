//! Byte spans to UTF-16 code-unit offsets, the indices JavaScript strings use. Everything that
//! leaves Rust (the API, the WASM module) reports these.

use crate::syntax::Span;

/// Converts byte spans of `text`. An offset inside a character maps to that character's start;
/// one past the end maps to the length.
pub fn offsets(text: &str, spans: &[Span]) -> Vec<Span> {
    let mut table = vec![0usize; text.len() + 1];
    let mut units = 0;
    for (i, c) in text.char_indices() {
        for slot in table.iter_mut().skip(i).take(c.len_utf8()) {
            *slot = units;
        }
        units += c.len_utf16();
    }
    if let Some(last) = table.last_mut() {
        *last = units;
    }
    let at = |i: usize| table.get(i).copied().unwrap_or(units);
    spans
        .iter()
        .map(|s| Span::new(at(s.start), at(s.end)))
        .collect()
}

/// Converts a UTF-16 code-unit offset back to a byte offset, snapping to the character that
/// starts at or contains it. The assist's caret arrives this way.
pub fn from_units(text: &str, units: usize) -> usize {
    let mut seen = 0usize;
    for (at, c) in text.char_indices() {
        if seen >= units {
            return at;
        }
        seen += c.len_utf16();
    }
    text.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cyrillic_and_emoji() {
        // "я" is 2 bytes and 1 unit; "🎉" is 4 bytes and 2 units.
        let text = "я🎉x";
        let spans = [
            Span::new(0, 2),
            Span::new(2, 6),
            Span::new(6, 7),
            Span::new(0, 7),
        ];
        assert_eq!(
            offsets(text, &spans),
            [
                Span::new(0, 1),
                Span::new(1, 3),
                Span::new(3, 4),
                Span::new(0, 4)
            ]
        );
        assert_eq!(offsets("", &[Span::new(0, 0)]), [Span::new(0, 0)]);
    }
}
