//! Fields of `PATCH` bodies that may be cleared (architecture.md §10): absent keeps the value,
//! `null` clears it, a value sets it. Use with
//! `#[serde(default, deserialize_with = "crate::patch::nullable")]` on an `Option<Option<T>>`.

use serde::{Deserialize, Deserializer};

/// Reads a present field: `null` becomes `Some(None)`, a value `Some(Some(value))`. An absent field
/// never reaches this function and stays `None` through `#[serde(default)]`.
pub fn nullable<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Body {
        #[serde(default, deserialize_with = "super::nullable")]
        value: Option<Option<i64>>,
    }

    #[test]
    fn absent_null_and_value_differ() {
        let parse = |text: &str| serde_json::from_str::<Body>(text).map(|b| b.value).ok();
        assert_eq!(parse("{}"), Some(None));
        assert_eq!(parse(r#"{"value": null}"#), Some(Some(None)));
        assert_eq!(parse(r#"{"value": 4}"#), Some(Some(Some(4))));
    }
}
