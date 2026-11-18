//! The query language in the browser (D-038): the syntax half of highlighting and checking, with
//! no names resolved (those need the server's `GET /queries/check`). No bindings generator: three
//! exports over linear memory.
//!
//! - `alloc(len)` reserves `len` bytes and returns their address; the caller writes UTF-8 there.
//! - `analyze(ptr, len)` reads the query and returns the address (high 32 bits) and the length
//!   (low 32 bits) of a UTF-8 JSON answer: `{ tokens: [{from, to, class}], problems: [{from, to,
//!   code, severity}], canonical, sort: {from, to} | null }`, offsets in UTF-16 code units.
//! - `free(ptr, len)` returns both the query's and the answer's bytes.
//!
//! The names are exported only on wasm32: on a host they would replace the C library's `free`.

use std::alloc::{Layout, alloc as reserve, dealloc};

use serde::Serialize;
use taskmaster_query::{Class, ProblemCode, Severity, Span, classes, parse, print, utf16};

#[derive(Debug, Serialize, PartialEq)]
pub struct Answer {
    pub tokens: Vec<Token>,
    pub problems: Vec<Found>,
    pub canonical: String,
    /// Where the sort clause is (`sort by: …`), so a list can replace it; `None` without one.
    pub sort: Option<Place>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Place {
    pub from: usize,
    pub to: usize,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Token {
    pub from: usize,
    pub to: usize,
    pub class: Class,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Found {
    pub from: usize,
    pub to: usize,
    pub code: ProblemCode,
    pub severity: Severity,
}

/// What `analyze` answers for `text`.
pub fn answer(text: &str) -> Answer {
    let query = parse(text);
    let found = classes(&query, text);
    let spans: Vec<Span> = found.iter().map(|(s, _)| *s).collect();
    let tokens = utf16::offsets(text, &spans)
        .into_iter()
        .zip(&found)
        .map(|(s, (_, class))| Token {
            from: s.start,
            to: s.end,
            class: *class,
        })
        .collect();
    let spans: Vec<Span> = query.problems.iter().map(|p| p.span).collect();
    let problems = utf16::offsets(text, &spans)
        .into_iter()
        .zip(&query.problems)
        .map(|(s, p)| Found {
            from: s.start,
            to: s.end,
            code: p.code,
            severity: p.severity,
        })
        .collect();
    let sort = query.sort.as_ref().and_then(|sort| {
        utf16::offsets(text, &[sort.span]).first().map(|s| Place {
            from: s.start,
            to: s.end,
        })
    });
    Answer {
        tokens,
        problems,
        canonical: print(&query),
        sort,
    }
}

fn layout(len: usize) -> Option<Layout> {
    Layout::array::<u8>(len).ok().filter(|l| l.size() > 0)
}

/// Reserves `len` bytes; null for zero bytes or when memory runs out.
#[allow(unsafe_code)]
#[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    match layout(len) {
        // SAFETY: the layout has a non-zero size.
        Some(l) => unsafe { reserve(l) },
        None => std::ptr::null_mut(),
    }
}

/// Returns bytes from `alloc` or from an answer of `analyze`.
///
/// # Safety
/// `ptr` and `len` are exactly what one of them gave, freed once.
#[allow(unsafe_code)]
#[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
pub unsafe extern "C" fn free(ptr: *mut u8, len: usize) {
    if let Some(l) = layout(len)
        && !ptr.is_null()
    {
        // SAFETY: by the contract, `ptr` was allocated with this layout.
        unsafe { dealloc(ptr, l) }
    }
}

/// Analyzes the UTF-8 query at `ptr` (invalid bytes read as U+FFFD).
///
/// # Safety
/// `ptr` points at `len` readable bytes (any pointer when `len` is zero).
#[allow(unsafe_code)]
#[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
pub unsafe extern "C" fn analyze(ptr: *const u8, len: usize) -> u64 {
    let bytes = if len == 0 || ptr.is_null() {
        &[][..]
    } else {
        // SAFETY: by the contract, `len` bytes at `ptr` are readable.
        unsafe { std::slice::from_raw_parts(ptr, len) }
    };
    let text = String::from_utf8_lossy(bytes);
    // Plain values always serialize.
    let json = serde_json::to_vec(&answer(&text)).unwrap_or_default();
    let out = alloc(json.len());
    if out.is_null() {
        return 0;
    }
    // SAFETY: `out` holds `json.len()` fresh bytes that do not overlap `json`.
    unsafe { std::ptr::copy_nonoverlapping(json.as_ptr(), out, json.len()) };
    // Addresses and lengths fit 32 bits on wasm32.
    ((out as usize as u64) << 32) | (json.len() as u64)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, unsafe_code)]
    use super::*;

    #[test]
    fn offsets_are_utf16_and_the_text_is_canonical() {
        let a = answer("тег:  🎉x");
        assert_eq!(a.canonical, "тег: 🎉x");
        assert_eq!(
            a.tokens.first(),
            Some(&Token {
                from: 0,
                to: 3,
                class: Class::Attribute
            })
        );
        assert_eq!(a.tokens.last().map(|t| (t.from, t.to)), Some((6, 9)));
        let json = serde_json::to_value(answer("(a")).unwrap();
        assert_eq!(json["problems"][0]["code"], "unclosed_group");
        assert_eq!(json["problems"][0]["severity"], "error");
    }

    #[test]
    fn qry_075_the_sort_clause_is_placed() {
        let text = "тег: 🎉 sort: id desc";
        let a = answer(text);
        let place = a.sort.unwrap();
        let units: Vec<u16> = text.encode_utf16().collect();
        let clause = String::from_utf16(&units[place.from..place.to]).unwrap();
        assert_eq!(clause, "sort: id desc");
        assert_eq!(answer("is: open").sort, None);
        assert_eq!(
            serde_json::to_value(answer("")).unwrap()["sort"],
            serde_json::Value::Null
        );
    }

    #[test]
    fn round_trip_through_memory() {
        let text = "is: open";
        let ptr = alloc(text.len());
        unsafe {
            std::ptr::copy_nonoverlapping(text.as_ptr(), ptr, text.len());
            let packed = analyze(ptr, text.len());
            free(ptr, text.len());
            // On a 64-bit host the address does not fit 32 bits: only the length is checked.
            let len = (packed & 0xffff_ffff) as usize;
            let json = serde_json::to_vec(&answer(text)).unwrap();
            assert_eq!(len, json.len());
        }
    }
}
