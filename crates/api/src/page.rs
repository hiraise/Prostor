//! Cursor pagination for every list endpoint (architecture.md → API conventions).
//!
//! A list is ordered by a unique key (for example `(updated_at, id)`). The client asks for `limit`
//! items; the response carries `next_cursor`, an opaque token that encodes the key of the last item.
//! The server fetches `limit + 1` rows: the extra row only tells that another page exists.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::error::FieldError;

/// Page size when the client does not ask for one.
pub const DEFAULT_LIMIT: u32 = 50;
/// Largest page size any endpoint allows; an endpoint may set a lower maximum.
pub const MAX_LIMIT: u32 = 200;
/// Longest cursor the server accepts; real cursors are much shorter.
const MAX_CURSOR_LEN: usize = 512;

/// Query parameters of a list request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PageQuery {
    /// Items per page, from 1 to the endpoint's maximum (at most 200); default 50.
    pub limit: Option<u32>,
    /// `next_cursor` from the previous page; omit for the first page.
    pub cursor: Option<String>,
}

/// A validated page request: how many items and after which key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageRequest<K> {
    pub limit: u32,
    pub after: Option<K>,
}

impl<K> PageRequest<K> {
    /// How many rows to fetch: one more than the page size.
    pub fn fetch_limit(&self) -> i64 {
        i64::from(self.limit) + 1
    }
}

impl PageQuery {
    /// Validates the parameters for an endpoint whose page size is at most `max`.
    pub fn resolve<K: DeserializeOwned>(
        &self,
        max: u32,
    ) -> Result<PageRequest<K>, Vec<FieldError>> {
        let max = max.clamp(1, MAX_LIMIT);
        let mut errors = Vec::new();

        let limit = match self.limit {
            None => DEFAULT_LIMIT.min(max),
            Some(limit) if (1..=max).contains(&limit) => limit,
            Some(_) => {
                errors.push(FieldError::new(
                    "limit",
                    "out_of_range",
                    format!("limit must be between 1 and {max}"),
                ));
                0
            }
        };

        let after = match self.cursor.as_deref() {
            None | Some("") => None,
            Some(cursor) => {
                let key = decode_cursor(cursor);
                if key.is_none() {
                    errors.push(FieldError::new(
                        "cursor",
                        "invalid",
                        "cursor is not a value returned by this list",
                    ));
                }
                key
            }
        };

        if errors.is_empty() {
            Ok(PageRequest { limit, after })
        } else {
            Err(errors)
        }
    }
}

/// One page of a list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Page<T> {
    pub items: Vec<T>,
    /// Pass as `cursor` to get the next page; `null` on the last page.
    #[schema(required = true)]
    pub next_cursor: Option<String>,
}

impl<T> Page<T> {
    /// Builds a page from rows fetched with [`PageRequest::fetch_limit`], ordered by the page key.
    /// `key` returns the key of a row; the cursor points after the last row of the page.
    pub fn from_rows<K: Serialize>(mut rows: Vec<T>, limit: u32, key: impl Fn(&T) -> K) -> Self {
        let limit = usize::try_from(limit).unwrap_or(usize::MAX);
        let has_more = rows.len() > limit;
        rows.truncate(limit);
        let next_cursor = if has_more {
            rows.last().map(|last| encode_cursor(&key(last)))
        } else {
            None
        };
        Self {
            items: rows,
            next_cursor,
        }
    }
}

/// Encodes a page key as an opaque, URL-safe token.
pub fn encode_cursor<K: Serialize>(key: &K) -> String {
    // A key is built from plain values (numbers, strings, timestamps), which always serialize.
    let json = serde_json::to_vec(key).unwrap_or_default();
    URL_SAFE_NO_PAD.encode(json)
}

/// Decodes a token made by [`encode_cursor`]; `None` for anything else.
pub fn decode_cursor<K: DeserializeOwned>(cursor: &str) -> Option<K> {
    if cursor.len() > MAX_CURSOR_LEN {
        return None;
    }
    let json = URL_SAFE_NO_PAD.decode(cursor).ok()?;
    serde_json::from_slice(&json).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(limit: Option<u32>, cursor: Option<&str>) -> PageQuery {
        PageQuery {
            limit,
            cursor: cursor.map(str::to_owned),
        }
    }

    #[test]
    fn defaults_to_the_first_page() {
        let page: PageRequest<i64> = query(None, None).resolve(MAX_LIMIT).unwrap();
        assert_eq!(page.limit, DEFAULT_LIMIT);
        assert_eq!(page.after, None);
        assert_eq!(page.fetch_limit(), i64::from(DEFAULT_LIMIT) + 1);
    }

    #[test]
    fn default_limit_respects_a_lower_endpoint_maximum() {
        let page: PageRequest<i64> = query(None, None).resolve(20).unwrap();
        assert_eq!(page.limit, 20);
    }

    #[test]
    fn rejects_limits_out_of_range() {
        for limit in [0, MAX_LIMIT + 1] {
            let errors = query(Some(limit), None)
                .resolve::<i64>(MAX_LIMIT)
                .unwrap_err();
            assert_eq!(errors.len(), 1);
            assert_eq!(errors[0].path, "limit");
            assert_eq!(errors[0].code, "out_of_range");
        }
        assert!(query(Some(30), None).resolve::<i64>(20).is_err());
    }

    #[test]
    fn cursor_round_trips_a_compound_key() {
        let key = ("2026-10-21T10:00:00Z".to_owned(), 42_i64);
        let cursor = encode_cursor(&key);
        assert!(
            cursor
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );
        let page: PageRequest<(String, i64)> =
            query(Some(10), Some(&cursor)).resolve(MAX_LIMIT).unwrap();
        assert_eq!(page.after, Some(key));
    }

    #[test]
    fn rejects_foreign_or_damaged_cursors() {
        let wrong_shape = encode_cursor(&"text");
        let too_long = "A".repeat(600);
        for cursor in [
            "%%%",
            "bm90IGpzb24",
            wrong_shape.as_str(),
            too_long.as_str(),
        ] {
            let errors = query(None, Some(cursor))
                .resolve::<i64>(MAX_LIMIT)
                .unwrap_err();
            assert_eq!(errors[0].path, "cursor", "{cursor}");
        }
    }

    #[test]
    fn reports_every_bad_parameter_at_once() {
        let errors = query(Some(0), Some("%%%"))
            .resolve::<i64>(MAX_LIMIT)
            .unwrap_err();
        assert_eq!(errors.len(), 2);
    }

    #[test]
    fn page_from_rows_sets_the_cursor_only_when_more_rows_exist() {
        let full = Page::from_rows(vec![10, 9, 8], 2, |row| *row);
        assert_eq!(full.items, vec![10, 9]);
        assert_eq!(
            decode_cursor::<i32>(full.next_cursor.as_deref().unwrap()),
            Some(9)
        );

        let last = Page::from_rows(vec![10, 9], 2, |row| *row);
        assert_eq!(last.items, vec![10, 9]);
        assert_eq!(last.next_cursor, None);

        let empty = Page::<i32>::from_rows(vec![], 2, |row| *row);
        assert!(empty.items.is_empty());
        assert_eq!(empty.next_cursor, None);
    }
}
